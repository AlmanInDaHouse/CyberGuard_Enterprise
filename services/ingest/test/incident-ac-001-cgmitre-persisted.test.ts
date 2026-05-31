import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { runDetectionCycle } from "../src/detect/index.js";
import { enrollTestAgent, getAlerts, insertCgesEvent } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";

// SPEC-007 incident_ac_001 — GREEN-GUARD (not red-by-design): cg_mitre is populated
// on the PERSISTED alert row. Phase 5 already writes cg_mitre (alerts.ts upsertAlert
// JSON.stringify(match.cgMitre)); SPEC-006's engine.test only asserted the PARSED
// rule (engine.test.ts), never the Postgres row. This is the first test to assert
// the persisted column, closing the gate-zero "populated, proved by code-trace +
// zod invariant, not by a row" nuance. It passes from the start. CI-able.

let config: Config;
beforeAll(() => {
  config = inject("ingestConfig");
});

const WINWORD = "C:\\Program Files\\Microsoft Office\\root\\Office16\\winword.exe";
const POWERSHELL = "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";

test("incident_ac_001: cg_mitre is non-empty on the persisted alert row", async () => {
  const agentId = "01934abc-def0-7000-89ab-000000000010";
  const orgId = "incident-ac-001";

  await insertCgesEvent(config, {
    agentId,
    orgId,
    eventId: "01934abc-def0-4000-89ab-000000000101",
    activityId: 1,
    processPid: 7000,
    processName: "winword.exe",
    imageFileName: WINWORD,
    time: "2026-05-31 12:00:00.000000000",
  });
  await insertCgesEvent(config, {
    agentId,
    orgId,
    eventId: "01934abc-def0-4000-89ab-000000000102",
    activityId: 1,
    processPid: 7001,
    processName: "powershell.exe",
    imageFileName: POWERSHELL,
    processParentPid: 7000,
    time: "2026-05-31 12:00:01.000000000",
  });

  await enrollTestAgent(config, agentId);
  await runDetectionCycle(detectConfig(config, orgId));

  // Scoped to this test's unique agent_id (per-test isolation, like detect_ac_003/004),
  // NOT a bare rule_id predicate — the single-rule MVP would match every sibling test.
  const alerts = await getAlerts(config, { agentId });
  expect(alerts).toHaveLength(1);
  const alert = alerts[0];
  expect(alert?.rule_id).toBe("rule.office_spawns_script_host");
  expect(alert?.cg_mitre).not.toBeNull();
  expect(alert?.cg_mitre?.tactics).toContain("execution");
  expect(alert?.cg_mitre?.tactics).toContain("initial-access");
  expect(alert?.cg_mitre?.techniques).toContain("T1059.001");
  expect(alert?.cg_mitre?.techniques).toContain("T1566.001");
});
