# Specifications

Functional and technical specifications. Every module of CyberGuard is preceded by a SPEC document landed in this directory before any code is written.

## Naming

`SPEC-XXX-short-kebab-title.md`, where `XXX` is a zero-padded sequential id starting at `001`.

## Required sections per SPEC

1. **Context** — why the module exists and what problem it solves.
2. **Scope** — what is in and what is explicitly out.
3. **Data contracts** — schemas referenced, request/response shapes, event shapes.
4. **Acceptance criteria** — observable conditions for "done".
5. **Test scenarios** — harness scenarios mapped to this SPEC, with expected inputs and outputs.
6. **Risks** — known failure modes and mitigations.
7. **Open questions** — unresolved decisions, tracked until closure.
8. **References** — related SPECs, ADRs, external standards.

## Catalog

| SPEC | Title | Status |
|---|---|---|
| [SPEC-001](SPEC-001-agent-heartbeat.md) | Agent heartbeat | Accepted |
| [SPEC-002](SPEC-002-agent-enrollment.md) | Agent enrollment | Accepted |
| [SPEC-003](SPEC-003-mtls-signed-envelope.md) | mTLS 1.3 and signed envelope | Accepted |
| [SPEC-004](SPEC-004-server-ingest-minimal.md) | Server ingest minimal | Accepted |
| [SPEC-005](SPEC-005-agent-process-telemetry-windows-etw.md) | Agent process telemetry — Windows ETW Kernel-Process | Accepted |
| [SPEC-006](SPEC-006-detection-mvp.md) | Detection MVP — process-rule pipeline | Accepted |
| [SPEC-007](SPEC-007-incident-grouping-mvp.md) | Incident grouping MVP | Accepted |
| [SPEC-008](SPEC-008-auth-core.md) | Auth-core | Accepted |
| [SPEC-009](SPEC-009-read-slice.md) | Read-slice | Accepted |
| [SPEC-010](SPEC-010-forensic-event-drill.md) | Forensic event drill — incident → raw `cges_events` timeline | Accepted |
| [SPEC-011](SPEC-011-incident-severity.md) | Incident severity aggregation — MAX of member alerts | Accepted |

## Dependencies

Cross-document edges surfaced at landing (each SPEC's own "Depends on" header is authoritative; this records the load-bearing catalog edges).

- SPEC-010 → ADR-0015 (the read-only ClickHouse reader in `services/api` that SPEC-010 implements)
- SPEC-010 → SPEC-009 (amends §Out of scope `:34` **by scope**: the deferred alert→source-event drill is delivered here; SPEC-009's `IncidentDetail` / `ResolvedAlert` read-models are unchanged)
- SPEC-011 → SPEC-010 (realises §Out of scope `:32` **by scope**: the deferred *"Severity / score aggregation per incident"* is delivered here — an `incidents.severity_id` MAX over member alerts; no new ADR)
- SPEC-011 → SPEC-007 (extends the `incidents` upsert + re-words the triage-preservation invariant) / SPEC-009 (adds `severity_id` to the incident read-models)
