import { type AddressInfo, type Server, type Socket, createServer } from "node:net";
import { createClient } from "@clickhouse/client";
import type { FastifyInstance } from "fastify";
import type pg from "pg";
import { afterAll, beforeAll, describe, expect, inject, test } from "vitest";
import { buildApp } from "../src/app.js";
import type { Config } from "../src/config.js";
import { type Services, buildServices, createPgPool } from "../src/services.js";
import { seedSession } from "./helpers/db.js";
import { ensureAgent, insertAlertRow, insertIncidentRow } from "./helpers/read-schema.js";

// read_ac_007 — SPEC-009 Amendment 2026-10-14 §3–§5: a store that cannot answer is
// a 503 { error: "unavailable" }, never a 404 that reads as "not found"; a 503 and a
// 500 are logged, never with a Postgres error's detail. The failing store is a local
// listener that drops, refuses or never answers a connection, so the failure comes
// through the real client and the real network path.

const ORG = "read-ac-007";
const OTHER_ORG = "read-ac-007-other";
const TOKEN = "tok-read-ac-007";

let config: Config;
let services: Services;
let dropper: Server;
let incidentId: string;
let otherOrgIncidentId: string;

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

/** A listener on 127.0.0.1 that accepts and drops every connection. */
function startDropper(): Promise<Server> {
  return new Promise((resolve, reject) => {
    const server = createServer((socket) => socket.destroy());
    server.once("error", reject);
    server.listen({ host: "127.0.0.1", port: 0 }, () => resolve(server));
  });
}

async function appWith(over: Partial<Services>): Promise<FastifyInstance> {
  const app = buildApp({ ...services, ...over });
  await app.ready();
  return app;
}

async function get(app: FastifyInstance, url: string) {
  return app.inject({ method: "GET", url, cookies: { cgsess: TOKEN } });
}

beforeAll(async () => {
  config = inject("apiConfig");
  services = await buildServices(config);
  dropper = await startDropper();

  const agentId = globalThis.crypto.randomUUID();
  const alertId = globalThis.crypto.randomUUID();
  incidentId = globalThis.crypto.randomUUID();
  await ensureAgent(config, agentId, ORG);
  // A source event that exists nowhere: the drill must reach ClickHouse to learn so.
  await insertAlertRow(config, {
    alertId,
    agentId,
    orgId: ORG,
    sourceEvents: [globalThis.crypto.randomUUID()],
  });
  await insertIncidentRow(config, { incidentId, agentId, orgId: ORG, alertIds: [alertId] });

  const otherAgent = globalThis.crypto.randomUUID();
  const otherAlert = globalThis.crypto.randomUUID();
  otherOrgIncidentId = globalThis.crypto.randomUUID();
  await ensureAgent(config, otherAgent, OTHER_ORG);
  await insertAlertRow(config, {
    alertId: otherAlert,
    agentId: otherAgent,
    orgId: OTHER_ORG,
    sourceEvents: [globalThis.crypto.randomUUID()],
  });
  await insertIncidentRow(config, {
    incidentId: otherOrgIncidentId,
    agentId: otherAgent,
    orgId: OTHER_ORG,
    alertIds: [otherAlert],
  });

  await seedSession(config, {
    token: TOKEN,
    userId: globalThis.crypto.randomUUID(),
    role: "analyst",
    orgId: ORG,
  });
});

afterAll(async () => {
  await services?.close();
  await new Promise<void>((resolve) => (dropper ? dropper.close(() => resolve()) : resolve()));
});

describe("Postgres cannot answer", () => {
  let app: FastifyInstance;
  let deadPool: pg.Pool;

  beforeAll(async () => {
    const port = (dropper.address() as AddressInfo).port;
    deadPool = createPgPool(`postgres://u:p@127.0.0.1:${port}/db`);
    app = await appWith({ pg: deadPool });
  });
  afterAll(async () => {
    await app?.close();
    await deadPool?.end();
  });

  test.each([
    ["the incidents list", () => "/v1/incidents"],
    ["the alerts list", () => "/v1/alerts"],
    ["the detail", () => `/v1/incidents/${incidentId}`],
    ["the drill", () => `/v1/incidents/${incidentId}/events`],
    ["the export", () => `/v1/incidents/${incidentId}/forensic-export`],
    ["the report", () => `/v1/incidents/${incidentId}/report`],
  ])("%s answers 503 unavailable", async (_name, url) => {
    const res = await get(app, url());
    expect(res.statusCode).toBe(503);
    expect(res.json()).toEqual({ error: "unavailable" });
  });

  test.each(["", "/events", "/forensic-export", "/report"])(
    "a malformed :id on /v1/incidents/:id%s is still a 404: decided before any read",
    async (suffix) => {
      const res = await get(app, `/v1/incidents/not-a-uuid${suffix}`);
      expect(res.statusCode).toBe(404);
      expect(res.json()).toEqual({ error: "not_found" });
    },
  );

  test("a malformed list parameter is still a 400: decided before any read", async () => {
    for (const url of ["/v1/incidents?agent_id=nope", "/v1/alerts?severity_id=9"]) {
      const res = await get(app, url);
      expect(res.statusCode, url).toBe(400);
      expect(res.json()).toEqual({ error: "bad_request" });
    }
  });

  test("another org's well-formed :id is a 503 too: the outage tells nothing about it", async () => {
    const res = await get(app, `/v1/incidents/${otherOrgIncidentId}`);
    expect(res.statusCode).toBe(503);
    expect(res.json()).toEqual({ error: "unavailable" });
  });
});

