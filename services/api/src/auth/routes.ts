import type { FastifyInstance } from "fastify";
import type { Services } from "../services.js";
import type { CreateUserInput, LoginInput } from "./service.js";

/**
 * SPEC-008 §Operational — the auth-core HTTP surface. During the RED gate every
 * handler delegates to the AuthService stub, which throws NotImplementedError →
 * the app error handler maps it to 501. The GREEN gate fills the service; the
 * route shapes (and, at GREEN, the RBAC preHandler + Zod body schemas + CSRF
 * check) land then. The routes exist so each AC's RED is "control absent on a
 * present endpoint", not a 404.
 */
export function registerAuthRoutes(app: FastifyInstance, services: Services): void {
  // --- session lifecycle ---
  app.post("/v1/auth/login", async (req) => services.auth.login(req.body as LoginInput));
  app.post("/v1/auth/logout", async () => services.auth.logout(""));
  app.post("/v1/auth/totp/confirm", async (req) =>
    services.auth.confirmTotpEnrollment(req.body as { userId: string; totp_code: string }),
  );
  app.post("/v1/auth/password", async (req) =>
    services.auth.changePassword(
      req.body as {
        userId: string;
        currentPassword: string;
        newPassword: string;
        totp_code: string;
      },
    ),
  );

  // --- admin-only user management (RBAC enforced at the GREEN gate) ---
  app.post("/v1/users", async (req) => services.auth.createUser(req.body as CreateUserInput));
  app.get("/v1/users", async () => services.auth.listUsers());
  app.post("/v1/users/:id/role", async (req) =>
    services.auth.assignRole(
      (req.params as { id: string }).id,
      (req.body as { role: string }).role,
    ),
  );
}
