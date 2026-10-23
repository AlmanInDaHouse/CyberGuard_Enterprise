import { type ChildProcess, spawn } from "node:child_process";
import { type AddressInfo, type Socket, connect, createServer } from "node:net";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import type { FastifyInstance } from "fastify";
import { Redis } from "ioredis";
import { afterAll, beforeAll, describe, expect, inject, test } from "vitest";
import { buildApp } from "../src/app.js";
import { type Config, loadConfig } from "../src/config.js";
import { type ApiServer, startApi } from "../src/server.js";
import { buildServices } from "../src/services.js";
import { DEFAULT_PASSWORD, currentTotp, insertUserDirect, seedSession } from "./helpers/db.js";

// deploy_ac_017 — SPEC-023 §Data contracts §3 and debt #20: `GET /health` answers
// `200 { "status": "ok" }`, and, once the api has started, still does while the stores
// are unreachable; with API_SESSION_IDLE_MINUTES=5 and API_SESSION_MAX_HOURS=1, a new
// session's idle lifetime and cap are those.

const PACKAGE_DIR = join(dirname(fileURLToPath(import.meta.url)), "..");
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

/**
 * A TCP forwarder on 127.0.0.1 to the host and port of `target`, a store's URL: `url` is
 * `target` through it, and `cut()` closes it and every connection through it, so the
 * store is unreachable from then on.
 */
async function forwarder(target: string): Promise<{ url: string; cut(): Promise<void> }> {
  const to = new URL(target);
  const sockets = new Set<Socket>();
  const server = createServer((client) => {
    const upstream = connect(Number(to.port), to.hostname);
    for (const s of [client, upstream]) {
      sockets.add(s);
      s.on("error", () => {});
      s.on("close", () => {
        client.destroy();
        upstream.destroy();
      });
    }
    client.pipe(upstream).pipe(client);
  });
  await new Promise<void>((resolve) => server.listen({ host: "127.0.0.1", port: 0 }, resolve));
  const via = new URL(target);
  via.hostname = "127.0.0.1";
  via.port = String((server.address() as AddressInfo).port);
  return {
    url: via.toString().replace(/\/$/, ""),
    cut: () =>
      new Promise((resolve) => {
        server.close(() => resolve());
        for (const s of sockets) s.destroy();
      }),
  };
}

/** A port on 127.0.0.1 that was free a moment ago. */
function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const server = createServer();
    server.once("error", reject);
    server.listen({ host: "127.0.0.1", port: 0 }, () => {
      const { port } = server.address() as AddressInfo;
      server.close(() => resolve(port));
    });
  });
}

/** The status of a GET, or null when nothing answers. */
async function statusOf(url: string, cookie?: string): Promise<number | null> {
  try {
    return (await fetch(url, { headers: cookie ? { cookie } : {} })).status;
  } catch {
    return null;
  }
}

describe("GET /health", () => {
  test("answers 200 { status: ok } on the started api", async () => {
    const api: ApiServer = await startApi(config);
    closers.push(() => api.close());
    const res = await fetch(`${api.url}/health`);
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({ status: "ok" });
  });

  test("once started, the api keeps running and answers 200 when every store is unreachable", async () => {
    // The api, run as a program, reaches each store through a forwarder the test cuts.
    const pg = await forwarder(config.API_PG_URL);
    const kv = await forwarder(config.API_REDIS_URL);
    const ch = await forwarder(config.API_CH_URL);
    const port = await freePort();
    const env = Object.fromEntries(
      Object.entries(process.env).filter(([k]) => !k.startsWith("API_")),
    );
    const child: ChildProcess = spawn(process.execPath, ["--import", "tsx", "src/index.ts"], {
      cwd: PACKAGE_DIR,
      env: {
        ...env,
        API_PG_URL: pg.url,
        API_REDIS_URL: kv.url,
        API_CH_URL: ch.url,
        API_CH_USER: config.API_CH_USER,
        API_CH_PASSWORD: config.API_CH_PASSWORD,
        API_CH_DB: config.API_CH_DB,
        API_DB_ENC_PASSPHRASE: config.API_DB_ENC_PASSPHRASE,
        API_FORENSIC_PASSPHRASE: config.API_FORENSIC_PASSPHRASE,
        API_PORT: String(port),
        API_LOG_LEVEL: "warn",
      },
    });
    let stderr = "";
    child.stderr?.setEncoding("utf8").on("data", (d: string) => {
      stderr += d;
    });
    let stdout = "";
    child.stdout?.setEncoding("utf8").on("data", (d: string) => {
      stdout += d;
    });
    const url = `http://127.0.0.1:${port}`;
    try {
      const begun = Date.now();
      while ((await statusOf(`${url}/health`)) !== 200) {
        if (child.exitCode !== null || Date.now() - begun > 60_000) {
          throw new Error(`the api did not start: ${stderr}`);
        }
        await new Promise((r) => setTimeout(r, 200));
      }
      // The pool holds, idle, the client the forensic key's load used at start (pg keeps
      // an idle client 10 s), whose connection the cut ends.
      await Promise.all([pg.cut(), kv.cut(), ch.cut()]);
      await new Promise((r) => setTimeout(r, 1500));
      expect(child.exitCode, stderr).toBeNull();
      // The lost connections are logged.
      expect(stdout).toContain("pg_connection_lost");
      expect(stdout).toContain("redis_unreachable");
      const res = await fetch(`${url}/health`);
      expect(res.status).toBe(200);
      expect(await res.json()).toEqual({ status: "ok" });
      // The stores are unreachable indeed: a route that reads one does not answer 200.
      expect(await statusOf(`${url}/v1/incidents`, "cgsess=x")).toBe(503);
    } finally {
      child.kill();
    }
  });
});

