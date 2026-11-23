import pg from "pg";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";

// SPEC-026 §Data contracts §1 — migration 0011_dashboard_rights, its structure: the triage
// history, written by its owner's trigger for each change of an incident's status or
// assignee and of an alert's status, and refused any change; and the unique key of an open
// incident, in place of the key of every incident. The grants, which act only where the
// bundle's roles exist, are the api's deploy_ac_004 test.
//
// Every assertion runs inside a BEGIN/ROLLBACK, so nothing persists in the Postgres the
// suite shares; within one transaction `now()` is the transaction's time, which the
// history's `changed_at` is.

let client: pg.Client;

beforeAll(async () => {
  const config: Config = inject("ingestConfig");
  client = new pg.Client({ connectionString: config.INGEST_PG_URL });
  await client.connect();
});
afterAll(async () => {
  await client?.end();
});

interface Seeded {
  agentId: string;
  alertId: string;
  incidentId: string;
}

/** An agent, an alert and an incident of `org`, inside the caller's transaction. */
async function seed(org: string, key: string, status = "open"): Promise<Seeded> {
  const agentId = globalThis.crypto.randomUUID();
  const alertId = globalThis.crypto.randomUUID();
  const incidentId = globalThis.crypto.randomUUID();
  await client.query(
    `INSERT INTO agents (agent_id, org_id, pubkey, cert_pem, expires_at)
     VALUES ($1, $2, '\\x00', 'c', now() + interval '1 day')`,
    [agentId, org],
  );
  await client.query(
    `INSERT INTO alerts
       (alert_id, org_id, agent_id, title, severity_id, cg_detection_source, rule_id,
        source_events, heuristic_score, final_score, cg_mitre, dedup_key, event_time)
     VALUES ($1, $2, $3, 't', 4, 'rule', 'rule.x', ARRAY[$1]::uuid[], 0.9, 0.9,
             '{"tactics":["execution"],"techniques":["T1059.001"]}', $4, now())`,
    [alertId, org, agentId, `dedup::${alertId}`],
  );
  await client.query(
    `INSERT INTO incidents
       (incident_id, org_id, agent_id, title, status, cg_mitre, alert_ids, severity_id,
        grouping_key, window_start)
     VALUES ($1, $2, $3, 't', $4, '{"tactics":["execution"],"techniques":["T1059.001"]}',
             ARRAY[$5]::uuid[], 4, $6, now())`,
    [incidentId, org, agentId, status, alertId, key],
  );
  return { agentId, alertId, incidentId };
}

interface HistoryRow {
  org_id: string;
  object_kind: string;
  object_id: string;
  field: string;
  old_value: string | null;
  new_value: string | null;
  at_now: boolean;
  changed_by_role: string;
}

async function history(objectId: string): Promise<HistoryRow[]> {
  const r = await client.query<HistoryRow>(
    `SELECT org_id, object_kind, object_id, field, old_value, new_value,
            changed_at = now() AS at_now, changed_by_role
       FROM triage_history WHERE object_id = $1 ORDER BY history_id`,
    [objectId],
  );
  return r.rows;
}

async function sessionUser(): Promise<string> {
  const r = await client.query<{ u: string }>("SELECT session_user::text AS u");
  return r.rows[0]?.u ?? "";
}

/** Runs `body` in a transaction that is always rolled back. */
async function rolledBack(body: () => Promise<void>): Promise<void> {
  await client.query("BEGIN");
  try {
    await body();
  } finally {
    await client.query("ROLLBACK");
  }
}

/** Runs `text` under a savepoint and returns the SQLSTATE it failed with, or null. */
async function failure(text: string, params: unknown[] = []): Promise<string | null> {
  await client.query("SAVEPOINT s");
  const code = await client.query(text, params).then(
    () => null,
    (e: unknown) => (e as { code?: string }).code ?? "unknown",
  );
  await client.query(code === null ? "RELEASE SAVEPOINT s" : "ROLLBACK TO SAVEPOINT s");
  return code;
}

test("an incident's status and assignee changes each write one history row", async () => {
  await rolledBack(async () => {
    const org = "migration-0011-incident";
    const { incidentId } = await seed(org, `${org}::k`);
    const role = await sessionUser();
    await client.query(
      "UPDATE incidents SET status = 'acknowledged', assigned_to = 'u-1' WHERE incident_id = $1",
      [incidentId],
    );
    await client.query("UPDATE incidents SET assigned_to = NULL WHERE incident_id = $1", [
      incidentId,
    ]);
    const base = { org_id: org, object_kind: "incident", object_id: incidentId, at_now: true };
    expect(await history(incidentId)).toEqual([
      {
        ...base,
        field: "status",
        old_value: "open",
        new_value: "acknowledged",
        changed_by_role: role,
      },
      { ...base, field: "assigned_to", old_value: null, new_value: "u-1", changed_by_role: role },
      { ...base, field: "assigned_to", old_value: "u-1", new_value: null, changed_by_role: role },
    ]);
  });
});

