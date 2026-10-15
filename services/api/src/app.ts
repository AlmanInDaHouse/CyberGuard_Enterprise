import cookie from "@fastify/cookie";
import Fastify, { type FastifyInstance, type FastifyServerOptions } from "fastify";
import { appendAudit } from "./auth/audit.js";
import { requestContext } from "./auth/context.js";
import { registerAuthRoutes } from "./auth/routes.js";
import {
  AuthError,
  BadRequestError,
  ConflictError,
  ForbiddenError,
  NotFoundError,
  NotImplementedError,
  RateLimitError,
  StoreUnavailableError,
} from "./errors.js";
import { registerReadRoutes } from "./read/routes.js";
import type { Services } from "./services.js";
import { fromStore } from "./stores.js";

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
 * 2026-10-14), NotFoundError→404 and ConflictError→409 (SPEC-008 Amendment
 * 2026-10-15). A 5xx is logged with its cause; no body carries internal detail.
 * Each request gets a UUID as its id, the `reqId` of its log lines and the
 * `correlation_id` of its audit rows; an id the client sends is ignored.
 */
export function buildApp(services: Services, logger: LoggerOpt = false): FastifyInstance {
  const app = Fastify({
    logger,
    genReqId: () => globalThis.crypto.randomUUID(),
    requestIdHeader: false,
  });
  app.register(cookie);

  app.setErrorHandler(async (err, req, reply) => {
    if (err instanceof AuthError) return reply.code(401).send({ error: "unauthorized" });
    if (err instanceof RateLimitError) return reply.code(429).send({ error: "rate_limited" });
    if (err instanceof ForbiddenError) {
      // SPEC-008 Amendment 2026-10-16 item 1: a role or CSRF refusal of a session's
      // principal writes its row; the request is refused either way.
      const session = req.authSession;
      const reason = err.reason;
      if (reason && session) {
        const ctx = requestContext(req, services.trustedProxies);
        try {
          await fromStore(() =>
            appendAudit(services.pg, {
              userId: session.user_id,
              orgId: session.org_id,
              action: "access.denied",
              outcome: "denied",
              sourceIp: ctx.sourceIp,
              correlationId: ctx.correlationId,
              detail: { reason, route: `${req.method} ${req.routeOptions.url}` },
            }),
          );
        } catch (cause) {
          // The row cannot be written: every failure of the write is the store's.
          const store = cause instanceof StoreUnavailableError ? cause.cause : cause;
          req.log.error({ error: loggable(store) }, "store unavailable");
          return reply.code(503).send({ error: "unavailable" });
        }
      }
      return reply.code(403).send({ error: "forbidden" });
    }
    if (err instanceof BadRequestError) return reply.code(400).send({ error: "bad_request" });
    if (err instanceof NotFoundError) return reply.code(404).send({ error: "not_found" });
    if (err instanceof ConflictError) return reply.code(409).send({ error: "conflict" });
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
    // A body Fastify cannot read keeps Fastify's 4xx (SPEC-008 Amendment 2026-10-15
    // item 2): 400, 413 or 415.
    return reply.code(code).send({ error: code < 500 ? "bad_request" : "internal" });
  });

  registerAuthRoutes(app, services);
  registerReadRoutes(app, services);
  return app;
}
