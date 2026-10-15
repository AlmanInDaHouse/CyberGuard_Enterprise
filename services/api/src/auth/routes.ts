import type { FastifyInstance, FastifyRequest } from "fastify";
import type { Services } from "../services.js";
import { requestContext } from "./context.js";
import {
  makeRequireCsrf,
  makeRequireIdParam,
  makeRequireRole,
  makeRequireSession,
} from "./prehandlers.js";
import {
  CreateUserBody,
  LoginBody,
  PasswordBody,
  RoleBody,
  StatusBody,
  TotpConfirmBody,
  parseBody,
} from "./schemas.js";
import type { RequestContext } from "./service.js";

/**
 * SPEC-008 §Operational — the auth-core HTTP surface. Session lifecycle is open
 * (login) or session-guarded (logout); admin user-management is guarded by
 * requireSession + requireRole('admin') + requireCsrf; mutations carry CSRF.
 * TOTP first-enrollment confirmation is proof-of-possession (a valid code), so it
 * needs no prior session (§Operational §8).
 *
 * SPEC-008 Amendment 2026-10-15 items 1 and 2: the session, role and CSRF checks and
 * the `:id` check are `onRequest` hooks, which run before Fastify reads the body, and
 * every body is parsed by its strict schema before any store is read.
 */
export function registerAuthRoutes(app: FastifyInstance, services: Services): void {
  const requireSession = makeRequireSession(services);
  const requireAdmin = makeRequireRole("admin");
  const requireCsrf = makeRequireCsrf();
  const requireUserId = makeRequireIdParam();

  /** The request's id, source address and session, for the limits and the audit rows
   *  (the first Amendment 2026-10-15, item 6; the second, *the source address behind a
   *  proxy*, items 2 and 3). */
  const ctxOf = (req: FastifyRequest): RequestContext =>
    requestContext(req, services.trustedProxies);

  app.post("/v1/auth/login", async (req, reply) => {
    const body = parseBody(LoginBody, req.body);
    const result = await services.auth.login(body, ctxOf(req));
    reply.setCookie("cgsess", result.token, {
      httpOnly: true,
      secure: true,
      sameSite: "strict",
      path: "/",
    });
    return reply.code(200).send({ csrf_token: result.csrfToken });
  });

  app.post("/v1/auth/logout", { onRequest: [requireSession, requireCsrf] }, async (req, reply) => {
    await services.auth.logout(req.cookies?.cgsess ?? "", ctxOf(req));
    reply.clearCookie("cgsess", { path: "/" });
    return { ok: true };
  });

  app.post("/v1/auth/totp/confirm", async (req) => {
    await services.auth.confirmTotpEnrollment(parseBody(TotpConfirmBody, req.body), ctxOf(req));
    return { ok: true };
  });

  app.post("/v1/auth/password", { onRequest: [requireSession, requireCsrf] }, async (req) => {
    const session = req.authSession;
    if (!session) throw new Error("unreachable: requireSession populates authSession");
    const body = parseBody(PasswordBody, req.body);
    await services.auth.changePassword({ userId: session.user_id, ...body }, ctxOf(req));
    return { ok: true };
  });

  // --- admin-only user management ---
  app.post(
    "/v1/users",
    { onRequest: [requireSession, requireAdmin, requireCsrf] },
    async (req, reply) => {
      const created = await services.auth.createUser(
        parseBody(CreateUserBody, req.body),
        ctxOf(req),
      );
      return reply.code(201).send(created);
    },
  );

  app.get("/v1/users", { onRequest: [requireSession, requireAdmin] }, async (req) =>
    services.auth.listUsers(ctxOf(req)),
  );

  app.post(
    "/v1/users/:id/role",
    { onRequest: [requireSession, requireAdmin, requireCsrf, requireUserId] },
    async (req) => {
      const body = parseBody(RoleBody, req.body);
      await services.auth.assignRole((req.params as { id: string }).id, body.role, ctxOf(req));
      return { ok: true };
    },
  );

  // SPEC-008 Amendment 2026-10-15 item 8: disable or enable a user (§2's admin revoke).
  app.post(
    "/v1/users/:id/status",
    { onRequest: [requireSession, requireAdmin, requireCsrf, requireUserId] },
    async (req) => {
      const body = parseBody(StatusBody, req.body);
      await services.auth.setStatus((req.params as { id: string }).id, body.status, ctxOf(req));
      return { ok: true };
    },
  );
}
