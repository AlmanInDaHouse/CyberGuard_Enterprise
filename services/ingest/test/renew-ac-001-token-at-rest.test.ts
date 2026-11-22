import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { Kysely, Migrator, PostgresDialect } from "kysely";
import pg from "pg";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import {
  createPgDatabase,
  dropPgDatabase,
  ingestMigrationProvider,
  pgDatabaseUrl,
} from "./helpers/throwaway.js";

// SPEC-025 renew_ac_001 (the token at rest) [server]. Migration 0010 over rows inserted with
// plain tokens, in a throwaway database at 0009's schema: no `token` column is left, each
// row's `token_sha256` is the SHA-256 of its former token, an issued row's `uses` is 0 and a
// consumed one's 1, a single-use row of 15 minutes and 2 seconds now ends 15 minutes after
// its `issued_at`, and the agent a token enrolled names it. Then `issue-token` prints a
// token whose SHA-256 is the only form of it in the database, and refuses `--org other`
// with exit 2.

const PACKAGE_DIR = join(dirname(fileURLToPath(import.meta.url)), "..");
const DB = "cyberguard_renew_ac_001";
const LEDGER = "renew_ac_001_kysely_migration";

let config: Config;
let url: string;

beforeAll(async () => {
  config = inject("ingestConfig");
  await createPgDatabase(config.INGEST_PG_URL, DB);
  url = pgDatabaseUrl(config.INGEST_PG_URL, DB);
});

afterAll(async () => {
  await dropPgDatabase(config.INGEST_PG_URL, DB);
});

function sha256(text: string): Buffer {
  return createHash("sha256").update(text, "ascii").digest();
}

function plainToken(): string {
  return Buffer.from(globalThis.crypto.getRandomValues(new Uint8Array(32))).toString("base64url");
}

/** The process environment without the INGEST_* variables a developer's shell may set. */
function baseEnv(): NodeJS.ProcessEnv {
  return Object.fromEntries(Object.entries(process.env).filter(([k]) => !k.startsWith("INGEST_")));
}

function issueTokenCli(argv: string[]) {
  const r = spawnSync(process.execPath, ["--import", "tsx", "src/cli/issue-token.ts", ...argv], {
    cwd: PACKAGE_DIR,
    encoding: "utf8",
    env: { ...baseEnv(), INGEST_PG_URL: url },
    timeout: 60_000,
  });
  return { status: r.status, stdout: r.stdout, stderr: r.stderr };
}

