# SPEC-014: Incident email notification — generic SMTP, fire-and-forget on incident create, hung off the ingest detection upsert

- **ID:** SPEC-014
- **Title:** Incident email notification (criterion MVP 4 — notify-only email on incident creation, generic SMTP fire-and-forget, a module in `services/ingest`)
- **Status:** Accepted
- **Depends on:** ADR-0017 (the two load-bearing decisions — generic-SMTP transport, and fire-and-forget-after-commit as the detection pipeline's first external side-effect); SPEC-007 (the incident grouping `upsertIncident` this hangs off, `services/ingest/src/detect/incidents.ts:66-101`, and its caller `runDetectionCycle`, `services/ingest/src/detect/index.ts:24-55`); SPEC-006 (the detection MVP producing the alerts the incident groups, and the `upsertAlert` create-vs-existing seam mirrored here, `services/ingest/src/detect/alerts.ts:74`). Also SPEC-011 (the incident `severity_id` carried in the email, `incidents.ts:84`); ADR-0009 (the at-least-once event delivery the best-effort notify projects off); ADR-0012 (the transitory TypeScript detect seam this module co-locates with, and the deferred Go prod-driver that will drive the cycle).
- **Authors:** Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

Blueprint §18 MVP acceptance criterion 4 requires *"Gmail/SMTP notification"* (`../product/blueprint.md:749`); the MVP narrative says the SOC *"receives an email with the summary"* of a grouped incident (`:740`). SPEC-007 and SPEC-008 deferred it as a *"future notifier slice … needs SMTP/Gmail credentials (ask-first)"* (`SPEC-007-incident-grouping-mvp.md:37`, `SPEC-008-auth-core.md:42`). This SPEC implements that slice for the **create** case, on the decisions fixed in [ADR-0017](../adr/0017-incident-email-notification.md).

A READ-ONLY audit (this session) established the constraints this SPEC fixes; it does not re-debate them:

- Notify-only, criterion MVP 4 — on incident **creation**, not escalation.
- A module in `services/ingest` (where detection and the `upsertIncident` write live), hung off `upsertIncident` (`incidents.ts:66-101`) — the point a future production driver activates without re-touching.
- **Test-validated altitude.** `runDetectionCycle` has no production caller (`index.ts:24`, test-only); this SPEC inherits that gap and does not resolve it. The capability is wired at the correct seam and exercised through the detection harness, exactly as MVP criteria 1–3 are — it is **not** an email running in production today.
- Generic SMTP, fire-and-forget after the upsert commits (ADR-0017).
- Recipient + SMTP credentials are operator-set `services/ingest` config (deployment-contract), never derived from `users.email`.

## Scope

### In scope

1. A notify module in `services/ingest` that, when `upsertIncident` **inserts a new incident row**, sends one best-effort email to the configured recipient over generic SMTP.
2. A create-vs-update signal on `upsertIncident` (mirroring `upsertAlert`'s `rowCount === 1` seam, `alerts.ts:74`) so the notify fires on **insert** only, never on a correlated-update / severity-raise of an existing incident.
3. The dispatch hung **after** `upsertIncident` resolves, **outside** its transaction, in `runDetectionCycle`'s new-alert block (`index.ts:38-55`); a send failure is caught, logged, and not propagated.
4. The notify config surface in `services/ingest` (EnvSchema, mirroring `INGEST_CA_PASSPHRASE`, `config.ts:18`): SMTP endpoint + credentials + sender + recipient.

### Out of scope

Each names its destination:

- **Escalation notification** — only creation notifies; an existing incident's severity raise (`GREATEST`, `incidents.ts:84`) emits no notification. A later increment, once an escalation transition is first-classed (ADR-0017 §Out of scope).
- **Delivery guarantees** (retry, durable queue / outbox, dead-letter) — the MVP is a single best-effort attempt. A later increment (ADR-0017 §A3).
- **Per-org / templated routing** — a single operator-set recipient; per-org templates are a later increment.
- **The SOAR playbook** (Blueprint §18 criterion 6) — a separate MVP item, separate branch; `services/soar/` stays a placeholder.
- **The production detection driver** — the Go `services/pipeline/` extraction / firehose that gives `runDetectionCycle` a prod caller (ADR-0012 §1 / §Out of scope, `../adr/0012-normalize-before-correlate-pipeline.md:28,236`). Inherited gap; notify rides whatever drives the cycle.
- **Recipient from `users.email`** — ownership boundary (the `users` table is owned by `services/api` per SPEC-008); recipient is config.

## Data contracts

### 1. The notification trigger (create-only)

The notify fires **iff** `upsertIncident` inserts a new incident row. `upsertIncident` today returns `Promise<void>` (`incidents.ts:66-69`) and exposes no create-vs-update signal; the code gate adds one by mirroring `upsertAlert` (`alerts.ts:74`, `result.rowCount === 1 ? alertId : null`) — e.g. a `RETURNING (xmax = 0) AS inserted` on the `INSERT … ON CONFLICT (grouping_key) DO UPDATE` (`incidents.ts:76-85`), where `xmax = 0` marks a fresh insert and a non-zero `xmax` marks a correlated DO UPDATE. A new alert that joins an **existing** incident (same `grouping_key`) updates, not inserts, and MUST NOT notify.

### 2. The email payload (what the SOC receives)

All fields are available at the seam, with no extra read:

- `incident_id` — the incident identity / link (`incidents.ts:77`, UUIDv7).
- `severity_id` — the incident severity at creation (`incidents.ts:77`, `:84`; SPEC-011 OCSF severity).
- `title` — the deterministic `"<canonical-tactics> activity on <agent_id>"` (`incidents.ts:72`).
- `org_id` — the owning org (`incidents.ts:77`; single-org `"default"` in the MVP).

The email body MUST contain at least `incident_id`, the severity, and the title. The exact subject / body format is a code-gate detail; no per-org templating (§Out of scope).

### 3. The config surface (operator-set, deployment-contract)

New `services/ingest` EnvSchema vars (mirroring `INGEST_CA_PASSPHRASE`, `config.ts:18`). **The exact set and names are tentative here and fixed at the code gate** (with `.env.example` wiring); the operator supplies the **values** (owner-STOP deployment-contract, §Open questions):

| Tentative var | Purpose |
| --- | --- |
| `INGEST_SMTP_HOST` | SMTP relay host (Gmail = `smtp.gmail.com`, or M365 / corporate relay) |
| `INGEST_SMTP_PORT` | SMTP port (e.g. 587 STARTTLS) |
| `INGEST_SMTP_USER` | SMTP auth user |
| `INGEST_SMTP_PASS` | SMTP auth secret (`z.string().min(1)`, no default — like `INGEST_CA_PASSPHRASE`) |
| `INGEST_SMTP_FROM` | sender address the email is sent as |
| `INGEST_NOTIFY_RECIPIENT` | the SOC mailbox notifications are sent to |

## Acceptance criteria

Each AC is CI-able with an **in-memory fake SMTP transport** (a spy / JSON transport — no network, no real mail), exercised through the same synthetic-event → `runDetectionCycle` path as the `detect_ac_*` suite (`services/ingest/test/`). No real email is sent.

- **notify_ac_001 (fires on create).** `services/ingest/test/notify-ac-001-on-create.test.ts`. A synthetic new alert that creates a new incident (a fresh `grouping_key`) → the notify transport is invoked exactly once, with an email carrying that incident's `incident_id`, `severity_id`, and `title`. Verified via the fake-transport spy.
- **notify_ac_002 (silent on correlated update).** `notify-ac-002-no-notify-on-update.test.ts`. A second synthetic alert that joins an **existing** incident (same `grouping_key`, an `ON CONFLICT DO UPDATE`) → the notify transport is **not** invoked. Exercises the create-vs-update seam (`xmax = 0`, mirroring `upsertAlert` `alerts.ts:74`).
- **notify_ac_003 (fire-and-forget — send failure does not break the pipeline).** `notify-ac-003-fire-and-forget.test.ts`. With a fake transport that **throws** on send, a new incident still persists (the row is present after `runDetectionCycle`), the cycle resolves successfully (`DetectCycleResult` unchanged, no throw), and the failure is logged. Proves notification does not block or fail detection.
- **notify_ac_004 (email content).** `notify-ac-004-content.test.ts`. The composed email contains the `incident_id`, the severity, and the title (`"<tactics> activity on <agent_id>"`). Asserted on the captured message from the fake transport.

## Test scenarios

Per ADR-0005 §Harness obligation; each maps 1:1 to an AC.

- **SC-NTF-001 — notify on new incident.** New alert → new incident → one email with id + severity + title. Realised by notify_ac_001.
- **SC-NTF-002 — no notify on correlated update.** New alert → existing incident updated → no email. Realised by notify_ac_002.
- **SC-NTF-003 — best-effort isolation.** Transport throws → incident persists, cycle succeeds, failure logged. Realised by notify_ac_003.
- **SC-NTF-004 — email content.** Email carries incident_id + severity + title. Realised by notify_ac_004.

## Compliance

- The notify **MUST** fire only when `upsertIncident` **inserts** a new incident row; it **MUST NOT** fire on a correlated `ON CONFLICT DO UPDATE` of an existing incident (the create-vs-update seam mirrors `upsertAlert` `rowCount === 1`, `alerts.ts:74`).
- The `upsertIncident` write **MUST** commit before the notify is dispatched, and the dispatch **MUST** be outside that write's transaction.
- A notify send failure **MUST** be caught and logged and **MUST NOT** propagate into `runDetectionCycle` or change `DetectCycleResult` — best-effort, fire-and-forget (ADR-0017 §2).
- Transport **MUST** be generic SMTP; it **MUST NOT** use the Gmail REST API / Google OAuth (ADR-0017 §1).
- Recipient, sender, SMTP endpoint, and credentials **MUST** come from operator-set `services/ingest` config (EnvSchema, like `INGEST_CA_PASSPHRASE`, `config.ts:18`) and **MUST NOT** be derived from `users.email` (owned by `services/api`, an ownership boundary).
- This SPEC closes criterion MVP 4 at **test-validated altitude** only: `runDetectionCycle` has no production driver (`index.ts:24`), so it **MUST** be read as delivering a testable capability hung at the correct seam, **not** an email running in production. When the prod-driver lands (the ADR-0012 firehose), notification activates without re-touching the seam.

## Risks

| Risk | Mitigation |
| --- | --- |
| Notify fires on every correlated re-fire (spam) instead of once per incident | The create-vs-update seam (`xmax = 0`) gates on **insert** only; notify_ac_002 asserts silence on update. |
| A slow / down SMTP relay stalls or fails the detection pipeline | Dispatch is after-commit and fire-and-forget; notify_ac_003 asserts the cycle succeeds and the incident persists even when the transport throws. |
| A missed email goes unnoticed (best-effort, no retry) | The incident is durably persisted and visible in the dashboard / read API; the send failure is logged; delivery guarantees are a named future increment (ADR-0017 §A3). |
| Reading the recipient from `users.email` breaks the service-ownership boundary | Recipient is operator config in `services/ingest`; Compliance forbids deriving it from the `services/api`-owned `users` table. |
| The proposed env-var set is wrong / incomplete as a deployment contract | The var set is tentative (§Data contracts 3) and an owner-STOP Open question; it is fixed (with `.env.example`) at the code gate, never defaulted. |

## Open questions

1. **SMTP endpoint, credentials, sender, and recipient (deployment contract — DEFERRED, owner-STOP).** This SPEC fixes that the notify is configured from operator-set `services/ingest` env vars and proposes the var **names** (§Data contracts 3); it does **not** pick their **values**, nor finalise the exact var set. These are a deployment-contract decision the client operator owns (a new config surface they set), in the same owner-STOP class as SPEC-012's `forensic_pubkey` trust anchoring — not an engineering default chosen by design inertia. **Reopen / settle when:** the code gate lands the EnvSchema fields + `.env.example` placeholders, ratified by Manuel; or the client operating model defines the SOC mailbox.

## Ratification record

Load-bearing decisions for Manuel's gate (recommended-default-and-rationale pattern, per SPEC-005..013):

1. **Notify-only, on incident CREATE** — closes criterion MVP 4's email; escalation is a later increment (no first-class escalation transition exists today).
2. **Module in `services/ingest`, hung off `upsertIncident`** — where detection and the write live; the single production insertion point (`index.ts:38-55`).
3. **Generic SMTP, not Gmail API** — portable, no Google-OAuth coupling; Gmail is one SMTP host (ADR-0017 §1).
4. **Fire-and-forget after commit, outside the transaction** — the persisted incident is the truth; the pipeline's reliability is never coupled to a third-party relay (ADR-0017 §2).
5. **Recipient + credentials are operator config, not `users.email`** — service-ownership boundary; deployment-contract owner-STOP.
6. **Test-validated altitude, honestly** — inherits the no-prod-driver gap (ADR-0012); a testable capability at the right seam, not prod email.
7. **The exact env-var set is tentative** — fixed at the code gate with `.env.example`; values are operator-set (Open question 1).
8. **Doc-only this gate** — ADR-0017 + SPEC-014 at `Proposed`; code (the notify module, the `upsertIncident` create-signal seam, the EnvSchema vars, the four `notify_ac_*` tests) is the next gate.

## References

- [ADR-0017](../adr/0017-incident-email-notification.md) — the two load-bearing decisions this SPEC implements (generic SMTP; fire-and-forget first side-effect).
- [Blueprint](../product/blueprint.md) — `:749` (MVP criterion 4 *"Gmail/SMTP notification"*; duplicate `:664`), `:740` (the *"receives an email with the summary"* narrative), §16 (the *"notifier abstraction with SMTP fallback"* origin).
- [SPEC-007](SPEC-007-incident-grouping-mvp.md) — the incident grouping `upsertIncident` this hangs off; its `§Out of scope:37` notifier deferral this supersedes. Code: `services/ingest/src/detect/incidents.ts:66-101` (`upsertIncident`; title `:72`, `GREATEST` `:84`), `services/ingest/src/detect/index.ts:38-55` (the new-alert block).
- [SPEC-008](SPEC-008-auth-core.md) — the second notifier deferral (`§Out of scope:42`) this supersedes (the email-delivery half only; the TOTP-on-screen and password-reset scoping is unchanged).
- [SPEC-006](SPEC-006-detection-mvp.md) — the detection MVP producing the alerts; `services/ingest/src/detect/alerts.ts:74` (the `rowCount` create-vs-existing seam mirrored here for incidents).
- [ADR-0009](../adr/0009-event-delivery-and-buffer.md) — at-least-once delivery + dedup (`:29,33`); the durable record best-effort notify projects off.
- [ADR-0012](../adr/0012-normalize-before-correlate-pipeline.md) — the transitory TS detect seam (`:28`, `:213`) and the deferred Go prod-driver / firehose (`:236`).
- `services/ingest/src/config.ts:18` (`INGEST_CA_PASSPHRASE`, the required-secret EnvSchema pattern the notify config mirrors).
