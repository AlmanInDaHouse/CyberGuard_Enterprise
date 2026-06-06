import type { ClickHouseClient } from "@clickhouse/client";
import type { Pool } from "pg";
import type {
  AlertListItem,
  EventTimeline,
  IncidentDetail,
  IncidentListItem,
  Page,
  ResolvedAlert,
  TimelineEvent,
} from "./types.js";

/**
 * SPEC-009 §Data contracts / §Operational §1-§2 — the read queries. Every query
 * is scoped to the session's `org_id` (the object-level enforcement point, MVP
 * single-org; SPEC-009 §Security considerations). Lists are keyset-paginated with
 * a hard cap (NFR-009-001). Timestamps are returned as ISO strings; numeric scores
 * as numbers.
 */

const MAX_LIMIT = 200;
const DEFAULT_LIMIT = 50;

function clampLimit(raw: string | undefined): number {
  const n = Number(raw);
  return Number.isFinite(n) && n > 0 ? Math.min(Math.floor(n), MAX_LIMIT) : DEFAULT_LIMIT;
}

interface Cursor {
  t: string;
  id: string;
}
function encodeCursor(c: Cursor): string {
  return Buffer.from(JSON.stringify(c)).toString("base64url");
}
function decodeCursor(raw: string): Cursor | null {
  try {
    const c = JSON.parse(Buffer.from(raw, "base64url").toString()) as Cursor;
    return typeof c.t === "string" && typeof c.id === "string" ? c : null;
  } catch {
    return null;
  }
}

export interface ListParams {
  status?: string;
  agentId?: string;
  severityId?: string;
  limit?: string;
  cursor?: string;
}

interface IncidentRow {
  incident_id: string;
  agent_id: string;
  status: string;
  title: string;
  severity_id: number;
  cg_mitre: { tactics: string[]; techniques: string[] } | null;
  alert_count: number;
  window_start: Date;
  updated_at: Date;
}

export async function listIncidents(
  pool: Pool,
  orgId: string,
  p: ListParams,
): Promise<Page<IncidentListItem>> {
  const limit = clampLimit(p.limit);
  const where: string[] = ["org_id = $1"];
  const args: unknown[] = [orgId];
  if (p.status) {
    args.push(p.status);
    where.push(`status = $${args.length}`);
  }
  if (p.agentId) {
    args.push(p.agentId);
    where.push(`agent_id = $${args.length}`);
  }
  const cur = p.cursor ? decodeCursor(p.cursor) : null;
  if (cur) {
    args.push(cur.t, cur.id);
    where.push(
      `(updated_at, incident_id) < ($${args.length - 1}::timestamptz, $${args.length}::uuid)`,
    );
  }
  args.push(limit + 1);
  const r = await pool.query<IncidentRow>(
    `SELECT incident_id, agent_id, status, title, severity_id, cg_mitre,
            cardinality(alert_ids) AS alert_count, window_start, updated_at
     FROM incidents WHERE ${where.join(" AND ")}
     ORDER BY updated_at DESC, incident_id DESC LIMIT $${args.length}`,
    args,
  );
  const hasMore = r.rows.length > limit;
  const page = r.rows.slice(0, limit);
  const items: IncidentListItem[] = page.map((row) => ({
    incident_id: row.incident_id,
    agent_id: row.agent_id,
    status: row.status,
    title: row.title,
    severity_id: row.severity_id,
    cg_mitre: row.cg_mitre,
    alert_count: Number(row.alert_count),
    window_start: row.window_start.toISOString(),
    updated_at: row.updated_at.toISOString(),
  }));
  const last = page[page.length - 1];
  const next_cursor =
    hasMore && last
      ? encodeCursor({ t: last.updated_at.toISOString(), id: last.incident_id })
      : null;
  return { items, next_cursor };
}

interface IncidentDetailRow {
  incident_id: string;
  agent_id: string;
  status: string;
  title: string;
  severity_id: number;
  cg_mitre: { tactics: string[]; techniques: string[] } | null;
  window_start: Date;
  assigned_to: string | null;
  created_at: Date;
  updated_at: Date;
  alert_ids: string[];
}
interface AlertRow {
  alert_id: string;
  title: string;
  severity_id: number;
  status: string;
  rule_id: string | null;
  cg_mitre: { tactics: string[]; techniques: string[] } | null;
  event_time: Date;
  final_score: number;
  // SPEC-010 — the alert's raw cges_events.event_id list (uuid[] → string[], the
  // node-postgres convention, like IncidentDetailRow.alert_ids). The forensic drill
  // consumes it; mapAlert does NOT expose it (ResolvedAlert/IncidentDetail unchanged).
  source_events: string[];
}

function mapAlert(a: AlertRow): ResolvedAlert {
  return {
    alert_id: a.alert_id,
    title: a.title,
    severity_id: a.severity_id,
    status: a.status,
    rule_id: a.rule_id,
    cg_mitre: a.cg_mitre,
    event_time: a.event_time.toISOString(),
    final_score: a.final_score,
  };
}

const ALERT_COLS =
  "alert_id, title, severity_id, status, rule_id, cg_mitre, event_time, final_score::float8 AS final_score, source_events";

