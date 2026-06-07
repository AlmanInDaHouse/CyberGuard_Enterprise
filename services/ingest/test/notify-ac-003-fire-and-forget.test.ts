import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { runDetectionCycle } from "../src/detect/index.js";
import { enrollTestAgent, getIncidents, insertCgesEvent } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";
import { spyNotify } from "./helpers/notify.js";

// SPEC-014 notify_ac_003 / SC-NTF-003 — fire-and-forget isolation (ADR-0017 §2): a
// transport that THROWS on send must NOT abort the detection cycle. The incident
// still persists, runDetectionCycle resolves successfully, and the failure is
// logged (not propagated). Proves a flaky SMTP cannot take down the pipeline —
// the persisted incident is the source of truth, the email is best-effort. CI-able.

let config: Config;
beforeAll(() => {
  config = inject("ingestConfig");
});

const WINWORD = "C:\\Program Files\\Microsoft Office\\root\\Office16\\winword.exe";
const POWERSHELL = "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";

test("notify_ac_003: a throwing transport does not abort the cycle; incident persists; failure logged", async () => {
  const agentId = "01934abc-def0-7000-89ab-000000000052";
  const orgId = "notify-ac-003";
  await enrollTestAgent(config, agentId);

  await insertCgesEvent(config, {
    agentId,
    orgId,
    eventId: "01934abc-def0-4000-89ab-000000000521",
    activityId: 1,
    processPid: 7070,
    processName: "winword.exe",
    imageFileName: WINWORD,
    time: "2026-05-31 12:00:00.000000000",
  });
  await insertCgesEvent(config, {
    agentId,
    orgId,
    eventId: "01934abc-def0-4000-89ab-000000000522",
    activityId: 1,
    processPid: 7071,
    processName: "powershell.exe",
    imageFileName: POWERSHELL,
    processParentPid: 7070,
    time: "2026-05-31 12:00:01.000000000",
  });

  const { notify, sent, logs } = spyNotify({ throwOnSend: true });

  // The cycle MUST resolve, not reject — the notify failure is swallowed.
  const result = await runDetectionCycle(detectConfig(config, orgId), notify);
  expect(result.alertsWritten).toBe(1);

  // The send threw, so nothing was captured …
  expect(sent).toHaveLength(0);
  // … but the incident persisted (the upsert committed before notify) …
  const incidents = await getIncidents(config, { agentId });
  expect(incidents).toHaveLength(1);
  // … and the failure was logged to the sink, never propagated.
  expect(logs).toHaveLength(1);
  expect(logs[0]?.msg).toContain("notification send failed");
});
