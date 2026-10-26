import { promises as fs } from "node:fs";
import * as path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { Kysely, type Migration, type MigrationProvider, Migrator, PostgresDialect } from "kysely";
import pg from "pg";
import { type Config, loadConfigFor } from "../config.js";
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

/** How long the one-shot's command waits for a lock before it fails: its own advisory lock,
 *  and each lock its migrations wait for, the migrator's own included (SPEC-023 §Data
 *  contracts §4, its Amendments 2026-10-25 and 2026-10-26; debts #122 and #127). */
export const MIGRATION_LOCK_WAIT_MS = 300_000;

/** How a run takes the advisory lock: the service skips when another process holds it; the
 *  one-shot's command waits for it (SPEC-023 §Data contracts §4), up to `waitMs`
 *  (MIGRATION_LOCK_WAIT_MS unless given), and then fails, and waits as long at most for each
 *  lock its migrations need. A role refused the lock function (the bundle's runtime roles,
 *  SPEC-023 Amendment 2026-10-25, item 2) fails either way. */
export interface MigrateOptions {
  wait?: boolean;
  waitMs?: number;
}

/** The bound, in milliseconds, of each lock wait of a run that waits. */
function boundMs(options: MigrateOptions): number {
  return Math.max(1, Math.trunc(options.waitMs ?? MIGRATION_LOCK_WAIT_MS));
}

/** 55P03, lock_not_available: a lock_timeout ran out. */
function lockTimedOut(err: unknown): boolean {
  return (err as { code?: string } | null | undefined)?.code === "55P03";
}

/**
 * Takes the migration lock on `lock`, whose connection's lock_timeout (set when it connected)
 * bounds the wait. A lock still held when it runs out fails the run, naming the lock.
 */
async function waitForLock(lock: pg.PoolClient, ms: number): Promise<void> {
  try {
    await lock.query("SELECT pg_advisory_lock($1)", [MIGRATION_LOCK_KEY]);
  } catch (err) {
    if (lockTimedOut(err)) {
      throw new Error(
        `the migration lock is held by another session: waited ${Math.round(ms / 1000)} s`,
      );
    }
    throw err;
  }
}

/**
 * Apply the api's own Postgres migrations under an advisory lock (its own key) so
 * concurrent instances cannot double-apply. The lock is taken and released on one
 * connection, held apart from the migrator's. Without `wait`, a run that finds the lock
 * held skips; with it, the run waits for the lock, so a command that completes has
 * applied the migrations, and fails when the lock is still held after `waitMs`. Idempotent.
 */
export async function runMigrations(
  config: Pick<Config, "API_PG_URL">,
  options: MigrateOptions = {},
): Promise<void> {
  // A run that waits bounds each lock wait of every connection (SPEC-023's second Amendment
  // 2026-10-26, item 1): its own lock's, the migrator's and each migration statement's. The
  // bound is set before the pool hands a connection out, so that a connection it cannot be
  // set on fails the run instead of waiting without bound.
  const ms = boundMs(options);
  const pool = new pg.Pool({
    connectionString: config.API_PG_URL,
    ...(options.wait
      ? {
          onConnect: async (client: pg.ClientBase) => {
            await client.query(`SET lock_timeout = ${ms}`);
          },
        }
      : {}),
  });
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
        await waitForLock(lock, ms);
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
          // Kysely ran the pending migrations in one transaction, which this failure rolled
          // back: the run committed none of them.
          if (options.wait && lockTimedOut(error)) {
            throw new Error(
              `a lock a migration waits for is held by another session: waited ${Math.round(ms / 1000)} s`,
            );
          }
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

/** The command's one line for a failure: the error's message, without a class name's prefix,
 *  so that the lock's line reads as SPEC-023 Amendment 2026-10-25, item 1, quotes it. */
export function migrateFatalLine(e: unknown): string {
  const reason = e instanceof Error && e.message !== "" ? e.message : String(e);
  return `cg-api migrate: fatal: ${reason}`;
}

// Allow `tsx src/db/migrate.ts` / `node dist/db/migrate.js` to run directly: the one-shot's
// command, which validates API_PG_URL alone and waits for the lock.
if (isMain(import.meta.url)) {
  Promise.resolve()
    .then(() => runMigrations(loadConfigFor(["API_PG_URL"]), { wait: true }))
    .then(() => process.exit(0))
    .catch((e: unknown) => {
      process.stderr.write(`${migrateFatalLine(e)}\n`);
      process.exit(1);
    });
}
