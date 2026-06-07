import { type ClickHouseClient, createClient } from "@clickhouse/client";
import { Redis } from "ioredis";
import { Kysely, PostgresDialect } from "kysely";
import pg from "pg";
import { type ServerCa, ensureCa } from "./ca.js";
import type { Config } from "./config.js";
import type { Database } from "./db/schema.js";
import type { NotifyConfig } from "./notify/index.js";
import { buildNotifyConfig } from "./notify/transport.js";

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
  /**
   * SPEC-014 — the incident email notify dependency, or `null` when SMTP is
   * unconfigured (notify disabled cleanly). Built once at boot (ADR-0017
   * §Decision §2); dormant until a production detection driver consumes it
   * (the inherited test-validated-altitude gap, ADR-0017 §Consequences).
   */
  notify: NotifyConfig | null;
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
  const notify = buildNotifyConfig(config);

  return {
    config,
    pool,
    db,
    redis,
    ch,
    ca,
    notify,
    async close() {
      await Promise.allSettled([db.destroy(), redis.quit(), ch.close()]);
    },
  };
}
