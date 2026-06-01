import pg from "pg";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";

// SPEC-008 §Data contracts §2 — migration 0002_audit_log gate (structure GREEN).
//
// Verifies the audit_log table EXISTS, its `outcome` CHECK rejects bad values,
// `action` is NOT NULL, and `id GENERATED ALWAYS AS IDENTITY` blocks a
// client-supplied id (the append-only identity property). Passes in CI while the
// auth_ac_* stay RED. BEGIN/ROLLBACK isolates every assertion.

let client: pg.Client;

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

test("audit_log table exists", async () => {
  await expect(client.query("SELECT count(*) FROM audit_log")).resolves.toBeDefined();
});

test("accepts a valid audit row", async () => {
  const ok = await attempt(async () => {
    await client.query(
      "INSERT INTO audit_log (org_id, action, outcome) VALUES ('default', 'auth.login.ok', 'ok')",
    );
  });
  expect(ok).toBe(true);
});

test("rejects an out-of-enum outcome", async () => {
  const ok = await attempt(async () => {
    await client.query(
      "INSERT INTO audit_log (org_id, action, outcome) VALUES ('default', 'auth.login.ok', 'weird')",
    );
  });
  expect(ok).toBe(false);
});

test("rejects a NULL action (NOT NULL)", async () => {
  const ok = await attempt(async () => {
    await client.query(
      "INSERT INTO audit_log (org_id, action, outcome) VALUES ('default', NULL, 'ok')",
    );
  });
  expect(ok).toBe(false);
});

test("rejects a client-supplied id (GENERATED ALWAYS — append-only identity)", async () => {
  const ok = await attempt(async () => {
    await client.query(
      "INSERT INTO audit_log (id, action, outcome) VALUES (999, 'auth.login.ok', 'ok')",
    );
  });
  expect(ok).toBe(false);
});
