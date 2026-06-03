/** Marks code paths whose logic is intentionally absent at the harness-first RED
 *  gate (SPEC-009 PART 2/2). The data-access stubs throw this so a failing test is
 *  unambiguously "logic not implemented yet", never a broken harness. */
export class NotImplementedError extends Error {
  constructor(what: string) {
    super(`not implemented: ${what}`);
    this.name = "NotImplementedError";
  }
}
