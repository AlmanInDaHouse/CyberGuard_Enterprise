import type { ScoreSignals } from "./types.js";

// ADR-0005 §Scoring composition default weights (rule_score maps to
// heuristicScore per ADR-0012 §4). w_rule + w_ueba + w_ml = 1.0. Per-org
// configurability is a later increment; the MVP uses the ADR-0005 defaults.
const WEIGHTS = { heuristic: 0.6, ueba: 0.25, ml: 0.15 } as const;

/**
 * Renormalized weighted-sum score per ADR-0012 §4:
 *
 *   final = Σ(w_i · s_i over present sources) / Σ(w_i over present sources)
 *
 * A source is "present" when its score is non-null. The denominator is the SUM
 * of the active weights (not a source count), so rule-only ⇒ (0.6·heuristic)/0.6
 * = heuristic. Forward-compatible: when ueba/ml scores arrive they enter both
 * numerator and denominator without a code change; with all three present the
 * denominator is 1.0 and the formula reduces to ADR-0005's literal weighted sum.
 *
 * The result is rounded to 3 decimals — the `numeric(4,3)` precision at which
 * `final_score` is stored (ADR-0012 §6): the renormalized value at storage
 * precision. This also makes the single-source identity exact, since IEEE-754
 * gives (0.6·0.9)/0.6 = 0.9000000000000001, not 0.9.
 *
 * Throws when no source is present (denominator 0): that violates the
 * `cg_detection_source` ≥1-score invariant and must fail loud rather than return
 * a silently wrong score.
 */
export function scoreAlert(signals: ScoreSignals): number {
  let numerator = 0;
  let denominator = 0;
  if (signals.heuristicScore != null) {
    numerator += WEIGHTS.heuristic * signals.heuristicScore;
    denominator += WEIGHTS.heuristic;
  }
  if (signals.uebaScore != null) {
    numerator += WEIGHTS.ueba * signals.uebaScore;
    denominator += WEIGHTS.ueba;
  }
  if (signals.mlScore != null) {
    numerator += WEIGHTS.ml * signals.mlScore;
    denominator += WEIGHTS.ml;
  }
  if (denominator === 0) {
    throw new Error(
      "scoreAlert: no scoring source present — violates the cg_detection_source >=1-score invariant",
    );
  }
  return Math.round((numerator / denominator) * 1000) / 1000;
}
