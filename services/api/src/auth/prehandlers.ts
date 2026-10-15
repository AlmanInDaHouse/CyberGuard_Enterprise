import type { FastifyRequest } from "fastify";
import { AuthError, ForbiddenError, NotFoundError } from "../errors.js";
import type { Services } from "../services.js";
import { canonicalId } from "./schemas.js";
import type { SessionInfo } from "./session.js";

declare module "fastify" {
  interface FastifyRequest {
    authSession?: SessionInfo;
  }
}

/**
 * SPEC-008 §Operational §4/§5 — the server-side auth preHandlers. Role and CSRF
 * authority come from the session entry (Redis), never the request body. RBAC and
 * CSRF deny with 403; an absent/revoked session denies with 401 (the immediate
 * revocation invariant, auth_ac_005).
 */
export function makeRequireSession(services: Services) {
  return async (req: FastifyRequest): Promise<void> => {
    const token = req.cookies?.cgsess ?? "";
    const sess = await services.auth.resolveSession(token);
    if (!sess) throw new AuthError("no session");
    req.authSession = sess;
  };
}

/** The session's role is one of `roles`, or a 403 (SPEC-009's third Amendment
 *  2026-10-15 gates the forensic routes on two). */
export function makeRequireRole(...roles: string[]) {
  return async (req: FastifyRequest): Promise<void> => {
    if (!roles.includes(req.authSession?.role ?? "")) {
      throw new ForbiddenError("insufficient role", "role");
    }
  };
}

export function makeRequireCsrf() {
  return async (req: FastifyRequest): Promise<void> => {
    const header = req.headers["x-csrf-token"];
    if (!req.authSession || typeof header !== "string" || header !== req.authSession.csrf_token) {
      throw new ForbiddenError("missing or invalid CSRF token", "csrf");
    }
  };
}

/** The `:id` of a user route: a canonical UUID, read in lower case, or a 404 before the
 *  body is read (SPEC-008 Amendment 2026-10-15 items 1 and 5). */
export function makeRequireIdParam() {
  return async (req: FastifyRequest): Promise<void> => {
    const params = req.params as { id: string };
    const id = canonicalId(params.id);
    if (id === null) throw new NotFoundError();
    params.id = id;
  };
}
