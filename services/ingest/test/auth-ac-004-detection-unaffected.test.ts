import { randomUUID } from "node:crypto";
import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { runDetectionCycle } from "../src/detect/index.js";
import { enrollTestAgent, getAlerts, insertCgesEvent, insertLogonEvents } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";

// SPEC-020 auth_ac_004 — detection is unaffected (§Operational §9). A matching
// winword.exe -> powershell.exe pair of class 1007 is stored among Authentication
// rows of the same agent, before and after it in arrival order. A detection cycle
// evaluates exactly the two 1007 rows and writes the one alert the pair raises:
// the 3002 rows, whose activity_id 1 means Logon, are neither evaluated nor taken
// as parents.

const ORG = "auth-ac-004";
const AGENT = "01934abc-def0-7000-89ab-0000000a3004";

const WINWORD = "C:\\Program Files\\Microsoft Office\\root\\Office16\\winword.exe";
const POWERSHELL = "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";
const PARENT_PID = 4200;
const CHILD_PID = 4201;

let config: Config;

beforeAll(() => {
  config = inject("ingestConfig");
});

/** Two Authentication rows, a success and a failure. */
async function insertLogons(second: number): Promise<void> {
  await insertLogonEvents(config, [
    {
      agentId: AGENT,
      orgId: ORG,
      eventId: randomUUID(),
      userUid: "S-1-5-21-1111-2222-3333-1001",
      userName: "auth-ac-004-user",
      statusId: 1,
      logonTypeId: 2,
      time: `2026-10-10 12:00:0${second}.000000001`,
    },
    {
      agentId: AGENT,
      orgId: ORG,
      eventId: randomUUID(),
      userUid: "S-1-0-0",
      userName: "<withheld>",
      statusId: 2,
      logonTypeId: 3,
      time: `2026-10-10 12:00:0${second}.000000002`,
    },
  ]);
}

test("auth_ac_004: a detection cycle evaluates the 1007 rows only and raises the pair's one alert", async () => {
  await enrollTestAgent(config, AGENT, ORG);

  await insertLogons(0);
  await insertCgesEvent(config, {
    agentId: AGENT,
    orgId: ORG,
    eventId: randomUUID(),
    activityId: 1,
    processPid: PARENT_PID,
    processName: "winword.exe",
    imageFileName: WINWORD,
    time: "2026-10-10 12:00:01.000000000",
  });
  await insertCgesEvent(config, {
    agentId: AGENT,
    orgId: ORG,
    eventId: randomUUID(),
    activityId: 1,
    processPid: CHILD_PID,
    processName: "powershell.exe",
    imageFileName: POWERSHELL,
    processParentPid: PARENT_PID,
    time: "2026-10-10 12:00:02.000000000",
  });
  await insertLogons(3);

  const result = await runDetectionCycle(detectConfig(config, ORG));
  expect(result.eventsEvaluated).toBe(2);

  const alerts = await getAlerts(config, { agentId: AGENT });
  expect(alerts).toHaveLength(1);
  expect(alerts[0]?.rule_id).toBe("rule.office_spawns_script_host");
});
