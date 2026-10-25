#!/bin/bash
# ClickHouse's users (SPEC-023 §Operational §4), run once by the image's initialisation, on
# an empty data directory, as the image's user, the administrator cg_admin (CLICKHOUSE_USER).
# The passwords come from the environment, each stored as its SHA-256. The tables do not
# exist yet: the ingest-migrate one-shot creates them, and the grants and the row policies
# name them in advance.
#
#   cg_ingest  SELECT and INSERT on the database cyberguard; reads every row, as the
#              administrator does.
#   cg_api     readonly = 2, SELECT on cyberguard.cges_events alone, rows of class 1007 alone
#              (the drill's, the export's and the report's: SPEC-010, SPEC-012, SPEC-013).
#
# A subshell: the image may source this file, and nothing here may leak into its shell.
(
  set -eu
  sha256() { printf '%s' "$1" | sha256sum | cut -d ' ' -f 1; }
  ingest_hash=$(sha256 "${CG_CH_INGEST_PASSWORD:?}")
  api_hash=$(sha256 "${CG_CH_API_PASSWORD:?}")
  clickhouse-client --host 127.0.0.1 --user "$CLICKHOUSE_USER" --password "$CLICKHOUSE_PASSWORD" --multiquery <<SQL
CREATE USER IF NOT EXISTS cg_ingest IDENTIFIED WITH sha256_hash BY '$ingest_hash' DEFAULT DATABASE cyberguard;
GRANT SELECT, INSERT ON cyberguard.* TO cg_ingest;
CREATE USER IF NOT EXISTS cg_api IDENTIFIED WITH sha256_hash BY '$api_hash' DEFAULT DATABASE cyberguard SETTINGS readonly = 2;
GRANT SELECT ON cyberguard.cges_events TO cg_api;
CREATE ROW POLICY IF NOT EXISTS cg_api_process_activity ON cyberguard.cges_events FOR SELECT USING class_uid = 1007 TO cg_api;
CREATE ROW POLICY IF NOT EXISTS cg_every_row ON cyberguard.cges_events FOR SELECT USING 1 TO cg_ingest, cg_admin;
SQL
)
