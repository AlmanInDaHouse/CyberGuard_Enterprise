import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp, seedSession } from "./helpers/db.js";
import { ensureAgent, insertIncidentRow } from "./helpers/read-schema.js";

// read_ac_002 — GET /v1/incidents returns the incident list with pagination.
//
// RED BY DESIGN: the handler is a stub (501). Two incidents are seeded into the
// REAL tables and the session is valid, so the RED is the absent list/pagination
// LOGIC. GREEN: 200 with items[] and a next_cursor when more exist (limit-capped).

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
});
afterAll(async () => {
  await apph?.close();
});

test("incidents list paginates (limit + next_cursor)", async () => {
  const org = "read-ac-002";
  const agentId = globalThis.crypto.randomUUID();
  await ensureAgent(config, agentId, org);
  await insertIncidentRow(config, {
    incidentId: globalThis.crypto.randomUUID(),
    agentId,
    orgId: org,
    alertIds: [globalThis.crypto.randomUUID()],
  });
  await insertIncidentRow(config, {
    incidentId: globalThis.crypto.randomUUID(),
    agentId,
    orgId: org,
    alertIds: [globalThis.crypto.randomUUID()],
  });
  await seedSession(config, {
    token: "tok-read-ac002",
    userId: globalThis.crypto.randomUUID(),
    role: "viewer",
    orgId: org,
  });

  const res = await apph.app.inject({
    method: "GET",
    url: "/v1/incidents?limit=1",
    cookies: { cgsess: "tok-read-ac002" },
  });
  // RED: 501 (list logic absent). GREEN: 200, one item, a next_cursor.
  expect(res.statusCode).toBe(200);
  const body = res.json();
  expect(body.items.length).toBe(1);
  expect(body.next_cursor).toBeTruthy();
});
