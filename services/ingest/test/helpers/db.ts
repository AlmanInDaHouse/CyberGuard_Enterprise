import { createClient } from "@clickhouse/client";
import pg from "pg";
import type { Config } from "../../src/config.js";
import type { DetectCursor } from "../../src/detect/types.js";

/**
 * Insert an enrollment_tokens row directly (the server's CLI does this in
 * production; tests insert directly to control expiry). Returns the opaque
 * token. `expiresInMs` negative ⇒ an already-expired token (AC-007).
 */
export async function issueToken(
  config: Config,
  opts: { expiresInMs?: number } = {},
): Promise<string> {
  const token = Buffer.from(globalThis.crypto.getRandomValues(new Uint8Array(32))).toString(
    "base64url",
  );
  const pool = new pg.Pool({ connectionString: config.INGEST_PG_URL });
  try {
    const expiresAt = new Date(Date.now() + (opts.expiresInMs ?? 15 * 60 * 1000));
    await pool.query(
      "INSERT INTO enrollment_tokens (token, org_id, scope, state, expires_at) VALUES ($1, 'default', 'enroll', 'issued', $2)",
      [token, expiresAt],
    );
  } finally {
    await pool.end();
  }
  return token;
}

export interface AgentRow {
  agent_id: string;
  pubkey: Buffer;
  enrolled_at: Date;
  last_seen: Date | null;
}

export async function getAgent(config: Config, agentId: string): Promise<AgentRow | null> {
  const pool = new pg.Pool({ connectionString: config.INGEST_PG_URL });
  try {
    const r = await pool.query<AgentRow>(
      "SELECT agent_id, pubkey, enrolled_at, last_seen FROM agents WHERE agent_id = $1",
      [agentId],
    );
    return r.rows[0] ?? null;
  } finally {
    await pool.end();
  }
}

export interface HeartbeatRow {
  sequence_number: string;
  status: string;
  arrived_at: string;
}

export async function getHeartbeats(config: Config, agentId: string): Promise<HeartbeatRow[]> {
  const ch = createClient({
    url: config.INGEST_CH_URL,
    username: config.INGEST_CH_USER,
    password: config.INGEST_CH_PASSWORD,
    database: config.INGEST_CH_DB,
  });
  try {
    const rs = await ch.query({
      query:
        "SELECT toString(sequence_number) AS sequence_number, status, toString(arrived_at) AS arrived_at FROM heartbeats WHERE agent_id = toUUID({id:String}) ORDER BY arrived_at",
      query_params: { id: agentId },
      format: "JSONEachRow",
    });
    return (await rs.json()) as HeartbeatRow[];
  } finally {
    await ch.close();
  }
}

// SPEC-005 additions — getCgesEvents helper + CgesEventRow interface.
// Queries the cges_events ClickHouse table for the Process Activity rows
// belonging to a given agent_id, ordered by capture timestamp ascending, with
// FINAL: the agent delivers at least once, and FINAL collapses a resent event
// (SPEC-017 §Data contracts) as the detection read-model does. It selects
// class_uid = 1007: the table also holds Network Activity rows, whose
// activity_id means something else (SPEC-019 §Operational §8, ADR-0018 §9).

export interface CgesEventRow {
  agent_id: string;
  class_uid: number;
  activity_id: number;
  process_pid: number;
  process_uid: string;
  process_name: string;
  process_created_time: string | null;
  process_exit_code: number | null;
  image_file_name: string;
  time: string;
}

export async function getCgesEvents(config: Config, agentId: string): Promise<CgesEventRow[]> {
  const ch = createClient({
    url: config.INGEST_CH_URL,
    username: config.INGEST_CH_USER,
    password: config.INGEST_CH_PASSWORD,
    database: config.INGEST_CH_DB,
  });
  try {
    const result = await ch.query({
      query: `
        SELECT
          agent_id,
          class_uid,
          activity_id,
          process_pid,
          process_uid,
          process_name,
          process_created_time,
          process_exit_code,
          image_file_name,
          time
        FROM cges_events FINAL
        WHERE agent_id = {agent_id:String} AND class_uid = 1007
        ORDER BY time ASC
      `,
      query_params: { agent_id: agentId },
      format: "JSONEachRow",
    });
    return await result.json<CgesEventRow>();
  } finally {
    await ch.close();
  }
}

