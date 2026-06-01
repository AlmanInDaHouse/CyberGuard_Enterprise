import pg from "pg";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";

// SPEC-008 §Data contracts §1 — migration 0001_users gate (structure GREEN).
//
// Verifies the users table EXISTS and its CHECK / UNIQUE / NOT NULL constraints
// REJECT bad data — stronger than "runMigrations() did not throw". This is the
// structure's own verifiable GREEN: it passes in CI (Linux + testcontainers)
// even while every auth_ac_* stays RED (NotImplemented). The structure enables
// the controls; it does not implement them.
//
// Every assertion runs inside BEGIN/ROLLBACK so nothing persists in the shared
// (singleFork) Postgres.

let client: pg.Client;
const PASSPHRASE = "test-api-passphrase"; // matches backends.ts API_DB_ENC_PASSPHRASE

let n = 0;
function uid(): string {
  n += 1;
  return `01934abc-def0-7000-89ab-${String(n).padStart(12, "0")}`;
}

async function insertUser(opts: {
  userId?: string;
  email?: string;
  role?: string;
  status?: string;
  passwordHash?: string | null;
}): Promise<void> {
  await client.query(
    `INSERT INTO users (user_id, org_id, email, password_hash, totp_secret, totp_enrolled, role, status)
     VALUES ($1, 'default', $2, $3, pgp_sym_encrypt('s', $4), false, $5, $6)`,
    [
      opts.userId ?? uid(),
      opts.email ?? `u${n}@example.test`,
      opts.passwordHash === undefined ? "argon2-placeholder" : opts.passwordHash,
      PASSPHRASE,
      opts.role ?? "admin",
      opts.status ?? "active",
    ],
  );
}

async function attempt(body: () => Promise<void>): Promise<boolean> {
  await client.query("BEGIN");
  let ok = false;
  try {
    await body();
    ok = true;
  } catch {
    ok = false;
  }
  await client.query("ROLLBACK");
  return ok;
}

beforeAll(async () => {
  const config: Config = inject("apiConfig");
  client = new pg.Client({ connectionString: config.API_PG_URL });
  await client.connect();
});

afterAll(async () => {
  await client?.end();
});

test("users table exists", async () => {
  await expect(client.query("SELECT count(*) FROM users")).resolves.toBeDefined();
});

test("pgcrypto is enabled (0001 self-contained)", async () => {
  await expect(client.query("SELECT pgp_sym_encrypt('x', 'k')")).resolves.toBeDefined();
});

test("accepts a valid user", async () => {
  const ok = await attempt(async () => {
    await insertUser({});
  });
  expect(ok).toBe(true);
});

test("rejects an out-of-enum role", async () => {
  const ok = await attempt(async () => {
    await insertUser({ role: "superadmin" });
  });
  expect(ok).toBe(false);
});

test("rejects an out-of-enum status", async () => {
  const ok = await attempt(async () => {
    await insertUser({ status: "frozen" });
  });
  expect(ok).toBe(false);
});

test("rejects a NULL password_hash (NOT NULL)", async () => {
  const ok = await attempt(async () => {
    await insertUser({ passwordHash: null });
  });
  expect(ok).toBe(false);
});

test("rejects a duplicate (org_id, email) (UNIQUE)", async () => {
  const ok = await attempt(async () => {
    const email = `dup${n}@example.test`;
    await insertUser({ email });
    await insertUser({ email });
  });
  expect(ok).toBe(false);
});
