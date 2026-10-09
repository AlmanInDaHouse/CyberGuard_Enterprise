import { type Kysely, sql } from "kysely";

/**
 * SPEC-018 §Data contracts / §Operational §6 — the detection cursor becomes the
 * pair (arrived_at, event_id) of the last row read: `last_arrived_at` (TEXT, the
 * ClickHouse DateTime64(3) string, read back by the read-model as a typed query
 * parameter) and `last_event_id` (uuid); `last_time`, the SPEC-006 event-time
 * watermark, is dropped. An existing row takes the column defaults, so after the
 * migration every org's cursor is at the beginning and the first cycles re-read
 * the stored events (§Operational §6).
 *
 * Idempotent: ADD/DROP COLUMN IF [NOT] EXISTS. `down` restores `last_time` with
 * its 0003 default (the epoch: the old read-model then re-reads from the start).
 */
export async function up(db: Kysely<unknown>): Promise<void> {
  await sql`
    ALTER TABLE detect_watermark
    ADD COLUMN IF NOT EXISTS last_arrived_at text NOT NULL DEFAULT '1970-01-01 00:00:00.000'
  `.execute(db);
  await sql`
    ALTER TABLE detect_watermark
    ADD COLUMN IF NOT EXISTS last_event_id uuid NOT NULL
      DEFAULT '00000000-0000-0000-0000-000000000000'
  `.execute(db);
  await sql`ALTER TABLE detect_watermark DROP COLUMN IF EXISTS last_time`.execute(db);
}

export async function down(db: Kysely<unknown>): Promise<void> {
  await sql`
    ALTER TABLE detect_watermark
    ADD COLUMN IF NOT EXISTS last_time text NOT NULL DEFAULT '1970-01-01 00:00:00.000000000'
  `.execute(db);
  await sql`ALTER TABLE detect_watermark DROP COLUMN IF EXISTS last_event_id`.execute(db);
  await sql`ALTER TABLE detect_watermark DROP COLUMN IF EXISTS last_arrived_at`.execute(db);
}
