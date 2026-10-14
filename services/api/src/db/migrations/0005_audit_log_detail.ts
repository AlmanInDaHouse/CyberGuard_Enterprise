import { type Kysely, sql } from "kysely";

/**
 * SPEC-008 Amendment 2026-10-15 item 6 — the audit row's `detail`, a nullable jsonb
 * that holds only role and status values (`{ "role": … }`, `{ "from": …, "to": … }`),
 * never an address, a password, a code or a token. Additive; rows written before
 * it read NULL.
 */
export async function up(db: Kysely<unknown>): Promise<void> {
  await sql`ALTER TABLE audit_log ADD COLUMN IF NOT EXISTS detail jsonb`.execute(db);
}

export async function down(db: Kysely<unknown>): Promise<void> {
  await sql`ALTER TABLE audit_log DROP COLUMN IF EXISTS detail`.execute(db);
}