describe("Postgres refuses the connection", () => {
  let app: FastifyInstance;
  let refusingPool: pg.Pool;

  beforeAll(async () => {
    refusingPool = createPgPool(`postgres://u:p@127.0.0.1:${await closedPort()}/db`);
    app = await appWith({ pg: refusingPool });
  });
  afterAll(async () => {
    await app?.close();
    await refusingPool?.end();
  });

  test.each([
    ["the incidents list", () => "/v1/incidents"],
    ["the detail", () => `/v1/incidents/${incidentId}`],
  ])("%s answers 503 unavailable", async (_name, url) => {
    const res = await get(app, url());
    expect(res.statusCode).toBe(503);
    expect(res.json()).toEqual({ error: "unavailable" });
  });
});

test("the api's own pool carries the 5 s connection timeout", () => {
  expect(services.pg.options.connectionTimeoutMillis).toBe(5000);
});

describe("Postgres accepts the connection and never answers", () => {
  let app: FastifyInstance;
  let silent: Server;
  const held = new Set<Socket>();
  let silentPool: pg.Pool;

  beforeAll(async () => {
    silent = await new Promise<Server>((resolve, reject) => {
      // Holds each connection open and writes nothing: only the pool's connection
      // timeout ends the wait.
      const server = createServer((socket) => {
        held.add(socket);
      });
      server.once("error", reject);
      server.listen({ host: "127.0.0.1", port: 0 }, () => resolve(server));
    });
    const port = (silent.address() as AddressInfo).port;
    // The api's pool as buildServices makes it, pointed at the silent listener.
    silentPool = createPgPool(`postgres://u:p@127.0.0.1:${port}/db`);
    app = await appWith({ pg: silentPool });
  });
  afterAll(async () => {
    await app?.close();
    await silentPool?.end();
    await new Promise<void>((resolve) => {
      for (const socket of held) socket.destroy();
      silent.close(() => resolve());
    });
  });

  test("the detail answers 503 once the connection times out", async () => {
    const started = Date.now();
    const res = await get(app, `/v1/incidents/${incidentId}`);
    expect(res.statusCode).toBe(503);
    expect(res.json()).toEqual({ error: "unavailable" });
    // The pool's 5 s connection timeout, not some other bound, ends the wait.
    const elapsed = Date.now() - started;
    expect(elapsed).toBeGreaterThanOrEqual(4900);
    expect(elapsed).toBeLessThan(8000);
  });
});

describe("ClickHouse cannot answer", () => {
  let app: FastifyInstance;
  let deadCh: ReturnType<typeof createClient>;

  beforeAll(async () => {
    const port = (dropper.address() as AddressInfo).port;
    deadCh = createClient({ url: `http://127.0.0.1:${port}` });
    app = await appWith({ ch: deadCh });
  });
  afterAll(async () => {
    await app?.close();
    await deadCh?.close();
  });

  test.each(["/events", "/forensic-export", "/report"])(
    "the incident's %s answers 503 unavailable",
    async (suffix) => {
      const res = await get(app, `/v1/incidents/${incidentId}${suffix}`);
      expect(res.statusCode).toBe(503);
      expect(res.json()).toEqual({ error: "unavailable" });
    },
  );

  test("the routes that read only Postgres still answer 200", async () => {
    for (const url of ["/v1/incidents", "/v1/alerts", `/v1/incidents/${incidentId}`]) {
      const res = await get(app, url);
      expect(res.statusCode, url).toBe(200);
    }
  });

  test("another org's incident is a 404 before ClickHouse is read: no existence oracle", async () => {
    for (const suffix of ["/events", "/forensic-export", "/report"]) {
      const res = await get(app, `/v1/incidents/${otherOrgIncidentId}${suffix}`);
      expect(res.statusCode, suffix).toBe(404);
      expect(res.json()).toEqual({ error: "not_found" });
    }
  });
});

