import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp, seedSession } from "./helpers/db.js";
import { ensureAgent, insertAlertRow, insertIncidentRow } from "./helpers/read-schema.js";

// SPEC-011 incident_severity_ac_003 — the read-model projects the incident's
// aggregated severity. With an incident whose severity_id is the MAX over its
// member alerts, BOTH read endpoints surface it:
//   GET /v1/incidents      (list)   -> items[].severity_id
//   GET /v1/incidents/:id  (detail) -> severity_id
// Org-scoped, behind the SPEC-008 session preHandler. Real read-API + real
// read-target tables (applyReadSchema in startBackends), like read_ac_001/002.

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
});
afterAll(async () => {
  await apph?.close();
});

test("incident severity = MAX(member alerts) is returned by the list AND detail reads", async () => {
  const org = "incident-severity-ac-003";
  const agentId = globalThis.crypto.randomUUID();
  const alertHigh = globalThis.crypto.randomUUID();
  const alertLow = globalThis.crypto.randomUUID();
  const incidentId = globalThis.crypto.randomUUID();

  await ensureAgent(config, agentId, org);
  // Two member alerts with mixed severities; the incident severity is their MAX.
  await insertAlertRow(config, { alertId: alertHigh, agentId, orgId: org, severityId: 6 });
  await insertAlertRow(config, { alertId: alertLow, agentId, orgId: org, severityId: 2 });
  const expectedMax = Math.max(6, 2);
  await insertIncidentRow(config, {
    incidentId,
    agentId,
    orgId: org,
    severityId: expectedMax, // the value the detection-path GREATEST aggregation would compute
    alertIds: [alertHigh, alertLow],
  });
  await seedSession(config, {
    token: "tok-sev-ac003",
    userId: globalThis.crypto.randomUUID(),
    role: "analyst",
    orgId: org,
  });

  // Detail — GET /v1/incidents/:id
  const detail = await apph.app.inject({
    method: "GET",
    url: `/v1/incidents/${incidentId}`,
    cookies: { cgsess: "tok-sev-ac003" },
  });
  expect(detail.statusCode).toBe(200);
  expect(detail.json().severity_id).toBe(expectedMax);

  // List — GET /v1/incidents
  const list = await apph.app.inject({
    method: "GET",
    url: "/v1/incidents",
    cookies: { cgsess: "tok-sev-ac003" },
  });
  expect(list.statusCode).toBe(200);
  const item = list.json().items.find((i: { incident_id: string }) => i.incident_id === incidentId);
  expect(item?.severity_id).toBe(expectedMax);
});