test("a connection lost while its client is checked out does not end the process", async () => {
  // The api's pool, as createPgPool builds it, with a client checked out when the cut comes.
  const pg = await forwarder(config.API_PG_URL);
  const child = spawn(
    process.execPath,
    ["--import", "tsx", "test/fixtures/checkout-loss.ts", pg.url],
    {
      cwd: PACKAGE_DIR,
    },
  );
  let stdout = "";
  let stderr = "";
  child.stdout?.setEncoding("utf8").on("data", (d: string) => {
    stdout += d;
  });
  child.stderr?.setEncoding("utf8").on("data", (d: string) => {
    stderr += d;
  });
  const exited = new Promise<number | null>((resolve) => child.on("exit", resolve));
  try {
    const begun = Date.now();
    while (!stdout.includes("checked-out")) {
      if (child.exitCode !== null || Date.now() - begun > 60_000) {
        throw new Error(`the program did not check a client out: ${stderr}`);
      }
      await new Promise((r) => setTimeout(r, 100));
    }
    await pg.cut();
    expect(await exited, stderr).toBe(0);
    expect(stdout).toContain("query failed");
    // It failed for the cut, not for a timeout, and the loss is logged.
    expect(stdout).toMatch(/"query_error":"[^"]*(terminated|ECONNRESET|closed)/i);
    expect(stdout).not.toContain("statement timeout");
    expect(stdout).toContain("pg_connection_lost");
  } finally {
    child.kill();
  }
});

describe("the session's lifetimes are settings", () => {
  let app: FastifyInstance;
  const key = (token: string) => `cgsess:${token}`;
  /** The suite's stores, as environment variables. */
  const storesEnv = () => ({
    API_PG_URL: config.API_PG_URL,
    API_REDIS_URL: config.API_REDIS_URL,
    API_CH_URL: config.API_CH_URL,
    API_CH_USER: config.API_CH_USER,
    API_CH_PASSWORD: config.API_CH_PASSWORD,
    API_CH_DB: config.API_CH_DB,
    API_DB_ENC_PASSPHRASE: config.API_DB_ENC_PASSPHRASE,
    API_FORENSIC_PASSPHRASE: config.API_FORENSIC_PASSPHRASE,
  });

  beforeAll(async () => {
    // From the environment, as the service reads it.
    const short = loadConfig({
      ...storesEnv(),
      API_SESSION_IDLE_MINUTES: "5",
      API_SESSION_MAX_HOURS: "1",
    });
    expect([short.API_SESSION_IDLE_MINUTES, short.API_SESSION_MAX_HOURS]).toEqual([5, 1]);
    const services = await buildServices(short);
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

  test("leading zeros are digits (SPEC-023 Amendment 2026-10-23 item 1)", () => {
    const parsed = loadConfig({
      ...storesEnv(),
      API_PORT: "08081",
      API_SESSION_IDLE_MINUTES: "060",
    });
    expect([parsed.API_PORT, parsed.API_SESSION_IDLE_MINUTES]).toEqual([8081, 60]);
  });

  test("an idle lifetime equal to the cap is accepted", () => {
    const parsed = loadConfig({
      ...storesEnv(),
      API_SESSION_IDLE_MINUTES: "60",
      API_SESSION_MAX_HOURS: "1",
    });
    expect([parsed.API_SESSION_IDLE_MINUTES, parsed.API_SESSION_MAX_HOURS]).toEqual([60, 1]);
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
