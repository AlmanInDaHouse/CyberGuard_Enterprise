import Fastify, { type FastifyInstance, type FastifyServerOptions } from "fastify";

/**
 * Build the Fastify application. SPEC-004 scaffold: serves `/health` and
 * 501 placeholders for the two agent endpoints. The B5 implementation
 * replaces the placeholders and adds the mTLS heartbeat listener.
 */
export function buildApp(opts: { logger?: FastifyServerOptions["logger"] } = {}): FastifyInstance {
  const app = Fastify({ logger: opts.logger ?? false });

  app.get("/health", async () => ({ status: "ok" }));

  app.post("/v1/agents/enroll", async (_req, reply) =>
    reply.code(501).send({ error: "not_implemented" }),
  );

  app.post("/v1/agents/heartbeat", async (_req, reply) =>
    reply.code(501).send({ error: "not_implemented" }),
  );

  return app;
}
