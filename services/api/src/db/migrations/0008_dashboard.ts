import { type Kysely, sql } from "kysely";

/**
 * SPEC-026 §Data contracts §2 — the dashboard's tables and rights on the api's side.
 *
 * - `users`: `password_hash` and `totp_secret` become nullable, both or neither, and a
 *   confirmed factor needs a secret. A user with neither is an invitation's or a reset's,
 *   and cannot sign in until it is completed (§Operational §3 and §4).
 * - `invitations`: an invitation's SHA-256, its user, its purpose, its 72 hours, the TOTP
 *   secret last shown to its holder, encrypted as `users.totp_secret` is, and its end. A user
 *   has at most one open invitation.
 * - `audit_log`: an index on `(org_id, id)`, the audit view's order.
 *
 * And `cg_api`'s rights, when the role exists, as `0007_runtime_grants` grants them: `SELECT`
 * on `audit_log`, besides its `INSERT`; on `invitations`, `SELECT`, the `INSERT` of the
 * columns the api writes (`issued_at` is the database's) and the `UPDATE` of the secret and
 * the invitation's end. No `DELETE` on either.
 *
 * Run again over its own schema, it changes nothing, as the migrations before it.
 */
export async function up(db: Kysely<unknown>): Promise<void> {
  await sql`
    ALTER TABLE users
      ALTER COLUMN password_hash DROP NOT NULL,
      ALTER COLUMN totp_secret DROP NOT NULL,
      DROP CONSTRAINT IF EXISTS users_credentials_paired,
      DROP CONSTRAINT IF EXISTS users_enrolled_has_secret,
      ADD CONSTRAINT users_credentials_paired
        CHECK ((password_hash IS NULL) = (totp_secret IS NULL)),
      ADD CONSTRAINT users_enrolled_has_secret
        CHECK (totp_secret IS NOT NULL OR NOT totp_enrolled)
  `.execute(db);
  await sql`
    CREATE TABLE IF NOT EXISTS invitations (
      invitation_id       uuid        PRIMARY KEY,
      user_id             uuid        NOT NULL REFERENCES users (user_id),
      org_id              text        NOT NULL,
      purpose             text        NOT NULL CHECK (purpose IN ('invite', 'reset')),
      token_sha256        bytea       NOT NULL UNIQUE,   -- SHA-256 of the text; the text is kept nowhere
      issued_at           timestamptz NOT NULL DEFAULT now(),
      expires_at          timestamptz NOT NULL CHECK (expires_at <= issued_at + interval '72 hours'),
      pending_totp_secret bytea,                         -- pgcrypto-encrypted, as users.totp_secret
      completed_at        timestamptz,
      withdrawn_at        timestamptz,
      CONSTRAINT invitations_one_end CHECK (completed_at IS NULL OR withdrawn_at IS NULL)
    )
  `.execute(db);
  await sql`
    CREATE UNIQUE INDEX IF NOT EXISTS invitations_open_per_user ON invitations (user_id)
      WHERE completed_at IS NULL AND withdrawn_at IS NULL
  `.execute(db);
  await sql`CREATE INDEX IF NOT EXISTS audit_log_org_id ON audit_log (org_id, id)`.execute(db);
  await sql`
    DO $$
    BEGIN
      IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'cg_api') THEN
        GRANT SELECT ON audit_log TO cg_api;
        GRANT SELECT ON invitations TO cg_api;
        GRANT INSERT (invitation_id, user_id, org_id, purpose, token_sha256, expires_at)
          ON invitations TO cg_api;
        GRANT UPDATE (pending_totp_secret, completed_at, withdrawn_at) ON invitations TO cg_api;
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
        REVOKE SELECT ON audit_log FROM cg_api;
      END IF;
    END
    $$
  `.execute(db);
  await sql`DROP INDEX IF EXISTS audit_log_org_id`.execute(db);
  await sql`DROP TABLE IF EXISTS invitations`.execute(db);
  await sql`
    ALTER TABLE users
      DROP CONSTRAINT IF EXISTS users_enrolled_has_secret,
      DROP CONSTRAINT IF EXISTS users_credentials_paired,
      ALTER COLUMN totp_secret SET NOT NULL,
      ALTER COLUMN password_hash SET NOT NULL
  `.execute(db);
}
