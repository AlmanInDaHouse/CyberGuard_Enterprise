import pg from "pg";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { upsertIncident } from "../src/detect/incidents.js";
import { runDetectionCycle } from "../src/detect/index.js";
import {
  type IncidentRow,
  enrollTestAgent,
  getIncidents,
  insertCgesEvent,
  setIncidentTriage,
} from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";
import { spyNotify } from "./helpers/notify.js";
import { type SoarHarness, getExecutions, shippedPlaybooks, soarHarness } from "./helpers/soar.js";

// SPEC-026 incident_ac_006 — §Operational §7: grouping's upsert conflicts on the open
// incident's key. An alert whose key has only closed incidents, resolved or false_positive,
// opens a new incident, which is a creation: notified (ADR-0017) and running its playbooks
// (ADR-0021); the closed one is unchanged. An alert whose key has an open incident, contained
// among the open statuses, joins it. And no alert joins an incident after its close: a close
// that commits while an alert of its key is being grouped leaves the alert to a new incident.

let config: Config;
let harness: SoarHarness;

beforeAll(() => {
  config = inject("ingestConfig");
  harness = soarHarness(config, shippedPlaybooks());
});
afterAll(async () => {
  await harness?.close();
});

const WINWORD = "C:\\Program Files\\Microsoft Office\\root\\Office16\\winword.exe";
const POWERSHELL = "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";

/** A winword.exe spawning powershell.exe at 12:`minute` (office_spawns_script_host, High).
 *  Two minutes six apart fall in different 5-minute dedup buckets of one 30-minute grouping
 *  window: two distinct alerts of one grouping key. */
async function seedSpawn(orgId: string, agentId: string, minute: number): Promise<void> {
  const pid = 7200 + minute * 10;
  const mm = String(minute).padStart(2, "0");
  await insertCgesEvent(config, {
    agentId,
    orgId,
    eventId: globalThis.crypto.randomUUID(),
    activityId: 1,
    processPid: pid,
    processName: "winword.exe",
    imageFileName: WINWORD,
    time: `2026-05-31 12:${mm}:00.000000000`,
  });
  await insertCgesEvent(config, {
    agentId,
    orgId,
    eventId: globalThis.crypto.randomUUID(),
    activityId: 1,
    processPid: pid + 1,
    processName: "powershell.exe",
    imageFileName: POWERSHELL,
    processParentPid: pid,
    time: `2026-05-31 12:${mm}:01.000000000`,
  });
}

/** The incident's fields that a later alert must not change. */
function frozen(i: IncidentRow | undefined) {
  return {
    status: i?.status,
    alert_ids: i?.alert_ids,
    severity_id: i?.severity_id,
    cg_mitre: i?.cg_mitre,
    updated_at: i?.updated_at?.toISOString(),
  };
}

for (const closed of ["resolved", "false_positive"] as const) {
  test(`an alert of a key whose incident is ${closed} opens a second incident, notified, with its playbooks`, async () => {
    const org = `incident-ac-006-${closed}`;
    const agent = globalThis.crypto.randomUUID();
    await enrollTestAgent(config, agent, org);

    await seedSpawn(org, agent, 0);
    await runDetectionCycle(detectConfig(config, org), spyNotify().notify, harness.soar);
    const [first] = await getIncidents(config, { agentId: agent });
    expect(first).toBeDefined();
    const firstId = first?.incident_id ?? "";
    await setIncidentTriage(config, firstId, { status: closed });
    const before = frozen((await getIncidents(config, { agentId: agent }))[0]);
    const firstRuns = await getExecutions(config, firstId);
    expect(firstRuns).toHaveLength(1);

    await seedSpawn(org, agent, 6);
    const spy = spyNotify();
    await runDetectionCycle(detectConfig(config, org), spy.notify, harness.soar);

    const incidents = await getIncidents(config, { agentId: agent });
    expect(incidents).toHaveLength(2);
    const [after, second] = incidents;
    // The closed incident took nothing.
    expect(after?.incident_id).toBe(firstId);
    expect(frozen(after)).toEqual(before);
    expect(await getExecutions(config, firstId)).toEqual(firstRuns);
    // A new incident of the same key, open, with the new alert alone.
    expect(second?.grouping_key).toBe(first?.grouping_key);
    expect(second?.status).toBe("open");
    expect(second?.alert_ids).toHaveLength(1);
    expect(first?.alert_ids).not.toContain(second?.alert_ids[0]);
    // A creation: notified once (the playbook's own email aside), and its playbooks run.
    const notices = spy.sent.filter((m) => m.subject.startsWith("[CyberGuard] New incident"));
    expect(notices.map((m) => m.text.includes(`Incident: ${second?.incident_id}`))).toEqual([true]);
    expect(spy.sent.some((m) => m.text.includes(firstId))).toBe(false);
    expect(
      (await getExecutions(config, second?.incident_id ?? "")).map((e) => e.playbook_id),
    ).toEqual(["pb_high_severity_containment"]);
  });
}

