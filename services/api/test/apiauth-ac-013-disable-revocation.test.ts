import {
  type AddressInfo,
  type Server,
  type Socket,
  createConnection,
  createServer,
} from "node:net";
import { Redis } from "ioredis";
import pg from "pg";
import { afterAll, beforeAll, expect, inject, test, vi } from "vitest";
import { buildApp } from "../src/app.js";
import { createAuthService } from "../src/auth/service.js";
import type { Config } from "../src/config.js";
import { buildServices, createPgPool } from "../src/services.js";
import {
  DEFAULT_PASSWORD,
  buildTestApp,
  currentTotp,
  insertUserDirect,
  seedSession,
} from "./helpers/db.js";

// apiauth_ac_013 — SPEC-008 Amendment 2026-10-15 items 7 to 10: an account can be
// disabled and enabled; each change of a user's authority deletes its sessions and
// raises its authority version, so a login that read the row before the change keeps
// no session; the org's last active admin cannot be demoted or disabled.

// A gate on the login's Argon2 verification, closed only by the race test: the login
// has read the user's row, and the change commits while it verifies.
const gate = vi.hoisted(() => ({ wait: Promise.resolve(), reached: () => {} }));
vi.mock("../src/auth/password.js", async (importOriginal) => {
  const mod = await importOriginal<typeof import("../src/auth/password.js")>();
  return {
    ...mod,
    verifyPassword: async (stored: string, password: string) => {
      gate.reached();
      await gate.wait;
      return mod.verifyPassword(stored, password);
    },
  };
});

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;
// The login works in the org `default` (SPEC-008 Amendment 2026-10-15 item 5), and these
// tests log the changed users in.
const ORG = "default";
const ADMIN = "tok-apiauth-013-admin";
let adminId: string;
let n = 0;

const asAdmin = { cookies: { cgsess: ADMIN }, headers: { "x-csrf-token": `csrf-${ADMIN}` } };

async function query<R extends pg.QueryResultRow>(text: string, values: unknown[] = []) {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    return (await pool.query<R>(text, values)).rows;
  } finally {
    await pool.end();
  }
}

/** The session keys Redis holds for a user. */
async function sessionsOf(userId: string): Promise<string[]> {
  const redis = new Redis(config.API_REDIS_URL);
  try {
    const keys: string[] = [];
    let cursor = "0";
    do {
      const [next, batch] = await redis.scan(cursor, "MATCH", "cgsess:*", "COUNT", 500);
      cursor = next;
      for (const k of batch) {
        const raw = await redis.get(k);
        if (raw && (JSON.parse(raw) as { user_id: string }).user_id === userId) keys.push(k);
      }
    } while (cursor !== "0");
    return keys;
  } finally {
    await redis.quit();
  }
}

async function login(email: string, ip: string) {
  return apph.app.inject({
    method: "POST",
    url: "/v1/auth/login",
    remoteAddress: ip,
    payload: { email, password: DEFAULT_PASSWORD, totp_code: currentTotp() },
  });
}

async function newUser(role: "admin" | "analyst" | "viewer" = "viewer", org = ORG) {
  n += 1;
  const email = `u${n}-${globalThis.crypto.randomUUID()}@apiauth013.test`;
  const userId = await insertUserDirect(config, { email, role, orgId: org });
  return { userId, email };
}

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
  adminId = await insertUserDirect(config, {
    email: "admin@apiauth013.test",
    role: "admin",
    orgId: ORG,
  });
  await seedSession(config, { token: ADMIN, userId: adminId, role: "admin", orgId: ORG });
  // A second active admin, so the tests that change one keep the org administered.
  await insertUserDirect(config, { email: "admin2@apiauth013.test", role: "admin", orgId: ORG });
});
afterAll(async () => {
  await apph?.close();
});

