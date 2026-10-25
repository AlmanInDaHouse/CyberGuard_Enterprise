import { spawn, spawnSync } from "node:child_process";
import { X509Certificate, createHash } from "node:crypto";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import pg from "pg";
import { afterAll, beforeAll, describe, expect, inject, test } from "vitest";
import { ensureCa } from "../src/ca.js";
import type { Config } from "../src/config.js";
import {
  MIGRATION_LOCK_KEY,
  MIGRATION_LOCK_WAIT_MS,
  migrateFatalLine,
  runMigrations,
} from "../src/db/migrate.js";
import { createPgDatabase, dropPgDatabase, pgDatabaseUrl } from "./helpers/throwaway.js";

// SPEC-023 §Data contracts §4, ingest's commands, and deploy_ac_009's first half:
// `export-ca.js` prints the CA certificate as PEM and a fingerprint equal to the SHA-256 of
// its DER; `issue-token.js` prints a token, validating INGEST_PG_URL alone; `migrate.js`,
// the one-shot's command, waits for its advisory lock, taken and released on one
// connection, while the service's own run keeps skipping. The bundle's half (an agent
// enrolling with the exported CA) is the bundle's test.

const PACKAGE_DIR = join(dirname(fileURLToPath(import.meta.url)), "..");

let config: Config;
let holder: pg.Client;

beforeAll(async () => {
  config = inject("ingestConfig");
  holder = new pg.Client({ connectionString: config.INGEST_PG_URL });
  await holder.connect();
  // The CA ingest generates at its first start; a test that starts ingest may not have run yet.
  const pool = new pg.Pool({ connectionString: config.INGEST_PG_URL });
  try {
    await ensureCa(pool, config.INGEST_CA_PASSPHRASE);
  } finally {
    await pool.end();
  }
});
afterAll(async () => {
  await holder?.end();
});

/** The process environment without the INGEST_* variables a developer's shell may set. */
function baseEnv(): NodeJS.ProcessEnv {
  return Object.fromEntries(Object.entries(process.env).filter(([k]) => !k.startsWith("INGEST_")));
}

function run(script: string, argv: string[], env: Record<string, string>) {
  const r = spawnSync(process.execPath, ["--import", "tsx", script, ...argv], {
    cwd: PACKAGE_DIR,
    encoding: "utf8",
    env: { ...baseEnv(), ...env },
    timeout: 60_000,
  });
  return { status: r.status, stdout: r.stdout, stderr: r.stderr };
}

describe("export-ca.js", () => {
  test("prints the stored CA certificate and the SHA-256 of its DER, from INGEST_PG_URL alone", async () => {
    const r = run("src/cli/export-ca.ts", [], { INGEST_PG_URL: config.INGEST_PG_URL });
    expect(r.status, r.stderr).toBe(0);
    const stored = (
      await holder.query<{ cert_pem: string }>("SELECT cert_pem FROM ca WHERE id = 1")
    ).rows[0]?.cert_pem as string;
    const lines = r.stdout.trimEnd().split("\n");
    const fingerprint = lines.pop();
    expect(`${lines.join("\n")}\n`).toBe(stored.endsWith("\n") ? stored : `${stored}\n`);
    const der = new X509Certificate(stored).raw;
    expect(fingerprint).toBe(`sha256:${createHash("sha256").update(der).digest("hex")}`);
  });
});

test("a malformed INGEST_PG_URL is refused with its name, exit 1", () => {
  for (const script of ["src/cli/export-ca.ts", "src/cli/issue-token.ts"]) {
    const r = run(script, [], { INGEST_PG_URL: "not a url" });
    expect(r.status, script).toBe(1);
    expect(r.stderr).toContain("invalid configuration: INGEST_PG_URL: must be a URL");
  }
});

