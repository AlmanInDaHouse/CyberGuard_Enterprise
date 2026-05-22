import { promises as fs } from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";
import { createClient } from "@clickhouse/client";
import { FileMigrationProvider, Kysely, Migrator, PostgresDialect, sql } from "kysely";
import pg from "pg";
import { type Config, loadConfig } from "../config.js";

// Fixed key for the Postgres advisory lock that serialises migration
// application across instances (SPEC-004 MUST-2 single-applier).
const MIGRATION_LOCK_KEY = 4_927_004;

/**
 * Apply Postgres migrations (under a `pg_try_advisory_lock`, so concurrent
 * instances cannot double-apply) and bootstrap the ClickHouse `heartbeats`
 * table. Idempotent.
 */
export async function runMigrations(config: Config = loadConfig()): Promise<void> {
  const pool = new pg.Pool({ connectionString: config.INGEST_PG_URL });
  const db = new Kysely<unknown>({ dialect: new PostgresDialect({ pool }) });
  try {
    const lock = await sql<{
      locked: boolean;
    }>`SELECT pg_try_advisory_lock(${MIGRATION_LOCK_KEY}) AS locked`.execute(db);
    if (!lock.rows[0]?.locked) {
      // Another instance holds the lock and is migrating; skip.
      return;
    }
    try {
      const migrator = new Migrator({
        db,
        provider: new FileMigrationProvider({
          fs,
          path,
          migrationFolder: path.join(path.dirname(fileURLToPath(import.meta.url)), "migrations"),
        }),
      });
      const { error, results } = await migrator.migrateToLatest();
      for (const r of results ?? []) {
        if (r.status === "Error") {
          throw new Error(`migration failed: ${r.migrationName}`);
        }
      }
      if (error) {
        throw error instanceof Error ? error : new Error(String(error));
      }
    } finally {
      await sql`SELECT pg_advisory_unlock(${MIGRATION_LOCK_KEY})`.execute(db);
    }
  } finally {
    await db.destroy();
  }

  await bootstrapClickHouse(config);
}

async function bootstrapClickHouse(config: Config): Promise<void> {
  const ch = createClient({
    url: config.INGEST_CH_URL,
    username: config.INGEST_CH_USER,
    password: config.INGEST_CH_PASSWORD,
    database: config.INGEST_CH_DB,
  });
  try {
    await ch.command({
      query: `
        CREATE TABLE IF NOT EXISTS heartbeats (
          agent_id        UUID,
          sequence_number UInt64,
          inner_sent_at   DateTime64(3, 'UTC'),
          outer_sent_at   DateTime64(3, 'UTC'),
          status          LowCardinality(String),
          uptime_seconds  UInt64,
          nonce           String,
          arrived_at      DateTime64(3, 'UTC') DEFAULT now64(3)
        ) ENGINE = MergeTree
        PARTITION BY toYYYYMMDD(arrived_at)
        ORDER BY (agent_id, arrived_at)
      `,
    });
  } finally {
    await ch.close();
  }
}

// Allow `tsx src/db/migrate.ts` / `node dist/db/migrate.js` to run directly.
if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  runMigrations()
    .then(() => process.exit(0))
    .catch((e: unknown) => {
      process.stderr.write(`cg-ingest migrate: ${String(e)}\n`);
      process.exit(1);
    });
}
