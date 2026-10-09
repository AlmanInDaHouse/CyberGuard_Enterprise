import { promises as fs } from "node:fs";
import * as path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { Kysely, type Migration, type MigrationProvider, Migrator, PostgresDialect } from "kysely";
import pg from "pg";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";
import * as migration0007 from "../src/db/migrations/0007_detect_arrival_cursor.js";
import { runDetectionCycle } from "../src/detect/index.js";
import {
  enrollTestAgent,
  getAlerts,
  getCursor,
  insertCgesEvent,
  lastArrivedRow,
} from "./helpers/db.js";
import { detectConfig } from "./helpers/detect.js";

// SPEC-018 late_ac_006 — cursor storage and upgrade (§Data contracts, §Operational
// §6). (1) Migration 0007 on a THROWAWAY database (the shared one is at latest and
// other suites read it): an existing detect_watermark row loses last_time and its
// cursor starts at the beginning; 0007 applies a second time without error; its
// down restores last_time with its default. (2) On the shared database: events
// already stored and alerted, the cursor put back at the beginning as 0007 leaves
// an existing row; the next cycle re-reads them, writes no second alert, and
// leaves the cursor on the last row read.

const INGEST_MIGRATIONS = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  "../src/db/migrations",
);
const THROWAWAY_DB = "cyberguard_late_ac_006";

const ORG = "late-ac-006";
const AGENT = "01934abc-def0-7000-89ab-0000000a6001";
const WINWORD = "C:\\Program Files\\Microsoft Office\\root\\Office16\\winword.exe";
const POWERSHELL = "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";

// Mirrors migrate.ts: import each migration file via a file:// URL (cross-platform ESM).
function ingestMigrationProvider(): MigrationProvider {
  return {
    async getMigrations(): Promise<Record<string, Migration>> {
      const entries = (await fs.readdir(INGEST_MIGRATIONS))
        .filter((f) => /\.(?:js|mjs|ts)$/.test(f) && !f.endsWith(".d.ts"))
        .sort();
      const migrations: Record<string, Migration> = {};
      for (const file of entries) {
        const name = file.replace(/\.(?:js|mjs|ts)$/, "");
        migrations[name] = (await import(
          pathToFileURL(path.join(INGEST_MIGRATIONS, file)).href
        )) as Migration;
      }
      return migrations;
    },
  };
}

function throwawayUrl(baseUrl: string): string {
  const u = new URL(baseUrl);
  u.pathname = `/${THROWAWAY_DB}`;
  return u.toString();
}

interface ColumnRow {
  column_name: string;
  column_default: string | null;
}

async function watermarkColumns(pool: pg.Pool): Promise<ColumnRow[]> {
  const r = await pool.query<ColumnRow>(
    `SELECT column_name, column_default FROM information_schema.columns
     WHERE table_name = 'detect_watermark' ORDER BY column_name`,
  );
  return r.rows;
}

let config: Config;

beforeAll(async () => {
  config = inject("ingestConfig");
  const admin = new pg.Pool({ connectionString: config.INGEST_PG_URL });
  try {
    await admin.query(`DROP DATABASE IF EXISTS ${THROWAWAY_DB} WITH (FORCE)`);
    await admin.query(`CREATE DATABASE ${THROWAWAY_DB}`);
  } finally {
    await admin.end();
  }
});

afterAll(async () => {
  const admin = new pg.Pool({ connectionString: config.INGEST_PG_URL });
  try {
    await admin.query(`DROP DATABASE IF EXISTS ${THROWAWAY_DB} WITH (FORCE)`);
  } finally {
    await admin.end();
  }
});

