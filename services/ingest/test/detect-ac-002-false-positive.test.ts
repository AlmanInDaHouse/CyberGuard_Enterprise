import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { runDetectionCycle } from "../src/detect/index.js";
import { getAlerts, getWatermark, insertCgesEvent } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";

// SPEC-006 detect_ac_002 — SC010 false-positive: a script host whose parent is
// NOT an Office app (explorer.exe -> powershell.exe, a routine admin action)
// produces ZERO alerts. Synthetic events; CI-able (Linux + testcontainers).
//
// Two-part assertion on purpose: (a) the cycle PROCESSED the events (it ran),
// AND (b) zero alerts. Without (a), "0 alerts" would pass vacuously when the
// slice is unimplemented; (a) forces the cycle to actually run, keeping this a
// real RED now and a real GREEN only when the slice runs and emits nothing.
//
// Harness-first RED: insertCgesEvent succeeds (cges_events exists), then
// runDetectionCycle throws NotImplementedError — the alert helpers are never
// reached. Fails by NotImplemented, not "alerts table missing".

let config: Config;

beforeAll(() => {
  config = inject("ingestConfig");
});

test("detect_ac_002: explorer.exe -> powershell.exe yields 0 alerts (FP), cycle ran", async () => {
  const agentId = "01934abc-def0-7000-89ab-000000000220";
  const parentPid = 5000;

  // Parent: explorer.exe (NOT an Office application).
  await insertCgesEvent(config, {
    agentId,
    orgId: "detect-ac-002",
    eventId: "01934abc-def0-4000-89ab-000000000221",
    activityId: 1,
    processPid: parentPid,
    processName: "explorer.exe",
    imageFileName: "C:\\Windows\\explorer.exe",
    time: "2026-05-31 10:00:00.000000000",
  });
  // Child: powershell.exe spawned by explorer (benign).
  await insertCgesEvent(config, {
    agentId,
    orgId: "detect-ac-002",
    eventId: "01934abc-def0-4000-89ab-000000000222",
    activityId: 1,
    processPid: 5001,
    processName: "powershell.exe",
    imageFileName: "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
    processParentPid: parentPid,
    time: "2026-05-31 10:00:01.000000000",
  });

  const result = await runDetectionCycle(detectConfig(config, "detect-ac-002"));

  // (a) the cycle processed the batch ...
  expect(result.eventsEvaluated).toBeGreaterThan(0);
  expect(await getWatermark(config, "detect-ac-002")).not.toBeNull();
  // (b) ... and produced no alert (parent is not an Office app).
  expect(await getAlerts(config, { agentId })).toHaveLength(0);
});
