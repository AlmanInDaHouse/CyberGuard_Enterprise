import { randomUUID } from "node:crypto";
import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { runDetectionCycle } from "../src/detect/index.js";
import { enrollTestAgent, getAlerts, insertCgesEvent, insertNetworkEvents } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";

// SPEC-019 net_ac_004 — detection is unaffected (§Operational §8). A matching
// winword.exe -> powershell.exe pair of class 1007 is stored among Network
// Activity rows of the same agent and the same PIDs, before and after it in
// arrival order. A detection cycle evaluates exactly the two 1007 rows and writes
// the one alert the pair raises: the 4001 rows, whose activity_id 1 means Open,
// are neither evaluated nor taken as parents.

const ORG = "net-ac-004";
const AGENT = "01934abc-def0-7000-89ab-0000000a4004";

const WINWORD = "C:\\Program Files\\Microsoft Office\\root\\Office16\\winword.exe";
const POWERSHELL = "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";
const PARENT_PID = 4100;
const CHILD_PID = 4101;

let config: Config;

beforeAll(() => {
  config = inject("ingestConfig");
});

/** Two Network Activity rows of the pair's processes, one per PID. */
async function insertConnections(second: number): Promise<void> {
  await insertNetworkEvents(
    config,
    [PARENT_PID, CHILD_PID].map((pid, i) => ({
      agentId: AGENT,
      orgId: ORG,
      eventId: randomUUID(),
      processPid: pid,
      srcIp: "192.0.2.10",
      srcPort: 49000 + i,
      dstIp: "198.51.100.7",
      dstPort: 443,
      direction: "outbound" as const,
      time: `2026-10-10 12:00:0${second}.00000000${i}`,
    })),
  );
}

test("net_ac_004: a detection cycle evaluates the 1007 rows only and raises the pair's one alert", async () => {
  await enrollTestAgent(config, AGENT, ORG);

  await insertConnections(0);
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
  await insertConnections(3);

  const result = await runDetectionCycle(detectConfig(config, ORG));
  expect(result.eventsEvaluated).toBe(2);

  const alerts = await getAlerts(config, { agentId: AGENT });
  expect(alerts).toHaveLength(1);
  expect(alerts[0]?.rule_id).toBe("rule.office_spawns_script_host");
});
