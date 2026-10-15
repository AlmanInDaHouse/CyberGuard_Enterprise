import { spawnSync } from "node:child_process";
import * as path from "node:path";
import { fileURLToPath } from "node:url";
import pg from "pg";
import { beforeAll, expect, inject, test } from "vitest";
import { CliRefusal, createUserFromCli } from "../src/cli/create-user.js";
import type { Config } from "../src/config.js";

// apiauth_ac_017 — SPEC-008 Amendment 2026-10-15 item 13: the bootstrap CLI takes its
// address, password and role through the API's schemas and password policy, refuses
// any other role, writes the user.create audit row with no acting principal, and
// fails with a conflict, writing nothing, for an address already taken.

let config: Config;

async function query<R extends pg.QueryResultRow>(text: string, values: unknown[] = []) {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    return (await pool.query<R>(text, values)).rows;
  } finally {
    await pool.end();
  }
}

beforeAll(() => {
  config = inject("apiConfig");
});

const args = (email: string, password: string, role?: string) => [
  "--email",
  email,
  "--password",
  password,
  ...(role ? ["--role", role] : []),
];

test("an unknown role is refused, and nothing is written", async () => {
  await expect(
    createUserFromCli(config, args("root@apiauth017.test", "Valid-passw0rd-017", "root")),
  ).rejects.toThrow(CliRefusal);
  expect(await query("SELECT 1 FROM users WHERE email = 'root@apiauth017.test'")).toEqual([]);
});

test("a password the policy refuses is refused, and the message does not quote it", async () => {
  const err = await createUserFromCli(config, args("weak@apiauth017.test", "short1")).then(
    () => null,
    (e: unknown) => e as Error,
  );
  expect(err).toBeInstanceOf(CliRefusal);
  expect(err?.message).toContain("password");
  expect(err?.message).not.toContain("short1");
  expect(await query("SELECT 1 FROM users WHERE email = 'weak@apiauth017.test'")).toEqual([]);
});

test("a created admin has a user.create row with its role and no acting principal", async () => {
  const created = await createUserFromCli(
    config,
    args("  First.Admin@apiauth017.test", "Valid-passw0rd-017"),
  );
  expect(created.role).toBe("admin");
  expect(created.email).toBe("first.admin@apiauth017.test");
  const rows = await query<{
    user_id: string | null;
    org_id: string;
    target_id: string;
    correlation_id: string | null;
    source_ip: string | null;
    detail: unknown;
  }>(
    `SELECT user_id::text, org_id, target_id, correlation_id::text, host(source_ip) AS source_ip, detail
       FROM audit_log WHERE action = 'user.create' AND target_id = $1`,
    [created.userId],
  );
  expect(rows).toEqual([
    {
      user_id: null,
      org_id: "default",
      target_id: created.userId,
      correlation_id: null,
      source_ip: null,
      detail: { role: "admin" },
    },
  ]);
});

test("an address already taken fails with a conflict, and writes nothing", async () => {
  await createUserFromCli(config, args("taken@apiauth017.test", "Valid-passw0rd-017", "viewer"));
  const err = await createUserFromCli(
    config,
    args("TAKEN@apiauth017.test", "Valid-passw0rd-017", "viewer"),
  ).then(
    () => null,
    (e: unknown) => e as Error,
  );
  expect(err).toBeInstanceOf(CliRefusal);
  expect(err?.message).toContain("already exists");
  expect(
    await query("SELECT 1 FROM users WHERE lower(email) = 'taken@apiauth017.test'"),
  ).toHaveLength(1);
});

/** Run the CLI as a program, as `task api:create-user` does, with the test's config. */
function runCli(argv: string[]): { status: number | null; stdout: string; stderr: string } {
  const r = spawnSync(process.execPath, ["--import", "tsx", "src/cli/create-user.ts", ...argv], {
    cwd: path.join(path.dirname(fileURLToPath(import.meta.url)), ".."),
    encoding: "utf8",
    env: {
      ...process.env,
      API_PG_URL: config.API_PG_URL,
      API_REDIS_URL: config.API_REDIS_URL,
      API_CH_URL: config.API_CH_URL,
      API_DB_ENC_PASSPHRASE: config.API_DB_ENC_PASSPHRASE,
      API_FORENSIC_PASSPHRASE: config.API_FORENSIC_PASSPHRASE,
    },
  });
  return { status: r.status, stdout: r.stdout, stderr: r.stderr };
}

test("run as a program, it creates the user and prints the URI once; a refusal exits 2", async () => {
  const ok = runCli(args("program@apiauth017.test", "Valid-passw0rd-017"));
  expect(ok.status, ok.stderr).toBe(0);
  expect(ok.stdout.trim()).toMatch(/^otpauth:\/\/totp\//);
  expect(ok.stdout.trim().split("\n")).toHaveLength(1);
  expect(await query("SELECT 1 FROM users WHERE email = 'program@apiauth017.test'")).toHaveLength(
    1,
  );
  const refused = runCli(args("program2@apiauth017.test", "weak"));
  expect(refused.status).toBe(2);
  expect(refused.stderr).toContain("refused: password");
  // The refusal names the field, never the password given.
  expect(refused.stderr).not.toContain("weak");
});