// SPEC-019 additions — Network Activity (class 4001) rows: read an agent's, and
// insert synthetic ones in the shape the ingest route writes (§Data contracts).

export interface NetworkEventRow {
  event_id: string;
  agent_id: string;
  activity_id: number;
  process_pid: number;
  process_uid: string;
  src_ip: string;
  src_port: number;
  dst_ip: string;
  dst_port: number;
  net_protocol: string;
  net_direction: string;
  time: string;
}

/** An agent's Network Activity rows, with FINAL, by capture time ascending. */
export async function getNetworkEvents(
  config: Config,
  agentId: string,
): Promise<NetworkEventRow[]> {
  const ch = createClient({
    url: config.INGEST_CH_URL,
    username: config.INGEST_CH_USER,
    password: config.INGEST_CH_PASSWORD,
    database: config.INGEST_CH_DB,
  });
  try {
    const rs = await ch.query({
      query: `
        SELECT toString(event_id) AS event_id, toString(agent_id) AS agent_id, activity_id,
               process_pid, process_uid, src_ip, src_port, dst_ip, dst_port, net_protocol,
               net_direction, toString(time) AS time
        FROM cges_events FINAL
        WHERE agent_id = {agent_id:String} AND class_uid = 4001
        ORDER BY time ASC
      `,
      query_params: { agent_id: agentId },
      format: "JSONEachRow",
    });
    return await rs.json<NetworkEventRow>();
  } finally {
    await ch.close();
  }
}

export interface InsertNetworkEventRow {
  agentId: string;
  eventId: string;
  processPid: number;
  processUid?: string;
  srcIp: string;
  srcPort: number;
  dstIp: string;
  dstPort: number;
  direction: "outbound" | "inbound";
  /** ClickHouse DateTime64(9) literal, e.g. "2026-05-31 10:00:00.000000000". */
  time: string;
  orgId?: string;
}

/** Insert Network Activity rows in ONE ClickHouse INSERT, as the route writes them. */
export async function insertNetworkEvents(
  config: Config,
  rows: InsertNetworkEventRow[],
): Promise<void> {
  const ch = createClient({
    url: config.INGEST_CH_URL,
    username: config.INGEST_CH_USER,
    password: config.INGEST_CH_PASSWORD,
    database: config.INGEST_CH_DB,
  });
  try {
    await ch.insert({
      table: "cges_events",
      values: rows.map((ev) => ({
        agent_id: ev.agentId,
        org_id: ev.orgId ?? "default",
        event_id: ev.eventId,
        class_uid: 4001,
        activity_id: 1,
        process_pid: ev.processPid,
        process_uid: ev.processUid ?? "",
        process_name: "",
        src_ip: ev.srcIp,
        src_port: ev.srcPort,
        dst_ip: ev.dstIp,
        dst_port: ev.dstPort,
        net_protocol: "tcp",
        net_direction: ev.direction,
        time: ev.time,
      })),
      format: "JSONEachRow",
    });
  } finally {
    await ch.close();
  }
}

// SPEC-006 additions — synthetic cges_events insert + alerts/watermark helpers
// for the Detection MVP harness. cges_events exists (migrations run in
// startBackends), so insertCgesEvent succeeds — it is "setup-that-exists". The
// alerts and detect_watermark tables do NOT exist yet (migration 0002_alerts
// lands in the Phase-5 impl); getAlerts/setAlertStatus/getWatermark (getCursor
// since SPEC-018) compile cleanly and run once that migration lands — same
// pattern as getCgesEvents.
// In the harness-first RED, every detect_ac_* test calls runDetectionCycle()
// (or scoreAlert) — which throws NotImplemented — BEFORE reaching these alert
// helpers, so the RED is NotImplemented, never "alerts table missing".

export interface InsertCgesEventRow {
  agentId: string;
  eventId: string;
  activityId: number;
  processPid: number;
  processName: string;
  imageFileName: string;
  /** ClickHouse DateTime64(9) literal, e.g. "2026-05-31 10:00:00.000000000". */
  time: string;
  classUid?: number;
  processUid?: string;
  processParentPid?: number | null;
  orgId?: string;
  /**
   * ClickHouse DateTime64(3) literal for `arrived_at`. Omitted ⇒ ClickHouse assigns
   * it, as for every event the ingest route writes (SPEC-018 §Data contracts).
   */
  arrivedAt?: string;
}

