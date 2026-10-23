import { spawn } from "node:child_process";
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
    // Long enough for a skipping command to have exited.
    await new Promise((r) => setTimeout(r, 4000));
    expect(exitCode, stderr).toBeUndefined();
  } finally {
    await holder.query("SELECT pg_advisory_unlock($1)", [MIGRATION_LOCK_KEY]);
  }
  expect(await exited, stderr).toBe(0);
  // Released on the connection that took it: no session holds it after the exit.
  expect(await heldLocks()).toBe(0);
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
