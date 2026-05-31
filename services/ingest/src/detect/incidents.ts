import type { DetectConfig, IncidentGroupingInput } from "./types.js";

/**
 * Thrown by the SPEC-007 incident-grouping stub during the harness-first RED
 * phase. The typed seam exists (so `tsc` + `biome` stay green and the
 * `incident_ac_*` tests RUN and fail visibly at the `upsertIncident` step, NOT at
 * setup); the grouping LOGIC lands at the SPEC-007 impl gate, at which point this
 * throw is replaced and incident_ac_002–005 go GREEN. Mirrors the SPEC-006
 * `NotImplementedError` pattern (`./errors.ts`).
 */
export class IncidentGroupingNotImplementedError extends Error {
  constructor(symbol: string) {
    super(
      `SPEC-007 NotImplemented: ${symbol} — incident grouping logic lands in the SPEC-007 impl gate.`,
    );
    this.name = "IncidentGroupingNotImplementedError";
  }
}

/**
 * SPEC-007 §Operational §1/§4/§6 — group a newly-persisted alert into an incident.
 * Computes the `grouping_key` (`<org>::<agent>::<canonical_tactics>::<window_bucket>`,
 * §Data contracts §4, event-time windowed per ADR-0013 §1) and create-or-updates the
 * incident: `INSERT … ON CONFLICT (grouping_key) DO UPDATE` appends `alert_id` to
 * `alert_ids` and bumps `updated_at`, WITHOUT clobbering `status`/`assigned_to`
 * (the triage-preservation invariant). Called as a sibling step after a newly
 * inserted alert in `runDetectionCycle` (§Operational §6) — wiring lands with the
 * logic at the impl gate.
 *
 * Harness-first RED: NotImplemented until the SPEC-007 impl gate.
 */
export function upsertIncident(
  _config: DetectConfig,
  _alert: IncidentGroupingInput,
): Promise<void> {
  throw new IncidentGroupingNotImplementedError("upsertIncident");
}
