import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp, seedSession } from "./helpers/db.js";
import { ensureAgent, insertAlertRow, insertIncidentRow } from "./helpers/read-schema.js";

// read_ac_001 — GET /v1/incidents/:id returns the incident detail with its
// alert_ids RESOLVED + MITRE (the teachable view).
//
// RED BY DESIGN: the handler is a stub (NotImplementedError → 501). The
// precondition incident + alert are seeded into the REAL read-target tables
// (materialised by applyReadSchema in startBackends), and the session is valid, so
// the RED is the absent read LOGIC — never a missing table, never an auth reject.
// GREEN: 200 with the resolved alert(s) + cg_mitre.

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
});
afterAll(async () => {
  await apph?.close();
});

test("incident detail resolves its alerts + MITRE", async () => {
  const org = "read-ac-001";
  const agentId = globalThis.crypto.randomUUID();
  const alertId = globalThis.crypto.randomUUID();
  const incidentId = globalThis.crypto.randomUUID();
  await ensureAgent(config, agentId, org);
  await insertAlertRow(config, { alertId, agentId, orgId: org });
  await insertIncidentRow(config, { incidentId, agentId, orgId: org, alertIds: [alertId] });
  await seedSession(config, {
    token: "tok-read-ac001",
    userId: globalThis.crypto.randomUUID(),
    role: "analyst",
    orgId: org,
  });

  const res = await apph.app.inject({
    method: "GET",
    url: `/v1/incidents/${incidentId}`,
    cookies: { cgsess: "tok-read-ac001" },
  });
  // RED: 501 (read logic absent). GREEN: 200 with resolved alerts + MITRE.
  expect(res.statusCode).toBe(200);
  const body = res.json();
  expect(Array.isArray(body.alerts)).toBe(true);
  expect(body.alerts.length).toBe(1);
  expect((body.cg_mitre?.tactics ?? []).length).toBeGreaterThan(0);
});
