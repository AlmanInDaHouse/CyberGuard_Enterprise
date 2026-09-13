import pg from "pg";
import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { buildDetectConfig, listEnrolledOrgs, startDetectionDriver } from "../src/detect/driver.js";
import { runDetectionCycle } from "../src/detect/index.js";
import type { DetectCycleResult } from "../src/detect/types.js";
import { enrollTestAgent, getAlerts, getWatermark, insertCgesEvent } from "./helpers/db.js";

// ADR-0012 Amendment 2026-06-07 — production detection driver. CI-able
// (testcontainers, NO ETW): synthetic cges_events drive the REAL runDetectionCycle
// through the driver (a), while injected listOrgs/runCycle fakes exercise the
// scheduler's single-flight (b), multi-org loop (c) and clean stop (d) without a
// real cycle. Same mould as detect-ac-005 (asserts on watermark / eventsEvaluated),
// NOT the elevated detect-ac-001 marquee.

const WINWORD = "C:\\Program Files\\Microsoft Office\\root\\Office16\\winword.exe";
const POWERSHELL = "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";
const EMPTY_CYCLE: DetectCycleResult = {
  processedThrough: null,
  eventsEvaluated: 0,
  alertsWritten: 0,
};

const delay = (ms: number): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms));

/** Poll `pred` until it is truthy or `timeoutMs` elapses; returns whether it became truthy. */
async function waitUntil(
  pred: () => Promise<boolean> | boolean,
  timeoutMs = 15_000,
  stepMs = 100,
): Promise<boolean> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    if (await pred()) return true;
    await delay(stepMs);
  }
  return false;
}

/** Insert a winword.exe parent + powershell.exe child Launch (the office_spawns rule). */
async function insertOfficeSpawn(
  config: Config,
  orgId: string,
  agentId: string,
  parentPid: number,
  childPid: number,
  seq: number,
  clock: string,
): Promise<void> {
  await insertCgesEvent(config, {
    agentId,
    orgId,
    eventId: `01934abc-def0-4000-89ab-${String(seq).padStart(12, "0")}`,
    activityId: 1,
    processPid: parentPid,
    processName: "winword.exe",
    imageFileName: WINWORD,
    time: `2026-06-07 ${clock}:00.000000000`,
  });
  await insertCgesEvent(config, {
    agentId,
    orgId,
    eventId: `01934abc-def0-4000-89ab-${String(seq + 1).padStart(12, "0")}`,
    activityId: 1,
    processPid: childPid,
    processName: "powershell.exe",
    imageFileName: POWERSHELL,
    processParentPid: parentPid,
    time: `2026-06-07 ${clock}:01.000000000`,
  });
}

let config: Config;

beforeAll(() => {
  config = inject("ingestConfig");
});

// (a) A self-rescheduled tick runs the REAL runDetectionCycle and advances the watermark.
test("driver tick: a pass runs runDetectionCycle, persists the alert and advances the watermark", async () => {
  const orgId = "drv-tick";
  const agentId = "01934abc-def0-7000-89ab-00000000da01";
  await insertOfficeSpawn(config, orgId, agentId, 9000, 9001, 901_000, "08:00");
  await enrollTestAgent(config, agentId);

  const driver = startDetectionDriver({
    intervalMs: 30,
    listOrgs: async () => [orgId],
    runCycle: (org) => runDetectionCycle(buildDetectConfig(config, org)),
  });
  try {
    const gotAlert = await waitUntil(
      async () => (await getAlerts(config, { agentId })).length >= 1,
    );
    expect(gotAlert).toBe(true);
  } finally {
    await driver.stop();
  }

  // The cursor advanced off the epoch default (the read-model processed forward).
  expect(await getWatermark(config, orgId)).not.toBeNull();
  const alerts = await getAlerts(config, { agentId });
  expect(alerts).toHaveLength(1);
  expect(alerts[0]?.rule_id).toBe("rule.office_spawns_script_host");
});

// (b) Single-flight: a cycle that runs longer than the interval never overlaps the next tick.
test("single-flight: a cycle longer than the interval never overlaps the next tick", async () => {
  let concurrent = 0;
  let maxConcurrent = 0;
  let calls = 0;
  const driver = startDetectionDriver({
    intervalMs: 20,
    listOrgs: async () => ["single-flight-org"],
    runCycle: async () => {
      concurrent += 1;
      maxConcurrent = Math.max(maxConcurrent, concurrent);
      calls += 1;
      await delay(120); // far longer than the 20 ms interval
      concurrent -= 1;
      return EMPTY_CYCLE;
    },
  });
  await delay(450);
  await driver.stop();

  expect(maxConcurrent).toBe(1); // never two cycles in flight at once
  expect(calls).toBeGreaterThanOrEqual(2); // it kept ticking — just strictly sequentially
});

// (c) Multi-org: the real enumeration returns DISTINCT agents.org_id, and the driver
// drives a cycle for each org listOrgs yields.
test("multi-org: listEnrolledOrgs returns DISTINCT agents.org_id and the driver processes each", async () => {
  await enrollTestAgent(config, "01934abc-def0-7000-89ab-00000000dc01", "drv-org-a");
  await enrollTestAgent(config, "01934abc-def0-7000-89ab-00000000dc02", "drv-org-b");

  const pool = new pg.Pool({ connectionString: config.INGEST_PG_URL });
  try {
    const orgs = await listEnrolledOrgs(pool);
    expect(orgs).toContain("drv-org-a");
    expect(orgs).toContain("drv-org-b");
  } finally {
    await pool.end();
  }

  const seen = new Set<string>();
  const driver = startDetectionDriver({
    intervalMs: 20,
    listOrgs: async () => ["drv-org-a", "drv-org-b"],
    runCycle: async (orgId) => {
      seen.add(orgId);
      return EMPTY_CYCLE;
    },
  });
  try {
    const both = await waitUntil(() => seen.has("drv-org-a") && seen.has("drv-org-b"), 5_000);
    expect(both).toBe(true);
  } finally {
    await driver.stop();
  }
});

// (d) close(): stop() halts the scheduler — no cycle runs after it resolves.
test("stop(): the scheduler halts — no cycle runs after stop() resolves", async () => {
  let calls = 0;
  const driver = startDetectionDriver({
    intervalMs: 25,
    listOrgs: async () => ["stop-org"],
    runCycle: async () => {
      calls += 1;
      return EMPTY_CYCLE;
    },
  });
  await waitUntil(() => calls >= 2, 5_000);
  await driver.stop();

  const frozen = calls;
  await delay(150); // several intervals' worth of would-be ticks
  expect(calls).toBe(frozen); // none fired after stop() resolved
  expect(frozen).toBeGreaterThanOrEqual(2);
});

// (e) Off-switch: INGEST_DETECT_INTERVAL_MS=0 ⇒ driver disabled — no ticks ever,
// stop() is a no-op. This is what the detect-ac-001 marquee relies on.
test("off-switch: intervalMs 0 schedules no ticks and stop() is a no-op", async () => {
  let calls = 0;
  const driver = startDetectionDriver({
    intervalMs: 0,
    listOrgs: async () => ["off-org"],
    runCycle: async () => {
      calls += 1;
      return EMPTY_CYCLE;
    },
  });
  await delay(120); // several would-be intervals at any sane tick rate
  expect(calls).toBe(0); // the driver never ticked
  await driver.stop(); // no-op, resolves cleanly
  expect(calls).toBe(0);
});