test("disabling a user ends each of its sessions and its login; enabling restores the login only", async () => {
  const { userId, email } = await newUser();
  await seedSession(config, { token: "tok-013-s1", userId, role: "viewer", orgId: ORG });
  await seedSession(config, { token: "tok-013-s2", userId, role: "viewer", orgId: ORG });

  const off = await apph.app.inject({
    method: "POST",
    url: `/v1/users/${userId}/status`,
    payload: { status: "disabled" },
    ...asAdmin,
  });
  expect(off.statusCode).toBe(200);
  for (const token of ["tok-013-s1", "tok-013-s2"]) {
    const res = await apph.app.inject({
      method: "GET",
      url: "/v1/incidents",
      cookies: { cgsess: token },
    });
    expect(res.statusCode, token).toBe(401);
  }
  expect((await login(email, "198.51.100.131")).statusCode).toBe(401);
  const [row] = await query<{ user_id: string; target_id: string; detail: unknown }>(
    "SELECT user_id, target_id, detail FROM audit_log WHERE action = 'user.disable' AND target_id = $1",
    [userId],
  );
  expect(row).toMatchObject({ user_id: adminId, detail: { from: "active", to: "disabled" } });

  const on = await apph.app.inject({
    method: "POST",
    url: `/v1/users/${userId}/status`,
    payload: { status: "active" },
    ...asAdmin,
  });
  expect(on.statusCode).toBe(200);
  expect(await sessionsOf(userId)).toEqual([]);
  expect((await login(email, "198.51.100.132")).statusCode).toBe(200);
});

test("the status route needs the CSRF token", async () => {
  const { userId } = await newUser();
  const res = await apph.app.inject({
    method: "POST",
    url: `/v1/users/${userId}/status`,
    cookies: { cgsess: ADMIN },
    payload: { status: "disabled" },
  });
  expect(res.statusCode).toBe(403);
});

test("a change to the value already held is recorded, and deletes the sessions", async () => {
  const { userId } = await newUser("viewer");
  await seedSession(config, { token: "tok-013-same", userId, role: "viewer", orgId: ORG });
  const res = await apph.app.inject({
    method: "POST",
    url: `/v1/users/${userId}/role`,
    payload: { role: "viewer" },
    ...asAdmin,
  });
  expect(res.statusCode).toBe(200);
  const rows = await query<{ detail: unknown }>(
    "SELECT detail FROM audit_log WHERE action = 'role.change' AND target_id = $1",
    [userId],
  );
  expect(rows).toEqual([{ detail: { from: "viewer", to: "viewer" } }]);
  expect(await sessionsOf(userId)).toEqual([]);
});

test("the org's last active admin cannot be demoted or disabled", async () => {
  const org = "apiauth-013-solo";
  const { userId: solo } = await newUser("admin", org);
  await seedSession(config, { token: "tok-013-solo", userId: solo, role: "admin", orgId: org });
  await seedSession(config, { token: "tok-013-solo-2", userId: solo, role: "admin", orgId: org });
  const as = {
    cookies: { cgsess: "tok-013-solo" },
    headers: { "x-csrf-token": "csrf-tok-013-solo" },
  };
  for (const [route, body] of [
    ["role", { role: "analyst" }],
    ["status", { status: "disabled" }],
  ] as const) {
    const res = await apph.app.inject({
      method: "POST",
      url: `/v1/users/${solo}/${route}`,
      payload: body,
      ...as,
    });
    expect(res.statusCode, route).toBe(409);
    expect(res.json()).toEqual({ error: "conflict" });
  }
  expect(await query("SELECT role, status FROM users WHERE user_id = $1", [solo])).toEqual([
    { role: "admin", status: "active" },
  ]);
  // Nothing changed: the sessions stand and no row was written.
  expect((await sessionsOf(solo)).sort()).toEqual(["cgsess:tok-013-solo", "cgsess:tok-013-solo-2"]);
  expect(await query("SELECT 1 FROM audit_log WHERE target_id = $1", [solo])).toEqual([]);
});

