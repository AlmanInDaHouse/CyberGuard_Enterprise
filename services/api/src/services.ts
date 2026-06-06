import { type ClickHouseClient, createClient } from "@clickhouse/client";
import { Redis } from "ioredis";
import pg from "pg";
import { type AuthService, createAuthService } from "./auth/service.js";
import type { Config } from "./config.js";
import { type ForensicKey, ensureForensicKey } from "./forensic/key.js";

/**
 * The api's wired dependencies: the Postgres pool (users/audit_log + the read
 * slice's incidents/alerts) and the Redis client (opaque sessions + rate-limit
 * counters + TOTP-replay markers, ADR-0003 §Decision). The AuthService is the real
 * SPEC-008 implementation backed by both. `ch` is the read-only ClickHouse reader
 * (ADR-0015 / SPEC-010) the forensic event drill uses to read cges_events; it is a
 * singleton built once per process (the request-path pattern,
 * services/ingest/src/services.ts:24-33 + routes/heartbeat.ts:55). It is lazy
 * (no connection until the first query), so the Postgres read routes do not depend
 * on ClickHouse availability. `forensicKey` is the dedicated Ed25519 evidence-signing
 * key (SPEC-012 / ADR-0016 §5) — the api's FIRST signing key, bootstrapped + decrypted
 * once per process (mirrors how ingest exposes its `ca`, services/ingest/services.ts:35);
 * it is NOT the ingest CA.
 */
export interface Services {
  pg: pg.Pool;
  redis: Redis;
  ch: ClickHouseClient;
  auth: AuthService;
  forensicKey: ForensicKey;
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
  // SPEC-012 §Data contracts §4 — bootstrap (first run) or decrypt-on-load the dedicated
  // forensic Ed25519 signing key from Postgres (mirrors ingest's ensureCa at
  // services/ingest/services.ts:35). Requires the 0003_forensic_key migration. No close
  // handle: the key is in-memory CryptoKey material, not a pooled connection.
  const forensicKey = await ensureForensicKey(pool, config.API_FORENSIC_PASSPHRASE);
  return {
    pg: pool,
    redis,
    ch,
    auth,
    forensicKey,
    // Promise.allSettled (aligns with services/ingest/src/services.ts:45): one
    // rejected close (e.g. pool.end()) must not prevent closing redis + ch.
    async close() {
      await Promise.allSettled([pool.end(), redis.quit(), ch.close()]);
    },
  };
}
