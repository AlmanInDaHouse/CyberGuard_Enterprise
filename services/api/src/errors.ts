/** Mapped to HTTP 501 by the app error handler (legacy RED-gate stub marker). */
export class NotImplementedError extends Error {
  constructor(what: string) {
    super(`not implemented: ${what}`);
    this.name = "NotImplementedError";
  }
}

/** Authentication failed — mapped to 401. Generic by design (no oracle, NFR-008-003). */
export class AuthError extends Error {
  constructor(message = "authentication failed") {
    super(message);
    this.name = "AuthError";
  }
}

/** Too many attempts — mapped to 429 (rate-limit, SPEC-008 §Operational §3). */
export class RateLimitError extends Error {
  constructor(message = "too many attempts") {
    super(message);
    this.name = "RateLimitError";
  }
}

/** Authorization / CSRF denied — mapped to 403 (RBAC §5, CSRF §4). A refusal of a known
 *  principal carries its reason, which the `access.denied` row records (SPEC-008
 *  Amendment 2026-10-16 item 1). */
export class ForbiddenError extends Error {
  constructor(
    message = "forbidden",
    readonly reason?: "role" | "csrf",
  ) {
    super(message);
    this.name = "ForbiddenError";
  }
}

/** A malformed request parameter — mapped to 400 (SPEC-009 Amendment 2026-10-14 §2). */
export class BadRequestError extends Error {
  constructor(message = "bad request") {
    super(message);
    this.name = "BadRequestError";
  }
}

/** A store that could not answer — mapped to 503 (SPEC-009 Amendment 2026-10-14 §3).
 *  The store client's error is the `cause`, logged and never sent. */
export class StoreUnavailableError extends Error {
  constructor(cause: unknown) {
    super("store unavailable", { cause });
    this.name = "StoreUnavailableError";
  }
}

/** An object that does not exist for the caller — mapped to 404 (SPEC-008 Amendment
 *  2026-10-15 item 5). */
export class NotFoundError extends Error {
  constructor(message = "not found") {
    super(message);
    this.name = "NotFoundError";
  }
}

/** A change the current state refuses — mapped to 409 (SPEC-008 Amendment 2026-10-15
 *  items 5 and 10). */
export class ConflictError extends Error {
  constructor(message = "conflict") {
    super(message);
    this.name = "ConflictError";
  }
}
