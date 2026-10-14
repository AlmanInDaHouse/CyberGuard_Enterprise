import { type AddressInfo, type Server, type Socket, createServer } from "node:net";
import type { FastifyInstance } from "fastify";
import type pg from "pg";
import { afterAll, beforeAll, describe, expect, inject, test } from "vitest";
import { buildApp } from "../src/app.js";
import { createAuthService } from "../src/auth/service.js";
import type { Config } from "../src/config.js";
import { type Services, buildServices, createPgPool, createRedisClient } from "../src/services.js";
import { seedSession } from "./helpers/db.js";

// apiauth_ac_014 — SPEC-008 Amendment 2026-10-15 items 14 and 15: Redis and Postgres
// are bounded. A Redis that refuses, stays silent or answers with an error makes a
// session-guarded request and a login answer 503 within 8 s, logged at error; a
// statement that runs 5 s is cancelled by Postgres, and a read whose statement runs
// past the bound answers 503. The failing store is a local listener, so the failure
// comes through the real client.

let config: Config;
let services: Services;
const servers: Server[] = [];
const held = new Set<Socket>();

function listen(onConnection: (socket: Socket) => void): Promise<Server> {
  return new Promise((resolve, reject) => {
    const server = createServer((socket) => {
      held.add(socket);
      onConnection(socket);
    });
    server.once("error", reject);
    server.listen({ host: "127.0.0.1", port: 0 }, () => {
      servers.push(server);
      resolve(server);
    });
  });
}

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

/** A Redis that answers INFO, so the client is ready, and every other command with an error. */
function erroringRedis(socket: Socket): void {
  socket.on("data", (chunk) => {
    for (const command of chunk.toString("utf8").split("*").slice(1)) {
      const name = command.split("\r\n")[2]?.toUpperCase();
      if (name === "INFO") {
        const body = "# Server\r\nloading:0\r\n";
        socket.write(`$${Buffer.byteLength(body)}\r\n${body}\r\n`);
      } else {
        socket.write("-ERR fake failure\r\n");
      }
    }
  });
}

beforeAll(async () => {
  config = inject("apiConfig");
  services = await buildServices(config);
});
afterAll(async () => {
  await services?.close();
  for (const socket of held) socket.destroy();
  await Promise.all(servers.map((s) => new Promise<void>((resolve) => s.close(() => resolve()))));
});

describe.each([
  ["refusing connections", async () => `redis://127.0.0.1:${await closedPort()}`],
  [
    "accepting a connection and never answering",
    async () => `redis://127.0.0.1:${((await listen(() => {})).address() as AddressInfo).port}`,
  ],
  [
    "answering with an error",
    async () =>
      `redis://127.0.0.1:${((await listen(erroringRedis)).address() as AddressInfo).port}`,
  ],
])("Redis %s", (_label, url) => {
  const lines: string[] = [];
  let app: FastifyInstance;
  let redis: ReturnType<typeof createRedisClient>;

  beforeAll(async () => {
    redis = createRedisClient(await url());
    redis.on("error", () => {});
    const auth = createAuthService(services.pg, redis, config);
    app = buildApp(
      { ...services, redis, auth },
      { level: "info", stream: { write: (line: string) => lines.push(line) } },
    );
    await app.ready();
  });
  afterAll(async () => {
    await app?.close();
    redis?.disconnect();
  });

  test("a session-guarded request answers 503 within 8 s", async () => {
    const started = Date.now();
    const res = await app.inject({ method: "GET", url: "/v1/users", cookies: { cgsess: "t" } });
    expect(res.statusCode).toBe(503);
    expect(res.json()).toEqual({ error: "unavailable" });
    expect(Date.now() - started).toBeLessThan(8000);
  });

  test("a login answers 503 within 8 s", async () => {
    const started = Date.now();
    const res = await app.inject({
      method: "POST",
      url: "/v1/auth/login",
      payload: { email: "nobody@ac014.test", password: "whatever", totp_code: "000000" },
    });
    expect(res.statusCode).toBe(503);
    expect(res.json()).toEqual({ error: "unavailable" });
    expect(Date.now() - started).toBeLessThan(8000);
  });

  test("the failure is logged at error", () => {
    const logged = lines.map((l) => JSON.parse(l) as { level: number; msg: string });
    expect(logged.filter((l) => l.msg === "store unavailable" && l.level === 50)).toHaveLength(2);
  });
});

