import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp, seedSession } from "./helpers/db.js";
import { insertCgesEvent } from "./helpers/events-schema.js";
import { ensureAgent, insertAlertRow, insertIncidentRow } from "./helpers/read-schema.js";

// drill_ac_001 — SPEC-010 marquee: the real cross-store join Postgres → ClickHouse.
// Seed real cges_events in ClickHouse + an incident whose alert's source_events point
// at those event ids; GET /v1/incidents/:id/events returns 200 with the events in
// `time` ASC order, org-scoped, each carrying the TimelineEvent fields.

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
});
afterAll(async () => {
  await apph?.close();
});

test("incident drill resolves its raw cges_events timeline (time ASC, cross-store)", async () => {
  const org = "drill-ac-001";
  const agentId = globalThis.crypto.randomUUID();
  const alertId = globalThis.crypto.randomUUID();
  const incidentId = globalThis.crypto.randomUUID();
  const eventEarly = globalThis.crypto.randomUUID();
  const eventLate = globalThis.crypto.randomUUID();

  await ensureAgent(config, agentId, org);
  // Insert LATE first to prove the ORDER BY time ASC is the query's, not insert order.
  await insertCgesEvent(config, {
    agentId,
    eventId: eventLate,
    orgId: org,
    time: "2026-06-05 10:00:05.000000000",
    processName: "powershell.exe",
    imageFileName: "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
    processPid: 4322,
    processParentPid: 1000,
  });
  await insertCgesEvent(config, {
    agentId,
    eventId: eventEarly,
    orgId: org,
    time: "2026-06-05 10:00:00.000000000",
    processName: "powershell.exe",
    imageFileName: "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
    processPid: 4321,
    processParentPid: 1000,
  });
  await insertAlertRow(config, {
    alertId,
    agentId,
    orgId: org,
    sourceEvents: [eventEarly, eventLate],
  });
  await insertIncidentRow(config, { incidentId, agentId, orgId: org, alertIds: [alertId] });
  await seedSession(config, {
    token: "tok-drill-ac001",
    userId: globalThis.crypto.randomUUID(),
    role: "analyst",
    orgId: org,
  });

  const res = await apph.app.inject({
    method: "GET",
    url: `/v1/incidents/${incidentId}/events`,
    cookies: { cgsess: "tok-drill-ac001" },
  });

  expect(res.statusCode).toBe(200);
  const body = res.json();
  expect(body.incident_id).toBe(incidentId);
  expect(Array.isArray(body.events)).toBe(true);
  expect(body.events.length).toBe(2);
  // Ordered by time ASC — the early event comes first regardless of insert order.
  expect(body.events[0].event_id).toBe(eventEarly);
  expect(body.events[1].event_id).toBe(eventLate);
  expect(body.events[0].event_time < body.events[1].event_time).toBe(true);
  // TimelineEvent shape + org-scoped agent.
  const e0 = body.events[0];
  expect(e0.agent_id).toBe(agentId);
  expect(e0.process_pid).toBe(4321);
  expect(e0.process_name).toBe("powershell.exe");
  expect(typeof e0.image_file_name).toBe("string");
  expect(e0.process_parent_pid).toBe(1000);
  expect(typeof e0.activity_id).toBe("number");
});