test("an alert's status change writes one history row", async () => {
  await rolledBack(async () => {
    const org = "migration-0011-alert";
    const { alertId } = await seed(org, `${org}::k`);
    await client.query("UPDATE alerts SET status = 'acknowledged' WHERE alert_id = $1", [alertId]);
    expect(await history(alertId)).toEqual([
      {
        org_id: org,
        object_kind: "alert",
        object_id: alertId,
        field: "status",
        old_value: "new",
        new_value: "acknowledged",
        at_now: true,
        changed_by_role: await sessionUser(),
      },
    ]);
  });
});

test("a value set to itself, and grouping's columns, write no history row", async () => {
  await rolledBack(async () => {
    const org = "migration-0011-unchanged";
    const { incidentId, alertId } = await seed(org, `${org}::k`);
    await client.query(
      "UPDATE incidents SET status = status, assigned_to = assigned_to WHERE incident_id = $1",
      [incidentId],
    );
    await client.query("UPDATE alerts SET status = status WHERE alert_id = $1", [alertId]);
    await client.query(
      `UPDATE incidents SET alert_ids = alert_ids || ARRAY[$2]::uuid[], severity_id = 6,
              cg_mitre = cg_mitre, updated_at = now()
        WHERE incident_id = $1`,
      [incidentId, globalThis.crypto.randomUUID()],
    );
    expect(await history(incidentId)).toEqual([]);
    expect(await history(alertId)).toEqual([]);
  });
});

test("a history row is never updated, deleted or truncated", async () => {
  await rolledBack(async () => {
    const org = "migration-0011-write-once";
    const { incidentId } = await seed(org, `${org}::k`);
    await client.query("UPDATE incidents SET status = 'resolved' WHERE incident_id = $1", [
      incidentId,
    ]);
    expect(await history(incidentId)).toHaveLength(1);
    expect(
      await failure("UPDATE triage_history SET new_value = 'open' WHERE object_id = $1", [
        incidentId,
      ]),
    ).toBe("P0001");
    expect(await failure("DELETE FROM triage_history WHERE object_id = $1", [incidentId])).toBe(
      "P0001",
    );
    expect(await failure("TRUNCATE triage_history")).toBe("P0001");
    expect(await history(incidentId)).toHaveLength(1);
  });
});

test("the history's checks refuse another kind of object or field", async () => {
  await rolledBack(async () => {
    const insert = `INSERT INTO triage_history
        (org_id, object_kind, object_id, field, old_value, new_value, changed_at, changed_by_role)
      VALUES ('o', $1, gen_random_uuid(), $2, NULL, NULL, now(), 'r')`;
    expect(await failure(insert, ["agent", "status"])).toBe("23514");
    expect(await failure(insert, ["incident", "title"])).toBe("23514");
    expect(await failure(insert, ["alert", "status"])).toBeNull();
  });
});

test("the history's trigger function runs as its owner and resolves names in ingest first", async () => {
  const r = await client.query<{ prosecdef: boolean; proconfig: string[] | null }>(
    `SELECT p.prosecdef, p.proconfig FROM pg_proc p
       JOIN pg_namespace n ON n.oid = p.pronamespace
      WHERE n.nspname = 'ingest' AND p.proname = 'triage_history_record'`,
  );
  expect(r.rows).toEqual([{ prosecdef: true, proconfig: ["search_path=ingest, pg_temp"] }]);
  const index = await client.query<{ indexdef: string }>(
    `SELECT indexdef FROM pg_indexes
      WHERE schemaname = 'ingest' AND tablename = 'triage_history'
        AND indexdef LIKE '%(org_id, object_kind, object_id, history_id)%'`,
  );
  expect(index.rows).toHaveLength(1);
});

test("one open incident per grouping key, and any number of closed ones", async () => {
  await rolledBack(async () => {
    const org = "migration-0011-key";
    const key = `${org}::k`;
    await seed(org, key, "resolved");
    await seed(org, key, "false_positive");
    const open = await seed(org, key, "contained");
    // A second open incident of the key is refused, whatever its open status.
    for (const status of ["open", "acknowledged", "investigating", "contained"]) {
      await client.query("SAVEPOINT k");
      const code = await seed(org, key, status).then(
        () => null,
        (e: unknown) => (e as { code?: string }).code,
      );
      await client.query("ROLLBACK TO SAVEPOINT k");
      expect(code, status).toBe("23505");
    }
    // A closed one can be reopened only while no other incident of its key is open.
    const closed = await client.query<{ incident_id: string }>(
      "SELECT incident_id FROM incidents WHERE grouping_key = $1 AND status = 'resolved'",
      [key],
    );
    const closedId = closed.rows[0]?.incident_id;
    expect(
      await failure("UPDATE incidents SET status = 'open' WHERE incident_id = $1", [closedId]),
    ).toBe("23505");
    await client.query("UPDATE incidents SET status = 'resolved' WHERE incident_id = $1", [
      open.incidentId,
    ]);
    expect(
      await failure("UPDATE incidents SET status = 'open' WHERE incident_id = $1", [closedId]),
    ).toBeNull();
    const constraint = await client.query(
      "SELECT 1 FROM pg_constraint WHERE conname = 'incidents_grouping_key_unique'",
    );
    expect(constraint.rows).toEqual([]);
  });
});
