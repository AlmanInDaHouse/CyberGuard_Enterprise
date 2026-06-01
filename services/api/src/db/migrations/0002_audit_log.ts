import { type Kysely, sql } from "kysely";

/**
 * SPEC-008 §Data contracts §2 — the append-only `audit_log` table. Fields per
 * the threat model (user_id, org_id, action, target_id, timestamp, correlation
 * id; threat-model § cg-api Repudiation). Append-only is an app-level contract
 * (the service path performs INSERT only); the `id GENERATED ALWAYS AS IDENTITY`
 * additionally blocks a client-supplied id. Owned by services/api. Idempotent.
 */
export async function up(db: Kysely<unknown>): Promise<void> {
  await sql`
    CREATE TABLE IF NOT EXISTS audit_log (
      id             bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
      occurred_at    timestamptz NOT NULL DEFAULT now(),       -- the "timestamp"
      user_id        uuid,                                     -- NULL for a failed login w/ unknown user
      org_id         text        NOT NULL DEFAULT 'default',
      action         text        NOT NULL,                     -- e.g. auth.login.ok / role.change / token.issue
      target_id      text,                                     -- the object acted on
      outcome        text        NOT NULL CHECK (outcome IN ('ok', 'denied', 'error')),
      source_ip      inet,
      correlation_id uuid
    )
  `.execute(db);
}

export async function down(db: Kysely<unknown>): Promise<void> {
  await sql`DROP TABLE IF EXISTS audit_log`.execute(db);
}