describe("ClickHouse refuses the connection", () => {
  let app: FastifyInstance;
  let refusingCh: ReturnType<typeof createClient>;

  beforeAll(async () => {
    refusingCh = createClient({ url: `http://127.0.0.1:${await closedPort()}` });
    app = await appWith({ ch: refusingCh });
  });
  afterAll(async () => {
    await app?.close();
    await refusingCh?.close();
  });

  test("the drill answers 503 unavailable", async () => {
    const res = await get(app, `/v1/incidents/${incidentId}/events`);
    expect(res.statusCode).toBe(503);
    expect(res.json()).toEqual({ error: "unavailable" });
  });
});

describe("what is logged", () => {
  // A Postgres error's detail can quote a whole row: on SPEC-008's users table, a
  // password hash and an encrypted TOTP secret. It must never reach a log line.
  const SECRET = "argon2id-hash-that-must-not-be-logged";
  const pgError = () =>
    Object.assign(new Error("new row violates check constraint"), {
      name: "DatabaseError",
      code: "23514",
      detail: `Failing row contains (${SECRET}).`,
    });
  const lines: string[] = [];
  const logger = { level: "info", stream: { write: (line: string) => lines.push(line) } };
  // Postgres fails every query with an error that carries a detail.
  let storeApp: FastifyInstance;
  // The forensic key fails to sign: not a store's failure, so a 500.
  let keyApp: FastifyInstance;
  // ClickHouse fails every query the same way.
  let chApp: FastifyInstance;

  beforeAll(async () => {
    const failingPool = {
      query: async () => {
        throw pgError();
      },
    } as unknown as pg.Pool;
    const failingKey = {
      publicKeyRaw: services.forensicKey.publicKeyRaw,
      sign: async () => {
        throw pgError();
      },
    };
    const failingCh = {
      query: async () => {
        throw pgError();
      },
    } as unknown as Services["ch"];
    storeApp = buildApp({ ...services, pg: failingPool }, logger);
    keyApp = buildApp({ ...services, forensicKey: failingKey }, logger);
    chApp = buildApp({ ...services, ch: failingCh }, logger);
    await Promise.all([storeApp.ready(), keyApp.ready(), chApp.ready()]);
  });
  afterAll(async () => {
    await Promise.all([storeApp?.close(), keyApp?.close(), chApp?.close()]);
  });

  function errorLines(msg: string) {
    return lines.map((l) => JSON.parse(l)).filter((l) => l.level === 50 && l.msg === msg);
  }

  test("a 503 is logged at error level with the cause's code and message, without its detail", async () => {
    const res = await get(storeApp, `/v1/incidents/${incidentId}`);
    expect(res.statusCode).toBe(503);
    const [line] = errorLines("store unavailable");
    expect(line?.error).toMatchObject({
      type: "DatabaseError",
      code: "23514",
      message: "new row violates check constraint",
    });
    expect(line?.error.stack).toContain("new row violates check constraint");
    expect(JSON.stringify(lines)).not.toContain(SECRET);
  });

  test("a ClickHouse failure is logged the same way", async () => {
    const before = errorLines("store unavailable").length;
    const res = await get(chApp, `/v1/incidents/${incidentId}/events`);
    expect(res.statusCode).toBe(503);
    const logged = errorLines("store unavailable");
    expect(logged.length).toBe(before + 1);
    expect(logged.at(-1)?.error).toMatchObject({ type: "DatabaseError", code: "23514" });
    expect(JSON.stringify(lines)).not.toContain(SECRET);
  });

  test("a failure that is not a store's is a 500, logged without its detail", async () => {
    const res = await get(keyApp, `/v1/incidents/${incidentId}/forensic-export`);
    expect(res.statusCode).toBe(500);
    expect(res.json()).toEqual({ error: "internal" });
    const [line] = errorLines("request failed");
    expect(line?.error).toMatchObject({ type: "DatabaseError", code: "23514" });
    expect(JSON.stringify(lines)).not.toContain(SECRET);
  });

  test("an error Fastify raises with its own status keeps it", async () => {
    const res = await storeApp.inject({
      method: "POST",
      url: "/v1/auth/login",
      headers: { "content-type": "application/json" },
      payload: "{",
    });
    expect(res.statusCode).toBe(400);
  });
});