test("two admins demoting each other at once leave one admin", async () => {
  const org = "apiauth-013-pair";
  const a = await newUser("admin", org);
  const b = await newUser("admin", org);
  await seedSession(config, { token: "tok-013-a", userId: a.userId, role: "admin", orgId: org });
  await seedSession(config, { token: "tok-013-b", userId: b.userId, role: "admin", orgId: org });
  const demote = (token: string, target: string) =>
    apph.app.inject({
      method: "POST",
      url: `/v1/users/${target}/role`,
      cookies: { cgsess: token },
      headers: { "x-csrf-token": `csrf-${token}` },
      payload: { role: "viewer" },
    });
  const results = await Promise.all([demote("tok-013-a", b.userId), demote("tok-013-b", a.userId)]);
  expect(results.map((r) => r.statusCode).sort()).toEqual([200, 409]);
  const admins = await query(
    "SELECT user_id FROM users WHERE org_id = $1 AND role = 'admin' AND status = 'active'",
    [org],
  );
  expect(admins).toHaveLength(1);
});

test("a login that read the row before a role change committed keeps no session", async () => {
  const { userId, email } = await newUser("analyst");
  let open = () => {};
  const reached = new Promise<void>((resolve) => {
    gate.reached = resolve;
  });
  gate.wait = new Promise<void>((resolve) => {
    open = resolve;
  });
  try {
    const pending = login(email, "198.51.100.133");
    // The login has read the row and is verifying the password: the change commits now.
    await reached;
    const change = await apph.app.inject({
      method: "POST",
      url: `/v1/users/${userId}/role`,
      payload: { role: "viewer" },
      ...asAdmin,
    });
    expect(change.statusCode).toBe(200);
    open();
    const res = await pending;
    expect(res.statusCode).toBe(401);
    expect(await sessionsOf(userId)).toEqual([]);
    // The credentials were right: nothing counts against the user's limit.
    const redis = new Redis(config.API_REDIS_URL);
    try {
      expect(await redis.get(`rl:acct:default:${email}`)).toBeNull();
    } finally {
      await redis.quit();
    }
  } finally {
    gate.wait = Promise.resolve();
    gate.reached = () => {};
    open();
  }
});

test("a password change raises the authority version and deletes every session, the caller's too", async () => {
  const { userId } = await newUser("analyst");
  await seedSession(config, { token: "tok-013-pw1", userId, role: "analyst", orgId: ORG });
  await seedSession(config, { token: "tok-013-pw2", userId, role: "analyst", orgId: ORG });
  const [before] = await query<{ authority_version: string }>(
    "SELECT authority_version FROM users WHERE user_id = $1",
    [userId],
  );
  const res = await apph.app.inject({
    method: "POST",
    url: "/v1/auth/password",
    cookies: { cgsess: "tok-013-pw1" },
    headers: { "x-csrf-token": "csrf-tok-013-pw1" },
    payload: {
      currentPassword: DEFAULT_PASSWORD,
      newPassword: "New-passw0rd-013",
      totp_code: currentTotp(),
    },
  });
  expect(res.statusCode).toBe(200);
  expect(await sessionsOf(userId)).toEqual([]);
  const [after] = await query<{ authority_version: string }>(
    "SELECT authority_version FROM users WHERE user_id = $1",
    [userId],
  );
  expect(Number(after?.authority_version)).toBe(Number(before?.authority_version) + 1);
});

