import Fastify, { type FastifyInstance, type FastifyServerOptions } from "fastify";
import { registerAuthRoutes } from "./auth/routes.js";
import { NotImplementedError } from "./errors.js";
import type { Services } from "./services.js";

type LoggerOpt = FastifyServerOptions["logger"];

/**
 * Build the human-facing API (SPEC-008). User-facing only — NO agent mTLS
 * (that is services/ingest's boundary, ADR-0014 §3). During the RED gate the
 * auth handlers delegate to a stub; the error handler maps NotImplementedError
 * → 501 so each auth AC observes a present-but-unimplemented control.
 */
export function buildApp(services: Services, logger: LoggerOpt = false): FastifyInstance {
  const app = Fastify({ logger });

  app.setErrorHandler((err, _req, reply) => {
    if (err instanceof NotImplementedError) {
      return reply.code(501).send({ error: "not_implemented", detail: err.message });
    }
    const statusCode = (err as { statusCode?: number }).statusCode;
    const code = typeof statusCode === "number" ? statusCode : 500;
    return reply.code(code).send({ error: "internal" });
  });

  registerAuthRoutes(app, services);
  return app;
}
