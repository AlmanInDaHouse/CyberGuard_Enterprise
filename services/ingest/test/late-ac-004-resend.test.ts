import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { runDetectionCycle } from "../src/detect/index.js";
import {
  type InsertCgesEventRow,
  enrollTestAgent,
  getAlerts,
  getIncidents,
  insertCgesEvent,
  setAlertStatus,
} from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";
import { spyNotify } from "./helpers/notify.js";

// SPEC-018 late_ac_004 — resend. The forward read has no FINAL (§Operational §3):
// an event the agent resends byte-identical (SPEC-017 §Operational §3) is a
// second row with a later arrived_at, and the next cycle reads and evaluates it
// again. The match rebuilds the same dedup_key (ADR-0012 §5: nothing in it
// depends on arrival or processing), so it writes nothing: still one alert, its
// triaged status preserved, its incident not updated, no notification.

const ORG = "late-ac-004";
const AGENT = "01934abc-def0-7000-89ab-0000000a4401";

const WINWORD = "C:\\Program Files\\Microsoft Office\\root\\Office16\\winword.exe";
const POWERSHELL = "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";

let config: Config;

beforeAll(() => {
  config = inject("ingestConfig");
});

const parent: InsertCgesEventRow = {
  agentId: AGENT,
  orgId: ORG,
  eventId: "01934abc-def0-4000-89ab-0000000a4410",
  activityId: 1,
  processPid: 4400,
  processName: "winword.exe",
  imageFileName: WINWORD,
  time: "2026-10-09 09:00:00.000000000",
};

const child: InsertCgesEventRow = {
  agentId: AGENT,
  orgId: ORG,
  eventId: "01934abc-def0-4000-89ab-0000000a4411",
  activityId: 1,
  processPid: 4401,
  processName: "powershell.exe",
  imageFileName: POWERSHELL,
  processParentPid: 4400,
  time: "2026-10-09 09:00:01.000000000",
};

test("late_ac_004: a resent event is read again and writes nothing: one alert, status kept, incident untouched", async () => {
  await enrollTestAgent(config, AGENT, ORG);
  await insertCgesEvent(config, parent);
  await insertCgesEvent(config, child);

  const first = await runDetectionCycle(detectConfig(config, ORG));
  expect(first.eventsEvaluated).toBe(2);
  expect(first.alertsWritten).toBe(1);
  const [alert] = await getAlerts(config, { agentId: AGENT });
  if (alert === undefined) throw new Error("the first cycle wrote no alert");
  await setAlertStatus(config, alert.alert_id, "acknowledged");
  const [incident] = await getIncidents(config, { agentId: AGENT });
  if (incident === undefined) throw new Error("the first cycle created no incident");

  // The agent resends the child byte-identical: same org_id, time and event_id.
  await insertCgesEvent(config, child);
  const spy = spyNotify();
  const second = await runDetectionCycle(detectConfig(config, ORG), spy.notify);

  // The cycle read it again (no FINAL) ...
  expect(second.eventsEvaluated).toBe(1);
  expect(second.processedThrough?.eventId).toBe(child.eventId);
  // ... and wrote nothing.
  expect(second.alertsWritten).toBe(0);
  const alerts = await getAlerts(config, { agentId: AGENT });
  expect(alerts).toHaveLength(1);
  expect(alerts[0]?.alert_id).toBe(alert.alert_id);
  expect(alerts[0]?.status).toBe("acknowledged");
  const incidents = await getIncidents(config, { agentId: AGENT });
  expect(incidents).toHaveLength(1);
  expect(incidents[0]?.alert_ids).toEqual(incident.alert_ids);
  expect(incidents[0]?.updated_at).toEqual(incident.updated_at);
  expect(spy.sent).toHaveLength(0);
});
