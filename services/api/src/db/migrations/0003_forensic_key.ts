import { type Kysely, sql } from "kysely";

/**
 * SPEC-012 §Data contracts §4 / ADR-0016 §5 — the dedicated forensic Ed25519 signing
 * key, at rest. A single-row table mirroring the ingest CA precedent
 * (services/ingest/src/db/migrations/0001_initial.ts:10-16): `id smallint PRIMARY KEY
 * DEFAULT 1 CHECK (id = 1)`, the PRIVATE key encrypted at rest with pgcrypto
 * `pgp_sym_encrypt` (bootstrapped by ensureForensicKey, src/forensic/key.ts), and the
 * PUBLIC key stored as plaintext raw bytes — it is published in the snapshot export
 * (SPEC-012 §Data contracts §3), so it is not a secret.
 *
 * This is the api's FIRST signing key; it is NOT the ingest CA (ADR-0016 §5 + §Compliance
 * line 103 — the api MUST NOT reach into services/ingest for key material; the ADR-0014
 * human↔agent trust boundary). The whole forensic module lives in services/api.
 *
 * pgcrypto is ALREADY enabled by 0001_users (0001_users.ts:15), so this migration does
 * NOT re-`CREATE EXTENSION`. Owned by services/api. Idempotent (`CREATE TABLE IF NOT
 * EXISTS`); auto-discovered by the filesystem migration provider (db/migrate.ts:31-47).
 */
export async function up(db: Kysely<unknown>): Promise<void> {
  await sql`
    CREATE TABLE IF NOT EXISTS forensic_key (
      id          smallint    PRIMARY KEY DEFAULT 1 CHECK (id = 1),
      private_key bytea       NOT NULL,   -- pgp_sym_encrypt'd PKCS#8 (base64); NEVER plaintext
      public_key  bytea       NOT NULL,   -- raw 32-byte Ed25519 public key (published in the export)
      created_at  timestamptz NOT NULL DEFAULT now()
    )
  `.execute(db);
}

export async function down(db: Kysely<unknown>): Promise<void> {
  await sql`DROP TABLE IF EXISTS forensic_key`.execute(db);
}
