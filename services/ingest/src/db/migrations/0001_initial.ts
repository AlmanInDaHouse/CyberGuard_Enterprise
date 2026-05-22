import { type Kysely, sql } from "kysely";

/** SPEC-004 §Data contracts — Postgres schema. Idempotent (MUST-2). */
export async function up(db: Kysely<unknown>): Promise<void> {
  await sql`
    CREATE TABLE IF NOT EXISTS ca (
      id          smallint    PRIMARY KEY DEFAULT 1 CHECK (id = 1),
      cert_pem    text        NOT NULL,
      private_key bytea       NOT NULL,
      created_at  timestamptz NOT NULL DEFAULT now()
    )
  `.execute(db);

  await sql`
    CREATE TABLE IF NOT EXISTS agents (
      agent_id    uuid        PRIMARY KEY,
      org_id      text        NOT NULL DEFAULT 'default',
      pubkey      bytea       NOT NULL,
      cert_pem    text        NOT NULL,
      enrolled_at timestamptz NOT NULL DEFAULT now(),
      expires_at  timestamptz NOT NULL,
      last_seen   timestamptz
    )
  `.execute(db);

  await sql`
    DO $$ BEGIN
      IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'token_state') THEN
        CREATE TYPE token_state AS ENUM ('issued', 'consumed', 'expired');
      END IF;
    END $$
  `.execute(db);

  await sql`
    CREATE TABLE IF NOT EXISTS enrollment_tokens (
      token                text        PRIMARY KEY,
      org_id               text        NOT NULL DEFAULT 'default',
      scope                text        NOT NULL DEFAULT 'enroll',
      state                token_state NOT NULL DEFAULT 'issued',
      issued_at            timestamptz NOT NULL DEFAULT now(),
      expires_at           timestamptz NOT NULL,
      consumed_at          timestamptz,
      consumed_by_agent_id uuid        REFERENCES agents(agent_id)
    )
  `.execute(db);
}

export async function down(db: Kysely<unknown>): Promise<void> {
  await sql`DROP TABLE IF EXISTS enrollment_tokens`.execute(db);
  await sql`DROP TABLE IF EXISTS agents`.execute(db);
  await sql`DROP TABLE IF EXISTS ca`.execute(db);
  await sql`DROP TYPE IF EXISTS token_state`.execute(db);
}
