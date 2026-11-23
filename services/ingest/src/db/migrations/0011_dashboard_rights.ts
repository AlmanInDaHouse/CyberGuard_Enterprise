import { type Kysely, sql } from "kysely";

/**
 * SPEC-026 §Data contracts §1 — the dashboard's rights on ingest's tables (ADR-0022
 * Amendment 2026-11-20). Run as `cg_ingest_owner`, in the transaction Kysely runs every
 * pending migration in, after 0010_agent_certificates.
 *
 * - `triage_history`: one row for each change of an incident's status or assignee and of an
 *   alert's status, written by `triage_history_record`, a `SECURITY DEFINER` trigger
 *   function this role owns, so that no runtime role needs `INSERT` on the history; its
 *   actor is the database role that made the change (`session_user`), its time the
 *   transaction's. The history refuses any update, delete or truncate
 *   (`triage_history_write_once`, on the model of 0008's `playbook_executions_write_once`);
 *   that holds against a writer that goes through a runtime role, not against this role,
 *   which can drop the triggers.
 * - The unique key of an open incident: `incidents_grouping_key_unique` replaced by a unique
 *   index on `grouping_key` of the incidents not `resolved` or `false_positive`, on which
 *   grouping's upsert conflicts (SPEC-026 §Operational §7).
 * - The grants, when the roles exist (0009_runtime_grants): the api's column grants on
 *   `enrollment_tokens`, `agents`, `incidents` and `alerts`, its `SELECT` on the history
 *   through `cg_incident_reader`, and `cg_ingest`'s table-level `UPDATE` on `incidents`
 *   narrowed to the columns grouping writes, so that neither runtime role but the api
 *   changes a status or an assignee.
 */
export async function up(db: Kysely<unknown>): Promise<void> {
  await sql`
    CREATE TABLE triage_history (
      history_id      bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
      org_id          text        NOT NULL,
      object_kind     text        NOT NULL
                        CONSTRAINT triage_history_object_kind CHECK (object_kind IN ('incident', 'alert')),
      object_id       uuid        NOT NULL,
      field           text        NOT NULL
                        CONSTRAINT triage_history_field CHECK (field IN ('status', 'assigned_to')),
      old_value       text,
      new_value       text,
      changed_at      timestamptz NOT NULL,
      changed_by_role text        NOT NULL
    )
  `.execute(db);
  await sql`
    CREATE INDEX triage_history_object
      ON triage_history (org_id, object_kind, object_id, history_id)
  `.execute(db);

  // The history's writer. It runs as this role, and resolves names in ingest's schema
  // before any other, so that a runtime role cannot place an object it would resolve first.
  await sql`
    CREATE FUNCTION triage_history_record() RETURNS trigger
      LANGUAGE plpgsql SECURITY DEFINER SET search_path = ingest, pg_temp AS $$
    BEGIN
      IF TG_TABLE_NAME = 'incidents' THEN
        IF NEW.status IS DISTINCT FROM OLD.status THEN
          INSERT INTO triage_history
            (org_id, object_kind, object_id, field, old_value, new_value, changed_at, changed_by_role)
          VALUES (NEW.org_id, 'incident', NEW.incident_id, 'status', OLD.status, NEW.status,
                  now(), session_user);
        END IF;
        IF NEW.assigned_to IS DISTINCT FROM OLD.assigned_to THEN
          INSERT INTO triage_history
            (org_id, object_kind, object_id, field, old_value, new_value, changed_at, changed_by_role)
          VALUES (NEW.org_id, 'incident', NEW.incident_id, 'assigned_to', OLD.assigned_to,
                  NEW.assigned_to, now(), session_user);
        END IF;
      ELSIF NEW.status IS DISTINCT FROM OLD.status THEN
        INSERT INTO triage_history
          (org_id, object_kind, object_id, field, old_value, new_value, changed_at, changed_by_role)
        VALUES (NEW.org_id, 'alert', NEW.alert_id, 'status', OLD.status, NEW.status,
                now(), session_user);
      END IF;
      RETURN NULL;
    END
    $$
  `.execute(db);
  await sql`
    CREATE TRIGGER triage_history_record AFTER UPDATE OF status, assigned_to ON incidents
      FOR EACH ROW EXECUTE FUNCTION triage_history_record()
  `.execute(db);
  await sql`
    CREATE TRIGGER triage_history_record AFTER UPDATE OF status ON alerts
      FOR EACH ROW EXECUTE FUNCTION triage_history_record()
  `.execute(db);

  await sql`
    CREATE FUNCTION triage_history_write_once() RETURNS trigger
      LANGUAGE plpgsql SET search_path = pg_catalog, pg_temp AS $$
    BEGIN
      RAISE EXCEPTION 'triage_history: rows are never changed, deleted or truncated'
        USING ERRCODE = 'P0001';
    END
    $$
  `.execute(db);
  await sql`
    CREATE TRIGGER triage_history_write_once_row BEFORE UPDATE OR DELETE ON triage_history
      FOR EACH ROW EXECUTE FUNCTION triage_history_write_once()
  `.execute(db);
  await sql`
    CREATE TRIGGER triage_history_no_truncate BEFORE TRUNCATE ON triage_history
      FOR EACH STATEMENT EXECUTE FUNCTION triage_history_write_once()
  `.execute(db);

  // At most one open incident per key, any number of closed ones.
  await sql`ALTER TABLE incidents DROP CONSTRAINT incidents_grouping_key_unique`.execute(db);
  await sql`
    CREATE UNIQUE INDEX incidents_open_grouping_key_unique ON incidents (grouping_key)
      WHERE status NOT IN ('resolved', 'false_positive')
  `.execute(db);

  // The grants, when the runtime roles exist (SPEC-023 §Operational §4).
  await sql`
    DO $$
    BEGIN
      IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'cg_api') THEN
        GRANT INSERT (token_id, token_sha256, org_id, scope, expires_at, max_uses)
          ON enrollment_tokens TO cg_api;
        GRANT SELECT (token_id, org_id, scope, state, issued_at, expires_at, consumed_at,
                      max_uses, uses, withdrawn_at)
          ON enrollment_tokens TO cg_api;
        GRANT UPDATE (withdrawn_at) ON enrollment_tokens TO cg_api;
        GRANT SELECT (agent_id, org_id, enrolled_at, expires_at, last_seen, enrolled_by_token_id,
                      hostname, platform, agent_version, revoked_at, revocation_reason,
                      duplicate_signal, duplicate_signal_at)
          ON agents TO cg_api;
        GRANT UPDATE (revoked_at, revocation_reason) ON agents TO cg_api;
        GRANT UPDATE (status, assigned_to, updated_at) ON incidents TO cg_api;
        GRANT UPDATE (status, updated_at) ON alerts TO cg_api;
      END IF;
      IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'cg_incident_reader') THEN
        GRANT SELECT ON triage_history TO cg_incident_reader;
      END IF;
      IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'cg_ingest') THEN
        REVOKE UPDATE ON incidents FROM cg_ingest;
        GRANT UPDATE (alert_ids, severity_id, cg_mitre, updated_at) ON incidents TO cg_ingest;
      END IF;
    END
    $$
  `.execute(db);
}

/**
 * The history is a record a change of triage cannot be taken back from, and a database may
 * hold several incidents of one key once an incident has closed: the migration is not
 * reversed, and a database is taken back before it from a backup.
 */
export async function down(): Promise<void> {
  throw new Error(
    "0011_dashboard_rights is not reversible: the triage history would be lost, and closed incidents may share a key",
  );
}
