#!/bin/sh
# CyberGuard's restore (SPEC-023 §Operational §10). Run it in the bundle's directory, with
# the .env of the installation that was backed up, on a stack whose stores are empty, as the
# user setup.sh ran as (root: .env is that user's, mode 0600), with gpg (2.2.7 or later) on
# the host:
#
#   ./restore.sh <cyberguard-postgres-….dump.gpg> <cyberguard-clickhouse-….zip.gpg>
#
# Before it stops or restores anything, it decrypts both files once, so that a wrong
# passphrase or a file gpg finds manipulated stops it there, and starts the stores and
# refuses unless both are empty. Then it stops ingest, the api and the dashboard, so that neither service
# generates a CA or a forensic key first, the migration one-shots not having run; restores
# the dump as the superuser (pg_restore --clean --if-exists), over the roles and schemas the
# initialisation created, and the ClickHouse archive, decrypted onto ClickHouse's backup
# disk, restored, and deleted from it; then starts the stack, whose one-shots find every
# migration applied. Redis is not restored: users sign in again. The passphrase reaches gpg
# on a file descriptor. docker compose reads COMPOSE_PROJECT_NAME and COMPOSE_FILE, when
# set, as it always does.
set -eu
umask 077

say() { printf 'restore.sh: %s\n' "$*"; }
fail() {
  say "$*"
  exit 1
}

# The last value .env gives a variable, without the quotes Compose strips from it.
value() {
  sed -n "s/^$1=//p" .env | sed -n '$p' | sed -e 's/^"\(.*\)"$/\1/' -e "s/^'\(.*\)'\$/\1/"
}

[ "$#" -eq 2 ] || fail "usage: restore.sh <postgres dump .gpg> <clickhouse archive .gpg>"
pg_file=$1
ch_file=$2
[ -f "$pg_file" ] || fail "no file $pg_file"
[ -f "$ch_file" ] || fail "no file $ch_file"
[ -f .env ] || fail "no .env: run me in the bundle's directory, with the backed-up installation's .env"
[ -r .env ] || fail ".env is not readable: run me as the user setup.sh ran as"
command -v gpg >/dev/null 2>&1 || fail "gpg not found: it is a prerequisite of the host"
passphrase=$(value CG_BACKUP_PASSPHRASE)
[ -n "$passphrase" ] || fail "CG_BACKUP_PASSPHRASE is empty: this .env is not the backed-up installation's"

archive="restore-$(date -u +%Y%m%dT%H%M%SZ).zip"
status=$(mktemp)
cleanup() {
  docker compose exec -T clickhouse rm -f "/backups/$archive" >/dev/null 2>&1 || true
  rm -f "$status"
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM

# $1 decrypted onto standard output. No passphrase cache in gpg-agent. gpg writes the whole
# plaintext before it reports a manipulated file, so its exit status is what counts.
decrypt() {
  gpg --batch --quiet --no-symkey-cache --pinentry-mode loopback --passphrase-fd 3 \
    --decrypt "$1" 3<<EOF
$passphrase
EOF
}

# A pipeline's first command, its exit status written to the status file, which POSIX sh
# does not give; the file is emptied before each pipeline, so that a stale status reads as
# a failure.
first() {
  rc=0
  "$@" || rc=$?
  echo "$rc" > "$status"
}
first_ok() { [ "$(cat "$status")" = 0 ]; }

say "checking both files"
for file in "$pg_file" "$ch_file"; do
  decrypt "$file" > /dev/null || fail "$file does not decrypt with this .env's passphrase, or gpg finds it manipulated"
done

say "starting the stores"
docker compose up -d --wait postgres clickhouse redis || fail "the stores did not start"
tables=$(docker compose exec -T postgres psql -U postgres -d cyberguard -Atc \
  "SELECT count(*) FROM pg_tables WHERE schemaname IN ('ingest', 'api')")
[ "$tables" = 0 ] || fail "Postgres is not empty ($tables tables in ingest and api): restore into a new stack"
ch_tables=$(docker compose exec -T clickhouse clickhouse-client \
  -q "SELECT count() FROM system.tables WHERE database = 'cyberguard'")
[ "$ch_tables" = 0 ] || fail "ClickHouse is not empty ($ch_tables tables in cyberguard): restore into a new stack"

say "stopping the dashboard, the api and ingest"
docker compose stop dashboard api ingest >/dev/null 2>&1 || true

say "the Postgres dump"
: > "$status"
first decrypt "$pg_file" |
  docker compose exec -T postgres pg_restore -U postgres -d cyberguard --clean --if-exists --exit-on-error ||
  fail "pg_restore failed"
first_ok || fail "decrypting $pg_file failed"

say "the ClickHouse archive"
# Decrypted onto the backup disk, restored only when gpg found it whole, then deleted.
: > "$status"
first decrypt "$ch_file" | docker compose exec -T clickhouse sh -c "cat > /backups/$archive" ||
  fail "writing the archive to the backup disk failed"
first_ok || fail "decrypting $ch_file failed"
# clickhouse-client reads the administrator's name and password from the container's
# environment (CLICKHOUSE_USER, CLICKHOUSE_PASSWORD).
docker compose exec -T clickhouse clickhouse-client \
  -q "RESTORE DATABASE cyberguard FROM Disk('backups', '$archive')" >/dev/null ||
  fail "RESTORE DATABASE failed"
docker compose exec -T clickhouse rm -f "/backups/$archive" ||
  fail "the archive could not be deleted from the backup disk: remove /backups/$archive in the clickhouse container"

say "starting the stack"
docker compose up -d --wait || fail "the stack did not start"
say "restored $pg_file and $ch_file; users sign in again"
