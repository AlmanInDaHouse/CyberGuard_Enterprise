import { type AddressInfo, type Server, createServer } from "node:net";
import type { FastifyInstance } from "fastify";
import pg from "pg";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import { buildApp } from "../src/app.js";
import type { Config } from "../src/config.js";
import { type Services, buildServices, createPgPool } from "../src/services.js";
import { seedSession } from "./helpers/db.js";
import { ensureAgent, insertAlertRow, insertIncidentRow } from "./helpers/read-schema.js";

// soar_ac_007 — SPEC-022 §Data contracts §4: GET /v1/incidents/:id/playbook-executions
// reads an incident's execution records behind the session, org-scoped, with the
// read API's error semantics (SPEC-009 Amendment 2026-10-14).

const ORG = "soar-ac-007";
const TOKEN = "tok-soar-ac-007";

let config: Config;
let services: Services;
let app: FastifyInstance;
let incidentId: string;
let quietIncidentId: string;
let otherOrgIncidentId: string;
const executionIds: string[] = [];

async function incident(org: string): Promise<{ agentId: string; incidentId: string }> {
  const agentId = globalThis.crypto.randomUUID();
  const alertId = globalThis.crypto.randomUUID();
  const id = globalThis.crypto.randomUUID();
  await ensureAgent(config, agentId, org);
  await insertAlertRow(config, { alertId, agentId, orgId: org });
  await insertIncidentRow(config, { incidentId: id, agentId, orgId: org, alertIds: [alertId] });
  return { agentId, incidentId: id };
}

/** Claim and complete one execution row, as ingest's run does. */
async function execution(
  pool: pg.Pool,
  org: string,
  agentId: string,
  incident: string,
  playbookId: string,
): Promise<string> {
  const id = globalThis.crypto.randomUUID();
  await pool.query(
    `INSERT INTO playbook_executions
       (execution_id, org_id, incident_id, agent_id, playbook_id, playbook_version)
     VALUES ($1, $2, $3, $4, $5, 1)`,
    [id, org, incident, agentId, playbookId],
  );
  const steps = [
    {
      step_id: "isolate",
      action: "agent.isolate",
      mode: "dry_run",
      outcome: "would_do",
      detail: `would isolate agent ${agentId}`,
      started_at: "2026-10-14T00:00:00.000Z",
      finished_at: "2026-10-14T00:00:00.001Z",
      link: "a".repeat(64),
    },
  ];
  await pool.query(
    `UPDATE playbook_executions
        SET status = 'completed', finished_at = now(), steps = $2::jsonb, chain_head = $3
      WHERE execution_id = $1`,
    [id, JSON.stringify(steps), "a".repeat(64)],
  );
  return id;
}

beforeAll(async () => {
  config = inject("apiConfig");
  services = await buildServices(config);
  app = buildApp(services);
  await app.ready();
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    const own = await incident(ORG);
    incidentId = own.incidentId;
    executionIds.push(await execution(pool, ORG, own.agentId, incidentId, "pb_a"));
    executionIds.push(await execution(pool, ORG, own.agentId, incidentId, "pb_b"));
    quietIncidentId = (await incident(ORG)).incidentId;
    const other = await incident(`${ORG}-other`);
    otherOrgIncidentId = other.incidentId;
    await execution(pool, `${ORG}-other`, other.agentId, otherOrgIncidentId, "pb_a");
  } finally {
    await pool.end();
  }
  await seedSession(config, {
    token: TOKEN,
    userId: globalThis.crypto.randomUUID(),
    role: "viewer",
    orgId: ORG,
  });
});
afterAll(async () => {
  await app?.close();
  await services?.close();
});

const get = (a: FastifyInstance, url: string, token: string | null = TOKEN) =>
  a.inject({ method: "GET", url, ...(token ? { cookies: { cgsess: token } } : {}) });

test("a session of the incident's org reads its executions, oldest first", async () => {
  const res = await get(app, `/v1/incidents/${incidentId}/playbook-executions`);
  expect(res.statusCode).toBe(200);
  const body = res.json();
  expect(body.incident_id).toBe(incidentId);
  expect(body.executions.map((e: { execution_id: string }) => e.execution_id)).toEqual(
    executionIds,
  );
  expect(body.executions[0]).toMatchObject({
    playbook_id: "pb_a",
    playbook_version: 1,
    status: "completed",
    chain_head: "a".repeat(64),
    steps: [{ step_id: "isolate", outcome: "would_do", link: "a".repeat(64) }],
  });
  expect(typeof body.executions[0].started_at).toBe("string");
  expect(typeof body.executions[0].finished_at).toBe("string");
});

test("an incident with no execution answers an empty list", async () => {
  const res = await get(app, `/v1/incidents/${quietIncidentId}/playbook-executions`);
  expect(res.statusCode).toBe(200);
  expect(res.json()).toEqual({ incident_id: quietIncidentId, executions: [] });
});

test("another org's, an absent and a malformed :id answer 404", async () => {
  for (const id of [otherOrgIncidentId, globalThis.crypto.randomUUID(), "not-a-uuid"]) {
    const res = await get(app, `/v1/incidents/${id}/playbook-executions`);
    expect(res.statusCode, id).toBe(404);
    expect(res.json()).toEqual({ error: "not_found" });
  }
});

test("no session answers 401", async () => {
  const res = await get(app, `/v1/incidents/${incidentId}/playbook-executions`, null);
  expect(res.statusCode).toBe(401);
});

test("a Postgres that cannot answer gives 503", async () => {
  const dropper: Server = await new Promise((resolve, reject) => {
    const server = createServer((socket) => socket.destroy());
    server.once("error", reject);
    server.listen({ host: "127.0.0.1", port: 0 }, () => resolve(server));
  });
  const dead = createPgPool(
    `postgres://u:p@127.0.0.1:${(dropper.address() as AddressInfo).port}/db`,
  );
  const deadApp = buildApp({ ...services, pg: dead });
  await deadApp.ready();
  try {
    const res = await get(deadApp, `/v1/incidents/${incidentId}/playbook-executions`);
    expect(res.statusCode).toBe(503);
    expect(res.json()).toEqual({ error: "unavailable" });
  } finally {
    await deadApp.close();
    await dead.end();
    await new Promise<void>((resolve) => dropper.close(() => resolve()));
  }
});
