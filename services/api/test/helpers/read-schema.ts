import pg from "pg";
import type { Config } from "../../src/config.js";

/**
 * SPEC-009 read-slice — TEST-ONLY materialisation of the read-target tables
 * (materialisation option (c), Session 19 gate).
 *
 * These tables (agents / alerts / incidents) are OWNED by the INGEST service's
 * migrations; the api migration runner (Option A, SPEC-008 §Operational §7)
 * applies only api's own (users / audit_log), so the api test Postgres lacks
 * them — without this a read_ac would fail with "relation does not exist"
 * (setup-broken), not the absent read CONTROL.
 *
 * This is a TEST-ONLY MIRROR, deliberately scoped to the columns the read-API
 * CONSUMES (SPEC-009 §Data contracts). The CANONICAL schema lives in:
 *   - services/ingest/src/db/migrations/0001_initial.ts            (agents)
 *   - services/ingest/src/db/migrations/0002_alerts.ts             (alerts)
 *   - services/ingest/src/db/migrations/0004_alerts_event_time.ts  (alerts.event_time)
 *   - services/ingest/src/db/migrations/0005_incidents.ts          (incidents)
 * A drift in a column the read-API READS is caught by a read_ac at the GREEN gate.
 *
 * NAMED ARCHITECTURE DEBT → option (b): replace this mirror with a shared schema
 * package (or a pnpm-workspace cross-import) when the shared api↔ingest surface
 * grows. TRIGGER: a 2nd ingest table this mirror does not cover, OR the first
 * un-caught drift. (Also recorded in SPEC-009 §Operational + engineering-notes.)
 * This is ARCHITECTURE debt, distinct from the harness-first RED Known CI debt.
 */
export async function applyReadSchema(config: Config): Promise<void> {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    await pool.query(`
      CREATE TABLE IF NOT EXISTS agents (
        agent_id uuid PRIMARY KEY,
        org_id   text NOT NULL DEFAULT 'default'
      )
    `);
    await pool.query(`
      CREATE TABLE IF NOT EXISTS alerts (
        alert_id     uuid         PRIMARY KEY,
        org_id       text         NOT NULL DEFAULT 'default',
        agent_id     uuid         NOT NULL REFERENCES agents (agent_id),
        title        text         NOT NULL,
        severity_id  smallint     NOT NULL,
        status       text         NOT NULL,
        rule_id      text,
        cg_mitre     jsonb,
        final_score  numeric(4,3) NOT NULL,
        event_time   timestamptz  NOT NULL,
        created_at   timestamptz  NOT NULL DEFAULT now()
      )
    `);
    await pool.query(`
      CREATE TABLE IF NOT EXISTS incidents (
        incident_id  uuid        PRIMARY KEY,
        org_id       text        NOT NULL DEFAULT 'default',
        agent_id     uuid        NOT NULL REFERENCES agents (agent_id),
        status       text        NOT NULL,
        title        text        NOT NULL,
        cg_mitre     jsonb,
        alert_ids    uuid[]      NOT NULL,
        assigned_to  text,
        window_start timestamptz NOT NULL,
        created_at   timestamptz NOT NULL DEFAULT now(),
        updated_at   timestamptz NOT NULL DEFAULT now()
      )
    `);
  } finally {
    await pool.end();
  }
}

export async function ensureAgent(
  config: Config,
  agentId: string,
  orgId = "default",
): Promise<void> {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    await pool.query(
      "INSERT INTO agents (agent_id, org_id) VALUES ($1, $2) ON CONFLICT (agent_id) DO NOTHING",
      [agentId, orgId],
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
}

export async function insertAlertRow(config: Config, a: SeedAlert): Promise<void> {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    await pool.query(
      `INSERT INTO alerts
         (alert_id, org_id, agent_id, title, severity_id, status, rule_id, cg_mitre, final_score, event_time)
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now())`,
      [
        a.alertId,
        a.orgId ?? "default",
        a.agentId,
        a.title ?? "Office spawned a script host",
        a.severityId ?? 4,
        a.status ?? "new",
        a.ruleId ?? "rule.office_spawns_script_host",
        JSON.stringify(a.cgMitre ?? { tactics: ["execution"], techniques: ["T1059.001"] }),
        a.finalScore ?? 0.9,
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

export async function insertIncidentRow(config: Config, i: SeedIncident): Promise<void> {
  const pool = new pg.Pool({ connectionString: config.API_PG_URL });
  try {
    await pool.query(
      `INSERT INTO incidents
         (incident_id, org_id, agent_id, status, title, cg_mitre, alert_ids, assigned_to, window_start)
       VALUES ($1, $2, $3, $4, $5, $6, $7::uuid[], $8, now())`,
      [
        i.incidentId,
        i.orgId ?? "default",
        i.agentId,
        i.status ?? "open",
        i.title ?? "Execution activity",
        JSON.stringify(i.cgMitre ?? { tactics: ["execution"], techniques: ["T1059.001"] }),
        i.alertIds,
        i.assignedTo ?? null,
      ],
    );
  } finally {
    await pool.end();
  }
}
