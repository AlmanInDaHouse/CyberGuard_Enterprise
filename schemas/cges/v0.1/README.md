# CGES v0.1 — CyberGuard Common Event Schema

This directory holds the canonical JSON Schemas for **CGES v0.1**, the CyberGuard Common Event Schema. CGES is structurally aligned with [OCSF v1.3](https://schema.ocsf.io/1.3.0/) and extends it with CyberGuard-specific fields namespaced under `cg_*`. The full design rationale lives in [ADR-0006](../../../docs/adr/0006-cges-ocsf-alignment.md).

All schemas are **JSON Schema draft 2020-12**.

## Layout

```text
schemas/cges/v0.1/
├── README.md                            (this file)
├── event.json                           Root schema. oneOf across the six classes below.
├── common/
│   ├── cg_agent.json                    Agent identity (agent_id, version, platform, hostname)
│   ├── cg_org.json                      Organisation (org_id, tenant — multi-tenancy deferred)
│   ├── cg_score.json                    Combined detection score (heuristic / ueba / ml / final)
│   ├── cg_raw_ref.json                  MinIO reference for raw payloads > 64 KB
│   ├── cg_mitre.json                    MITRE ATT&CK mapping (tactics, techniques)
│   └── ocsf_severity.json               OCSF severity enum (0 Unknown → 6 Fatal)
├── objects/
│   ├── process.json                     OCSF Process object
│   ├── user.json                        OCSF User object
│   ├── device.json                      OCSF Device object
│   ├── file.json                        OCSF File object
│   ├── network_endpoint.json            OCSF Network Endpoint object
│   └── authentication_factor.json       OCSF Authentication Factor object
├── classes/
│   ├── 1001_filesystem_activity.json    OCSF class UID 1001 (category 1 System Activity)
│   ├── 1007_process_activity.json       OCSF class UID 1007 (category 1 System Activity)
│   ├── 3002_authentication.json         OCSF class UID 3002 (category 3 IAM)
│   ├── 4001_network_activity.json       OCSF class UID 4001 (category 4 Network Activity)
│   ├── alert.json                       CGES-specific (category_uid 10, class_uid 10001)
│   └── incident.json                    CGES-specific (category_uid 10, class_uid 10002)
└── examples/
    ├── 01_process_start.json            class 1007
    ├── 02_network_connection.json       class 4001
    ├── 03_auth_login_success.json       class 3002
    ├── 04_alert_rule_source.json        alert with detection_source = "rule"
    ├── 05_alert_ml_source.json          alert with detection_source = "ml" + cg_raw_ref
    └── 06_incident_grouped.json         incident grouping alerts 04 and 05
```

## Conventions

- Every schema declares `$schema: https://json-schema.org/draft/2020-12/schema` and an absolute `$id` under `https://cyberguard.io/schemas/cges/v0.1/`.
- `$ref` between files uses **relative paths** resolved against the document's `$id`. Examples: `"$ref": "common/cg_agent.json"` from `event.json`; `"$ref": "../objects/file.json"` from a class file.
- `additionalProperties: false` is used on closed CGES objects (`cg_*`). OCSF-inherited objects and classes use `additionalProperties: true` to allow OCSF extension.

> **Note (gotcha):** classes under `classes/` use `additionalProperties: true` **even for CGES-specific classes** (`alert`, `incident`). This is a deliberate workaround for the `oneOf` wrapping in [`event.json`](event.json) — strict `additionalProperties: false` on a class file breaks event-level field resolution, because the class sub-schema would reject the common top-level fields (`cg_agent`, `cg_org`, `time`, etc.) that belong to the wrapping event schema. Do **not** "fix" this to `false` unless you are also refactoring the `event.json` `oneOf` pattern. See Session 3 Vuelta 2 closure notes.

- Required fields are listed explicitly. No implicit optional/required.
- Timestamps use `format: date-time` (RFC 3339 / ISO 8601 UTC with milliseconds). Server-side ±5 min skew validation lives in `cg-ingest`, not in the schema (see ADR-0004 §Server validation order).
- `event_id` is UUIDv7 (RFC 9562). Pattern enforced at the schema level.
- `raw_data` (inline) and `cg_raw_ref` (MinIO offload) are mutually exclusive; the `oneOf` constraint in `event.json` enforces this.

## Adding a new schema artifact

| Adding a... | Where it goes | Required updates |
|---|---|---|
| New CGES extension object (`cg_*`) | `common/` | Reference it from `event.json` if it lives top-level; from a class file otherwise. Set `additionalProperties: false`. |
| New OCSF object inherited verbatim | `objects/` | Reference it from the class files that use it. Set `additionalProperties: true`. |
| New OCSF class | `classes/NNNN_name.json` | Add it to the `oneOf` array in `event.json`. Use `const` for `category_uid` and `class_uid` to make the discriminator unambiguous. |
| New CGES-specific class | `classes/name.json` | Same as above. Use a `category_uid` and `class_uid` in the CyberGuard custom range (≥ 10000) and document the assignment inline. |
| New example payload | `examples/NN_name.json` | The example must validate end-to-end against `event.json` with all `$refs` resolved. CI blocks merges otherwise. |

When adding new fields to an existing schema, never change the meaning of an existing field. Field-level changes that alter semantics require bumping `schema_version`.

## Validation

The canonical validator is [ajv-cli](https://github.com/ajv-validator/ajv-cli) with [ajv-formats](https://github.com/ajv-validator/ajv-formats) for `date-time` support.

### Run locally

```sh
task validate-schemas
```

The target invokes `ajv-cli` via `npx`, registers all `common/`, `objects/` and `classes/` files with `-r`, and validates every example under `examples/` against `event.json`.

### CI

[`.github/workflows/schema-validation.yml`](../../../.github/workflows/schema-validation.yml) runs the same validation on `push` to `main` and on every `pull_request` that touches `schemas/**`. Adding an example that does not validate is a blocking CI failure.

## Versioning

CGES follows semver via the top-level `schema_version` field. v0.1 is the MVP baseline. The OCSF version CGES tracks is recorded separately via `ocsf_version`. See ADR-0006 §Schema versioning for the full lifecycle.

## References

- [ADR-0006 — CGES alignment with OCSF v1.3](../../../docs/adr/0006-cges-ocsf-alignment.md)
- [ADR-0003 — Polyglot storage](../../../docs/adr/0003-polyglot-storage.md) (ClickHouse partitioning, MinIO immutability)
- [ADR-0004 — Agent-Server secure protocol](../../../docs/adr/0004-agent-server-protocol.md) (CGES is the body inside the signed envelope)
- [ADR-0005 — Detection rules and ML in parallel](../../../docs/adr/0005-detection-rules-and-ml-in-parallel.md) (`cg_detection_source`, `cg_trust_sources`, `cg_score`)
- [OCSF v1.3 specification](https://schema.ocsf.io/1.3.0/)
- [JSON Schema draft 2020-12](https://json-schema.org/draft/2020-12)
