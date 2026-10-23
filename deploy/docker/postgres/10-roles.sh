#!/bin/sh
# SPEC-023 §Operational §4 — the five roles, created once by the Postgres image's
# initialisation (an empty data directory), as the superuser. The passwords come from the
# environment, so this is a shell script; psql quotes each as a literal. None of the roles
# is a superuser or has CREATEDB or CREATEROLE.
#
# The image's entrypoint sources this file under `set -e`: a missing password stops the
# initialisation, and with it the container.
: "${CG_PG_INGEST_OWNER_PASSWORD:?CG_PG_INGEST_OWNER_PASSWORD is required}"
: "${CG_PG_INGEST_PASSWORD:?CG_PG_INGEST_PASSWORD is required}"
: "${CG_PG_API_OWNER_PASSWORD:?CG_PG_API_OWNER_PASSWORD is required}"
: "${CG_PG_API_PASSWORD:?CG_PG_API_PASSWORD is required}"

psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" \
  -v ingest_owner="$CG_PG_INGEST_OWNER_PASSWORD" \
  -v ingest_runtime="$CG_PG_INGEST_PASSWORD" \
  -v api_owner="$CG_PG_API_OWNER_PASSWORD" \
  -v api_runtime="$CG_PG_API_PASSWORD" <<'SQL'
CREATE ROLE cg_ingest_owner LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE PASSWORD :'ingest_owner';
CREATE ROLE cg_ingest LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE PASSWORD :'ingest_runtime';
CREATE ROLE cg_api_owner LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE PASSWORD :'api_owner';
CREATE ROLE cg_api LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE PASSWORD :'api_runtime';
-- The api reads ingest's alerts, incidents and playbook executions through this role,
-- which ingest's migrations grant SELECT on them.
CREATE ROLE cg_incident_reader NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE;
GRANT cg_incident_reader TO cg_api;
SQL
