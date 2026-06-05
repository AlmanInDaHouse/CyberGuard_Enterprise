import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { upsertIncident } from "../src/detect/incidents.js";
import type { IncidentGroupingInput } from "../src/detect/types.js";
import { enrollTestAgent, getIncidents } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";

// SPEC-011 incident_severity_ac_001 — the MAX marquee. Two distinct alerts in the
// SAME correlation window (same grouping_key) collapse to one incident whose
// severity_id is the GREATEST of the members, regardless of ARRIVAL ORDER:
//   low then high  -> high
//   high then low  -> high (a lower follower never lowers it)
// Proves GREATEST(incidents.severity_id, excluded.severity_id) is commutative /
// order-independent. Real Postgres (testcontainers), like incident_ac_004. CI-able.

let config: Config;
beforeAll(() => {
  config = inject("ingestConfig");
});

const MITRE = { tactics: ["execution", "initial-access"], techniques: ["T1059.001", "T1566.001"] };
const WINDOW = "2026-05-31 18:00:00.000000000";

function input(agentId: string, n: number, severityId: number): IncidentGroupingInput {
  return {
    alertId: `01934abc-def0-7000-89ab-0000000006${String(n).padStart(2, "0")}`,
    orgId: "incident-severity-ac-001",
    agentId,
    cgMitre: MITRE,
    eventTime: WINDOW,
    severityId,
  };
}

test("incident_severity_ac_001: incident severity is MAX of its alerts, low->high then high->low", async () => {
  const orgId = "incident-severity-ac-001";

  // Incident A: low (2) THEN high (6) -> escalates to 6.
  const agentA = "01934abc-def0-7000-89ab-0000000006a0";
  await enrollTestAgent(config, agentA);
  await upsertIncident(detectConfig(config, orgId), input(agentA, 1, 2));
  await upsertIncident(detectConfig(config, orgId), input(agentA, 2, 6));

  // Incident B: high (6) THEN low (2) -> stays at 6 (a lower follower never lowers it).
  const agentB = "01934abc-def0-7000-89ab-0000000006b0";
  await enrollTestAgent(config, agentB);
  await upsertIncident(detectConfig(config, orgId), input(agentB, 3, 6));
  await upsertIncident(detectConfig(config, orgId), input(agentB, 4, 2));

  const incA = (await getIncidents(config, { agentId: agentA }))[0];
  expect(incA?.alert_ids).toHaveLength(2);
  expect(incA?.severity_id).toBe(6);

  const incB = (await getIncidents(config, { agentId: agentB }))[0];
  expect(incB?.alert_ids).toHaveLength(2);
  expect(incB?.severity_id).toBe(6);
});