export async function insertCgesEvent(config: Config, ev: InsertCgesEventRow): Promise<void> {
  await insertCgesEvents(config, [ev]);
}

/**
 * Insert `rows` in ONE ClickHouse INSERT: rows that leave `arrivedAt` unset share
 * one ClickHouse-assigned `arrived_at` (SPEC-018 §Context 6).
 */
export async function insertCgesEvents(config: Config, rows: InsertCgesEventRow[]): Promise<void> {
  const ch = createClient({
    url: config.INGEST_CH_URL,
    username: config.INGEST_CH_USER,
    password: config.INGEST_CH_PASSWORD,
    database: config.INGEST_CH_DB,
  });
  try {
    await ch.insert({
      table: "cges_events",
      values: rows.map((ev) => ({
        agent_id: ev.agentId,
        org_id: ev.orgId ?? "default",
        event_id: ev.eventId,
        class_uid: ev.classUid ?? 1007,
        activity_id: ev.activityId,
        process_pid: ev.processPid,
        process_uid: ev.processUid ?? "",
        process_name: ev.processName,
        process_parent_pid: ev.processParentPid ?? null,
        image_file_name: ev.imageFileName,
        time: ev.time,
        ...(ev.arrivedAt === undefined ? {} : { arrived_at: ev.arrivedAt }),
      })),
      format: "JSONEachRow",
    });
  } finally {
    await ch.close();
  }
}

export interface AlertRow {
  alert_id: string;
  rule_id: string | null;
  cg_detection_source: string;
  final_score: number;
  status: string;
  dedup_key: string;
  source_events: string[];
  cg_mitre: { tactics: string[]; techniques: string[] } | null;
  /** The source event's `time`, at whole seconds (alerts.ts `to_timestamp`). */
  event_time: Date;
}

export async function getAlerts(
  config: Config,
  opts: { agentId?: string; dedupKey?: string } = {},
): Promise<AlertRow[]> {
  const pool = new pg.Pool({ connectionString: config.INGEST_PG_URL });
  try {
    const clauses: string[] = [];
    const params: unknown[] = [];
    if (opts.agentId !== undefined) {
      params.push(opts.agentId);
      clauses.push(`agent_id = $${params.length}`);
    }
    if (opts.dedupKey !== undefined) {
      params.push(opts.dedupKey);
      clauses.push(`dedup_key = $${params.length}`);
    }
    const where = clauses.length > 0 ? `WHERE ${clauses.join(" AND ")}` : "";
    const r = await pool.query<AlertRow>(
      `SELECT alert_id, rule_id, cg_detection_source, final_score::float8 AS final_score,
              status, dedup_key, source_events, cg_mitre, event_time
       FROM alerts ${where}
       ORDER BY created_at`,
      params,
    );
    return r.rows;
  } finally {
    await pool.end();
  }
}

export async function setAlertStatus(
  config: Config,
  alertId: string,
  status: string,
): Promise<void> {
  const pool = new pg.Pool({ connectionString: config.INGEST_PG_URL });
  try {
    await pool.query("UPDATE alerts SET status = $2, updated_at = now() WHERE alert_id = $1", [
      alertId,
      status,
    ]);
  } finally {
    await pool.end();
  }
}

/** The org's stored detection cursor (SPEC-018 §Data contracts); null when it has no row yet. */
export async function getCursor(config: Config, orgId: string): Promise<DetectCursor | null> {
  const pool = new pg.Pool({ connectionString: config.INGEST_PG_URL });
  try {
    const r = await pool.query<{ last_arrived_at: string; last_event_id: string }>(
      "SELECT last_arrived_at, last_event_id FROM detect_watermark WHERE org_id = $1",
      [orgId],
    );
    const row = r.rows[0];
    return row === undefined
      ? null
      : { arrivedAt: row.last_arrived_at, eventId: row.last_event_id };
  } finally {
    await pool.end();
  }
}

/**
 * The org's last Process Activity row in arrival order, as ClickHouse orders it
 * (SPEC-018 §Operational §1): where a cycle that read everything leaves the cursor.
 */
