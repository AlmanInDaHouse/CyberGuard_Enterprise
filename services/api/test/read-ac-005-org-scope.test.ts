import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp, seedSession } from "./helpers/db.js";
import { ensureAgent, insertIncidentRow } from "./helpers/read-schema.js";

// read_ac_005 — org-scope (SECURITY): a session sees ONLY its own org's incidents
// (the object-level enforcement point, scoped-to-org for the single-org MVP;
// SPEC-009 §Security considerations).
//
// RED BY DESIGN: the handler is a stub (501), so NO org filtering happens. Two
// orgs' incidents are seeded into the REAL tables, so the RED is the absent
// FILTERING logic — NOT a missing table. GREEN: org A's session sees only org A's
// incident; org B's row is never returned.

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
});
afterAll(async () => {
  await apph?.close();
});

test("a session sees only its own org's incidents", async () => {
  const orgA = "read-ac-005-a";
  const orgB = "read-ac-005-b";
  const agentA = globalThis.crypto.randomUUID();
  const agentB = globalThis.crypto.randomUUID();
  const incidentA = globalThis.crypto.randomUUID();
  const incidentB = globalThis.crypto.randomUUID();
  await ensureAgent(config, agentA, orgA);
  await ensureAgent(config, agentB, orgB);
  await insertIncidentRow(config, {
    incidentId: incidentA,
    agentId: agentA,
    orgId: orgA,
    alertIds: [globalThis.crypto.randomUUID()],
  });
  await insertIncidentRow(config, {
    incidentId: incidentB,
    agentId: agentB,
    orgId: orgB,
    alertIds: [globalThis.crypto.randomUUID()],
  });
  await seedSession(config, {
    token: "tok-read-ac005",
    userId: globalThis.crypto.randomUUID(),
    role: "analyst",
    orgId: orgA,
  });

  const res = await apph.app.inject({
    method: "GET",
    url: "/v1/incidents",
    cookies: { cgsess: "tok-read-ac005" },
  });
  // RED: 501 (org-filtering logic absent). GREEN: 200, and org B's incident is
  // never present in org A's results.
  expect(res.statusCode).toBe(200);
  const body = res.json();
  // Both sides: a session sees its OWN org's incident, and NEVER the other org's.
  expect(body.items.some((i: { incident_id: string }) => i.incident_id === incidentA)).toBe(true);
  expect(body.items.some((i: { incident_id: string }) => i.incident_id === incidentB)).toBe(false);
});