test("export-ca.js, before ingest has generated its CA, says so and exits 1", async () => {
  const db = `deploy_noca_${Date.now()}`;
  await createPgDatabase(config.INGEST_PG_URL, db);
  try {
    const url = pgDatabaseUrl(config.INGEST_PG_URL, db);
    await runMigrations({ ...config, INGEST_PG_URL: url });
    const r = run("src/cli/export-ca.ts", [], { INGEST_PG_URL: url });
    expect(r.status).toBe(1);
    expect(r.stdout).toBe("");
    expect(r.stderr).toContain("no CA yet");
  } finally {
    await dropPgDatabase(config.INGEST_PG_URL, db);
  }
});

describe("issue-token.js", () => {
  test("prints a token it stored, from INGEST_PG_URL alone", async () => {
    const r = run("src/cli/issue-token.ts", ["--org", "deploy-ac-009"], {
      INGEST_PG_URL: config.INGEST_PG_URL,
    });
    expect(r.status, r.stderr).toBe(0);
    const token = r.stdout.trim();
    expect(token).toMatch(/^[A-Za-z0-9_-]{43}$/);
    const rows = await holder.query<{ org_id: string; state: string }>(
      "SELECT org_id, state FROM enrollment_tokens WHERE token = $1",
      [token],
    );
    expect(rows.rows).toEqual([{ org_id: "deploy-ac-009", state: "issued" }]);
  });
});

