/**
 * SPEC-025 §Operational §5 — the rate limits of enrollment and renewal, in the process's
 * memory: constants, no setting, reset at a restart, and held per instance behind several.
 * ADR-0003's table puts rate limits in Redis; SPEC-025 departs from it under ADR-0003
 * §Compliance, since ingest's Valkey user is limited to its anti-replay nonces (ADR-0022 §4).
 */

/** Failed enrollments from one source address that refuse its next ones. */
export const ENROLL_FAILURE_LIMIT = 10;
/** The window the failures are counted in, and how long an address stays refused. */
export const ENROLL_WINDOW_MS = 15 * 60 * 1000;
/** One certificate per agent per this interval. */
export const RENEWAL_INTERVAL_MS = 10 * 60 * 1000;
/** Past this many entries, a map drops the ones whose time has passed. */
const SWEEP_ABOVE = 10_000;

/**
 * The enrollment limit: once ten enrollments from one address have failed within 15
 * minutes, every enrollment from it is refused until 15 minutes after the tenth failure.
 */
export class EnrollmentLimiter {
  private readonly failures = new Map<string, number[]>();
  private readonly refusedUntil = new Map<string, number>();

  constructor(private readonly now: () => number = Date.now) {}

  /** Whether an enrollment from `address` is refused now, before its token is read. */
  refuses(address: string): boolean {
    const until = this.refusedUntil.get(address);
    if (until === undefined) return false;
    if (this.now() < until) return true;
    this.refusedUntil.delete(address);
    return false;
  }

  /** An enrollment from `address` that §1 refused for its token. */
  recordFailure(address: string): void {
    const t = this.now();
    const recent = (this.failures.get(address) ?? []).filter((at) => t - at < ENROLL_WINDOW_MS);
    recent.push(t);
    if (recent.length >= ENROLL_FAILURE_LIMIT) {
      this.failures.delete(address);
      this.refusedUntil.set(address, t + ENROLL_WINDOW_MS);
    } else {
      this.failures.set(address, recent);
    }
    this.sweep(t);
  }

  private sweep(t: number): void {
    if (this.failures.size + this.refusedUntil.size <= SWEEP_ABOVE) return;
    for (const [address, times] of this.failures) {
      if (times.every((at) => t - at >= ENROLL_WINDOW_MS)) this.failures.delete(address);
    }
    for (const [address, until] of this.refusedUntil) {
      if (t >= until) this.refusedUntil.delete(address);
    }
  }
}

/** The renewal limit: one certificate per agent per 10 minutes. */
export class RenewalLimiter {
  private readonly signedAt = new Map<string, number>();

  constructor(private readonly now: () => number = Date.now) {}

  /** Takes the agent's issuance if none was taken in the interval; false when one was. */
  take(agentId: string): boolean {
    const t = this.now();
    const last = this.signedAt.get(agentId);
    if (last !== undefined && t - last < RENEWAL_INTERVAL_MS) return false;
    this.signedAt.set(agentId, t);
    if (this.signedAt.size > SWEEP_ABOVE) {
      for (const [id, at] of this.signedAt) {
        if (t - at >= RENEWAL_INTERVAL_MS) this.signedAt.delete(id);
      }
    }
    return true;
  }
}
