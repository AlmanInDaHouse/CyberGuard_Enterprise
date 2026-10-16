import type { FastifyInstance } from "fastify";
import { Redis } from "ioredis";
import { afterAll, beforeAll, describe, expect, inject, test } from "vitest";
import { buildApp } from "../src/app.js";
import { createAuthService } from "../src/auth/service.js";
import { resolveSession } from "../src/auth/session.js";
import type { Config } from "../src/config.js";
import { buildServices } from "../src/services.js";
import { DEFAULT_PASSWORD, currentTotp, insertUserDirect, seedSession } from "./helpers/db.js";

// apiauth_ac_022 — SPEC-008's second Amendment 2026-10-16: a session ends after 8 h
// without use, and 24 h after its login at the latest (debt #82). The tests move a
// session in time by editing its entry's issued_at and time-to-live, not by waiting.

const HOUR = 60 * 60 * 1000;
/** How far a time-to-live read after a request may fall below the one the request set. */
const SLACK = 5000;

let config: Config;
let redis: Redis;
let app: FastifyInstance;
const closers: (() => Promise<void>)[] = [];

/** An app whose auth service talks to Redis through `client`. */
async function appWith(client?: (real: Redis) => Redis): Promise<FastifyInstance> {
  const services = await buildServices(config);
  const r = client ? client(services.redis) : services.redis;
  const built = buildApp(
    client ? { ...services, redis: r, auth: createAuthService(services.pg, r, config) } : services,
  );
  await built.ready();
  closers.push(async () => {
    await built.close();
    await services.close();
  });
  return built;
}

/** A Redis client whose `command` always fails, and whose other commands pass. */
function refusing(command: "set" | "del") {
  return (real: Redis): Redis =>
    new Proxy(real, {
      get(target, prop) {
        if (prop === command) return () => Promise.reject(new Error(`${command} is refused`));
        const value = Reflect.get(target, prop, target);
        return typeof value === "function" ? value.bind(target) : value;
      },
    });
}

beforeAll(async () => {
  config = inject("apiConfig");
  redis = new Redis(config.API_REDIS_URL);
  app = await appWith();
});
afterAll(async () => {
  for (const close of closers) await close();
  await redis?.quit();
});

const key = (token: string) => `cgsess:${token}`;

interface Entry {
  user_id: string;
  org_id: string;
  role: string;
  csrf_token: string;
  issued_at: string;
  last_seen: string;
}

async function entryOf(token: string): Promise<Entry | null> {
  const raw = await redis.get(key(token));
  return raw === null ? null : (JSON.parse(raw) as Entry);
}

/** A request a session authorizes: every role reads the incident list. */
function use(token: string, on: FastifyInstance = app) {
  return on.inject({ method: "GET", url: "/v1/incidents", cookies: { cgsess: token } });
}

async function seeded(token: string, issuedAt: Date | string): Promise<void> {
  await seedSession(config, {
    token,
    userId: globalThis.crypto.randomUUID(),
    role: "viewer",
    orgId: "ac022-org",
    issuedAt: typeof issuedAt === "string" ? issuedAt : issuedAt.toISOString(),
  });
}

