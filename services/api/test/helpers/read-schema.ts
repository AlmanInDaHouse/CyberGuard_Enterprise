import { promises as fs } from "node:fs";
import * as path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { Kysely, type Migration, type MigrationProvider, Migrator, PostgresDialect } from "kysely";
import pg from "pg";
import type { Config } from "../../src/config.js";

/**
 * SPEC-009 read-slice — materialise the read-target tables (agents / alerts /
 * incidents) the read-API reads, by applying the INGEST service's REAL Postgres
 * migrations in-workspace.
 *
 * The (b) architecture debt is RETIRED here: this WAS a test-only DDL mirror
 * (option (c)), which risked drifting from ingest's canonical schema. The pnpm
 * workspace now lets the api test apply ingest's REAL migrations, so the read
 * tables come from the source of truth and the mirror-drift risk is gone by
 * construction.
 *
 * The ClickHouse blocker that sank option (a') is avoided faithfully: we run
 * ingest's Postgres migration FILES via our OWN Kysely Migrator — NOT ingest's
 * `runMigrations`, which couples a ClickHouse bootstrap
 * (`services/ingest/src/db/migrate.ts:83`/`:86`, `bootstrapClickHouse`, private).
 * The migration files are pure Postgres (they import only `kysely`); ClickHouse is
 * never touched. Resolution of their `kysely` import relies on the pnpm workspace
 * (hoisted/symlinked into `services/ingest/node_modules`) — the (B) enabler.
 *
 * Distinct ledger table (`ingest_kysely_migration`) in the throwaway api test PG,
 * so it never collides with api's own runner (`api_kysely_migration`).
 */
const INGEST_MIGRATIONS = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  "../../../ingest/src/db/migrations",
);
const LEDGER = "ingest_kysely_migration";

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

/** Apply ingest's REAL Postgres migrations to the api test PG (no ClickHouse). */
export async function applyReadSchema(config: Config): Promise<void> {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  const db = new Kysely<unknown>({ dialect: new PostgresDialect({ pool }) });
  try {
    const migrator = new Migrator({
      db,
      provider: ingestMigrationProvider(),
      migrationTableName: LEDGER,
      migrationLockTableName: `${LEDGER}_lock`,
    });
    const { error, results } = await migrator.migrateToLatest();
    for (const r of results ?? []) {
      if (r.status === "Error") {
        throw new Error(`ingest migration failed in api test setup: ${r.migrationName}`);
      }
    }
    if (error) throw error instanceof Error ? error : new Error(String(error));
  } finally {
    await db.destroy();
  }
}

/** Enrol an agent (production-faithful: alerts/incidents FK → agents). The real
 *  `agents` table (ingest 0001) requires pubkey/cert_pem/expires_at. */
export async function ensureAgent(
  config: Config,
  agentId: string,
  orgId = "default",
): Promise<void> {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    await pool.query(
      `INSERT INTO agents (agent_id, org_id, pubkey, cert_pem, expires_at)
       VALUES ($1, $2, $3, $4, now() + interval '1 day')
       ON CONFLICT (agent_id) DO NOTHING`,
      [agentId, orgId, Buffer.from([0]), "test-cert"],
    );
  } finally {
    await pool.end();
  }
}

export interface SeedAlert {
  alertId: string;
  agentId: string;
  orgId?: string;
  title?: string;
  severityId?: number;
  status?: string;
  ruleId?: string | null;
  cgMitre?: { tactics: string[]; techniques: string[] } | null;
  finalScore?: number;
  /** SPEC-010 — the alert's source_events (cges_events.event_id list). Defaults to
   *  [agentId] (a valid uuid) so the SPEC-009 read-AC seeds, which do not read it,
   *  stay valid; the drill ACs pass real event ids that exist in cges_events. */
  sourceEvents?: string[];
}

/** Insert an alert valid against ingest's REAL `alerts` schema (0002 + 0004):
 *  all NOT NULLs + the rule_id/score-present/range CHECKs satisfied. The columns
 *  the read-API does not read (cg_detection_source, source_events, dedup_key,
 *  heuristic_score) are filled with valid values; dedup_key is unique per alert. */
export async function insertAlertRow(config: Config, a: SeedAlert): Promise<void> {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    await pool.query(
      `INSERT INTO alerts
         (alert_id, org_id, agent_id, title, severity_id, cg_detection_source, rule_id,
          source_events, heuristic_score, final_score, cg_mitre, dedup_key, event_time, status)
       VALUES ($1, $2, $3, $4, $5, 'rule', $6, $7::uuid[], 0.9, $8, $9, $10, now(), $11)`,
      [
        a.alertId,
        a.orgId ?? "default",
        a.agentId,
        a.title ?? "Office spawned a script host",
        a.severityId ?? 4,
        a.ruleId ?? "rule.office_spawns_script_host",
        a.sourceEvents ?? [a.agentId], // SPEC-010: source_events; default [agentId] (a valid uuid)
        a.finalScore ?? 0.9,
        JSON.stringify(a.cgMitre ?? { tactics: ["execution"], techniques: ["T1059.001"] }),
        `${a.agentId}::rule::${a.alertId}`, // unique dedup_key
        a.status ?? "new",
      ],
    );
  } finally {
    await pool.end();
  }
}

export interface SeedIncident {
  incidentId: string;
  agentId: string;
  orgId?: string;
  status?: string;
  title?: string;
  cgMitre?: { tactics: string[]; techniques: string[] } | null;
  alertIds: string[];
  assignedTo?: string | null;
}

/** Insert an incident valid against ingest's REAL `incidents` schema (0005):
 *  grouping_key (NOT NULL UNIQUE) + window_start + alert_ids (>= 1) supplied. */
export async function insertIncidentRow(config: Config, i: SeedIncident): Promise<void> {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    await pool.query(
      `INSERT INTO incidents
         (incident_id, org_id, agent_id, status, title, cg_mitre, alert_ids, assigned_to,
          grouping_key, window_start)
       VALUES ($1, $2, $3, $4, $5, $6, $7::uuid[], $8, $9, now())`,
      [
        i.incidentId,
        i.orgId ?? "default",
        i.agentId,
        i.status ?? "open",
        i.title ?? "Execution activity",
        JSON.stringify(i.cgMitre ?? { tactics: ["execution"], techniques: ["T1059.001"] }),
        i.alertIds,
        i.assignedTo ?? null,
        `${i.orgId ?? "default"}::${i.agentId}::${i.incidentId}`, // unique grouping_key
      ],
    );
  } finally {
    await pool.end();
  }
}
