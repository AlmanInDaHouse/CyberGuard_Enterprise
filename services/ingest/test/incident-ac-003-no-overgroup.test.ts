import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { upsertIncident } from "../src/detect/incidents.js";
import type { IncidentGroupingInput } from "../src/detect/types.js";
import { enrollTestAgent, getIncidents } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";

// SPEC-007 incident_ac_003 — no over-group: alerts that differ by canonical
// tactic-set OR fall outside the correlation window MUST land in separate incidents.
// Harness-first RED: upsertIncident throws NotImplemented. RED-by-design. CI-able.

let config: Config;
beforeAll(() => {
  config = inject("ingestConfig");
});

const TACTICS_EXEC = { tactics: ["execution", "initial-access"], techniques: ["T1059.001"] };
const TACTICS_PERSIST = { tactics: ["persistence"], techniques: ["T1547.001"] };

test("incident_ac_003: distinct tactic-set OR out-of-window -> separate incidents", async () => {
  const agentId = "01934abc-def0-7000-89ab-000000000030";
  const orgId = "incident-ac-003";
  await enrollTestAgent(config, agentId);

  const inputs: IncidentGroupingInput[] = [
    // A: execution tactic-set, window @ 14:00.
    {
      alertId: "01934abc-def0-7000-89ab-000000000301",
      orgId,
      agentId,
      cgMitre: TACTICS_EXEC,
      eventTime: "2026-05-31 14:00:00.000000000",
      severityId: 4, // SPEC-011 — grouping carries severity; this test asserts grouping only
    },
    // B: DIFFERENT tactic-set, same agent + same window @ 14:00 -> separate incident.
    {
      alertId: "01934abc-def0-7000-89ab-000000000302",
      orgId,
      agentId,
      cgMitre: TACTICS_PERSIST,
      eventTime: "2026-05-31 14:00:00.000000000",
      severityId: 4,
    },
    // C: same tactic-set as A, but 2 h later (> 1800 s) -> different window -> separate incident.
    {
      alertId: "01934abc-def0-7000-89ab-000000000303",
      orgId,
      agentId,
      cgMitre: TACTICS_EXEC,
      eventTime: "2026-05-31 16:00:00.000000000",
      severityId: 4,
    },
  ];
  for (const input of inputs) {
    await upsertIncident(detectConfig(config, orgId), input);
  }

  // Three distinct grouping_keys: (exec, 14:00-window), (persist, 14:00-window),
  // (exec, 16:00-window) -> three incidents.
  const incidents = await getIncidents(config, { agentId });
  expect(incidents).toHaveLength(3);
});
