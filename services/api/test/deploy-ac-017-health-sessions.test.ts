import { type AddressInfo, createServer } from "node:net";
import { createClient } from "@clickhouse/client";
import type { FastifyInstance } from "fastify";
import { Redis } from "ioredis";
import { afterAll, beforeAll, describe, expect, inject, test } from "vitest";
import { buildApp } from "../src/app.js";
import { createAuthService } from "../src/auth/service.js";
import { type Config, loadConfig } from "../src/config.js";
import { type ApiServer, startApi } from "../src/server.js";
import { type Services, buildServices, createPgPool, createRedisClient } from "../src/services.js";
import { DEFAULT_PASSWORD, currentTotp, insertUserDirect, seedSession } from "./helpers/db.js";

// deploy_ac_017 — SPEC-023 §Data contracts §3 and debt #20: `GET /health` answers
// `200 { "status": "ok" }`, and, once the api has started, still does while the stores
// are unreachable; with API_SESSION_IDLE_MINUTES=5 and API_SESSION_MAX_HOURS=1, a new
// session's idle lifetime and cap are those.

const MINUTE = 60 * 1000;
const HOUR = 60 * MINUTE;
/** How far a time-to-live read after a request may fall below the one the request set. */
const SLACK = 5000;

let config: Config;
let redis: Redis;
const closers: (() => Promise<void>)[] = [];

beforeAll(() => {
  config = inject("apiConfig");
  redis = new Redis(config.API_REDIS_URL);
});
afterAll(async () => {
  for (const close of closers) await close();
  await redis?.quit();
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

describe("GET /health", () => {
  test("answers 200 { status: ok } on the started api", async () => {
    const api: ApiServer = await startApi(config);
    closers.push(() => api.close());
    const res = await fetch(`${api.url}/health`);
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({ status: "ok" });
  });

  test("still answers 200 when every store is unreachable", async () => {
    const real = await buildServices(config);
    const port = await closedPort();
    const pg = createPgPool(`postgres://u:p@127.0.0.1:${port}/db`);
    const unreachableRedis = createRedisClient(`redis://127.0.0.1:${port}`);
    unreachableRedis.on("error", () => {});
    const ch = createClient({ url: `http://127.0.0.1:${port}` });
    const services: Services = {
      ...real,
      pg,
      redis: unreachableRedis,
      ch,
      auth: createAuthService(pg, unreachableRedis, config),
    };
    const app = buildApp(services);
    closers.push(async () => {
      await app.close();
      await pg.end();
      unreachableRedis.disconnect();
      await ch.close();
      await real.close();
    });
    const res = await app.inject({ method: "GET", url: "/health" });
    expect(res.statusCode).toBe(200);
    expect(res.json()).toEqual({ status: "ok" });
    // The stores are unreachable indeed: a route that reads one does not answer 200.
    const read = await app.inject({
      method: "GET",
      url: "/v1/incidents",
      cookies: { cgsess: "x" },
    });
    expect(read.statusCode).toBe(503);
  });
});

describe("the session's lifetimes are settings", () => {
  let app: FastifyInstance;
  const short: Partial<Config> = { API_SESSION_IDLE_MINUTES: 5, API_SESSION_MAX_HOURS: 1 };
  const key = (token: string) => `cgsess:${token}`;

  beforeAll(async () => {
    const services = await buildServices({ ...config, ...short });
    app = buildApp(services);
    await app.ready();
    closers.push(async () => {
      await app.close();
      await services.close();
    });
  });

  test("the defaults are 480 minutes and 24 hours", () => {
    const parsed = loadConfig({
      API_PG_URL: "postgres://u:p@127.0.0.1:5432/db",
      API_REDIS_URL: "redis://127.0.0.1:6379",
      API_CH_URL: "http://127.0.0.1:8123",
      API_DB_ENC_PASSPHRASE: "x",
      API_FORENSIC_PASSPHRASE: "y",
    });
    expect(parsed.API_SESSION_IDLE_MINUTES).toBe(480);
    expect(parsed.API_SESSION_MAX_HOURS).toBe(24);
  });

  test("a login's entry lives 5 minutes", async () => {
    await insertUserDirect(config, {
      email: "deploy017@example.test",
      role: "viewer",
      totpEnrolled: true,
    });
    const res = await app.inject({
      method: "POST",
      url: "/v1/auth/login",
      remoteAddress: "198.18.17.1",
      payload: {
        email: "deploy017@example.test",
        password: DEFAULT_PASSWORD,
        totp_code: currentTotp(),
      },
    });
    expect(res.statusCode).toBe(200);
    const token = res.cookies.find((c) => c.name === "cgsess")?.value ?? "";
    const ttl = await redis.pttl(key(token));
    expect(ttl).toBeGreaterThan(5 * MINUTE - SLACK);
    expect(ttl).toBeLessThanOrEqual(5 * MINUTE);
  });

  test("a use gives a session 5 minutes again, and no more than its cap's remainder", async () => {
    const use = (token: string) =>
      app.inject({ method: "GET", url: "/v1/incidents", cookies: { cgsess: token } });
    await seedSession(config, {
      token: "tok-deploy017-idle",
      userId: globalThis.crypto.randomUUID(),
      role: "viewer",
      orgId: "deploy017-org",
      issuedAt: new Date(Date.now() - 10 * MINUTE).toISOString(),
    });
    await redis.pexpire(key("tok-deploy017-idle"), 1000 * 30);
    expect((await use("tok-deploy017-idle")).statusCode).toBe(200);
    const ttl = await redis.pttl(key("tok-deploy017-idle"));
    expect(ttl).toBeGreaterThan(5 * MINUTE - SLACK);
    expect(ttl).toBeLessThanOrEqual(5 * MINUTE);

    await seedSession(config, {
      token: "tok-deploy017-capped",
      userId: globalThis.crypto.randomUUID(),
      role: "viewer",
      orgId: "deploy017-org",
      issuedAt: new Date(Date.now() - HOUR + 2 * MINUTE).toISOString(),
    });
    expect((await use("tok-deploy017-capped")).statusCode).toBe(200);
    const capped = await redis.pttl(key("tok-deploy017-capped"));
    expect(capped).toBeGreaterThan(2 * MINUTE - SLACK);
    expect(capped).toBeLessThanOrEqual(2 * MINUTE);
  });

  test("a session issued an hour and a second ago is no session", async () => {
    await seedSession(config, {
      token: "tok-deploy017-past",
      userId: globalThis.crypto.randomUUID(),
      role: "viewer",
      orgId: "deploy017-org",
      issuedAt: new Date(Date.now() - HOUR - 1000).toISOString(),
    });
    const res = await app.inject({
      method: "GET",
      url: "/v1/incidents",
      cookies: { cgsess: "tok-deploy017-past" },
    });
    expect(res.statusCode).toBe(401);
    expect(await redis.exists(key("tok-deploy017-past"))).toBe(0);
  });
});
