# Postgres initialisation

Mounted at `/docker-entrypoint-initdb.d` of the bundle's `postgres` service, these files run once, on an empty data directory, as the superuser: the five roles (`10-roles.sh`, the passwords from `CG_PG_*`), the extensions, the schemas `ingest` and `api` and their privileges (`20-schemas.sql`), and the logging settings that keep a statement's parameters out of the log (`30-logging.sql`). A role or a grant changed later does not reach an installed stack through them; the services' migrations grant each table (SPEC-023 §Operational §4).