test("renew_ac_001: the migration hashes the plain tokens, counts their uses and names the enrolling token", async () => {
  const pool = new pg.Pool({ connectionString: url });
  pool.on("error", () => {});
  const db = new Kysely<unknown>({ dialect: new PostgresDialect({ pool }) });
  try {
    const migrator = new Migrator({
      db,
      provider: ingestMigrationProvider(),
      migrationTableName: LEDGER,
      migrationLockTableName: `${LEDGER}_lock`,
      migrationTableSchema: "ingest",
    });
    const pre = await migrator.migrateTo("0009_runtime_grants");
    if (pre.error) throw pre.error instanceof Error ? pre.error : new Error(String(pre.error));

    // The state before 0010: plain tokens, one consumed by an agent.
    const agentId = "01934abc-def0-7000-89ab-0000000a0001";
    await pool.query(
      `INSERT INTO agents (agent_id, pubkey, cert_pem, expires_at)
       VALUES ($1, $2, 'test-cert', now() + interval '1 day')`,
      [agentId, Buffer.from([0])],
    );
    const issued = plainToken();
    const consumed = plainToken();
    await pool.query(
      `INSERT INTO enrollment_tokens (token, org_id, scope, state, issued_at, expires_at)
       VALUES ($1, 'default', 'enroll', 'issued', '2026-11-22 10:00:00+00',
               timestamptz '2026-11-22 10:00:00+00' + interval '15 minutes 2 seconds')`,
      [issued],
    );
    await pool.query(
      `INSERT INTO enrollment_tokens
         (token, org_id, scope, state, issued_at, expires_at, consumed_at, consumed_by_agent_id)
       VALUES ($1, 'default', 'enroll', 'consumed', '2026-11-22 11:00:00+00',
               '2026-11-22 11:15:00+00', '2026-11-22 11:05:00+00', $2)`,
      [consumed, agentId],
    );

    const post = await migrator.migrateToLatest();
    if (post.error) throw post.error instanceof Error ? post.error : new Error(String(post.error));
    expect((post.results ?? []).map((r) => r.migrationName)).toContain("0010_agent_certificates");

    const columns = await pool.query<{ column_name: string }>(
      `SELECT column_name FROM information_schema.columns
        WHERE table_schema = 'ingest' AND table_name = 'enrollment_tokens'`,
    );
    const names = columns.rows.map((r) => r.column_name);
    expect(names).not.toContain("token");
    expect(names).not.toContain("consumed_by_agent_id");

    const rows = await pool.query<{
      token_id: string;
      token_sha256: Buffer;
      uses: number;
      max_uses: number;
      state: string;
      ends_after_issue: string;
    }>(
      `SELECT token_id, token_sha256, uses, max_uses, state::text AS state,
              (expires_at - issued_at)::text AS ends_after_issue
         FROM enrollment_tokens`,
    );
    const byHash = (t: string) => rows.rows.find((r) => r.token_sha256.equals(sha256(t)));
    const issuedRow = byHash(issued);
    const consumedRow = byHash(consumed);
    expect(issuedRow, "the issued token's row is found by its SHA-256").toBeDefined();
    expect(consumedRow, "the consumed token's row is found by its SHA-256").toBeDefined();
    expect(issuedRow?.uses).toBe(0);
    expect(issuedRow?.max_uses).toBe(1);
    expect(issuedRow?.ends_after_issue).toBe("00:15:00");
    expect(consumedRow?.uses).toBe(1);
    expect(consumedRow?.state).toBe("consumed");

    const agent = await pool.query<{ enrolled_by_token_id: string | null }>(
      "SELECT enrolled_by_token_id FROM agents WHERE agent_id = $1",
      [agentId],
    );
    expect(agent.rows[0]?.enrolled_by_token_id).toBe(consumedRow?.token_id);
  } finally {
    await db.destroy();
  }
});

test("renew_ac_001: issue-token prints a token whose SHA-256 is its only form in the database", async () => {
  const r = issueTokenCli([]);
  expect(r.status, r.stderr).toBe(0);
  const token = r.stdout.trim();
  expect(r.stdout).toBe(`${token}\n`);
  expect(token).toMatch(/^[A-Za-z0-9_-]{43}$/);

  const pool = new pg.Pool({ connectionString: url });
  pool.on("error", () => {});
  try {
    const row = await pool.query<{
      token_id: string;
      org_id: string;
      scope: string;
      state: string;
      uses: number;
      max_uses: number;
      lifetime: string;
    }>(
      `SELECT token_id, org_id, scope, state::text AS state, uses, max_uses,
              (expires_at - issued_at)::text AS lifetime
         FROM enrollment_tokens WHERE token_sha256 = $1`,
      [sha256(token)],
    );
    expect(row.rows).toHaveLength(1);
    const stored = row.rows[0];
    expect(stored).toMatchObject({
      org_id: "default",
      scope: "enroll",
      state: "issued",
      uses: 0,
      max_uses: 1,
      lifetime: "00:15:00",
    });
    // Its identifier and expiry go to standard error.
    expect(r.stderr).toContain(stored?.token_id as string);
    // The token's text is in no column of any row.
    const plain = await pool.query<{ n: string }>(
      "SELECT count(*) AS n FROM enrollment_tokens t WHERE position($1 in t::text) > 0",
      [token],
    );
    expect(Number(plain.rows[0]?.n)).toBe(0);
  } finally {
    await pool.end();
  }
});

test("renew_ac_001: issue-token refuses --org other with exit 2, writing nothing", async () => {
  const pool = new pg.Pool({ connectionString: url });
  pool.on("error", () => {});
  try {
    const count = async () =>
      Number(
        (await pool.query<{ n: string }>("SELECT count(*) AS n FROM enrollment_tokens")).rows[0]?.n,
      );
    const before = await count();
    const r = issueTokenCli(["--org", "other"]);
    expect(r.status, r.stderr).toBe(2);
    expect(r.stdout).toBe("");
    expect(await count()).toBe(before);
  } finally {
    await pool.end();
  }
});
