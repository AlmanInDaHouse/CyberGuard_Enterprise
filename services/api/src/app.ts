import cookie from "@fastify/cookie";
import Fastify, { type FastifyInstance, type FastifyServerOptions } from "fastify";
import { registerAuthRoutes } from "./auth/routes.js";
import {
  AuthError,
  BadRequestError,
  ForbiddenError,
  NotImplementedError,
  RateLimitError,
  StoreUnavailableError,
} from "./errors.js";
import { registerReadRoutes } from "./read/routes.js";
import type { Services } from "./services.js";

type LoggerOpt = FastifyServerOptions["logger"];

/** What a log line may say about an error: its type, code, message and stack, never the
 *  rest of the object. A Postgres error's `detail` can quote a whole row, and a `users`
 *  row holds a password hash and an encrypted TOTP secret (SPEC-009 Amendment
 *  2026-10-14 §4). Logged under `error`, not `err`: pino's `err` serializer would
 *  replace `type` with this object's own constructor name. */
function loggable(err: unknown): Record<string, unknown> {
  if (!(err instanceof Error)) return { type: typeof err };
  const { code } = err as { code?: unknown };
  return { type: err.name, code, message: err.message, stack: err.stack };
}

/**
 * Build the human-facing API (SPEC-008). User-facing only — NO agent mTLS (that
 * is services/ingest's boundary, ADR-0014 §3). `@fastify/cookie` backs the opaque
 * session cookie. The error handler maps the error taxonomy to status codes:
 * AuthError→401, RateLimitError→429, ForbiddenError→403 (RBAC/CSRF),
 * BadRequestError→400 and StoreUnavailableError→503 (SPEC-009 Amendment
 * 2026-10-14). A 5xx is logged with its cause; no body carries internal detail.
 */
export function buildApp(services: Services, logger: LoggerOpt = false): FastifyInstance {
  const app = Fastify({ logger });
  app.register(cookie);

  app.setErrorHandler((err, req, reply) => {
    if (err instanceof AuthError) return reply.code(401).send({ error: "unauthorized" });
    if (err instanceof RateLimitError) return reply.code(429).send({ error: "rate_limited" });
    if (err instanceof ForbiddenError) return reply.code(403).send({ error: "forbidden" });
    if (err instanceof BadRequestError) return reply.code(400).send({ error: "bad_request" });
    if (err instanceof NotImplementedError) {
      return reply.code(501).send({ error: "not_implemented" });
    }
    if (err instanceof StoreUnavailableError) {
      req.log.error({ error: loggable(err.cause) }, "store unavailable");
      return reply.code(503).send({ error: "unavailable" });
    }
    // An error Fastify raises with its own status keeps it; anything else is a 500.
    const statusCode = (err as { statusCode?: number }).statusCode;
    const code = typeof statusCode === "number" ? statusCode : 500;
    if (code >= 500) req.log.error({ error: loggable(err) }, "request failed");
    return reply.code(code).send({ error: "internal" });
  });

  registerAuthRoutes(app, services);
  registerReadRoutes(app, services);
  return app;
}
