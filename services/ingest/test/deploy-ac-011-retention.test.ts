import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import pg from "pg";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import {
  RETENTION_DEFAULTS,
  type Retention,
  bootstrapClickHouseTables,
} from "../src/db/clickhouse-bootstrap.js";
import { runMigrations } from "../src/db/migrate.js";
import {
  createChDatabase,
  createPgDatabase,
  dropChDatabase,
  dropPgDatabase,
  pgDatabaseUrl,
  withChClient,
} from "./helpers/throwaway.js";

// deploy_ac_011 — SPEC-023 §Operational §9, ingest's half: the migration step sets on
// cges_events a time-to-live that deletes a row of class 1007, 4001 or 3002 the set number
// of days after its arrival, and a row of another class at the longest of the three, and on
// heartbeats one of INGEST_RETENTION_HEARTBEAT_DAYS; a run with other values sets the new
// ones, and a row stored before whose arrival is past its new period is gone once the table
// has merged; a value of 6 or 3651 stops the migrate command before it sets a time-to-live,
// and stops ingest, each with the variable named. SPEC-020's auth_ac_003 is restated in
// auth-ac-003-storage-retention.test.ts.

const PACKAGE_DIR = join(dirname(fileURLToPath(import.meta.url)), "..");
const DB = "deploy_ac_011";

let config: Config;

beforeAll(async () => {
  config = inject("ingestConfig");
  await createChDatabase(config, DB);
});
afterAll(async () => {
  await dropChDatabase(config, DB);
});

function retention(
  processDays: number,
  networkDays: number,
  logonDays: number,
  heartbeatDays: number,
) {
  return {
    INGEST_RETENTION_PROCESS_DAYS: processDays,
    INGEST_RETENTION_NETWORK_DAYS: networkDays,
    INGEST_RETENTION_LOGON_DAYS: logonDays,
    INGEST_RETENTION_HEARTBEAT_DAYS: heartbeatDays,
  };
}

async function migrate(days: ReturnType<typeof retention>): Promise<void> {
  await runMigrations({ ...config, INGEST_CH_DB: DB, ...days });
}

async function showCreate(table: string): Promise<string> {
  return withChClient(config, DB, async (ch) => {
    const rs = await ch.query({ query: `SHOW CREATE TABLE ${table}`, format: "TabSeparatedRaw" });
    return rs.text();
  });
}

/** The days the cges_events time-to-live gives `where`, as ClickHouse prints it. */
function ttlDays(create: string, where: string): number | null {
  const escaped = where.replace(/[()]/g, "\\$&");
  const m = new RegExp(
    `toDateTime\\(arrived_at\\) \\+ toIntervalDay\\((\\d+)\\)( DELETE)? WHERE ${escaped}`,
  ).exec(create);
  return m ? Number(m[1]) : null;
}

function cgesTtl(create: string) {
  return {
    process: ttlDays(create, "class_uid = 1007"),
    network: ttlDays(create, "class_uid = 4001"),
    logon: ttlDays(create, "class_uid = 3002"),
    other: ttlDays(create, "class_uid NOT IN (1007, 4001, 3002)"),
  };
}

function heartbeatTtl(create: string): number | null {
  const m = /TTL toDateTime\(arrived_at\) \+ toIntervalDay\((\d+)\)/.exec(create);
  return m ? Number(m[1]) : null;
}

/** A minimal cges_events row of `classUid` whose arrival is `ageDays` days old. */
function row(classUid: number, ageDays: number): Record<string, unknown> {
  const arrived = new Date(Date.now() - ageDays * 24 * 3600 * 1000);
  return {
    agent_id: "01934abc-def0-7000-89ab-000000000011",
    event_id: globalThis.crypto.randomUUID(),
    class_uid: classUid,
    activity_id: 1,
    process_pid: 0,
    process_uid: "",
    process_name: "",
    time: "2026-10-10 12:00:00.000000000",
    arrived_at: arrived.toISOString().replace("T", " ").replace("Z", ""),
  };
}

async function present(ids: string[]): Promise<string[]> {
  return withChClient(config, DB, async (ch) => {
    const rs = await ch.query({
      query:
        "SELECT toString(event_id) AS id FROM cges_events WHERE event_id IN ({ids:Array(UUID)})",
      query_params: { ids },
      format: "JSONEachRow",
    });
    return (await rs.json<{ id: string }>()).map((r) => r.id).sort();
  });
}

test("deploy_ac_011: the four settings give each class its time-to-live, and heartbeats theirs", async () => {
  await migrate(retention(7, 8, 9, 10));
  expect(cgesTtl(await showCreate("cges_events"))).toEqual({
    process: 7,
    network: 8,
    logon: 9,
    other: 9,
  });
  expect(heartbeatTtl(await showCreate("heartbeats"))).toBe(10);
});

