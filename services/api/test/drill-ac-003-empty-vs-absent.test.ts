import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp, seedSession } from "./helpers/db.js";
import { ensureAgent, insertAlertRow, insertIncidentRow } from "./helpers/read-schema.js";

// drill_ac_003 — SPEC-010: empty (existing, no resolvable raw events) vs absent.
// (a) an existing incident whose alerts resolve to no matching cges_events → 200
//     { events: [] } (NOT 404 — the incident exists).
// (b) a non-existent incident OR a non-UUID :id → 404.

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
});
afterAll(async () => {
  await apph?.close();
});

test("existing incident with no resolvable events returns 200 with an empty list", async () => {
  const org = "drill-ac-003";
  const agentId = globalThis.crypto.randomUUID();
  const alertId = globalThis.crypto.randomUUID();
  const incidentId = globalThis.crypto.randomUUID();
  // source_events points at an event id that was never inserted into cges_events,
  // so the cross-store lookup resolves to zero rows.
  const danglingEventId = globalThis.crypto.randomUUID();
  await ensureAgent(config, agentId, org);
  await insertAlertRow(config, { alertId, agentId, orgId: org, sourceEvents: [danglingEventId] });
  await insertIncidentRow(config, { incidentId, agentId, orgId: org, alertIds: [alertId] });
  await seedSession(config, {
    token: "tok-drill-ac003",
    userId: globalThis.crypto.randomUUID(),
    role: "viewer",
    orgId: org,
  });

  const res = await apph.app.inject({
    method: "GET",
    url: `/v1/incidents/${incidentId}/events`,
    cookies: { cgsess: "tok-drill-ac003" },
  });
  expect(res.statusCode).toBe(200);
  const body = res.json();
  expect(body.incident_id).toBe(incidentId);
  expect(body.events).toEqual([]);
});

test("a non-existent incident and a non-UUID :id both return 404", async () => {
  const org = "drill-ac-003";
  await seedSession(config, {
    token: "tok-drill-ac003b",
    userId: globalThis.crypto.randomUUID(),
    role: "analyst",
    orgId: org,
  });

  const absent = await apph.app.inject({
    method: "GET",
    url: `/v1/incidents/${globalThis.crypto.randomUUID()}/events`,
    cookies: { cgsess: "tok-drill-ac003b" },
  });
  expect(absent.statusCode).toBe(404);

  const malformed = await apph.app.inject({
    method: "GET",
    url: "/v1/incidents/not-a-uuid/events",
    cookies: { cgsess: "tok-drill-ac003b" },
  });
  expect(malformed.statusCode).toBe(404);
});
