import type { FastifyInstance } from "fastify";
import {
  authenticateAgentRequest,
  cutAgentText,
  nextProcessStarts,
  raiseCopySignal,
  serialOf,
} from "../agent-request.js";
import { type CgesEvent, OuterEnvelopeSchema } from "../schemas.js";
import type { Services } from "../services.js";

/** RFC3339 (`...T...Z`) → ClickHouse DateTime64 input (`YYYY-MM-DD HH:MM:SS.fff`). */
function toChDateTime(iso: string): string {
  return new Date(iso).toISOString().replace("T", " ").replace("Z", "");
}

/**
 * Convert string-encoded Unix nanoseconds to ClickHouse DateTime64(9, 'UTC')
 * literal format. Uses BigInt to preserve full u64 precision (nanosecond
 * timestamps for 2024+ exceed Number.MAX_SAFE_INTEGER = 2^53).
 *
 * Input: decimal string of nanoseconds (e.g. "1779997630000000100").
 * Output: "YYYY-MM-DD HH:MM:SS.nnnnnnnnn" per the existing `toChDateTime`
 * convention.
 */
function stringNanosToChDateTime(nanos: string): string {
  const bn = BigInt(nanos);
  const millis = Number(bn / 1_000_000n);
  const fractionalNanos = Number(bn % 1_000_000_000n);
  const isoMs = new Date(millis).toISOString();
  // isoMs format: "YYYY-MM-DDTHH:MM:SS.mmmZ"
  // ClickHouse format: "YYYY-MM-DD HH:MM:SS.nnnnnnnnn"
  const datePart = isoMs.slice(0, 10);
  const timePart = isoMs.slice(11, 19);
  const nineDigitNanos = String(fractionalNanos).padStart(9, "0");
  return `${datePart} ${timePart}.${nineDigitNanos}`;
}

/**
 * One `cges_events` row for one events[] element, by its class (SPEC-019 §Data
 * contracts). A Process Activity row is written as before; its six network columns
 * keep their defaults. A Network Activity row sets the columns every class shares,
 * the process it belongs to (`process_uid` '' when the element has no uid, and
 * `process_name` '') and the six network columns; the other process columns keep
 * their defaults. Every column without a default is sent (§Operational §6).
 */
function cgesEventRow(
  agentId: string,
  orgId: string,
  cgesEvent: CgesEvent,
): Record<string, unknown> {
  if (cgesEvent.class_uid === 3002) {
    // SPEC-020 §Data contracts: the process columns without a default are set
    // empty; the source's address goes to src_ip, beside SPEC-019's.
    return {
      agent_id: agentId,
      org_id: orgId,
      event_id: cgesEvent.event_id,
      class_uid: cgesEvent.class_uid,
      activity_id: cgesEvent.activity_id,
      process_pid: 0,
      process_uid: "",
      process_name: "",
      src_ip: cgesEvent.src_endpoint?.ip ?? "",
      user_uid: cgesEvent.user.uid,
      user_name: cgesEvent.user.name,
      user_domain: cgesEvent.user.domain,
      logon_type_id: cgesEvent.logon_type_id,
      status_id: cgesEvent.status_id,
      status_code: cgesEvent.status_id === 2 ? cgesEvent.status_code : "",
      status_detail: cgesEvent.status_id === 2 ? cgesEvent.status_detail : "",
      auth_protocol: cgesEvent.auth_protocol,
      auth_protocol_id: cgesEvent.auth_protocol_id,
      src_hostname: cgesEvent.src_endpoint?.hostname ?? "",
      elevated_token: cgesEvent.status_id === 1 ? (cgesEvent.cg_elevated_token ?? null) : null,
      time: stringNanosToChDateTime(cgesEvent.time),
    };
  }
  if (cgesEvent.class_uid === 4001) {
    return {
      agent_id: agentId,
      org_id: orgId,
      event_id: cgesEvent.event_id,
      class_uid: cgesEvent.class_uid,
      activity_id: cgesEvent.activity_id,
      process_pid: cgesEvent.actor.process.pid,
      process_uid: cgesEvent.actor.process.uid ?? "",
      process_name: "",
      src_ip: cgesEvent.src_endpoint.ip,
      src_port: cgesEvent.src_endpoint.port,
      dst_ip: cgesEvent.dst_endpoint.ip,
      dst_port: cgesEvent.dst_endpoint.port,
      net_protocol: cgesEvent.connection_info.protocol_name,
      net_direction: cgesEvent.connection_info.direction,
      time: stringNanosToChDateTime(cgesEvent.time),
    };
  }
  return {
    agent_id: agentId,
    org_id: orgId,
    event_id: cgesEvent.event_id,
    class_uid: cgesEvent.class_uid,
    activity_id: cgesEvent.activity_id,
    process_pid: cgesEvent.process.pid,
    process_uid: cgesEvent.process.uid,
    process_name: cgesEvent.process.name,
    process_created_time:
      cgesEvent.process.created_time === null
        ? null
        : stringNanosToChDateTime(cgesEvent.process.created_time),
    process_exit_code: cgesEvent.process.exit_code ?? null,
    process_parent_pid: cgesEvent.process.parent_pid,
    process_command_line: cgesEvent.process.command_line,
    subject_user_sid: cgesEvent.process.subject_user_sid,
    image_file_name: cgesEvent.process.image_file_name,
    time: stringNanosToChDateTime(cgesEvent.time),
  };
}

