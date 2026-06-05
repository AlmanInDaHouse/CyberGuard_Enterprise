import { type Kysely, sql } from "kysely";

/**
 * SPEC-011 §Operational §1 — add aggregated `severity_id` to `incidents`: the
 * OCSF ordinal 0–6 MAX severity over the incident's member alerts. New rows set it
 * in the upsert (`../../detect/incidents.ts`, `GREATEST(incidents.severity_id,
 * excluded.severity_id)`); pre-existing rows are backfilled here from
 * `MAX(alerts.severity_id)` over `alert_ids`.
 *
 * Order (SPEC-011 §Operational §1, mirroring 0004's add → backfill → NOT NULL):
 * add nullable → backfill from the member alerts' MAX → set NOT NULL + range CHECK.
 * The `SET NOT NULL` MUST follow the backfill: `ADD COLUMN` leaves existing rows
 * NULL, so enforcing NOT NULL first would reject them. The whole `migrateToLatest`
 * run is one transaction on Postgres (Kysely Migrator, `supportsTransactionalDdl`),
 * so a residual NULL at step (c) rolls the column back atomically.
 *
 * The range CHECK is a byte-mirror of `alerts_severity_range` (0002_alerts.ts:50):
 * incident and alert severity share the OCSF 0–6 scale. Idempotent: ADD/DROP COLUMN
 * IF [NOT] EXISTS, UPDATE only the still-null rows.
 *
 * Backfill is fail-loud (SPEC-011 §Risks): an incident whose `alert_ids` resolve to
 * NO alerts row yields `MAX(...) = NULL`, and the SET NOT NULL then fails the
 * migration — surfacing data corruption rather than masking it with a fallback. In
 * a faithful DB every `alert_ids` entry references a real alert (created together,
 * Convention #12), so MAX is non-null.
 */
export async function up(db: Kysely<unknown>): Promise<void> {
  await sql`ALTER TABLE incidents ADD COLUMN IF NOT EXISTS severity_id smallint`.execute(db);
  await sql`
    UPDATE incidents
    SET severity_id = (
      SELECT MAX(a.severity_id) FROM alerts a WHERE a.alert_id = ANY(incidents.alert_ids)
    )
    WHERE severity_id IS NULL
  `.execute(db);
  await sql`ALTER TABLE incidents ALTER COLUMN severity_id SET NOT NULL`.execute(db);
  await sql`
    ALTER TABLE incidents
    ADD CONSTRAINT incidents_severity_range CHECK (severity_id BETWEEN 0 AND 6)
  `.execute(db);
}

export async function down(db: Kysely<unknown>): Promise<void> {
  await sql`ALTER TABLE incidents DROP COLUMN IF EXISTS severity_id`.execute(db);
}
