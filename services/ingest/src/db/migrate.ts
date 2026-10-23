import { promises as fs } from "node:fs";
import * as path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { createClient } from "@clickhouse/client";
import { Kysely, type Migration, type MigrationProvider, Migrator, PostgresDialect } from "kysely";
import pg from "pg";
import { type Config, ConfigError, loadConfigFor } from "../config.js";
import { isMain } from "../is-main.js";
import { watchPool } from "../store-events.js";
import { bootstrapClickHouseTables } from "./clickhouse-bootstrap.js";

// Fixed key for the Postgres advisory lock that serialises migration
// application across instances (SPEC-004 MUST-2 single-applier).
export const MIGRATION_LOCK_KEY = 4_927_004;

/**
 * Auto-discovering migration provider that imports each file via a `file://`
 * URL. Kysely's bundled `FileMigrationProvider` `import()`s raw absolute
 * paths, which fail on Windows ESM (`ERR_UNSUPPORTED_ESM_URL_SCHEME`); this
 * keeps the same drop-a-file-in-the-folder convention but is cross-platform.
 */
export function folderMigrationProvider(migrationFolder: string): MigrationProvider {
  return {
    async getMigrations(): Promise<Record<string, Migration>> {
      const entries = (await fs.readdir(migrationFolder))
        .filter((f) => /\.(?:js|mjs|ts)$/.test(f) && !f.endsWith(".d.ts"))
        .sort();
      const migrations: Record<string, Migration> = {};
      for (const file of entries) {
        const name = file.replace(/\.(?:js|mjs|ts)$/, "");
        migrations[name] = (await import(
          pathToFileURL(path.join(migrationFolder, file)).href
        )) as Migration;
      }
      return migrations;
    },
  };
}

/** The variables a migration run uses: the stores' (SPEC-023 §Data contracts §4). */
export const MIGRATE_VARIABLES = [
  "INGEST_PG_URL",
  "INGEST_CH_URL",
  "INGEST_CH_USER",
  "INGEST_CH_PASSWORD",
  "INGEST_CH_DB",
] as const;
export type MigrateConfig = Pick<Config, (typeof MIGRATE_VARIABLES)[number]>;

/** How a run takes the advisory lock: the service skips when another process holds it; the
 *  one-shot's command waits for it (SPEC-023 §Data contracts §4). */
export interface MigrateOptions {
  wait?: boolean;
}

/** Ingest's schema (SPEC-023 §Operational §4): its tables and its migration ledger. */
export const INGEST_SCHEMA = "ingest";

/**
 * Apply ingest's Postgres migrations under an advisory lock, so concurrent instances cannot
 * double-apply. The lock is taken and released on one connection, held apart from the
 * migrator's. Without `wait`, a run that finds the lock held skips and returns false; with
 * it, the run waits for the lock, so a command that completes has applied the migrations.
 *
 * SPEC-023 §Operational §4: each connection's search path is ingest's schema and then
 * `public`, whatever the URL says, so a migration's table lands in the schema; the migrator
 * checks for the schema and creates it when it is absent (a superuser's dev stack and
 * tests), before the first migration and before its ledger, which is the schema's too.
 * Idempotent.
 */
export async function runPgMigrations(
  config: Pick<Config, "INGEST_PG_URL">,
  options: MigrateOptions = {},
): Promise<boolean> {
  const pool = new pg.Pool({ connectionString: config.INGEST_PG_URL });
  // A lost connection fails the run with its fatal line, not as an unheard error event.
  watchPool(pool);
  // Queued on each new client before any query of the migrator's.
  pool.on("connect", (client) => {
    client.query(`SET search_path TO ${INGEST_SCHEMA}, public`).catch(() => {});
  });
  const db = new Kysely<unknown>({ dialect: new PostgresDialect({ pool }) });
  try {
    const lock = await pool.connect();
    try {
      if (options.wait) {
        await lock.query("SELECT pg_advisory_lock($1)", [MIGRATION_LOCK_KEY]);
      } else {
        const r = await lock.query<{ locked: boolean }>(
          "SELECT pg_try_advisory_lock($1) AS locked",
          [MIGRATION_LOCK_KEY],
        );
        // Another instance holds the lock and is migrating; skip.
        if (!r.rows[0]?.locked) return false;
      }
      try {
        const migrator = new Migrator({
          db,
          provider: folderMigrationProvider(
            path.join(path.dirname(fileURLToPath(import.meta.url)), "migrations"),
          ),
          migrationTableSchema: INGEST_SCHEMA,
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
        await lock.query("SELECT pg_advisory_unlock($1)", [MIGRATION_LOCK_KEY]);
      }
    } finally {
      lock.release();
    }
  } finally {
    await db.destroy();
  }
  return true;
}

/**
 * Ingest's migration step: the Postgres migrations (`runPgMigrations`), then, unless they
 * were skipped, the ClickHouse tables (`./clickhouse-bootstrap.ts`). Idempotent.
 */
export async function runMigrations(
  config: MigrateConfig,
  options: MigrateOptions = {},
): Promise<void> {
  if (await runPgMigrations(config, options)) await bootstrapClickHouse(config);
}

async function bootstrapClickHouse(config: MigrateConfig): Promise<void> {
  const ch = createClient({
    url: config.INGEST_CH_URL,
    username: config.INGEST_CH_USER,
    password: config.INGEST_CH_PASSWORD,
    database: config.INGEST_CH_DB,
  });
  try {
    await bootstrapClickHouseTables(ch);
  } finally {
    await ch.close();
  }
}

// Allow `tsx src/db/migrate.ts` / `node dist/db/migrate.js` to run directly: the one-shot's
// command, which validates the stores' variables alone and waits for the lock.
if (isMain(import.meta.url)) {
  Promise.resolve()
    .then(() => runMigrations(loadConfigFor(MIGRATE_VARIABLES), { wait: true }))
    .then(() => process.exit(0))
    .catch((e: unknown) => {
      const line = e instanceof ConfigError ? e.message : String(e);
      process.stderr.write(`cg-ingest migrate: fatal: ${line}\n`);
      process.exit(1);
    });
}
