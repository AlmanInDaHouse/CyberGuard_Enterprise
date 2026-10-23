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

-- No runtime role creates or owns a large object (ADR-0022 §4): no privilege governs their
-- creation, so the functions that create one are closed to PUBLIC. Nothing of CyberGuard's
-- uses them.
REVOKE EXECUTE ON FUNCTION lo_create(oid), lo_creat(integer), lo_from_bytea(oid, bytea) FROM PUBLIC;
