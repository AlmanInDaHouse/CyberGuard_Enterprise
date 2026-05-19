# CGES — CyberGuard Common Event Schema

OCSF-aligned event schema. Every event flowing through the platform conforms to a CGES version.

## Versioning

Semver, with the major version exposed in the directory name (`v0.1`, `v1`, `v2`, ...).

- **Minor** bumps add optional fields or relax constraints. Backward compatible.
- **Major** bumps remove or rename fields, change types, or tighten constraints. Require a migration plan.

Producers tag events with `schema_version`. Consumers must reject events with unsupported `schema_version`.

## Current versions

| Version | Status | Populated by |
|---|---|---|
| [`v0.1/`](v0.1/) | Draft | SPEC-XXX-cges-v0.1 |
