import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { runDetectionCycle } from "../src/detect/index.js";
import { enrollTestAgent, getIncidents, insertCgesEvent } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";
import { spyNotify } from "./helpers/notify.js";

// SPEC-014 notify_ac_001 / SC-NTF-001 — a newly CREATED incident triggers exactly
// one best-effort email carrying the incident's id, severity, and title. Driven
// through the synthetic-event → runDetectionCycle path (the detect_ac pattern), with
// an in-memory fake transport (no network, no real mail). The notify rides the
// caller (runDetectionCycle), AFTER the upsert commits (ADR-0017 §2). CI-able.

let config: Config;
beforeAll(() => {
  config = inject("ingestConfig");
});

const WINWORD = "C:\\Program Files\\Microsoft Office\\root\\Office16\\winword.exe";
const POWERSHELL = "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";

test("notify_ac_001: a newly created incident sends one notification with its fields", async () => {
  const agentId = "01934abc-def0-7000-89ab-000000000054";
  const orgId = "notify-ac-001";
  await enrollTestAgent(config, agentId);

  await insertCgesEvent(config, {
    agentId,
    orgId,
    eventId: "01934abc-def0-4000-89ab-000000000501",
    activityId: 1,
    processPid: 7050,
    processName: "winword.exe",
    imageFileName: WINWORD,
    time: "2026-05-31 12:00:00.000000000",
  });
  await insertCgesEvent(config, {
    agentId,
    orgId,
    eventId: "01934abc-def0-4000-89ab-000000000502",
    activityId: 1,
    processPid: 7051,
    processName: "powershell.exe",
    imageFileName: POWERSHELL,
    processParentPid: 7050,
    time: "2026-05-31 12:00:01.000000000",
  });

  const { notify, sent } = spyNotify();
  await runDetectionCycle(detectConfig(config, orgId), notify);

  const incident = (await getIncidents(config, { agentId }))[0];
  expect(incident).toBeDefined();

  expect(sent).toHaveLength(1);
  const msg = sent[0];
  expect(msg?.to).toBe("soc-team@cyberguard.test");
  expect(msg?.from).toBe("soc-noreply@cyberguard.test");
  // The email carries the incident's id, its severity (rule severity_id = 4), and
  // the deterministic title.
  expect(msg?.text).toContain(incident?.incident_id ?? "MISSING");
  expect(msg?.text).toContain(`Severity: ${incident?.severity_id}`);
  expect(msg?.text).toContain(`activity on ${agentId}`);
});
