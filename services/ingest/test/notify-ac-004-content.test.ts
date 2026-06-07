import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { runDetectionCycle } from "../src/detect/index.js";
import { enrollTestAgent, getIncidents, insertCgesEvent } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";
import { spyNotify } from "./helpers/notify.js";

// SPEC-014 notify_ac_004 / SC-NTF-004 — the composed email content carries the
// incident_id, the severity, and the deterministic title (SPEC-014 §Data contracts
// §2). Asserted on the message captured by the in-memory fake transport. CI-able.

let config: Config;
beforeAll(() => {
  config = inject("ingestConfig");
});

const WINWORD = "C:\\Program Files\\Microsoft Office\\root\\Office16\\winword.exe";
const POWERSHELL = "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";

test("notify_ac_004: the email content carries incident_id, severity, and title", async () => {
  const agentId = "01934abc-def0-7000-89ab-000000000053";
  const orgId = "notify-ac-004";
  await enrollTestAgent(config, agentId);

  await insertCgesEvent(config, {
    agentId,
    orgId,
    eventId: "01934abc-def0-4000-89ab-000000000531",
    activityId: 1,
    processPid: 7080,
    processName: "winword.exe",
    imageFileName: WINWORD,
    time: "2026-05-31 12:00:00.000000000",
  });
  await insertCgesEvent(config, {
    agentId,
    orgId,
    eventId: "01934abc-def0-4000-89ab-000000000532",
    activityId: 1,
    processPid: 7081,
    processName: "powershell.exe",
    imageFileName: POWERSHELL,
    processParentPid: 7080,
    time: "2026-05-31 12:00:01.000000000",
  });

  const { notify, sent } = spyNotify();
  await runDetectionCycle(detectConfig(config, orgId), notify);

  const incident = (await getIncidents(config, { agentId }))[0];
  expect(incident).toBeDefined();
  expect(sent).toHaveLength(1);
  const msg = sent[0];

  // The rule's severity_id is 4 (OCSF High); the deterministic title is
  // "<canonical-tactics> activity on <agent>" = "execution,initial-access activity on …".
  const expectedTitle = `execution,initial-access activity on ${agentId}`;
  expect(msg?.subject).toContain("New incident (severity 4)");
  expect(msg?.subject).toContain(expectedTitle);
  expect(msg?.text).toContain(`Incident: ${incident?.incident_id}`);
  expect(msg?.text).toContain("Severity: 4");
  expect(msg?.text).toContain(expectedTitle);
  expect(msg?.text).toContain(`Org:      ${orgId}`);
});
