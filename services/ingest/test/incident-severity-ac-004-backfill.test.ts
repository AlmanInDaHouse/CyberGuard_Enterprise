import { promises as fs } from "node:fs";
import * as path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { Kysely, type Migration, type MigrationProvider, Migrator, PostgresDialect } from "kysely";
import pg from "pg";
import { afterAll, beforeAll, expect, inject, test } from "vitest";
import type { Config } from "../src/config.js";

// SPEC-011 incident_severity_ac_004 — migration 0006 backfill correctness + ordering.
//
// The suite's shared Postgres is already migrated to latest (global-setup), so a
// backfill cannot be exercised in-place without contaminating other suites. This
// test runs the REAL ingest migration chain against a THROWAWAY database:
//   migrateTo("0005_incidents")  -> the pre-severity schema (no severity_id column)
//   seed incidents referencing mixed-severity alerts (the pre-migration state)
//   migrateToLatest()            -> applies 0006: ADD nullable -> backfill MAX -> SET NOT NULL
// and asserts each incident's severity_id = MAX(its alerts). Because migrateToLatest
// completes (does not throw), the SET NOT NULL ran AFTER the backfill filled the rows
// — an out-of-order migration would have failed on the NULL rows. CI-able (Postgres
// only, same testcontainers backend as the rest of the suite).

const INGEST_MIGRATIONS = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  "../src/db/migrations",
);
const THROWAWAY_DB = "cyberguard_sev_backfill";
const LEDGER = "sev_backfill_kysely_migration";

// Mirrors `migrate.ts` / `read-schema.ts`: import each migration file via a file://
// URL (cross-platform ESM), keeping the drop-a-file-in-the-folder convention.
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

async function seedAgent(pool: pg.Pool, agentId: string): Promise<void> {
  await pool.query(
    `INSERT INTO agents (agent_id, pubkey, cert_pem, expires_at)
     VALUES ($1, $2, $3, now() + interval '1 day')`,
    [agentId, Buffer.from([0]), "test-cert"],
  );
}

// Insert an alert valid against the 0002 + 0004 schema (the state at 0005): all
// NOT NULLs + the score-present / range / rule_id CHECKs satisfied.
async function seedAlert(
  pool: pg.Pool,
  agentId: string,
  alertId: string,
  severityId: number,
): Promise<void> {
  await pool.query(
    `INSERT INTO alerts
       (alert_id, org_id, agent_id, title, severity_id, cg_detection_source, rule_id,
        source_events, heuristic_score, final_score, cg_mitre, dedup_key, event_time, status)
     VALUES ($1, 'default', $2, 'backfill-test', $3, 'rule', 'rule.office_spawns_script_host',
             $4::uuid[], 0.9, 0.9, $5::jsonb, $6, now(), 'new')`,
    [
      alertId,
      agentId,
      severityId,
      [agentId], // source_events: a valid uuid; not read by the backfill
      JSON.stringify({ tactics: ["execution"], techniques: ["T1059.001"] }),
      `dedup::${alertId}`,
    ],
  );
}

// Insert an incident at the 0005 schema — i.e. WITHOUT severity_id (the column does
// not exist yet). This is the genuine pre-migration row the backfill targets.
async function seedIncidentPre0006(
  pool: pg.Pool,
  incidentId: string,
  agentId: string,
  alertIds: string[],
  groupingKey: string,
): Promise<void> {
  await pool.query(
    `INSERT INTO incidents
       (incident_id, org_id, agent_id, title, status, cg_mitre, alert_ids, grouping_key, window_start)
     VALUES ($1, 'default', $2, 'backfill-incident', 'open', $3::jsonb, $4::uuid[], $5, now())`,
    [
      incidentId,
      agentId,
      JSON.stringify({ tactics: ["execution"], techniques: ["T1059.001"] }),
      alertIds,
      groupingKey,
    ],
  );
}

test("incident_severity_ac_004: 0006 backfills each incident's severity_id to MAX(its alerts)", async () => {
  const pool = new pg.Pool({ connectionString: throwawayUrl(config.INGEST_PG_URL) });
  const db = new Kysely<unknown>({ dialect: new PostgresDialect({ pool }) });
  try {
    const migrator = new Migrator({
      db,
      provider: ingestMigrationProvider(),
      migrationTableName: LEDGER,
      migrationLockTableName: `${LEDGER}_lock`,
    });

    // 1) Migrate to the pre-severity schema (through 0005_incidents, exclusive of 0006).
    const pre = await migrator.migrateTo("0005_incidents");
    if (pre.error) throw pre.error instanceof Error ? pre.error : new Error(String(pre.error));
    for (const r of pre.results ?? []) expect(r.status).not.toBe("Error");

    // The severity_id column must NOT exist yet — proves we are genuinely pre-migration.
    const before = await pool.query(
      `SELECT 1 FROM information_schema.columns
       WHERE table_name = 'incidents' AND column_name = 'severity_id'`,
    );
    expect(before.rowCount).toBe(0);

    // 2) Seed mixed-severity alerts + incidents that reference them (no severity yet).
    const agentId = "01934abc-def0-7000-89ab-0000000004a0";
    await seedAgent(pool, agentId);
    const a2 = "01934abc-def0-7000-89ab-000000000402";
    const a6 = "01934abc-def0-7000-89ab-000000000406";
    const a5 = "01934abc-def0-7000-89ab-000000000405";
    const a3 = "01934abc-def0-7000-89ab-000000000403";
    await seedAlert(pool, agentId, a2, 2);
    await seedAlert(pool, agentId, a6, 6);
    await seedAlert(pool, agentId, a5, 5);
    await seedAlert(pool, agentId, a3, 3);
    const inc1 = "01934abc-def0-7000-89ab-0000000041a1";
    const inc2 = "01934abc-def0-7000-89ab-0000000041a2";
    await seedIncidentPre0006(pool, inc1, agentId, [a2, a6], "gk-1"); // MAX(2,6) -> 6
    await seedIncidentPre0006(pool, inc2, agentId, [a5, a3], "gk-2"); // MAX(5,3) -> 5

    // 3) Apply 0006: ADD nullable -> backfill MAX -> SET NOT NULL + CHECK.
    const post = await migrator.migrateToLatest();
    if (post.error) throw post.error instanceof Error ? post.error : new Error(String(post.error));
    for (const r of post.results ?? []) expect(r.status).not.toBe("Error");
    // 0006 actually ran (not a no-op) and did not throw -> SET NOT NULL succeeded AFTER
    // the backfill (an out-of-order migration would have failed on the NULL rows).
    expect((post.results ?? []).some((r) => r.migrationName === "0006_incident_severity")).toBe(
      true,
    );

    // 4) Each incident is backfilled to the MAX severity over its alert_ids.
    const r1 = await pool.query<{ severity_id: number }>(
      "SELECT severity_id FROM incidents WHERE incident_id = $1",
      [inc1],
    );
    expect(r1.rows[0]?.severity_id).toBe(6);
    const r2 = await pool.query<{ severity_id: number }>(
      "SELECT severity_id FROM incidents WHERE incident_id = $1",
      [inc2],
    );
    expect(r2.rows[0]?.severity_id).toBe(5);
  } finally {
    await db.destroy(); // ends the pool it owns
  }
});
