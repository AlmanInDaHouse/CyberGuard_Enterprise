import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp, seedSession } from "./helpers/db.js";
import { insertCgesEvent } from "./helpers/events-schema.js";
import { ensureAgent, insertAlertRow, insertIncidentRow } from "./helpers/read-schema.js";

// drill_ac_002 — SPEC-010 SECURITY: session + org enforcement on the drill.
// (a) no/invalid cgsess → 401 BEFORE the handler (makeRequireSession).
// (b) an incident in another org → 404 (no cross-org leak, no existence oracle).

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
});
afterAll(async () => {
  await apph?.close();
});

test("no/invalid session is rejected with 401 before the handler", async () => {
  // A real, fully-seeded incident in org B so the only variable is the session.
  const orgB = "drill-ac-002-orgB";
  const agentId = globalThis.crypto.randomUUID();
  const alertId = globalThis.crypto.randomUUID();
  const incidentId = globalThis.crypto.randomUUID();
  const eventId = globalThis.crypto.randomUUID();
  await ensureAgent(config, agentId, orgB);
  await insertCgesEvent(config, {
    agentId,
    eventId,
    orgId: orgB,
    time: "2026-06-05 11:00:00.000000000",
  });
  await insertAlertRow(config, { alertId, agentId, orgId: orgB, sourceEvents: [eventId] });
  await insertIncidentRow(config, { incidentId, agentId, orgId: orgB, alertIds: [alertId] });

  const noCookie = await apph.app.inject({
    method: "GET",
    url: `/v1/incidents/${incidentId}/events`,
  });
  expect(noCookie.statusCode).toBe(401);

  const bogusCookie = await apph.app.inject({
    method: "GET",
    url: `/v1/incidents/${incidentId}/events`,
    cookies: { cgsess: "not-a-real-session-token" },
  });
  expect(bogusCookie.statusCode).toBe(401);
});

test("an incident in another org returns 404 (no cross-org read, no existence oracle)", async () => {
  const orgA = "drill-ac-002-orgA";
  const orgB = "drill-ac-002-orgB2";
  const agentId = globalThis.crypto.randomUUID();
  const alertId = globalThis.crypto.randomUUID();
  const incidentBId = globalThis.crypto.randomUUID();
  const eventId = globalThis.crypto.randomUUID();
  await ensureAgent(config, agentId, orgB);
  await insertCgesEvent(config, {
    agentId,
    eventId,
    orgId: orgB,
    time: "2026-06-05 11:30:00.000000000",
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
    token: "tok-drill-ac002-orgA",
    userId: globalThis.crypto.randomUUID(),
    role: "analyst",
    orgId: orgA,
  });

  const res = await apph.app.inject({
    method: "GET",
    url: `/v1/incidents/${incidentBId}/events`,
    cookies: { cgsess: "tok-drill-ac002-orgA" },
  });
  expect(res.statusCode).toBe(404);
});