export async function getIncidentDetail(
  pool: Pool,
  orgId: string,
  id: string,
): Promise<IncidentDetail | null> {
  const ir = await pool.query<IncidentDetailRow>(
    `SELECT incident_id, agent_id, status, title, severity_id, cg_mitre, window_start,
            assigned_to, created_at, updated_at, alert_ids
     FROM incidents WHERE incident_id = $1 AND org_id = $2`,
    [id, orgId],
  );
  const inc = ir.rows[0];
  if (!inc) return null;
  const ar = await pool.query<AlertRow>(
    `SELECT ${ALERT_COLS} FROM alerts
     WHERE alert_id = ANY($1::uuid[]) AND org_id = $2
     ORDER BY event_time DESC, alert_id DESC`,
    [inc.alert_ids, orgId],
  );
  return {
    incident_id: inc.incident_id,
    agent_id: inc.agent_id,
    status: inc.status,
    title: inc.title,
    severity_id: inc.severity_id,
    cg_mitre: inc.cg_mitre,
    window_start: inc.window_start.toISOString(),
    assigned_to: inc.assigned_to,
    created_at: inc.created_at.toISOString(),
    updated_at: inc.updated_at.toISOString(),
    alerts: ar.rows.map(mapAlert),
  };
}

interface TimelineRow {
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

function mapTimelineEvent(r: TimelineRow): TimelineEvent {
  return {
    event_id: r.event_id,
    agent_id: r.agent_id,
    activity_id: r.activity_id,
    process_pid: r.process_pid,
    process_uid: r.process_uid,
    process_name: r.process_name,
    image_file_name: r.image_file_name,
    process_parent_pid: r.process_parent_pid,
    event_time: r.event_time,
  };
}

/**
 * SPEC-010 — the forensic event drill (step 1). Resolves an incident's grouped
 * alerts in Postgres (reusing the getIncidentDetail set-membership resolution),
 * gathers their `source_events`, flattens + dedups the event ids in TS (the repo's
 * canonical array handling — there is no array_agg/unnest precedent), and reads the
 * raw events from ClickHouse by event_id: the cross-store join `cges_events.event_id
 * = ANY(alerts.source_events)` (ADR-0015). Org-scoped on BOTH stores. Returns null
 * when the incident does not exist or is cross-org (the route → 404, no existence
 * oracle); an existing incident that resolves to no source_events returns
 * `{ events: [] }` (200), never 404. A malformed (non-UUID) `:id` throws on the pg
 * cast and is caught by the route → 404, mirroring getIncidentDetail.
 */
export async function getIncidentEventTimeline(
  pool: Pool,
  ch: ClickHouseClient,
  orgId: string,
  id: string,
): Promise<EventTimeline | null> {
  const ir = await pool.query<{ alert_ids: string[] }>(
    "SELECT alert_ids FROM incidents WHERE incident_id = $1 AND org_id = $2",
    [id, orgId],
  );
  const inc = ir.rows[0];
  if (!inc) return null;

  const ar = await pool.query<AlertRow>(
    `SELECT ${ALERT_COLS} FROM alerts
     WHERE alert_id = ANY($1::uuid[]) AND org_id = $2`,
    [inc.alert_ids, orgId],
  );
  const eventIds = [...new Set(ar.rows.flatMap((a) => a.source_events))];
  if (eventIds.length === 0) return { incident_id: id, events: [] };

  const rs = await ch.query({
    query: `
      SELECT toString(event_id) AS event_id, toString(agent_id) AS agent_id, activity_id,
             process_pid, process_uid, process_name, image_file_name,
             process_parent_pid, toString(time) AS event_time
      FROM cges_events FINAL
      WHERE org_id = {org:String}
        AND toString(event_id) IN ({ids:Array(String)})
      ORDER BY time ASC, event_id ASC
    `,
    query_params: { org: orgId, ids: eventIds },
    format: "JSONEachRow",
  });
  const rows = await rs.json<TimelineRow>();
  return { incident_id: id, events: rows.map(mapTimelineEvent) };
}

export async function listAlerts(
  pool: Pool,
  orgId: string,
  p: ListParams,
): Promise<Page<AlertListItem>> {
  const limit = clampLimit(p.limit);
  const where: string[] = ["org_id = $1"];
  const args: unknown[] = [orgId];
  if (p.status) {
    args.push(p.status);
    where.push(`status = $${args.length}`);
  }
  if (p.agentId) {
    args.push(p.agentId);
    where.push(`agent_id = $${args.length}`);
  }
  if (p.severityId) {
    args.push(Number(p.severityId));
    where.push(`severity_id = $${args.length}`);
  }
  const cur = p.cursor ? decodeCursor(p.cursor) : null;
  if (cur) {
    args.push(cur.t, cur.id);
    where.push(
      `(event_time, alert_id) < ($${args.length - 1}::timestamptz, $${args.length}::uuid)`,
    );
  }
  args.push(limit + 1);
  const r = await pool.query<AlertRow>(
    `SELECT ${ALERT_COLS} FROM alerts WHERE ${where.join(" AND ")}
     ORDER BY event_time DESC, alert_id DESC LIMIT $${args.length}`,
    args,
  );
  const hasMore = r.rows.length > limit;
  const page = r.rows.slice(0, limit);
  const items = page.map(mapAlert);
  const last = page[page.length - 1];
  const next_cursor =
    hasMore && last ? encodeCursor({ t: last.event_time.toISOString(), id: last.alert_id }) : null;
  return { items, next_cursor };
}