/**
 * The fields of the insert-failure log line (SPEC-020 §Operational §7): the error's
 * name, code and type, never its message, which ClickHouse fills with the row it
 * could not parse — a logon's user name, a process's command line.
 */
export function insertFailureLogFields(e: unknown): {
  err_name: string;
  err_code: string | null;
  err_type: string | null;
} {
  const err = (e ?? {}) as { name?: unknown; code?: unknown; type?: unknown };
  const code = err.code;
  return {
    err_name: typeof err.name === "string" ? err.name : "unknown",
    err_code: typeof code === "string" ? code : typeof code === "number" ? String(code) : null,
    err_type: typeof err.type === "string" ? err.type : null,
  };
}

function isConnectivityError(e: unknown): boolean {
  const code = (e as { code?: string }).code;
  return (
    code === "ECONNREFUSED" ||
    code === "ETIMEDOUT" ||
    code === "ENOTFOUND" ||
    /terminat|connection|ECONN|timed out/i.test(String((e as { message?: string }).message ?? ""))
  );
}

/**
 * `POST /v1/agents/heartbeat` (mTLS). SPEC-004 FR-009–FR-012 with the FR-010
 * validation order and the §Behavior cross-store partial-failure forks, as SPEC-025
 * §Operational §2 and §3 amend them: the agent's row read once, its revocation and the
 * certificate it presents checked before the nonce is spent, every row it writes under the
 * row's org, and its first accepted request under its pending certificate making that
 * certificate the agent's. The mTLS handshake (FR-008) is enforced by Node TLS before this
 * handler runs.
 */
