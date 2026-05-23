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
