import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import type { FastifyInstance } from "fastify";
import pg from "pg";
import { GenericContainer, type StartedTestContainer, Wait } from "testcontainers";
import { afterAll, beforeAll, describe, expect, inject, test } from "vitest";
import { buildApp } from "../src/app.js";
import { createUserFromCli } from "../src/cli/create-user.js";
import type { Config } from "../src/config.js";
import { MIGRATION_LOCK_KEY as API_LOCK_KEY, runMigrations } from "../src/db/migrate.js";
import { buildServices } from "../src/services.js";
import { DEFAULT_PASSWORD, currentTotp, insertUserDirect, seedSession } from "./helpers/db.js";
import { insertCgesEvent } from "./helpers/events-schema.js";
import { ensureAgent, insertAlertRow, insertIncidentRow } from "./helpers/read-schema.js";

// deploy_ac_004 and deploy_ac_005 — SPEC-023 §Operational §4: the bundle's Postgres
// initialisation (deploy/docker/postgres/) and the two services' migrations, run as their
// owner roles, give each runtime role the rights its code uses and no other, and the server
// logs no statement's parameters. And the api's own code runs as `cg_api`: the forensic key,
// the bootstrap command, a sign-in and the reads of ingest's tables.
//
// deploy_ac_004's and deploy_ac_005's test (SPEC-023 Amendment 2026-10-25, item 6): here,
// against a Postgres started with the bundle's own initialisation files.

const REPO = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
const INIT = join(REPO, "deploy", "docker", "postgres");
const INGEST_MIGRATE = join(REPO, "services", "ingest", "src", "db", "migrate.ts");

const PASSWORDS = {
  postgres: "deploy-004-superuser",
  cg_ingest_owner: "deploy-004-ingest-owner",
  cg_ingest: "deploy-004-ingest",
  cg_api_owner: "deploy-004-api-owner",
  cg_api: "deploy-004-api",
} as const;
type Login = keyof typeof PASSWORDS;
const RUNTIME = ["cg_ingest", "cg_api"] as const;

let container: StartedTestContainer;
let suite: Config;
const pools = new Map<string, pg.Pool>();

/** A role's URL, with a search path when given (as each service's connection sets it). */
function url(user: Login, schema?: "ingest" | "api"): string {
  const base = `postgres://${user}:${PASSWORDS[user]}@${container.getHost()}:${container.getMappedPort(5432)}/cyberguard`;
  return schema ? `${base}?options=-c%20search_path%3D${schema}%2Cpublic` : base;
}

/** A pool for `user` with `schema`'s search path, kept for the file. */
function as(user: Login, schema?: "ingest" | "api"): pg.Pool {
  const key = `${user}/${schema ?? ""}`;
  let pool = pools.get(key);
  if (!pool) {
    pool = new pg.Pool({ connectionString: url(user, schema), max: 2 });
    pool.on("error", () => {});
    pools.set(key, pool);
  }
  return pool;
}

/** The statement is refused for want of a privilege (SQLSTATE 42501). */
async function refused(pool: pg.Pool, text: string): Promise<void> {
  const err = await pool.query(text).then(
    () => null,
    (e: unknown) => e as { code?: string; message?: string },
  );
  expect(err, `${text} was allowed`).not.toBeNull();
  expect(err?.code, `${text}: ${err?.message}`).toBe("42501");
}

async function allowed(pool: pg.Pool, text: string): Promise<void> {
  await expect(pool.query(text)).resolves.toBeDefined();
}

beforeAll(async () => {
  suite = inject("apiConfig");
  container = await new GenericContainer("pgvector/pgvector:pg16")
    .withEnvironment({
      POSTGRES_USER: "postgres",
      POSTGRES_PASSWORD: PASSWORDS.postgres,
      POSTGRES_DB: "cyberguard",
      CG_PG_INGEST_OWNER_PASSWORD: PASSWORDS.cg_ingest_owner,
      CG_PG_INGEST_PASSWORD: PASSWORDS.cg_ingest,
      CG_PG_API_OWNER_PASSWORD: PASSWORDS.cg_api_owner,
      CG_PG_API_PASSWORD: PASSWORDS.cg_api,
    })
    .withCopyDirectoriesToContainer([{ source: INIT, target: "/docker-entrypoint-initdb.d" }])
    .withExposedPorts(5432)
    .withWaitStrategy(Wait.forLogMessage(/database system is ready to accept connections/, 2))
    .start();

  // Each service's migrations as its owner role, as the bundle's one-shots run them.
  const ingest = (await import(pathToFileURL(INGEST_MIGRATE).href)) as {
    runPgMigrations(config: { INGEST_PG_URL: string }): Promise<boolean>;
  };
  expect(await ingest.runPgMigrations({ INGEST_PG_URL: url("cg_ingest_owner", "ingest") })).toBe(
    true,
  );
  await runMigrations({ API_PG_URL: url("cg_api_owner", "api") });
  // A table in `public`, for the refusals there that need one.
  await as("postgres").query("CREATE TABLE public.deploy_004_probe (x int)");
}, 240_000);

