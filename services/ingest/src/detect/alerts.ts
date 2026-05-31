import pg from "pg";
import { v7 as uuidv7 } from "uuid";
import type { DetectConfig, RuleMatch, SigmaRule } from "./types.js";

// SPEC-006 5e — alert assembly + persist. Builds a CGES Alert (class_uid 10001)
// from a 5c RuleMatch + the 5d final score and upserts it into the Postgres
// alerts table (ADR-0012 §6). Dedup is declarative: ON CONFLICT (dedup_key) DO
// NOTHING — a re-fire in the same (agent, rule, process, 5-min bucket) is a
// no-op and never resets a triaged status.

/** Dedup bucket width = the ADR-0012 §8 correlation window (5 minutes). */
const DEDUP_BUCKET_SECONDS = 300;

/**
 * dedup_key = `<agent_id>::<rule_id>::<process_name>::<bucket_5min>` (ADR-0012
 * §5). The bucket is derived from the EVENT's `time` (not `now()`), so events
 * are bucketed by when they occurred. Sub-second precision is irrelevant to a
 * 5-minute bucket, so the seconds-granularity parse is sufficient.
 */
/**
 * Event-occurrence time as integer Unix seconds (UTC), parsed from the ClickHouse
 * `time` string ("YYYY-MM-DD HH:MM:SS.fffffffff"). The shared basis for BOTH the
 * dedup bucket (below) and the alert's `event_time` column (SPEC-007 §Data contracts
 * §1; ADR-0013 §1 event-time windowing), so the two never diverge.
 */
function eventUnixSeconds(time: string): number {
  return Math.floor(Date.parse(`${time.slice(0, 10)}T${time.slice(11, 19)}Z`) / 1000);
}

export function buildDedupKey(match: RuleMatch): string {
  const bucket = Math.floor(eventUnixSeconds(match.sourceEvent.time) / DEDUP_BUCKET_SECONDS);
  return `${match.sourceEvent.agentId}::${match.ruleId}::${match.sourceEvent.processName}::${bucket}`;
}

/**
 * Persist an alert for a rule match. Returns true if a new row was inserted,
 * false if the dedup_key already existed (ON CONFLICT DO NOTHING). The CGES base
 * fields (category_uid 10, class_uid 10001, activity_id 1, cg_kind 'alert') and
 * status ('new') / timestamps come from the column defaults (migration 0002).
 */
export async function upsertAlert(
  config: DetectConfig,
  match: RuleMatch,
  rule: SigmaRule,
  finalScore: number,
): Promise<boolean> {
  const pool = new pg.Pool({ connectionString: config.ingest.INGEST_PG_URL });
  try {
    const result = await pool.query(
      `INSERT INTO alerts
         (alert_id, org_id, agent_id, title, severity_id, cg_detection_source, rule_id,
          source_events, heuristic_score, final_score, cg_mitre, dedup_key, event_time)
       VALUES ($1, $2, $3, $4, $5, 'rule', $6, $7::uuid[], $8, $9, $10::jsonb, $11, to_timestamp($12))
       ON CONFLICT (dedup_key) DO NOTHING`,
      [
        uuidv7(),
        config.orgId,
        match.sourceEvent.agentId,
        rule.title,
        match.severityId,
        match.ruleId,
        [match.sourceEvent.eventId],
        match.heuristicScore,
        finalScore,
        JSON.stringify(match.cgMitre),
        buildDedupKey(match),
        eventUnixSeconds(match.sourceEvent.time),
      ],
    );
    return result.rowCount === 1;
  } finally {
    await pool.end();
  }
}