export function registerHeartbeatRoutes(app: FastifyInstance, services: Services): void {
  const { pool, ch } = services;

  app.post("/v1/agents/heartbeat", async (req, reply) => {
    // FR-009: structural validation.
    const parsed = OuterEnvelopeSchema.safeParse(req.body);
    if (!parsed.success) {
      req.log.warn({ reason: "invalid_request", response_status: 400 }, "heartbeat rejected");
      return reply.code(400).send({ error: "invalid_request" });
    }
    const env = parsed.data;

    // SPEC-025 §Operational §2: the CN, the row, the revocation and the certificate, then
    // FR-010's timestamp, nonce and signature.
    const checked = await authenticateAgentRequest({
      req,
      reply,
      services,
      agentId: env.agent_id,
      sentAt: env.sent_at,
      nonce: env.nonce,
      signature: env.signature,
      signed: req.body as Record<string, unknown>,
      rejected: "heartbeat rejected",
    });
    if (checked === null) return reply;
    const { row, presented } = checked;

    // FR-010 step 6 / §Behavior cross-store order: ClickHouse insert
    // (synchronous, SHOULD-2) → Postgres.
    try {
      await ch.insert({
        table: "heartbeats",
        format: "JSONEachRow",
        values: [
          {
            agent_id: env.agent_id,
            sequence_number: env.body.sequence_number,
            inner_sent_at: toChDateTime(env.body.sent_at),
            outer_sent_at: toChDateTime(env.sent_at),
            status: env.body.status,
            uptime_seconds: env.body.uptime_seconds,
            nonce: env.nonce,
          },
        ],
      });
    } catch (err) {
      // Nonce already burned in Redis; the heartbeat is lost (accepted data
      // loss). The agent retries next interval with a fresh nonce.
      req.log.error({ backend: "clickhouse", error: String(err) }, "backend unavailable");
      return reply.code(503).send({ error: "unavailable" });
    }

    // SPEC-005 events[] persistence path. Inserts to cges_events per the
    // FR-010 step 6 cross-store ordering: nonce SETNX → heartbeats insert
    // → cges_events insert → the agent's row UPDATE. Each row carries the
    // agent's org, read from its row (SPEC-025 §Operational §2).
    //
    // Empty events[] (SPEC-001/002/003 backward-compat envelopes) short-
    // circuits; no ClickHouse round-trip. Otherwise the insert is wrapped
    // in try-catch; isConnectivityError(e) true → 503 with Retry-After: 5;
    // false → 500 with the error logged. Failure here returns BEFORE the
    // agent's row is updated, so a pending certificate is not promoted; the
    // nonce in Redis is already burned (accepted data loss on retry per the
    // existing cross-store policy).
    if (env.body.events.length > 0) {
      try {
        await ch.insert({
          table: "cges_events",
          format: "JSONEachRow",
          values: env.body.events.map((cgesEvent) =>
            cgesEventRow(env.agent_id, row.org_id, cgesEvent),
          ),
        });
      } catch (e) {
        if (isConnectivityError(e)) {
          reply.header("Retry-After", "5");
          return reply.code(503).send({
            error: "cges_events persistence unavailable; retry",
          });
        }
        req.log.error(insertFailureLogFields(e), "cges_events insert failed");
        return reply.code(500).send({
          error: "cges_events insert failed",
        });
      }
    }

    // The agent's row, in one statement (SPEC-025 §Operational §2): last_seen, what the agent
    // block carries, and the process columns of §6; under the pending certificate, also its
    // promotion, by a compare-and-set on the agent's certificate and the pending one (§3).
    const agent = env.body.agent;
    const startMs = new Date(env.body.sent_at).getTime() - env.body.uptime_seconds * 1000;
    const starts = Number.isFinite(startMs)
      ? nextProcessStarts(row, startMs)
      : {
          processStartedAt: row.process_started_at,
          previousProcessStartedAt: row.previous_process_started_at,
          interleaved: false,
        };
    const promote = presented.kind === "pending";
    let recorded = false;
    try {
      const params: unknown[] = [
        env.agent_id,
        cutAgentText(agent.agent_hostname),
        cutAgentText(agent.agent_platform),
        cutAgentText(agent.agent_version),
        starts.processStartedAt,
        starts.previousProcessStartedAt,
      ];
      let text = `UPDATE agents
            SET last_seen = now(), hostname = $2, platform = $3, agent_version = $4,
                process_started_at = $5, previous_process_started_at = $6`;
      if (promote) {
        params.push(
          Buffer.from(presented.key),
          presented.certPem,
          presented.notAfter,
          row.cert_pem,
        );
        text += `, pubkey = $7, cert_pem = $8, expires_at = $9, pending_cert_pem = NULL
          WHERE agent_id = $1 AND cert_pem = $10 AND pending_cert_pem = $8`;
      } else {
        text += " WHERE agent_id = $1";
      }
      const updated = await pool.query(text, params);
      recorded = updated.rowCount === 1;
    } catch (err) {
      req.log.warn({ agent_id: env.agent_id, error: String(err) }, "agent row update failed");
    }

    if (promote) {
      // A 2xx under the pending certificate always means it is the agent's (§3): a promotion
      // the database did not record answers 503, and the next request is compared afresh.
      if (!recorded) {
        req.log.warn(
          { agent_id: env.agent_id, reason: "promotion_not_recorded", response_status: 503 },
          "heartbeat rejected",
        );
        return reply.code(503).send({ error: "unavailable" });
      }
      req.log.info(
        {
          agent_id: env.agent_id,
          old_serial: serialOf(row.cert_pem),
          new_serial: serialOf(presented.certPem),
        },
        "agent certificate promoted",
      );
    }
    // Otherwise the row is a denormalized cache; ClickHouse is the source of truth, so a
    // failure here still returns 200 (SPEC-004 §Behavior cross-store, third fork).
    if (recorded && starts.interleaved && row.duplicate_signal === null) {
      await raiseCopySignal(pool, req.log, env.agent_id, "interleaved_process");
    }

    req.log.info(
      { agent_id: env.agent_id, sequence_number: env.body.sequence_number, response_status: 200 },
      "heartbeat accepted",
    );
    return reply.code(200).send({ status: "ok" });
  });
}
