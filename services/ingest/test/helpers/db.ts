import { createClient } from "@clickhouse/client";
import pg from "pg";
import type { Config } from "../../src/config.js";

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
// Queries the cges_events ClickHouse table for rows belonging to a
// given agent_id, ordered by capture timestamp ascending. The table
// itself does not exist yet (D6 PARTIALLY DONE; literal DDL lands in
// Phase 3.5 alongside the schema acceptance for events[] envelopes);
// this helper compiles cleanly and runs once Phase 3.5 lands the DDL.

export interface CgesEventRow {
  agent_id: string;
  class_uid: number;
  activity_id: number;
  process_pid: number;
  process_uid: string;
  process_name: string;
  process_created_time: string | null;
  process_exit_code: number | null;
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
          time
        FROM cges_events
        WHERE agent_id = {agent_id:String}
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

// SPEC-006 additions — synthetic cges_events insert + alerts/watermark helpers
// for the Detection MVP harness. cges_events exists (migrations run in
// startBackends), so insertCgesEvent succeeds — it is "setup-that-exists". The
// alerts and detect_watermark tables do NOT exist yet (migration 0002_alerts
// lands in the Phase-5 impl); getAlerts/setAlertStatus/getWatermark compile
// cleanly and run once that migration lands — same pattern as getCgesEvents.
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
}

export async function insertCgesEvent(config: Config, ev: InsertCgesEventRow): Promise<void> {
  const ch = createClient({
    url: config.INGEST_CH_URL,
    username: config.INGEST_CH_USER,
    password: config.INGEST_CH_PASSWORD,
    database: config.INGEST_CH_DB,
  });
  try {
    await ch.insert({
      table: "cges_events",
      values: [
        {
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
        },
      ],
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
              status, dedup_key, source_events
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

export async function getWatermark(config: Config, orgId: string): Promise<string | null> {
  const pool = new pg.Pool({ connectionString: config.INGEST_PG_URL });
  try {
    const r = await pool.query<{ last_time: string }>(
      "SELECT last_time::text AS last_time FROM detect_watermark WHERE org_id = $1",
      [orgId],
    );
    return r.rows[0]?.last_time ?? null;
  } finally {
    await pool.end();
  }
}
