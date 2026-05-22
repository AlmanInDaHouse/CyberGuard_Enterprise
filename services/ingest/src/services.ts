import { type ClickHouseClient, createClient } from "@clickhouse/client";
import { Redis } from "ioredis";
import { Kysely, PostgresDialect } from "kysely";
import pg from "pg";
import { type ServerCa, ensureCa } from "./ca.js";
import type { Config } from "./config.js";
import type { Database } from "./db/schema.js";

/**
 * The shared persistence + crypto layer behind both listeners (FR-002):
 * Postgres (agents, tokens, CA), Redis (nonce anti-replay), ClickHouse
 * (heartbeats), and the server CA. Built once per process.
 */
export interface Services {
  config: Config;
  pool: pg.Pool;
  db: Kysely<Database>;
  redis: Redis;
  ch: ClickHouseClient;
  ca: ServerCa;
  close(): Promise<void>;
}

export async function buildServices(config: Config): Promise<Services> {
  const pool = new pg.Pool({ connectionString: config.INGEST_PG_URL });
  const db = new Kysely<Database>({ dialect: new PostgresDialect({ pool }) });
  const redis = new Redis(config.INGEST_REDIS_URL, { maxRetriesPerRequest: 1 });
  const ch = createClient({
    url: config.INGEST_CH_URL,
    username: config.INGEST_CH_USER,
    password: config.INGEST_CH_PASSWORD,
    database: config.INGEST_CH_DB,
  });

  const ca = await ensureCa(pool, config.INGEST_CA_PASSPHRASE);

  return {
    config,
    pool,
    db,
    redis,
    ch,
    ca,
    async close() {
      await Promise.allSettled([db.destroy(), redis.quit(), ch.close()]);
    },
  };
}
