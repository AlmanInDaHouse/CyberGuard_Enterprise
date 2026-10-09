import { randomUUID } from "node:crypto";
import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { runDetectionCycle } from "../src/detect/index.js";
import { enrollTestAgent, getAlerts, insertCgesEvent } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";

// SPEC-018 late_ac_001 — another agent, earlier time. Agent A's events are
// evaluated first; agent B then delivers a matching winword.exe -> powershell.exe
// pair whose `time` is earlier than A's. The read-model advances by arrival
// (§Operational §1), so the next cycle still reads B's events and raises B's alert,
// stamped with the child's event time (§Operational §4). Under the SPEC-006 `time`
// watermark B's events fell behind A's and were never read (§Context 3).

const ORG = "late-ac-001";
const AGENT_A = "01934abc-def0-7000-89ab-0000000a1001";
const AGENT_B = "01934abc-def0-7000-89ab-0000000a1002";

const EXPLORER = "C:\\Windows\\explorer.exe";
const NOTEPAD = "C:\\Windows\\System32\\notepad.exe";
const WINWORD = "C:\\Program Files\\Microsoft Office\\root\\Office16\\winword.exe";
const POWERSHELL = "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";

let config: Config;

beforeAll(() => {
  config = inject("ingestConfig");
});

/** Insert a parent Launch and a child Launch of it, both by `agentId`. */
async function insertPair(
  agentId: string,
  parent: { pid: number; image: string; time: string },
  child: { pid: number; image: string; time: string },
): Promise<void> {
  for (const p of [parent, { ...child, parentPid: parent.pid }]) {
    await insertCgesEvent(config, {
      agentId,
      orgId: ORG,
      eventId: randomUUID(),
      activityId: 1,
      processPid: p.pid,
      processName: p.image.split("\\").pop() ?? p.image,
      imageFileName: p.image,
      processParentPid: "parentPid" in p ? p.parentPid : null,
      time: p.time,
    });
  }
}

test("late_ac_001: another agent's events with an earlier time are evaluated and alert", async () => {
  await enrollTestAgent(config, AGENT_A, ORG);
  await enrollTestAgent(config, AGENT_B, ORG);

  // Agent A: a benign pair at 12:00, evaluated first.
  await insertPair(
    AGENT_A,
    { pid: 1100, image: EXPLORER, time: "2026-10-09 12:00:00.000000000" },
    { pid: 1101, image: NOTEPAD, time: "2026-10-09 12:00:01.000000000" },
  );
  const first = await runDetectionCycle(detectConfig(config, ORG));
  expect(first.eventsEvaluated).toBe(2);

  // Agent B delivers afterwards a matching pair from an hour earlier.
  const childTime = "2026-10-09 11:00:01.000000000";
  await insertPair(
    AGENT_B,
    { pid: 1200, image: WINWORD, time: "2026-10-09 11:00:00.000000000" },
    { pid: 1201, image: POWERSHELL, time: childTime },
  );
  const second = await runDetectionCycle(detectConfig(config, ORG));
  expect(second.eventsEvaluated).toBe(2);

  const alerts = await getAlerts(config, { agentId: AGENT_B });
  expect(alerts).toHaveLength(1);
  expect(alerts[0]?.rule_id).toBe("rule.office_spawns_script_host");
  expect(alerts[0]?.event_time).toEqual(new Date("2026-10-09T11:00:01Z"));
  // Agent A's benign pair raised nothing.
  expect(await getAlerts(config, { agentId: AGENT_A })).toHaveLength(0);
});
