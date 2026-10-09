import { type ClickHouseClient, createClient } from "@clickhouse/client";
import pg from "pg";
import {
  type DetectConfig,
  type DetectCursor,
  type NormalizedProcessEvent,
  PARENT_LOOKBACK_SECONDS,
  SETTLE_MARGIN_MS,
} from "./types.js";

// SPEC-006 5b — the read-model. Reads CGES Process Activity (class_uid 1007)
// from ClickHouse cges_events forward by a per-org arrival cursor (SPEC-018
// §Operational §1, amending SPEC-006 §Operational §1 by scope; ADR-0012
// Amendment 2026-10-09), resolves each child's parent image per child (SPEC-016
// §Operational §1, amending SPEC-006 §Operational §2 by scope), and manages the
// Postgres detect_watermark cursor. It does NOT evaluate rules (engine.ts),
// score (scorer.ts), or persist alerts (alerts.ts); runDetectionCycle (index.ts)
// wires those steps together.
//
// NOTE on aliasing: the projected timestamp is aliased `event_time`, NOT `time`.
// Aliasing `toString(time) AS time` would shadow the DateTime64 `time` column in
// the WHERE clause (ClickHouse resolves the String alias there), producing
// "No operation greater between String and DateTime64". For the same reason the
// forward read projects `event_id` and `arrived_at` unaliased: its WHERE and
// ORDER BY must compare and order the UUID and DateTime64 columns themselves.

/** The cursor before any read; mirrors the detect_watermark column defaults (0007). */
const CURSOR_START: DetectCursor = {
  arrivedAt: "1970-01-01 00:00:00.000",
  eventId: "00000000-0000-0000-0000-000000000000",
};

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
  arrived_at: string;
}

/** One forward read: its events, and the cursor of its last row. */
export interface ReadBatch {
  events: NormalizedProcessEvent[];
  /** The `(arrived_at, event_id)` of the last row returned; null when the read was empty. */
  cursor: DetectCursor | null;
}

/** A candidate parent: a Launch of one of the batch's parent pids inside the look-back. */
interface LaunchRow {
  agent_id: string;
  process_pid: number;
  process_uid: string;
  image_file_name: string;
  event_time: string;
}

/** A Terminate of a candidate parent, matched by process_uid (ADR-0011 §6). */
interface TerminateRow {
  agent_id: string;
  process_uid: string;
  event_time: string;
}

function chClient(config: DetectConfig): ClickHouseClient {
  return createClient({
    url: config.ingest.INGEST_CH_URL,
    username: config.ingest.INGEST_CH_USER,
    password: config.ingest.INGEST_CH_PASSWORD,
    database: config.ingest.INGEST_CH_DB,
  });
}

function processKey(agentId: string, pid: number): string {
  return `${agentId}:${pid}`;
}

function uidKey(agentId: string, uid: string): string {
  return `${agentId}/${uid}`;
}

/**
 * `time` ("YYYY-MM-DD HH:MM:SS.fffffffff", as ClickHouse prints a DateTime64(9))
 * minus whole `seconds`, in the same form, so the two compare lexicographically.
 */
function minusSeconds(time: string, seconds: number): string {
  const ms = Date.parse(`${time.slice(0, 10)}T${time.slice(11, 19)}Z`) - seconds * 1000;
  const iso = new Date(ms).toISOString();
  return `${iso.slice(0, 10)} ${iso.slice(11, 19)}${time.slice(19)}`;
}

/**
 * Read the next batch of Process Activity events after `cursor`, in arrival order
 * (SPEC-018 §Operational §1): the rows whose `(arrived_at, event_id)` is after the
 * cursor and whose `arrived_at` is at least the settle margin older than
 * ClickHouse's `now64(3)` (§2), ordered by `(arrived_at, event_id)`, at most
 * `limit`. No FINAL: a resent event is a later row and is read again; the
 * dedup_key makes a repeated match a no-op (§3). The lower bound on `arrived_at`
 * is a predicate of its own beside the tuple comparison, which is what lets the
 * ix_arrived_at index prune (§7). The margin is measured on ClickHouse's clock,
 * the one that assigned `arrived_at`, never on this host's.
 *
 * The returned cursor is the `(arrived_at, event_id)` of the last row, as
 * ClickHouse returned it: ClickHouse orders and compares the pair, and this
 * service never orders `event_id` values itself. Each event carries its resolved
 * `parentImage` (null when no parent resolves — see resolveParents).
 *
 * Column names are read VERBATIM from cges_events (process_pid, process_name,
 * image_file_name, process_parent_pid, …), NEVER the OCSF names (cmd_line / file
 * / user / parent_process), which are not columns. process_command_line and
 * subject_user_sid are structurally empty in v0.1 and are not projected.
 */
