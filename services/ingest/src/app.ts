import type { ServerOptions as HttpsServerOptions } from "node:https";
import Fastify, { type FastifyInstance, type FastifyServerOptions } from "fastify";
import { registerEnrollRoutes } from "./routes/enroll.js";
import { registerHeartbeatRoutes } from "./routes/heartbeat.js";
import type { Services } from "./services.js";

type LoggerOpt = FastifyServerOptions["logger"];

/**
 * Plain-HTTP listener app (FR-002): `/health` + `POST /v1/agents/enroll`.
 * Accepts clients with no certificate yet (enrollment chicken-and-egg).
 */
export function buildEnrollApp(services: Services, logger: LoggerOpt = false): FastifyInstance {
  const app = Fastify({ logger });
  registerEnrollRoutes(app, services);
  return app;
}

/**
 * mTLS listener app (FR-002): `POST /v1/agents/heartbeat`. The TLS options
 * (`requestCert` + `rejectUnauthorized` against the CA) are applied by the
 * caller via Fastify's `https` server options.
 */
export function buildHeartbeatApp(
  services: Services,
  https: HttpsServerOptions,
  logger: LoggerOpt = false,
): FastifyInstance {
  const app = Fastify({ logger, https });
  registerHeartbeatRoutes(app, services);
  return app;
}