test("a session deletion that fails after the commit answers 503, logged with the user's id", async () => {
  const { userId } = await newUser("viewer");
  const lines: string[] = [];
  const services = await buildServices(config);
  // A Redis that refuses SCAN: the role change commits, and its deletion fails.
  const refusingScan = new Proxy(services.redis, {
    get(target, prop, receiver) {
      if (prop === "scan") return () => Promise.reject(new Error("scan refused"));
      return Reflect.get(target, prop, receiver);
    },
  });
  const auth = createAuthService(services.pg, refusingScan, config);
  const app = buildApp(
    { ...services, auth },
    { level: "info", stream: { write: (line: string) => lines.push(line) } },
  );
  await app.ready();
  try {
    const res = await app.inject({
      method: "POST",
      url: `/v1/users/${userId}/role`,
      payload: { role: "analyst" },
      ...asAdmin,
    });
    expect(res.statusCode).toBe(503);
    expect(await query("SELECT role FROM users WHERE user_id = $1", [userId])).toEqual([
      { role: "analyst" },
    ]);
    const logged = lines
      .map((l) => JSON.parse(l) as { level: number; msg: string; error?: { message?: string } })
      .filter((l) => l.msg === "store unavailable" && l.level === 50);
    expect(logged.some((l) => l.error?.message?.includes(userId))).toBe(true);
  } finally {
    await app.close();
    await services.close();
  }
});

test("a change Postgres never received deletes no session, the target's org unread", async () => {
  // Another org's user, with a live session: a request that fails before its
  // transaction began must not reach it.
  const victim = await newUser("viewer", "apiauth-013-elsewhere");
  await seedSession(config, {
    token: "tok-013-victim",
    userId: victim.userId,
    role: "viewer",
    orgId: "apiauth-013-elsewhere",
  });
  const services = await buildServices(config);
  const refusing = createPgPool(`postgres://u:p@127.0.0.1:${await closedPort()}/db`);
  const auth = createAuthService(refusing, services.redis, config);
  const app = buildApp({ ...services, pg: refusing, auth });
  await app.ready();
  try {
    const res = await app.inject({
      method: "POST",
      url: `/v1/users/${victim.userId}/status`,
      payload: { status: "disabled" },
      ...asAdmin,
    });
    expect(res.statusCode).toBe(503);
    expect(await sessionsOf(victim.userId)).toEqual(["cgsess:tok-013-victim"]);
  } finally {
    await app.close();
    await refusing.end();
    await services.close();
  }
});

test("a change on a connection whose peer goes silent answers 503 within 8 s", async () => {
  const { userId } = await newUser("viewer");
  const target = new URL(config.API_PG_URL);
  let silent = false;
  const pipes = new Set<Socket>();
  const proxy = await new Promise<Server>((resolve) => {
    const server = createServer((client) => {
      const upstream = createConnection({ host: target.hostname, port: Number(target.port) });
      pipes.add(client);
      pipes.add(upstream);
      client.on("data", (chunk) => upstream.write(chunk));
      upstream.on("data", (chunk) => {
        if (!silent) client.write(chunk);
      });
      client.on("error", () => upstream.destroy());
      upstream.on("error", () => client.destroy());
    });
    server.listen({ host: "127.0.0.1", port: 0 }, () => resolve(server));
  });
  const via = new URL(config.API_PG_URL);
  via.hostname = "127.0.0.1";
  via.port = String((proxy.address() as AddressInfo).port);
  const pool = createPgPool(via.toString());
  await pool.query("SELECT 1");
  const services = await buildServices(config);
  const auth = createAuthService(pool, services.redis, config);
  const app = buildApp({ ...services, pg: pool, auth });
  await app.ready();
  try {
    silent = true;
    const started = Date.now();
    const res = await app.inject({
      method: "POST",
      url: `/v1/users/${userId}/role`,
      payload: { role: "analyst" },
      ...asAdmin,
    });
    expect(res.statusCode).toBe(503);
    expect(Date.now() - started).toBeLessThan(8000);
  } finally {
    await app.close();
    await pool.end().catch(() => {});
    await services.close();
    for (const s of pipes) s.destroy();
    await new Promise<void>((resolve) => proxy.close(() => resolve()));
  }
});

/** A port on 127.0.0.1 that was bound a moment ago and is closed now: it refuses. */
function closedPort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const server = createServer();
    server.once("error", reject);
    server.listen({ host: "127.0.0.1", port: 0 }, () => {
      const { port } = server.address() as AddressInfo;
      server.close(() => resolve(port));
    });
  });
}
