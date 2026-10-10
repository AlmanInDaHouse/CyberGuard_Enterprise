import { type ClickHouseClient, createClient } from "@clickhouse/client";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { runMigrations } from "../src/db/migrate.js";

// SPEC-020 auth_ac_003 — storage migration and retention (§Data contracts,
// §Operational §8). After the bootstrap cges_events has the eleven Authentication
// columns with their types and defaults, on a new table and on a table created
// without them; a row written before reads the defaults; a second bootstrap leaves
// the create statement unchanged; that statement carries a 365-day time-to-live on
// arrived_at restricted to class 3002. In a throwaway database, OPTIMIZE ... FINAL
// deletes a 3002 row 366 days old and keeps a 1007 row of the same age and a 3002
// row 364 days old. As in net_ac_003, the cases that need a table without the
// columns run the real bootstrap against a throwaway database.

const THROWAWAY_DB = "auth_ac_003";

interface ColumnRow {
  name: string;
  type: string;
  default_kind: string;
  default_expression: string;
}

const EXPECTED_COLUMNS: ColumnRow[] = [
  { name: "user_uid", type: "String", default_kind: "DEFAULT", default_expression: "''" },
  { name: "user_name", type: "String", default_kind: "DEFAULT", default_expression: "''" },
  { name: "user_domain", type: "String", default_kind: "DEFAULT", default_expression: "''" },
  { name: "logon_type_id", type: "UInt8", default_kind: "DEFAULT", default_expression: "0" },
  { name: "status_id", type: "UInt8", default_kind: "DEFAULT", default_expression: "0" },
  { name: "status_code", type: "String", default_kind: "DEFAULT", default_expression: "''" },
  { name: "status_detail", type: "String", default_kind: "DEFAULT", default_expression: "''" },
  { name: "auth_protocol", type: "String", default_kind: "DEFAULT", default_expression: "''" },
  { name: "auth_protocol_id", type: "UInt8", default_kind: "DEFAULT", default_expression: "0" },
  { name: "src_hostname", type: "String", default_kind: "DEFAULT", default_expression: "''" },
  {
    name: "elevated_token",
    type: "Nullable(Bool)",
    default_kind: "DEFAULT",
    default_expression: "NULL",
  },
];
const LOGON_COLUMNS = EXPECTED_COLUMNS.map((c) => c.name);

/** The time-to-live as ClickHouse normalizes it in the create statement. */
const TTL_CLAUSE =
  /TTL toDateTime\(arrived_at\) \+ toIntervalDay\(365\)( DELETE)? WHERE class_uid = 3002/;

let config: Config;

async function withClient<T>(
  database: string,
  fn: (ch: ClickHouseClient) => Promise<T>,
): Promise<T> {
  const ch = createClient({
    url: config.INGEST_CH_URL,
    username: config.INGEST_CH_USER,
    password: config.INGEST_CH_PASSWORD,
    database,
  });
  try {
    return await fn(ch);
  } finally {
    await ch.close();
  }
}

/** The eleven columns of `database`.cges_events, in table order. */
async function logonColumns(database: string): Promise<ColumnRow[]> {
  return withClient(config.INGEST_CH_DB, async (ch) => {
    const rs = await ch.query({
      query: `
        SELECT name, type, default_kind, default_expression
        FROM system.columns
        WHERE database = {db:String} AND table = 'cges_events'
          AND name IN ({names:Array(String)})
        ORDER BY position
      `,
      query_params: { db: database, names: LOGON_COLUMNS },
      format: "JSONEachRow",
    });
    return rs.json<ColumnRow>();
  });
}

async function showCreate(database: string): Promise<string> {
  return withClient(database, async (ch) => {
    const rs = await ch.query({
      query: "SHOW CREATE TABLE cges_events",
      format: "TabSeparatedRaw",
    });
    return rs.text();
  });
}

async function bootstrap(database: string): Promise<void> {
  await runMigrations({ ...config, INGEST_CH_DB: database });
}

