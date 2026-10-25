#!/bin/sh
# The key-value store's entrypoint (SPEC-023 §Operational §4). Writes the ACL file from the
# environment onto the container's tmpfs, readable by the image's valkey user, then runs the
# image's own entrypoint, which drops to that user. The file holds each password's SHA-256,
# not the password, and no password reaches a command line.
set -eu
: "${CG_REDIS_INGEST_PASSWORD:?}"
: "${CG_REDIS_API_PASSWORD:?}"

umask 077
acl=/run/cyberguard/users.acl

# printf is the shell's own: the password is never an argument of another process.
sha256() { printf '%s' "$1" | sha256sum | cut -d ' ' -f 1; }

{
  echo "user default off resetkeys resetchannels -@all"
  # Ingest: its nonces, and the commands of its client and of the connection's set-up alone.
  echo "user cg_ingest on #$(sha256 "$CG_REDIS_INGEST_PASSWORD") resetkeys ~nonce:* resetchannels -@all +auth +ping +info +client|setname +client|setinfo +quit +set"
  # The api: its sessions, its rate-limit counters and its TOTP replay keys; the commands of
  # its client and those its rate-limit scripts call.
  echo "user cg_api on #$(sha256 "$CG_REDIS_API_PASSWORD") resetkeys ~cgsess:* ~rl:* ~totp:replay:* resetchannels -@all +auth +ping +info +client|setname +client|setinfo +quit +get +set +del +scan +eval +incr +decr +exists +expire"
} > "$acl"
chown valkey:valkey "$acl"

exec docker-entrypoint.sh valkey-server --appendonly yes --aclfile "$acl"
