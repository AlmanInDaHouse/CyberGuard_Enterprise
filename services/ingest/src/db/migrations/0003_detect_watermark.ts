import { type Kysely, sql } from "kysely";

/**
 * SPEC-006 5b — detect_watermark: the per-org read-model progress cursor.
 * Postgres (mutable progress state, same rationale as the alerts table).
 *
 * `last_time` is TEXT (the ClickHouse DateTime64(9) string), NOT timestamptz:
 * ETW timestamps are nanosecond-precision and Postgres timestamptz truncates to
 * microseconds — a truncated watermark would re-process or skip boundary events
 * (the Phase-4 IEEE-754 lesson). TEXT preserves the exact nanos; the read-model
 * casts it back in the ClickHouse query. No row for an org ⇒ getWatermark
 * returns the epoch default ⇒ the first poll reads everything. Idempotent.
 */
export async function up(db: Kysely<unknown>): Promise<void> {
  await sql`
    CREATE TABLE IF NOT EXISTS detect_watermark (
      org_id     text        PRIMARY KEY,
      last_time  text        NOT NULL DEFAULT '1970-01-01 00:00:00.000000000',
      updated_at timestamptz NOT NULL DEFAULT now()
    )
  `.execute(db);
}

export async function down(db: Kysely<unknown>): Promise<void> {
  await sql`DROP TABLE IF EXISTS detect_watermark`.execute(db);
}
