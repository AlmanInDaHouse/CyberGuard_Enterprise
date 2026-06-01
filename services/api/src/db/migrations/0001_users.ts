import { type Kysely, sql } from "kysely";

/**
 * SPEC-008 §Data contracts §1 — the `users` table (auth-core local identity,
 * ADR-0014 §1/§4). Owned by services/api (§Operational §7). Self-contained: it
 * ensures `pgcrypto` itself (the same idempotent statement ingest's 0001 uses),
 * so api does not depend on services/ingest having run first — `totp_secret` is
 * encrypted at rest with `pgp_sym_encrypt` at the GREEN gate.
 *
 * `role` is a CHECK-constrained column (one role per user on the record,
 * ADR-0014 §4 — no separate role table for three fixed global roles), mirroring
 * the `alerts.status` CHECK pattern. Idempotent (CREATE TABLE IF NOT EXISTS).
 */
export async function up(db: Kysely<unknown>): Promise<void> {
  await sql`CREATE EXTENSION IF NOT EXISTS pgcrypto`.execute(db);
  await sql`
    CREATE TABLE IF NOT EXISTS users (
      user_id        uuid        PRIMARY KEY,                 -- UUIDv7, slice-generated
      org_id         text        NOT NULL DEFAULT 'default',
      email          text        NOT NULL,                    -- login identifier (case-insensitive per org)
      password_hash  text        NOT NULL,                    -- Argon2id encoded string (GREEN gate)
      totp_secret    bytea       NOT NULL,                    -- pgcrypto-encrypted base32 secret; NEVER plaintext
      totp_enrolled  boolean     NOT NULL DEFAULT false,      -- false until first successful TOTP confirmation
      role           text        NOT NULL
                       CHECK (role IN ('admin', 'analyst', 'viewer')),
      status         text        NOT NULL DEFAULT 'active'
                       CHECK (status IN ('active', 'disabled')),
      created_at     timestamptz NOT NULL DEFAULT now(),
      updated_at     timestamptz NOT NULL DEFAULT now(),
      last_login_at  timestamptz,
      CONSTRAINT users_email_org_unique UNIQUE (org_id, email)
    )
  `.execute(db);
}

export async function down(db: Kysely<unknown>): Promise<void> {
  await sql`DROP TABLE IF EXISTS users`.execute(db);
}
