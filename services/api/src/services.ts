import { type ClickHouseClient, createClient } from "@clickhouse/client";
import { Redis } from "ioredis";
import pg from "pg";
import { type AuthService, createAuthService } from "./auth/service.js";
import type { Config } from "./config.js";

/**
 * The api's wired dependencies: the Postgres pool (users/audit_log + the read
 * slice's incidents/alerts) and the Redis client (opaque sessions + rate-limit
 * counters + TOTP-replay markers, ADR-0003 §Decision). The AuthService is the real
 * SPEC-008 implementation backed by both. `ch` is the read-only ClickHouse reader
 * (ADR-0015 / SPEC-010) the forensic event drill uses to read cges_events; it is a
 * singleton built once per process (the request-path pattern,
 * services/ingest/src/services.ts:24-33 + routes/heartbeat.ts:55). It is lazy
 * (no connection until the first query), so the Postgres read routes do not depend
 * on ClickHouse availability.
 */
export interface Services {
  pg: pg.Pool;
  redis: Redis;
  ch: ClickHouseClient;
  auth: AuthService;
  close(): Promise<void>;
}

export async function buildServices(config: Config): Promise<Services> {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  const redis = new Redis(config.API_REDIS_URL, { maxRetriesPerRequest: null });
  const ch = createClient({
    url: config.API_CH_URL,
    username: config.API_CH_USER,
    password: config.API_CH_PASSWORD,
    database: config.API_CH_DB,
  });
  const auth = createAuthService(pool, redis, config);
  return {
    pg: pool,
    redis,
    ch,
    auth,
    // Promise.allSettled (aligns with services/ingest/src/services.ts:45): one
    // rejected close (e.g. pool.end()) must not prevent closing redis + ch.
    async close() {
      await Promise.allSettled([pool.end(), redis.quit(), ch.close()]);
    },
  };
}
