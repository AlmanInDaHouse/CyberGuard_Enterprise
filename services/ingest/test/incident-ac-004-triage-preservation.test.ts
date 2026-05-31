import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { upsertIncident } from "../src/detect/incidents.js";
import type { IncidentGroupingInput } from "../src/detect/types.js";
import { enrollTestAgent, getIncidents, setIncidentTriage } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";

// SPEC-007 incident_ac_004 — triage preservation: a new correlated alert entering an
// existing incident's window appends its alert_id and bumps updated_at, but MUST NOT
// reset human triage (status / assigned_to). The incident analogue of SPEC-006's
// ON CONFLICT DO NOTHING, here a TARGETED DO UPDATE. Harness-first RED: upsertIncident
// throws NotImplemented. RED-by-design. CI-able.

let config: Config;
beforeAll(() => {
  config = inject("ingestConfig");
});

const MITRE = { tactics: ["execution", "initial-access"], techniques: ["T1059.001", "T1566.001"] };

test("incident_ac_004: a new alert in window preserves status/assigned_to, grows alert_ids", async () => {
  const agentId = "01934abc-def0-7000-89ab-000000000040";
  const orgId = "incident-ac-004";
  await enrollTestAgent(config, agentId);

  const first: IncidentGroupingInput = {
    alertId: "01934abc-def0-7000-89ab-000000000401",
    orgId,
    agentId,
    cgMitre: MITRE,
    eventTime: "2026-05-31 16:00:00.000000000",
  };
  await upsertIncident(detectConfig(config, orgId), first);

  // An analyst moves the incident to investigating and assigns it.
  const created = (await getIncidents(config, { agentId }))[0];
  expect(created).toBeDefined();
  await setIncidentTriage(config, created?.incident_id ?? "", {
    status: "investigating",
    assignedTo: "analyst-x",
  });

  // A second distinct alert in the SAME window (identical event_time) -> same grouping_key.
  const second: IncidentGroupingInput = {
    alertId: "01934abc-def0-7000-89ab-000000000402",
    orgId,
    agentId,
    cgMitre: MITRE,
    eventTime: "2026-05-31 16:00:00.000000000",
  };
  await upsertIncident(detectConfig(config, orgId), second);

  const incident = (await getIncidents(config, { agentId }))[0];
  expect(incident?.status).toBe("investigating");
  expect(incident?.assigned_to).toBe("analyst-x");
  expect(incident?.alert_ids).toHaveLength(2);
});
