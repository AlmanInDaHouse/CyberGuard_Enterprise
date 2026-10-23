import { spawnSync } from "node:child_process";
import type { AddressInfo } from "node:net";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import pg from "pg";
import { GenericContainer, type StartedTestContainer, Wait } from "testcontainers";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import { buildEnrollApp } from "../src/app.js";
import { type ServerCa, ensureCa } from "../src/ca.js";
import type { Config } from "../src/config.js";
import { runPgMigrations } from "../src/db/migrate.js";
import { runDetectionCycle } from "../src/detect/index.js";
import type { Services } from "../src/services.js";
import { insertCgesEvent } from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";
import { POWERSHELL, WINWORD, soarHarness, testPlaybook } from "./helpers/soar.js";
import { enroll } from "./helpers/test-client.js";

// SPEC-023 §Operational §4 — ingest's code runs as `cg_ingest`, its runtime role, with the
// rights ingest's migrations grant and no other: the CA at the first start, a token issued
// by the command, an agent enrolled through the enroll route, a heartbeat's statements, and
// a detection cycle that writes an alert, an incident, its cursor and a playbook's
// execution. Against a Postgres started with the bundle's own initialisation
// (deploy/docker/postgres/), ingest's migrations run as `cg_ingest_owner`.

const PACKAGE_DIR = join(dirname(fileURLToPath(import.meta.url)), "..");
const INIT = join(PACKAGE_DIR, "..", "..", "deploy", "docker", "postgres");
const PASSWORDS = {
  postgres: "runtime-superuser",
  cg_ingest_owner: "runtime-ingest-owner",
  cg_ingest: "runtime-ingest",
  cg_api_owner: "runtime-api-owner",
  cg_api: "runtime-api",
} as const;
const ORG = "runtime-role";

let container: StartedTestContainer;
let suite: Config;
let runtime: Config;
let runtimePool: pg.Pool;
let superPool: pg.Pool;
let ca: ServerCa;
let agentId: string;

function url(user: keyof typeof PASSWORDS, schema?: string): string {
  const base = `postgres://${user}:${PASSWORDS[user]}@${container.getHost()}:${container.getMappedPort(5432)}/cyberguard`;
  return schema ? `${base}?options=-c%20search_path%3D${schema}%2Cpublic` : base;
}

beforeAll(async () => {
  suite = inject("ingestConfig");
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
  expect(await runPgMigrations({ INGEST_PG_URL: url("cg_ingest_owner", "ingest") })).toBe(true);
  // The suite's ClickHouse and Redis; this Postgres, as the runtime role.
  runtime = { ...suite, INGEST_PG_URL: url("cg_ingest", "ingest") };
  runtimePool = new pg.Pool({ connectionString: runtime.INGEST_PG_URL });
  superPool = new pg.Pool({ connectionString: url("postgres", "ingest") });
  // The container's stop ends a backend a pool's end left closing.
  runtimePool.on("error", () => {});
  superPool.on("error", () => {});
}, 240_000);

afterAll(async () => {
  await runtimePool?.end();
  await superPool?.end();
  await container?.stop();
});

test("the CA is generated at the first start and loaded at the next", async () => {
  ca = await ensureCa(runtimePool, runtime.INGEST_CA_PASSPHRASE);
  const again = await ensureCa(runtimePool, runtime.INGEST_CA_PASSPHRASE);
  expect(again.caCertPem).toBe(ca.caCertPem);
});

test("issue-token.js issues a token, and the enroll route enrolls an agent with it", async () => {
  const issued = spawnSync(process.execPath, ["--import", "tsx", "src/cli/issue-token.ts"], {
    cwd: PACKAGE_DIR,
    encoding: "utf8",
    env: { ...process.env, INGEST_PG_URL: runtime.INGEST_PG_URL },
    timeout: 60_000,
  });
  expect(issued.status, issued.stderr).toBe(0);
  const token = issued.stdout.trim();

  const app = buildEnrollApp({ pool: runtimePool, ca } as unknown as Services);
  await app.listen({ host: "127.0.0.1", port: 0 });
  try {
    const port = (app.server.address() as AddressInfo).port;
    const identity = await enroll(`http://127.0.0.1:${port}`, token);
    agentId = identity.agentId;
  } finally {
    await app.close();
  }
  const row = await superPool.query("SELECT org_id FROM agents WHERE agent_id = $1", [agentId]);
  expect(row.rows).toEqual([{ org_id: "default" }]);
});

test("a heartbeat's statements: the agent's key read, its last_seen written", async () => {
  // The two statements of the heartbeat route (routes/heartbeat.ts), as the role.
  const key = await runtimePool.query("SELECT pubkey FROM agents WHERE agent_id = $1", [agentId]);
  expect(key.rows).toHaveLength(1);
  await runtimePool.query("UPDATE agents SET last_seen = now() WHERE agent_id = $1", [agentId]);
});

test("a detection cycle writes an alert, an incident, its cursor and a playbook's execution", async () => {
  await superPool.query("UPDATE agents SET org_id = $1 WHERE agent_id = $2", [ORG, agentId]);
  await insertCgesEvent(suite, {
    agentId,
    orgId: ORG,
    eventId: globalThis.crypto.randomUUID(),
    activityId: 1,
    processPid: 9100,
    processName: "winword.exe",
    imageFileName: WINWORD,
    time: "2026-10-23 10:00:00.000000000",
  });
  await insertCgesEvent(suite, {
    agentId,
    orgId: ORG,
    eventId: globalThis.crypto.randomUUID(),
    activityId: 1,
    processPid: 9101,
    processName: "powershell.exe",
    imageFileName: POWERSHELL,
    processParentPid: 9100,
    time: "2026-10-23 10:00:01.000000000",
  });
  const harness = soarHarness(runtime, [
    testPlaybook({ steps: [{ id: "isolate", action: "agent.isolate", mode: "dry_run" }] }),
  ]);
  try {
    await runDetectionCycle(detectConfig(runtime, ORG), undefined, harness.soar);
  } finally {
    await harness.close();
  }
  const counts = await superPool.query<{
    alerts: string;
    incidents: string;
    cursors: string;
    runs: string;
  }>(
    `SELECT (SELECT count(*) FROM alerts WHERE agent_id = $1) AS alerts,
            (SELECT count(*) FROM incidents WHERE org_id = $2) AS incidents,
            (SELECT count(*) FROM detect_watermark WHERE org_id = $2) AS cursors,
            (SELECT count(*) FROM playbook_executions WHERE org_id = $2 AND status <> 'running') AS runs`,
    [agentId, ORG],
  );
  expect(counts.rows[0]).toEqual({ alerts: "1", incidents: "1", cursors: "1", runs: "1" });
});
