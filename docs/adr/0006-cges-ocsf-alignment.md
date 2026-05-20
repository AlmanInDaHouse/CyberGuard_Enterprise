# ADR-0006: CGES alignment with OCSF v1.3

- Status: Accepted
- Date: 2026-05-20
- Deciders: Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

ADR-0001 reserved `schemas/cges/v0.1/` as the home of CyberGuard's Common Event Schema. ADR-0004 specified that the wire format of events — the body inside the signed envelope sent from agent to server — is out of its scope and belongs to a dedicated ADR. ADR-0005 introduced detection-source attribution, trust-source playbook gating, and a scoring composition that depends on per-alert structured fields. The schema is the contract between agents, the server pipeline, the ML service, and the forensic exporter; getting it wrong now causes painful migrations later.

The fundamental decision is whether to design CGES from scratch or to align it with an existing standard. The Open Cybersecurity Schema Framework (OCSF) emerged in 2022 as a vendor-neutral effort with adoption by AWS, Splunk, IBM, CrowdStrike, Cloudflare and others. Aligning with OCSF means inheriting a taxonomy that is being absorbed by the industry; deviating means rebuilding work that the OCSF community has already done.

The decision recorded here must:

1. Choose between an OCSF-aligned schema, an alternative standard (ECS, CIM), or a bespoke schema, and justify the choice.
2. Define the inheritance boundary explicitly: what CGES takes from OCSF and what CGES extends or overrides.
3. Specify the schema versioning policy and how it interacts with OCSF's own version evolution.
4. Specify validation: who validates, when, and against which artifact.

## Decision

CGES v0.1 is **structurally aligned with OCSF v1.3**. It inherits OCSF's category taxonomy, class identifiers, and core object models, and extends them with CyberGuard-specific fields namespaced under `cg_*`. CGES is not OCSF-compliant in the certification sense; it is OCSF-aligned in the engineering sense.

### Inheritance from OCSF v1.3

CGES adopts the following from OCSF v1.3 without modification:

- **Category UIDs and class UIDs.** Examples: 1001 File System Activity, 3002 Authentication, 4001 Network Activity. The full mapping lives in the JSON Schema files under `schemas/cges/v0.1/`.
- **Core object schemas:** Process, User, Device, Network Endpoint, File, Email, Authentication Factor.
- **Severity scale:** 0 (Informational) through 6 (Fatal), as defined by OCSF.
- **`activity_id` semantics** within each class.
- **MITRE ATT&CK mapping field structure** (tactics and techniques as arrays of identifiers).

### Extensions specific to CGES (the `cg_*` namespace)

CGES adds the following CyberGuard-specific objects and fields. All such fields are namespaced under `cg_*` to make the boundary with OCSF explicit and to allow per-field compliance claims if marketing requires them.

- **`event_id`** — UUIDv7 (RFC 9562), time-ordered and collision-resistant. OCSF leaves event-id format to the implementation; CGES locks it for ClickHouse partition compatibility per ADR-0003 Rule 2.
- **`cg_agent`** — object containing `agent_id`, `agent_version`, `agent_platform`, `agent_hostname`. Replaces OCSF's generic `metadata.product` path with a typed, CyberGuard-specific structure.
- **`cg_org`** — object containing `org_id`, `tenant`. The `tenant` field is reserved for future multi-tenancy and is fixed to `"default"` in MVP (Blueprint §17.1).
- **`cg_score`** — object containing `heuristic_score`, `ueba_score`, `ml_score`, `final_score`, per ADR-0005 §Scoring composition.
- **`cg_detection_source`** — enum `"rule" | "ml" | "hybrid"`, per ADR-0005 §Detection source.
- **`cg_trust_sources`** — array of detection-source labels declaring which sources a downstream playbook may trust for this event, per ADR-0005 §Playbook governance.
- **`cg_raw_ref`** — object referencing an off-loaded raw payload in MinIO (see §Deliberate divergence below).

### Deliberate divergence from OCSF

CGES intentionally departs from OCSF on three points. Each divergence is recorded here so that future OCSF-alignment reviews do not mistake them for drift.

- **Dual timestamps are mandatory.** CGES requires both `time` (OCSF-compatible `occurred_at`) and `cg_ingested_at`. OCSF only requires `time`. The dual timestamp is non-negotiable for forensic reconstruction; clock skew between agent and server (±5 min per ADR-0004) makes a single timestamp insufficient.
- **`raw_data` has a hard maximum of 64 KB.** Payloads larger than 64 KB are offloaded to MinIO and referenced by `cg_raw_ref`, an object containing `{hash, bucket, object_key, size_bytes, content_type}` where `hash` is `sha256` of the offloaded payload. OCSF allows unbounded `raw_data`, which is impractical at ClickHouse scale (ADR-0003 Rule 2).
- **`event_id` format is locked to UUIDv7.** OCSF allows any string. CGES requires UUIDv7 explicitly for time-ordering at the ID layer, which is a necessary property given the ClickHouse `ORDER BY` clause defined in ADR-0003 Rule 2.

### Schema versioning

- A `schema_version` field at the top level carries the CGES version (semver). v0.1 is the MVP baseline.
- Breaking changes bump the major version and require an offline migration in ClickHouse (documented in the retention runbook when written).
- The OCSF version that CGES tracks is recorded separately via an `ocsf_version` field. CGES v0.1 tracks OCSF v1.3. When OCSF releases v2.x, a new ADR evaluates the alignment delta and decides the upgrade path.

### Validation

