import { type Kysely, sql } from "kysely";

/**
 * SPEC-008 Amendment 2026-10-15 item 9 — a user's authority version. A role change,
 * a status change and a password change raise it in their transaction; the login
 * reads it with the user's row and again after it writes its session, and drops that
 * session if it moved, so a login racing a change keeps no session with the old
 * authority. Additive; existing users start at 0.
 */
export async function up(db: Kysely<unknown>): Promise<void> {
  await sql`
    ALTER TABLE users ADD COLUMN IF NOT EXISTS authority_version bigint NOT NULL DEFAULT 0
  `.execute(db);
}

export async function down(db: Kysely<unknown>): Promise<void> {
  await sql`ALTER TABLE users DROP COLUMN IF EXISTS authority_version`.execute(db);
}
