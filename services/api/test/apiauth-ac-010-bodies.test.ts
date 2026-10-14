import { Redis } from "ioredis";
import pg from "pg";
import { afterAll, beforeAll, describe, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import {
  buildTestApp,
  getAuditRows,
  getUserByEmail,
  insertUserDirect,
  seedSession,
} from "./helpers/db.js";

// apiauth_ac_010 — SPEC-008 Amendment 2026-10-15 items 1 and 2: each route parses its
// body with a strict schema before any store is read, and a body that fails answers
// 400 with nothing written. The session, role and CSRF checks come first, before
// Fastify reads the body, so a caller without them learns nothing about a body.

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;
const ADMIN_TOKEN = "tok-apiauth-010-admin";
const VIEWER_TOKEN = "tok-apiauth-010-viewer";
let targetId: string;

const admin = (extra: Record<string, unknown> = {}) => ({
  cookies: { cgsess: ADMIN_TOKEN },
  headers: { "x-csrf-token": `csrf-${ADMIN_TOKEN}` },
  ...extra,
});

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
  const adminId = await insertUserDirect(config, { email: "admin@apiauth010.test", role: "admin" });
  await seedSession(config, { token: ADMIN_TOKEN, userId: adminId, role: "admin" });
  const viewerId = await insertUserDirect(config, {
    email: "viewer@apiauth010.test",
    role: "viewer",
  });
  await seedSession(config, { token: VIEWER_TOKEN, userId: viewerId, role: "viewer" });
  targetId = await insertUserDirect(config, { email: "target@apiauth010.test", role: "viewer" });
});
afterAll(async () => {
  await apph?.close();
});

async function create(payload: unknown) {
  return apph.app.inject({
    method: "POST",
    url: "/v1/users",
    payload: payload as object,
    ...admin(),
  });
}

describe("a body that fails its schema is a 400, with nothing written", () => {
  test("an unknown role on create (#72) writes no user", async () => {
    const res = await create({
      email: "made-root@apiauth010.test",
      password: "Valid-passw0rd",
      role: "root",
    });
    expect(res.statusCode).toBe(400);
    expect(res.json()).toEqual({ error: "bad_request" });
    expect(await getUserByEmail(config, "made-root@apiauth010.test")).toBeNull();
  });

  test("an unknown role on a role change leaves the role and writes no row", async () => {
    const res = await apph.app.inject({
      method: "POST",
      url: `/v1/users/${targetId}/role`,
      payload: { role: "root" },
      ...admin(),
    });
    expect(res.statusCode).toBe(400);
    expect((await getUserByEmail(config, "target@apiauth010.test"))?.role).toBe("viewer");
    const pool = new pg.Pool({ connectionString: config.API_PG_URL });
    try {
      const rows = await pool.query(
        "SELECT 1 FROM audit_log WHERE action = 'role.change' AND target_id = $1",
        [targetId],
      );
      expect(rows.rowCount).toBe(0);
    } finally {
      await pool.end();
    }
  });

  test.each([
    ["a userId that is not a UUID", { userId: "not-a-uuid", totp_code: "000000" }],
    ["an extra field", { userId: globalThis.crypto.randomUUID(), totp_code: "000000", x: 1 }],
  ])("the TOTP confirmation refuses %s", async (_label, payload) => {
    const res = await apph.app.inject({
      method: "POST",
      url: "/v1/auth/totp/confirm",
      remoteAddress: "198.51.100.103",
      payload,
    });
    expect(res.statusCode).toBe(400);
  });

  test("the status route refuses a status it does not know", async () => {
    const res = await apph.app.inject({
      method: "POST",
      url: `/v1/users/${targetId}/status`,
      payload: { status: "gone" },
      ...admin(),
    });
    expect(res.statusCode).toBe(400);
    expect((await getUserByEmail(config, "target@apiauth010.test"))?.status).toBe("active");
  });

  test("the password change refuses a new password the policy refuses", async () => {
    const res = await apph.app.inject({
      method: "POST",
      url: "/v1/auth/password",
      payload: { currentPassword: "whatever", newPassword: "short1", totp_code: "000000" },
      ...admin(),
    });
    expect(res.statusCode).toBe(400);
  });

  test.each([
    [
      "an extra field",
      { email: "x@apiauth010.test", password: "Valid-passw0rd", role: "viewer", org_id: "other" },
    ],
    [
      "a field of the wrong type",
      { email: "x@apiauth010.test", password: 123456789012, role: "viewer" },
    ],
    ["a missing field", { email: "x@apiauth010.test", role: "viewer" }],
    [
      "an address zod refuses",
      { email: "not-an-address", password: "Valid-passw0rd", role: "viewer" },
    ],
  ])("%s on create", async (_label, payload) => {
    const res = await create(payload);
    expect(res.statusCode).toBe(400);
    expect(await getUserByEmail(config, "x@apiauth010.test")).toBeNull();
  });

  test.each([
    ["11 code points", "Abcdefghi1j"],
    ["129 code points", `A1${"b".repeat(127)}`],
    ["12 with no digit", "Abcdefghijkl"],
    ["12 with no letter", "123456789012"],
  ])("a new password of %s is refused", async (_label, password) => {
    const res = await create({ email: "weak@apiauth010.test", password, role: "viewer" });
    expect(res.statusCode).toBe(400);
    expect(await getUserByEmail(config, "weak@apiauth010.test")).toBeNull();
  });

  test("12 code points with a letter and a digit, counted as code points, is accepted", async () => {
    // Six astral letters (two UTF-16 units each), five ASCII letters and a digit.
    const res = await create({
      email: "strong@apiauth010.test",
      password: `${"\u{1D49C}".repeat(6)}abcde1`,
      role: "viewer",
    });
    expect(res.statusCode).toBe(201);
  });
});

