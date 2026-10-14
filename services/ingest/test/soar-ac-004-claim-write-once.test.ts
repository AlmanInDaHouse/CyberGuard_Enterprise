import pg from "pg";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { runDetectionCycle } from "../src/detect/index.js";
import { runPlaybooks } from "../src/soar/run.js";
import { enrollTestAgent, getIncidents } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";
import { spyNotify } from "./helpers/notify.js";
import {
  type SoarHarness,
  getExecutions,
  seedOfficeSpawn,
  shippedPlaybooks,
  soarHarness,
  testPlaybook,
} from "./helpers/soar.js";

// soar_ac_004 — SPEC-022 §Data contracts §2 / §Operational §3: a run claims its row
// before its first step (the unique key is the idempotency key), completes it once,
// and Postgres refuses any other change to the table.

let config: Config;
let harness: SoarHarness;
let pool: pg.Pool;

beforeAll(() => {
  config = inject("ingestConfig");
  harness = soarHarness(config, shippedPlaybooks());
  pool = new pg.Pool({ connectionString: config.INGEST_PG_URL });
});
afterAll(async () => {
  await harness?.close();
  await pool?.end();
});

/** An incident created with SOAR off, so the test controls its execution rows. */
async function bareIncident(org: string): Promise<{ agent: string; incidentId: string }> {
  const agent = globalThis.crypto.randomUUID();
  await enrollTestAgent(config, agent, org);
  await seedOfficeSpawn(config, org, agent);
  await runDetectionCycle(detectConfig(config, org));
  const [incident] = await getIncidents(config, { agentId: agent });
  if (!incident) throw new Error("no incident");
  return { agent, incidentId: incident.incident_id };
}

function created(org: string, agent: string, incidentId: string) {
  return {
    incidentId,
    orgId: org,
    agentId: agent,
    title: "t",
    severityId: 4,
    detectionSource: "rule",
  };
}

test("with the pair already claimed, a run sends nothing and writes nothing", async () => {
  const org = "soar-ac-004-claimed";
  const { agent, incidentId } = await bareIncident(org);
  await pool.query(
    `INSERT INTO playbook_executions
       (execution_id, org_id, incident_id, agent_id, playbook_id, playbook_version)
     VALUES ($1, $2, $3, $4, 'pb_high_severity_containment', 1)`,
    [globalThis.crypto.randomUUID(), org, incidentId, agent],
  );
  const spy = spyNotify();
  await runPlaybooks(harness.soar, spy.notify, created(org, agent, incidentId));

  expect(spy.sent).toEqual([]);
  const rows = await getExecutions(config, incidentId);
  expect(rows).toHaveLength(1);
  expect(rows[0]).toMatchObject({ status: "running", steps: [], chain_head: null });
  expect(harness.logs.some((l) => l.obj.event === "soar_run_skipped_claimed")).toBe(true);
});

test("while its email step sends, the row reads running", async () => {
  const org = "soar-ac-004-running";
  const { agent, incidentId } = await bareIncident(org);
  let seenWhileSending: string | undefined;
  const spy = spyNotify({
    onSend: async () => {
      const r = await pool.query<{ status: string }>(
        "SELECT status FROM playbook_executions WHERE incident_id = $1",
        [incidentId],
      );
      seenWhileSending = r.rows[0]?.status;
    },
  });
  await runPlaybooks(harness.soar, spy.notify, created(org, agent, incidentId));
  expect(seenWhileSending).toBe("running");
  expect((await getExecutions(config, incidentId))[0]?.status).toBe("completed");
});

test("Postgres refuses every change but the one completion", async () => {
  const org = "soar-ac-004-refusals";
  const { agent, incidentId } = await bareIncident(org);
  await runPlaybooks(harness.soar, spyNotify().notify, created(org, agent, incidentId));
  const [done] = await getExecutions(config, incidentId);
  if (!done) throw new Error("no execution");

  // A completed row: neither updated nor deleted.
  await expect(
    pool.query("UPDATE playbook_executions SET status = 'failed' WHERE execution_id = $1", [
      done.execution_id,
    ]),
  ).rejects.toThrow(/a completed row is never changed/);
  await expect(
    pool.query("DELETE FROM playbook_executions WHERE execution_id = $1", [done.execution_id]),
  ).rejects.toThrow(/never deleted/);
  await expect(pool.query("TRUNCATE playbook_executions")).rejects.toThrow(/never deleted/);

  // A running claim: an update must complete it, and only the completion columns move.
  const claimId = globalThis.crypto.randomUUID();
  await pool.query(
    `INSERT INTO playbook_executions
       (execution_id, org_id, incident_id, agent_id, playbook_id, playbook_version)
     VALUES ($1, $2, $3, $4, 'pb_other', 1)`,
    [claimId, org, incidentId, agent],
  );
  await expect(
    pool.query("UPDATE playbook_executions SET steps = '[{}]'::jsonb WHERE execution_id = $1", [
      claimId,
    ]),
  ).rejects.toThrow(/must complete the row/);
  await expect(
    pool.query(
      "UPDATE playbook_executions SET status = 'completed', playbook_id = 'pb_renamed' WHERE execution_id = $1",
      [claimId],
    ),
  ).rejects.toThrow(/only status, finished_at, steps and chain_head change/);

  // A new row is a running claim, never a completed one.
  await expect(
    pool.query(
      `INSERT INTO playbook_executions
         (execution_id, org_id, incident_id, agent_id, playbook_id, playbook_version, status)
       VALUES ($1, $2, $3, $4, 'pb_third', 1, 'completed')`,
      [globalThis.crypto.randomUUID(), org, incidentId, agent],
    ),
  ).rejects.toThrow(/a new row must be a running claim/);
});

test("a second playbook on the same incident gets its own row", async () => {
  const org = "soar-ac-004-two";
  const { agent, incidentId } = await bareIncident(org);
  const two = soarHarness(config, [testPlaybook({ id: "pb_a" }), testPlaybook({ id: "pb_b" })]);
  try {
    await runPlaybooks(two.soar, spyNotify().notify, created(org, agent, incidentId));
  } finally {
    await two.close();
  }
  expect((await getExecutions(config, incidentId)).map((e) => e.playbook_id).sort()).toEqual([
    "pb_a",
    "pb_b",
  ]);
});
