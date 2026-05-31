import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { upsertIncident } from "../src/detect/incidents.js";
import type { IncidentGroupingInput } from "../src/detect/types.js";
import { enrollTestAgent, getIncidents } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";

// SPEC-007 incident_ac_005 — production-faithful FK (Convention #12): the
// incidents.agent_id -> agents FK rejects an incident for an unenrolled agent (the
// constraint doing its job); the test satisfies the production precondition by
// ENROLLING the agent, NEVER by weakening the FK. Harness-first RED: upsertIncident
// throws NotImplemented, so the FK-rejection match fails (the message is not a FK
// error). RED-by-design; goes GREEN when the grouping logic + FK behaviour land.
// CI-able.

let config: Config;
beforeAll(() => {
  config = inject("ingestConfig");
});

const MITRE = { tactics: ["execution", "initial-access"], techniques: ["T1059.001", "T1566.001"] };

test("incident_ac_005: FK rejects unenrolled agent; enrolling satisfies it (FK never weakened)", async () => {
  const agentId = "01934abc-def0-7000-89ab-000000000050";
  const orgId = "incident-ac-005";
  const input: IncidentGroupingInput = {
    alertId: "01934abc-def0-7000-89ab-000000000501",
    orgId,
    agentId,
    cgMitre: MITRE,
    eventTime: "2026-05-31 17:00:00.000000000",
  };

  // Agent NOT enrolled: the incidents.agent_id -> agents FK must reject the insert.
  // (In the RED phase upsertIncident throws NotImplemented, whose message is not a FK
  // error, so this matcher fails by design until the grouping logic lands.)
  await expect(upsertIncident(detectConfig(config, orgId), input)).rejects.toThrow(
    /foreign key|violates/i,
  );

  // Production-faithful fix: enroll the agent (satisfy the precondition the FK
  // encodes), never weaken the constraint.
  await enrollTestAgent(config, agentId);
  await upsertIncident(detectConfig(config, orgId), input);

  const incidents = await getIncidents(config, { agentId });
  expect(incidents).toHaveLength(1);
  expect(incidents[0]?.agent_id).toBe(agentId);
});
