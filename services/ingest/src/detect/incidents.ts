import pg from "pg";
import { v7 as uuidv7 } from "uuid";
import { eventUnixSeconds } from "./alerts.js";
import { INCIDENT_CORRELATION_WINDOW_SECONDS } from "./types.js";
import type { DetectConfig, IncidentGroupingInput } from "./types.js";

// SPEC-007 — incident grouping. Groups distinct correlated alerts (ADR-0013
// event-time windowing) into incidents via a declarative grouping_key +
// INSERT … ON CONFLICT (grouping_key) DO UPDATE, on the key of the open incident
// (SPEC-026 §Operational §7). The grouping is the incident analogue of SPEC-006's
// alert dedup, one level coarser: dedup collapses identical re-fires (DO NOTHING),
// grouping accretes distinct alerts (DO UPDATE) without clobbering human triage.

/**
 * Canonical tactic token for the grouping key (SPEC-007 §Data contracts §4): the
 * alert's MITRE tactics, de-duplicated, sorted, and joined — order-independent, so
 * two alerts with the same tactic-set route to the same incident.
 */
function canonicalTactics(tactics: string[]): string {
  return [...new Set(tactics)].sort().join(",");
}

/**
 * The declarative correlation key (SPEC-007 §Data contracts §4):
 * `<org>::<agent>::<canonical_tactics>::<window_bucket>`, event-time windowed
 * (ADR-0013 §1) over INCIDENT_CORRELATION_WINDOW_SECONDS. The bucket embeds the
 * window so create-or-update is a single declarative upsert (no read-then-write
 * race), exactly as `dedup_key` does for alerts.
 */
export function buildGroupingKey(alert: IncidentGroupingInput): {
  groupingKey: string;
  windowBucket: number;
} {
  const windowBucket = Math.floor(
    eventUnixSeconds(alert.eventTime) / INCIDENT_CORRELATION_WINDOW_SECONDS,
  );
  const groupingKey = `${alert.orgId}::${alert.agentId}::${canonicalTactics(
    alert.cgMitre.tactics,
  )}::${windowBucket}`;
  return { groupingKey, windowBucket };
}

/**
 * SPEC-007 §Operational §1/§4/§6 — create-or-update an incident for a newly
 * persisted alert:
 *
 * - No OPEN incident for the grouping_key ⇒ CREATE (incident_id UUIDv7, status
 *   'open', activity_id 1 = Created, alert_ids = [this alert], severity_id = this
 *   alert's severity, window_start = the bucket start, deterministic title). An
 *   incident of the key that is `resolved` or `false_positive` is closed: it takes
 *   no new alert, and the alert opens a new incident, a creation, notified and running
 *   its playbooks (SPEC-026 §Operational §7). The upsert conflicts on the partial
 *   unique index of the open incident's key (0011_dashboard_rights): a close that
 *   commits while this statement waits on the row's lock moves the row out of that
 *   index, and the statement inserts instead.
 * - An open incident exists for the grouping_key ⇒ the `DO UPDATE` SET splits
 *   the row into two field classes (SPEC-011 §Operational §4):
 *     · MACHINE-RECOMPUTED on every correlated alert — `alert_ids` APPENDS this id
 *       (idempotent: `@>` skips an already-present id, preserving order and
 *       uniqueItems), `severity_id` is raised to GREATEST(current, incoming) so
 *       severity tracks the running MAX over the members, and `cg_mitre.techniques`
 *       becomes the incident's techniques followed by the alert's that it lacks, in the
 *       alert's order (SPEC-026 §Operational §7); `updated_at` is bumped.
 *     · PRESERVED triage-state — `status` / `assigned_to` / `activity_id` /
 *       `cg_mitre.tactics` / `title` are NOT in the SET, so a new correlated alert
 *       never resets a human's triage (the triage-preservation invariant, §Operational §4).
 *   Severity and techniques are machine-derived evidence, not triage-state, so
 *   recomputing them extends the recomputed class without weakening the preservation
 *   promise.
 *
 * The `incidents.agent_id → agents` FK is production-faithful (Convention #12):
 * the alert (hence the incident) belongs to an enrolled agent. window_start is the
 * bucket-start UTC timestamp, derived from event-time, never insert-time.
 *
 * SPEC-014 — the upsert now reports whether it CREATED a new incident (vs a
 * correlated DO UPDATE) so the caller can fire a notify on creation only. Because
 * the statement is `ON CONFLICT DO UPDATE`, `rowCount` does NOT discriminate
 * insert from update (a conflicting update also reports an affected row — unlike
 * `upsertAlert`'s `DO NOTHING` where `rowCount === 1` means insert). The reliable
 * discriminator on a `DO UPDATE` is the system column `xmax`: `xmax = 0` on a
 * fresh INSERT, non-zero on a conflict update. `RETURNING … (xmax = 0) AS inserted`
 * plus `incident_id` / `title` / `severity_id` gives the notify payload with no
 * extra read. The function stays a pure DB write — the email side-effect lives in
 * the caller (ADR-0017 §Decision §2).
 */