export async function readNewEvents(
  config: DetectConfig,
  cursor: DetectCursor,
  limit: number,
): Promise<ReadBatch> {
  const ch = chClient(config);
  try {
    const childRs = await ch.query({
      query: `
        SELECT event_id, toString(agent_id) AS agent_id, activity_id,
               process_pid, process_uid, process_name, image_file_name,
               process_parent_pid, toString(time) AS event_time, arrived_at
        FROM cges_events
        WHERE org_id = {org:String}
          AND class_uid = 1007
          AND arrived_at >= {cursorAt:DateTime64(3, 'UTC')}
          AND (arrived_at, event_id) > ({cursorAt:DateTime64(3, 'UTC')}, {cursorId:UUID})
          AND arrived_at <= subtractMilliseconds(now64(3), {settle:UInt32})
        ORDER BY arrived_at ASC, event_id ASC
        LIMIT {batch:UInt32}
      `,
      query_params: {
        org: config.orgId,
        cursorAt: cursor.arrivedAt,
        cursorId: cursor.eventId,
        settle: config.settleMarginMs ?? SETTLE_MARGIN_MS,
        batch: limit,
      },
      format: "JSONEachRow",
    });
    const children = await childRs.json<ChildRow>();
    const last = children.at(-1);
    if (last === undefined) return { events: [], cursor: null };

    const parentImages = await resolveParents(ch, config, children);

    const events = children.map((c, i) => ({
      eventId: c.event_id,
      agentId: c.agent_id,
      activityId: c.activity_id,
      pid: c.process_pid,
      uid: c.process_uid,
      processName: c.process_name,
      imageFileName: c.image_file_name,
      parentPid: c.process_parent_pid,
      parentImage: parentImages[i] ?? null,
      time: c.event_time,
    }));
    return { events, cursor: { arrivedAt: last.arrived_at, eventId: last.event_id } };
  } finally {
    await ch.close();
  }
}

/**
 * Parent resolution, per child (SPEC-016 §Operational §1). For each child R is
 * the most recent Launch with the child's agent_id, process_pid = the child's
 * process_parent_pid, and child.time - PARENT_LOOKBACK_SECONDS <= R.time <=
 * child.time; the child's parent image is R.image_file_name, or null when there
 * is no such R or R's own Terminate (same agent_id and process_uid) is earlier
 * than the child. An empty process_uid matches no Terminate. Independent of the
 * watermark: the parent may have launched in an earlier batch.
 *
 * Two queries serve the whole batch — the candidate Launches of its parent pids
 * across [oldest child - look-back, newest child], and the Terminates of their
 * process_uids across the same span — and each child then picks its own R, so a
 * later reuse of the parent's pid in the same batch never replaces it. A parent
 * that started before the agent's ETW session was never captured and resolves to
 * null (SPEC-006 §Operational §2, unchanged). Returns one entry per child, in order.
 */
