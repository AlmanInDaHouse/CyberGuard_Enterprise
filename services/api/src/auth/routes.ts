import type { FastifyInstance } from "fastify";
import type { Services } from "../services.js";
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
  TotpConfirmBody,
  parseBody,
} from "./schemas.js";

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

  app.post("/v1/auth/login", async (req, reply) => {
    const body = parseBody(LoginBody, req.body);
    const result = await services.auth.login(body, req.ip);
    reply.setCookie("cgsess", result.token, {
      httpOnly: true,
      secure: true,
      sameSite: "strict",
      path: "/",
    });
    return reply.code(200).send({ csrf_token: result.csrfToken });
  });

  app.post("/v1/auth/logout", { onRequest: [requireSession, requireCsrf] }, async (req, reply) => {
    await services.auth.logout(req.cookies?.cgsess ?? "");
    reply.clearCookie("cgsess", { path: "/" });
    return { ok: true };
  });

  app.post("/v1/auth/totp/confirm", async (req) => {
    await services.auth.confirmTotpEnrollment(parseBody(TotpConfirmBody, req.body));
    return { ok: true };
  });

  app.post("/v1/auth/password", { onRequest: [requireSession, requireCsrf] }, async (req) => {
    const session = req.authSession;
    if (!session) throw new Error("unreachable: requireSession populates authSession");
    const body = parseBody(PasswordBody, req.body);
    await services.auth.changePassword({ userId: session.user_id, ...body });
    return { ok: true };
  });

  // --- admin-only user management ---
  app.post(
    "/v1/users",
    { onRequest: [requireSession, requireAdmin, requireCsrf] },
    async (req, reply) => {
      const created = await services.auth.createUser(parseBody(CreateUserBody, req.body));
      return reply.code(201).send(created);
    },
  );

  app.get("/v1/users", { onRequest: [requireSession, requireAdmin] }, async () =>
    services.auth.listUsers(),
  );

  app.post(
    "/v1/users/:id/role",
    { onRequest: [requireSession, requireAdmin, requireCsrf, requireUserId] },
    async (req) => {
      const body = parseBody(RoleBody, req.body);
      await services.auth.assignRole((req.params as { id: string }).id, body.role);
      return { ok: true };
    },
  );
}
