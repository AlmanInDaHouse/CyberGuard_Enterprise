import { spawn, spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import pg from "pg";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { MIGRATION_LOCK_KEY, runMigrations } from "../src/db/migrate.js";

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
