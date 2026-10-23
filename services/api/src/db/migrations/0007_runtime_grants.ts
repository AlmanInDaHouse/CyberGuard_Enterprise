import { type Kysely, sql } from "kysely";

/**
 * SPEC-023 §Operational §4 — the api's runtime role's rights on the api's tables, granted
 * table by table as the api's code uses them, when the role exists: the bundle's Postgres
 * initialisation creates it, and the dev stack and the tests, where one superuser owns
 * everything, have none, so this does nothing there. Run as `cg_api_owner`, which owns the
 * schema and the tables.
 *
 * `cg_api`: `SELECT`, `INSERT` and `UPDATE` on `users`; `SELECT` and `INSERT` on
 * `forensic_key`, so that the running api cannot rewrite the key; `INSERT` alone on
 * `audit_log`, which its code never reads, so that the log is append-only by grant; no
 * right on its migration ledger, nor `DELETE`. Its reads of ingest's tables come from
 * `cg_incident_reader`, which ingest's migrations grant.
 *
 * A later migration that creates a table grants it in the same way.
 */
export async function up(db: Kysely<unknown>): Promise<void> {
  await sql`
    DO $$
    BEGIN
      IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'cg_api') THEN
        GRANT SELECT, INSERT, UPDATE ON users TO cg_api;
        GRANT SELECT, INSERT ON forensic_key TO cg_api;
        GRANT INSERT ON audit_log TO cg_api;
      END IF;
    END
    $$
  `.execute(db);
}

export async function down(db: Kysely<unknown>): Promise<void> {
  await sql`
    DO $$
    BEGIN
      IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'cg_api') THEN
        REVOKE ALL ON users, forensic_key, audit_log FROM cg_api;
      END IF;
    END
    $$
  `.execute(db);
}