describe("migrate.js", () => {
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

  /** The advisory locks on the migration key that any other session holds. */
  async function heldLocks(): Promise<number> {
    const r = await holder.query<{ n: string }>(
      `SELECT count(*) AS n FROM pg_locks
        WHERE locktype = 'advisory' AND granted AND objid = $1 AND pid <> pg_backend_pid()`,
      [MIGRATION_LOCK_KEY],
    );
    return Number(r.rows[0]?.n ?? 0);
  }

  test("waits for the lock another session holds, then migrates and exits 0, from the stores' variables alone", async () => {
    await holder.query("SELECT pg_advisory_lock($1)", [MIGRATION_LOCK_KEY]);
    const child = spawn(process.execPath, ["--import", "tsx", "src/db/migrate.ts"], {
      cwd: PACKAGE_DIR,
      env: {
        ...baseEnv(),
        INGEST_PG_URL: config.INGEST_PG_URL,
        INGEST_CH_URL: config.INGEST_CH_URL,
        INGEST_CH_USER: config.INGEST_CH_USER,
        INGEST_CH_PASSWORD: config.INGEST_CH_PASSWORD,
        INGEST_CH_DB: config.INGEST_CH_DB,
      },
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
    expect(await heldLocks()).toBe(0);
  });

  test("a command that waited and completed has applied the migrations, in a database it found empty", async () => {
    const db = `deploy_migrate_${Date.now()}`;
    await createPgDatabase(config.INGEST_PG_URL, db);
    const url = pgDatabaseUrl(config.INGEST_PG_URL, db);
    const other = new pg.Client({ connectionString: url });
    other.on("error", () => {});
    await other.connect();
    try {
      // Advisory locks are the database's: this one is held in the empty database.
      await other.query("SELECT pg_advisory_lock($1)", [MIGRATION_LOCK_KEY]);
      const child = spawn(process.execPath, ["--import", "tsx", "src/db/migrate.ts"], {
        cwd: PACKAGE_DIR,
        env: {
          ...baseEnv(),
          INGEST_PG_URL: url,
          INGEST_CH_URL: config.INGEST_CH_URL,
          INGEST_CH_USER: config.INGEST_CH_USER,
          INGEST_CH_PASSWORD: config.INGEST_CH_PASSWORD,
          INGEST_CH_DB: config.INGEST_CH_DB,
        },
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
        "SELECT count(*) AS n FROM pg_tables WHERE tablename IN ('agents', 'alerts', 'incidents', 'playbook_executions')",
      );
      expect(Number(tables.rows[0]?.n)).toBe(4);
    } finally {
      await other.end();
      await dropPgDatabase(config.INGEST_PG_URL, db);
    }
  });

  test.each([["src/db/migrate.ts"], ["src/db/migrate"]])(
    "the command, run as %s, exits non-zero when it cannot migrate, and names no password",
    (script) => {
      // Node runs a script named without its extension; the command must run, not exit 0.
      const r = run(script, [], {
        INGEST_PG_URL: "postgres://u:s3cr3t-009@127.0.0.1:1/db",
        INGEST_CH_URL: config.INGEST_CH_URL,
      });
      expect(r.status, r.stderr).toBe(1);
      expect(r.stderr).toMatch(/^cg-ingest migrate: fatal: /);
      expect(r.stderr).not.toContain("s3cr3t-009");
    },
  );

  test("a malformed store variable is refused before anything is changed", async () => {
    const db = `deploy_migrate_bad_${Date.now()}`;
    await createPgDatabase(config.INGEST_PG_URL, db);
    try {
      const r = run("src/db/migrate.ts", [], {
        INGEST_PG_URL: pgDatabaseUrl(config.INGEST_PG_URL, db),
        INGEST_CH_URL: "not a url",
      });
      expect(r.status).toBe(1);
      expect(r.stderr.trim()).toBe(
        "cg-ingest migrate: fatal: invalid configuration: INGEST_CH_URL: must be a URL",
      );
      const other = new pg.Client({ connectionString: pgDatabaseUrl(config.INGEST_PG_URL, db) });
      other.on("error", () => {});
      await other.connect();
      try {
        const tables = await other.query<{ n: string }>(
          "SELECT count(*) AS n FROM pg_tables WHERE schemaname NOT IN ('pg_catalog', 'information_schema')",
        );
        expect(Number(tables.rows[0]?.n)).toBe(0);
      } finally {
        await other.end();
      }
    } finally {
      await dropPgDatabase(config.INGEST_PG_URL, db);
    }
  });

  test("the service's own run skips while another session holds the lock", async () => {
    await holder.query("SELECT pg_advisory_lock($1)", [MIGRATION_LOCK_KEY]);
    try {
      const start = Date.now();
      await runMigrations(config);
      expect(Date.now() - start).toBeLessThan(10_000);
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
    await createPgDatabase(config.INGEST_PG_URL, db);
    const url = pgDatabaseUrl(config.INGEST_PG_URL, db);
    const other = new pg.Client({ connectionString: url });
    other.on("error", () => {});
    await other.connect();
    try {
      // Advisory locks are the database's: this one is held in the empty database.
      await other.query("SELECT pg_advisory_lock($1)", [MIGRATION_LOCK_KEY]);
      const start = Date.now();
      // The command's run: the Postgres migrations, then ClickHouse's under the same lock.
      const failed = await runMigrations(
        { ...config, INGEST_PG_URL: url },
        { wait: true, waitMs: 2000 },
      ).then(
        () => null,
        (e: unknown) => e,
      );
      // The command's line for it, as SPEC-023 Amendment 2026-10-25 quotes it.
      expect(migrateFatalLine(failed)).toBe(
        "cg-ingest migrate: fatal: the migration lock is held by another session: waited 2 s",
      );
      const elapsed = Date.now() - start;
      // Measured by Postgres in the Docker VM, whose clock steps against the host's: it
      // waited, rather than failing at once.
      expect(elapsed).toBeGreaterThanOrEqual(1000);
      expect(elapsed).toBeLessThan(15_000);
      // Having changed nothing: no table, no schema of ingest's.
      const changed = await other.query<{ tables: string; schemas: string }>(
        `SELECT (SELECT count(*) FROM pg_tables
                   WHERE schemaname NOT IN ('pg_catalog', 'information_schema')) AS tables,
                (SELECT count(*) FROM pg_namespace WHERE nspname = 'ingest') AS schemas`,
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
      await dropPgDatabase(config.INGEST_PG_URL, db);
    }
  });
});