afterAll(async () => {
  for (const pool of pools.values()) await pool.end().catch(() => {});
  await container?.stop();
});

describe("deploy_ac_004 — the roles", () => {
  test("none of the five roles is a superuser, or creates databases or roles", async () => {
    const r = await as("postgres").query<{ rolname: string; flags: boolean }>(
      `SELECT rolname, (rolsuper OR rolcreatedb OR rolcreaterole) AS flags FROM pg_roles
        WHERE rolname IN ('cg_ingest_owner', 'cg_ingest', 'cg_api_owner', 'cg_api', 'cg_incident_reader')
        ORDER BY rolname`,
    );
    expect(r.rows).toEqual([
      { rolname: "cg_api", flags: false },
      { rolname: "cg_api_owner", flags: false },
      { rolname: "cg_incident_reader", flags: false },
      { rolname: "cg_ingest", flags: false },
      { rolname: "cg_ingest_owner", flags: false },
    ]);
  });

  test("no runtime role owns an object", async () => {
    const r = await as("postgres").query<{ n: string }>(
      `SELECT (SELECT count(*) FROM pg_class WHERE relowner IN (SELECT oid FROM pg_roles WHERE rolname IN ('cg_ingest', 'cg_api')))
            + (SELECT count(*) FROM pg_namespace WHERE nspowner IN (SELECT oid FROM pg_roles WHERE rolname IN ('cg_ingest', 'cg_api')))
            + (SELECT count(*) FROM pg_proc WHERE proowner IN (SELECT oid FROM pg_roles WHERE rolname IN ('cg_ingest', 'cg_api')))
            + (SELECT count(*) FROM pg_type WHERE typowner IN (SELECT oid FROM pg_roles WHERE rolname IN ('cg_ingest', 'cg_api')))
            + (SELECT count(*) FROM pg_largeobject_metadata WHERE lomowner IN (SELECT oid FROM pg_roles WHERE rolname IN ('cg_ingest', 'cg_api'))) AS n`,
    );
    expect(Number(r.rows[0]?.n)).toBe(0);
  });

  test("neither runtime role connects to the server's other databases", async () => {
    for (const role of RUNTIME) {
      for (const database of ["postgres", "template1"]) {
        const other = new URL(url(role));
        other.pathname = `/${database}`;
        const client = new pg.Client({ connectionString: other.toString() });
        client.on("error", () => {});
        const err = await client.connect().then(
          () => null,
          (e: unknown) => e as { code?: string },
        );
        if (err === null) await client.end();
        expect(err?.code, `${role} connected to ${database}`).toBe("42501");
      }
    }
  });

  test("no runtime role owns anything the server tracks, in any database", async () => {
    // pg_shdepend records an owner of every kind of object, across the databases.
    const r = await as("postgres").query<{ n: string }>(
      `SELECT count(*) AS n FROM pg_shdepend
        WHERE deptype = 'o' AND refobjid IN (SELECT oid FROM pg_roles WHERE rolname IN ('cg_ingest', 'cg_api'))`,
    );
    expect(Number(r.rows[0]?.n)).toBe(0);
  });

  test("neither runtime role creates a large object", async () => {
    for (const role of RUNTIME) {
      const pool = as(role, role === "cg_ingest" ? "ingest" : "api");
      await refused(pool, "SELECT lo_create(0)");
      await refused(pool, "SELECT lo_creat(-1)");
      await refused(pool, "SELECT lo_from_bytea(0, '\\x00')");
    }
  });

  // SPEC-023 Amendment 2026-10-25 (debt #122): a runtime role takes no advisory lock, so it
  // cannot hold a migration's key; the owner roles, which the migrations above ran as, do.
  test("neither runtime role takes an advisory lock; the owner roles do", async () => {
    // The sixteen: four functions and their pg_try_ forms, each with a bigint key and with
    // two integer keys.
    const takers = [
      "advisory_lock",
      "advisory_lock_shared",
      "advisory_xact_lock",
      "advisory_xact_lock_shared",
    ].flatMap((f) =>
      [`pg_${f}`, `pg_try_${f}`].flatMap((fn) => [`${fn}(4927004)`, `${fn}(8927008, 1)`]),
    );
    expect(takers).toHaveLength(16);
    for (const role of RUNTIME) {
      const pool = as(role, role === "cg_ingest" ? "ingest" : "api");
      for (const call of takers) await refused(pool, `SELECT ${call}`);
    }
    for (const owner of ["cg_ingest_owner", "cg_api_owner"] as const) {
      const r = await as(owner).query<{ locked: boolean }>(
        "SELECT pg_try_advisory_xact_lock(424242) AS locked",
      );
      expect(r.rows[0]?.locked).toBe(true);
    }
  });

  // SPEC-023 Amendment 2026-10-25, item 2: a service's own run (`*_RUN_MIGRATIONS=true`) as
  // its runtime role is refused the lock, held or not, and fails.
  test("a service's own run as its runtime role is refused the lock, held or not, and fails", async () => {
    const ingest = (await import(pathToFileURL(INGEST_MIGRATE).href)) as {
      MIGRATION_LOCK_KEY: number;
      runPgMigrations(config: { INGEST_PG_URL: string }): Promise<boolean>;
    };
    const runs = [
      () => ingest.runPgMigrations({ INGEST_PG_URL: url("cg_ingest", "ingest") }),
      () => runMigrations({ API_PG_URL: url("cg_api", "api") }),
    ];
    async function refusedRuns(): Promise<void> {
      for (const run of runs) {
        const err = await run().then(
          () => null,
          (e: unknown) => e as { code?: string; message?: string },
        );
        expect(err, "the run was not refused").not.toBeNull();
        expect(err?.code, err?.message).toBe("42501");
        // Refused the lock itself, not a later statement of the migrator's.
        expect(err?.message).toBe("permission denied for function pg_try_advisory_lock");
      }
    }
    await refusedRuns();
    // Held by another session: refused all the same, rather than skipped.
    const holder = new pg.Client({ connectionString: url("postgres") });
    holder.on("error", () => {});
    await holder.connect();
    try {
      await holder.query("SELECT pg_advisory_lock($1), pg_advisory_lock($2)", [
        ingest.MIGRATION_LOCK_KEY,
        API_LOCK_KEY,
      ]);
      await refusedRuns();
    } finally {
      await holder.end();
    }
  });

  test("each ingest table the api reads or writes is owned by cg_ingest_owner, in the schema ingest", async () => {
    const tables = [
      "agents",
      "alerts",
      "enrollment_tokens",
      "incidents",
      "playbook_executions",
      "triage_history",
    ];
    const r = await as("postgres").query<{ tablename: string; tableowner: string }>(
      `SELECT tablename, tableowner FROM pg_tables
        WHERE schemaname = 'ingest' AND tablename = ANY($1)
        ORDER BY tablename`,
      [tables],
    );
    expect(r.rows).toEqual(
      tables.map((tablename) => ({ tablename, tableowner: "cg_ingest_owner" })),
    );
  });

  test("cg_ingest: no SELECT on the api's users, forensic_key and audit_log", async () => {
    const ingest = as("cg_ingest", "ingest");
    for (const t of ["users", "forensic_key", "audit_log"])
      await refused(ingest, `SELECT 1 FROM api.${t}`);
  });

  // SPEC-025 renew_ac_013 (the grants): ingest's runtime role writes the agents' columns its
  // code writes, and never a revocation, nor a copy signal at the insert.
  test("cg_ingest: on agents, last_seen written, revoked_at never, nor revocation or signal columns at insert (renew_ac_013)", async () => {
    const ingest = as("cg_ingest", "ingest");
    const agentId = "01934abc-def0-7000-89ab-0000000a0d13";
    await allowed(
      ingest,
      `INSERT INTO agents (agent_id, org_id, pubkey, cert_pem, enrolled_at, expires_at, hostname, platform, agent_version)
       VALUES ('${agentId}', 'default', '\\x00', 'c', now(), now() + interval '1 day', 'h', 'p', 'v')`,
    );
    try {
      await allowed(ingest, `UPDATE agents SET last_seen = now() WHERE agent_id = '${agentId}'`);
      // The copy signal it may write is written once: the owner's trigger binds the role too.
      await allowed(
        ingest,
        `UPDATE agents SET duplicate_signal = 'superseded_certificate', duplicate_signal_at = now() WHERE agent_id = '${agentId}'`,
      );
      const again = await ingest
        .query(
          `UPDATE agents SET duplicate_signal = 'interleaved_process' WHERE agent_id = '${agentId}'`,
        )
        .then(
          () => null,
          (e: unknown) => e as { code?: string },
        );
      expect(again?.code).toBe("P0001");
      await refused(
        ingest,
        `UPDATE agents SET revoked_at = now(), revocation_reason = 'r' WHERE agent_id = '${agentId}'`,
      );
      await refused(
        ingest,
        `UPDATE agents SET revocation_reason = 'r' WHERE agent_id = '${agentId}'`,
      );
      for (const [column, value] of [
        ["revoked_at", "now()"],
        ["revocation_reason", "'r'"],
        ["duplicate_signal", "'superseded_certificate'"],
      ] as const) {
        await refused(
          ingest,
          `INSERT INTO agents (agent_id, org_id, pubkey, cert_pem, enrolled_at, expires_at, ${column})
           VALUES (gen_random_uuid(), 'default', '\\x00', 'c', now(), now() + interval '1 day', ${value})`,
        );
      }
    } finally {
      await as("postgres").query(`DELETE FROM ingest.agents WHERE agent_id = '${agentId}'`);
    }
  });

  test("cg_ingest: INSERT into ca, and neither UPDATE nor DELETE on it", async () => {
    const ingest = as("cg_ingest", "ingest");
    await allowed(ingest, "INSERT INTO ca (id, cert_pem, private_key) VALUES (1, 'x', '\\x00')");
    await refused(ingest, "UPDATE ca SET cert_pem = 'y'");
    await refused(ingest, "DELETE FROM ca");
    await as("postgres").query("DELETE FROM ingest.ca");
  });

  test.each([["ingest"], ["api"], ["public"]])(
    "cg_ingest and cg_api create nothing, and alter nothing, in %s",
    async (schema) => {
      const table = {
        ingest: "ingest.agents",
        api: "api.users",
        public: "public.deploy_004_probe",
      }[schema as "ingest" | "api" | "public"];
      for (const role of RUNTIME) {
        const pool = as(role, role === "cg_ingest" ? "ingest" : "api");
        await refused(pool, `CREATE TABLE ${schema}.deploy_004_t (x int)`);
        await refused(
          pool,
          `CREATE FUNCTION ${schema}.deploy_004_f() RETURNS int LANGUAGE sql AS 'SELECT 1'`,
        );
        await refused(pool, `CREATE VIEW ${schema}.deploy_004_v AS SELECT 1 AS x`);
        await refused(pool, `CREATE POLICY deploy_004_p ON ${table} USING (true)`);
        await refused(pool, `ALTER TABLE ${table} ADD COLUMN deploy_004_c int`);
      }
    },
  );

  test("cg_api: no SELECT on ca", async () => {
    await refused(as("cg_api", "api"), "SELECT 1 FROM ingest.ca");
  });

  test("cg_api: SELECT on alerts, incidents and playbook_executions, and no write", async () => {
    const api = as("cg_api", "api");
    for (const t of ["alerts", "incidents", "playbook_executions"]) {
      await allowed(api, `SELECT 1 FROM ingest.${t}`);
      await refused(api, `INSERT INTO ingest.${t} DEFAULT VALUES`);
      await refused(api, `UPDATE ingest.${t} SET org_id = org_id`);
      await refused(api, `DELETE FROM ingest.${t}`);
    }
  });

  // SPEC-026 deploy_ac_004's rows for migration 0011_dashboard_rights: the api's column
  // grants on ingest's tables, its read of the triage history, and ingest's own UPDATE on
  // incidents narrowed to the columns grouping writes.

  /** An agent, an alert and an incident of their own, written as the superuser. */
  async function seedIngestRows(): Promise<{
    agentId: string;
    alertId: string;
    incidentId: string;
  }> {
    const agentId = globalThis.crypto.randomUUID();
    const alertId = globalThis.crypto.randomUUID();
    const incidentId = globalThis.crypto.randomUUID();
    const owner = as("postgres");
    await owner.query(
      `INSERT INTO ingest.agents (agent_id, org_id, pubkey, cert_pem, expires_at)
       VALUES ($1, 'deploy-004', '\\x00', 'c', now() + interval '1 day')`,
      [agentId],
    );
    await owner.query(
      `INSERT INTO ingest.alerts
         (alert_id, org_id, agent_id, title, severity_id, cg_detection_source, rule_id,
          source_events, heuristic_score, final_score, cg_mitre, dedup_key, event_time)
       VALUES ($1, 'deploy-004', $2, 't', 4, 'rule', 'rule.x', ARRAY[$1]::uuid[], 0.9, 0.9,
               '{"tactics":["execution"],"techniques":["T1059.001"]}', $3, now())`,
      [alertId, agentId, `dedup::${alertId}`],
    );
    await owner.query(
      `INSERT INTO ingest.incidents
         (incident_id, org_id, agent_id, title, cg_mitre, alert_ids, severity_id, grouping_key,
          window_start)
       VALUES ($1, 'deploy-004', $2, 't', '{"tactics":["execution"],"techniques":["T1059.001"]}',
               ARRAY[$3]::uuid[], 4, $4, now())`,
      [incidentId, agentId, alertId, `deploy-004::${incidentId}`],
    );
    return { agentId, alertId, incidentId };
  }

  test("cg_api: on enrollment_tokens, INSERT of a new token's columns, SELECT but of its hash, UPDATE of withdrawn_at", async () => {
    const api = as("cg_api", "api");
    const tokenId = "01934abc-def0-7000-89ab-0000000a0d16";
    const hash = `\\x${"ab".repeat(32)}`;
    try {
      await allowed(
        api,
        `INSERT INTO ingest.enrollment_tokens (token_id, token_sha256, org_id, scope, expires_at, max_uses)
         VALUES ('${tokenId}', '${hash}', 'deploy-004', 'enroll', now() + interval '10 minutes', 1)`,
      );
      await refused(
        api,
        `INSERT INTO ingest.enrollment_tokens (token_id, token_sha256, org_id, expires_at, uses)
         VALUES (gen_random_uuid(), '\\x${"cd".repeat(32)}', 'deploy-004', now() + interval '1 minute', 0)`,
      );
      await allowed(
        api,
        `SELECT token_id, org_id, scope, state, issued_at, expires_at, consumed_at, max_uses, uses,
                withdrawn_at
           FROM ingest.enrollment_tokens WHERE token_id = '${tokenId}'`,
      );
      await refused(api, "SELECT token_sha256 FROM ingest.enrollment_tokens");
      await refused(api, "SELECT * FROM ingest.enrollment_tokens");
      await allowed(
        api,
        `UPDATE ingest.enrollment_tokens SET withdrawn_at = now() WHERE token_id = '${tokenId}'`,
      );
      await refused(api, "UPDATE ingest.enrollment_tokens SET max_uses = 2");
      await refused(api, "UPDATE ingest.enrollment_tokens SET uses = 1");
      await refused(api, "DELETE FROM ingest.enrollment_tokens");
    } finally {
      await as("postgres").query("DELETE FROM ingest.enrollment_tokens WHERE token_id = $1", [
        tokenId,
      ]);
    }
  });

  test("cg_api: on agents, SELECT of the inventory's columns, not the key or certificates, UPDATE of the revocation alone", async () => {
    const api = as("cg_api", "api");
    const { agentId, alertId, incidentId } = await seedIngestRows();
    try {
      await allowed(
        api,
        `SELECT agent_id, org_id, enrolled_at, expires_at, last_seen, enrolled_by_token_id, hostname,
                platform, agent_version, revoked_at, revocation_reason, duplicate_signal,
                duplicate_signal_at
           FROM ingest.agents WHERE agent_id = '${agentId}'`,
      );
      for (const column of [
        "pubkey",
        "cert_pem",
        "pending_cert_pem",
        "process_started_at",
        "previous_process_started_at",
      ]) {
        await refused(api, `SELECT ${column} FROM ingest.agents`);
      }
      await refused(
        api,
        `UPDATE ingest.agents SET last_seen = now() WHERE agent_id = '${agentId}'`,
      );
      await refused(api, "UPDATE ingest.agents SET cert_pem = 'x'");
      await allowed(
        api,
        `UPDATE ingest.agents SET revoked_at = now(), revocation_reason = 'deploy-004'
          WHERE agent_id = '${agentId}'`,
      );
      await refused(
        api,
        `INSERT INTO ingest.agents (agent_id, org_id, pubkey, cert_pem, expires_at)
         VALUES (gen_random_uuid(), 'deploy-004', '\\x00', 'c', now())`,
      );
      await refused(api, "DELETE FROM ingest.agents");
    } finally {
      const owner = as("postgres");
      await owner.query("DELETE FROM ingest.incidents WHERE incident_id = $1", [incidentId]);
      await owner.query("DELETE FROM ingest.alerts WHERE alert_id = $1", [alertId]);
      await owner.query("DELETE FROM ingest.agents WHERE agent_id = $1", [agentId]);
    }
  });

  test("cg_api: on incidents and alerts, UPDATE of the triage columns alone; each change in the history as cg_api", async () => {
    const api = as("cg_api", "api");
    const { agentId, alertId, incidentId } = await seedIngestRows();
    try {
      await allowed(
        api,
        `UPDATE ingest.incidents SET status = 'acknowledged', assigned_to = 'u-deploy-004',
                updated_at = now()
          WHERE incident_id = '${incidentId}'`,
      );
      for (const set of [
        "alert_ids = alert_ids",
        "cg_mitre = cg_mitre",
        "severity_id = 1",
        "title = 'x'",
        "activity_id = 2",
      ]) {
        await refused(api, `UPDATE ingest.incidents SET ${set}`);
      }
      await allowed(
        api,
        `UPDATE ingest.alerts SET status = 'acknowledged', updated_at = now()
          WHERE alert_id = '${alertId}'`,
      );
      for (const set of ["source_events = source_events", "final_score = 0", "cg_mitre = NULL"]) {
        await refused(api, `UPDATE ingest.alerts SET ${set}`);
      }
      const history = await as("postgres").query<{
        object_kind: string;
        field: string;
        new_value: string;
        changed_by_role: string;
      }>(
        `SELECT object_kind, field, new_value, changed_by_role FROM ingest.triage_history
          WHERE object_id = ANY($1::uuid[]) ORDER BY history_id`,
        [[incidentId, alertId]],
      );
      expect(history.rows).toEqual([
        {
          object_kind: "incident",
          field: "status",
          new_value: "acknowledged",
          changed_by_role: "cg_api",
        },
        {
          object_kind: "incident",
          field: "assigned_to",
          new_value: "u-deploy-004",
          changed_by_role: "cg_api",
        },
        {
          object_kind: "alert",
          field: "status",
          new_value: "acknowledged",
          changed_by_role: "cg_api",
        },
      ]);
    } finally {
      const owner = as("postgres");
      await owner.query("DELETE FROM ingest.incidents WHERE incident_id = $1", [incidentId]);
      await owner.query("DELETE FROM ingest.alerts WHERE alert_id = $1", [alertId]);
      await owner.query("DELETE FROM ingest.agents WHERE agent_id = $1", [agentId]);
    }
  });

  test("cg_api: SELECT on triage_history, and no INSERT, UPDATE or DELETE; cg_ingest: no right on it", async () => {
    const api = as("cg_api", "api");
    await allowed(api, "SELECT * FROM ingest.triage_history");
    const insert = `INSERT INTO ingest.triage_history
        (org_id, object_kind, object_id, field, old_value, new_value, changed_at, changed_by_role)
      VALUES ('deploy-004', 'incident', gen_random_uuid(), 'status', 'open', 'resolved', now(), 'x')`;
    await refused(api, insert);
    await refused(api, "UPDATE ingest.triage_history SET new_value = 'open'");
    await refused(api, "DELETE FROM ingest.triage_history");
    const ingest = as("cg_ingest", "ingest");
    await refused(ingest, "SELECT 1 FROM ingest.triage_history");
    await refused(ingest, insert);
  });

  test("cg_ingest: on incidents, UPDATE of grouping's columns, not of the status or the assignee", async () => {
    const ingest = as("cg_ingest", "ingest");
    const { agentId, alertId, incidentId } = await seedIngestRows();
    try {
      await allowed(
        ingest,
        `UPDATE incidents SET alert_ids = alert_ids, severity_id = 5, cg_mitre = cg_mitre,
                updated_at = now()
          WHERE incident_id = '${incidentId}'`,
      );
      await refused(
        ingest,
        `UPDATE incidents SET status = 'resolved' WHERE incident_id = '${incidentId}'`,
      );
      await refused(ingest, "UPDATE incidents SET assigned_to = 'x'");
    } finally {
      const owner = as("postgres");
      await owner.query("DELETE FROM ingest.incidents WHERE incident_id = $1", [incidentId]);
      await owner.query("DELETE FROM ingest.alerts WHERE alert_id = $1", [alertId]);
      await owner.query("DELETE FROM ingest.agents WHERE agent_id = $1", [agentId]);
    }
  });

  // SPEC-026 deploy_ac_004's rows: `SELECT` on audit_log, for the audit view, besides its INSERT.
  test("cg_api: INSERT into audit_log and forensic_key; neither UPDATE nor DELETE; SELECT on audit_log", async () => {
    const api = as("cg_api", "api");
    await allowed(api, "INSERT INTO audit_log (action, outcome) VALUES ('deploy.004', 'ok')");
    await allowed(
      api,
      "INSERT INTO forensic_key (id, private_key, public_key) VALUES (1, '\\x00', '\\x00')",
    );
    // Constants, so that the UPDATE needs its own right and not SELECT's too.
    await refused(api, "UPDATE audit_log SET action = 'x'");
    await refused(api, "UPDATE forensic_key SET created_at = now()");
    for (const t of ["audit_log", "forensic_key"]) await refused(api, `DELETE FROM ${t}`);
    await allowed(api, "SELECT id, occurred_at, user_id, action, detail FROM audit_log");
    await as("postgres").query("DELETE FROM api.forensic_key; DELETE FROM api.audit_log");
  });

  // SPEC-026 deploy_ac_004's rows: on invitations, SELECT, the INSERT of the columns the api
  // writes and the UPDATE of the secret and the end; no DELETE, and issued_at the database's.
  test("cg_api: on invitations, SELECT, INSERT and UPDATE of its columns, and no DELETE", async () => {
    const api = as("cg_api", "api");
    const userId = "01934abc-def0-7000-89ab-0000000a0d14";
    const invitationId = "01934abc-def0-7000-89ab-0000000a0d15";
    await as("postgres").query(
      `INSERT INTO api.users (user_id, org_id, email, password_hash, totp_secret, totp_enrolled, role)
       VALUES ($1, 'default', 'invited@deploy004.test', NULL, NULL, false, 'viewer')`,
      [userId],
    );
    try {
      await allowed(
        api,
        `INSERT INTO invitations (invitation_id, user_id, org_id, purpose, token_sha256, expires_at)
         VALUES ('${invitationId}', '${userId}', 'default', 'invite', '\\x01', now() + interval '72 hours')`,
      );
      await allowed(api, "SELECT invitation_id, issued_at, pending_totp_secret FROM invitations");
      await allowed(
        api,
        `UPDATE invitations SET pending_totp_secret = '\\x02' WHERE invitation_id = '${invitationId}'`,
      );
      await allowed(
        api,
        `UPDATE invitations SET withdrawn_at = now(), pending_totp_secret = NULL WHERE invitation_id = '${invitationId}'`,
      );
      await allowed(
        api,
        `UPDATE invitations SET completed_at = NULL WHERE invitation_id = '${invitationId}'`,
      );
      await refused(
        api,
        `INSERT INTO invitations (invitation_id, user_id, org_id, purpose, token_sha256, issued_at, expires_at)
         VALUES (gen_random_uuid(), '${userId}', 'default', 'invite', '\\x03', now(), now() + interval '1 hour')`,
      );
      for (const set of [
        "token_sha256 = '\\x04'",
        "expires_at = now()",
        "issued_at = now()",
        "user_id = user_id",
        "purpose = 'reset'",
      ]) {
        await refused(api, `UPDATE invitations SET ${set}`);
      }
      await refused(api, "DELETE FROM invitations");
    } finally {
      await as("postgres").query("DELETE FROM api.invitations");
      await as("postgres").query("DELETE FROM api.users WHERE user_id = $1", [userId]);
    }
  });

  // 0008_dashboard run again over its own schema, cg_api present, changes none of its rights.
  test("0008_dashboard run again as cg_api_owner leaves cg_api's rights as they were", async () => {
    const rights = async () =>
      (
        await as("postgres").query<{ t: string; c: string | null; p: string }>(
          `SELECT table_name AS t, NULL AS c, privilege_type AS p FROM information_schema.table_privileges
            WHERE grantee = 'cg_api' AND table_schema = 'api'
           UNION ALL
           SELECT table_name, column_name, privilege_type FROM information_schema.column_privileges
            WHERE grantee = 'cg_api' AND table_schema = 'api'
           ORDER BY 1, 2, 3`,
        )
      ).rows;
    const before = await rights();
    expect(before).toContainEqual({ t: "invitations", c: "withdrawn_at", p: "UPDATE" });
    const removed = await as("postgres").query(
      "DELETE FROM api.api_kysely_migration WHERE name = '0008_dashboard'",
    );
    expect(removed.rowCount).toBe(1);
    await runMigrations({ API_PG_URL: url("cg_api_owner", "api") });
    expect(await rights()).toEqual(before);
  });

  test("either runtime role: no temporary table, and its migration ledger neither read nor written", async () => {
    await refused(as("cg_ingest", "ingest"), "CREATE TEMPORARY TABLE deploy_004_tmp (x int)");
    await refused(as("cg_api", "api"), "CREATE TEMPORARY TABLE deploy_004_tmp (x int)");
    const ledgers: [(typeof RUNTIME)[number], string, string][] = [
      ["cg_ingest", "ingest", "kysely_migration"],
      ["cg_ingest", "ingest", "kysely_migration_lock"],
      ["cg_api", "api", "api_kysely_migration"],
      ["cg_api", "api", "api_kysely_migration_lock"],
    ];
    for (const [role, schema, t] of ledgers) {
      const pool = as(role, schema as "ingest" | "api");
      await refused(pool, `SELECT 1 FROM ${schema}.${t}`);
      await refused(pool, `INSERT INTO ${schema}.${t} DEFAULT VALUES`);
      // Kysely's ledger: the migrations' names, and the lock row's flag.
      const column = t.endsWith("_lock") ? "is_locked = 0" : "name = 'x'";
      await refused(pool, `UPDATE ${schema}.${t} SET ${column}`);
      await refused(pool, `DELETE FROM ${schema}.${t}`);
    }
  });
});

