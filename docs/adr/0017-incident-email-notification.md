# ADR-0017: Incident email notification — generic SMTP transport, fire-and-forget on incident create, the detection pipeline's first external side-effect

- Status: Accepted
- Date: 2026-06-07
- Last updated: 2026-06-07
- Deciders: Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

Blueprint §18 MVP acceptance criterion 4 requires *"Gmail/SMTP notification"* (`../product/blueprint.md:749`), and the MVP narrative says the SOC *"receives an email with the summary"* of a grouped incident (`:740`). Two Accepted SPECs deferred this with the same language — a *"future notifier slice … needs SMTP/Gmail credentials (ask-first)"* (SPEC-007 §Out of scope, `../specs/SPEC-007-incident-grouping-mvp.md:37`; SPEC-008 §Out of scope, `../specs/SPEC-008-auth-core.md:42`), both pointing at Blueprint §16's *"notifier abstraction with SMTP fallback."* SPEC-014 implements that slice for the create case; this ADR records the two load-bearing decisions it rests on.

The notification hangs off the incident write the detection pipeline already performs: `upsertIncident` (`services/ingest/src/detect/incidents.ts:66-101`), called from `runDetectionCycle` (`services/ingest/src/detect/index.ts:24`) right after a genuinely-new alert is persisted (`index.ts:38-55`, guarded by `alertId !== null`). That cycle has **no production caller** today — it runs only under the detection harness (`detect_ac_*`, `incident_ac_*`); its production driver is the Go `services/pipeline/` extraction deferred to the firehose ADR (ADR-0012 §1 / §Out of scope, `0012-normalize-before-correlate-pipeline.md:28,236`). This ADR is honest about that: it fixes *where* and *how* notification attaches, at the altitude the rest of the detection MVP is validated (test-driven), not as an email running in production today.

Two decisions carry weight and are recorded here rather than settled silently in code: the **transport** (a generic-SMTP client vs the Gmail REST API) and the introduction of the **first external side-effect** into a pipeline that has, until now, only ever written to its own databases.

## Decision

### 1. Transport is a generic SMTP client, not the Gmail REST API

Notification is delivered over **generic SMTP** (host / port / credentials / STARTTLS), using a maintained JavaScript SMTP client. Gmail is reached as **one SMTP host among many** (`smtp.gmail.com` with an app password), not through the Gmail REST API.

**Why:** the MVP criterion says *"Gmail/SMTP"* (`../product/blueprint.md:749`), and generic SMTP is the portable subset. The Gmail REST API would couple CyberGuard to Google OAuth (client registration, token refresh, consent) — a dependency that contradicts the self-deployable, on-prem promise of the product (the operator runs against their own mail relay). SMTP works against Gmail, Microsoft 365, a corporate relay, or a local fake-SMTP in tests with no code change. The concrete library (e.g. `nodemailer` or `emailjs`) is an implementation choice settled at the code gate; this ADR fixes only "generic SMTP, not Gmail API / OAuth."

### 2. Notification is the detection pipeline's first external side-effect — fire-and-forget, after the upsert commits, outside its transaction

`upsertIncident` has, until now, been a pure database write, and `runDetectionCycle` has only ever touched ClickHouse and Postgres. Notification is the **first outbound, third-party side-effect** in that loop, and it is bounded as follows:

- **After commit.** The notify is dispatched only **after `upsertIncident` resolves** — i.e. after the incident row is durably written. The send is **outside** the upsert's statement / transaction; it never widens or blocks the write.
- **Fire-and-forget.** A send failure (SMTP unreachable, auth rejected, timeout) is **caught and logged** (observable), and **does not propagate** an error back into `runDetectionCycle`. The detection cycle's success is independent of mail delivery.
- **No synchronous retry.** A single best-effort attempt. Retry / queue / dead-letter is a future increment (§Out of scope), not MVP.

