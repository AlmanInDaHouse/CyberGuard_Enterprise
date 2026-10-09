import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { advanceCursor, getCursor, readNewEvents } from "../src/detect/read-model.js";
import { insertCgesEvent } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";

// SPEC-006 5b — read-model unit/integration gate. Exercises readNewEvents +
// getCursor + advanceCursor (SPEC-018 arrival cursor) directly against synthetic
// cges_events, with the settle margin at 0 (helpers/detect.ts). This
// is 5b's own verifiable GREEN: it passes in CI even while detect_ac_002..006
// stay RED (NotImplemented), because the detect_ac go through runDetectionCycle
// (5e) — 5b does NOT clear the Known CI debt. Also exercises migrations 0003 +
// 0007 (detect_watermark table + cursor start). Each test uses a distinct org_id so it
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
  const { events } = await readNewEvents(cfg, await getCursor(cfg), 100);
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

test("no FINAL: a resent row arrives after the cursor and is read again (SPEC-018 §Operational §3)", async () => {
  const org = "rm-resend";
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
  const cfg = detectConfig(config, org);
  await insertCgesEvent(config, row);
  const first = await readNewEvents(cfg, await getCursor(cfg), 100);
  expect(first.events.map((x) => x.eventId)).toEqual([eventId]);
  if (first.cursor === null) throw new Error("the first read returned no row");
  await advanceCursor(cfg, first.cursor);

  // The resend: the same (org, time, event_id), a later arrived_at. A merge may
  // collapse the two rows at any time; the survivor is the later one (the
  // ReplacingMergeTree version is arrived_at), so the read sees it either way.
  await insertCgesEvent(config, row);
  const second = await readNewEvents(cfg, await getCursor(cfg), 100);
  expect(second.events.map((x) => x.eventId)).toEqual([eventId]);
  if (second.cursor === null) throw new Error("the second read returned no row");
  expect(second.cursor.arrivedAt > first.cursor.arrivedAt).toBe(true);
});

test("cursor: starts at the beginning, read batch A, advance, next poll reads only batch B", async () => {
  const org = "rm-wm";
  const agentId = "01934abc-def0-7000-89ab-0000000000c3";
  const cfg = detectConfig(config, org);

  // Fresh org → no detect_watermark row → the cursor's start (the 0007 defaults).
  expect(await getCursor(cfg)).toEqual({
    arrivedAt: "1970-01-01 00:00:00.000",
    eventId: "00000000-0000-0000-0000-000000000000",
  });

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

  const batchA = await readNewEvents(cfg, await getCursor(cfg), 100);
  expect(batchA.events).toHaveLength(2);
  if (batchA.cursor === null) throw new Error("batch A returned no row");
  // The cursor is batch A's last row: its own event, in arrival order.
  expect(batchA.cursor.eventId).toBe(batchA.events[1]?.eventId);
  await advanceCursor(cfg, batchA.cursor);
  expect(await getCursor(cfg)).toEqual(batchA.cursor);

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

  const batchB = await readNewEvents(cfg, await getCursor(cfg), 100);
  expect(batchB.events).toHaveLength(2);
  const idsA = new Set(batchA.events.map((e) => e.eventId));
  expect(batchB.events.some((e) => idsA.has(e.eventId))).toBe(false);
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

  const { events } = await readNewEvents(cfg, await getCursor(cfg), 100);
  expect(events.find((e) => e.eventId === childWithParent)?.parentImage).toBe(WINWORD);
  expect(events.find((e) => e.eventId === orphan)?.parentImage).toBeNull();
});
