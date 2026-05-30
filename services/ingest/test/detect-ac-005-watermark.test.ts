import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { runDetectionCycle } from "../src/detect/index.js";
import { insertCgesEvent } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";

// SPEC-006 detect_ac_005 — read-model watermark: batch A is not re-evaluated
// when batch B arrives. The second cycle evaluates ONLY batch B's events (batch
// A is behind the advanced watermark). Asserting eventsEvaluated (not alert
// count) isolates the watermark from the dedup path, which would otherwise mask
// re-processing. CI-able.
//
// Harness-first RED: the first runDetectionCycle throws NotImplemented.

let config: Config;

beforeAll(() => {
  config = inject("ingestConfig");
});

const WINWORD = "C:\\Program Files\\Microsoft Office\\root\\Office16\\winword.exe";
const POWERSHELL = "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";

async function insertOfficeSpawn(
  agentId: string,
  parentPid: number,
  childPid: number,
  seq: number,
  clock: string,
): Promise<void> {
  await insertCgesEvent(config, {
    agentId,
    eventId: `01934abc-def0-4000-89ab-${String(seq).padStart(12, "0")}`,
    activityId: 1,
    processPid: parentPid,
    processName: "winword.exe",
    imageFileName: WINWORD,
    time: `2026-05-31 ${clock}:00.000000000`,
  });
  await insertCgesEvent(config, {
    agentId,
    eventId: `01934abc-def0-4000-89ab-${String(seq + 1).padStart(12, "0")}`,
    activityId: 1,
    processPid: childPid,
    processName: "powershell.exe",
    imageFileName: POWERSHELL,
    processParentPid: parentPid,
    time: `2026-05-31 ${clock}:01.000000000`,
  });
}

test("detect_ac_005: watermark advances; the 2nd cycle evaluates only batch B", async () => {
  const agentId = "01934abc-def0-7000-89ab-000000000550";

  // Batch A — bucket at 13:00.
  await insertOfficeSpawn(agentId, 8000, 8001, 5500, "13:00");
  const first = await runDetectionCycle(detectConfig(config));
  expect(first.eventsEvaluated).toBe(2); // both Launch events in batch A

  // Batch B — a later bucket at 13:10.
  await insertOfficeSpawn(agentId, 8002, 8003, 5502, "13:10");
  const second = await runDetectionCycle(detectConfig(config));
  // Only batch B's 2 events are evaluated; batch A is behind the watermark.
  expect(second.eventsEvaluated).toBe(2);
});