**Why:** the system's source of truth is the **persisted alert and incident**, not the email. Event delivery upstream is deliberately **at-least-once with server-side dedup**, precisely because *"a security agent that silently drops events … undermines the analyst's view"* (ADR-0009 §1, `0009-event-delivery-and-buffer.md:33`); the durable record is therefore authoritative. Notification is a **downstream best-effort projection off that already-durable record** — a missed email is recoverable (the incident is in the dashboard and the read API), unlike a dropped event, which is not. Coupling the reliability of the detection pipeline to a third-party SMTP server would invert that: a flaky mail relay must never be able to fail detection. Best-effort here is correct *because* the upstream is not.

## Alternatives considered

### A1 — Gmail REST API transport

Pros: richer delivery signals; no SMTP relay to operate.

Cons: ties the product to Google OAuth (client registration, token lifecycle, consent screens), breaking the self-deployable promise and excluding non-Gmail operators. Rejected — SMTP reaches Gmail too, without the coupling.

### A2 — Transactional / synchronous notification (block the cycle until sent)

Pros: a stronger delivery guarantee at send time; an email failure is surfaced immediately.

Cons: couples detection-pipeline liveness to a third-party SMTP server; a slow or down relay stalls or fails incident persistence — exactly backwards, since the persisted incident is the truth and the email is the courtesy copy. Rejected — best-effort after commit.

### A3 — Durable queue + retry (outbox / dead-letter) for delivery guarantees

Pros: at-least-once notification; survives transient relay outages.

Cons: introduces a queue, a worker, and delivery-state schema — scope well beyond an MVP notify-only slice, and premature before a production detection driver even exists. Rejected for the MVP; named as the future increment if delivery guarantees are later required.

## Consequences

### Positive

- **MVP criterion 4 is closeable** as a testable capability hung at the correct seam (`upsertIncident`), with no new infrastructure (no queue, no broker, no new service).
- **Pipeline reliability is insulated** from third-party mail: detection cannot be failed by SMTP.
- **Portable** across mail providers (Gmail, M365, corporate relay, fake-SMTP in CI) by configuration alone.

### Negative

- **Test-validated altitude only.** Because `runDetectionCycle` has no production caller yet (ADR-0012 §1, `0012-normalize-before-correlate-pipeline.md:28`), notification does not fire in a deployed system today. It is wired at the correct point and exercised end-to-end through the detection harness; when the Go `services/pipeline/` prod-driver lands (the firehose ADR, `:236`), notification activates **without re-touching the seam**. This inherited gap is pre-existing and shared by the whole detection MVP — this ADR neither widens nor resolves it.
- **Best-effort delivery.** A missed email is silent to the recipient (logged server-side only). Acceptable because the incident is durably recorded and visible in the dashboard / read API; delivery guarantees are a named future increment.

### Neutral

- The notify dispatch lives in `services/ingest` (where detection and the upsert live), the same transitory TypeScript seam ADR-0012 §1 flags as *"debt by construction; ported to Go on the firehose ADR"* (`:213`). When that extraction happens, the notify hook moves with the cycle.
- Operator-set SMTP endpoint, credentials, and recipient are a deployment-contract surface (see §Out of scope and §Compliance); SPEC-014 fixes the env-var **names**, the operator supplies the **values**.

## Compliance

- The notify dispatch **MUST** occur only **after** `upsertIncident` has durably written the incident row, and **MUST** be outside that write's transaction — it MUST NOT widen, delay, or be able to roll back the upsert.
- A notification send failure **MUST** be caught and logged and **MUST NOT** propagate an error into `runDetectionCycle` or alter `DetectCycleResult` — the detection cycle's outcome is independent of mail delivery.
- Notification **MUST** fire only on incident **creation** (a fresh incident row), never on a correlated-update / severity-raise of an existing incident — the create-vs-update signal mirrors `upsertAlert`'s `rowCount === 1` seam (`services/ingest/src/detect/alerts.ts:74`).
- The SMTP endpoint, credentials, sender, and recipient **MUST** come from operator-set `services/ingest` configuration (mirroring `INGEST_CA_PASSPHRASE`, `services/ingest/src/config.ts:18`), and **MUST NOT** be derived from `users.email` (a login identifier owned by `services/api`, an ownership boundary the trigger service MUST NOT cross).
- Transport **MUST** be generic SMTP; it **MUST NOT** depend on the Gmail REST API or Google OAuth.

