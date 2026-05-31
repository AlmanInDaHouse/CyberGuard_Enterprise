import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { advanceWatermark, getWatermark, readNewEvents } from "../src/detect/read-model.js";
import { insertCgesEvent } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";

// SPEC-006 5b — read-model unit/integration gate. Exercises readNewEvents +
// getWatermark + advanceWatermark directly against synthetic cges_events. This
// is 5b's own verifiable GREEN: it passes in CI even while detect_ac_002..006
// stay RED (NotImplemented), because the detect_ac go through runDetectionCycle
// (5e) — 5b does NOT clear the Known CI debt. Also exercises migration 0003
// (detect_watermark table + epoch init). Each test uses a distinct org_id so it
// only sees its own events in the shared (singleFork) ClickHouse.

let config: Config;

const PS = "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";
const WINWORD = "C:\\Program Files\\Microsoft Office\\root\\Office16\\winword.exe";

let seq = 0;
function ev(): string {
  seq += 1;
  return `01934abc-def0-4000-89ab-${String(seq).padStart(12, "0")}`;
}

beforeAll(() => {
  config = inject("ingestConfig");
});

test("projects verbatim cges_events columns into a NormalizedProcessEvent", async () => {
  const org = "rm-cols";
  const agentId = "01934abc-def0-7000-89ab-0000000000c1";
  const eventId = ev();
  await insertCgesEvent(config, {
    agentId,
    eventId,
    orgId: org,
    activityId: 1,
    processPid: 9001,
    processUid: "uid-9001",
    processName: "powershell.exe",
    imageFileName: PS,
    processParentPid: 9000,
    time: "2026-05-31 14:00:00.000000000",
  });

  const cfg = detectConfig(config, org);
  const events = await readNewEvents(cfg, await getWatermark(cfg), 100);
  const e = events.find((x) => x.eventId === eventId);

  expect(e).toBeDefined();
  expect(e?.agentId).toBe(agentId);
  expect(e?.pid).toBe(9001);
  expect(e?.uid).toBe("uid-9001");
  expect(e?.processName).toBe("powershell.exe");
  expect(e?.imageFileName).toBe(PS);
  expect(e?.parentPid).toBe(9000);
  expect(e?.time).toBe("2026-05-31 14:00:00.000000000");
  expect(e?.parentImage).toBeNull(); // parent pid 9000 was not captured
});

test("FINAL collapses at-least-once duplicate rows (same org/time/event_id)", async () => {
  const org = "rm-final";
  const eventId = ev();
  const row = {
    agentId: "01934abc-def0-7000-89ab-0000000000c2",
    eventId,
    orgId: org,
    activityId: 1,
    processPid: 9100,
    processName: "cmd.exe",
    imageFileName: "C:\\Windows\\System32\\cmd.exe",
    time: "2026-05-31 14:05:00.000000000",
  };
  await insertCgesEvent(config, row);
  await insertCgesEvent(config, row); // duplicate: same ORDER BY key (org, time, event_id)

  const cfg = detectConfig(config, org);
  const events = await readNewEvents(cfg, await getWatermark(cfg), 100);
  expect(events.filter((x) => x.eventId === eventId)).toHaveLength(1);
});

test("watermark: epoch init, read batch A, advance, next poll reads only batch B", async () => {
  const org = "rm-wm";
  const agentId = "01934abc-def0-7000-89ab-0000000000c3";
  const cfg = detectConfig(config, org);

  // Fresh org → no detect_watermark row → epoch default (migration 0003 init).
  expect(await getWatermark(cfg)).toBe("1970-01-01 00:00:00.000000000");

  await insertCgesEvent(config, {
    agentId,
    eventId: ev(),
    orgId: org,
    activityId: 1,
    processPid: 9201,
    processName: "winword.exe",
    imageFileName: WINWORD,
    time: "2026-05-31 15:00:00.000000000",
  });
  await insertCgesEvent(config, {
    agentId,
    eventId: ev(),
    orgId: org,
    activityId: 1,
    processPid: 9202,
    processName: "powershell.exe",
    imageFileName: PS,
    processParentPid: 9201,
    time: "2026-05-31 15:00:01.000000000",
  });

  const batchA = await readNewEvents(cfg, await getWatermark(cfg), 100);
  expect(batchA).toHaveLength(2);
  const maxA = batchA.map((e) => e.time).reduce((a, b) => (a > b ? a : b));
  await advanceWatermark(cfg, maxA);
  expect(await getWatermark(cfg)).toBe(maxA);

  await insertCgesEvent(config, {
    agentId,
    eventId: ev(),
    orgId: org,
    activityId: 1,
    processPid: 9203,
    processName: "winword.exe",
    imageFileName: WINWORD,
    time: "2026-05-31 15:10:00.000000000",
  });
  await insertCgesEvent(config, {
    agentId,
    eventId: ev(),
    orgId: org,
    activityId: 1,
    processPid: 9204,
    processName: "powershell.exe",
    imageFileName: PS,
    processParentPid: 9203,
    time: "2026-05-31 15:10:01.000000000",
  });

  const batchB = await readNewEvents(cfg, await getWatermark(cfg), 100);
  expect(batchB).toHaveLength(2);
  expect(batchB.every((e) => e.time > maxA)).toBe(true);
});

test("parent-pid self-join: captured parent resolves; absent parent yields null", async () => {
  const org = "rm-join";
  const agentId = "01934abc-def0-7000-89ab-0000000000c4";
  const cfg = detectConfig(config, org);

  // Parent winword (pid 9300) captured; child powershell whose parent is 9300.
  await insertCgesEvent(config, {
    agentId,
    eventId: ev(),
    orgId: org,
    activityId: 1,
    processPid: 9300,
    processName: "winword.exe",
    imageFileName: WINWORD,
    time: "2026-05-31 16:00:00.000000000",
  });
  const childWithParent = ev();
  await insertCgesEvent(config, {
    agentId,
    eventId: childWithParent,
    orgId: org,
    activityId: 1,
    processPid: 9301,
    processName: "powershell.exe",
    imageFileName: PS,
    processParentPid: 9300,
    time: "2026-05-31 16:00:01.000000000",
  });
  // Child powershell whose parent (9999) was NOT captured → parentImage null.
  const orphan = ev();
  await insertCgesEvent(config, {
    agentId,
    eventId: orphan,
    orgId: org,
    activityId: 1,
    processPid: 9302,
    processName: "powershell.exe",
    imageFileName: PS,
    processParentPid: 9999,
    time: "2026-05-31 16:00:02.000000000",
  });

  const events = await readNewEvents(cfg, await getWatermark(cfg), 100);
  expect(events.find((e) => e.eventId === childWithParent)?.parentImage).toBe(WINWORD);
  expect(events.find((e) => e.eventId === orphan)?.parentImage).toBeNull();
});