describe("apiauth_ac_022 — the session's lifetime", () => {
  test("a login's entry lives 8 h", async () => {
    await insertUserDirect(config, {
      email: "ac022@example.test",
      role: "viewer",
      totpEnrolled: true,
    });
    const res = await app.inject({
      method: "POST",
      url: "/v1/auth/login",
      remoteAddress: "198.18.222.1",
      payload: {
        email: "ac022@example.test",
        password: DEFAULT_PASSWORD,
        totp_code: currentTotp(),
      },
    });
    expect(res.statusCode).toBe(200);
    const token = res.cookies.find((c) => c.name === "cgsess")?.value ?? "";
    const ttl = await redis.pttl(key(token));
    expect(ttl).toBeGreaterThan(8 * HOUR - SLACK);
    expect(ttl).toBeLessThanOrEqual(8 * HOUR);
  });

  test("a use of a session idle for 7 h 59 min gives it 8 h again, and records the use", async () => {
    await seeded("tok-022-idle", new Date(Date.now() - 2 * HOUR));
    await redis.pexpire(key("tok-022-idle"), 60_000);
    const seededEntry = await entryOf("tok-022-idle");
    const before = Date.now();
    expect((await use("tok-022-idle")).statusCode).toBe(200);
    const after = Date.now();
    const ttl = await redis.pttl(key("tok-022-idle"));
    expect(ttl).toBeGreaterThan(8 * HOUR - SLACK);
    expect(ttl).toBeLessThanOrEqual(8 * HOUR);
    const used = await entryOf("tok-022-idle");
    const seen = Date.parse(used?.last_seen ?? "");
    expect(seen).toBeGreaterThanOrEqual(before);
    expect(seen).toBeLessThanOrEqual(after);
    // The rewrite changes last_seen alone.
    expect({ ...used, last_seen: seededEntry?.last_seen }).toEqual(seededEntry);
  });

  test("a use of a session issued 23 h ago gives it the hour left to its cap, not 8 h, and keeps issued_at", async () => {
    const issuedAt = new Date(Date.now() - 23 * HOUR).toISOString();
    await seeded("tok-022-capped", issuedAt);
    await redis.pexpire(key("tok-022-capped"), 60_000);
    expect((await use("tok-022-capped")).statusCode).toBe(200);
    const ttl = await redis.pttl(key("tok-022-capped"));
    expect(ttl).toBeGreaterThan(HOUR - SLACK);
    expect(ttl).toBeLessThanOrEqual(HOUR);
    expect((await entryOf("tok-022-capped"))?.issued_at).toBe(issuedAt);
    expect((await use("tok-022-capped")).statusCode).toBe(200);
    expect(await redis.pttl(key("tok-022-capped"))).toBeLessThanOrEqual(HOUR);
  });

  test("a request the CSRF check refuses is a use", async () => {
    await seedSession(config, {
      token: "tok-022-csrf",
      userId: globalThis.crypto.randomUUID(),
      role: "admin",
      orgId: "ac022-org",
    });
    await redis.pexpire(key("tok-022-csrf"), 60_000);
    const res = await app.inject({
      method: "POST",
      url: "/v1/users",
      cookies: { cgsess: "tok-022-csrf" },
      headers: { "x-csrf-token": "not-the-session-token" },
      payload: { email: "nobody2@ac022.test", role: "viewer", password: "a long password 1" },
    });
    expect(res.statusCode).toBe(403);
    expect(await redis.pttl(key("tok-022-csrf"))).toBeGreaterThan(8 * HOUR - SLACK);
  });

  test("a request the role check refuses is a use", async () => {
    await seedSession(config, {
      token: "tok-022-refused",
      userId: globalThis.crypto.randomUUID(),
      role: "viewer",
      orgId: "ac022-org",
    });
    await redis.pexpire(key("tok-022-refused"), 60_000);
    const res = await app.inject({
      method: "POST",
      url: "/v1/users",
      cookies: { cgsess: "tok-022-refused" },
      headers: { "x-csrf-token": "csrf-tok-022-refused" },
      payload: { email: "nobody@ac022.test", role: "viewer", password: "a long password 1" },
    });
    expect(res.statusCode).toBe(403);
    expect(await redis.pttl(key("tok-022-refused"))).toBeGreaterThan(8 * HOUR - SLACK);
  });

  test("a session issued 24 h and one second ago is no session, and its entry is gone", async () => {
    await seeded("tok-022-past", new Date(Date.now() - 24 * HOUR - 1000));
    expect((await use("tok-022-past")).statusCode).toBe(401);
    expect(await redis.exists(key("tok-022-past"))).toBe(0);
  });

  test("an entry whose issued_at is not a time is no session, and is deleted", async () => {
    await seeded("tok-022-garbled", "not a time");
    expect((await use("tok-022-garbled")).statusCode).toBe(401);
    expect(await redis.exists(key("tok-022-garbled"))).toBe(0);
  });

  test("an entry that is not a JSON object is no session, and is deleted", async () => {
    const entries: [string, string][] = [
      ["tok-022-text", "not json"],
      ["tok-022-null", "null"],
    ];
    for (const [token, value] of entries) {
      await redis.set(key(token), value, "EX", 3600);
      expect((await use(token)).statusCode, value).toBe(401);
      expect(await redis.exists(key(token)), value).toBe(0);
    }
  });

  test("an entry with no issued_at is no session, and is deleted", async () => {
    await seeded("tok-022-unissued", new Date());
    const { issued_at: _, ...rest } = (await entryOf("tok-022-unissued")) ?? {};
    await redis.set(key("tok-022-unissued"), JSON.stringify(rest), "KEEPTTL");
    expect((await use("tok-022-unissued")).statusCode).toBe(401);
    expect(await redis.exists(key("tok-022-unissued"))).toBe(0);
  });

  test("a use does not bring back a session deleted after its entry was read", async () => {
    await seeded("tok-022-raced", new Date());
    // The entry is deleted once the read returns it, as a revocation between the read
    // and the use's rewrite would.
    const racing = new Proxy(redis, {
      get(target, prop) {
        if (prop === "get") {
          return async (k: string) => {
            const value = await target.get(k);
            await target.del(k);
            return value;
          };
        }
        const value = Reflect.get(target, prop, target);
        return typeof value === "function" ? value.bind(target) : value;
      },
    });
    const session = await resolveSession(racing, "tok-022-raced");
    expect(session?.role).toBe("viewer");
    expect(await redis.exists(key("tok-022-raced"))).toBe(0);
  });

  test("a logout still ends the session", async () => {
    await seeded("tok-022-out", new Date());
    const out = await app.inject({
      method: "POST",
      url: "/v1/auth/logout",
      cookies: { cgsess: "tok-022-out" },
      headers: { "x-csrf-token": "csrf-tok-022-out" },
    });
    expect(out.statusCode).toBe(200);
    expect((await use("tok-022-out")).statusCode).toBe(401);
  });

  test("a rewrite Redis fails answers 503, and so does the delete of a session past its cap", async () => {
    const noSet = await appWith(refusing("set"));
    await seeded("tok-022-noset", new Date());
    expect((await use("tok-022-noset", noSet)).statusCode).toBe(503);
    const noDel = await appWith(refusing("del"));
    await seeded("tok-022-nodel", new Date(Date.now() - 25 * HOUR));
    expect((await use("tok-022-nodel", noDel)).statusCode).toBe(503);
  });
});
