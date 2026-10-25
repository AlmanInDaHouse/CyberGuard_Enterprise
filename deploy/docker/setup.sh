#!/bin/sh
# CyberGuard's set-up script (SPEC-023 §Operational §3). Run it in the bundle's directory, as
# root. POSIX sh, using only cp, mv, od, tr, sed, grep, chmod and chown.
#
# Without .env: copies .env.example to .env (mode 0600) and stops, asking for the hostnames
# and the TLS mode. With .env: refuses, changing nothing, while a hostname is empty or while
# provided TLS (an empty CG_TLS_MODE is provided, as for the compose) lacks its files; then
# adds each variable .env.example has and .env lacks, with its default; fills each empty
# generated secret with 32 random bytes in hexadecimal; keeps every value already set; gives
# the TLS key to the proxy's user (uid 10001, mode 0400) in provided mode.
# It prints the names of what it filled, never a value. Once the secrets are filled, a further
# run changes nothing.
set -eu
umask 077

SECRETS="CG_PG_SUPERUSER_PASSWORD CG_PG_INGEST_OWNER_PASSWORD CG_PG_INGEST_PASSWORD
CG_PG_API_OWNER_PASSWORD CG_PG_API_PASSWORD CG_CH_ADMIN_PASSWORD CG_CH_INGEST_PASSWORD
CG_CH_API_PASSWORD CG_REDIS_INGEST_PASSWORD CG_REDIS_API_PASSWORD CG_INGEST_CA_PASSPHRASE
CG_API_DB_ENC_PASSPHRASE CG_API_FORENSIC_PASSPHRASE CG_BACKUP_PASSPHRASE"
PROXY_UID=10001

say() { printf 'setup.sh: %s\n' "$*"; }

if [ ! -f .env.example ]; then
  say "run me in the bundle's directory, beside .env.example"
  exit 1
fi

if [ ! -f .env ]; then
  cp .env.example .env
  chmod 0600 .env
  say "wrote .env from .env.example; set CG_DASHBOARD_HOSTNAME, CG_AGENT_SERVER_NAMES and CG_TLS_MODE in it, then run setup.sh again"
  exit 1
fi

# The last value .env gives a variable, without the quotes Compose strips from it.
value() {
  sed -n "s/^$1=//p" .env | sed -n '$p' | sed -e 's/^"\(.*\)"$/\1/' -e "s/^'\(.*\)'\$/\1/"
}

missing=""
[ -n "$(value CG_DASHBOARD_HOSTNAME)" ] || missing="$missing CG_DASHBOARD_HOSTNAME"
[ -n "$(value CG_AGENT_SERVER_NAMES)" ] || missing="$missing CG_AGENT_SERVER_NAMES"
mode=$(value CG_TLS_MODE)
# Empty, the compose's default: provided.
[ -n "$mode" ] || mode=provided
if [ "$mode" = provided ]; then
  [ -f tls/dashboard.crt ] || missing="$missing tls/dashboard.crt"
  [ -f tls/dashboard.key ] || missing="$missing tls/dashboard.key"
fi
if [ -n "$missing" ]; then
  say "missing:$missing"
  exit 1
fi

# A last line without its newline would have the first variable added joined to it.
if [ -s .env ] && [ "$(od -An -tx1 .env | tr -d ' \n' | sed 's/.*\(..\)$/\1/')" != 0a ]; then
  printf '\n' >> .env
fi

# Each variable of the template that .env lacks, added with its default.
while IFS= read -r line || [ -n "$line" ]; do
  case "$line" in
    '' | '#'*) continue ;;
  esac
  name=${line%%=*}
  if ! grep -q "^$name=" .env; then
    printf '%s\n' "$line" >> .env
    say "added $name"
  fi
done < .env.example

filled=""
for name in $SECRETS; do
  if [ -z "$(value "$name")" ]; then
    secret=$(od -An -tx1 -N32 /dev/urandom | tr -d ' \n')
    sed "s/^$name=.*/$name=$secret/" .env > .env.tmp
    mv .env.tmp .env
    filled="$filled $name"
  fi
done
chmod 0600 .env

if [ "$mode" = provided ]; then
  chown "$PROXY_UID:$PROXY_UID" tls/dashboard.key
  chmod 0400 tls/dashboard.key
fi

if [ -n "$filled" ]; then
  say "filled:$filled"
else
  say "nothing to fill"
fi