test("late_ac_006: migration 0007 starts an existing cursor at the beginning, applies twice, and its down restores last_time", async () => {
  const pool = new pg.Pool({ connectionString: throwawayUrl(config.INGEST_PG_URL) });
  const db = new Kysely<unknown>({ dialect: new PostgresDialect({ pool }) });
  try {
    const migrator = new Migrator({ db, provider: ingestMigrationProvider() });
    const pre = await migrator.migrateTo("0006_incident_severity");
    expect(pre.error).toBeUndefined();
    // An org that a SPEC-006 cycle had advanced by event time.
    await pool.query(
      "INSERT INTO detect_watermark (org_id, last_time) VALUES ('org-upgraded', '2026-10-09 12:00:00.000000000')",
    );

    const applied = await migrator.migrateTo("0007_detect_arrival_cursor");
    expect(applied.error).toBeUndefined();
    expect(applied.results?.map((r) => [r.migrationName, r.status])).toEqual([
      ["0007_detect_arrival_cursor", "Success"],
    ]);
    const cursorColumns = [
      { column_name: "last_arrived_at", column_default: "'1970-01-01 00:00:00.000'::text" },
      {
        column_name: "last_event_id",
        column_default: "'00000000-0000-0000-0000-000000000000'::uuid",
      },
      { column_name: "org_id", column_default: null },
      { column_name: "updated_at", column_default: "now()" },
    ];
    expect(await watermarkColumns(pool)).toEqual(cursorColumns);
    const row = await pool.query(
      "SELECT last_arrived_at, last_event_id FROM detect_watermark WHERE org_id = 'org-upgraded'",
    );
    expect(row.rows).toEqual([
      {
        last_arrived_at: "1970-01-01 00:00:00.000",
        last_event_id: "00000000-0000-0000-0000-000000000000",
      },
    ]);

    // A second application changes nothing and does not fail.
    await migration0007.up(db);
    expect(await watermarkColumns(pool)).toEqual(cursorColumns);

    const down = await migrator.migrateDown();
    expect(down.error).toBeUndefined();
    expect(down.results?.map((r) => [r.migrationName, r.status])).toEqual([
      ["0007_detect_arrival_cursor", "Success"],
    ]);
    expect(await watermarkColumns(pool)).toEqual([
      { column_name: "last_time", column_default: "'1970-01-01 00:00:00.000000000'::text" },
      { column_name: "org_id", column_default: null },
      { column_name: "updated_at", column_default: "now()" },
    ]);
  } finally {
    await db.destroy();
  }
});

test("late_ac_006: from the beginning, the next cycle re-reads stored, alerted events and writes no second alert", async () => {
  await enrollTestAgent(config, AGENT, ORG);
  await insertCgesEvent(config, {
    agentId: AGENT,
    orgId: ORG,
    eventId: "01934abc-def0-4000-89ab-0000000a6010",
    activityId: 1,
    processPid: 6000,
    processName: "winword.exe",
    imageFileName: WINWORD,
    time: "2026-10-09 08:00:00.000000000",
  });
  await insertCgesEvent(config, {
    agentId: AGENT,
    orgId: ORG,
    eventId: "01934abc-def0-4000-89ab-0000000a6011",
    activityId: 1,
    processPid: 6001,
    processName: "powershell.exe",
    imageFileName: POWERSHELL,
    processParentPid: 6000,
    time: "2026-10-09 08:00:01.000000000",
  });
  const first = await runDetectionCycle(detectConfig(config, ORG));
  expect(first.alertsWritten).toBe(1);

  // The cursor at the beginning, as 0007 leaves an existing row: the column defaults.
  const pool = new pg.Pool({ connectionString: config.INGEST_PG_URL });
  try {
    await pool.query(
      `UPDATE detect_watermark SET last_arrived_at = DEFAULT, last_event_id = DEFAULT
       WHERE org_id = $1`,
      [ORG],
    );
  } finally {
    await pool.end();
  }
  expect(await getCursor(config, ORG)).toEqual({
    arrivedAt: "1970-01-01 00:00:00.000",
    eventId: "00000000-0000-0000-0000-000000000000",
  });

  const reread = await runDetectionCycle(detectConfig(config, ORG));
  expect(reread.eventsEvaluated).toBe(2);
  expect(reread.alertsWritten).toBe(0);
  expect(await getAlerts(config, { agentId: AGENT })).toHaveLength(1);
  const last = await lastArrivedRow(config, ORG);
  expect(last).not.toBeNull();
  expect(reread.processedThrough).toEqual(last);
  expect(await getCursor(config, ORG)).toEqual(last);
});
