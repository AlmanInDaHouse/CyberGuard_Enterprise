import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { computeChainHex } from "../src/forensic/hashchain.js";
import type { TimelineEvent } from "../src/read/types.js";
import { buildTestApp, seedSession } from "./helpers/db.js";
import { insertCgesEvent } from "./helpers/events-schema.js";
import { ensureAgent, insertAlertRow, insertIncidentRow } from "./helpers/read-schema.js";

// SPEC-012 hashchain_ac_003 (SC-HC-003) — the `(time, event_id)` total order is
// LOAD-BEARING. Two events with the SAME `time` but distinct `event_id` come back
// from the drill in `event_id` ASC order regardless of INSERT order (SPEC-010
// Amendment 2026-06-06 / Pieza A `queries.ts:262`), and the chain over the drill
// output is the canonical-order chain — which DIFFERS from the reverse order, so the
// tiebreaker is what makes the chain reproducible. Real backend (Postgres +
// ClickHouse drill).

let apph: Awaited<ReturnType<typeof buildTestApp>>;
let config: Config;

beforeAll(async () => {
  config = inject("apiConfig");
  apph = await buildTestApp(config);
});
afterAll(async () => {
  await apph?.close();
});

test("hashchain_ac_003: same-time events → deterministic (time, event_id) drill order → reproducible chain", async () => {
  const org = "hashchain-ac-003";
  const agentId = globalThis.crypto.randomUUID();
  const alertId = globalThis.crypto.randomUUID();
  const incidentId = globalThis.crypto.randomUUID();
  // Two event ids with a KNOWN lexicographic order (eLow < eHigh), identical `time`.
  const eLow = "01934abc-def0-7000-89ab-000000000001";
  const eHigh = "01934abc-def0-7000-89ab-000000000002";
  const t = "2026-06-05 10:00:00.000000000";

  await ensureAgent(config, agentId, org);
  // Insert HIGH first (reverse of the tiebreaker order) to prove the order is the
  // query's `ORDER BY time ASC, event_id ASC`, not the insert order.
  await insertCgesEvent(config, {
    agentId,
    eventId: eHigh,
    orgId: org,
    time: t,
    processName: "powershell.exe",
    imageFileName: "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
    processPid: 4322,
    processParentPid: 1000,
  });
  await insertCgesEvent(config, {
    agentId,
    eventId: eLow,
    orgId: org,
    time: t,
    processName: "powershell.exe",
    imageFileName: "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
    processPid: 4321,
    processParentPid: 1000,
  });
  await insertAlertRow(config, { alertId, agentId, orgId: org, sourceEvents: [eHigh, eLow] });
  await insertIncidentRow(config, { incidentId, agentId, orgId: org, alertIds: [alertId] });
  await seedSession(config, {
    token: "tok-hc-ac003",
    userId: globalThis.crypto.randomUUID(),
    role: "analyst",
    orgId: org,
  });

  const res = await apph.app.inject({
    method: "GET",
    url: `/v1/incidents/${incidentId}/events`,
    cookies: { cgsess: "tok-hc-ac003" },
  });
  expect(res.statusCode).toBe(200);
  const events = res.json().events as TimelineEvent[];
  expect(events.length).toBe(2);

  // Pieza A: the drill applied the (time, event_id) tiebreaker despite the reverse
  // insert order — eLow before eHigh.
  expect(events[0]?.event_id).toBe(eLow);
  expect(events[1]?.event_id).toBe(eHigh);

  // The chain over the drill output equals the canonical-order chain, AND the order
  // is load-bearing: the reverse order yields a DIFFERENT chain — so the tiebreaker
  // is what makes chain_N reproducible for same-time events.
  const canonical = await computeChainHex([events[0] as TimelineEvent, events[1] as TimelineEvent]);
  const reversed = await computeChainHex([events[1] as TimelineEvent, events[0] as TimelineEvent]);
  expect(await computeChainHex(events)).toBe(canonical);
  expect(reversed).not.toBe(canonical);
});
