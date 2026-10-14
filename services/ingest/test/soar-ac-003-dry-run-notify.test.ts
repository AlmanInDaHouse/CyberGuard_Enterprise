import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { runDetectionCycle } from "../src/detect/index.js";
import { runPlaybooks } from "../src/soar/run.js";
import { enrollTestAgent, getIncidents } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";
import { spyNotify } from "./helpers/notify.js";
import {
  type SoarHarness,
  getExecutions,
  seedOfficeSpawn,
  shippedPlaybooks,
  soarHarness,
  testPlaybook,
} from "./helpers/soar.js";

// soar_ac_003 — SPEC-022 §Operational §4–§5: the shipped playbook records the
// isolation it would do and emails the SOC with what it recorded; SPEC-014's own
// email is sent too; with no notify carrier the email step is skipped.

let config: Config;
let harness: SoarHarness;

beforeAll(() => {
  config = inject("ingestConfig");
  harness = soarHarness(config, shippedPlaybooks());
});
afterAll(async () => {
  await harness?.close();
});

/** Enroll an agent and run one cycle over an Office spawn; return the incident. */
async function highIncident(org: string, notify = spyNotify()) {
  const agent = globalThis.crypto.randomUUID();
  await enrollTestAgent(config, agent, org);
  await seedOfficeSpawn(config, org, agent);
  await runDetectionCycle(detectConfig(config, org), notify.notify, harness.soar);
  const [incident] = await getIncidents(config, { agentId: agent });
  if (!incident) throw new Error("no incident");
  return { agent, incident, notify };
}

test("the record holds the dry-run isolation and the sent email", async () => {
  const { agent, incident, notify } = await highIncident("soar-ac-003-record");
  const [execution] = await getExecutions(config, incident.incident_id);
  expect(execution?.status).toBe("completed");
  expect(execution?.chain_head).toMatch(/^[0-9a-f]{64}$/);
  expect(execution?.steps.map((s) => [s.step_id, s.action, s.mode, s.outcome])).toEqual([
    ["isolate", "agent.isolate", "dry_run", "would_do"],
    ["notify", "notify.email", "live", "done"],
  ]);
  expect(execution?.steps[0]?.detail).toContain(`would isolate agent ${agent}`);
  expect(execution?.steps[1]?.detail).toBe("sent");

  // Two emails: SPEC-014's announcement, then the playbook's.
  expect(notify.sent).toHaveLength(2);
  const playbookMail = notify.sent[1];
  expect(playbookMail?.subject).toBe(`[CyberGuard] Containment pending: ${incident.title}`);
  expect(playbookMail?.text).toContain(`Incident: ${incident.incident_id}`);
  expect(playbookMail?.text).toContain(
    `isolate agent.isolate dry_run: would_do — would isolate agent ${agent}`,
  );
});

test("a subject has its line breaks replaced with spaces", async () => {
  const { agent, incident } = await highIncident("soar-ac-003-subject");
  const spy = spyNotify();
  const own = soarHarness(config, [testPlaybook({ id: "pb_subject" })]);
  try {
    await runPlaybooks(own.soar, spy.notify, {
      incidentId: incident.incident_id,
      orgId: "soar-ac-003-subject",
      agentId: agent,
      title: "first line\nsecond line\r\nthird\u2028fourth\u0085fifth",
      severityId: 4,
      detectionSource: "rule",
    });
  } finally {
    await own.close();
  }
  expect(spy.sent.map((m) => m.subject)).toEqual([
    "Pending: first line second line third fourth fifth",
  ]);
});

test("with no notify carrier the email step is skipped and the run completes", async () => {
  const org = "soar-ac-003-unconfigured";
  const agent = globalThis.crypto.randomUUID();
  await enrollTestAgent(config, agent, org);
  await seedOfficeSpawn(config, org, agent);
  await runDetectionCycle(detectConfig(config, org), undefined, harness.soar);

  const [incident] = await getIncidents(config, { agentId: agent });
  const [execution] = await getExecutions(config, incident?.incident_id ?? "");
  expect(execution?.status).toBe("completed");
  expect(execution?.steps[1]).toMatchObject({
    step_id: "notify",
    outcome: "skipped",
    detail: "notification is not configured",
  });
});
