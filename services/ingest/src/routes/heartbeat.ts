import type { TLSSocket } from "node:tls";
import type { FastifyInstance } from "fastify";
import { verifyEnvelopeSignature } from "../jcs.js";
import { OuterEnvelopeSchema } from "../schemas.js";
import type { Services } from "../services.js";

/** ±5 min skew window (SPEC-004 FR-010 step 3). */
const SKEW_MS = 5 * 60 * 1000;
/** Nonce TTL: 10 min, comfortably above the skew window (FR-011). */
const NONCE_TTL_SECONDS = 600;

/** RFC3339 (`...T...Z`) → ClickHouse DateTime64 input (`YYYY-MM-DD HH:MM:SS.fff`). */
function toChDateTime(iso: string): string {
  return new Date(iso).toISOString().replace("T", " ").replace("Z", "");
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
 * validation order and the §Behavior cross-store partial-failure forks. The
 * mTLS handshake (FR-008) is enforced by Node TLS before this handler runs.
 */
export function registerHeartbeatRoutes(app: FastifyInstance, services: Services): void {
  const { pool, redis, ch } = services;

  app.post("/v1/agents/heartbeat", async (req, reply) => {
    // FR-009: structural validation.
    const parsed = OuterEnvelopeSchema.safeParse(req.body);
    if (!parsed.success) {
      req.log.warn({ reason: "invalid_request", response_status: 400 }, "heartbeat rejected");
      return reply.code(400).send({ error: "invalid_request" });
    }
    const env = parsed.data;
    const raw = req.body as Record<string, unknown>;

    // FR-010 step 2: envelope agent_id must equal the client-cert CN and be a
    // known agent. The mTLS layer already proved the cert chains to the CA.
    const peerCn = peerCertificateCn(req.raw.socket as TLSSocket);
    if (!peerCn || peerCn !== env.agent_id) {
      req.log.warn(
        { agent_id: env.agent_id, reason: "unknown_agent", response_status: 401 },
        "heartbeat rejected",
      );
      return reply.code(401).send({ error: "unknown_agent" });
    }

    let agentPubkey: Buffer;
    try {
      const row = await pool.query<{ pubkey: Buffer }>(
        "SELECT pubkey FROM agents WHERE agent_id = $1",
        [env.agent_id],
      );
      const found = row.rows[0];
      if (!found) {
        req.log.warn(
          { agent_id: env.agent_id, reason: "unknown_agent", response_status: 401 },
          "heartbeat rejected",
        );
        return reply.code(401).send({ error: "unknown_agent" });
      }
      agentPubkey = found.pubkey;
    } catch (err) {
      req.log.error({ backend: "postgres", error: String(err) }, "backend unavailable");
      return reply.code(503).send({ error: "unavailable" });
    }

    // FR-010 step 3: timestamp window.
    const sentAtMs = new Date(env.sent_at).getTime();
    if (Number.isNaN(sentAtMs) || Math.abs(Date.now() - sentAtMs) > SKEW_MS) {
      req.log.warn(
        { agent_id: env.agent_id, reason: "stale_timestamp", response_status: 422 },
        "heartbeat rejected",
      );
      return reply.code(422).send({ error: "stale_timestamp" });
    }

    // FR-010 step 4 / FR-011: atomic anti-replay. SET NX both checks and
    // persists the nonce — the first cross-store write (§Behavior order).
    let nonceWasNew: string | null;
    try {
      nonceWasNew = await redis.set(`nonce:${env.nonce}`, "1", "EX", NONCE_TTL_SECONDS, "NX");
    } catch (err) {
      req.log.error({ backend: "redis", error: String(err) }, "backend unavailable");
      return reply.code(503).send({ error: "unavailable" });
    }
    if (nonceWasNew === null) {
      req.log.warn(
        { agent_id: env.agent_id, reason: "replay", response_status: 409 },
        "heartbeat rejected",
      );
      return reply.code(409).send({ error: "replay" });
    }

    // FR-010 step 5: signature over JCS(outer minus signature). Verified
    // against the raw received body so the canonical bytes match the agent's.
    const sigOk = await verifyEnvelopeSignature(raw, env.signature, new Uint8Array(agentPubkey));
    if (!sigOk) {
      req.log.warn(
        { agent_id: env.agent_id, reason: "bad_signature", response_status: 401 },
        "heartbeat rejected",
      );
      return reply.code(401).send({ error: "bad_signature" });
    }

    // FR-010 step 6 / §Behavior cross-store order: ClickHouse insert
    // (synchronous, SHOULD-2) → Postgres last_seen.
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

    // last_seen is a denormalized cache; ClickHouse is the source of truth, so
    // a failure here still returns 200 (§Behavior cross-store, third fork).
    try {
      await pool.query("UPDATE agents SET last_seen = now() WHERE agent_id = $1", [env.agent_id]);
    } catch (err) {
      req.log.warn({ agent_id: env.agent_id, error: String(err) }, "last_seen update failed");
    }

    req.log.info(
      { agent_id: env.agent_id, sequence_number: env.body.sequence_number, response_status: 200 },
      "heartbeat accepted",
    );
    return reply.code(200).send({ status: "ok" });
  });
}

/** Extract the CN of the validated client certificate (mTLS already passed). */
function peerCertificateCn(socket: TLSSocket): string | null {
  if (typeof socket.getPeerCertificate !== "function") {
    return null;
  }
  const cert = socket.getPeerCertificate();
  const cn = cert?.subject?.CN;
  return typeof cn === "string" && cn.length > 0 ? cn : null;
}
