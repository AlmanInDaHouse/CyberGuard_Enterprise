import { type Kysely, sql } from "kysely";

/**
 * SPEC-025 §Data contracts §1 — the enrollment token at rest, the agent's certificates,
 * revocation and a copied identity. Run as `cg_ingest_owner`, in the transaction Kysely runs
 * every pending migration in; it references no new enum value.
 *
 * - `enrollment_tokens`: the token's SHA-256 under an identifier, its uses and its cap, its
 *   withdrawal; the plain `token` and `consumed_by_agent_id` dropped, after the latter is
 *   copied to `agents.enrolled_by_token_id`; each existing row's expiry capped at its bound
 *   before the bounds' checks are added.
 * - `agents`: the enrolling token, what the agent last sent, the revocation, the process
 *   starts and the copy signal, and the pending certificate of a renewal.
 * - Three triggers, whose functions this role owns: a revocation and a copy signal written
 *   once, a token's terms fixed and its uses only growing, and a token's issue dated by the
 *   database.
 * - `cg_ingest`'s table-level `INSERT` and `UPDATE` on `agents` narrowed to the columns its
 *   code writes, when the role exists (0009_runtime_grants).
 */
export async function up(db: Kysely<unknown>): Promise<void> {
  // enrollment_tokens: the identifier and the hash, filled for each existing row.
  await sql`ALTER TABLE enrollment_tokens ADD COLUMN token_id uuid`.execute(db);
  await sql`UPDATE enrollment_tokens SET token_id = gen_random_uuid()`.execute(db);
  await sql`ALTER TABLE enrollment_tokens ADD COLUMN token_sha256 bytea`.execute(db);
  // pgcrypto lives in `public` (0001_initial); the token's text is base64url, ASCII.
  await sql`UPDATE enrollment_tokens SET token_sha256 = public.digest(token, 'sha256')`.execute(db);
  await sql`
    ALTER TABLE enrollment_tokens
      ALTER COLUMN token_id SET NOT NULL,
      ALTER COLUMN token_sha256 SET NOT NULL,
      ADD CONSTRAINT enrollment_tokens_token_sha256_key UNIQUE (token_sha256),
      ADD CONSTRAINT enrollment_tokens_token_sha256_length CHECK (octet_length(token_sha256) = 32),
      ADD COLUMN max_uses integer NOT NULL DEFAULT 1
        CONSTRAINT enrollment_tokens_max_uses_range CHECK (max_uses BETWEEN 1 AND 1000),
      ADD COLUMN uses integer NOT NULL DEFAULT 0,
      ADD COLUMN withdrawn_at timestamptz
  `.execute(db);
  await sql`UPDATE enrollment_tokens SET uses = 1 WHERE state = 'consumed'`.execute(db);
  await sql`
    ALTER TABLE enrollment_tokens
      ADD CONSTRAINT enrollment_tokens_uses_range CHECK (uses BETWEEN 0 AND max_uses)
  `.execute(db);

  // agents: the columns, then the enrolling token copied before the old column goes.
  await sql`
    ALTER TABLE agents
      ADD COLUMN hostname text,
      ADD COLUMN platform text,
      ADD COLUMN agent_version text,
      ADD COLUMN revoked_at timestamptz,
      ADD COLUMN revocation_reason text
        CONSTRAINT agents_revocation_reason_length
          CHECK (char_length(revocation_reason) BETWEEN 1 AND 500),
      ADD CONSTRAINT agents_revocation_has_reason
        CHECK ((revoked_at IS NULL) = (revocation_reason IS NULL)),
      ADD COLUMN process_started_at timestamptz,
      ADD COLUMN previous_process_started_at timestamptz,
      ADD COLUMN duplicate_signal text
        CONSTRAINT agents_duplicate_signal_value
          CHECK (duplicate_signal IN ('superseded_certificate', 'interleaved_process')),
      ADD COLUMN duplicate_signal_at timestamptz,
      ADD COLUMN pending_cert_pem text
  `.execute(db);

  // The identifier becomes the key the agent's row references.
  await sql`ALTER TABLE enrollment_tokens DROP CONSTRAINT enrollment_tokens_pkey`.execute(db);
  await sql`ALTER TABLE enrollment_tokens ADD PRIMARY KEY (token_id)`.execute(db);
  await sql`
    ALTER TABLE agents
      ADD COLUMN enrolled_by_token_id uuid REFERENCES enrollment_tokens (token_id)
  `.execute(db);
  await sql`
    UPDATE agents a SET enrolled_by_token_id = t.token_id
      FROM enrollment_tokens t
     WHERE t.consumed_by_agent_id = a.agent_id
  `.execute(db);
  await sql`ALTER TABLE enrollment_tokens DROP COLUMN consumed_by_agent_id`.execute(db);
  await sql`ALTER TABLE enrollment_tokens DROP COLUMN token`.execute(db);

  // Every existing row is single-use: its expiry is capped at 15 minutes after its issue,
  // which the 7 days' bound then holds too.
  await sql`
    UPDATE enrollment_tokens
       SET expires_at = LEAST(
             expires_at,
             issued_at + interval '7 days',
             CASE WHEN max_uses = 1 THEN issued_at + interval '15 minutes' END)
  `.execute(db);
  await sql`
    ALTER TABLE enrollment_tokens
      ADD CONSTRAINT enrollment_tokens_lifetime CHECK (expires_at <= issued_at + interval '7 days'),
      ADD CONSTRAINT enrollment_tokens_single_use_lifetime
        CHECK (max_uses <> 1 OR expires_at <= issued_at + interval '15 minutes')
  `.execute(db);

  // The triggers. Their functions resolve nothing through the search path.
  await sql`
    CREATE FUNCTION agents_write_once() RETURNS trigger
      LANGUAGE plpgsql SET search_path = pg_catalog, pg_temp AS $$
    BEGIN
      IF OLD.revoked_at IS NOT NULL AND (
           NEW.revoked_at IS DISTINCT FROM OLD.revoked_at
           OR NEW.revocation_reason IS DISTINCT FROM OLD.revocation_reason) THEN
        RAISE EXCEPTION 'revocation is write-once' USING ERRCODE = 'P0001';
      END IF;
      IF (OLD.duplicate_signal IS NOT NULL
            AND NEW.duplicate_signal IS DISTINCT FROM OLD.duplicate_signal)
         OR (OLD.duplicate_signal_at IS NOT NULL
            AND NEW.duplicate_signal_at IS DISTINCT FROM OLD.duplicate_signal_at) THEN
        RAISE EXCEPTION 'duplicate signal is write-once' USING ERRCODE = 'P0001';
      END IF;
      IF OLD.revoked_at IS NULL AND NEW.revoked_at IS NOT NULL THEN
        NEW.revoked_at := now();
      END IF;
      RETURN NEW;
    END
    $$
  `.execute(db);
  await sql`
    CREATE TRIGGER agents_write_once BEFORE UPDATE ON agents
      FOR EACH ROW EXECUTE FUNCTION agents_write_once()
  `.execute(db);

  await sql`
    CREATE FUNCTION enrollment_tokens_issue() RETURNS trigger
      LANGUAGE plpgsql SET search_path = pg_catalog, pg_temp AS $$
    BEGIN
      NEW.issued_at := now();
      NEW.uses := 0;
      NEW.state := 'issued';
      NEW.consumed_at := NULL;
      NEW.withdrawn_at := NULL;
      RETURN NEW;
    END
    $$
  `.execute(db);
  await sql`
    CREATE TRIGGER enrollment_tokens_issue BEFORE INSERT ON enrollment_tokens
      FOR EACH ROW EXECUTE FUNCTION enrollment_tokens_issue()
  `.execute(db);

  await sql`
    CREATE FUNCTION enrollment_tokens_write_once() RETURNS trigger
      LANGUAGE plpgsql SET search_path = pg_catalog, pg_temp AS $$
    BEGIN
      IF OLD.withdrawn_at IS NOT NULL AND NEW.withdrawn_at IS DISTINCT FROM OLD.withdrawn_at THEN
        RAISE EXCEPTION 'withdrawal is write-once' USING ERRCODE = 'P0001';
      END IF;
      IF NEW.token_sha256 IS DISTINCT FROM OLD.token_sha256
         OR NEW.org_id IS DISTINCT FROM OLD.org_id
         OR NEW.scope IS DISTINCT FROM OLD.scope
         OR NEW.issued_at IS DISTINCT FROM OLD.issued_at
         OR NEW.expires_at IS DISTINCT FROM OLD.expires_at
         OR NEW.max_uses IS DISTINCT FROM OLD.max_uses THEN
        RAISE EXCEPTION 'token terms are fixed' USING ERRCODE = 'P0001';
      END IF;
      IF NEW.uses < OLD.uses
         OR (NEW.state = 'issued' AND OLD.state IS DISTINCT FROM 'issued') THEN
        RAISE EXCEPTION 'token uses only grow' USING ERRCODE = 'P0001';
      END IF;
      IF OLD.withdrawn_at IS NULL AND NEW.withdrawn_at IS NOT NULL THEN
        NEW.withdrawn_at := now();
      END IF;
      RETURN NEW;
    END
    $$
  `.execute(db);
  await sql`
    CREATE TRIGGER enrollment_tokens_write_once BEFORE UPDATE ON enrollment_tokens
      FOR EACH ROW EXECUTE FUNCTION enrollment_tokens_write_once()
  `.execute(db);

  // The grants, when the runtime role exists (SPEC-023 §Operational §4).
  await sql`
    DO $$
    BEGIN
      IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'cg_ingest') THEN
        REVOKE INSERT, UPDATE ON agents FROM cg_ingest;
        GRANT INSERT (agent_id, org_id, pubkey, cert_pem, enrolled_at, expires_at,
                      enrolled_by_token_id, hostname, platform, agent_version)
          ON agents TO cg_ingest;
        GRANT UPDATE (pubkey, cert_pem, expires_at, last_seen, hostname, platform,
                      agent_version, process_started_at, previous_process_started_at,
                      duplicate_signal, duplicate_signal_at, pending_cert_pem)
          ON agents TO cg_ingest;
      END IF;
    END
    $$
  `.execute(db);
}

/**
 * The plain tokens cannot be recovered from their hashes: the migration is not reversed,
 * and a database is taken back before it from a backup.
 */
export async function down(): Promise<void> {
  throw new Error("0010_agent_certificates is not reversible: the plain tokens are gone");
}
