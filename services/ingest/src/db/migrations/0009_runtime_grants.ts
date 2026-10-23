import { type Kysely, sql } from "kysely";

/**
 * SPEC-023 §Operational §4 — the runtime roles' rights on ingest's tables, granted table by
 * table as ingest's code uses them, when the roles exist: the bundle's Postgres
 * initialisation creates them, and the dev stack and the tests, where one superuser owns
 * everything, have none, so this does nothing there. Run as `cg_ingest_owner`, which owns
 * the schema and the tables.
 *
 * - `cg_ingest`, the running service: `SELECT`, `INSERT` and `UPDATE` where its code writes
 *   and reads back (agents, enrollment tokens, the detection watermark, incidents, playbook
 *   executions); `SELECT` and `INSERT` alone on `ca` and `alerts`, so that it cannot replace
 *   the CA, nor change an alert it wrote; no right on its migration ledger, nor `DELETE`.
 * - `cg_incident_reader`, which the api's runtime role holds: `USAGE` on the schema and
 *   `SELECT` on the three tables the api reads.
 *
 * A later migration that creates a table grants it in the same way.
 */
export async function up(db: Kysely<unknown>): Promise<void> {
  await sql`
    DO $$
    BEGIN
      IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'cg_ingest') THEN
        GRANT SELECT, INSERT, UPDATE
          ON agents, enrollment_tokens, detect_watermark, incidents, playbook_executions
          TO cg_ingest;
        GRANT SELECT, INSERT ON ca, alerts TO cg_ingest;
      END IF;
      IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'cg_incident_reader') THEN
        GRANT USAGE ON SCHEMA ingest TO cg_incident_reader;
        GRANT SELECT ON alerts, incidents, playbook_executions TO cg_incident_reader;
      END IF;
    END
    $$
  `.execute(db);
}

export async function down(db: Kysely<unknown>): Promise<void> {
  await sql`
    DO $$
    BEGIN
      IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'cg_ingest') THEN
        REVOKE ALL
          ON agents, enrollment_tokens, detect_watermark, incidents, playbook_executions, ca, alerts
          FROM cg_ingest;
      END IF;
      IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'cg_incident_reader') THEN
        REVOKE ALL ON alerts, incidents, playbook_executions FROM cg_incident_reader;
        REVOKE USAGE ON SCHEMA ingest FROM cg_incident_reader;
      END IF;
    END
    $$
  `.execute(db);
}
