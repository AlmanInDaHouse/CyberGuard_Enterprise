import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { runDetectionCycle } from "../src/detect/index.js";
import { enrollTestAgent, getIncidents, insertCgesEvent } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";
import { spyNotify } from "./helpers/notify.js";

// SPEC-014 notify_ac_002 / SC-NTF-002 — a correlated alert that UPDATES an existing
// incident (same grouping_key, ON CONFLICT DO UPDATE) does NOT re-notify; only the
// CREATE notifies. Two pairs in the SAME 30-min grouping window but DIFFERENT 5-min
// dedup buckets ⇒ two distinct alerts → one incident (create + update), one cycle →
// notify exactly once. Exercises the xmax create-vs-update seam. CI-able.

let config: Config;
beforeAll(() => {
  config = inject("ingestConfig");
});

const WINWORD = "C:\\Program Files\\Microsoft Office\\root\\Office16\\winword.exe";
const POWERSHELL = "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";

test("notify_ac_002: a correlated update of an existing incident does NOT re-notify", async () => {
  const agentId = "01934abc-def0-7000-89ab-000000000051";
  const orgId = "notify-ac-002";
  await enrollTestAgent(config, agentId);

  // Pair 1 @ 12:00 — creates the incident (notify #1).
  await insertCgesEvent(config, {
    agentId,
    orgId,
    eventId: "01934abc-def0-4000-89ab-000000000511",
    activityId: 1,
    processPid: 7060,
    processName: "winword.exe",
    imageFileName: WINWORD,
    time: "2026-05-31 12:00:00.000000000",
  });
  await insertCgesEvent(config, {
    agentId,
    orgId,
    eventId: "01934abc-def0-4000-89ab-000000000512",
    activityId: 1,
    processPid: 7061,
    processName: "powershell.exe",
    imageFileName: POWERSHELL,
    processParentPid: 7060,
    time: "2026-05-31 12:00:01.000000000",
  });

  // Pair 2 @ 12:06 — a distinct alert (different 5-min dedup bucket) in the SAME
  // 30-min grouping window ⇒ a correlated DO UPDATE of the same incident (NO notify).
  await insertCgesEvent(config, {
    agentId,
    orgId,
    eventId: "01934abc-def0-4000-89ab-000000000513",
    activityId: 1,
    processPid: 7062,
    processName: "winword.exe",
    imageFileName: WINWORD,
    time: "2026-05-31 12:06:00.000000000",
  });
  await insertCgesEvent(config, {
    agentId,
    orgId,
    eventId: "01934abc-def0-4000-89ab-000000000514",
    activityId: 1,
    processPid: 7063,
    processName: "powershell.exe",
    imageFileName: POWERSHELL,
    processParentPid: 7062,
    time: "2026-05-31 12:06:01.000000000",
  });

  const { notify, sent } = spyNotify();
  await runDetectionCycle(detectConfig(config, orgId), notify);

  const incidents = await getIncidents(config, { agentId });
  // One incident, two correlated alerts grouped — but notified only on the create.
  expect(incidents).toHaveLength(1);
  expect(incidents[0]?.alert_ids).toHaveLength(2);
  expect(sent).toHaveLength(1);
});
