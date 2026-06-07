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
| [SPEC-012](SPEC-012-forensic-evidence-hashchain.md) | Forensic evidence hash-chain (escalón 3 — implements ADR-0016) | Accepted |
| [SPEC-013](SPEC-013-forensic-report-render.md) | Forensic report render (escalón 4 — per-incident PDF via `@react-pdf/renderer`) | Accepted |
| [SPEC-014](SPEC-014-incident-notification.md) | Incident email notification (criterion MVP 4 — generic SMTP, notify-on-create, fire-and-forget) | Accepted |

## Dependencies

Cross-document edges surfaced at landing (each SPEC's own "Depends on" header is authoritative; this records the load-bearing catalog edges).

- SPEC-010 → ADR-0015 (the read-only ClickHouse reader in `services/api` that SPEC-010 implements)
- SPEC-010 → SPEC-009 (amends §Out of scope `:34` **by scope**: the deferred alert→source-event drill is delivered here; SPEC-009's `IncidentDetail` / `ResolvedAlert` read-models are unchanged)
- SPEC-010 self-amendment 2026-06-06: drill order → total `(time, event_id)` (requirement of ADR-0016; response shape unchanged, only the same-`time` row order is newly pinned)
- SPEC-011 → SPEC-010 (realises §Out of scope `:32` **by scope**: the deferred *"Severity / score aggregation per incident"* is delivered here — an `incidents.severity_id` MAX over member alerts; no new ADR)
- SPEC-011 → SPEC-007 (extends the `incidents` upsert + re-words the triage-preservation invariant) / SPEC-009 (adds `severity_id` to the incident read-models)
- SPEC-012 → ADR-0016 (implements escalón 3 — the forensic evidence hash-chain) / SPEC-010 (the canonicalized drill output is the evidence unit). **Carries a deployment-contract Open question:** out-of-band trust anchoring of the forensic public key (cross-ref SPEC-012 §Open questions)
- SPEC-013 → SPEC-010 / SPEC-011 / SPEC-012 (escalón 4 — composes the drill timeline + incident severity + the hash-chain seal into a per-incident PDF) / ADR-0015 (the read-only ClickHouse reader the timeline uses) / ADR-0007 (TS-language precedent — the render is a module in `services/api`, **not** the blueprint's Go service). **Inherits SPEC-012's deployment-contract Open question** (out-of-band trust anchoring); landing SPEC-013 fires its reopen trigger (*"the render/export escalón lands"*) — mitigated with a visible integrity-not-authenticity note in the PDF, **not** resolved
- SPEC-014 → ADR-0017 (the two load-bearing decisions — generic-SMTP transport, fire-and-forget-after-commit as the detection pipeline's first external side-effect) / SPEC-007 (hangs off the `upsertIncident` create seam — notify on incident **create** only) / SPEC-006 (the detection MVP producing the alerts; mirrors `upsertAlert`'s create-vs-existing `rowCount` seam at the incident level). **Test-validated altitude:** `runDetectionCycle` has no production caller yet (the ADR-0012 firehose prod-driver is deferred), so SPEC-014 closes MVP criterion 4 as a testable capability hung at the correct seam, **not** an email running in production; supersedes the SPEC-007 `:37` / SPEC-008 `:42` notifier deferrals (incident-notification half)
