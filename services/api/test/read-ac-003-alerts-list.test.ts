import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildTestApp, seedSession } from "./helpers/db.js";
import { ensureAgent, insertAlertRow } from "./helpers/read-schema.js";

// read_ac_003 — GET /v1/alerts returns the alert list.
//
// RED BY DESIGN: the handler is a stub (501). An alert is seeded into the REAL
// table and the session is valid, so the RED is the absent list LOGIC. GREEN: 200
// with items[] of the documented AlertListItem shape.

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
});
afterAll(async () => {
  await apph?.close();
});

test("alerts list returns items", async () => {
  const org = "read-ac-003";
  const agentId = globalThis.crypto.randomUUID();
  await ensureAgent(config, agentId, org);
  await insertAlertRow(config, { alertId: globalThis.crypto.randomUUID(), agentId, orgId: org });
  await seedSession(config, {
    token: "tok-read-ac003",
    userId: globalThis.crypto.randomUUID(),
    role: "viewer",
    orgId: org,
  });

  const res = await apph.app.inject({
    method: "GET",
    url: "/v1/alerts",
    cookies: { cgsess: "tok-read-ac003" },
  });
  // RED: 501 (list logic absent). GREEN: 200 with the alert items.
  expect(res.statusCode).toBe(200);
  const body = res.json();
  expect(Array.isArray(body.items)).toBe(true);
  expect(body.items.length).toBeGreaterThanOrEqual(1);
});
