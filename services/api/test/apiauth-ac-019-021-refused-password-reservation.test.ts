import type { FastifyInstance } from "fastify";
import { Redis } from "ioredis";
import pg from "pg";
import { afterAll, beforeAll, describe, expect, inject, test } from "vitest";
import { buildApp } from "../src/app.js";
import { createAuthService } from "../src/auth/service.js";
import type { Config } from "../src/config.js";
import { buildServices } from "../src/services.js";
import { DEFAULT_PASSWORD, currentTotp, insertUserDirect, seedSession } from "./helpers/db.js";

// apiauth_ac_019 to 021 — SPEC-008 Amendment 2026-10-16: a role or CSRF refusal writes an
// access.denied row (debt #80); the password change is limited, records its failures and
// uses up its code (debt #100); every limited attempt reserves its slots atomically before
// the credentials are checked (debt #101).

let config: Config;
let redis: Redis;
const closers: (() => Promise<void>)[] = [];
let app: FastifyInstance;
let auditRefused: FastifyInstance;
let usersRefused: FastifyInstance;
const logLines: string[] = [];

/** An app whose Postgres refuses the statements `refuse` picks, and answers the rest. */
async function appWith(refuse?: (sql: string) => boolean): Promise<FastifyInstance> {
  const services = await buildServices(config);
  const refusing = new Proxy(services.pg, {
    get(target, prop, receiver) {
      if (prop !== "query") return Reflect.get(target, prop, receiver);
      return (text: unknown, ...rest: unknown[]) =>
        typeof text === "string" && refuse?.(text)
          ? Promise.reject(new Error("the statement is refused"))
          : (target.query as (...a: unknown[]) => unknown).call(target, text, ...rest);
    },
  });
  // The auth service reads through the same pool, so a refusal reaches the login too.
  const auth = createAuthService(refusing, services.redis, config);
  const built = buildApp(refuse ? { ...services, pg: refusing, auth } : services, {
    level: "info",
    stream: { write: (line: string) => logLines.push(line) },
  });
  await built.ready();
  closers.push(async () => {
    await built.close();
    await services.close();
  });
  return built;
}

/** The reqId the app logged for `method url`, among the lines from `from` on. */
function reqIdOf(method: string, url: string, from: number): string | undefined {
  return logLines
    .slice(from)
    .map(
      (l) =>
        JSON.parse(l) as { reqId?: string; msg?: string; req?: { method?: string; url?: string } },
    )
    .find((l) => l.msg === "incoming request" && l.req?.method === method && l.req?.url === url)
    ?.reqId;
}

interface Row {
  user_id: string | null;
  org_id: string;
  action: string;
  outcome: string;
  target_id: string | null;
  source_ip: string | null;
  correlation_id: string | null;
  detail: Record<string, string> | null;
}

async function rowsOf(userId: string): Promise<Row[]> {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    const r = await pool.query<Row>(
      `SELECT user_id::text, org_id, action, outcome, target_id, host(source_ip) AS source_ip,
              correlation_id::text, detail
         FROM audit_log WHERE user_id = $1 ORDER BY id`,
      [userId],
    );
    return r.rows;
  } finally {
    await pool.end();
  }
}

async function countOf(key: string): Promise<number> {
  return Number((await redis.get(key)) ?? 0);
}

let seq = 0;
function address(): string {
  seq += 1;
  return `198.18.${Math.floor(seq / 250)}.${(seq % 250) + 1}`;
}

beforeAll(async () => {
  config = inject("apiConfig");
  redis = new Redis(config.API_REDIS_URL);
  app = await appWith();
  auditRefused = await appWith((sql) => sql.trimStart().startsWith("INSERT INTO audit_log"));
  // The login's read of the account, so that a login ends in a store failure.
  usersRefused = await appWith((sql) =>
    sql.includes("FROM users WHERE org_id = $2 AND email = $3"),
  );
});
afterAll(async () => {
  for (const close of closers) await close();
  await redis?.quit();
});

