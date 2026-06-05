import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { upsertIncident } from "../src/detect/incidents.js";
import type { IncidentGroupingInput } from "../src/detect/types.js";
import { enrollTestAgent, getIncidents } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";

// SPEC-007 incident_ac_002 — grouping: N distinct alerts (same agent + canonical
// tactic-set, within one correlation window) collapse to ONE incident carrying N
// alert_ids. Harness-first RED: upsertIncident throws NotImplemented (grouping logic
// lands at the SPEC-007 impl gate). RED-by-design — the incidents table EXISTS
// (migration 0005) and the agent is enrolled, so the red is missing-logic, not
// missing-setup. CI-able.

let config: Config;
beforeAll(() => {
  config = inject("ingestConfig");
});

const MITRE = { tactics: ["execution", "initial-access"], techniques: ["T1059.001", "T1566.001"] };

test("incident_ac_002: 3 distinct alerts, same agent+tactic+window -> 1 incident with 3 alert_ids", async () => {
  const agentId = "01934abc-def0-7000-89ab-000000000020";
  const orgId = "incident-ac-002";
  await enrollTestAgent(config, agentId);

  // Three DISTINCT alerts (distinct alert_id), same agent + tactic-set, identical
  // event_time => same correlation window (robust to bucket-boundary alignment).
  const inputs: IncidentGroupingInput[] = [1, 2, 3].map((n) => ({
    alertId: `01934abc-def0-7000-89ab-0000000002${String(n).padStart(2, "0")}`,
    orgId,
    agentId,
    cgMitre: MITRE,
    eventTime: "2026-05-31 13:00:00.000000000",
    severityId: 4, // SPEC-011 — grouping carries severity; this test does not assert it
  }));
  for (const input of inputs) {
    await upsertIncident(detectConfig(config, orgId), input);
  }

  const incidents = await getIncidents(config, { agentId });
  expect(incidents).toHaveLength(1);
  expect(incidents[0]?.alert_ids).toHaveLength(3);
  expect(incidents[0]?.status).toBe("open");
  expect(incidents[0]?.cg_mitre?.tactics).toContain("execution");
});
