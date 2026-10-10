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
| [SPEC-015](SPEC-015-detection-evaluator-generalization.md) | Detection evaluator — generalized Sigma subset (process_creation) | Accepted |
| [SPEC-016](SPEC-016-detection-rule-set-v1.md) | Detection rule set v1 — MVP criterion 1 (process_creation) | Accepted |
| [SPEC-017](SPEC-017-agent-capture-normal-run-path.md) | Agent capture on the normal run path — startup, delivery, path translation | Accepted |
| [SPEC-018](SPEC-018-detection-read-model-arrival-cursor.md) | Detection read-model — arrival cursor (late events) | Accepted |
| [SPEC-019](SPEC-019-agent-network-telemetry-windows-etw.md) | Agent network telemetry — Windows ETW Kernel-Network | Accepted |

## Dependencies

Cross-document edges surfaced at landing (each SPEC's own "Depends on" header is authoritative; this records the load-bearing catalog edges).

- SPEC-004 self-amendment 2026-09-21: listener bind address → optional `INGEST_BIND_HOST` (default `127.0.0.1`, the prior behavior); additive, FR-002 ports unchanged
- SPEC-008 self-amendment 2026-09-23: listener bind address → optional `API_BIND_HOST` (default `127.0.0.1`, the prior behavior); additive, `API_PORT` unchanged
- SPEC-010 → ADR-0015 (the read-only ClickHouse reader in `services/api` that SPEC-010 implements)
- SPEC-010 → SPEC-009 (amends §Out of scope `:34` **by scope**: the deferred alert→source-event drill is delivered here; SPEC-009's `IncidentDetail` / `ResolvedAlert` read-models are unchanged)
- SPEC-010 self-amendment 2026-06-06: drill order → total `(time, event_id)` (requirement of ADR-0016; response shape unchanged, only the same-`time` row order is newly pinned)
- SPEC-011 → SPEC-010 (realises §Out of scope `:32` **by scope**: the deferred *"Severity / score aggregation per incident"* is delivered here — an `incidents.severity_id` MAX over member alerts; no new ADR)
- SPEC-011 → SPEC-007 (extends the `incidents` upsert + re-words the triage-preservation invariant) / SPEC-009 (adds `severity_id` to the incident read-models)
- SPEC-012 → ADR-0016 (implements escalón 3 — the forensic evidence hash-chain) / SPEC-010 (the canonicalized drill output is the evidence unit). **Carries a deployment-contract Open question:** out-of-band trust anchoring of the forensic public key (cross-ref SPEC-012 §Open questions)
- SPEC-013 → SPEC-010 / SPEC-011 / SPEC-012 (escalón 4 — composes the drill timeline + incident severity + the hash-chain seal into a per-incident PDF) / ADR-0015 (the read-only ClickHouse reader the timeline uses) / ADR-0007 (TS-language precedent — the render is a module in `services/api`, **not** the blueprint's Go service). **Inherits SPEC-012's deployment-contract Open question** (out-of-band trust anchoring); landing SPEC-013 fires its reopen trigger (*"the render/export escalón lands"*) — mitigated with a visible integrity-not-authenticity note in the PDF, **not** resolved
- SPEC-014 → ADR-0017 (the two load-bearing decisions — generic-SMTP transport, fire-and-forget-after-commit as the detection pipeline's first external side-effect) / SPEC-007 (hangs off the `upsertIncident` create seam — notify on incident **create** only) / SPEC-006 (the detection MVP producing the alerts; mirrors `upsertAlert`'s create-vs-existing `rowCount` seam at the incident level). **Test-validated altitude — resolved:** the in-process TypeScript scheduler (ADR-0012 Amendment 2026-06-07) now gives `runDetectionCycle` a production caller (the Go firehose is decoupled from that role and still deferred), so SPEC-014 closes MVP criterion 4 as a testable capability hung at the correct seam that now also fires in a deployed system; supersedes the SPEC-007 `:37` / SPEC-008 `:42` notifier deferrals (incident-notification half)
- SPEC-015 → SPEC-006 (amends §In scope `:26` and the evaluator note `:134` **by scope**: the generalized Sigma subset is delivered here; SPEC-006's MVP rule and detect_ac_* ACs are unchanged)
- SPEC-016 → SPEC-015 (amends the rule-document contract `:47` **by scope**: the loader contract; resolves Open question 1 — multi-hop lineage stays out of the MVP) / SPEC-006 (realises §Out of scope `:42` **by scope**: the full detection bar, ten rules; closes Open questions 1–2; amends §Operational §2 and NFR-006-003 / NFR-006-004 **by scope**: the parent is resolved per child, with its own 24 h look-back) / SPEC-007 (meets the reopen condition of Open question 2: grouping kept, reopened at roadmap §E)
- SPEC-017 → SPEC-005 (realises capture on the normal run path; amends **by scope** the wire shape, the rejected-envelope failure mode, AC-007's `parent_process`, and where the cache and the timestamp are handled; defers NFR-005-003) / SPEC-003 (amends its Amendment 2026-05-23 part (a) **by scope**: events inside the signed `body`, no `batch_hash`) / SPEC-001 (implements its Amendment 2026-05-23; FR-009 amended **by scope** for the two startup lines) / ADR-0004 and ADR-0011 (amended in place, 2026-10-04). SPEC-017 self-amendment 2026-10-09: a Terminate carries the image base name, so the Win32 form is asserted on the Launch
- SPEC-018 → SPEC-006 (amends **by scope** the read-model line of §In scope, §Operational §1, NFR-006-002 and the wording of detect_ac_005: the forward read advances by an arrival cursor behind a settle margin, without `FINAL`) / SPEC-017 (delivers its §Out of scope item on events that arrive after the watermark; relies on in-order POSTs and byte-identical resends) / SPEC-016 (parent resolution relied on, unchanged) / ADR-0012 (amended in place, 2026-10-09)
- SPEC-019 → ADR-0018 (produces its per-class decisions for Network Activity 4001) / SPEC-005 (delivers its §Out of scope §1 for the network provider; ring, cache and timestamp conversion reused, and the cache gains a lookup that leaves the entry in place) / SPEC-017 (amends **by scope** the "Event element" of §Data contracts — an element is one of two shapes, told apart by `class_uid` — and the dispatch-callback description of §Operational §6; delivery relied on, unchanged) / SPEC-018 (the read-model relied on, unchanged: it selects class 1007; the destination of its §Out of scope item on other classes moves from roadmap §D to a later detection SPEC). SPEC-019 self-amendment 2026-10-10: net_ac_009 covers IPv6 (event 31) and net_ac_002 two more refusals; the server checks a 4001 `event_id` as a UUID and its `pid` as 32-bit; §2, §3, §4, §6 and NFR-019-001 restated to match the code (no lock or I/O of the callback's own beyond the cache, the ring and the pre-1970 `error` line; discards by event id whatever the `PID`; unlisted members ignored)
