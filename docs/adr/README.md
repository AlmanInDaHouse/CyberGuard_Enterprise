# Architecture Decision Records

This directory holds Architecture Decision Records following the [MADR](https://adr.github.io/madr/) format.

## Naming

`NNNN-short-kebab-title.md`, where `NNNN` is a zero-padded sequential id starting at `0001`.

## Status values

- `Proposed` — under discussion.
- `Accepted` — current binding decision.
- `Deprecated` — superseded by a newer ADR; kept for historical context.
- `Superseded by ADR-NNNN` — explicit replacement pointer.

## Catalog

| ADR | Title | Status |
|---|---|---|
| [0001](0001-monorepo-layout.md) | Monorepo layout for CyberGuard | Accepted |
| [0002](0002-language-per-component.md) | Language per component | Accepted |
| [0003](0003-polyglot-storage.md) | Polyglot storage | Accepted |
| [0004](0004-agent-server-protocol.md) | Agent-Server secure protocol | Accepted |
| [0005](0005-detection-rules-and-ml-in-parallel.md) | Detection — rules and ML in parallel | Accepted |
| [0006](0006-cges-ocsf-alignment.md) | CGES alignment with OCSF v1.3 | Accepted |
| [0007](0007-ingest-language-typescript-mvp.md) | Ingest service language — TypeScript for the MVP control plane | Accepted |
| [0008](0008-etw-crate-selection.md) | ETW crate selection for Windows event capture | Accepted |
| [0009](0009-event-delivery-and-buffer.md) | Event delivery semantics and agent buffer model | Accepted |
| [0010](0010-agent-privilege-model-mvp.md) | Agent privilege model and installation posture for the MVP | Accepted |
| [0011](0011-cges-process-activity-v0-1.md) | Per-class CGES jurisprudence — Process Activity v0.1 | Accepted |
| [0012](0012-normalize-before-correlate-pipeline.md) | Normalize-before-correlate pipeline (Detection MVP) | Accepted |
| [0013](0013-incident-correlation-windowing.md) | Incident correlation windowing — event-time basis | Accepted |
| [0014](0014-human-authentication-model.md) | Human authentication model — local self-hosted, password + TOTP | Accepted |
| [0015](0015-readonly-clickhouse-reader-in-api.md) | Read-only ClickHouse reader in `services/api` (forensic event-drill boundary) | Accepted |
| [0016](0016-forensic-evidence-hash-chain.md) | Forensic evidence hash-chain — per-event SHA-256 chain + dedicated Ed25519 root signature | Accepted |

## Dependencies

- ADR-0002 → ADR-0001 (defines the top-level components this ADR assigns languages to)
- ADR-0003 → ADR-0001 (defines the components whose data this ADR routes)
- ADR-0003 → ADR-0002 (language choices cite NATS and ClickHouse Go client maturity)
- ADR-0004 → ADR-0001 (locates `agent/` and `services/ingest/`)
- ADR-0004 → ADR-0002 (Rust for agent is an input to the protocol; server language is language-agnostic per ADR-0007)
- ADR-0004 → ADR-0003 (Redis hosts nonce cache and revocation list)
- ADR-0005 → none (detection principle is independent of stack and storage)
- ADR-0006 → ADR-0001 (places `schemas/cges/` per the monorepo layout)
- ADR-0006 → ADR-0003 (ClickHouse partitioning on `event_id` and `occurred_at`; MinIO hosts the offloaded raw payload via `cg_raw_ref`)
- ADR-0006 → ADR-0004 (CGES is the body inside the signed envelope defined by the agent-server protocol)
- ADR-0006 → ADR-0005 (`cg_detection_source`, `cg_trust_sources`, and `cg_score` are first-class CGES fields because of the detection decision)
- ADR-0007 → ADR-0002 (amends the `services/ingest/` language row to TypeScript for the MVP)
- ADR-0007 → ADR-0004 (the agent-server protocol is language-agnostic; ingest implements it in TypeScript)
- ADR-0008 → ADR-0001 (the agent at `agent/cg-agent/` is where the ETW consumer lives)
- ADR-0008 → ADR-0002 (Rust for the agent; this ADR picks a Rust crate)
- ADR-0008 — additive new decision; supersedes nothing, amends nothing
- ADR-0009 → ADR-0003 (ClickHouse engine + partitioning unchanged from this ADR)
- ADR-0009 → ADR-0004 (amends in part via the in-place 2026-05-23 amendment; does not supersede)
- ADR-0009 → ADR-0006 (`event_id` is UUIDv7 per CGES; the dedup contract relies on this)
- ADR-0009 → ADR-0008 (the ring's enqueue side is the ferrisetw callback path)
- ADR-0010 → ADR-0001 (the privilege model attaches to the agent's process at `agent/cg-agent/`)
- ADR-0010 → ADR-0002 (Rust for the agent; this ADR specifies how that Rust process runs)
- ADR-0010 → ADR-0008 (ETW driver of the elevation requirement; ADR-0008 forward-references here)
- ADR-0010 — additive new decision; supersedes nothing, amends nothing
- ADR-0011 → ADR-0006 (per-class jurisprudence pattern under ADR-0006's framework deferral; first per-class ADR)
- ADR-0011 → ADR-0008 (ETW Kernel-Process provider whose crate ADR-0008 selects)
- ADR-0011 → ADR-0009 (process.uid recipe's "no agent-side mapping" requirement aligns with ADR-0009 §Decision part 3's "no on-disk event state")
- ADR-0011 → ADR-0010 (elevated process precondition enables ETW Kernel-Process consumption)
- ADR-0011 — additive new decision; supersedes nothing, amends nothing
- ADR-0012 → ADR-0002 (amends in part: `services/pipeline/` Go assignment superseded for the MVP detection slice only; restored on the firehose ADR)
- ADR-0012 → ADR-0003 (amends in part: §Retention Alerts row → Postgres-only for the MVP)
- ADR-0012 → ADR-0005 (the anticipated normalize-before-correlate ADR; clarifies scoring composition for the absent-source case)
- ADR-0012 → ADR-0006 (emits the CGES Alert / score / mitre classes under its framework)
- ADR-0012 → ADR-0007 (extends the transitory-TypeScript-for-MVP logic to the detection slice; same firehose-deferred exit)
- ADR-0012 → ADR-0009 (FINAL / GROUP BY reads because of ReplacingMergeTree at-least-once delivery)
- ADR-0012 → ADR-0011 (reads Process Activity 1007; flags the §4 ETW-mapping contradiction)
- ADR-0013 → ADR-0012 (amends in part: ADR-0013 §2 amends ADR-0012 §8's single-tunable framing in place — incident/stateful correlation uses its own window, distinct from and wider than the 300 s dedup bucket; does not supersede)
- ADR-0014 → ADR-0001 (locates the human-facing `services/api` component in the monorepo)
- ADR-0014 → ADR-0002 (`services/api` = TypeScript + Fastify + Zod, per the language table row)
- ADR-0014 → ADR-0003 (consumes the storage homes: users / RBAC / audit_log → Postgres, sessions → Redis; does not amend)
- ADR-0015 → ADR-0003 (consumes the ClickHouse storage home as a read-only reader in `services/api`; does not amend or re-route storage)
- ADR-0015 → ADR-0014 (preserves the human/agent trust-boundary split — keeps the forensic event read in the user-facing api, not the agent boundary)
- ADR-0015 → SPEC-009 (amends-by-scope `:34`: the alert→source-event drill SPEC-009 deferred-with-destination is delivered by this ADR + SPEC-010)
- ADR-0016 → ADR-0014 (introduces a dedicated forensic signing key in the human-facing `services/api`; does NOT reuse the ingest CA, preserving the human/agent trust-boundary split)
- ADR-0016 → ADR-0003 (consumes the evidence definition; explicitly does NOT use the MinIO at-rest home — recorte part (b))
- ADR-0016 → SPEC-010 (the canonicalized drill output `EventTimeline` is the evidence unit; requires its `(time, event_id)` total-order amendment)
- ADR-0016 → SPEC-011 / SPEC-007 (the incident the evidence is scoped to: its grouped alerts and aggregated severity)
- ADR-0016 → SPEC-003 (reuses the JCS canonicalization discipline; does not amend it)
