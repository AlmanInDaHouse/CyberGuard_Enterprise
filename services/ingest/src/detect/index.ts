import { upsertAlert } from "./alerts.js";
import { evaluateRule, loadRules } from "./engine.js";
import { upsertIncident } from "./incidents.js";
import { advanceWatermark, getWatermark, readNewEvents } from "./read-model.js";
import { scoreAlert } from "./scorer.js";
import type { DetectConfig, DetectCycleResult } from "./types.js";

/** Per-cycle read cap (SPEC-006 NFR-006-002). */
const BATCH_LIMIT = 1000;

/**
 * Run one detection cycle (SPEC-006 §Operational; ADR-0012 §7): poll
 * `cges_events` forward by the per-org watermark (FINAL), resolve each event's
 * parent image, evaluate the loaded rules, score each match (renormalized), and
 * upsert the resulting alerts into Postgres (ON CONFLICT DO NOTHING), and group
 * each newly-persisted alert into an incident (SPEC-007 §Operational §6). The
 * watermark advances to the batch's max `time` after processing.
 *
 * Loop structure (a DECISION, not a side effect of ordering): every event is
 * evaluated against EVERY rule — each rule is an independent detector (ADR-0005),
 * so an event matching N rules yields N alerts (one per (rule, event) match),
 * collapsed by dedup_key (which includes rule_id). No early-exit on first match.
 */
export async function runDetectionCycle(config: DetectConfig): Promise<DetectCycleResult> {
  const rules = loadRules(config.rulesDir);
  const watermark = await getWatermark(config);
  const events = await readNewEvents(config, watermark, BATCH_LIMIT);
  if (events.length === 0) {
    return { processedThrough: null, eventsEvaluated: 0, alertsWritten: 0 };
  }

  let alertsWritten = 0;
  for (const event of events) {
    for (const rule of rules) {
      const match = evaluateRule(rule, event);
      if (match === null) continue;
      const finalScore = scoreAlert({ heuristicScore: match.heuristicScore });
      const alertId = await upsertAlert(config, match, rule, finalScore);
      if (alertId !== null) {
        alertsWritten += 1;
        // SPEC-007 §Operational §6 — sibling step: group the NEWLY persisted alert
        // into an incident. Rides the new-alert insert (a dedup-collapsed re-fire
        // returns null and was grouped at its first insert). The alert exists before
        // the incident references its alert_id (order-causal).
        await upsertIncident(config, {
          alertId,
          orgId: config.orgId,
          agentId: match.sourceEvent.agentId,
          cgMitre: match.cgMitre,
          eventTime: match.sourceEvent.time,
          // SPEC-011 §Operational §2 — the alert's OCSF severity feeds the incident's
          // MAX aggregation. `match.severityId` is the same always-present value
          // upsertAlert persisted two lines up (NOT NULL, range-checked).
          severityId: match.severityId,
        });
      }
    }
  }

  const processedThrough = events.map((e) => e.time).reduce((a, b) => (a > b ? a : b));
  await advanceWatermark(config, processedThrough);
  return { processedThrough, eventsEvaluated: events.length, alertsWritten };
}
