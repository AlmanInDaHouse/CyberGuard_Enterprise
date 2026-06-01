/**
 * Thrown by every auth-core stub during the SPEC-008 harness-first RED gate.
 * The real logic (login, session, RBAC, rate-limit, CSRF, audit, user
 * management) lands at the GREEN gate; until then each entry point throws this,
 * so every auth AC fails RED because the CONTROL is absent — never because the
 * scaffold or backends are broken. The app's error handler maps it to HTTP 501.
 */
export class NotImplementedError extends Error {
  constructor(what: string) {
    super(`not implemented: ${what} (SPEC-008 auth-core; logic lands at the GREEN gate)`);
    this.name = "NotImplementedError";
  }
}
