#!/bin/sh
# CyberGuard's backup (SPEC-023 §Operational §10). Run it in the bundle's directory, as the
# user setup.sh ran as (root: .env is that user's, mode 0600), with the stack up and gpg
# (2.2.7 or later) on the host.
#
# Writes two files into CG_BACKUP_DIR (mode 0700), each with the date in its name and each
# encrypted with gpg (AES256) under CG_BACKUP_PASSPHRASE as it leaves its container: a
# pg_dump of the database cyberguard in custom format, and a ClickHouse BACKUP of the
# database cyberguard, written to ClickHouse's backup disk, streamed out and deleted from it.
# The passphrase reaches gpg on a file descriptor, never on a command line, and only the
# encrypted files are written. .env is not copied: keep a copy of it apart, since it holds
# the keys to the CA, the TOTP secrets and the forensic key, which the dump holds encrypted.
# docker compose reads COMPOSE_PROJECT_NAME and COMPOSE_FILE, when set, as it always does.
set -eu
umask 077

say() { printf 'backup.sh: %s\n' "$*"; }
fail() {
  say "$*"
  exit 1
}

# The last value .env gives a variable, without the quotes Compose strips from it.
value() {
  sed -n "s/^$1=//p" .env | sed -n '$p' | sed -e 's/^"\(.*\)"$/\1/' -e "s/^'\(.*\)'\$/\1/"
}

[ -f .env ] || fail "no .env: run me in the bundle's directory, after setup.sh"
[ -r .env ] || fail ".env is not readable: run me as the user setup.sh ran as"
command -v gpg >/dev/null 2>&1 || fail "gpg not found: it is a prerequisite of the host"
passphrase=$(value CG_BACKUP_PASSPHRASE)
[ -n "$passphrase" ] || fail "CG_BACKUP_PASSPHRASE is empty: run setup.sh"
dir=$(value CG_BACKUP_DIR)
[ -n "$dir" ] || dir=backups

mkdir -p "$dir"
chmod 0700 "$dir"
stamp=$(date -u +%Y%m%dT%H%M%SZ)
pg_file="$dir/cyberguard-postgres-$stamp.dump.gpg"
ch_file="$dir/cyberguard-clickhouse-$stamp.zip.gpg"
archive="cyberguard-$stamp.zip"
[ ! -e "$pg_file" ] && [ ! -e "$ch_file" ] || fail "a backup of $stamp exists already: run me again"
status=$(mktemp)

# On any exit: the archive off ClickHouse's backup disk, the status file, and, on a failure,
# the files this run wrote.
done_ok=""
cleanup() {
  docker compose exec -T clickhouse rm -f "/backups/$archive" >/dev/null 2>&1 || true
  rm -f "$status"
  if [ -z "$done_ok" ]; then rm -f "$pg_file" "$ch_file"; fi
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM

# Standard input, encrypted into $1. No passphrase cache in gpg-agent.
encrypt() {
  gpg --batch --yes --quiet --no-symkey-cache --pinentry-mode loopback --passphrase-fd 3 \
    --symmetric --cipher-algo AES256 --output "$1" 3<<EOF
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

say "the Postgres dump"
: > "$status"
first docker compose exec -T postgres pg_dump -U postgres -d cyberguard --format=custom | encrypt "$pg_file"
first_ok || fail "pg_dump failed"

say "the ClickHouse archive"
# clickhouse-client reads the administrator's name and password from the container's
# environment (CLICKHOUSE_USER, CLICKHOUSE_PASSWORD).
docker compose exec -T clickhouse clickhouse-client \
  -q "BACKUP DATABASE cyberguard TO Disk('backups', '$archive')" >/dev/null ||
  fail "BACKUP DATABASE failed"
: > "$status"
first docker compose exec -T clickhouse cat "/backups/$archive" | encrypt "$ch_file"
first_ok || fail "reading the archive failed"

# Both files are whole: a failure to delete the clear archive keeps them.
done_ok=1
docker compose exec -T clickhouse rm -f "/backups/$archive" ||
  fail "wrote $pg_file and $ch_file, but the archive could not be deleted from the backup disk: remove /backups/$archive in the clickhouse container"
say "wrote $pg_file and $ch_file"
