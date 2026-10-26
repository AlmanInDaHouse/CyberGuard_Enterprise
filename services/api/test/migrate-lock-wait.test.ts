import { spawn, spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import pg from "pg";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import {
  MIGRATION_LOCK_KEY,
  MIGRATION_LOCK_WAIT_MS,
  migrateFatalLine,
  runMigrations,
} from "../src/db/migrate.js";

// SPEC-023 §Data contracts §4 — `node dist/db/migrate.js`, the one-shot's command, waits
// for its advisory lock, taken and released on one connection, rather than skipping when
// another process holds it, so that a one-shot that completes has applied the migrations;
// the service's own run (API_RUN_MIGRATIONS=true) keeps skipping. The command validates
// the variables it uses alone: API_PG_URL.

const PACKAGE_DIR = join(dirname(fileURLToPath(import.meta.url)), "..");

let config: Config;
let holder: pg.Client;

beforeAll(async () => {
  config = inject("apiConfig");
  holder = new pg.Client({ connectionString: config.API_PG_URL });
  await holder.connect();
});
afterAll(async () => {
  await holder?.end();
});

/** The process environment without the API_* variables a developer's shell may set. */
function withoutApiVars(): NodeJS.ProcessEnv {
  return Object.fromEntries(Object.entries(process.env).filter(([k]) => !k.startsWith("API_")));
}

/** Whether, within 30 s, a session other than `client`'s waits for the migration lock. */
async function waitingFor(client: pg.Client): Promise<boolean> {
  const start = Date.now();
  while (Date.now() - start < 30_000) {
    const r = await client.query<{ n: string }>(
      `SELECT count(*) AS n FROM pg_locks
        WHERE locktype = 'advisory' AND NOT granted AND objid = $1 AND pid <> pg_backend_pid()`,
      [MIGRATION_LOCK_KEY],
    );
    if (Number(r.rows[0]?.n ?? 0) > 0) return true;
    await new Promise((r) => setTimeout(r, 100));
  }
  return false;
}

/** The advisory locks on the migration key that any session holds. */
async function heldLocks(): Promise<number> {
  const r = await holder.query<{ n: string }>(
    `SELECT count(*) AS n FROM pg_locks
      WHERE locktype = 'advisory' AND granted AND objid = $1 AND pid <> pg_backend_pid()`,
    [MIGRATION_LOCK_KEY],
  );
  return Number(r.rows[0]?.n ?? 0);
}

test("the command waits for the lock another session holds, then migrates and exits 0", async () => {
  await holder.query("SELECT pg_advisory_lock($1)", [MIGRATION_LOCK_KEY]);
  const env = Object.fromEntries(
    Object.entries(process.env).filter(([k]) => !k.startsWith("API_")),
  );
  const child = spawn(process.execPath, ["--import", "tsx", "src/db/migrate.ts"], {
    cwd: PACKAGE_DIR,
    env: { ...env, API_PG_URL: config.API_PG_URL },
  });
  let stderr = "";
  child.stderr.setEncoding("utf8").on("data", (d: string) => {
    stderr += d;
  });
  let exitCode: number | null | undefined;
  const exited = new Promise<number | null>((resolve) =>
    child.on("exit", (code) => {
      exitCode = code;
      resolve(code);
    }),
  );
  try {
    // The command's request for the lock, seen waiting, not granted.
    expect(await waitingFor(holder), stderr).toBe(true);
    expect(exitCode, stderr).toBeUndefined();
  } finally {
    await holder.query("SELECT pg_advisory_unlock($1)", [MIGRATION_LOCK_KEY]);
  }
  expect(await exited, stderr).toBe(0);
  // Released on the connection that took it: no session holds it after the exit.
  expect(await heldLocks()).toBe(0);
});

test("a command that waited and completed has applied the migrations, in a database it found empty", async () => {
  const db = `deploy_migrate_${Date.now()}`;
  await holder.query(`CREATE DATABASE ${db}`);
  const url = new URL(config.API_PG_URL);
  url.pathname = `/${db}`;
  const other = new pg.Client({ connectionString: url.toString() });
  // The drop of the throwaway database ends a backend the pool's end left closing: heard, not
  // an uncaught error event.
  other.on("error", () => {});
  await other.connect();
  try {
    // Advisory locks are the database's: this one is held in the empty database.
    await other.query("SELECT pg_advisory_lock($1)", [MIGRATION_LOCK_KEY]);
    const child = spawn(process.execPath, ["--import", "tsx", "src/db/migrate.ts"], {
      cwd: PACKAGE_DIR,
      env: { ...withoutApiVars(), API_PG_URL: url.toString() },
    });
    let stderr = "";
    child.stderr.setEncoding("utf8").on("data", (d: string) => {
      stderr += d;
    });
    const exited = new Promise<number | null>((resolve) => child.on("exit", resolve));
    expect(await waitingFor(other), stderr).toBe(true);
    await other.query("SELECT pg_advisory_unlock($1)", [MIGRATION_LOCK_KEY]);
    expect(await exited, stderr).toBe(0);
    const tables = await other.query<{ n: string }>(
      "SELECT count(*) AS n FROM pg_tables WHERE tablename IN ('users', 'audit_log', 'forensic_key')",
    );
    expect(Number(tables.rows[0]?.n)).toBe(3);
  } finally {
    await other.end();
    await holder.query(`DROP DATABASE IF EXISTS ${db} WITH (FORCE)`);
  }
});

test.each([["src/db/migrate.ts"], ["src/db/migrate"]])(
  "the command, run as %s, exits non-zero when it cannot migrate, and names no password",
  (script) => {
    // Node runs a script named without its extension; the command must run, not exit 0.
    const r = spawnSync(process.execPath, ["--import", "tsx", script], {
      cwd: PACKAGE_DIR,
      encoding: "utf8",
      env: { ...withoutApiVars(), API_PG_URL: "postgres://u:s3cr3t-016@127.0.0.1:1/db" },
      timeout: 60_000,
    });
    expect(r.status, r.stderr).toBe(1);
    expect(r.stderr).toMatch(/^cg-api migrate: fatal: /);
    expect(r.stderr).not.toContain("s3cr3t-016");
  },
);

test("a malformed API_PG_URL is refused before anything is done, its value not quoted", () => {
  const r = spawnSync(process.execPath, ["--import", "tsx", "src/db/migrate.ts"], {
    cwd: PACKAGE_DIR,
    encoding: "utf8",
    env: { ...withoutApiVars(), API_PG_URL: "not a url s3cr3t" },
    timeout: 60_000,
  });
  expect(r.status).toBe(1);
  expect(r.stderr.trim()).toBe(
    "cg-api migrate: fatal: invalid configuration: API_PG_URL: must be a URL",
  );
});

test("the service's own run skips while another session holds the lock", async () => {
  await holder.query("SELECT pg_advisory_lock($1)", [MIGRATION_LOCK_KEY]);
  try {
    const start = Date.now();
    await runMigrations(config);
    expect(Date.now() - start).toBeLessThan(3000);
  } finally {
    await holder.query("SELECT pg_advisory_unlock($1)", [MIGRATION_LOCK_KEY]);
  }
});

test("the service's own run releases the lock it took", async () => {
  await runMigrations(config);
  expect(await heldLocks()).toBe(0);
});

// SPEC-023 Amendment 2026-10-25 (debt #122): the command waits up to five minutes, then
// fails naming the lock, having changed nothing, rather than holding `docker compose up
// --wait` without bound.
test("the wait is bounded: a lock still held after it fails the run, naming the lock, having changed nothing", async () => {
  expect(MIGRATION_LOCK_WAIT_MS).toBe(300_000);
  const db = `deploy_migrate_wait_${Date.now()}`;
  await holder.query(`CREATE DATABASE ${db}`);
  const url = new URL(config.API_PG_URL);
  url.pathname = `/${db}`;
  const other = new pg.Client({ connectionString: url.toString() });
  other.on("error", () => {});
  await other.connect();
  try {
    // Advisory locks are the database's: this one is held in the empty database.
    await other.query("SELECT pg_advisory_lock($1)", [MIGRATION_LOCK_KEY]);
    const start = Date.now();
    const failed = await runMigrations(
      { API_PG_URL: url.toString() },
      { wait: true, waitMs: 2000 },
    ).then(
      () => null,
      (e: unknown) => e,
    );
    // The command's line for it, as SPEC-023 Amendment 2026-10-25 quotes it.
    expect(migrateFatalLine(failed)).toBe(
      "cg-api migrate: fatal: the migration lock is held by another session: waited 2 s",
    );
    const elapsed = Date.now() - start;
    // Measured by Postgres in the Docker VM, whose clock steps against the host's: it
    // waited, rather than failing at once.
    expect(elapsed).toBeGreaterThanOrEqual(1000);
    expect(elapsed).toBeLessThan(15_000);
    // Having changed nothing: no table, no schema of the api's.
    const changed = await other.query<{ tables: string; schemas: string }>(
      `SELECT (SELECT count(*) FROM pg_tables
                 WHERE schemaname NOT IN ('pg_catalog', 'information_schema')) AS tables,
              (SELECT count(*) FROM pg_namespace WHERE nspname = 'api') AS schemas`,
    );
    expect(changed.rows[0]).toEqual({ tables: "0", schemas: "0" });
    // The run that gave up holds nothing: the only advisory lock is this session's.
    const held = await other.query<{ n: string }>(
      `SELECT count(*) AS n FROM pg_locks
        WHERE locktype = 'advisory' AND pid <> pg_backend_pid()
          AND database = (SELECT oid FROM pg_database WHERE datname = current_database())`,
    );
    expect(Number(held.rows[0]?.n)).toBe(0);
  } finally {
    await other.end();
    await holder.query(`DROP DATABASE IF EXISTS ${db} WITH (FORCE)`);
  }
});

// SPEC-023's second Amendment 2026-10-26, item 1 (debts #122, #127): every lock the run
// waits for is bounded, not only its own: the one the migrator takes before the migrations,
// and a lock on a table a migration changes. A wait that runs out fails the run with the line
// for a migration's lock, having committed none of the run's migrations.

/** The lock Kysely's migrator takes before the migrations: the transaction-level advisory key
 *  of its Postgres adapter (kysely 0.28, PostgresAdapter.acquireMigrationLock). */
const KYSELY_LOCK_KEY = "3853314791062309107";

/** The migrations a run finds pending in `withPending`'s database. */
const PENDING = ["0005_audit_log_detail", "0006_users_authority_version", "0007_runtime_grants"];

/**
 * A database with the api's migrations applied, and then the ledger's rows of the last three
 * removed, so that a run has them pending. Over the applied schema they change nothing
 * (`ADD COLUMN IF NOT EXISTS`, and grants to a role the tests' cluster has not), but 0006 alters
 * `users`, so it waits for any lock another session holds on it.
 */
async function withPending(
  name: string,
): Promise<{ url: string; other: pg.Client; drop: () => Promise<void> }> {
  const db = `deploy_migrate_${name}_${Date.now()}`;
  await holder.query(`CREATE DATABASE ${db}`);
  const url = new URL(config.API_PG_URL);
  url.pathname = `/${db}`;
  await runMigrations({ API_PG_URL: url.toString() });
  const other = new pg.Client({ connectionString: url.toString() });
  other.on("error", () => {});
  await other.connect();
  const removed = await other.query("DELETE FROM api.api_kysely_migration WHERE name = ANY($1)", [
    PENDING,
  ]);
  expect(removed.rowCount).toBe(PENDING.length);
  return {
    url: url.toString(),
    other,
    drop: async () => {
      await other.end();
      await holder.query(`DROP DATABASE IF EXISTS ${db} WITH (FORCE)`);
    },
  };
}

/** The ledger's names of the pending migrations, in the database `other` is connected to. */
async function applied(other: pg.Client): Promise<string[]> {
  const r = await other.query<{ name: string }>(
    "SELECT name FROM api.api_kysely_migration WHERE name = ANY($1) ORDER BY name",
    [PENDING],
  );
  return r.rows.map((row) => row.name);
}

/** Runs the command's migrations, waiting at most 2 s, and checks the failure it must give. */
async function failsWithMigrationLockLine(url: string): Promise<void> {
  const start = Date.now();
  const failed = await runMigrations({ API_PG_URL: url }, { wait: true, waitMs: 2000 }).then(
    () => null,
    (e: unknown) => e,
  );
  expect(migrateFatalLine(failed)).toBe(
    "cg-api migrate: fatal: a lock a migration waits for is held by another session: waited 2 s",
  );
  const elapsed = Date.now() - start;
  expect(elapsed).toBeGreaterThanOrEqual(1000);
  expect(elapsed).toBeLessThan(15_000);
}

test("a lock another session holds on a table a migration changes fails the run with its own line, having committed none of the run's migrations", async () => {
  const { url, other, drop } = await withPending("table");
  try {
    // A running service's write in flight: its transaction holds users in ROW EXCLUSIVE mode.
    await other.query("BEGIN");
    await other.query("LOCK TABLE api.users IN ROW EXCLUSIVE MODE");
    await failsWithMigrationLockLine(url);
    await other.query("ROLLBACK");
    // 0005 ran before 0006 waited, in the same transaction: neither is in the ledger.
    expect(await applied(other)).toEqual([]);
    // The run that gave up holds nothing, and the three were the run's to apply.
    const held = await other.query<{ n: string }>(
      `SELECT count(*) AS n FROM pg_locks
        WHERE locktype = 'advisory' AND pid <> pg_backend_pid()
          AND database = (SELECT oid FROM pg_database WHERE datname = current_database())`,
    );
    expect(Number(held.rows[0]?.n)).toBe(0);
    await runMigrations({ API_PG_URL: url }, { wait: true, waitMs: 2000 });
    expect(await applied(other)).toEqual(PENDING);
  } finally {
    await drop();
  }
});

test("the lock the migrator takes before the migrations is bounded too, with the same line, committing none of them", async () => {
  const { url, other, drop } = await withPending("migrator");
  try {
    await other.query("SELECT pg_advisory_lock($1)", [KYSELY_LOCK_KEY]);
    await failsWithMigrationLockLine(url);
    expect(await applied(other)).toEqual([]);
    await other.query("SELECT pg_advisory_unlock($1)", [KYSELY_LOCK_KEY]);
    await runMigrations({ API_PG_URL: url }, { wait: true, waitMs: 2000 });
    expect(await applied(other)).toEqual(PENDING);
  } finally {
    await drop();
  }
});

test("a run that waits fails when its bound cannot be set on a connection, rather than waiting without one", async () => {
  // Postgres refuses the value: the run must not go on without the bound.
  const failed = await runMigrations(
    { API_PG_URL: config.API_PG_URL },
    { wait: true, waitMs: Number.NaN },
  ).then(
    () => null,
    (e: unknown) => e,
  );
  expect(migrateFatalLine(failed)).toMatch(/^cg-api migrate: fatal: .*lock_timeout/);
});
