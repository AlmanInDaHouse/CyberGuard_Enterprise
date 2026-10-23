import { promises as fs } from "node:fs";
import * as path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { Kysely, type Migration, type MigrationProvider, Migrator, PostgresDialect } from "kysely";
import pg from "pg";
import { type Config, ConfigError, loadConfigFor } from "../config.js";
import { isMain } from "../is-main.js";
import { watchPool } from "../store-events.js";

/**
 * services/api OWNS its auth migrations independently of services/ingest
 * (SPEC-008 §Operational §7, ratified Option A): a DISTINCT advisory-lock key
 * and DISTINCT Kysely ledger tables, so the two services' migration histories
 * never collide on the shared Postgres instance (ADR-0003, one instance). The
 * applier itself mirrors the services/ingest pattern; it bootstraps no
 * ClickHouse (api is Postgres + Redis only).
 */
export const MIGRATION_LOCK_KEY = 8_927_008;
const MIGRATION_TABLE = "api_kysely_migration";
const MIGRATION_LOCK_TABLE = "api_kysely_migration_lock";
/** The api's schema (SPEC-023 §Operational §4): its tables and its migration ledger. */
export const API_SCHEMA = "api";

/**
 * Auto-discovering migration provider importing each file via a `file://` URL
 * (cross-platform on Windows ESM, like the ingest applier).
 */
function folderMigrationProvider(migrationFolder: string): MigrationProvider {
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

/** How a run takes the advisory lock: the service skips when another process holds it; the
 *  one-shot's command waits for it (SPEC-023 §Data contracts §4). */
export interface MigrateOptions {
  wait?: boolean;
}

/**
 * Apply the api's own Postgres migrations under an advisory lock (its own key) so
 * concurrent instances cannot double-apply. The lock is taken and released on one
 * connection, held apart from the migrator's. Without `wait`, a run that finds the lock
 * held skips; with it, the run waits for the lock, so a command that completes has
 * applied the migrations. Idempotent.
 */
export async function runMigrations(
  config: Pick<Config, "API_PG_URL">,
  options: MigrateOptions = {},
): Promise<void> {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  // A lost connection fails the run with its fatal line, not as an unheard error event.
  watchPool(pool);
  // SPEC-023 §Operational §4: each connection's search path is the api's schema and then
  // `public`, whatever the URL says, so a migration's table lands in the schema; the
  // migrator checks for the schema and creates it when absent (a superuser's dev stack and
  // tests), before the first migration and before its ledger, which is the schema's too.
  // Queued on each new client before any query of the migrator's.
  pool.on("connect", (client) => {
    client.query(`SET search_path TO ${API_SCHEMA}, public`).catch(() => {});
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
        // Another api instance holds the lock and is migrating; skip.
        if (!r.rows[0]?.locked) return;
      }
      try {
        const migrator = new Migrator({
          db,
          provider: folderMigrationProvider(
            path.join(path.dirname(fileURLToPath(import.meta.url)), "migrations"),
          ),
          migrationTableName: MIGRATION_TABLE,
          migrationLockTableName: MIGRATION_LOCK_TABLE,
          migrationTableSchema: API_SCHEMA,
        });
        const { error, results } = await migrator.migrateToLatest();
        // The failed migration's own message is the operator's instruction (0004 names the
        // org and the count to resolve, SPEC-008 Amendment 2026-10-15 item 3).
        const failed = (results ?? []).find((r) => r.status === "Error");
        if (failed || error) {
          const reason = error instanceof Error ? error.message : String(error ?? "");
          throw new Error(
            `migration failed: ${failed?.migrationName ?? "(unknown)"}${reason ? `: ${reason}` : ""}`,
          );
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
}

// Allow `tsx src/db/migrate.ts` / `node dist/db/migrate.js` to run directly: the one-shot's
// command, which validates API_PG_URL alone and waits for the lock.
if (isMain(import.meta.url)) {
  Promise.resolve()
    .then(() => runMigrations(loadConfigFor(["API_PG_URL"]), { wait: true }))
    .then(() => process.exit(0))
    .catch((e: unknown) => {
      const line = e instanceof ConfigError ? e.message : String(e);
      process.stderr.write(`cg-api migrate: fatal: ${line}\n`);
      process.exit(1);
    });
}
