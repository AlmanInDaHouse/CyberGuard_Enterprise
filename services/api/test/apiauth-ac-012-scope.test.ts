import pg from "pg";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp, insertUserDirect, seedSession } from "./helpers/db.js";

// apiauth_ac_012 — SPEC-008 Amendment 2026-10-15 item 5: the user routes act in the
// session's org. Another org's user is a 404, as an unknown id is, and a
// non-canonical :id is a 404 before its body is read; nothing changes.

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;
const ORG_A = "apiauth-012-a";
const ORG_B = "apiauth-012-b";
const ADMIN = "tok-apiauth-012-admin";
let inA: string;
let inB: string;

async function roleOf(userId: string): Promise<string | undefined> {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    const r = await pool.query<{ role: string; status: string }>(
      "SELECT role, status FROM users WHERE user_id = $1",
      [userId],
    );
    return r.rows[0] ? `${r.rows[0].role}/${r.rows[0].status}` : undefined;
  } finally {
    await pool.end();
  }
}

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
  const adminId = await insertUserDirect(config, {
    email: "admin@apiauth012.test",
    role: "admin",
    orgId: ORG_A,
  });
  await seedSession(config, { token: ADMIN, userId: adminId, role: "admin", orgId: ORG_A });
  inA = await insertUserDirect(config, {
    email: "a@apiauth012.test",
    role: "viewer",
    orgId: ORG_A,
  });
  inB = await insertUserDirect(config, {
    email: "b@apiauth012.test",
    role: "viewer",
    orgId: ORG_B,
  });
});
afterAll(async () => {
  await apph?.close();
});

const asAdmin = { cookies: { cgsess: ADMIN }, headers: { "x-csrf-token": `csrf-${ADMIN}` } };

test("the list holds only the session org's users, each with its status", async () => {
  const res = await apph.app.inject({ method: "GET", url: "/v1/users", ...asAdmin });
  expect(res.statusCode).toBe(200);
  const users = res.json() as Array<{ userId: string; email: string; status: string }>;
  expect(users.map((u) => u.email).sort()).toEqual(["a@apiauth012.test", "admin@apiauth012.test"]);
  expect(users.every((u) => u.status === "active")).toBe(true);
});

test.each([
  ["role", { role: "analyst" }],
  ["status", { status: "disabled" }],
])("a %s change aimed at another org's user is a 404, and nothing changes", async (route, body) => {
  const res = await apph.app.inject({
    method: "POST",
    url: `/v1/users/${inB}/${route}`,
    payload: body,
    ...asAdmin,
  });
  expect(res.statusCode).toBe(404);
  expect(res.json()).toEqual({ error: "not_found" });
  expect(await roleOf(inB)).toBe("viewer/active");
  expect(await auditRowsFor(inB)).toBe(0);
});

test.each([
  ["role", { role: "analyst" }],
  ["status", { status: "disabled" }],
])("an unknown id and a non-canonical :id are 404s on the %s route", async (route, body) => {
  for (const id of [globalThis.crypto.randomUUID(), `{${inA}}`, inA.replaceAll("-", "")]) {
    const res = await apph.app.inject({
      method: "POST",
      url: `/v1/users/${id}/${route}`,
      payload: body,
      ...asAdmin,
    });
    expect(res.statusCode, id).toBe(404);
    expect(res.json()).toEqual({ error: "not_found" });
  }
  expect(await roleOf(inA)).toBe("viewer/active");
  expect(await auditRowsFor(inA)).toBe(0);
});

async function auditRowsFor(userId: string): Promise<number> {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    const r = await pool.query("SELECT 1 FROM audit_log WHERE target_id = $1", [userId]);
    return r.rowCount ?? 0;
  } finally {
    await pool.end();
  }
}

test("an :id in upper case is read in lower case", async () => {
  const res = await apph.app.inject({
    method: "POST",
    url: `/v1/users/${inA.toUpperCase()}/role`,
    payload: { role: "analyst" },
    ...asAdmin,
  });
  expect(res.statusCode).toBe(200);
  expect(await roleOf(inA)).toBe("analyst/active");
});
