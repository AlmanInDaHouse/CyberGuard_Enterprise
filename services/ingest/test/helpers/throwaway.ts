import * as path from "node:path";
import { fileURLToPath } from "node:url";
import { type ClickHouseClient, createClient } from "@clickhouse/client";
import type { MigrationProvider } from "kysely";
import pg from "pg";
import type { Config } from "../../src/config.js";
import { folderMigrationProvider, runMigrations } from "../../src/db/migrate.js";

// A throwaway database (debt #8). A test that runs a migration, or the ClickHouse
// bootstrap, against a schema older than the latest does it in a database of its own:
// the suite's shared databases are at the latest schema (global-setup) and other files
// read them. The test file creates the database afresh before its tests, dropping one a
// failed run left, and drops it after them.

const INGEST_MIGRATIONS = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  "../../src/db/migrations",
);

/** A database name is written into SQL unquoted, and Postgres folds an unquoted name to
 *  lower case: only a lowercase identifier is accepted. */
function checkedName(database: string): string {
  if (!/^[a-z_][a-z0-9_]*$/.test(database)) {
    throw new Error(`throwaway database name must be a lowercase identifier: ${database}`);
  }
  return database;
}

/** Ingest's Postgres migrations, read as `runMigrations` reads them. */
export function ingestMigrationProvider(): MigrationProvider {
  return folderMigrationProvider(INGEST_MIGRATIONS);
}

/** `baseUrl`, a Postgres URL, with its database replaced by `database`. */
export function pgDatabaseUrl(baseUrl: string, database: string): string {
  const u = new URL(baseUrl);
  u.pathname = `/${checkedName(database)}`;
  return u.toString();
}

/** Creates the Postgres database `database` on `baseUrl`'s server, dropping a leftover
 *  one first. */
export async function createPgDatabase(baseUrl: string, database: string): Promise<void> {
  checkedName(database);
  const admin = new pg.Pool({ connectionString: baseUrl });
  try {
    await admin.query(`DROP DATABASE IF EXISTS ${database} WITH (FORCE)`);
    await admin.query(`CREATE DATABASE ${database}`);
  } finally {
    await admin.end();
  }
}

/** Drops the Postgres database `database` on `baseUrl`'s server. */
export async function dropPgDatabase(baseUrl: string, database: string): Promise<void> {
  checkedName(database);
  const admin = new pg.Pool({ connectionString: baseUrl });
  try {
    await admin.query(`DROP DATABASE IF EXISTS ${database} WITH (FORCE)`);
  } finally {
    await admin.end();
  }
}

/** Runs `fn` with a client of ingest's ClickHouse whose default database is `database`,
 *  and closes the client after it. */
export async function withChClient<T>(
  config: Config,
  database: string,
  fn: (ch: ClickHouseClient) => Promise<T>,
): Promise<T> {
  const ch = createClient({
    url: config.INGEST_CH_URL,
    username: config.INGEST_CH_USER,
    password: config.INGEST_CH_PASSWORD,
    database,
  });
  try {
    return await fn(ch);
  } finally {
    await ch.close();
  }
}

/** Creates the ClickHouse database `database`, dropping a leftover one first. */
export async function createChDatabase(config: Config, database: string): Promise<void> {
  checkedName(database);
  await withChClient(config, config.INGEST_CH_DB, async (ch) => {
    await ch.command({ query: `DROP DATABASE IF EXISTS ${database}` });
    await ch.command({ query: `CREATE DATABASE ${database}` });
  });
}

/** Drops the ClickHouse database `database`. */
export async function dropChDatabase(config: Config, database: string): Promise<void> {
  checkedName(database);
  await withChClient(config, config.INGEST_CH_DB, async (ch) => {
    await ch.command({ query: `DROP DATABASE IF EXISTS ${database}` });
  });
}

/** Ingest's real bootstrap (`runMigrations`) with its ClickHouse tables in `database`;
 *  the Postgres migrations run against the shared database, already at the latest.
 *  `runMigrations` skips both when another holder has its advisory lock; the suite runs
 *  in one fork (`vitest.config.ts`), so none does. */
export async function bootstrapChDatabase(config: Config, database: string): Promise<void> {
  await runMigrations({ ...config, INGEST_CH_DB: database });
}
