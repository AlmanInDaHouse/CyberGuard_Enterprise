import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp, seedSession } from "./helpers/db.js";
import { ensureAgent, insertAlertRow, insertIncidentRow } from "./helpers/read-schema.js";

// read_ac_006 — SPEC-009 NFR-009-003 and Amendment 2026-10-14 §1–§2: a malformed
// `:id` is a 404 on every `:id` route, and a malformed list parameter is a 400 on
// the list that takes it. Each body carries only `error`: no SQL, no stack.

const TOKEN = "tok-read-ac-006";
const ORG = "read-ac-006";

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;
// An incident that exists, so a refused spelling cannot pass as "not found".
let agentId: string;
let incidentId: string;

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
  await seedSession(config, {
    token: TOKEN,
    userId: globalThis.crypto.randomUUID(),
    role: "viewer",
    orgId: ORG,
  });
  agentId = globalThis.crypto.randomUUID();
  incidentId = globalThis.crypto.randomUUID();
  const alertId = globalThis.crypto.randomUUID();
  await ensureAgent(config, agentId, ORG);
  await insertAlertRow(config, {
    alertId,
    agentId,
    orgId: ORG,
    sourceEvents: [globalThis.crypto.randomUUID()],
  });
  await insertIncidentRow(config, { incidentId, agentId, orgId: ORG, alertIds: [alertId] });
});
afterAll(async () => {
  await apph?.close();
});

async function get(url: string) {
  return apph.app.inject({ method: "GET", url, cookies: { cgsess: TOKEN } });
}

/** A cursor as the API encodes one (base64url of `{ t, id }`), with chosen fields. */
function cursor(t: string, id: string): string {
  return Buffer.from(JSON.stringify({ t, id })).toString("base64url");
}

const ZERO = "00000000-0000-4000-8000-000000000000";

const MALFORMED_IDS = [
  "not-a-uuid",
  "1",
  // Spellings Postgres accepts but the read API does not: braces, no hyphens.
  "{00000000-0000-4000-8000-000000000000}",
  "00000000000040008000000000000000",
];

test.each(["", "/events", "/forensic-export", "/report"])(
  "a malformed :id on /v1/incidents/:id%s is a bare 404",
  async (suffix) => {
    for (const id of MALFORMED_IDS) {
      const res = await get(`/v1/incidents/${encodeURIComponent(id)}${suffix}`);
      expect(res.statusCode, id).toBe(404);
      expect(res.json(), id).toEqual({ error: "not_found" });
    }
  },
);

test.each([
  ["/v1/incidents?agent_id=not-a-uuid", "agent_id"],
  ["/v1/alerts?agent_id=not-a-uuid", "agent_id"],
  ["/v1/alerts?severity_id=7", "severity_id out of range"],
  ["/v1/alerts?severity_id=abc", "severity_id not an integer"],
  ["/v1/alerts?severity_id=-1", "severity_id negative"],
  ["/v1/alerts?severity_id=2.5", "severity_id not an integer"],
  ["/v1/incidents?status=%00", "status with a NUL"],
  ["/v1/alerts?status=new%00", "status with a NUL"],
  ["/v1/incidents?cursor=%25%25%25", "cursor that does not decode"],
  [
    `/v1/incidents?cursor=${cursor("yesterday", "00000000-0000-4000-8000-000000000000")}`,
    "cursor time",
  ],
  [`/v1/alerts?cursor=${cursor("2026-10-14T00:00:00.000Z", "nope")}`, "cursor id"],
  // Times toISOString() writes and Postgres refuses: years 0, -1 and 10000.
  [`/v1/incidents?cursor=${cursor("0000-01-01T00:00:00.000Z", ZERO)}`, "cursor year 0"],
  [`/v1/incidents?cursor=${cursor("-000001-01-01T00:00:00.000Z", ZERO)}`, "cursor year -1"],
  [`/v1/alerts?cursor=${cursor("+010000-01-01T00:00:00.000Z", ZERO)}`, "cursor year 10000"],
])("%s is a bare 400 (%s)", async (url) => {
  const res = await get(url);
  expect(res.statusCode).toBe(400);
  expect(res.json()).toEqual({ error: "bad_request" });
});

test("well-formed list parameters still answer 200", async () => {
  const agent = "00000000-0000-4000-8000-000000000000";
  const ok = cursor("2026-10-14T00:00:00.000Z", agent);
  for (const url of [
    `/v1/incidents?agent_id=${agent}&cursor=${ok}`,
    `/v1/alerts?agent_id=${agent}&severity_id=6&cursor=${ok}`,
    "/v1/alerts?severity_id=0",
    // The earliest year a cursor may carry.
    `/v1/incidents?cursor=${cursor("0001-01-01T00:00:00.000Z", agent)}`,
    "/v1/incidents?status=unknown-status",
  ]) {
    const res = await get(url);
    expect(res.statusCode, url).toBe(200);
  }
});

test("an existing incident spelled with braces or without hyphens is a 404, not the incident", async () => {
  const braces = `{${incidentId}}`;
  const bare = incidentId.replaceAll("-", "");
  for (const suffix of ["", "/events", "/forensic-export", "/report"]) {
    for (const id of [braces, bare]) {
      const res = await get(`/v1/incidents/${encodeURIComponent(id)}${suffix}`);
      expect(res.statusCode, `${id}${suffix}`).toBe(404);
    }
  }
  // The same incident in canonical form is found, so the 404s above are the form.
  expect((await get(`/v1/incidents/${incidentId}`)).statusCode).toBe(200);
  for (const url of [
    `/v1/incidents?agent_id=${encodeURIComponent(`{${agentId}}`)}`,
    `/v1/alerts?agent_id=${agentId.replaceAll("-", "")}`,
  ]) {
    const res = await get(url);
    expect(res.statusCode, url).toBe(400);
    expect(res.json()).toEqual({ error: "bad_request" });
  }
});

test("an :id in upper case is read, and answered, in lower case", async () => {
  const upper = incidentId.toUpperCase();
  const detail = await get(`/v1/incidents/${upper}`);
  expect(detail.statusCode).toBe(200);
  expect(detail.json().incident_id).toBe(incidentId);
  for (const suffix of ["/events", "/forensic-export"]) {
    const res = await get(`/v1/incidents/${upper}${suffix}`);
    expect(res.statusCode, suffix).toBe(200);
    expect(res.json().incident_id, suffix).toBe(incidentId);
  }
});

test("a severity_id read as a number, as before: 06 is 6", async () => {
  const res = await get("/v1/alerts?severity_id=06");
  expect(res.statusCode).toBe(200);
});

test("an unknown status matches no row on either list, and an empty parameter is absent", async () => {
  for (const url of ["/v1/incidents?status=unknown-status", "/v1/alerts?status=unknown-status"]) {
    const res = await get(url);
    expect(res.statusCode, url).toBe(200);
    expect(res.json().items, url).toEqual([]);
  }
  const all = await get(`/v1/alerts?agent_id=${agentId}`);
  for (const url of [
    `/v1/alerts?agent_id=${agentId}&cursor=&severity_id=&status=`,
    `/v1/incidents?agent_id=${agentId}&cursor=`,
  ]) {
    const res = await get(url);
    expect(res.statusCode, url).toBe(200);
    expect(res.json().items.length, url).toBeGreaterThan(0);
  }
  expect(all.json().items.length).toBe(1);
});