export interface UpsertIncidentResult {
  /** True iff this call INSERTed a new incident row (`xmax = 0`); false on a correlated DO UPDATE. */
  created: boolean;
  incidentId: string;
  title: string;
  severityId: number;
  orgId: string;
}

interface UpsertIncidentRow {
  incident_id: string;
  title: string;
  severity_id: number;
  inserted: boolean;
}

export async function upsertIncident(
  config: DetectConfig,
  alert: IncidentGroupingInput,
): Promise<UpsertIncidentResult> {
  const { groupingKey, windowBucket } = buildGroupingKey(alert);
  const windowStartSeconds = windowBucket * INCIDENT_CORRELATION_WINDOW_SECONDS;
  const title = `${canonicalTactics(alert.cgMitre.tactics)} activity on ${alert.agentId}`;
  const pool = new pg.Pool({ connectionString: config.ingest.INGEST_PG_URL });
  try {
    const result = await pool.query<UpsertIncidentRow>(
      `INSERT INTO incidents
         (incident_id, org_id, agent_id, title, status, cg_mitre, alert_ids, severity_id, grouping_key, window_start)
       VALUES ($1, $2, $3, $4, 'open', $5::jsonb, ARRAY[$6]::uuid[], $7, $8, to_timestamp($9))
       ON CONFLICT (grouping_key) WHERE status NOT IN ('resolved', 'false_positive') DO UPDATE
         SET alert_ids = CASE
                           WHEN incidents.alert_ids @> excluded.alert_ids THEN incidents.alert_ids
                           ELSE incidents.alert_ids || excluded.alert_ids
                         END,
             severity_id = GREATEST(incidents.severity_id, excluded.severity_id),
             -- The techniques accumulated; an incident without cg_mitre keeps none.
             cg_mitre = jsonb_set(
               incidents.cg_mitre,
               '{techniques}',
               COALESCE(incidents.cg_mitre -> 'techniques', '[]'::jsonb)
                 || COALESCE(
                      (SELECT jsonb_agg(t.value ORDER BY t.first)
                         FROM (SELECT e.value, min(e.ordinality) AS first
                                 FROM jsonb_array_elements(excluded.cg_mitre -> 'techniques')
                                        WITH ORDINALITY AS e(value, ordinality)
                                GROUP BY e.value) AS t
                        WHERE NOT COALESCE(incidents.cg_mitre -> 'techniques', '[]'::jsonb)
                                  @> jsonb_build_array(t.value)),
                      '[]'::jsonb)),
             updated_at = now()
       RETURNING incident_id, title, severity_id, (xmax = 0) AS inserted`,
      [
        uuidv7(),
        alert.orgId,
        alert.agentId,
        title,
        JSON.stringify(alert.cgMitre),
        alert.alertId,
        alert.severityId,
        groupingKey,
        windowStartSeconds,
      ],
    );
    const row = result.rows[0];
    if (row === undefined) {
      // An INSERT … ON CONFLICT DO UPDATE always returns exactly one row; this
      // guard is defensive and keeps the return type non-nullable.
      throw new Error("upsertIncident: no row returned from upsert");
    }
    return {
      created: row.inserted,
      incidentId: row.incident_id,
      title: row.title,
      severityId: row.severity_id,
      orgId: alert.orgId,
    };
  } finally {
    await pool.end();
  }
}
