import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { runDetectionCycle } from "../src/detect/index.js";
import { enrollTestAgent, getAlerts, insertCgesEvent } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";

// SPEC-006 detect_ac_003 — dedup: five matching Office->script-host spawns in
// the same (agent_id, rule, process_name, 5-min bucket) produce exactly ONE
// alert (INSERT ... ON CONFLICT (dedup_key) DO NOTHING, ADR-0012 §5/§6). Also
// positively exercises rule-match + alert emission in CI. CI-able.
//
// Harness-first RED: inserts succeed, runDetectionCycle throws NotImplemented.

let config: Config;

beforeAll(() => {
  config = inject("ingestConfig");
});

const WINWORD = "C:\\Program Files\\Microsoft Office\\root\\Office16\\winword.exe";
const POWERSHELL = "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";

test("detect_ac_003: 5 winword->powershell spawns in one 5-min bucket dedupe to 1 alert", async () => {
  const agentId = "01934abc-def0-7000-89ab-000000000330";

  // Five parent(winword)->child(powershell) pairs, all inside one 5-minute
  // bucket (11:00:00..11:04:59 → floor(unix/300) identical).
  for (let i = 0; i < 5; i++) {
    const parentPid = 6000 + i * 2;
    const childPid = 6001 + i * 2;
    const seq = 3400 + i * 2;
    await insertCgesEvent(config, {
      agentId,
      orgId: "detect-ac-003",
      eventId: `01934abc-def0-4000-89ab-${String(seq).padStart(12, "0")}`,
      activityId: 1,
      processPid: parentPid,
      processName: "winword.exe",
      imageFileName: WINWORD,
      time: `2026-05-31 11:0${i}:00.000000000`,
    });
    await insertCgesEvent(config, {
      agentId,
      orgId: "detect-ac-003",
      eventId: `01934abc-def0-4000-89ab-${String(seq + 1).padStart(12, "0")}`,
      activityId: 1,
      processPid: childPid,
      processName: "powershell.exe",
      imageFileName: POWERSHELL,
      processParentPid: parentPid,
      time: `2026-05-31 11:0${i}:01.000000000`,
    });
  }

  await enrollTestAgent(config, agentId);
  await runDetectionCycle(detectConfig(config, "detect-ac-003"));

  const alerts = await getAlerts(config, { agentId });
  expect(alerts).toHaveLength(1);
  expect(alerts[0]?.rule_id).toBe("rule.office_spawns_script_host");
  expect(alerts[0]?.cg_detection_source).toBe("rule");
});
