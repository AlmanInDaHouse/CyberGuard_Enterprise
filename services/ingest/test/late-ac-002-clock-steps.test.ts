import { randomUUID } from "node:crypto";
import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { runDetectionCycle } from "../src/detect/index.js";
import { enrollTestAgent, getAlerts, insertCgesEvent } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";

// SPEC-018 late_ac_002 — one agent whose clock steps. The agent first delivers an
// event stamped an hour ahead of the server; once evaluated, its clock is back at
// the server's present and it delivers a matching winword.exe -> powershell.exe
// pair. The read-model advances by arrival (§Operational §1), so the pair is read
// and raises its alert. Under the SPEC-006 `time` watermark the ahead event held
// the watermark an hour in the future and the pair was never read (§Context 3).

const ORG = "late-ac-002";
const AGENT = "01934abc-def0-7000-89ab-0000000a2001";

const NOTEPAD = "C:\\Windows\\System32\\notepad.exe";
const WINWORD = "C:\\Program Files\\Microsoft Office\\root\\Office16\\winword.exe";
const POWERSHELL = "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";

let config: Config;

beforeAll(() => {
  config = inject("ingestConfig");
});

/** Whole-second epoch milliseconds as a ClickHouse DateTime64(9) literal. */
function chTime(ms: number): string {
  const iso = new Date(ms).toISOString();
  return `${iso.slice(0, 10)} ${iso.slice(11, 19)}.000000000`;
}

async function insertLaunch(
  pid: number,
  image: string,
  time: string,
  parentPid: number | null = null,
): Promise<void> {
  await insertCgesEvent(config, {
    agentId: AGENT,
    orgId: ORG,
    eventId: randomUUID(),
    activityId: 1,
    processPid: pid,
    processName: image.split("\\").pop() ?? image,
    imageFileName: image,
    processParentPid: parentPid,
    time,
  });
}

test("late_ac_002: after an event an hour ahead, a pair at the present is evaluated and alerts", async () => {
  await enrollTestAgent(config, AGENT, ORG);
  const now = Math.floor(Date.now() / 1000) * 1000;

  // The agent's clock runs an hour ahead.
  await insertLaunch(2100, NOTEPAD, chTime(now + 3_600_000));
  const first = await runDetectionCycle(detectConfig(config, ORG));
  expect(first.eventsEvaluated).toBe(1);

  // Its clock is back at the server's present.
  await insertLaunch(2200, WINWORD, chTime(now - 1000));
  await insertLaunch(2201, POWERSHELL, chTime(now), 2200);
  const second = await runDetectionCycle(detectConfig(config, ORG));
  expect(second.eventsEvaluated).toBe(2);

  const alerts = await getAlerts(config, { agentId: AGENT });
  expect(alerts).toHaveLength(1);
  expect(alerts[0]?.rule_id).toBe("rule.office_spawns_script_host");
  expect(alerts[0]?.event_time).toEqual(new Date(now));
});
