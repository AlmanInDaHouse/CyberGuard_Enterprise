import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp, seedSession } from "./helpers/db.js";
import { insertCgesEvent } from "./helpers/events-schema.js";
import { pdfText } from "./helpers/pdf.js";
import { ensureAgent, insertAlertRow, insertIncidentRow } from "./helpers/read-schema.js";

// SPEC-013 report_ac_003 (SC-RPT-003) — transport. GET /v1/incidents/:id/report over HTTP
// (buildTestApp/inject) returns 200 with Content-Type application/pdf and a valid PDF body
// (the FIRST non-JSON response in the api); requireSession (no/invalid session → 401, mirroring
// the drill route); org-scoped (an incident in another org → 404); a non-existent / malformed
// id → 404. CI-able, throwaway-DB, no marquee. Gate 2 (the route; the module is gate 1).

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
});
afterAll(async () => {
  await apph?.close();
});

test("report_ac_003: GET .../report returns 200 application/pdf with a valid PDF body", async () => {
  const org = "report-ac-003";
  const agentId = globalThis.crypto.randomUUID();
  const alertId = globalThis.crypto.randomUUID();
  const incidentId = globalThis.crypto.randomUUID();
  const eventId = globalThis.crypto.randomUUID();

  await ensureAgent(config, agentId, org);
  await insertCgesEvent(config, {
    agentId,
    eventId,
    orgId: org,
    time: "2026-06-05 13:00:00.000000000",
    processName: "powershell.exe",
  });
  await insertAlertRow(config, { alertId, agentId, orgId: org, sourceEvents: [eventId] });
  await insertIncidentRow(config, {
    incidentId,
    agentId,
    orgId: org,
    title: "Execution on host",
    alertIds: [alertId],
  });
  await seedSession(config, {
    token: "tok-report-ac003",
    userId: globalThis.crypto.randomUUID(),
    role: "analyst",
    orgId: org,
  });

  const res = await apph.app.inject({
    method: "GET",
    url: `/v1/incidents/${incidentId}/report`,
    cookies: { cgsess: "tok-report-ac003" },
  });

  expect(res.statusCode).toBe(200);
  // First non-JSON response in the api.
  expect(res.headers["content-type"]).toContain("application/pdf");
  // A valid PDF body (binary), with the report's content extractable.
  expect(res.rawPayload.subarray(0, 5).toString("latin1")).toBe("%PDF-");
  const text = await pdfText(res.rawPayload);
  expect(text.toLowerCase()).toContain("execution on host"); // the title rendered into the PDF
});

test("report_ac_003: no/invalid session is rejected with 401 before the handler", async () => {
  // A fully-seeded incident so the only variable is the session (mirrors drill_ac_002).
  const org = "report-ac-003-401";
  const agentId = globalThis.crypto.randomUUID();
  const alertId = globalThis.crypto.randomUUID();
  const incidentId = globalThis.crypto.randomUUID();
  const eventId = globalThis.crypto.randomUUID();
  await ensureAgent(config, agentId, org);
  await insertCgesEvent(config, {
    agentId,
    eventId,
    orgId: org,
    time: "2026-06-05 13:10:00.000000000",
  });
  await insertAlertRow(config, { alertId, agentId, orgId: org, sourceEvents: [eventId] });
  await insertIncidentRow(config, { incidentId, agentId, orgId: org, alertIds: [alertId] });

  const noCookie = await apph.app.inject({
    method: "GET",
    url: `/v1/incidents/${incidentId}/report`,
  });
  expect(noCookie.statusCode).toBe(401);

  const bogusCookie = await apph.app.inject({
    method: "GET",
    url: `/v1/incidents/${incidentId}/report`,
    cookies: { cgsess: "not-a-real-session-token" },
  });
  expect(bogusCookie.statusCode).toBe(401);
});

test("report_ac_003: cross-org / non-existent / malformed id all return 404", async () => {
  const orgA = "report-ac-003-orgA";
  const orgB = "report-ac-003-orgB";
  const agentId = globalThis.crypto.randomUUID();
  const alertId = globalThis.crypto.randomUUID();
  const incidentBId = globalThis.crypto.randomUUID();
  const eventId = globalThis.crypto.randomUUID();
  await ensureAgent(config, agentId, orgB);
  await insertCgesEvent(config, {
    agentId,
    eventId,
    orgId: orgB,
    time: "2026-06-05 13:20:00.000000000",
  });
  await insertAlertRow(config, { alertId, agentId, orgId: orgB, sourceEvents: [eventId] });
  await insertIncidentRow(config, {
    incidentId: incidentBId,
    agentId,
    orgId: orgB,
    alertIds: [alertId],
  });
  // The caller's session is in org A; the incident lives in org B.
  await seedSession(config, {
    token: "tok-report-ac003-orgA",
    userId: globalThis.crypto.randomUUID(),
    role: "analyst",
    orgId: orgA,
  });

  // cross-org → 404 (no cross-org read, no existence oracle)
  const crossOrg = await apph.app.inject({
    method: "GET",
    url: `/v1/incidents/${incidentBId}/report`,
    cookies: { cgsess: "tok-report-ac003-orgA" },
  });
  expect(crossOrg.statusCode).toBe(404);

  // non-existent incident (valid UUID, no row) → 404
  const missing = await apph.app.inject({
    method: "GET",
    url: `/v1/incidents/${globalThis.crypto.randomUUID()}/report`,
    cookies: { cgsess: "tok-report-ac003-orgA" },
  });
  expect(missing.statusCode).toBe(404);

  // malformed :id (not a UUID → pg cast throws, caught) → 404
  const malformed = await apph.app.inject({
    method: "GET",
    url: "/v1/incidents/not-a-uuid/report",
    cookies: { cgsess: "tok-report-ac003-orgA" },
  });
  expect(malformed.statusCode).toBe(404);
});