test("deploy_ac_011: a row of another class keeps the longest of the three, whichever it is", async () => {
  await migrate(retention(40, 9, 8, 10));
  expect(cgesTtl(await showCreate("cges_events")).other).toBe(40);
  await migrate(retention(8, 41, 9, 10));
  expect(cgesTtl(await showCreate("cges_events")).other).toBe(41);
});

test("deploy_ac_011: the bootstrap itself refuses a period out of bounds, before any statement", async () => {
  const fresh = `${DB}_refused`;
  await createChDatabase(config, fresh);
  try {
    const refused: [Retention, string][] = [
      [{ ...RETENTION_DEFAULTS, processDays: 6 }, "processDays"],
      [{ ...RETENTION_DEFAULTS, heartbeatDays: 3651 }, "heartbeatDays"],
      [{ ...RETENTION_DEFAULTS, logonDays: 30.5 }, "logonDays"],
      [
        { processDays: 30, networkDays: 30, logonDays: 30 } as unknown as Retention,
        "heartbeatDays",
      ],
    ];
    for (const [r, name] of refused) {
      await expect(
        withChClient(config, fresh, (ch) => bootstrapClickHouseTables(ch, r)),
      ).rejects.toThrow(`retention: ${name} must be a whole number of days from 7 to 3650`);
    }
    const tables = await withChClient(config, fresh, async (ch) => {
      const rs = await ch.query({ query: "SHOW TABLES", format: "JSONEachRow" });
      return rs.json<{ name: string }>();
    });
    expect(tables).toEqual([]);
  } finally {
    await dropChDatabase(config, fresh);
  }
});

test("deploy_ac_011: a run with other values sets them, and a row past its new period goes at the merge", async () => {
  await migrate(retention(30, 30, 30, 30));
  const kept = row(1007, 20);
  await withChClient(config, DB, (ch) =>
    ch.insert({ table: "cges_events", format: "JSONEachRow", values: [kept] }),
  );
  await withChClient(config, DB, (ch) => ch.command({ query: "OPTIMIZE TABLE cges_events FINAL" }));
  expect(await present([kept.event_id as string])).toEqual([kept.event_id]);

  const before = await mutations("cges_events");
  await migrate(retention(14, 15, 16, 17));
  expect(cgesTtl(await showCreate("cges_events"))).toEqual({
    process: 14,
    network: 15,
    logon: 16,
    other: 16,
  });
  expect(heartbeatTtl(await showCreate("heartbeats"))).toBe(17);
  // The materialisation runs as a mutation: the merge below comes once it is done.
  await mutationsDone("cges_events");
  // The new rules are materialised: a mutation measures the stored rows against them.
  expect(await mutations("cges_events")).toBeGreaterThan(before);
  await withChClient(config, DB, (ch) => ch.command({ query: "OPTIMIZE TABLE cges_events FINAL" }));
  expect(await present([kept.event_id as string])).toEqual([]);
});

/** Waits until no mutation of `table` is running. */
async function mutationsDone(table: string): Promise<void> {
  const start = Date.now();
  for (;;) {
    const running = await withChClient(config, DB, async (ch) => {
      const rs = await ch.query({
        query:
          "SELECT count() AS n FROM system.mutations WHERE database = {db:String} AND table = {t:String} AND NOT is_done",
        query_params: { db: DB, t: table },
        format: "JSONEachRow",
      });
      return Number((await rs.json<{ n: string }>())[0]?.n ?? 0);
    });
    if (running === 0) return;
    if (Date.now() - start > 30_000) throw new Error(`mutations of ${table} still running`);
    await new Promise((r) => setTimeout(r, 100));
  }
}

/** The `MODIFY TTL` statements ClickHouse has finished in `DB`, from its query log. */
async function modifyTtls(): Promise<number> {
  return withChClient(config, DB, async (ch) => {
    await ch.command({ query: "SYSTEM FLUSH LOGS" });
    const rs = await ch.query({
      query:
        "SELECT count() AS n FROM system.query_log WHERE type = 'QueryFinish' AND query_kind = 'Alter' AND current_database = {db:String} AND query ILIKE '%MODIFY TTL%'",
      query_params: { db: DB },
      format: "JSONEachRow",
    });
    return Number((await rs.json<{ n: string }>())[0]?.n ?? 0);
  });
}

