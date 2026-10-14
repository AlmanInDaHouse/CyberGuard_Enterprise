import { promises as fs } from "node:fs";
import * as path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { Kysely, type Migration, Migrator, PostgresDialect } from "kysely";
import pg from "pg";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { runMigrations } from "../src/db/migrate.js";
import { buildTestApp, currentTotp, insertUserDirect, seedSession } from "./helpers/db.js";

// apiauth_ac_011 — SPEC-008 Amendment 2026-10-15 item 3: an address is trimmed and
// lower-cased before it is stored, looked up or counted, and migration 0004 makes
// uniqueness per org case-insensitive, refusing an org whose addresses clash.

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;
const ADMIN = "tok-apiauth-011-admin";
const PASSWORD = "Valid-passw0rd-011";

async function sql<R extends pg.QueryResultRow>(text: string, values: unknown[] = []) {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    return (await pool.query<R>(text, values)).rows;
  } finally {
    await pool.end();
  }
}

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
  const adminId = await insertUserDirect(config, { email: "admin@apiauth011.test", role: "admin" });
  await seedSession(config, { token: ADMIN, userId: adminId, role: "admin" });
});
afterAll(async () => {
  await apph?.close();
});

async function create(email: string) {
  return apph.app.inject({
    method: "POST",
    url: "/v1/users",
    cookies: { cgsess: ADMIN },
    headers: { "x-csrf-token": `csrf-${ADMIN}` },
    payload: { email, password: PASSWORD, role: "viewer" },
  });
}

test("a user created with spaces and capitals is stored, and logs in, lower-cased", async () => {
  const res = await create("  Ann@Example.COM");
  expect(res.statusCode).toBe(201);
  const { userId, totpProvisioningUri } = res.json() as {
    userId: string;
    totpProvisioningUri: string;
  };
  const [row] = await sql<{ email: string }>("SELECT email FROM users WHERE user_id = $1", [
    userId,
  ]);
  expect(row?.email).toBe("ann@example.com");
  const secret = new URL(totpProvisioningUri).searchParams.get("secret") ?? "";
  await sql("UPDATE users SET totp_enrolled = true WHERE user_id = $1", [userId]);
  const login = await apph.app.inject({
    method: "POST",
    url: "/v1/auth/login",
    remoteAddress: "198.51.100.111",
    payload: { email: "ann@example.com", password: PASSWORD, totp_code: currentTotp(secret) },
  });
  expect(login.statusCode).toBe(200);
});

test("a second address equal once lower-cased is a 409, and nothing is written", async () => {
  const res = await create("ANN@example.com");
  expect(res.statusCode).toBe(409);
  expect(res.json()).toEqual({ error: "conflict" });
  const rows = await sql("SELECT user_id FROM users WHERE lower(email) = 'ann@example.com'");
  expect(rows).toHaveLength(1);
});

test("failed logins under case variants of one address count as one account", async () => {
  await insertUserDirect(config, { email: "bob@apiauth011.test", role: "viewer" });
  const variants = [
    "Bob@apiauth011.test",
    "BOB@apiauth011.test",
    " bob@apiauth011.test",
    "bob@APIAUTH011.test",
    "bOb@apiauth011.test ",
  ];
  // A fresh address for each attempt, so only the per-account limit can trip.
  for (const [i, email] of variants.entries()) {
    const res = await apph.app.inject({
      method: "POST",
      url: "/v1/auth/login",
      remoteAddress: `198.51.100.${120 + i}`,
      payload: { email, password: "wrong-password", totp_code: "000000" },
    });
    expect(res.statusCode, email).toBe(401);
  }
  const sixth = await apph.app.inject({
    method: "POST",
    url: "/v1/auth/login",
    remoteAddress: "198.51.100.130",
    payload: { email: "bob@apiauth011.test", password: "wrong-password", totp_code: "000000" },
  });
  expect(sixth.statusCode).toBe(429);
});

test("migration 0004 refuses an org whose addresses clash, naming no address, and rewrites a single one", async () => {
  const db = `apiauth011_${Date.now()}`;
  const admin = new pg.Pool({ connectionString: config.API_PG_URL });
  await admin.query(`CREATE DATABASE ${db}`);
  const url = new URL(config.API_PG_URL);
  url.pathname = `/${db}`;
  const throwaway = url.toString();
  try {
    // Migrate the throwaway database to 0003, as a deployment before this amendment.
    const folder = path.join(path.dirname(fileURLToPath(import.meta.url)), "../src/db/migrations");
    const k = new Kysely<unknown>({
      dialect: new PostgresDialect({ pool: new pg.Pool({ connectionString: throwaway }) }),
    });
    try {
      const migrator = new Migrator({
        db: k,
        provider: {
          async getMigrations() {
            const out: Record<string, Migration> = {};
            for (const f of (await fs.readdir(folder)).filter((f) => f.endsWith(".ts")).sort()) {
              out[f.replace(/\.ts$/, "")] = (await import(
                pathToFileURL(path.join(folder, f)).href
              )) as Migration;
            }
            return out;
          },
        },
        migrationTableName: "api_kysely_migration",
        migrationLockTableName: "api_kysely_migration_lock",
      });
      const to3 = await migrator.migrateTo("0003_forensic_key");
      expect(to3.error).toBeUndefined();
    } finally {
      await k.destroy();
    }
    const pool = new pg.Pool({ connectionString: throwaway });
    const insert = (email: string) =>
      pool.query(
        `INSERT INTO users (user_id, org_id, email, password_hash, totp_secret, role)
         VALUES ($1, 'default', $2, 'h', 'x'::bytea, 'viewer')`,
        [globalThis.crypto.randomUUID(), email],
      );
    try {
      await insert("Dup@apiauth011.test");
      await insert("dup@apiauth011.test");
      const err = await runMigrations({ ...config, API_PG_URL: throwaway }).then(
        () => null,
        (e: unknown) => e as Error,
      );
      expect(err?.message).toContain("0004_users_email_ci");
      expect(err?.message).toContain('org "default" holds 2 addresses');
      expect(err?.message.toLowerCase()).not.toContain("dup@apiauth011.test");
      // Nothing changed: both rows stand as they were.
      const kept = await pool.query<{ email: string }>("SELECT email FROM users ORDER BY email");
      expect(kept.rows.map((r) => r.email).sort()).toEqual(
        ["Dup@apiauth011.test", "dup@apiauth011.test"].sort(),
      );

      // Two addresses that differ only in surrounding spaces clash too.
      await pool.query("DELETE FROM users");
      await insert("space@apiauth011.test");
      await insert("space@apiauth011.test  ");
      const spaces = await runMigrations({ ...config, API_PG_URL: throwaway }).then(
        () => null,
        (e: unknown) => e as Error,
      );
      expect(spaces?.message).toContain('org "default" holds 2 addresses');

      await pool.query("DELETE FROM users");
      await insert("Dup@apiauth011.test");
      await runMigrations({ ...config, API_PG_URL: throwaway });
      const index = await pool.query(
        "SELECT 1 FROM pg_indexes WHERE indexname = 'users_email_org_lower_unique'",
      );
      expect(index.rowCount).toBe(1);
      const after = await pool.query<{ email: string }>("SELECT email FROM users");
      expect(after.rows.map((r) => r.email)).toEqual(["dup@apiauth011.test"]);
    } finally {
      await pool.end();
    }
  } finally {
    await admin.query(`DROP DATABASE IF EXISTS ${db} WITH (FORCE)`);
    await admin.end();
  }
});
