import type { FastifyInstance } from "fastify";
import type { Services } from "../services.js";
import { makeRequireCsrf, makeRequireRole, makeRequireSession } from "./prehandlers.js";
import type { CreateUserInput, LoginInput } from "./service.js";

/**
 * SPEC-008 §Operational — the auth-core HTTP surface. Session lifecycle is open
 * (login) or session-guarded (logout); admin user-management is guarded by
 * requireSession + requireRole('admin') + requireCsrf; mutations carry CSRF.
 * TOTP first-enrollment confirmation is proof-of-possession (a valid code), so it
 * needs no prior session (§Operational §8).
 */
export function registerAuthRoutes(app: FastifyInstance, services: Services): void {
  const requireSession = makeRequireSession(services);
  const requireAdmin = makeRequireRole("admin");
  const requireCsrf = makeRequireCsrf();

  app.post("/v1/auth/login", async (req, reply) => {
    const result = await services.auth.login(req.body as LoginInput, req.ip);
    reply.setCookie("cgsess", result.token, {
      httpOnly: true,
      secure: true,
      sameSite: "strict",
      path: "/",
    });
    return reply.code(200).send({ csrf_token: result.csrfToken });
  });

  app.post("/v1/auth/logout", { preHandler: requireSession }, async (req, reply) => {
    await services.auth.logout(req.cookies?.cgsess ?? "");
    reply.clearCookie("cgsess", { path: "/" });
    return { ok: true };
  });

  app.post("/v1/auth/totp/confirm", async (req) => {
    await services.auth.confirmTotpEnrollment(req.body as { userId: string; totp_code: string });
    return { ok: true };
  });

  app.post("/v1/auth/password", { preHandler: [requireSession, requireCsrf] }, async (req) => {
    const session = req.authSession;
    if (!session) throw new Error("unreachable: requireSession populates authSession");
    const body = req.body as { currentPassword: string; newPassword: string; totp_code: string };
    await services.auth.changePassword({ userId: session.user_id, ...body });
    return { ok: true };
  });

  // --- admin-only user management ---
  app.post(
    "/v1/users",
    { preHandler: [requireSession, requireAdmin, requireCsrf] },
    async (req, reply) => {
      const created = await services.auth.createUser(req.body as CreateUserInput);
      return reply.code(201).send(created);
    },
  );

  app.get("/v1/users", { preHandler: [requireSession, requireAdmin] }, async () =>
    services.auth.listUsers(),
  );

  app.post(
    "/v1/users/:id/role",
    { preHandler: [requireSession, requireAdmin, requireCsrf] },
    async (req) => {
      await services.auth.assignRole(
        (req.params as { id: string }).id,
        (req.body as { role: string }).role,
      );
      return { ok: true };
    },
  );
}
