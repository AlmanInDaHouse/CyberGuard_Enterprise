import { Redis } from "ioredis";
import pg from "pg";
import { type AuthService, createAuthService } from "./auth/service.js";
import type { Config } from "./config.js";

/**
 * The api's wired dependencies: the Postgres pool (users/audit_log) and the Redis
 * client (opaque sessions + rate-limit counters + TOTP-replay markers, ADR-0003
 * §Decision). The AuthService is the real SPEC-008 implementation backed by both.
 */
export interface Services {
  pg: pg.Pool;
  redis: Redis;
  auth: AuthService;
  close(): Promise<void>;
}

export async function buildServices(config: Config): Promise<Services> {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  const redis = new Redis(config.API_REDIS_URL, { maxRetriesPerRequest: null });
  const auth = createAuthService(pool, redis, config);
  return {
    pg: pool,
    redis,
    auth,
    async close() {
      await pool.end();
      await redis.quit();
    },
  };
}