- The canonical schema lives in `schemas/cges/v0.1/*.json` as JSON Schema draft 2020-12.
- All **producers** (agent, `cg-ingest` for re-emit) validate the event before emit; all **consumers** (`cg-pipeline`, `cg-ml`, `cg-forensic`) validate on receive. Belt and suspenders: a producer regression should fail on emit, not corrupt downstream state.
- A CI workflow validates every example file in `schemas/cges/v0.1/examples/` with `ajv`. The workflow itself is reserved for a future iteration in line with the existing `.github/workflows/` cadence.

### Out of scope

- The full per-class field list. It lives in the JSON Schema files under `schemas/cges/v0.1/`, not in this narrative ADR.
- Migration tooling between schema versions. A separate SPEC will cover this when CGES v0.2 is on the horizon.
- Field-level encryption for PII. Deferred per Blueprint §17.11 (Advanced anonymisation out of MVP).

## Alternatives considered

### A1 — Design CGES from scratch, ignore OCSF

Pros: maximum design freedom, no compatibility constraints, no need to track an external standard's evolution.

Cons: loses interoperability with future SIEM and XDR vendors that adopt OCSF (every major one is on the path); rebuilds the taxonomy work the OCSF community has already done; harder to onboard analysts familiar with OCSF-aligned tools.

Rejected. The compatibility argument is decisive for a product that wants to be taken seriously in enterprise procurement.

### A2 — Adopt OCSF v1.3 verbatim, no extensions

Pros: full OCSF compliance, the cleanest possible interoperability story, no `cg_*` namespace to maintain.

Cons: OCSF lacks fields critical to CyberGuard's product principles. There is no concept of `detection_source` (ADR-0005), no `trust_sources` for playbook gating (ADR-0005), no raw-payload offload semantics (ADR-0003 storage scale), and no dual-timestamp requirement (forensic correctness). Forcing these into OCSF's `metadata.unmapped` defeats the purpose of having a schema in the first place.

Rejected. Alignment yes; verbatim no.

### A3 — Adopt ECS (Elastic Common Schema) instead of OCSF

Pros: ECS is mature (Elastic shipped it in 2019); large field catalogue; widely used in SIEM deployments built on Elastic.

Cons: ECS is governed by Elastic and has historically been tied to their licensing decisions; OCSF is vendor-neutral. OCSF is winning vendor adoption for cross-vendor interoperability; ECS is winning *within* the Elastic ecosystem. CyberGuard does not depend on Elastic and does not want to inherit a governance dependency.

Rejected. Strategic alignment with the open vendor-neutral standard.

### A4 — Adopt CIM (Splunk's Common Information Model)

Pros: well-understood in Splunk shops.

Cons: tightly coupled to Splunk; not a standard outside that ecosystem.

Rejected for the same reasons as A3.

## Consequences

### Positive

- Future integrations with OCSF-emitting tools (cloud providers, cloud-native security tools) require translation, not redesign.
- Analysts familiar with OCSF can read CGES events with minimal ramp-up.
- The taxonomy work — categories, classes, activities — is inherited for free.
- Extensions are clearly namespaced under `cg_*`; per-field OCSF compliance can be asserted if marketing or procurement requires it.

### Negative

- OCSF v1.3 will evolve. The project pays the cost of alignment reviews periodically — estimated annually — to evaluate delta and decide on adoption.
- Some OCSF object paths are deeply nested. The ClickHouse storage schema flattens them for query performance, which means the wire format and the storage format diverge slightly. The divergence is documented in the CGES v0.1 schema files.
- The 64 KB `raw_data` cap plus MinIO offload is an extra mechanism that the agent and the forensic exporter must implement correctly. Bug risk is real and is mitigated by harness scenarios that exercise the offload path explicitly.

### Neutral

- CGES v0.2+ may add field-level encryption for PII fields. Reserved for a future ADR; out of MVP scope per Blueprint §17.11.
- The dual timestamp may complicate downstream tools that expect a single `time` field. Mitigated by aliasing in the schema (`time` is an alias of `occurred_at`, present for OCSF compatibility).

## Compliance

Subsequent ADRs, SPECs and schema files must respect the inheritance and divergence boundaries declared here. Specifically:

- New CGES fields that map onto an OCSF concept must use OCSF naming and structure, not `cg_*`.
- New CGES fields with no OCSF equivalent must be namespaced under `cg_*` and documented in the schema file that introduces them.
- Any breaking change to CGES bumps the major version, opens a migration SPEC for ClickHouse, and updates `ocsf_version` if the trigger is OCSF evolution.
- The contract-generation tooling reserved for ADR-0008 will consume the JSON Schemas under `schemas/cges/` as its source of truth; manual translation between languages is forbidden.

## References

- [ADR-0001](0001-monorepo-layout.md) — Monorepo layout (places `schemas/cges/`)
- [ADR-0003](0003-polyglot-storage.md) — Polyglot storage (ClickHouse partitioning on `(org_id, occurred_at, event_id)`; MinIO hosts offloaded raw payloads)
- [ADR-0004](0004-agent-server-protocol.md) — Agent-Server protocol (events are the body inside the signed envelope; this ADR defines that body)
- [ADR-0005](0005-detection-rules-and-ml-in-parallel.md) — Detection (`cg_detection_source`, `cg_trust_sources`, `cg_score` are CGES first-class fields because of this decision)
- [OCSF v1.3 specification](https://schema.ocsf.io/1.3.0/)
- MITRE ATT&CK v15 (mapping target)
- [Foundational Blueprint](../product/blueprint.md) — §8 (Common Event Schema sketch), §17.1 (multi-tenancy out), §17.11 (advanced anonymisation out)
