import cookie from "@fastify/cookie";
import Fastify, { type FastifyInstance, type FastifyServerOptions } from "fastify";
import { registerAuthRoutes } from "./auth/routes.js";
import { AuthError, ForbiddenError, NotImplementedError, RateLimitError } from "./errors.js";
import type { Services } from "./services.js";

type LoggerOpt = FastifyServerOptions["logger"];

/**
 * Build the human-facing API (SPEC-008). User-facing only — NO agent mTLS (that
 * is services/ingest's boundary, ADR-0014 §3). `@fastify/cookie` backs the opaque
 * session cookie. The error handler maps the auth error taxonomy to status codes:
 * AuthError→401, RateLimitError→429, ForbiddenError→403 (RBAC/CSRF).
 */
export function buildApp(services: Services, logger: LoggerOpt = false): FastifyInstance {
  const app = Fastify({ logger });
  app.register(cookie);

  app.setErrorHandler((err, _req, reply) => {
    if (err instanceof AuthError) return reply.code(401).send({ error: "unauthorized" });
    if (err instanceof RateLimitError) return reply.code(429).send({ error: "rate_limited" });
    if (err instanceof ForbiddenError) return reply.code(403).send({ error: "forbidden" });
    if (err instanceof NotImplementedError) {
      return reply.code(501).send({ error: "not_implemented" });
    }
    const statusCode = (err as { statusCode?: number }).statusCode;
    return reply
      .code(typeof statusCode === "number" ? statusCode : 500)
      .send({ error: "internal" });
  });

  registerAuthRoutes(app, services);
  return app;
}
