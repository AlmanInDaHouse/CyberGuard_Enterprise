# Relay CA

When the SMTP relay's certificate is issued by a private CA, put that CA's PEM file here and
set `CG_SMTP_CA_FILE` in `.env` to its name. The compose mounts this directory read-only into
ingest at `/run/cyberguard/relay`, and the send then trusts that file's certificates, and not
the system's roots, for the relay (SPEC-023 §Data contracts §3, §Operational §14).
