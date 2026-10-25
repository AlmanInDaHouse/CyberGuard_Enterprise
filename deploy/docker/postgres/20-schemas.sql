-- SPEC-023 §Operational §4 — in the database `cyberguard` (POSTGRES_DB), as the superuser,
-- once: the extensions in `public`, owned by the superuser; the schemas `ingest` and `api`,
-- owned by their services' owner roles; each runtime role's USAGE on its own schema and no
-- default privileges, so that a table its owner creates gives it no right until a migration
-- grants one; CONNECT for the login roles; and PUBLIC's CREATE on `public` and TEMPORARY on
-- the database revoked. No runtime role owns an object or can create one.
CREATE EXTENSION IF NOT EXISTS pgcrypto WITH SCHEMA public;
CREATE EXTENSION IF NOT EXISTS vector WITH SCHEMA public;

CREATE SCHEMA ingest AUTHORIZATION cg_ingest_owner;
CREATE SCHEMA api AUTHORIZATION cg_api_owner;

GRANT USAGE ON SCHEMA ingest TO cg_ingest;
GRANT USAGE ON SCHEMA api TO cg_api;

REVOKE CREATE ON SCHEMA public FROM PUBLIC;
REVOKE ALL ON DATABASE cyberguard FROM PUBLIC;
GRANT CONNECT ON DATABASE cyberguard TO cg_ingest_owner, cg_ingest, cg_api_owner, cg_api;
-- The image's other databases too: the runtime roles connect to `cyberguard` alone, and so
-- create nothing elsewhere, a temporary table or a large object in `postgres` or `template1`
-- included.
REVOKE CONNECT, TEMPORARY ON DATABASE postgres FROM PUBLIC;
REVOKE CONNECT ON DATABASE template1 FROM PUBLIC;

-- No runtime role creates or owns a large object (ADR-0022 §4): no privilege governs their
-- creation, so the functions that create one are closed to PUBLIC. Nothing of CyberGuard's
-- uses them.
REVOKE EXECUTE ON FUNCTION lo_create(oid), lo_creat(integer), lo_from_bytea(oid, bytea) FROM PUBLIC;

-- Debt #122 (SPEC-023 Amendment 2026-10-25): no runtime role takes an advisory lock, so none
-- can hold a migration's key and stall the one-shots; the owner roles, which the migrations
-- run as, take them. Function privileges are the database's, as these are `cyberguard`'s.
REVOKE EXECUTE ON FUNCTION
  pg_advisory_lock(bigint), pg_advisory_lock(integer, integer),
  pg_advisory_lock_shared(bigint), pg_advisory_lock_shared(integer, integer),
  pg_advisory_xact_lock(bigint), pg_advisory_xact_lock(integer, integer),
  pg_advisory_xact_lock_shared(bigint), pg_advisory_xact_lock_shared(integer, integer),
  pg_try_advisory_lock(bigint), pg_try_advisory_lock(integer, integer),
  pg_try_advisory_lock_shared(bigint), pg_try_advisory_lock_shared(integer, integer),
  pg_try_advisory_xact_lock(bigint), pg_try_advisory_xact_lock(integer, integer),
  pg_try_advisory_xact_lock_shared(bigint), pg_try_advisory_xact_lock_shared(integer, integer)
FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  pg_advisory_lock(bigint), pg_advisory_lock(integer, integer),
  pg_advisory_lock_shared(bigint), pg_advisory_lock_shared(integer, integer),
  pg_advisory_xact_lock(bigint), pg_advisory_xact_lock(integer, integer),
  pg_advisory_xact_lock_shared(bigint), pg_advisory_xact_lock_shared(integer, integer),
  pg_try_advisory_lock(bigint), pg_try_advisory_lock(integer, integer),
  pg_try_advisory_lock_shared(bigint), pg_try_advisory_lock_shared(integer, integer),
  pg_try_advisory_xact_lock(bigint), pg_try_advisory_xact_lock(integer, integer),
  pg_try_advisory_xact_lock_shared(bigint), pg_try_advisory_xact_lock_shared(integer, integer)
TO cg_ingest_owner, cg_api_owner;
