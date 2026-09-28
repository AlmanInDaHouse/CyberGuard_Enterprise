/**
 * Typed errors for the detection slice.
 *
 * - `NotImplementedError` — thrown by the SPEC-006 detection-slice stubs during
 *   the harness-first RED phase: the typed API existed so `tsc` + `biome` stayed
 *   green and the `detect_ac_*` tests ran and failed visibly, before the Phase-5
 *   implementation replaced the throws. Retained for historical parity.
 * - `UnsupportedRuleError` — thrown when a rule, or a rule's `condition`, falls
 *   outside the evaluator's supported subset (SPEC-015) or the loader contract
 *   (SPEC-016 §Data contracts §1). The loader fails closed and names the
 *   offending construct: a security evaluator that silently accepts a rule it
 *   cannot evaluate is worse than one that does not detect.
 */
export class NotImplementedError extends Error {
  constructor(symbol: string) {
    super(`SPEC-006 NotImplemented: ${symbol} — detection logic lands in the Phase-5 impl.`);
    this.name = "NotImplementedError";
  }
}

/** Thrown when a rule (or its `condition`) is outside the evaluator subset or the loader contract. */
export class UnsupportedRuleError extends Error {
  /** The construct-specific part of the message, without the contract summary. */
  readonly detail: string;

  constructor(detail: string) {
    super(
      `unsupported rule — outside the SPEC-015 evaluator subset or the SPEC-016 loader contract (fields Image/ParentImage; modifiers exact match (no modifier), endswith, startswith, contains; a boolean condition over and/or/not/parentheses that needs at least one block to match; logsource windows / process_creation; a rule.<file name> id; the OCSF severity of its level; cg.cg_detection_source "rule"; ATT&CK tactic names and technique ids): ${detail}`,
    );
    this.name = "UnsupportedRuleError";
    this.detail = detail;
  }
}