describe("apiauth_ac_019 — refused requests", () => {
  test("an analyst's POST /v1/users writes one access.denied row with its reason and route", async () => {
    const userId = globalThis.crypto.randomUUID();
    await seedSession(config, {
      token: "tok-019-analyst",
      userId,
      role: "analyst",
      csrfToken: "c019",
      orgId: "ac019-org",
    });
    const ip = address();
    const mark = logLines.length;
    const res = await app.inject({
      method: "POST",
      url: "/v1/users",
      cookies: { cgsess: "tok-019-analyst" },
      headers: { "x-csrf-token": "c019" },
      remoteAddress: ip,
      payload: { email: "nobody@ac019.test", role: "viewer", password: "a long password 1" },
    });
    expect(res.statusCode).toBe(403);
    expect(await rowsOf(userId)).toEqual([
      {
        user_id: userId,
        org_id: "ac019-org",
        action: "access.denied",
        outcome: "denied",
        target_id: null,
        source_ip: ip,
        correlation_id: reqIdOf("POST", "/v1/users", mark),
        detail: { reason: "role", route: "POST /v1/users" },
      },
    ]);
  });

  test("an admin's POST /v1/users without X-CSRF-Token writes one with reason csrf", async () => {
    const userId = globalThis.crypto.randomUUID();
    await seedSession(config, { token: "tok-019-admin", userId, role: "admin" });
    const res = await app.inject({
      method: "POST",
      url: "/v1/users",
      cookies: { cgsess: "tok-019-admin" },
      remoteAddress: address(),
      payload: { email: "nobody2@ac019.test", role: "viewer", password: "a long password 1" },
    });
    expect(res.statusCode).toBe(403);
    const rows = await rowsOf(userId);
    expect(rows.map((r) => [r.action, r.outcome, r.detail])).toEqual([
      ["access.denied", "denied", { reason: "csrf", route: "POST /v1/users" }],
    ]);
  });

  test("a viewer's forensic export and report write one access.denied row each, no forensic row", async () => {
    const userId = globalThis.crypto.randomUUID();
    await seedSession(config, { token: "tok-019-viewer", userId, role: "viewer" });
    for (const suffix of ["forensic-export", "report"]) {
      const res = await app.inject({
        method: "GET",
        url: `/v1/incidents/${globalThis.crypto.randomUUID()}/${suffix}`,
        cookies: { cgsess: "tok-019-viewer" },
        remoteAddress: address(),
      });
      expect(res.statusCode, suffix).toBe(403);
    }
    const rows = await rowsOf(userId);
    expect(rows.map((r) => [r.action, r.detail])).toEqual([
      ["access.denied", { reason: "role", route: "GET /v1/incidents/:id/forensic-export" }],
      ["access.denied", { reason: "role", route: "GET /v1/incidents/:id/report" }],
    ]);
  });

  test("a refused request with a query string records the route's pattern without it", async () => {
    const userId = globalThis.crypto.randomUUID();
    await seedSession(config, { token: "tok-019-query", userId, role: "viewer" });
    const res = await app.inject({
      method: "GET",
      url: "/v1/users?limit=5&email=someone%40example.test",
      cookies: { cgsess: "tok-019-query" },
      remoteAddress: address(),
    });
    expect(res.statusCode).toBe(403);
    expect((await rowsOf(userId)).map((r) => r.detail)).toEqual([
      { reason: "role", route: "GET /v1/users" },
    ]);
  });

  test("a request with no session writes no row", async () => {
    const pool = new pg.Pool({ connectionString: config.API_PG_URL });
    try {
      const before = (await pool.query("SELECT count(*)::int AS n FROM audit_log")).rows[0].n;
      const res = await app.inject({ method: "GET", url: "/v1/users", remoteAddress: address() });
      expect(res.statusCode).toBe(401);
      const after = (await pool.query("SELECT count(*)::int AS n FROM audit_log")).rows[0].n;
      expect(after).toBe(before);
    } finally {
      await pool.end();
    }
  });

  test("when the row cannot be written, the refused request answers 503", async () => {
    const userId = globalThis.crypto.randomUUID();
    await seedSession(config, { token: "tok-019-refused", userId, role: "viewer" });
    const res = await auditRefused.inject({
      method: "GET",
      url: "/v1/users",
      cookies: { cgsess: "tok-019-refused" },
      remoteAddress: address(),
    });
    expect(res.statusCode).toBe(503);
    expect(res.json()).toEqual({ error: "unavailable" });
  });
});

/** A user with a confirmed TOTP, and a session of its own. */
async function signedIn(tag: string) {
  const email = `${tag}@ac020.test`;
  const userId = await insertUserDirect(config, { email, role: "viewer" });
  const token = `tok-${tag}`;
  await seedSession(config, { token, userId, role: "viewer", csrfToken: `c-${tag}` });
  return { email, userId, token, csrf: `c-${tag}` };
}

function changePassword(
  who: { token: string; csrf: string },
  body: Record<string, string>,
  ip: string,
) {
  return app.inject({
    method: "POST",
    url: "/v1/auth/password",
    cookies: { cgsess: who.token },
    headers: { "x-csrf-token": who.csrf },
    remoteAddress: ip,
    payload: body,
  });
}

