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
 * Largest heartbeat body the mTLS listener accepts: 4 MiB, so a batch of
 * 1024 events with long paths is not refused by Fastify's 1 MiB default
 * (SPEC-017 §Operational §7).
 */
export const HEARTBEAT_BODY_LIMIT_BYTES = 4 * 1024 * 1024;

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
  const app = Fastify({ logger, https, bodyLimit: HEARTBEAT_BODY_LIMIT_BYTES });
  registerHeartbeatRoutes(app, services);
  return app;
}
