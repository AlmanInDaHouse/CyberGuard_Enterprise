import { type ClickHouseClient, createClient } from "@clickhouse/client";
import { Redis } from "ioredis";
import pg from "pg";
import { type AuthService, createAuthService } from "./auth/service.js";
import { type TrustedProxies, parseTrustedProxies } from "./auth/source-address.js";
import type { Config } from "./config.js";
import { type ForensicKey, ensureForensicKey } from "./forensic/key.js";

/**
 * The api's wired dependencies: the Postgres pool (users/audit_log + the read
 * slice's incidents/alerts) and the Redis client (opaque sessions + rate-limit
 * counters + TOTP-replay markers, ADR-0003 §Decision). The AuthService is the real
 * SPEC-008 implementation backed by both. `ch` is the read-only ClickHouse reader
 * (ADR-0015 / SPEC-010) the forensic event drill uses to read cges_events; it is a
 * singleton built once per process (the request-path pattern,
 * services/ingest/src/services.ts:46-51 + routes/heartbeat.ts:151). It is lazy
 * (no connection until the first query), so the Postgres read routes do not depend
 * on ClickHouse availability. `forensicKey` is the dedicated Ed25519 evidence-signing
 * key (SPEC-012 / ADR-0016 §5) — the api's FIRST signing key, bootstrapped + decrypted
 * once per process (mirrors how ingest exposes its `ca`, services/ingest/src/services.ts:53);
 * it is NOT the ingest CA. `trustedProxies` is `API_TRUSTED_PROXIES`, read once (SPEC-008's
 * second Amendment 2026-10-15); `null` when it lists nothing.
 */
export interface Services {
  pg: pg.Pool;
  redis: Redis;
  ch: ClickHouseClient;
  auth: AuthService;
  forensicKey: ForensicKey;
  trustedProxies: TrustedProxies | null;
  close(): Promise<void>;
}

/** How long the pool waits for a connection, whether Postgres has not established it
 *  or a busy pool has none free, before the query fails: a read then answers 503
 *  (SPEC-009 Amendment 2026-10-14 §3). */
const PG_CONNECTION_TIMEOUT_MS = 5000;
/** The server cancels a statement that runs this long (SPEC-008 Amendment 2026-10-15
 *  item 15). */
const PG_STATEMENT_TIMEOUT_MS = 5000;
/** The client fails a query with no answer for this long, a silent peer included;
 *  longer than the statement timeout, so a slow statement ends at the server first. */
const PG_QUERY_TIMEOUT_MS = 6000;

/**
 * The api's Postgres pool. A query that failed leaves the pool with its client. An idle
 * client whose connection ends, Postgres restarting or unreachable, is an `error` event of
 * the pool, which without a listener would end the process (SPEC-023 deploy_ac_017: the
 * api keeps answering /health): the pool drops that client, and the next query that needs
 * Postgres fails, is logged and answers 503.
 */
export function createPgPool(connectionString: string): pg.Pool {
  const pool = new pg.Pool({
    connectionString,
    connectionTimeoutMillis: PG_CONNECTION_TIMEOUT_MS,
    statement_timeout: PG_STATEMENT_TIMEOUT_MS,
    query_timeout: PG_QUERY_TIMEOUT_MS,
  });
  pool.on("error", () => {});
  return pool;
}

/** The bound of a connection to Redis and of each command (SPEC-008 Amendment
 *  2026-10-15 item 14). */
const REDIS_TIMEOUT_MS = 5000;

/**
 * The api's Redis client. A command it has not completed within 5 s fails, whether
 * the connection could not be made or the server does not answer, and after one
 * failed reconnection the commands it queued fail too, so no request waits for Redis
 * to return (debt #69). Every failure reaches the caller, which answers 503. A lost
 * connection is also an `error` event of the client, listened to here so that ioredis
 * does not print it as unhandled: the commands' failures are what is logged.
 */
export function createRedisClient(url: string): Redis {
  const redis = new Redis(url, {
    connectTimeout: REDIS_TIMEOUT_MS,
    commandTimeout: REDIS_TIMEOUT_MS,
    maxRetriesPerRequest: 1,
  });
  redis.on("error", () => {});
  return redis;
}

export async function buildServices(config: Config): Promise<Services> {
  const trustedProxies = parseTrustedProxies(config.API_TRUSTED_PROXIES);
  const pool = createPgPool(config.API_PG_URL);
  const redis = createRedisClient(config.API_REDIS_URL);
  const ch = createClient({
    url: config.API_CH_URL,
    username: config.API_CH_USER,
    password: config.API_CH_PASSWORD,
    database: config.API_CH_DB,
  });
  const auth = createAuthService(pool, redis, config);
  // SPEC-012 §Data contracts §4 — bootstrap (first run) or decrypt-on-load the dedicated
  // forensic Ed25519 signing key from Postgres (mirrors ingest's ensureCa at
  // services/ingest/src/services.ts:53). Requires the 0003_forensic_key migration. No close
  // handle: the key is in-memory CryptoKey material, not a pooled connection.
  const forensicKey = await ensureForensicKey(pool, config.API_FORENSIC_PASSPHRASE);
  return {
    pg: pool,
    redis,
    ch,
    auth,
    forensicKey,
    trustedProxies,
    // Promise.allSettled (aligns with services/ingest/src/services.ts:74): one
    // rejected close (e.g. pool.end()) must not prevent closing redis + ch.
    async close() {
      await Promise.allSettled([pool.end(), redis.quit(), ch.close()]);
    },
  };
}