describe("the login's body", () => {
  test.each([
    [
      "an extra field",
      { email: "nobody@apiauth010.test", password: "p", totp_code: "000000", x: 1 },
    ],
    [
      "a malformed totp_code",
      { email: "nobody@apiauth010.test", password: "p", totp_code: "12345" },
    ],
    [
      "a password of the wrong type",
      { email: "nobody@apiauth010.test", password: 1, totp_code: "000000" },
    ],
  ])("%s is a 400, with no audit row and no count against a limit", async (_label, payload) => {
    const before = (await getAuditRows(config, { action: "auth.login.fail" })).length;
    const res = await apph.app.inject({ method: "POST", url: "/v1/auth/login", payload });
    expect(res.statusCode).toBe(400);
    expect((await getAuditRows(config, { action: "auth.login.fail" })).length).toBe(before);
    const redis = new Redis(config.API_REDIS_URL);
    try {
      expect(await redis.get("rl:acct:default:nobody@apiauth010.test")).toBeNull();
    } finally {
      await redis.quit();
    }
  });

  test.each([
    ["only spaces", "   "],
    ["a NUL", "nul\u0000@apiauth010.test"],
  ])("an address of %s is a 400, with no audit row", async (_label, email) => {
    const before = (await getAuditRows(config, { action: "auth.login.fail" })).length;
    const res = await apph.app.inject({
      method: "POST",
      url: "/v1/auth/login",
      remoteAddress: "198.51.100.101",
      payload: { email, password: "p", totp_code: "000000" },
    });
    expect(res.statusCode).toBe(400);
    expect((await getAuditRows(config, { action: "auth.login.fail" })).length).toBe(before);
  });

  test("a refused login body counts against no per-IP limit", async () => {
    await apph.app.inject({
      method: "POST",
      url: "/v1/auth/login",
      remoteAddress: "198.51.100.102",
      payload: { email: "x@apiauth010.test", password: "p", totp_code: "1" },
    });
    const redis = new Redis(config.API_REDIS_URL);
    try {
      expect(await redis.get("rl:ip:198.51.100.102")).toBeNull();
    } finally {
      await redis.quit();
    }
  });

  test("a body over the size limit is a 413, and a media type with no parser a 415", async () => {
    const big = await apph.app.inject({
      method: "POST",
      url: "/v1/auth/login",
      headers: { "content-type": "application/json" },
      payload: JSON.stringify({
        email: "x".repeat(2 * 1024 * 1024),
        password: "p",
        totp_code: "000000",
      }),
    });
    expect(big.statusCode).toBe(413);
    expect(big.json()).toEqual({ error: "bad_request" });
    const xml = await apph.app.inject({
      method: "POST",
      url: "/v1/auth/login",
      headers: { "content-type": "application/xml" },
      payload: "<login/>",
    });
    expect(xml.statusCode).toBe(415);
    expect(xml.json()).toEqual({ error: "bad_request" });
  });

  test("JSON that does not parse is a 400 bad_request", async () => {
    const res = await apph.app.inject({
      method: "POST",
      url: "/v1/auth/login",
      headers: { "content-type": "application/json" },
      payload: "{",
    });
    expect(res.statusCode).toBe(400);
    expect(res.json()).toEqual({ error: "bad_request" });
  });

  test("a text/plain body is read and refused by the schema", async () => {
    const res = await apph.app.inject({
      method: "POST",
      url: "/v1/auth/login",
      headers: { "content-type": "text/plain" },
      payload: "email=a",
    });
    expect(res.statusCode).toBe(400);
  });
});

