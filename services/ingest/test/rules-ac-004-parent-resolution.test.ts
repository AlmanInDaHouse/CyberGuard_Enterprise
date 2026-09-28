import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { getWatermark, readNewEvents } from "../src/detect/read-model.js";
import type { NormalizedProcessEvent } from "../src/detect/types.js";
import { insertCgesEvent } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";

// SPEC-016 rules_ac_004 — parent resolution per child (§Operational §1): the
// parent is the most recent Launch of the child's parent pid on the same agent,
// at most 24 h before the child, unless that process's own Terminate (matched by
// process_uid) precedes the child. Exercised on readNewEvents directly, each case
// in its own org so it sees only its own events.

const WINWORD = "C:\\Program Files\\Microsoft Office\\root\\Office16\\WINWORD.EXE";
const EXCEL = "C:\\Program Files\\Microsoft Office\\root\\Office16\\EXCEL.EXE";
const NOTEPAD = "C:\\Windows\\System32\\notepad.exe";
const PS = "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";
const CMD = "C:\\Windows\\System32\\cmd.exe";

let config: Config;
let seq = 0;

beforeAll(() => {
  config = inject("ingestConfig");
});

interface Proc {
  org: string;
  agent: string;
  pid: number;
  image: string;
  time: string;
  parentPid?: number;
  uid?: string;
  activity?: number;
}

/** Insert one Process Activity row (a Launch unless `activity` says otherwise); returns its event_id. */
async function insert(p: Proc): Promise<string> {
  seq += 1;
  const eventId = `01934abc-def0-4000-89ab-0000000a4${String(seq).padStart(3, "0")}`;
  await insertCgesEvent(config, {
    agentId: p.agent,
    orgId: p.org,
    eventId,
    activityId: p.activity ?? 1,
    processPid: p.pid,
    processUid: p.uid ?? "",
    processName: p.image.split("\\").pop() ?? p.image,
    imageFileName: p.image,
    processParentPid: p.parentPid ?? null,
    time: p.time,
  });
  return eventId;
}

/**
 * Read the org's batch after `watermark` (default: the org's stored watermark)
 * and return the event with `eventId`. A watermark past the parent's Launch puts
 * the parent in an earlier batch, as when an earlier cycle already read it.
 */
async function readEvent(
  org: string,
  eventId: string,
  watermark?: string,
): Promise<NormalizedProcessEvent | undefined> {
  const cfg = detectConfig(config, org);
  const events = await readNewEvents(cfg, watermark ?? (await getWatermark(cfg)), 1000);
  return events.find((e) => e.eventId === eventId);
}

test("a parent launched 2 h before its child resolves", async () => {
  const org = "rules-ac-004-2h";
  const agent = "01934abc-def0-7000-89ab-0000000a4001";
  await insert({ org, agent, pid: 7100, image: WINWORD, time: "2026-09-27 08:00:00.000000000" });
  const child = await insert({
    org,
    agent,
    pid: 7101,
    image: PS,
    parentPid: 7100,
    time: "2026-09-27 10:00:00.000000000",
  });

  // The parent was read by an earlier cycle; the batch holds the child only.
  const behindParent = "2026-09-27 08:00:00.000000000";
  expect((await readEvent(org, child, behindParent))?.parentImage).toBe(WINWORD);
});

test("a later reuse of the parent's pid, inside the same batch, does not replace it", async () => {
  const org = "rules-ac-004-reuse";
  const agent = "01934abc-def0-7000-89ab-0000000a4002";
  await insert({ org, agent, pid: 7200, image: WINWORD, time: "2026-09-27 10:00:00.000000000" });
  const child = await insert({
    org,
    agent,
    pid: 7201,
    image: PS,
    parentPid: 7200,
    time: "2026-09-27 10:00:01.000000000",
  });
  // pid 7200 is reused by a later process in the same batch, which has a child of its own.
  await insert({
    org,
    agent,
    pid: 7200,
    image: NOTEPAD,
    parentPid: 7300,
    time: "2026-09-27 10:00:05.000000000",
  });
  const laterChild = await insert({
    org,
    agent,
    pid: 7202,
    image: CMD,
    parentPid: 7200,
    time: "2026-09-27 10:00:06.000000000",
  });

  expect((await readEvent(org, child))?.parentImage).toBe(WINWORD);
  expect((await readEvent(org, laterChild))?.parentImage).toBe(NOTEPAD);
});

