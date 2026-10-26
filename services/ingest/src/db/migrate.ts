import { promises as fs } from "node:fs";
import * as path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { createClient } from "@clickhouse/client";
import { Kysely, type Migration, type MigrationProvider, Migrator, PostgresDialect } from "kysely";
import pg from "pg";
import { type Config, loadConfigFor } from "../config.js";
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

/** The variables a migration run uses: the stores' and the retention's (SPEC-023 §Data
 *  contracts §4), all checked before anything is changed. */
export const MIGRATE_VARIABLES = [
  "INGEST_PG_URL",
  "INGEST_CH_URL",
  "INGEST_CH_USER",
  "INGEST_CH_PASSWORD",
  "INGEST_CH_DB",
  "INGEST_RETENTION_PROCESS_DAYS",
  "INGEST_RETENTION_NETWORK_DAYS",
  "INGEST_RETENTION_LOGON_DAYS",
  "INGEST_RETENTION_HEARTBEAT_DAYS",
] as const;
export type MigrateConfig = Pick<Config, (typeof MIGRATE_VARIABLES)[number]>;

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

/** Ingest's schema (SPEC-023 §Operational §4): its tables and its migration ledger. */
export const INGEST_SCHEMA = "ingest";

/**
 * Apply ingest's Postgres migrations under an advisory lock, so concurrent instances cannot
 * double-apply. The lock is taken and released on one connection, held apart from the
 * migrator's. Without `wait`, a run that finds the lock held skips and returns false; with
 * it, the run waits for the lock, so a command that completes has applied the migrations,
 * and fails when the lock is still held after `waitMs`.
 * `underLock`, when given, runs after the migrations and before the lock is released.
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
  underLock?: () => Promise<void>,
): Promise<boolean> {
  // A run that waits bounds each lock wait of every connection (SPEC-023's second Amendment
  // 2026-10-26, item 1): its own lock's, the migrator's and each migration statement's. The
  // bound is set before the pool hands a connection out, so that a connection it cannot be
  // set on fails the run instead of waiting without bound.
  const ms = boundMs(options);
  const pool = new pg.Pool({
    connectionString: config.INGEST_PG_URL,
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
  // Queued on each new client before any query of the migrator's.
  pool.on("connect", (client) => {
    client.query(`SET search_path TO ${INGEST_SCHEMA}, public`).catch(() => {});
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
        // A lock wait of a run that waits ran out. Kysely ran the pending migrations in one
        // transaction, which this failure rolled back: the run committed none of them. Any
        // other failure is reported as before (SPEC-023's second Amendment 2026-10-26, item 1).
        if (options.wait && lockTimedOut(error)) {
          throw new Error(
            `a lock a migration waits for is held by another session: waited ${Math.round(ms / 1000)} s`,
          );
        }
        for (const r of results ?? []) {
          if (r.status === "Error") {
            throw new Error(`migration failed: ${r.migrationName}`);
          }
        }
        if (error) {
          throw error instanceof Error ? error : new Error(String(error));
        }
        await underLock?.();
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
 * were skipped, the ClickHouse tables and the retention (`./clickhouse-bootstrap.ts`), under
 * the same lock, so two runs set the retention one after the other. Idempotent.
 */
export async function runMigrations(
  config: MigrateConfig,
  options: MigrateOptions = {},
): Promise<void> {
  await runPgMigrations(config, options, () => bootstrapClickHouse(config));
}

async function bootstrapClickHouse(config: MigrateConfig): Promise<void> {
  const ch = createClient({
    url: config.INGEST_CH_URL,
    username: config.INGEST_CH_USER,
    password: config.INGEST_CH_PASSWORD,
    database: config.INGEST_CH_DB,
  });
  try {
    await bootstrapClickHouseTables(ch, {
      processDays: config.INGEST_RETENTION_PROCESS_DAYS,
      networkDays: config.INGEST_RETENTION_NETWORK_DAYS,
      logonDays: config.INGEST_RETENTION_LOGON_DAYS,
      heartbeatDays: config.INGEST_RETENTION_HEARTBEAT_DAYS,
    });
  } finally {
    await ch.close();
  }
}

/** The command's one line for a failure: the error's message, without a class name's prefix,
 *  so that the lock's line reads as SPEC-023 Amendment 2026-10-25, item 1, quotes it. */
export function migrateFatalLine(e: unknown): string {
  const reason = e instanceof Error && e.message !== "" ? e.message : String(e);
  return `cg-ingest migrate: fatal: ${reason}`;
}

// Allow `tsx src/db/migrate.ts` / `node dist/db/migrate.js` to run directly: the one-shot's
// command, which validates the variables it uses, the stores' and the retention's, and waits
// for the lock.
if (isMain(import.meta.url)) {
  Promise.resolve()
    .then(() => runMigrations(loadConfigFor(MIGRATE_VARIABLES), { wait: true }))
    .then(() => process.exit(0))
    .catch((e: unknown) => {
      process.stderr.write(`${migrateFatalLine(e)}\n`);
      process.exit(1);
    });
}
