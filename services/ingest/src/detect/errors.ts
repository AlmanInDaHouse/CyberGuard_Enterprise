/**
 * Thrown by the SPEC-006 detection-slice stubs during the harness-first RED
 * phase. The typed API exists (so `tsc` + `biome` stay green and the vitest
 * detect_ac_* tests RUN and fail visibly at the test step); the detection
 * LOGIC lands in the Phase-5 implementation, at which point these throws are
 * replaced and the tests go GREEN. Mirrors the Rust `todo!()` skeleton used
 * for the SPEC-005 harness-first RED.
 */
export class NotImplementedError extends Error {
  constructor(symbol: string) {
    super(`SPEC-006 NotImplemented: ${symbol} — detection logic lands in the Phase-5 impl.`);
    this.name = "NotImplementedError";
  }
}