describe("apiauth_ac_020 — the password change", () => {
  test("a wrong current password and a wrong code each answer 401 and write a denied row", async () => {
    const who = await signedIn("pw-wrong");
    const wrongPassword = await changePassword(
      who,
      {
        currentPassword: "not the password",
        newPassword: "a brand new password 1",
        totp_code: currentTotp(),
      },
      address(),
    );
    expect(wrongPassword.statusCode).toBe(401);
    const wrongCode = await changePassword(
      who,
      {
        currentPassword: DEFAULT_PASSWORD,
        newPassword: "a brand new password 1",
        totp_code: "000000",
      },
      address(),
    );
    expect(wrongCode.statusCode).toBe(401);
    const rows = (await rowsOf(who.userId)).filter((r) => r.action === "user.password.change");
    expect(rows.map((r) => r.outcome)).toEqual(["denied", "denied"]);
  });

  test("after five failures for one user, from five addresses, the sixth answers 429", async () => {
    const who = await signedIn("pw-limit");
    for (let i = 0; i < 5; i++) {
      const res = await changePassword(
        who,
        {
          currentPassword: "not the password",
          newPassword: "a brand new password 1",
          totp_code: "000000",
        },
        address(),
      );
      expect(res.statusCode, `attempt ${i}`).toBe(401);
    }
    const sixth = await changePassword(
      who,
      {
        currentPassword: DEFAULT_PASSWORD,
        newPassword: "a brand new password 1",
        totp_code: currentTotp(),
      },
      address(),
    );
    expect(sixth.statusCode).toBe(429);
  });

  test("failed TOTP confirmations of the user do not count toward it", async () => {
    const who = await signedIn("pw-confirm");
    for (let i = 0; i < 5; i++) {
      const confirm = await app.inject({
        method: "POST",
        url: "/v1/auth/totp/confirm",
        remoteAddress: address(),
        payload: { userId: who.userId, totp_code: "000000" },
      });
      // Refused as the confirmation refuses an enrolled user: it counted on its own.
      expect(confirm.statusCode, `confirmation ${i}`).toBe(401);
    }
    expect(await countOf(`rl:user:${who.userId}`)).toBe(5);
    const res = await changePassword(
      who,
      {
        currentPassword: DEFAULT_PASSWORD,
        newPassword: "a brand new password 1",
        totp_code: currentTotp(),
      },
      address(),
    );
    expect(res.statusCode).toBe(200);
  });

  test("failed logins from its address do not count toward it, nor its failures toward them", async () => {
    const who = await signedIn("pw-ip");
    const ip = address();
    for (let i = 0; i < 10; i++) {
      await app.inject({
        method: "POST",
        url: "/v1/auth/login",
        remoteAddress: ip,
        payload: { email: `pw-ip-${i}@ac020.test`, password: "x", totp_code: "000000" },
      });
    }
    expect(await countOf(`rl:ip:${ip}`)).toBe(10);
    const wrong = await changePassword(
      who,
      {
        currentPassword: "not the password",
        newPassword: "a brand new password 1",
        totp_code: "000000",
      },
      ip,
    );
    expect(wrong.statusCode).toBe(401);
    expect(await countOf(`rl:ip:${ip}`)).toBe(10);
    const ok = await changePassword(
      who,
      {
        currentPassword: DEFAULT_PASSWORD,
        newPassword: "a brand new password 1",
        totp_code: currentTotp(),
      },
      ip,
    );
    expect(ok.statusCode).toBe(200);
  });

  test("a code that changed the password neither opens a session nor changes it again", async () => {
    const who = await signedIn("pw-code");
    const code = currentTotp();
    const changed = await changePassword(
      who,
      { currentPassword: DEFAULT_PASSWORD, newPassword: "a brand new password 1", totp_code: code },
      address(),
    );
    expect(changed.statusCode).toBe(200);
    const login = await app.inject({
      method: "POST",
      url: "/v1/auth/login",
      remoteAddress: address(),
      payload: { email: who.email, password: "a brand new password 1", totp_code: code },
    });
    expect(login.statusCode).toBe(401);
    await seedSession(config, {
      token: `${who.token}-2`,
      userId: who.userId,
      role: "viewer",
      csrfToken: "c2",
    });
    const again = await changePassword(
      { token: `${who.token}-2`, csrf: "c2" },
      {
        currentPassword: "a brand new password 1",
        newPassword: "another new password 2",
        totp_code: code,
      },
      address(),
    );
    expect(again.statusCode).toBe(401);
  });
});

