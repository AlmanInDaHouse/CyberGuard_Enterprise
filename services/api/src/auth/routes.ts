import type { FastifyInstance, FastifyRequest } from "fastify";
import { AuthError } from "../errors.js";
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
  EmptyBody,
  InvitationCompleteBody,
  InvitationTotpBody,
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

  // SPEC-026 §Data contracts §3: each route of its tables answers `Cache-Control: no-store`,
  // its refusals included (the app's onSend hook).
  const noStore = { noStore: true };

  app.post("/v1/auth/login", { config: noStore }, async (req, reply) => {
    const body = parseBody(LoginBody, req.body);
    const result = await services.auth.login(body, ctxOf(req), req.cookies?.cgsess);
    reply.setCookie("cgsess", result.token, {
      httpOnly: true,
      secure: true,
      sameSite: "strict",
      path: "/",
    });
    return reply.code(200).send({ csrf_token: result.csrfToken });
  });

  // SPEC-026 §Data contracts §3: the session's user, for the dashboard's pages.
  app.get("/v1/auth/session", { config: noStore, onRequest: [requireSession] }, async (req) => {
    const session = req.authSession;
    if (!session) throw new Error("unreachable: requireSession populates authSession");
    const user = await services.auth.sessionUser(session);
    if (!user) throw new AuthError("no session");
    return user;
  });

  app.post("/v1/auth/logout", { onRequest: [requireSession, requireCsrf] }, async (req, reply) => {
    await services.auth.logout(req.cookies?.cgsess ?? "", ctxOf(req));
    reply.clearCookie("cgsess", { path: "/" });
    return { ok: true };
  });

  app.post("/v1/auth/totp/confirm", { config: noStore }, async (req) => {
    await services.auth.confirmTotpEnrollment(parseBody(TotpConfirmBody, req.body), ctxOf(req));
    return { ok: true };
  });

  app.post(
    "/v1/auth/password",
    { config: noStore, onRequest: [requireSession, requireCsrf] },
    async (req) => {
      const session = req.authSession;
      if (!session) throw new Error("unreachable: requireSession populates authSession");
      const body = parseBody(PasswordBody, req.body);
      await services.auth.changePassword({ userId: session.user_id, ...body }, ctxOf(req));
      return { ok: true };
    },
  );

  // SPEC-026 §Operational §3: the invitation's two routes take no session, and so no role or
  // CSRF check; the invitation in the body is the credential, limited per invitation and per
  // source address.
  app.post("/v1/invitations/totp", { config: noStore }, async (req) =>
    services.auth.invitationTotp(parseBody(InvitationTotpBody, req.body), ctxOf(req)),
  );

  app.post("/v1/invitations/complete", { config: noStore }, async (req, reply) => {
    await services.auth.completeInvitation(parseBody(InvitationCompleteBody, req.body), ctxOf(req));
    return reply.code(204).send();
  });

  // --- admin-only user management ---
  app.post(
    "/v1/users",
    { config: noStore, onRequest: [requireSession, requireAdmin, requireCsrf] },
    async (req, reply) => {
      const created = await services.auth.createUser(
        parseBody(CreateUserBody, req.body),
        ctxOf(req),
      );
      return reply.code(201).send(created);
    },
  );

  app.get(
    "/v1/users",
    { config: noStore, onRequest: [requireSession, requireAdmin] },
    async (req) => services.auth.listUsers(ctxOf(req)),
  );

  app.post(
    "/v1/users/:id/role",
    { config: noStore, onRequest: [requireSession, requireAdmin, requireCsrf, requireUserId] },
    async (req) => {
      const body = parseBody(RoleBody, req.body);
      await services.auth.assignRole((req.params as { id: string }).id, body.role, ctxOf(req));
      return { ok: true };
    },
  );

  // SPEC-008 Amendment 2026-10-15 item 8: disable or enable a user (§2's admin revoke).
  app.post(
    "/v1/users/:id/status",
    { config: noStore, onRequest: [requireSession, requireAdmin, requireCsrf, requireUserId] },
    async (req) => {
      const body = parseBody(StatusBody, req.body);
      await services.auth.setStatus((req.params as { id: string }).id, body.status, ctxOf(req));
      return { ok: true };
    },
  );

  // SPEC-026 §Data contracts §3: a new invitation for a user with no credentials, and the
  // withdrawal of the open one.
  app.post(
    "/v1/users/:id/invitation",
    { config: noStore, onRequest: [requireSession, requireAdmin, requireCsrf, requireUserId] },
    async (req, reply) => {
      parseBody(EmptyBody, req.body);
      const issued = await services.auth.issueInvitation(
        (req.params as { id: string }).id,
        ctxOf(req),
      );
      return reply.code(201).send(issued);
    },
  );

  // SPEC-026 §Operational §4: an admin's reset of another user's credentials, by an invitation.
  app.post(
    "/v1/users/:id/reset",
    { config: noStore, onRequest: [requireSession, requireAdmin, requireCsrf, requireUserId] },
    async (req, reply) => {
      parseBody(EmptyBody, req.body);
      const issued = await services.auth.resetUser((req.params as { id: string }).id, ctxOf(req));
      return reply.code(201).send(issued);
    },
  );

  app.post(
    "/v1/users/:id/invitation/withdraw",
    { config: noStore, onRequest: [requireSession, requireAdmin, requireCsrf, requireUserId] },
    async (req, reply) => {
      parseBody(EmptyBody, req.body);
      await services.auth.withdrawInvitation((req.params as { id: string }).id, ctxOf(req));
      return reply.code(204).send();
    },
  );
}