test("an alert of a key whose incident is contained joins it, and nothing is notified", async () => {
  const org = "incident-ac-006-contained";
  const agent = globalThis.crypto.randomUUID();
  await enrollTestAgent(config, agent, org);

  await seedSpawn(org, agent, 0);
  await runDetectionCycle(detectConfig(config, org), spyNotify().notify, harness.soar);
  const [first] = await getIncidents(config, { agentId: agent });
  const firstId = first?.incident_id ?? "";
  await setIncidentTriage(config, firstId, { status: "contained" });

  await seedSpawn(org, agent, 6);
  const spy = spyNotify();
  await runDetectionCycle(detectConfig(config, org), spy.notify, harness.soar);

  const incidents = await getIncidents(config, { agentId: agent });
  expect(incidents).toHaveLength(1);
  expect(incidents[0]?.incident_id).toBe(firstId);
  expect(incidents[0]?.status).toBe("contained");
  expect(incidents[0]?.alert_ids).toHaveLength(2);
  expect(spy.sent).toEqual([]);
  expect(await getExecutions(config, firstId)).toHaveLength(1);
});

test("a close that commits while an alert of its key is grouped leaves the alert to a new incident", async () => {
  const org = "incident-ac-006-race";
  const agent = globalThis.crypto.randomUUID();
  await enrollTestAgent(config, agent, org);
  const mitre = { tactics: ["execution"], techniques: ["T1059.001"] };
  const eventTime = "2026-05-31 12:00:00.000000000";
  const firstAlert = globalThis.crypto.randomUUID();
  const created = await upsertIncident(detectConfig(config, org), {
    alertId: firstAlert,
    orgId: org,
    agentId: agent,
    cgMitre: mitre,
    eventTime,
    severityId: 4,
  });
  expect(created.created).toBe(true);

  // The analyst's close, held open: it locks the incident's row.
  const closer = new pg.Client({ connectionString: config.INGEST_PG_URL });
  await closer.connect();
  const watcher = new pg.Client({ connectionString: config.INGEST_PG_URL });
  await watcher.connect();
  try {
    await closer.query("BEGIN");
    await closer.query(
      "UPDATE incidents SET status = 'resolved', updated_at = now() WHERE incident_id = $1",
      [created.incidentId],
    );
    const closerPid = (await closer.query<{ pid: number }>("SELECT pg_backend_pid() AS pid"))
      .rows[0]?.pid;

    const secondAlert = globalThis.crypto.randomUUID();
    const grouping = upsertIncident(detectConfig(config, org), {
      alertId: secondAlert,
      orgId: org,
      agentId: agent,
      cgMitre: mitre,
      eventTime,
      severityId: 4,
    });
    // Wait until the upsert is blocked behind the close's row lock.
    let blocked = false;
    for (let i = 0; i < 200 && !blocked; i++) {
      const r = await watcher.query<{ n: string }>(
        `SELECT count(*) AS n FROM pg_stat_activity
          WHERE $1 = ANY(pg_blocking_pids(pid)) AND query LIKE 'INSERT INTO incidents%'`,
        [closerPid],
      );
      blocked = Number(r.rows[0]?.n) > 0;
      if (!blocked) await new Promise((resolve) => setTimeout(resolve, 25));
    }
    expect(blocked).toBe(true);
    await closer.query("COMMIT");

    const second = await grouping;
    expect(second.created).toBe(true);
    expect(second.incidentId).not.toBe(created.incidentId);
    const incidents = await getIncidents(config, { agentId: agent });
    expect(incidents.map((i) => [i.status, i.alert_ids])).toEqual([
      ["resolved", [firstAlert]],
      ["open", [secondAlert]],
    ]);
  } finally {
    await closer.query("ROLLBACK").catch(() => {});
    await closer.end();
    await watcher.end();
  }
});