## Out of scope

Each deferred item names its destination:

- **Escalation notifications** (notify on an existing incident's severity raise) — only incident creation notifies in the MVP. A later notification increment, once an escalation transition is first-classed (today severity rises via `GREATEST` with no emitted transition, `services/ingest/src/detect/incidents.ts:84`).
- **Delivery guarantees** — retry, durable queue / outbox, dead-letter. The A3 future increment; the MVP is a single best-effort attempt.
- **Per-org / templated routing** — recipient is a single operator-set mailbox; per-org templates and routing rules are a later increment.
- **The SOAR playbook** (Blueprint §18 criterion 6, *"1 SOAR playbook operational"*) — a separate MVP item and a separate branch; this ADR is notify-only and does not build the playbook engine (`services/soar/` stays a placeholder).
- **The production detection driver** — the Go `services/pipeline/` extraction / event firehose that gives `runDetectionCycle` a prod caller is deferred to the firehose ADR (ADR-0012 §1 / §Out of scope, `0012-normalize-before-correlate-pipeline.md:28,236`). Inherited gap; notification rides whatever drives the cycle.
- **Recipient derived from `users.email`** — rejected on the ownership boundary (the `users` table is owned by `services/api` per SPEC-008; `services/ingest` must not read it). Recipient is config.
- **Concrete SMTP credential / endpoint / recipient values** — a deployment-contract the operator sets at deploy time (owner-STOP, like SPEC-012's `forensic_pubkey` trust anchoring); SPEC-014 declares the var names and the `.env.example` placeholders at the code gate, never a baked-in default.

## Landing checklist (atomic on flip to Accepted)

1. Flip this ADR's `- Status:` from `Proposed` to `Accepted` and bump `- Last updated:`.
2. Flip SPEC-014's `- **Status:**` to `Accepted`; add the ADR-0017 catalog row to `docs/adr/README.md` and the SPEC-014 row to `docs/specs/README.md`.
3. Add the dependency edges: ADR-0017 → ADR-0009 (best-effort notify projects off at-least-once), ADR-0017 → ADR-0012 (the deferred prod-driver the notify rides), ADR-0017 → SPEC-007 (the `upsertIncident` seam), SPEC-014 → ADR-0017.

## References

- [Blueprint](../product/blueprint.md) — `:749` (MVP criterion 4 *"Gmail/SMTP notification"*; verbatim duplicate at `:664` in the Phase 2 roadmap), `:740` (the *"receives an email with the summary"* narrative), §16 (the *"notifier abstraction with SMTP fallback"* both deferrals cite).
- [SPEC-014](../specs/SPEC-014-incident-notification.md) — the slice this ADR backs: the ingest notify module hung off `upsertIncident` on create.
- [SPEC-007](../specs/SPEC-007-incident-grouping-mvp.md) / [SPEC-008](../specs/SPEC-008-auth-core.md) — the two `§Out of scope` notifier deferrals (`:37` / `:42`); SPEC-014 discharges the incident-notification half (SPEC-008's password-reset / invite email scoping is untouched).
- [ADR-0009](0009-event-delivery-and-buffer.md) — at-least-once event delivery + server-side dedup (`:29,33`); the durable record that best-effort notification projects off.
- [ADR-0012](0012-normalize-before-correlate-pipeline.md) — the transitory TypeScript detect seam (`:28`, `:213`) and the deferred Go `services/pipeline/` prod-driver / firehose (`:236`) that will drive the cycle the notify rides.
- `services/ingest/src/detect/incidents.ts:66-101` (the `upsertIncident` seam; title `:72`, `GREATEST` severity `:84`), `services/ingest/src/detect/index.ts:38-55` (the new-alert block, the single production insertion point), `services/ingest/src/detect/alerts.ts:74` (the `rowCount` create-vs-existing precedent), `services/ingest/src/config.ts:18` (`INGEST_CA_PASSPHRASE`, the required-secret config pattern).
