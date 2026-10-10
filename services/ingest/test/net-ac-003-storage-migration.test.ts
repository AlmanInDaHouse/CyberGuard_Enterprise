import { type ClickHouseClient, createClient } from "@clickhouse/client";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { runMigrations } from "../src/db/migrate.js";

// SPEC-019 net_ac_003 — storage migration (§Data contracts, §Operational §7). After
// the bootstrap cges_events has the six Network Activity columns with their types
// and defaults: on a new table, and on a table created without them; a second
// bootstrap changes nothing; a row written before the columns existed reads with
// the defaults. As in late_ac_007, the cases that need a table without the columns
// run the real bootstrap against a throwaway ClickHouse database.

const THROWAWAY_DB = "net_ac_003";

interface ColumnRow {
  name: string;
  type: string;
  default_kind: string;
  default_expression: string;
}

const EXPECTED_COLUMNS: ColumnRow[] = [
  { name: "src_ip", type: "String", default_kind: "DEFAULT", default_expression: "''" },
  { name: "src_port", type: "UInt16", default_kind: "DEFAULT", default_expression: "0" },
  { name: "dst_ip", type: "String", default_kind: "DEFAULT", default_expression: "''" },
  { name: "dst_port", type: "UInt16", default_kind: "DEFAULT", default_expression: "0" },
  { name: "net_protocol", type: "String", default_kind: "DEFAULT", default_expression: "''" },
  { name: "net_direction", type: "String", default_kind: "DEFAULT", default_expression: "''" },
];
const NETWORK_COLUMNS = EXPECTED_COLUMNS.map((c) => c.name);

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

/** The six network columns of `database`.cges_events, in table order. */
async function networkColumns(database: string): Promise<ColumnRow[]> {
  return withClient(config.INGEST_CH_DB, async (ch) => {
    const rs = await ch.query({
      query: `
        SELECT name, type, default_kind, default_expression
        FROM system.columns
        WHERE database = {db:String} AND table = 'cges_events'
          AND name IN ({names:Array(String)})
        ORDER BY position
      `,
      query_params: { db: database, names: NETWORK_COLUMNS },
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

test("net_ac_003: the bootstrap adds the six network columns to a new table and to one without them", async () => {
  // The shared database: bootstrapped once by global-setup.
  expect(await networkColumns(config.INGEST_CH_DB)).toEqual(EXPECTED_COLUMNS);

  // A new table: the bootstrap creates cges_events with the columns.
  await bootstrap(THROWAWAY_DB);
  expect(await networkColumns(THROWAWAY_DB)).toEqual(EXPECTED_COLUMNS);

  // A table without them, as every cges_events created before SPEC-019, holding a
  // Process Activity row written then.
  await withClient(THROWAWAY_DB, (ch) =>
    ch.command({
      query: `ALTER TABLE cges_events ${NETWORK_COLUMNS.map((c) => `DROP COLUMN ${c}`).join(", ")}`,
    }),
  );
  expect(await networkColumns(THROWAWAY_DB)).toEqual([]);
  const eventId = globalThis.crypto.randomUUID();
  await withClient(THROWAWAY_DB, (ch) =>
    ch.insert({
      table: "cges_events",
      format: "JSONEachRow",
      values: [
        {
          agent_id: "01934abc-def0-7000-89ab-000000000003",
          event_id: eventId,
          class_uid: 1007,
          activity_id: 1,
          process_pid: 4321,
          process_uid: "",
          process_name: "notepad.exe",
          image_file_name: "C:\\Windows\\System32\\notepad.exe",
          time: "2026-10-10 12:00:00.000000000",
        },
      ],
    }),
  );

  await bootstrap(THROWAWAY_DB);
  expect(await networkColumns(THROWAWAY_DB)).toEqual(EXPECTED_COLUMNS);

  // The row written before the columns existed reads with the defaults.
  const old = await withClient(THROWAWAY_DB, async (ch) => {
    const rs = await ch.query({
      query: `
        SELECT src_ip, src_port, dst_ip, dst_port, net_protocol, net_direction
        FROM cges_events WHERE event_id = {id:UUID}
      `,
      query_params: { id: eventId },
      format: "JSONEachRow",
    });
    return rs.json<Record<string, unknown>>();
  });
  expect(old).toEqual([
    { src_ip: "", src_port: 0, dst_ip: "", dst_port: 0, net_protocol: "", net_direction: "" },
  ]);

  // A second bootstrap changes nothing.
  const before = await showCreate(THROWAWAY_DB);
  await bootstrap(THROWAWAY_DB);
  expect(await showCreate(THROWAWAY_DB)).toBe(before);
  expect(await networkColumns(THROWAWAY_DB)).toEqual(EXPECTED_COLUMNS);
});
