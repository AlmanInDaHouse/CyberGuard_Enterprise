import type { FastifyRequest } from "fastify";
import { AuthError, ForbiddenError } from "../errors.js";
import type { Services } from "../services.js";
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

export function makeRequireRole(role: string) {
  return async (req: FastifyRequest): Promise<void> => {
    if (req.authSession?.role !== role) {
      throw new ForbiddenError("insufficient role");
    }
  };
}

export function makeRequireCsrf() {
  return async (req: FastifyRequest): Promise<void> => {
    const header = req.headers["x-csrf-token"];
    if (!req.authSession || typeof header !== "string" || header !== req.authSession.csrf_token) {
      throw new ForbiddenError("missing or invalid CSRF token");
    }
  };
}