describe("deploy_ac_005 — Postgres logging", () => {
  test("a statement logged at log_statement = all is logged without its parameters", async () => {
    const marker = `deploy-005-marker-${globalThis.crypto.randomUUID()}`;
    const client = new pg.Client({ connectionString: url("postgres") });
    await client.connect();
    try {
      await client.query("SET log_statement = 'all'");
      await client.query("SELECT $1::text AS deploy_005", [marker]);
    } finally {
      await client.end();
    }
    const logs = await new Promise<string>((resolve, reject) => {
      container
        .logs()
        .then((stream) => {
          let text = "";
          stream.on("data", (d: Buffer | string) => {
            text += d.toString();
          });
          setTimeout(() => {
            stream.destroy();
            resolve(text);
          }, 1500);
        })
        .catch(reject);
    });
    expect(logs).toContain("SELECT $1::text AS deploy_005");
    expect(logs).not.toContain(marker);
  });
});

describe("the api's code runs as cg_api", () => {
  let app: FastifyInstance;
  let close: () => Promise<void>;
  let runtime: Config;
  /** The suite's own stores but this Postgres, as the superuser, for the fixtures. */
  let fixtures: Config;

  beforeAll(async () => {
    runtime = { ...suite, API_PG_URL: url("cg_api", "api"), API_RUN_MIGRATIONS: false };
    fixtures = { ...suite, API_PG_URL: url("postgres", "api") };
    // The forensic key's first load: INSERT and SELECT on forensic_key.
    const services = await buildServices(runtime);
    app = buildApp(services);
    await app.ready();
    close = async () => {
      await app.close();
      await services.close();
    };
  });
  afterAll(async () => {
    await close?.();
  });

  test("the bootstrap command writes a user and its audit row", async () => {
    const created = await createUserFromCli(runtime, {
      email: "boot@deploy004.test",
      password: "Valid-passw0rd-004",
      role: "admin",
    });
    const rows = await as("postgres").query<{ n: string }>(
      "SELECT count(*) AS n FROM api.audit_log WHERE action = 'user.create' AND target_id = $1",
      [created.userId],
    );
    expect(Number(rows.rows[0]?.n)).toBe(1);
  });

  test("a sign-in reads the user, records it and opens a session", async () => {
    await insertUserDirect(fixtures, {
      email: "login@deploy004.test",
      role: "analyst",
      totpEnrolled: true,
    });
    const res = await app.inject({
      method: "POST",
      url: "/v1/auth/login",
      remoteAddress: "198.18.4.1",
      payload: {
        email: "login@deploy004.test",
        password: DEFAULT_PASSWORD,
        totp_code: currentTotp(),
      },
    });
    expect(res.statusCode, res.body).toBe(200);
  });

  test("the read routes and the forensic export read ingest's tables", async () => {
    const org = "deploy-004";
    const agentId = globalThis.crypto.randomUUID();
    const alertId = globalThis.crypto.randomUUID();
    const incidentId = globalThis.crypto.randomUUID();
    const eventId = globalThis.crypto.randomUUID();
    await ensureAgent(fixtures, agentId, org);
    await insertCgesEvent(suite, {
      agentId,
      eventId,
      orgId: org,
      time: "2026-10-23 09:00:00.000000000",
    });
    await insertAlertRow(fixtures, { alertId, agentId, orgId: org, sourceEvents: [eventId] });
    await insertIncidentRow(fixtures, { incidentId, agentId, orgId: org, alertIds: [alertId] });
    await seedSession(runtime, {
      token: "tok-deploy-004",
      userId: globalThis.crypto.randomUUID(),
      role: "analyst",
      orgId: org,
    });
    const cookies = { cgsess: "tok-deploy-004" };
    const list = await app.inject({ method: "GET", url: "/v1/incidents", cookies });
    expect(list.statusCode, list.body).toBe(200);
    const detail = await app.inject({ method: "GET", url: `/v1/incidents/${incidentId}`, cookies });
    expect(detail.statusCode, detail.body).toBe(200);
    const exported = await app.inject({
      method: "GET",
      url: `/v1/incidents/${incidentId}/forensic-export`,
      cookies,
    });
    expect(exported.statusCode, exported.body).toBe(200);
  });
});
