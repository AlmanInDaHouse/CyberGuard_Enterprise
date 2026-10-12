# CGES — CyberGuard Common Event Schema

OCSF-aligned event schema. Every event flowing through the platform conforms to a CGES version.

## Versioning

Semver, with the major version exposed in the directory name (`v0.1`, `v1`, `v2`, ...).

- **Minor** bumps add optional fields or relax constraints. Backward compatible.
- **Major** bumps remove or rename fields, change types, or tighten constraints. Require a migration plan.

While the version is 0.x and no consumer outside the repository reads CGES, a change keeps the version, additive or breaking; a breaking one lands with its ClickHouse migration, the ADR or SPEC that makes it states it, and the server is upgraded before the agents ([ADR-0006](../../docs/adr/0006-cges-ocsf-alignment.md), Amendment 2026-10-13).

The schemas require a top-level `schema_version` (`v0.1/event.json`). The wire between the agent and the ingest service carries none yet, and no consumer checks it (ADR-0006, Amendment 2026-10-13).

## Current versions

| Version | Status | Populated by |
|---|---|---|
| [`v0.1/`](v0.1/) | Draft | SPEC-XXX-cges-v0.1 |