test("an error inside the rate limiter's transaction answers 503", async () => {
  // A counter GET reads (1.5 is under the limit) and INCR refuses ("not an integer"),
  // so the failure comes inside EXEC, which still resolves.
  const redis = createRedisClient(config.API_REDIS_URL);
  await redis.set("rl:acct:default:wrongtype@ac014.test", "1.5", "EX", 60);
  const app = buildApp(services);
  await app.ready();
  try {
    const res = await app.inject({
      method: "POST",
      url: "/v1/auth/login",
      remoteAddress: "198.51.100.142",
      payload: { email: "wrongtype@ac014.test", password: "whatever", totp_code: "000000" },
    });
    expect(res.statusCode).toBe(503);
  } finally {
    await app.close();
    await redis.del("rl:acct:default:wrongtype@ac014.test");
    redis.disconnect();
  }
});

describe("Postgres", () => {
  let pool: pg.Pool;
  beforeAll(() => {
    pool = createPgPool(config.API_PG_URL);
  });
  afterAll(async () => {
    await pool?.end();
  });

  test("a statement that runs 5 s on the api's pool is cancelled by Postgres", async () => {
    const started = Date.now();
    const err = await pool.query("SELECT pg_sleep(6)").then(
      () => null,
      (e: unknown) => e,
    );
    // 57014 is Postgres cancelling the statement, so the server's bound ended it, and
    // before the client's 6 s. Postgres times it on the Docker VM's clock, which steps
    // by up to about 2.4 s (debt #41, handoff-38), so the host sees it from about 2.5 s.
    expect((err as { code?: string } | null)?.code).toBe("57014");
    const elapsed = Date.now() - started;
    expect(elapsed).toBeGreaterThanOrEqual(2500);
    expect(elapsed).toBeLessThan(6000);
  });

  test("a read on which the pool's bound cancels a statement answers 503", async () => {
    // Before the route's own query, the same bounded pool runs one that outlasts the
    // bound; Postgres cancels it (57014), and that failure reaches the route as the
    // store's. The route's query itself is never sent.
    const slow = {
      query: async (...args: Parameters<pg.Pool["query"]>) => {
        await pool.query("SELECT pg_sleep(6)");
        return pool.query(...args);
      },
    } as unknown as pg.Pool;
    await seedSession(config, {
      token: "tok-apiauth-014",
      userId: globalThis.crypto.randomUUID(),
      role: "analyst",
    });
    const app = buildApp({ ...services, pg: slow });
    await app.ready();
    try {
      const started = Date.now();
      const res = await app.inject({
        method: "GET",
        url: "/v1/incidents",
        cookies: { cgsess: "tok-apiauth-014" },
      });
      expect(res.statusCode).toBe(503);
      expect(res.json()).toEqual({ error: "unavailable" });
      expect(Date.now() - started).toBeLessThan(8000);
    } finally {
      await app.close();
    }
  });

  test("a Postgres that refuses connections makes SPEC-008's own routes answer 503", async () => {
    const refusing = createPgPool(`postgres://u:p@127.0.0.1:${await closedPort()}/db`);
    const auth = createAuthService(refusing, services.redis, config);
    const app = buildApp({ ...services, pg: refusing, auth });
    await app.ready();
    const adminId = globalThis.crypto.randomUUID();
    await seedSession(config, { token: "tok-apiauth-014-admin", userId: adminId, role: "admin" });
    try {
      const list = await app.inject({
        method: "GET",
        url: "/v1/users",
        cookies: { cgsess: "tok-apiauth-014-admin" },
      });
      expect(list.statusCode).toBe(503);
      const login = await app.inject({
        method: "POST",
        url: "/v1/auth/login",
        remoteAddress: "198.51.100.141",
        payload: { email: "nobody@ac014.test", password: "whatever", totp_code: "000000" },
      });
      expect(login.statusCode).toBe(503);
    } finally {
      await app.close();
      await refusing.end();
    }
  });
});
