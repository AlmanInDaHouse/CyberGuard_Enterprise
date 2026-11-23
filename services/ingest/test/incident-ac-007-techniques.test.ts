import pg from "pg";
import { beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { upsertIncident } from "../src/detect/incidents.js";
import type { IncidentGroupingInput } from "../src/detect/types.js";
import { enrollTestAgent, getIncidents, setIncidentTriage } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";

// SPEC-026 incident_ac_007 — §Operational §7: on grouping's update, the incident's
// techniques become its own followed by those of the alert that it lacks, in the alert's
// order (debt #26, its first half); its tactics, status, assignee, activity_id and title are
// kept, as SPEC-007's invariant says of a human's triage.

let config: Config;
beforeAll(() => {
  config = inject("ingestConfig");
});

test("incident_ac_007: an incident accumulates its alerts' techniques, in order, and keeps the rest", async () => {
  const agentId = globalThis.crypto.randomUUID();
  const orgId = "incident-ac-007";
  await enrollTestAgent(config, agentId, orgId);
  const ids = [1, 2, 3].map(() => globalThis.crypto.randomUUID());
  const alert = (n: number, tactics: string[], techniques: string[]): IncidentGroupingInput => ({
    alertId: ids[n - 1] ?? "",
    orgId,
    agentId,
    cgMitre: { tactics, techniques },
    // One window: one grouping key, the tactics' order aside (it is canonical in the key).
    eventTime: "2026-05-31 18:00:00.000000000",
    severityId: 3,
  });

  const first = await upsertIncident(
    detectConfig(config, orgId),
    alert(1, ["execution", "persistence"], ["T1"]),
  );
  expect(first.created).toBe(true);
  await setIncidentTriage(config, first.incidentId, {
    status: "investigating",
    assignedTo: "analyst-7",
  });
  const [created] = await getIncidents(config, { agentId });

  const second = await upsertIncident(
    detectConfig(config, orgId),
    alert(2, ["persistence", "execution"], ["T2", "T1"]),
  );
  const third = await upsertIncident(
    detectConfig(config, orgId),
    alert(3, ["execution", "persistence"], ["T3"]),
  );
  expect([second.created, third.created]).toEqual([false, false]);
  expect([second.incidentId, third.incidentId]).toEqual([first.incidentId, first.incidentId]);

  const incidents = await getIncidents(config, { agentId });
  expect(incidents).toHaveLength(1);
  const [incident] = incidents;
  expect(incident?.cg_mitre).toEqual({
    tactics: ["execution", "persistence"],
    techniques: ["T1", "T2", "T3"],
  });
  expect(incident?.status).toBe("investigating");
  expect(incident?.assigned_to).toBe("analyst-7");
  expect(incident?.title).toBe(created?.title);
  expect(incident?.alert_ids).toEqual(ids);
  const pool = new pg.Pool({ connectionString: config.INGEST_PG_URL });
  try {
    const r = await pool.query<{ activity_id: number }>(
      "SELECT activity_id FROM incidents WHERE incident_id = $1",
      [first.incidentId],
    );
    expect(r.rows).toEqual([{ activity_id: 1 }]);
  } finally {
    await pool.end();
  }
});

test("incident_ac_007: an alert whose techniques the incident holds, or that has none, changes no technique", async () => {
  const agentId = globalThis.crypto.randomUUID();
  const orgId = "incident-ac-007-none";
  await enrollTestAgent(config, agentId, orgId);
  const alert = (techniques: string[]): IncidentGroupingInput => ({
    alertId: globalThis.crypto.randomUUID(),
    orgId,
    agentId,
    cgMitre: { tactics: ["discovery"], techniques },
    eventTime: "2026-05-31 19:00:00.000000000",
    severityId: 2,
  });
  await upsertIncident(detectConfig(config, orgId), alert(["T1082", "T1016"]));
  await upsertIncident(detectConfig(config, orgId), alert(["T1016", "T1082"]));
  await upsertIncident(detectConfig(config, orgId), alert([]));
  const [incident] = await getIncidents(config, { agentId });
  expect(incident?.cg_mitre).toEqual({ tactics: ["discovery"], techniques: ["T1082", "T1016"] });
  expect(incident?.alert_ids).toHaveLength(3);
});