describe("apiauth_ac_021 — the reservation", () => {
  test("twenty parallel failed logins from one address answer ten 401 and ten 429", async () => {
    const ip = address();
    const results = await Promise.all(
      Array.from({ length: 20 }, (_, i) =>
        app.inject({
          method: "POST",
          url: "/v1/auth/login",
          remoteAddress: ip,
          payload: { email: `burst-${i}@ac021.test`, password: "x", totp_code: "000000" },
        }),
      ),
    );
    const statuses = results.map((r) => r.statusCode).sort();
    expect(statuses.filter((s) => s === 401).length).toBe(10);
    expect(statuses.filter((s) => s === 429).length).toBe(10);
  });

  test("six parallel failed logins of one account, from six addresses, answer five 401 and one 429", async () => {
    const results = await Promise.all(
      Array.from({ length: 6 }, () =>
        app.inject({
          method: "POST",
          url: "/v1/auth/login",
          remoteAddress: address(),
          payload: { email: "one-account@ac021.test", password: "x", totp_code: "000000" },
        }),
      ),
    );
    const statuses = results.map((r) => r.statusCode);
    expect(statuses.filter((s) => s === 401).length).toBe(5);
    expect(statuses.filter((s) => s === 429).length).toBe(1);
  });

  test("parallel failed TOTP confirmations of one user id answer five 401", async () => {
    const userId = globalThis.crypto.randomUUID();
    const results = await Promise.all(
      Array.from({ length: 8 }, () =>
        app.inject({
          method: "POST",
          url: "/v1/auth/totp/confirm",
          remoteAddress: address(),
          payload: { userId, totp_code: "000000" },
        }),
      ),
    );
    const statuses = results.map((r) => r.statusCode);
    expect(statuses.filter((s) => s === 401).length).toBe(5);
    expect(statuses.filter((s) => s === 429).length).toBe(3);
  });

  test("parallel failed password changes of one user answer five 401", async () => {
    const who = await signedIn("pw-burst");
    const results = await Promise.all(
      Array.from({ length: 8 }, () =>
        changePassword(
          who,
          {
            currentPassword: "not the password",
            newPassword: "a brand new password 1",
            totp_code: "000000",
          },
          address(),
        ),
      ),
    );
    const statuses = results.map((r) => r.statusCode);
    expect(statuses.filter((s) => s === 401).length).toBe(5);
    expect(statuses.filter((s) => s === 429).length).toBe(3);
  });

  test("a successful login gives its slot back to the address, and keeps its window", async () => {
    const email = "slot-back@ac021.test";
    await insertUserDirect(config, { email, role: "viewer" });
    const ip = address();
    await redis.del(`rl:ip:${ip}`);
    const failed = await app.inject({
      method: "POST",
      url: "/v1/auth/login",
      remoteAddress: ip,
      payload: { email, password: "x", totp_code: "000000" },
    });
    expect(failed.statusCode).toBe(401);
    await redis.expire(`rl:ip:${ip}`, 100);
    const ok = await app.inject({
      method: "POST",
      url: "/v1/auth/login",
      remoteAddress: ip,
      payload: { email, password: DEFAULT_PASSWORD, totp_code: currentTotp() },
    });
    expect(ok.statusCode).toBe(200);
    expect(await countOf(`rl:ip:${ip}`)).toBe(1);
    const kept = await redis.ttl(`rl:ip:${ip}`);
    expect(kept).toBeGreaterThan(0);
    expect(kept).toBeLessThanOrEqual(100);
  });

  test("a login that ends in a store failure gives its slots back", async () => {
    const ip = address();
    const email = "store-failure@ac021.test";
    await redis.set(`rl:ip:${ip}`, "3", "EX", 900);
    await redis.set(`rl:acct:default:${email}`, "2", "EX", 900);
    const res = await usersRefused.inject({
      method: "POST",
      url: "/v1/auth/login",
      remoteAddress: ip,
      payload: { email, password: "x", totp_code: "000000" },
    });
    expect(res.statusCode).toBe(503);
    expect(await countOf(`rl:ip:${ip}`)).toBe(3);
    expect(await countOf(`rl:acct:default:${email}`)).toBe(2);
  });

  test("an attempt answered 429 does not restart the window", async () => {
    const ip = address();
    for (let i = 0; i < 10; i++) {
      await app.inject({
        method: "POST",
        url: "/v1/auth/login",
        remoteAddress: ip,
        payload: { email: `window-${i}@ac021.test`, password: "x", totp_code: "000000" },
      });
    }
    await redis.expire(`rl:ip:${ip}`, 100);
    const refused = await app.inject({
      method: "POST",
      url: "/v1/auth/login",
      remoteAddress: ip,
      payload: { email: "window-x@ac021.test", password: "x", totp_code: "000000" },
    });
    expect(refused.statusCode).toBe(429);
    const kept = await redis.ttl(`rl:ip:${ip}`);
    expect(kept).toBeGreaterThan(0);
    expect(kept).toBeLessThanOrEqual(100);
    expect(await countOf(`rl:ip:${ip}`)).toBe(10);
  });
});