async function resolveParents(
  ch: ClickHouseClient,
  config: DetectConfig,
  children: ChildRow[],
): Promise<Array<string | null>> {
  const withParent = children.filter((c) => c.process_parent_pid !== null);
  if (withParent.length === 0) return children.map(() => null);

  const agentIds = [...new Set(withParent.map((c) => c.agent_id))];
  const parentPids = [
    ...new Set(withParent.map((c) => c.process_parent_pid).filter((p): p is number => p !== null)),
  ];
  const times = withParent.map((c) => c.event_time);
  const oldestChild = times.reduce((a, b) => (a < b ? a : b));
  const newestChild = times.reduce((a, b) => (a > b ? a : b));
  const span = {
    org: config.orgId,
    agents: agentIds,
    lo: oldestChild,
    hi: newestChild,
    lookback: PARENT_LOOKBACK_SECONDS,
  };

  const launchRs = await ch.query({
    query: `
      SELECT toString(agent_id) AS agent_id, process_pid, process_uid, image_file_name,
             toString(time) AS event_time
      FROM cges_events FINAL
      WHERE org_id = {org:String}
        AND class_uid = 1007
        AND activity_id = 1
        AND toString(agent_id) IN ({agents:Array(String)})
        AND process_pid IN ({pids:Array(UInt32)})
        AND time >= subtractSeconds(parseDateTime64BestEffort({lo:String}, 9, 'UTC'), {lookback:UInt32})
        AND time <= parseDateTime64BestEffort({hi:String}, 9, 'UTC')
      ORDER BY time ASC
    `,
    query_params: { ...span, pids: parentPids },
    format: "JSONEachRow",
  });
  const launches = await launchRs.json<LaunchRow>();

  // Candidate Launches per (agent, pid), ascending by time.
  const launchesByProcess = new Map<string, LaunchRow[]>();
  for (const l of launches) {
    const key = processKey(l.agent_id, l.process_pid);
    const list = launchesByProcess.get(key);
    if (list === undefined) launchesByProcess.set(key, [l]);
    else list.push(l);
  }

  // Earliest Terminate per (agent, process_uid) of the candidates.
  const firstTerminate = new Map<string, string>();
  const uids = [...new Set(launches.map((l) => l.process_uid).filter((u) => u !== ""))];
  if (uids.length > 0) {
    const terminateRs = await ch.query({
      query: `
        SELECT toString(agent_id) AS agent_id, process_uid, toString(time) AS event_time
        FROM cges_events FINAL
        WHERE org_id = {org:String}
          AND class_uid = 1007
          AND activity_id = 2
          AND toString(agent_id) IN ({agents:Array(String)})
          AND process_uid IN ({uids:Array(String)})
          AND time >= subtractSeconds(parseDateTime64BestEffort({lo:String}, 9, 'UTC'), {lookback:UInt32})
          AND time <= parseDateTime64BestEffort({hi:String}, 9, 'UTC')
      `,
      query_params: { ...span, uids },
      format: "JSONEachRow",
    });
    for (const t of await terminateRs.json<TerminateRow>()) {
      const key = uidKey(t.agent_id, t.process_uid);
      const seen = firstTerminate.get(key);
      if (seen === undefined || t.event_time < seen) firstTerminate.set(key, t.event_time);
    }
  }

  return children.map((c) => {
    if (c.process_parent_pid === null) return null;
    const candidates = launchesByProcess.get(processKey(c.agent_id, c.process_parent_pid)) ?? [];
    // The most recent Launch at or before the child, inside the look-back.
    const parent = candidates.findLast((l) => l.event_time <= c.event_time);
    if (parent === undefined) return null;
    if (parent.event_time < minusSeconds(c.event_time, PARENT_LOOKBACK_SECONDS)) return null;
    // A candidate that terminated before the child cannot be its parent.
    if (parent.process_uid !== "") {
      const terminated = firstTerminate.get(uidKey(parent.agent_id, parent.process_uid));
      if (terminated !== undefined && terminated < c.event_time) return null;
    }
    return parent.image_file_name;
  });
}

/** Read the per-org cursor (SPEC-018 §Data contracts); the start when no row exists yet. */
export async function getCursor(config: DetectConfig): Promise<DetectCursor> {
  const pool = new pg.Pool({ connectionString: config.ingest.INGEST_PG_URL });
  try {
    const r = await pool.query<{ last_arrived_at: string; last_event_id: string }>(
      "SELECT last_arrived_at, last_event_id FROM detect_watermark WHERE org_id = $1",
      [config.orgId],
    );
    const row = r.rows[0];
    if (row === undefined) return CURSOR_START;
    return { arrivedAt: row.last_arrived_at, eventId: row.last_event_id };
  } finally {
    await pool.end();
  }
}

/** Advance the per-org cursor to `cursor`, the last row of a processed batch. */
export async function advanceCursor(config: DetectConfig, cursor: DetectCursor): Promise<void> {
  const pool = new pg.Pool({ connectionString: config.ingest.INGEST_PG_URL });
  try {
    await pool.query(
      `INSERT INTO detect_watermark (org_id, last_arrived_at, last_event_id, updated_at)
       VALUES ($1, $2, $3, now())
       ON CONFLICT (org_id) DO UPDATE
         SET last_arrived_at = excluded.last_arrived_at,
             last_event_id = excluded.last_event_id,
             updated_at = now()`,
      [config.orgId, cursor.arrivedAt, cursor.eventId],
    );
  } finally {
    await pool.end();
  }
}