/** The mutations ClickHouse has recorded on `table` (a MODIFY TTL that materialises adds one). */
async function mutations(table: string): Promise<number> {
  return withChClient(config, DB, async (ch) => {
    const rs = await ch.query({
      query:
        "SELECT count() AS n FROM system.mutations WHERE database = {db:String} AND table = {t:String}",
      query_params: { db: DB, t: table },
      format: "JSONEachRow",
    });
    return Number((await rs.json<{ n: string }>())[0]?.n ?? 0);
  });
}

test("deploy_ac_011: a second run with the same values changes nothing and materialises nothing", async () => {
  const altersBefore = await modifyTtls();
  await migrate(retention(21, 22, 23, 24));
  // The control: the first run, with other values than the table had, alters it.
  const altersAfterFirst = await modifyTtls();
  expect(altersAfterFirst).toBeGreaterThan(altersBefore);
  const cges = await showCreate("cges_events");
  const heartbeats = await showCreate("heartbeats");
  const before = [await mutations("cges_events"), await mutations("heartbeats")];
  await migrate(retention(21, 22, 23, 24));
  expect(await showCreate("cges_events")).toBe(cges);
  expect(await showCreate("heartbeats")).toBe(heartbeats);
  // The time-to-live already said these days: no MODIFY TTL, so no part is rewritten.
  expect([await mutations("cges_events"), await mutations("heartbeats")]).toEqual(before);
  // Nor does ingest send one: the time-to-live is read first (SPEC-023 §Operational §9).
  expect(await modifyTtls()).toBe(altersAfterFirst);
});

test.each([
  ["INGEST_RETENTION_PROCESS_DAYS", "6"],
  ["INGEST_RETENTION_NETWORK_DAYS", "3651"],
  ["INGEST_RETENTION_LOGON_DAYS", "6"],
  ["INGEST_RETENTION_HEARTBEAT_DAYS", "3651"],
])(
  "deploy_ac_011: %s=%s stops the migrate command before it sets a time-to-live",
  async (variable, value) => {
    await migrate(retention(40, 40, 40, 40));
    const before = await showCreate("cges_events");
    const beforeHeartbeats = await showCreate("heartbeats");
    const env = Object.fromEntries(
      Object.entries(process.env).filter(([k]) => !k.startsWith("INGEST_")),
    );
    const r = spawnSync(process.execPath, ["--import", "tsx", "src/db/migrate.ts"], {
      cwd: PACKAGE_DIR,
      encoding: "utf8",
      env: {
        ...env,
        INGEST_PG_URL: config.INGEST_PG_URL,
        INGEST_CH_URL: config.INGEST_CH_URL,
        INGEST_CH_USER: config.INGEST_CH_USER,
        INGEST_CH_PASSWORD: config.INGEST_CH_PASSWORD,
        INGEST_CH_DB: DB,
        [variable]: value,
      },
      timeout: 60_000,
    });
    expect(r.status, r.stderr).toBe(1);
    expect(r.stderr.trim()).toBe(
      `cg-ingest migrate: fatal: invalid configuration: ${variable}: must be an integer from 7 to 3650`,
    );
    expect(await showCreate("cges_events")).toBe(before);
    expect(await showCreate("heartbeats")).toBe(beforeHeartbeats);
  },
);

test("deploy_ac_011: a refused retention changes nothing in Postgres either", async () => {
  const pgDb = `deploy_011_pg_${Date.now()}`;
  await createPgDatabase(config.INGEST_PG_URL, pgDb);
  try {
    const env = Object.fromEntries(
      Object.entries(process.env).filter(([k]) => !k.startsWith("INGEST_")),
    );
    const r = spawnSync(process.execPath, ["--import", "tsx", "src/db/migrate.ts"], {
      cwd: PACKAGE_DIR,
      encoding: "utf8",
      env: {
        ...env,
        INGEST_PG_URL: pgDatabaseUrl(config.INGEST_PG_URL, pgDb),
        INGEST_CH_URL: config.INGEST_CH_URL,
        INGEST_CH_DB: DB,
        INGEST_RETENTION_PROCESS_DAYS: "6",
      },
      timeout: 60_000,
    });
    expect(r.status, r.stderr).toBe(1);
    const pool = new pg.Pool({ connectionString: pgDatabaseUrl(config.INGEST_PG_URL, pgDb) });
    pool.on("error", () => {});
    try {
      const tables = await pool.query<{ n: string }>(
        "SELECT count(*) AS n FROM pg_tables WHERE schemaname NOT IN ('pg_catalog', 'information_schema')",
      );
      expect(Number(tables.rows[0]?.n)).toBe(0);
    } finally {
      await pool.end();
    }
  } finally {
    await dropPgDatabase(config.INGEST_PG_URL, pgDb);
  }
});