test("a candidate whose Terminate precedes the child is not used", async () => {
  const org = "rules-ac-004-terminated";
  const agent = "01934abc-def0-7000-89ab-0000000a4003";
  const t = (s: string) => `2026-09-27 10:00:${s}.000000000`;

  // WINWORD (uid u-7300) launches, spawns a child, then terminates; a later child of pid 7300.
  await insert({ org, agent, pid: 7300, uid: "u-7300", image: WINWORD, time: t("00") });
  const beforeExit = await insert({
    org,
    agent,
    pid: 7301,
    image: PS,
    parentPid: 7300,
    time: t("05"),
  });
  await insert({
    org,
    agent,
    pid: 7300,
    uid: "u-7300",
    image: WINWORD,
    activity: 2,
    time: t("10"),
  });
  const afterExit = await insert({
    org,
    agent,
    pid: 7302,
    image: PS,
    parentPid: 7300,
    time: t("20"),
  });

  // Control: a Terminate with another process_uid excludes nothing.
  await insert({ org, agent, pid: 7310, uid: "u-7310", image: EXCEL, time: t("00") });
  await insert({ org, agent, pid: 7310, uid: "u-other", image: EXCEL, activity: 2, time: t("10") });
  const otherUid = await insert({
    org,
    agent,
    pid: 7311,
    image: PS,
    parentPid: 7310,
    time: t("20"),
  });

  // Control: an empty process_uid matches no Terminate.
  await insert({ org, agent, pid: 7320, uid: "", image: EXCEL, time: t("00") });
  await insert({ org, agent, pid: 7320, uid: "", image: EXCEL, activity: 2, time: t("10") });
  const emptyUid = await insert({
    org,
    agent,
    pid: 7321,
    image: PS,
    parentPid: 7320,
    time: t("20"),
  });

  expect((await readEvent(org, beforeExit))?.parentImage).toBe(WINWORD);
  expect((await readEvent(org, afterExit))?.parentImage).toBeNull();
  expect((await readEvent(org, otherUid))?.parentImage).toBe(EXCEL);
  expect((await readEvent(org, emptyUid))?.parentImage).toBe(EXCEL);
});

test("a parent launched more than 24 h before its child resolves to null", async () => {
  const org = "rules-ac-004-24h";
  const agent = "01934abc-def0-7000-89ab-0000000a4004";
  // 24 h + 1 s before the child: outside the look-back.
  await insert({ org, agent, pid: 7400, image: WINWORD, time: "2026-09-26 09:59:59.000000000" });
  const tooOld = await insert({
    org,
    agent,
    pid: 7401,
    image: PS,
    parentPid: 7400,
    time: "2026-09-27 10:00:00.000000000",
  });
  // Exactly 24 h before the child: the look-back is inclusive.
  await insert({ org, agent, pid: 7410, image: EXCEL, time: "2026-09-26 10:00:00.000000000" });
  const boundary = await insert({
    org,
    agent,
    pid: 7411,
    image: PS,
    parentPid: 7410,
    time: "2026-09-27 10:00:00.000000000",
  });

  // Both parents were read by an earlier cycle; the batch holds the children only.
  const behindParents = "2026-09-27 00:00:00.000000000";
  expect((await readEvent(org, tooOld, behindParents))?.parentImage).toBeNull();
  expect((await readEvent(org, boundary, behindParents))?.parentImage).toBe(EXCEL);
});