export async function lastArrivedRow(config: Config, orgId: string): Promise<DetectCursor | null> {
  const ch = createClient({
    url: config.INGEST_CH_URL,
    username: config.INGEST_CH_USER,
    password: config.INGEST_CH_PASSWORD,
    database: config.INGEST_CH_DB,
  });
  try {
    const rs = await ch.query({
      query: `
        SELECT arrived_at, event_id
        FROM cges_events
        WHERE org_id = {org:String} AND class_uid = 1007
        ORDER BY arrived_at DESC, event_id DESC
        LIMIT 1
      `,
      query_params: { org: orgId },
      format: "JSONEachRow",
    });
    const row = (await rs.json<{ arrived_at: string; event_id: string }>())[0];
    return row === undefined ? null : { arrivedAt: row.arrived_at, eventId: row.event_id };
  } finally {
    await ch.close();
  }
}

// Enroll a dummy agent so an alert referencing it satisfies the
// alerts.agent_id -> agents foreign key (ADR-0012 §6 — every alert belongs to an
// enrolled agent). The synthetic detection tests fabricate cges_events directly
// (cges_events has no FK to agents), so they must enroll the agent the events
// belong to BEFORE runDetectionCycle inserts an alert. Idempotent.
export async function enrollTestAgent(
  config: Config,
  agentId: string,
  orgId = "default",
): Promise<void> {
  const pool = new pg.Pool({ connectionString: config.INGEST_PG_URL });
  try {
    await pool.query(
      `INSERT INTO agents (agent_id, org_id, pubkey, cert_pem, expires_at)
       VALUES ($1, $2, $3, $4, now() + interval '1 day')
       ON CONFLICT (agent_id) DO NOTHING`,
      [agentId, orgId, Buffer.from([0]), "test-cert"],
    );
  } finally {
    await pool.end();
  }
}

// SPEC-007 additions — incidents read/triage helpers for the incident-grouping
// harness. The incidents table lands in migration 0005 (runs in startBackends), so
// these query a real table. In the harness-first RED, incident_ac_002–005 throw at
// upsertIncident (NotImplemented) BEFORE reaching getIncidents — the RED is
// NotImplemented, never "incidents table missing".

export interface IncidentRow {
  incident_id: string;
  org_id: string;
  agent_id: string;
  status: string;
  // SPEC-011 — the incident's aggregated OCSF severity (MAX over member alerts).
  severity_id: number;
  assigned_to: string | null;
  cg_mitre: { tactics: string[]; techniques: string[] } | null;
  alert_ids: string[];
  grouping_key: string;
  updated_at: Date;
}

export async function getIncidents(
  config: Config,
  opts: { agentId?: string; orgId?: string } = {},
): Promise<IncidentRow[]> {
  const pool = new pg.Pool({ connectionString: config.INGEST_PG_URL });
  try {
    const clauses: string[] = [];
    const params: unknown[] = [];
    if (opts.agentId !== undefined) {
      params.push(opts.agentId);
      clauses.push(`agent_id = $${params.length}`);
    }
    if (opts.orgId !== undefined) {
      params.push(opts.orgId);
      clauses.push(`org_id = $${params.length}`);
    }
    const where = clauses.length > 0 ? `WHERE ${clauses.join(" AND ")}` : "";
    const r = await pool.query<IncidentRow>(
      `SELECT incident_id, org_id, agent_id, status, severity_id, assigned_to, cg_mitre, alert_ids, grouping_key,
              updated_at
       FROM incidents ${where}
       ORDER BY created_at`,
      params,
    );
    return r.rows;
  } finally {
    await pool.end();
  }
}

/** Simulate an analyst moving an incident's triage state (status / assignment). */
export async function setIncidentTriage(
  config: Config,
  incidentId: string,
  triage: { status?: string; assignedTo?: string },
): Promise<void> {
  const pool = new pg.Pool({ connectionString: config.INGEST_PG_URL });
  try {
    await pool.query(
      `UPDATE incidents
       SET status = COALESCE($2, status), assigned_to = COALESCE($3, assigned_to), updated_at = now()
       WHERE incident_id = $1`,
      [incidentId, triage.status ?? null, triage.assignedTo ?? null],
    );
  } finally {
    await pool.end();
  }
}
