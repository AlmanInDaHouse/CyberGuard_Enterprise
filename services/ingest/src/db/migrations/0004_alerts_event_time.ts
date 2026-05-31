import { type Kysely, sql } from "kysely";

/**
 * SPEC-007 §Data contracts §1 + ADR-0013 §1 — add `event_time` to `alerts`: the
 * event-OCCURRENCE time incident grouping windows on (ADR-0013 §1: event-time,
 * never insert-time). `created_at`/`updated_at` (0002) are insert/update-time only;
 * the only other event-time trace on the row is the `dedup_key` 5-minute bucket.
 *
 * New rows populate `event_time` at write from the source event time (see
 * `upsertAlert` in `../../detect/alerts.ts`, the same value `buildDedupKey`
 * consumes). Pre-existing rows are backfilled here from the `dedup_key` bucket: its
 * last `::`-delimited component is `floor(unix_seconds(time) / 300)`, so the bucket
 * component × 300 read as a UTC timestamp recovers the event time floored to the
 * 5-minute dedup bucket — self-contained in Postgres (no ClickHouse round-trip),
 * consistent with the dedup's event-time basis, and within the ≥ 1800 s correlation
 * window's tolerance (SPEC-007 §Data contracts §1 "Accepted imprecision").
 *
 * Order (SPEC-007 §Data contracts §1): add nullable → backfill from the bucket →
 * set NOT NULL. Idempotent: ADD/DROP COLUMN IF [NOT] EXISTS, UPDATE only the
 * still-null rows, SET NOT NULL is a no-op when already enforced.
 */
export async function up(db: Kysely<unknown>): Promise<void> {
  await sql`ALTER TABLE alerts ADD COLUMN IF NOT EXISTS event_time timestamptz`.execute(db);
  await sql`
    UPDATE alerts
    SET event_time = to_timestamp(split_part(dedup_key, '::', 4)::bigint * 300)
    WHERE event_time IS NULL
  `.execute(db);
  await sql`ALTER TABLE alerts ALTER COLUMN event_time SET NOT NULL`.execute(db);
}

export async function down(db: Kysely<unknown>): Promise<void> {
  await sql`ALTER TABLE alerts DROP COLUMN IF EXISTS event_time`.execute(db);
}
