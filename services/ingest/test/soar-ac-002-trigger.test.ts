import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { runDetectionCycle } from "../src/detect/index.js";
import { enrollTestAgent, getIncidents } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";
import { spyNotify } from "./helpers/notify.js";
import {
  type SoarHarness,
  getExecutions,
  seedOfficeSpawn,
  seedStartupFolderExec,
  shippedPlaybooks,
  soarHarness,
  testPlaybook,
} from "./helpers/soar.js";

// soar_ac_002 — SPEC-022 §Operational §2: at incident create, after SPEC-014's
// notification, the cycle runs each playbook whose trust_sources holds `rule` and
// whose severity floor the incident's severity at creation reaches.

let config: Config;
let harness: SoarHarness;

beforeAll(() => {
  config = inject("ingestConfig");
  harness = soarHarness(config, shippedPlaybooks());
});
afterAll(async () => {
  await harness?.close();
});

test("a High incident from a rule runs the shipped playbook once", async () => {
  const org = "soar-ac-002-high";
  const agent = globalThis.crypto.randomUUID();
  await enrollTestAgent(config, agent, org);
  await seedOfficeSpawn(config, org, agent);

  await runDetectionCycle(detectConfig(config, org), spyNotify().notify, harness.soar);

  const incidents = await getIncidents(config, { agentId: agent });
  expect(incidents).toHaveLength(1);
  expect(incidents[0]?.severity_id).toBe(4);
  const executions = await getExecutions(config, incidents[0]?.incident_id ?? "");
  expect(executions.map((e) => [e.playbook_id, e.playbook_version])).toEqual([
    ["pb_high_severity_containment", 1],
  ]);
});

test("a Medium incident runs no playbook", async () => {
  const org = "soar-ac-002-medium";
  const agent = globalThis.crypto.randomUUID();
  await enrollTestAgent(config, agent, org);
  await seedStartupFolderExec(config, org, agent);

  await runDetectionCycle(detectConfig(config, org), spyNotify().notify, harness.soar);

  const incidents = await getIncidents(config, { agentId: agent });
  expect(incidents.length).toBeGreaterThan(0);
  for (const incident of incidents) {
    expect(incident.severity_id).toBe(3);
    expect(await getExecutions(config, incident.incident_id)).toEqual([]);
  }
});

test("a playbook whose trust_sources lacks rule does not run", async () => {
  const org = "soar-ac-002-trust";
  const agent = globalThis.crypto.randomUUID();
  await enrollTestAgent(config, agent, org);
  await seedOfficeSpawn(config, org, agent);
  const mlOnly = soarHarness(config, [testPlaybook({ id: "pb_ml_only", trustSources: ["ml"] })]);
  try {
    await runDetectionCycle(detectConfig(config, org), spyNotify().notify, mlOnly.soar);
  } finally {
    await mlOnly.close();
  }

  const incidents = await getIncidents(config, { agentId: agent });
  expect(incidents).toHaveLength(1);
  expect(await getExecutions(config, incidents[0]?.incident_id ?? "")).toEqual([]);
});

test("an alert that joins an existing incident runs nothing more", async () => {
  const org = "soar-ac-002-update";
  const agent = globalThis.crypto.randomUUID();
  await enrollTestAgent(config, agent, org);
  await seedOfficeSpawn(config, org, agent);
  await runDetectionCycle(detectConfig(config, org), spyNotify().notify, harness.soar);

  // A second Office spawn within the correlation window joins the same incident.
  await seedOfficeSpawn(config, org, agent, 1);
  const spy = spyNotify();
  const before = harness.logs.length;
  const second = await runDetectionCycle(detectConfig(config, org), spy.notify, harness.soar);
  // The second alert was written, and joined the incident.
  expect(second.alertsWritten).toBe(1);
  // Not triggered at all, rather than triggered and stopped by the claim.
  expect(harness.logs.slice(before)).toEqual([]);

  const incidents = await getIncidents(config, { agentId: agent });
  expect(incidents).toHaveLength(1);
  expect(incidents[0]?.alert_ids).toHaveLength(2);
  expect(spy.sent).toEqual([]);
  expect(await getExecutions(config, incidents[0]?.incident_id ?? "")).toHaveLength(1);
});
