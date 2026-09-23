/**
 * Typed errors for the detection slice.
 *
 * - `NotImplementedError` — thrown by the SPEC-006 detection-slice stubs during
 *   the harness-first RED phase: the typed API existed so `tsc` + `biome` stayed
 *   green and the `detect_ac_*` tests ran and failed visibly, before the Phase-5
 *   implementation replaced the throws. Retained for historical parity.
 * - `UnsupportedRuleError` — thrown when a Sigma rule, or a rule's `condition`,
 *   uses a construct outside the evaluator's supported subset. The loader fails
 *   closed and names the offending construct: a security evaluator that silently
 *   accepts a rule it cannot evaluate is worse than one that does not detect.
 */
export class NotImplementedError extends Error {
  constructor(symbol: string) {
    super(`SPEC-006 NotImplemented: ${symbol} — detection logic lands in the Phase-5 impl.`);
    this.name = "NotImplementedError";
  }
}

/** Thrown when a rule (or its `condition`) uses a construct outside the evaluator subset. */
export class UnsupportedRuleError extends Error {
  constructor(detail: string) {
    super(
      `unsupported Sigma construct — outside the SPEC-015 §Scope evaluator subset (fields Image/ParentImage; modifiers exact/endswith/startswith/contains; a boolean condition over and/or/not/parentheses; logsource.category process_creation; the MVP rule is SPEC-006): ${detail}`,
    );
    this.name = "UnsupportedRuleError";
  }
}
