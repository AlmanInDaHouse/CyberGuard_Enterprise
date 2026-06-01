import { promises as fs } from "node:fs";
import * as path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import {
  Kysely,
  type Migration,
  type MigrationProvider,
  Migrator,
  PostgresDialect,
  sql,
} from "kysely";
import pg from "pg";
import { type Config, loadConfig } from "../config.js";

/**
 * services/api OWNS its auth migrations independently of services/ingest
 * (SPEC-008 §Operational §7, ratified Option A): a DISTINCT advisory-lock key
 * and DISTINCT Kysely ledger tables, so the two services' migration histories
 * never collide on the shared Postgres instance (ADR-0003, one instance). The
 * applier itself mirrors the services/ingest pattern; it bootstraps no
 * ClickHouse (api is Postgres + Redis only).
 */
const MIGRATION_LOCK_KEY = 8_927_008;
const MIGRATION_TABLE = "api_kysely_migration";
const MIGRATION_LOCK_TABLE = "api_kysely_migration_lock";

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

/**
 * Apply the api's own Postgres migrations under a `pg_try_advisory_lock` (its
 * own key) so concurrent instances cannot double-apply. Idempotent.
 */
export async function runMigrations(config: Config = loadConfig()): Promise<void> {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  const db = new Kysely<unknown>({ dialect: new PostgresDialect({ pool }) });
  try {
    const lock = await sql<{
      locked: boolean;
    }>`SELECT pg_try_advisory_lock(${MIGRATION_LOCK_KEY}) AS locked`.execute(db);
    if (!lock.rows[0]?.locked) {
      // Another api instance holds the lock and is migrating; skip.
      return;
    }
    try {
      const migrator = new Migrator({
        db,
        provider: folderMigrationProvider(
          path.join(path.dirname(fileURLToPath(import.meta.url)), "migrations"),
        ),
        migrationTableName: MIGRATION_TABLE,
        migrationLockTableName: MIGRATION_LOCK_TABLE,
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
}

// Allow `tsx src/db/migrate.ts` / `node dist/db/migrate.js` to run directly.
if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  runMigrations()
    .then(() => process.exit(0))
    .catch((e: unknown) => {
      process.stderr.write(`cg-api migrate: ${String(e)}\n`);
      process.exit(1);
    });
}