/** A minimal row of `classUid` whose arrived_at is `ageDays` days old. */
function row(classUid: number, ageDays: number): Record<string, unknown> {
  const arrived = new Date(Date.now() - ageDays * 24 * 3600 * 1000);
  return {
    agent_id: "01934abc-def0-7000-89ab-000000000003",
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

beforeAll(async () => {
  config = inject("ingestConfig");
  await withClient(config.INGEST_CH_DB, async (ch) => {
    await ch.command({ query: `DROP DATABASE IF EXISTS ${THROWAWAY_DB}` });
    await ch.command({ query: `CREATE DATABASE ${THROWAWAY_DB}` });
  });
});

afterAll(async () => {
  await withClient(config.INGEST_CH_DB, async (ch) => {
    await ch.command({ query: `DROP DATABASE IF EXISTS ${THROWAWAY_DB}` });
  });
});

test("auth_ac_003: the bootstrap adds the eleven columns and the class-3002 time-to-live", async () => {
  // The shared database: bootstrapped once by global-setup.
  expect(await logonColumns(config.INGEST_CH_DB)).toEqual(EXPECTED_COLUMNS);
  expect(await showCreate(config.INGEST_CH_DB)).toMatch(TTL_CLAUSE);

  // A new table.
  await bootstrap(THROWAWAY_DB);
  expect(await logonColumns(THROWAWAY_DB)).toEqual(EXPECTED_COLUMNS);

  // A table as it was before SPEC-020: without the columns and without a
  // time-to-live, holding a row written then — the upgrade the bootstrap meets.
  await withClient(THROWAWAY_DB, (ch) =>
    ch.command({ query: "ALTER TABLE cges_events REMOVE TTL" }),
  );
  await withClient(THROWAWAY_DB, (ch) =>
    ch.command({
      query: `ALTER TABLE cges_events ${LOGON_COLUMNS.map((c) => `DROP COLUMN ${c}`).join(", ")}`,
    }),
  );
  expect(await showCreate(THROWAWAY_DB)).not.toMatch(TTL_CLAUSE);
  expect(await logonColumns(THROWAWAY_DB)).toEqual([]);
  const old = row(1007, 0);
  await withClient(THROWAWAY_DB, (ch) =>
    ch.insert({ table: "cges_events", format: "JSONEachRow", values: [old] }),
  );

  await bootstrap(THROWAWAY_DB);
  expect(await logonColumns(THROWAWAY_DB)).toEqual(EXPECTED_COLUMNS);
  expect(await showCreate(THROWAWAY_DB)).toMatch(TTL_CLAUSE);
  const read = await withClient(THROWAWAY_DB, async (ch) => {
    const rs = await ch.query({
      query: `SELECT ${LOGON_COLUMNS.join(", ")} FROM cges_events WHERE event_id = {id:UUID}`,
      query_params: { id: old.event_id as string },
      format: "JSONEachRow",
    });
    return rs.json<Record<string, unknown>>();
  });
  expect(read).toEqual([
    {
      user_uid: "",
      user_name: "",
      user_domain: "",
      logon_type_id: 0,
      status_id: 0,
      status_code: "",
      status_detail: "",
      auth_protocol: "",
      auth_protocol_id: 0,
      src_hostname: "",
      elevated_token: null,
    },
  ]);

  // A second bootstrap leaves the create statement unchanged.
  const before = await showCreate(THROWAWAY_DB);
  expect(before).toMatch(TTL_CLAUSE);
  await bootstrap(THROWAWAY_DB);
  expect(await showCreate(THROWAWAY_DB)).toBe(before);
});

test("auth_ac_003: the time-to-live deletes a 3002 row 366 days old and nothing else", async () => {
  await bootstrap(THROWAWAY_DB);
  const expired = row(3002, 366);
  const young = row(3002, 364);
  const process = row(1007, 366);
  await withClient(THROWAWAY_DB, (ch) =>
    ch.insert({ table: "cges_events", format: "JSONEachRow", values: [expired, young, process] }),
  );
  await withClient(THROWAWAY_DB, (ch) => ch.command({ query: "OPTIMIZE TABLE cges_events FINAL" }));

  const left = await withClient(THROWAWAY_DB, async (ch) => {
    const rs = await ch.query({
      query: `SELECT toString(event_id) AS id FROM cges_events
              WHERE event_id IN ({ids:Array(UUID)})`,
      query_params: { ids: [expired.event_id, young.event_id, process.event_id] },
      format: "JSONEachRow",
    });
    return (await rs.json<{ id: string }>()).map((r) => r.id).sort();
  });
  expect(left).toEqual([young.event_id as string, process.event_id as string].sort());
});
