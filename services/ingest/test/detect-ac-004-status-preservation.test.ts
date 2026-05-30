import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { runDetectionCycle } from "../src/detect/index.js";
import { getAlerts, insertCgesEvent, setAlertStatus } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";

// SPEC-006 detect_ac_004 — status preservation: a re-fire over an alert a human
// already moved off 'new' must NOT reset it (INSERT ... ON CONFLICT DO NOTHING,
// not DO UPDATE; ADR-0012 §6). CI-able.
//
// Harness-first RED: the first runDetectionCycle throws NotImplemented, so
// setAlertStatus / getAlerts are never reached — fails by NotImplemented.

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

test("detect_ac_004: re-fire over an acknowledged alert keeps it acknowledged", async () => {
  const agentId = "01934abc-def0-7000-89ab-000000000440";

  await insertOfficeSpawn(agentId, 7000, 7001, 4400, "12:00");
  await runDetectionCycle(detectConfig(config));

  const [alert] = await getAlerts(config, { agentId });
  if (!alert) {
    throw new Error("detect_ac_004: expected exactly one alert after the first cycle");
  }
  await setAlertStatus(config, alert.alert_id, "acknowledged");

  // Re-fire inside the SAME 5-minute bucket (same dedup_key), new event ids.
  await insertOfficeSpawn(agentId, 7002, 7003, 4402, "12:01");
  await runDetectionCycle(detectConfig(config));

  const after = await getAlerts(config, { dedupKey: alert.dedup_key });
  expect(after).toHaveLength(1);
  expect(after[0]?.status).toBe("acknowledged");
});
