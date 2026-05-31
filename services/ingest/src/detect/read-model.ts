import { type ClickHouseClient, createClient } from "@clickhouse/client";
import pg from "pg";
import type { DetectConfig, NormalizedProcessEvent } from "./types.js";

// SPEC-006 5b — the read-model. Reads CGES Process Activity (class_uid 1007)
// from ClickHouse cges_events forward by a per-org watermark (FINAL collapses
// at-least-once duplicates, ADR-0012 §2/§7), resolves the parent image via a
// two-step parent-pid look-back (SPEC-006 §Operational §2), and manages the
// Postgres detect_watermark cursor. It does NOT evaluate rules (5c), score (5d),
// or persist alerts (5e); runDetectionCycle (index.ts) stays NotImplemented
// until 5e wires these together.
//
// NOTE on aliasing: the projected timestamp is aliased `event_time`, NOT `time`.
// Aliasing `toString(time) AS time` would shadow the DateTime64 `time` column in
// the WHERE clause (ClickHouse resolves the String alias there), producing
// "No operation greater between String and DateTime64".

/** Epoch default; mirrors the detect_watermark.last_time column default (0003). */
const WATERMARK_EPOCH = "1970-01-01 00:00:00.000000000";

interface ChildRow {
  event_id: string;
  agent_id: string;
  activity_id: number;
  process_pid: number;
  process_uid: string;
  process_name: string;
  image_file_name: string;
  process_parent_pid: number | null;
  event_time: string;
}

interface ParentRow {
  agent_id: string;
  process_pid: number;
  image_file_name: string;
}

function chClient(config: DetectConfig): ClickHouseClient {
  return createClient({
    url: config.ingest.INGEST_CH_URL,
    username: config.ingest.INGEST_CH_USER,
    password: config.ingest.INGEST_CH_PASSWORD,
    database: config.ingest.INGEST_CH_DB,
  });
}

function parentKey(agentId: string, pid: number): string {
  return `${agentId}:${pid}`;
}

/**
 * Read the next batch of Process Activity events with `time` strictly after the
 * watermark, FINAL-collapsed, ordered ascending. Each event carries its resolved
 * `parentImage` (null when the parent Launch was not captured — see §2).
 *
 * Column names are read VERBATIM from cges_events (process_pid, process_name,
 * image_file_name, process_parent_pid, …), NEVER the OCSF names (cmd_line / file
 * / user / parent_process), which are not columns. process_command_line and
 * subject_user_sid are structurally empty in v0.1 and are not projected.
 */
export async function readNewEvents(
  config: DetectConfig,
  watermark: string,
  limit: number,
): Promise<NormalizedProcessEvent[]> {
  const ch = chClient(config);
  try {
    const childRs = await ch.query({
      query: `
        SELECT toString(event_id) AS event_id, toString(agent_id) AS agent_id, activity_id,
               process_pid, process_uid, process_name, image_file_name,
               process_parent_pid, toString(time) AS event_time
        FROM cges_events FINAL
        WHERE org_id = {org:String}
          AND class_uid = 1007
          AND time > parseDateTime64BestEffort({watermark:String}, 9, 'UTC')
        ORDER BY time ASC
        LIMIT {batch:UInt32}
      `,
      query_params: { org: config.orgId, watermark, batch: limit },
      format: "JSONEachRow",
    });
    const children = await childRs.json<ChildRow>();
    if (children.length === 0) return [];

    const parents = await resolveParents(ch, config, children);

    return children.map((c) => ({
      eventId: c.event_id,
      agentId: c.agent_id,
      activityId: c.activity_id,
      pid: c.process_pid,
      uid: c.process_uid,
      processName: c.process_name,
      imageFileName: c.image_file_name,
      parentPid: c.process_parent_pid,
      parentImage:
        c.process_parent_pid === null
          ? null
          : (parents.get(parentKey(c.agent_id, c.process_parent_pid)) ?? null),
      time: c.event_time,
    }));
  } finally {
    await ch.close();
  }
}

/**
 * Step 2 of the parent-pid self-join (SPEC-006 §Operational §2): query parent
 * Launches for the batch's parent pids within the correlation-window look-back —
 * INDEPENDENT of the watermark, since the parent may have launched before it.
 * Returns a `(agent_id, pid) -> most-recent image` map. A parent launched more
 * than `correlationWindowSeconds` before the child, or never captured (e.g. an
 * Office app already running when the agent started), is absent from the map →
 * the caller resolves parent_image = null (the documented production
 * false-negative, "or outside the correlation window"). PID reuse inside the
 * window is accepted v0.1 best-effort per §2 (most-recent-wins).
 */
async function resolveParents(
  ch: ClickHouseClient,
  config: DetectConfig,
  children: ChildRow[],
): Promise<Map<string, string>> {
  const map = new Map<string, string>();
  const parentPids = [
    ...new Set(children.map((c) => c.process_parent_pid).filter((p): p is number => p !== null)),
  ];
  if (parentPids.length === 0) return map;

  const agentIds = [...new Set(children.map((c) => c.agent_id))];
  const times = children.map((c) => c.event_time);
  const minChildTime = times.reduce((a, b) => (a < b ? a : b));
  const maxChildTime = times.reduce((a, b) => (a > b ? a : b));

  const parentRs = await ch.query({
    query: `
      SELECT toString(agent_id) AS agent_id, process_pid, image_file_name
      FROM cges_events FINAL
      WHERE org_id = {org:String}
        AND class_uid = 1007
        AND activity_id = 1
        AND toString(agent_id) IN ({agents:Array(String)})
        AND process_pid IN ({pids:Array(UInt32)})
        AND time >= subtractSeconds(parseDateTime64BestEffort({lo:String}, 9, 'UTC'), {win:UInt32})
        AND time <= parseDateTime64BestEffort({hi:String}, 9, 'UTC')
      ORDER BY time ASC
    `,
    query_params: {
      org: config.orgId,
      agents: agentIds,
      pids: parentPids,
      lo: minChildTime,
      hi: maxChildTime,
      win: config.correlationWindowSeconds,
    },
    format: "JSONEachRow",
  });
  const parents = await parentRs.json<ParentRow>();
  // Rows arrive ascending by time, so the most-recent Launch per (agent, pid) overwrites.
  for (const p of parents) {
    map.set(parentKey(p.agent_id, p.process_pid), p.image_file_name);
  }
  return map;
}

/** Read the per-org watermark; the epoch default when no row exists yet. */
export async function getWatermark(config: DetectConfig): Promise<string> {
  const pool = new pg.Pool({ connectionString: config.ingest.INGEST_PG_URL });
  try {
    const r = await pool.query<{ last_time: string }>(
      "SELECT last_time FROM detect_watermark WHERE org_id = $1",
      [config.orgId],
    );
    return r.rows[0]?.last_time ?? WATERMARK_EPOCH;
  } finally {
    await pool.end();
  }
}

/** Advance the per-org watermark to `maxTime` (the max `time` of a processed batch). */
export async function advanceWatermark(config: DetectConfig, maxTime: string): Promise<void> {
  const pool = new pg.Pool({ connectionString: config.ingest.INGEST_PG_URL });
  try {
    await pool.query(
      `INSERT INTO detect_watermark (org_id, last_time, updated_at)
       VALUES ($1, $2, now())
       ON CONFLICT (org_id) DO UPDATE SET last_time = excluded.last_time, updated_at = now()`,
      [config.orgId, maxTime],
    );
  } finally {
    await pool.end();
  }
}
