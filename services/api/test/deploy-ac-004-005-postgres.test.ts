import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import type { FastifyInstance } from "fastify";
import pg from "pg";
import { GenericContainer, type StartedTestContainer, Wait } from "testcontainers";
import { afterAll, beforeAll, describe, expect, inject, test } from "vitest";
import { buildApp } from "../src/app.js";
import { createUserFromCli } from "../src/cli/create-user.js";
import type { Config } from "../src/config.js";
import { runMigrations } from "../src/db/migrate.js";
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
// Until the bundle's tests drive its compose (deploy-ci.yml), these run here, against a
// Postgres started with the bundle's own initialisation files.

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
            + (SELECT count(*) FROM pg_proc WHERE proowner IN (SELECT oid FROM pg_roles WHERE rolname IN ('cg_ingest', 'cg_api'))) AS n`,
    );
    expect(Number(r.rows[0]?.n)).toBe(0);
  });

  test("each ingest table the api reads is owned by cg_ingest_owner, in the schema ingest", async () => {
    const r = await as("postgres").query<{ tablename: string; tableowner: string }>(
      `SELECT tablename, tableowner FROM pg_tables
        WHERE schemaname = 'ingest' AND tablename IN ('alerts', 'incidents', 'playbook_executions')
        ORDER BY tablename`,
    );
    expect(r.rows).toEqual([
      { tablename: "alerts", tableowner: "cg_ingest_owner" },
      { tablename: "incidents", tableowner: "cg_ingest_owner" },
      { tablename: "playbook_executions", tableowner: "cg_ingest_owner" },
    ]);
  });

  test("cg_ingest: no SELECT on the api's users, forensic_key and audit_log", async () => {
    const ingest = as("cg_ingest", "ingest");
    for (const t of ["users", "forensic_key", "audit_log"])
      await refused(ingest, `SELECT 1 FROM api.${t}`);
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

  test("cg_api: no SELECT on ca, enrollment_tokens and agents", async () => {
    const api = as("cg_api", "api");
    for (const t of ["ca", "enrollment_tokens", "agents"])
      await refused(api, `SELECT 1 FROM ingest.${t}`);
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

  test("cg_api: INSERT into audit_log and forensic_key; neither UPDATE nor DELETE; no SELECT on audit_log", async () => {
    const api = as("cg_api", "api");
    await allowed(api, "INSERT INTO audit_log (action, outcome) VALUES ('deploy.004', 'ok')");
    await allowed(
      api,
      "INSERT INTO forensic_key (id, private_key, public_key) VALUES (1, '\\x00', '\\x00')",
    );
    for (const [t, column] of [
      ["audit_log", "action"],
      ["forensic_key", "created_at"],
    ]) {
      await refused(api, `UPDATE ${t} SET ${column} = ${column}`);
      await refused(api, `DELETE FROM ${t}`);
    }
    await refused(api, "SELECT 1 FROM audit_log");
    await as("postgres").query("DELETE FROM api.forensic_key; DELETE FROM api.audit_log");
  });

  test("either runtime role: no temporary table, and its migration ledger neither read nor written", async () => {
    await refused(as("cg_ingest", "ingest"), "CREATE TEMPORARY TABLE deploy_004_tmp (x int)");
    await refused(as("cg_api", "api"), "CREATE TEMPORARY TABLE deploy_004_tmp (x int)");
    for (const t of ["kysely_migration", "kysely_migration_lock"]) {
      await refused(as("cg_ingest", "ingest"), `SELECT 1 FROM ingest.${t}`);
      await refused(as("cg_ingest", "ingest"), `DELETE FROM ingest.${t}`);
    }
    for (const t of ["api_kysely_migration", "api_kysely_migration_lock"]) {
      await refused(as("cg_api", "api"), `SELECT 1 FROM api.${t}`);
      await refused(as("cg_api", "api"), `DELETE FROM api.${t}`);
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
