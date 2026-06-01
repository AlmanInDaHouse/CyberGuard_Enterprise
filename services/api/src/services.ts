import { Redis } from "ioredis";
import pg from "pg";
import { type AuthService, notImplementedAuthService } from "./auth/service.js";
import type { Config } from "./config.js";

/**
 * The api's wired dependencies: the Postgres pool (users/audit_log) and the
 * Redis client (opaque sessions + rate-limit counters, ADR-0003 §Decision). The
 * AuthService is the RED-gate stub; the GREEN gate swaps in the real
 * implementation backed by these two clients. Redis is `lazyConnect` so the
 * RED scaffold opens no connection it never uses.
 */
export interface Services {
  pg: pg.Pool;
  redis: Redis;
  auth: AuthService;
  close(): Promise<void>;
}

export async function buildServices(config: Config): Promise<Services> {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  const redis = new Redis(config.API_REDIS_URL, { lazyConnect: true, maxRetriesPerRequest: 1 });
  const auth = notImplementedAuthService();
  return {
    pg: pool,
    redis,
    auth,
    async close() {
      await pool.end();
      redis.disconnect();
    },
  };
}
