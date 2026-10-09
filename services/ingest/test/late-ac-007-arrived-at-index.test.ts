import { type ClickHouseClient, createClient } from "@clickhouse/client";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import { runMigrations } from "../src/db/migrate.js";

// SPEC-018 late_ac_007 — the bootstrap gives cges_events the minmax skip index
// ix_arrived_at (§Data contracts, §Operational §7): on a new table, and on a table
// that already exists without it; a second bootstrap changes nothing. The shared
// database was bootstrapped by global-setup; the cases that need a table without
// the index run the real bootstrap (runMigrations) against a throwaway ClickHouse
// database, so the shared cges_events is never altered. The Postgres side of
// runMigrations is already at latest and is a no-op here.

const THROWAWAY_DB = "late_ac_007";

interface IndexRow {
  name: string;
  type: string;
  expr: string;
  granularity: string;
}

interface TableRow {
  engine: string;
  partition_key: string;
  sorting_key: string;
}

let config: Config;

function client(database: string): ClickHouseClient {
  return createClient({
    url: config.INGEST_CH_URL,
    username: config.INGEST_CH_USER,
    password: config.INGEST_CH_PASSWORD,
    database,
  });
}

async function withClient<T>(
  database: string,
  fn: (ch: ClickHouseClient) => Promise<T>,
): Promise<T> {
  const ch = client(database);
  try {
    return await fn(ch);
  } finally {
    await ch.close();
  }
}

async function indices(database: string): Promise<IndexRow[]> {
  return withClient(config.INGEST_CH_DB, async (ch) => {
    const rs = await ch.query({
      query: `
        SELECT name, type, expr, toString(granularity) AS granularity
        FROM system.data_skipping_indices
        WHERE database = {db:String} AND table = 'cges_events'
        ORDER BY name
      `,
      query_params: { db: database },
      format: "JSONEachRow",
    });
    return rs.json<IndexRow>();
  });
}

async function table(database: string): Promise<TableRow | undefined> {
  return withClient(config.INGEST_CH_DB, async (ch) => {
    const rs = await ch.query({
      query: `
        SELECT engine, partition_key, sorting_key
        FROM system.tables
        WHERE database = {db:String} AND name = 'cges_events'
      `,
      query_params: { db: database },
      format: "JSONEachRow",
    });
    return (await rs.json<TableRow>())[0];
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

/** The real bootstrap, pointed at `database`. */
async function bootstrap(database: string): Promise<void> {
  await runMigrations({ ...config, INGEST_CH_DB: database });
}

const EXPECTED_INDEX: IndexRow = {
  name: "ix_arrived_at",
  type: "minmax",
  expr: "arrived_at",
  granularity: "1",
};

const EXPECTED_TABLE: TableRow = {
  engine: "ReplacingMergeTree",
  partition_key: "(org_id, toYYYYMMDD(time))",
  sorting_key: "org_id, time, event_id",
};

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

test("late_ac_007: the bootstrap adds ix_arrived_at to a new table and to one without it; a second bootstrap changes nothing", async () => {
  // The shared database: bootstrapped once by global-setup.
  expect(await indices(config.INGEST_CH_DB)).toEqual([EXPECTED_INDEX]);
  expect(await table(config.INGEST_CH_DB)).toEqual(EXPECTED_TABLE);

  // A new table: the bootstrap creates cges_events and indexes it.
  await bootstrap(THROWAWAY_DB);
  expect(await indices(THROWAWAY_DB)).toEqual([EXPECTED_INDEX]);
  expect(await table(THROWAWAY_DB)).toEqual(EXPECTED_TABLE);

  // A table that exists without the index, as every cges_events created before
  // SPEC-018 does: the CREATE is a no-op and the ALTER adds the index.
  await withClient(THROWAWAY_DB, (ch) =>
    ch.command({ query: "ALTER TABLE cges_events DROP INDEX ix_arrived_at" }),
  );
  expect(await indices(THROWAWAY_DB)).toEqual([]);
  expect(await showCreate(THROWAWAY_DB)).not.toContain("ix_arrived_at");
  await bootstrap(THROWAWAY_DB);
  expect(await indices(THROWAWAY_DB)).toEqual([EXPECTED_INDEX]);

  // A second bootstrap changes nothing.
  const before = await showCreate(THROWAWAY_DB);
  await bootstrap(THROWAWAY_DB);
  expect(await showCreate(THROWAWAY_DB)).toBe(before);
  expect(await indices(THROWAWAY_DB)).toEqual([EXPECTED_INDEX]);
  expect(await table(THROWAWAY_DB)).toEqual(EXPECTED_TABLE);
});
