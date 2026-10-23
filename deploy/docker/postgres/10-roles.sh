#!/bin/sh
# SPEC-023 §Operational §4 — the five roles, created once by the Postgres image's
# initialisation (an empty data directory), as the superuser. The passwords come from the
# environment: psql reads each from it (\getenv) and quotes it as a literal, so none is on a
# command line. None of the roles is a superuser or has CREATEDB or CREATEROLE.
#
# The image's entrypoint runs this file, or sources it where a copy lost its mode: either
# way a missing password ends it with an error, and with it the initialisation and the
# container. No `set` here, which a sourced file would leave on the entrypoint.
: "${CG_PG_INGEST_OWNER_PASSWORD:?CG_PG_INGEST_OWNER_PASSWORD is required}"
: "${CG_PG_INGEST_PASSWORD:?CG_PG_INGEST_PASSWORD is required}"
: "${CG_PG_API_OWNER_PASSWORD:?CG_PG_API_OWNER_PASSWORD is required}"
: "${CG_PG_API_PASSWORD:?CG_PG_API_PASSWORD is required}"

psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<'SQL'
\getenv ingest_owner CG_PG_INGEST_OWNER_PASSWORD
\getenv ingest_runtime CG_PG_INGEST_PASSWORD
\getenv api_owner CG_PG_API_OWNER_PASSWORD
\getenv api_runtime CG_PG_API_PASSWORD
CREATE ROLE cg_ingest_owner LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE PASSWORD :'ingest_owner';
CREATE ROLE cg_ingest LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE PASSWORD :'ingest_runtime';
CREATE ROLE cg_api_owner LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE PASSWORD :'api_owner';
CREATE ROLE cg_api LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE PASSWORD :'api_runtime';
-- The api reads ingest's alerts, incidents and playbook executions through this role,
-- which ingest's migrations grant SELECT on them.
CREATE ROLE cg_incident_reader NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE;
GRANT cg_incident_reader TO cg_api;
SQL
