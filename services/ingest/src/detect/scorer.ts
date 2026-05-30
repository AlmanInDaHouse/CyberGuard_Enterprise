import { NotImplementedError } from "./errors.js";
import type { ScoreSignals } from "./types.js";

/**
 * Renormalized weighted-sum score per ADR-0012 §4 + SPEC-006 §Operational §3:
 * `final = Σ(w_i · s_i present) / Σ(w_i present)` over the configured weights
 * (w_rule=0.6, w_ueba=0.25, w_ml=0.15). Rule-only ⇒ `final = heuristicScore`.
 *
 * Harness-first RED stub; logic lands in the Phase-5 impl (5d).
 */
export function scoreAlert(signals: ScoreSignals): number {
  const present = Object.keys(signals).join(",") || "<empty>";
  throw new NotImplementedError(`scoreAlert(${present})`);
}