describe("the other routes' bodies", () => {
  test.each([
    [
      "the password change",
      "/v1/auth/password",
      { currentPassword: "a", newPassword: "Valid-passw0rd", totp_code: "12345" },
    ],
    [
      "the TOTP confirmation",
      "/v1/auth/totp/confirm",
      { userId: globalThis.crypto.randomUUID(), totp_code: "abcdef" },
    ],
  ])("%s refuses a malformed totp_code", async (_label, url, payload) => {
    const res = await apph.app.inject({
      method: "POST",
      url,
      payload,
      remoteAddress: "198.51.100.104",
      ...admin(),
    });
    expect(res.statusCode).toBe(400);
  });

  test("a refused confirmation writes nothing and counts nothing", async () => {
    const userId = globalThis.crypto.randomUUID();
    const res = await apph.app.inject({
      method: "POST",
      url: "/v1/auth/totp/confirm",
      remoteAddress: "198.51.100.105",
      payload: { userId, totp_code: "1", x: 1 },
    });
    expect(res.statusCode).toBe(400);
    const redis = new Redis(config.API_REDIS_URL);
    try {
      expect(await redis.get(`rl:user:${userId}`)).toBeNull();
      expect(await redis.get("rl:ip:198.51.100.105")).toBeNull();
    } finally {
      await redis.quit();
    }
  });

  test("with a session and the CSRF token, JSON that does not parse is a 400", async () => {
    const res = await apph.app.inject({
      method: "POST",
      url: "/v1/users",
      cookies: { cgsess: ADMIN_TOKEN },
      headers: { "x-csrf-token": `csrf-${ADMIN_TOKEN}`, "content-type": "application/json" },
      payload: "{",
    });
    expect(res.statusCode).toBe(400);
    expect(res.json()).toEqual({ error: "bad_request" });
  });

  test("a logout with a body that parses ignores it", async () => {
    const userId = await insertUserDirect(config, {
      email: "out2@apiauth010.test",
      role: "viewer",
    });
    await seedSession(config, { token: "tok-apiauth-010-out2", userId, role: "viewer" });
    const res = await apph.app.inject({
      method: "POST",
      url: "/v1/auth/logout",
      cookies: { cgsess: "tok-apiauth-010-out2" },
      headers: { "x-csrf-token": "csrf-tok-apiauth-010-out2" },
      payload: { anything: "here" },
    });
    expect(res.statusCode).toBe(200);
  });
});

describe("the status route's body", () => {
  test.each([
    ["an extra field", { status: "disabled", x: 1 }],
    ["a field of the wrong type", { status: 1 }],
  ])("refuses %s", async (_label, payload) => {
    const res = await apph.app.inject({
      method: "POST",
      url: `/v1/users/${targetId}/status`,
      payload,
      ...admin(),
    });
    expect(res.statusCode).toBe(400);
    expect((await getUserByEmail(config, "target@apiauth010.test"))?.status).toBe("active");
  });
});

describe("an extra field is refused on every route that takes a body", () => {
  test.each([
    ["the role change", () => `/v1/users/${targetId}/role`, { role: "analyst", x: 1 }],
    [
      "the password change",
      () => "/v1/auth/password",
      { currentPassword: "a", newPassword: "Valid-passw0rd", totp_code: "000000", x: 1 },
    ],
  ])("%s", async (_label, url, payload) => {
    const res = await apph.app.inject({ method: "POST", url: url(), payload, ...admin() });
    expect(res.statusCode).toBe(400);
  });
});

describe("the checks come before the body", () => {
  test.each(["/v1/auth/password", "/v1/users/00000000-0000-4000-8000-000000000001/role"])(
    "without a session, %s answers 401 whatever the body",
    async (url) => {
      const res = await apph.app.inject({
        method: "POST",
        url,
        headers: { "content-type": "application/json" },
        payload: "{",
      });
      expect(res.statusCode).toBe(401);
    },
  );

  test("without a session, an unparseable body is a 401", async () => {
    const res = await apph.app.inject({
      method: "POST",
      url: "/v1/users",
      headers: { "content-type": "application/json" },
      payload: "{",
    });
    expect(res.statusCode).toBe(401);
  });

  test("for a viewer, an unparseable body is a 403", async () => {
    const res = await apph.app.inject({
      method: "POST",
      url: "/v1/users",
      cookies: { cgsess: VIEWER_TOKEN },
      headers: { "x-csrf-token": `csrf-${VIEWER_TOKEN}`, "content-type": "application/json" },
      payload: "{",
    });
    expect(res.statusCode).toBe(403);
  });

  test("a non-canonical :id is a 404 before its body is read", async () => {
    const res = await apph.app.inject({
      method: "POST",
      url: `/v1/users/{${targetId}}/role`,
      cookies: { cgsess: ADMIN_TOKEN },
      headers: { "x-csrf-token": `csrf-${ADMIN_TOKEN}`, "content-type": "application/json" },
      payload: "{",
    });
    expect(res.statusCode).toBe(404);
  });

  test("a logout without the CSRF token is a 403, and with it a 200", async () => {
    const userId = await insertUserDirect(config, { email: "out@apiauth010.test", role: "viewer" });
    await seedSession(config, { token: "tok-apiauth-010-out", userId, role: "viewer" });
    const without = await apph.app.inject({
      method: "POST",
      url: "/v1/auth/logout",
      cookies: { cgsess: "tok-apiauth-010-out" },
    });
    expect(without.statusCode).toBe(403);
    const withToken = await apph.app.inject({
      method: "POST",
      url: "/v1/auth/logout",
      cookies: { cgsess: "tok-apiauth-010-out" },
      headers: { "x-csrf-token": "csrf-tok-apiauth-010-out" },
    });
    expect(withToken.statusCode).toBe(200);
  });
});
