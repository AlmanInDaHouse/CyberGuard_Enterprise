# Handoff — End of Session 24

Canonical state-of-the-world at the close of Session 24. This document is the contract Session 25 resumes from; it is written to let a **cold or compacted** session recover the thread in one read — nothing here assumes the Session 24 conversation.

Session 24 delivered **incident email notification — MVP acceptance criterion 4** (*"Gmail/SMTP notification"*, `blueprint.md:749`; the SOC *"receives an email with the summary"*, `blueprint.md:740`): when the detection pipeline **creates** a new incident, a best-effort email is sent over **generic SMTP**, fire-and-forget. This was the **S24 branch decision** — over MinIO at-rest, the reopened out-of-band trust-anchoring Open-Q, and the triage/detection pivots — chosen as the **most self-contained remaining MVP front**. It landed as **ADR-0017 + SPEC-014** (both `Accepted`) through the project's **READ-ONLY audit → approved-diff-gate → atomic-land** flow: the spec/decision layer first (born `Accepted`), then the code gate. S24 also cleared a **documentation-coherence debt in the canonical decision layer** (the ADR-0002/0007 Go→TS forensic contradiction). All `ts-ci` jobs are green at the code-gate tip and Known CI debt stays at **zero**.

- **Anchor commit:** `<S24-CLOSE-SHA>` (`docs(handoff): Session 24 close`) — substituted into the placeholder by an immediate follow-up commit per the two-commit anchor pattern (cf. Session 23 `9cc7ab6`/`63cd5fd`, Session 22 `1ec02d8`/`4702b8f`).
- **S24 ADR-coherence amendment SHAs (ADR-0002/0007 Go→TS, in-place):** `096eb81` (`docs(adr): amend ADR-0002/0007 -- forensic Go→TS superseded by SPEC-013 (coherence fix)`) + `2eed0d3` (`docs(adr): fix MD028 blank-line-in-blockquote in ADR-0002 forensic amendment`).
- **S24 spec-layer SHA (ADR-0017 + SPEC-014 Accepted + catalogs, atomic):** `a88f242` (`docs(adr-0017,spec-014): incident email notification (Accepted) -- SMTP fire-and-forget, notify-on-create`).
- **S24 code-gate SHA (notify module + seam + tests):** `7fdba4f` (`feat(spec-014): incident email notification -- notify-on-create, SMTP fire-and-forget, NotifyConfig DI`).
- **Branch:** `main`
- **Date:** 2026-06-07
- **CI verdict at the code-gate tip (`7fdba4f`):** `ts-ci` **success** (the ingest job runs the 4 `notify_ac` on Linux + testcontainers with an **in-memory fake transport** — no network — alongside typecheck + biome + vitest across the three workspaces). `schema-validation` + `rust-ci` + `markdown-lint` **not triggered** (path filters — no `schemas/cges/**`, no `agent/**`, no `.md`), counting toward ALL GREEN. Verdict: **ALL GREEN**, verified first-hand (`head_sha == 7fdba4f`).
- **Known CI debt:** **ZERO rows** ([CLAUDE.md](../CLAUDE.md) §Known CI debt). S24 added none — every commit landed green (the audit→diff-gate→atomic-land flow), no RED-by-design phase to co-locate.
- **Working tree:** clean (only `.claude/` untracked, pre-existing local config).
- **Catalogs:** ADR — **17** entries (`0001`–`0017`), all Accepted ([docs/adr/README.md](adr/README.md)) — `ADR-0017` is new in S24. SPEC — **14** entries (`SPEC-001`–`SPEC-014`), all Accepted ([docs/specs/README.md](specs/README.md)) — `SPEC-014` is new in S24.

## Session 24 commit arc

S24's deliverable is **a docs-coherence amendment + a spec-layer atomic land + a code gate + a two-commit handoff close**, all green, scoped by READ-ONLY repo audits and held at diff gates for Manuel's review + local verification before each land.

| Commit | SHA | Scope / why |
|---|---|---|
| ADR-0002/0007 coherence amendment | `096eb81` | In-place `## Amendment 2026-06-07` on ADR-0002 + ADR-0007 recording that `services/forensic/` went **Go → TypeScript** (a module in `services/api`, realised by SPEC-013), closing the Accepted-vs-Accepted contradiction. Table-adjacent blockquote (ADR-0002, `:45` ingest precedent) + bottom amendment section (`:139` detection precedent); forensic pivot marked **NOT transitory** (its MVP home is the api module, unlike the ingest/pipeline firehose pivots). `Status` stays Accepted on both. |
| ADR-coherence MD028 fix | `2eed0d3` | The two adjacent `> **Amended …**` blockquotes separated by a blank line tripped `MD028/no-blanks-blockquote` **in CI** (escaped the local gate). Joined into one blockquote with a `>` separator; content unchanged. |
| ADR-0017 + SPEC-014 Accepted (atomic) | `a88f242` | The decisions + spec for incident email notification, born `Accepted`: **ADR-0017** (generic-SMTP transport; the pipeline's first external side-effect = fire-and-forget) + **SPEC-014** (the ingest notify module, on-CREATE, 4 `notify_ac`). Catalog rows + dependency edges in both READMEs (atomic flip Proposed→Accepted in the same commit). |
| **Code gate (notify module + seam + tests)** | **`7fdba4f`** | `src/notify/{index,transport}.ts`, the `upsertIncident` create-signal seam, `runDetectionCycle(config, notify?)` wiring, `config.ts` optional-SMTP `superRefine`, `services.ts` `NotifyConfig` wiring, `test/helpers/notify.ts` (spy), 4 `notify-ac-*`, `.env.example` + `docker-compose.dev.yml`, `package.json` (`+nodemailer +@types/nodemailer`), lockfile. 15 files, +640/−23. |
| Session 24 close (this handoff) | `<S24-CLOSE-SHA>` | `docs(handoff): Session 24 close`. Anchor SHA filled by the two-commit follow-up. |

## Incident email notification (criterion MVP 4) — delivery declaration

MVP criterion 4 (*"Gmail/SMTP notification"*) is **delivered as a testable capability hung at the correct seam** — explicitly **NOT** an email running in production today (see *altitude* below). SPEC-014's 4 ACs (`notify_ac_001`–`004`) are all CI-able with an **in-memory fake transport** (no network), exercised through the same synthetic-event → `runDetectionCycle` path as the `detect_ac_*` suite.

**The branch decision (S24 opening):** email notify over MinIO at-rest, the reopened trust-anchoring Open-Q, or a pivot. Notify is the **most self-contained** open MVP front — it hangs off the **existing** `upsertIncident` write, needs **no schema/migration** (it reads incident fields already written), and **no new service**. (MinIO and trust-anchoring both drag heavier dependencies; the latter is owner-gated.)

**The spec layer (`a88f242`) — ADR-0017 + SPEC-014, both `Accepted`:** unlike S23 (no ADR), this **did** warrant an ADR — it records two **load-bearing** decisions (transport choice + the first external side-effect), the deliberate contrast S23 documented (a render that composed existing outputs needed none; a new outbound effect does).

**The code gate (`7fdba4f`) — the notify module + seam + tests:**

- **`src/notify/index.ts`** — `notifyIncidentCreated(notify, incident)` builds the email (subject + body carry `incident_id`, severity, deterministic title) and sends via the injected `NotifyConfig.mailer`. **Fire-and-forget:** its own `try/catch` logs any send failure to the sink and **never throws** — it carries **no transport import**, so the detect-cycle tests inject an in-memory fake and never load `nodemailer`.
- **`src/notify/transport.ts`** — `buildNotifyConfig(config)` constructs the **real** `nodemailer` SMTP transport via `createRequire` (the `jcs.ts`/`canonicalize` CJS-from-ESM precedent), or returns **`null`** when SMTP is unconfigured (notify disabled cleanly; the boot does **not** fail). Imported only by `services.ts`, never by tests.
- **The create-signal seam** — `upsertIncident` now returns `{ created, incidentId, title, severityId, orgId }` via `RETURNING … (xmax = 0) AS inserted`. **`rowCount` is NOT used** — the statement is `ON CONFLICT DO UPDATE`, where a conflict also reports an affected row (unlike `upsertAlert`'s `DO NOTHING`); `xmax = 0` is the reliable insert-vs-update discriminator. `upsertIncident` stays a **pure DB write** — the side-effect lives in the caller. The widening `void → {…}` breaks **zero** callers (all six ignore the return).
- **The wiring (enganche B)** — `runDetectionCycle(config, notify?)` fires `notifyIncidentCreated` in the **caller**, after `upsertIncident` returns `created === true`, **only on CREATE** (never a correlated update). `notify` is an **optional 2nd parameter** — `DetectConfig` is **untouched**, so the existing `detect_ac` cycles are unaffected.
- **The config (1st `superRefine` in the repo)** — the six `INGEST_SMTP_HOST/PORT/USER/PASS/FROM` + `INGEST_NOTIFY_RECIPIENT` vars are **optional and all-or-nothing** (a `superRefine` cross-field check; empty strings from a `${CG_*:-}` compose default are treated as absent). Unset ⇒ notify disabled, boot OK; partial ⇒ a clear validation error.
- **4 CI-able `notify_ac`** (synthetic winword→powershell events → `runDetectionCycle(…, spyNotify)`): `notify_ac_001` (on-create fires, correct fields), `notify_ac_002` (a correlated DO UPDATE does **not** re-notify — two pairs, same grouping window, distinct dedup buckets, notify exactly once), `notify_ac_003` (**fire-and-forget** — a throwing transport does not abort the cycle; the incident persists; the failure is logged), `notify_ac_004` (content: `incident_id` + severity + title).

**Total criterion 4:** **4 `notify_ac` ACs**, all CI-able / fake-transport / no marquee — green locally (Manuel: `notify-ac 4/4`, `incident-ac 5/5`, `detect-ac 6/6` incl. the marquee after rebuild + elevated terminal — environment, not regression) and **ALL GREEN in CI** at `7fdba4f`.

## Architecture — Session 24 decisions

ADR-0017 records two **load-bearing** decisions and a hard honesty constraint:

- **Generic SMTP, not the Gmail REST API.** Gmail is reached as one SMTP host (`smtp.gmail.com`), never via Google OAuth — portability over the self-deployable promise (ADR-0017 §Decision 1).
- **The detection pipeline's first external side-effect = fire-and-forget, outside the upsert's transaction.** Notify is dispatched **after** the incident row commits; a send failure is **logged and swallowed**, never propagated. Rationale: the **persisted incident is the forensic truth**, the email is a best-effort projection off an already-durable record — coherent with ADR-0009's at-least-once event delivery (the durable record is authoritative; a missed email is recoverable from the incident, unlike a dropped event). The pipeline's reliability is **never coupled** to a third-party SMTP server (ADR-0017 §Decision 2; the A2 transactional / A3 outbox alternatives rejected).
- **Test-validated altitude, affirmed in both documents.** `runDetectionCycle` has **no production caller** (test-only); criterion 4 is closed as a **testable capability at the correct seam**, **not** prod email. The inherited prod-driver gap is **not** resolved here. Affirmed in ADR-0017 §Consequences (Negative) + SPEC-014 §Compliance.

**The S24 coherence amendment (ADR-0002/0007):** SPEC-013 (S23) realised the forensic render in **TypeScript** while two Accepted ADRs still asserted `forensic = Go` — an **Accepted-vs-Accepted contradiction** in the canonical decision layer. S24 closed it by **in-place amendment** (not a rewrite): a dated `## Amendment 2026-06-07` on both, marking the pivot **NOT transitory** (the api module is the MVP home, unlike the ingest/pipeline firehose pivots which name a Go exit). This is the **narrow ADR-layer** fix; the **broad** blueprint/`services/README.md` Go-constellation naming remains a separate debt (below).

## Open questions (live)

- **Out-of-band trust anchoring of `forensic_pubkey` (owner STOP, deployment contract).** Unchanged from S23 — SPEC-012's seal proves **integrity under a given key**, not **authenticity against a compromised server**. The resolution (likely a `CG_FORENSIC_PUBKEY_FINGERPRINT` operator-published pin) **depends on the client's operating model, which is still not determined** — an **owner STOP, not to be picked by design inertia**. Reopened in S23 (its render/export trigger fired); **not actioned in S24**. Tracked in [SPEC-012 §Open questions](specs/SPEC-012-forensic-evidence-hashchain.md) + [ADR-0016 §Out of scope](adr/0016-forensic-evidence-hash-chain.md) + [CLAUDE.md §Decision authority](../CLAUDE.md). The **single highest-weight live decision**.
- **SPEC-010 §Open questions (404 → 503).** A transient ClickHouse failure on the cross-store reads returns `404`, conflating *evidence-inaccessible* with *evidence-absent* (`GET /v1/incidents/:id/events`, `/forensic-export`, `/report`). Refine to `503`. Carried from S20; still open ([SPEC-010 §Open questions](specs/SPEC-010-forensic-event-drill.md) item 3).
- **SPEC-014 §Open questions (the SMTP/recipient deployment contract).** The six SMTP/recipient **var names** are fixed (SPEC-014 §Data contracts 3, `config.ts`); their **values** are an operator-set deployment contract (owner STOP), never defaulted. Settled at deploy when the client mailbox model is defined.

## Debts / deferred — with destination

- **🔴 Node 20 deprecation on GitHub Actions — URGENT, hard deadline 2026-06-16.** `actions/checkout@v4` and `markdownlint-cli2-action@v16` run on Node 20, which GitHub **forces to Node 24 starting 2026-06-16** (and removes 2026-09-16). The `markdown-lint` workflow already emits the deprecation warning. **If not actioned, the affected workflows will break.** **Destination:** a tooling/CI sweep (bump the action majors / pin Node 24) — **candidate #1 for S25**. Surfaced in every S24 docs push.
- **`markdownlint` local-vs-CI version skew (carried, both directions now).** Local `markdownlint v0.40.0` flags `MD060/table-column-style` on **pre-existing** catalog/handoff tables that the CI action (`markdownlint-cli2-action@v16`) does **not** (main is green) — AND, the inverse, CI caught an `MD028` in S24 that the local run did not surface until the file was edited. **Lesson:** run `markdownlint` **locally before every docs push** (S24's `2eed0d3` was the avoidable second commit), and when the local linter flags lines you did not touch, verify against `HEAD` before "fixing". **Destination:** pin the local `markdownlint-cli2` to the CI action's version, or align table style. Ties into the Node-20 tooling sweep.
- **Ingest harness fragility — hardcoded agent UUIDs collide across tests (new in S24).** `notify_ac_001` reused the agent id `…050` that `incident_ac_005` needs **un-enrolled** (to assert its FK rejection); because the suite shares one Postgres (single fork), enrolling it broke 005. Resolved by moving `notify_ac_001` to `…054`. **Root cause:** ingest tests pick agent ids by hand against a shared DB with no per-test namespace; the api side has a throwaway-DB pattern ingest lacks. **Destination:** an agent-id allocator or per-test schema for the ingest harness. **Reactive correction**, caught by the local run.
- **Marquee `detect_ac_001` "0 events" is an ENVIRONMENT trap (recurred, 2nd time).** S24's local failure was a **non-elevated terminal** (ETW `AccessDenied` → zero capture → zero alerts), **not** a stale binary and **not** the notify gate (the release `.exe` was newer than every `agent/` source; the pre-alert path is byte-unchanged; the marquee calls `runDetectionCycle` with no notify arg). The "0 events" signature is symptom-identical across *stale binary / not-elevated / dirty-watermark*. **Destination:** make the marquee precondition harder to forget (assert event count > 0 before the alert assertion, or detect-and-fail-fast on a non-elevated ETW open). **It never affects CI** (`skipIf(win32)`, dev-local only).
- **Documentation-coherence debt: Go → TS, the BROAD sweep (distinct from the S24 ADR amendment).** S24 fixed the **ADR-layer** contradiction (ADR-0002/0007). Still open: `blueprint.md:393` (the `report` stage = *"Go + headless Chromium"*) and `services/README.md` (rows labelling `ingest`/`forensic` as Go) still name a Go constellation the project does not build. **Destination:** a blueprint + `services/README.md` reconciliation sweep. Not blocking — docs aspirational, code authoritative.
- **Carried from S23 (unchanged, still open):** DDL-mirror of `cges_events` in the api test harness (SPEC-010 §Open-Q 1 trigger); shared `canonicalize` package not extracted; throwaway-DB migration-backfill pattern not extracted to a shared helper; double incident-resolution in `buildReport` (on-demand-scale-irrelevant).

## Background pendings — untouched, non-blocking

- **The detection prod-driver does not exist (the front S24 surfaced sharply).** `runDetectionCycle` has **no production caller** — no scheduler / cron / worker in `services/ingest/src` invokes it; it runs only under the test harness. The whole pipeline (detection → incident → **notify**) is **validated but does not run in production**. Its future home is the Go `services/pipeline/` extraction / event firehose, **deferred** per ADR-0012 §1 (the transitory TS detect seam, ported to Go on the firehose ADR). **Strong S25 candidate** — it is the load-bearing gap under criteria 1/2/4.
- **Infra-dev chore: `dashboard/` in dev compose.** Still not in `infra/dev/docker-compose.dev.yml`. Carried from S19.
- **`cases` ≠ `incident`.** No `cases` entity; the `Cases` dashboard view is P1, deferred (`blueprint.md:557`). `incidents` fills the human-work role in the MVP.
- **Carried forward, none actioned in S24:** MinIO at-rest (the last forensic increment); detection breadth (1 rule vs the ~10-rule bar — drags agent telemetry that does not exist yet: network / logon / registry / service-creation); triage-writes (analyst acknowledge/assign — the model preserves human triage, CSRF ready); SOAR / `email.send` (`services/soar/` README-only — criterion 6); ML/UEBA (`services/ml/` README-only); the visual `Severity` column; plus the long-tail items from S13–S23 handoffs.

## MVP scorecard — updated after Session 24

The Blueprint §18 acceptance criteria, against the repo at `7fdba4f`:

| # | Criterion (`blueprint.md`) | State after S24 |
|---|---|---|
| 3 | OTP login + RBAC 3 roles (`:748`) | **Delivered** (SPEC-008, `services/api`). |
| 5 | Incident PDF export (`:750`) | **Delivered** (SPEC-013, `services/api`). |
| 4 | Gmail/SMTP notification (`:749`) | **Delivered — NEW in S24** (SPEC-014; **test-validated altitude** — see below). |
| 1 | 10 detection rules (`:746`) | **Partial 1/10** — by ratified decision (SPEC-006). |
| 2 | Windows agent: processes / network / logins (`:747`) | **Partial 1/3** — only processes (SPEC-005); network + logins deferred. |
| 6 | 1 SOAR playbook (`:751`) | **Pending** — no SPEC, no code (`services/soar/` README-only). |
| 7 | Installation docs < 30 min (`:752`) | **Pending** — no install/runbook doc. |

**The load-bearing caveat across criteria 1/2/4:** the **detection prod-driver** (above) — the pipeline is validated end-to-end but has no production scheduler, so detection → incident → notify does not fire in a deployed system today. Closing that gap (the Go firehose extraction, ADR-0012) lights up all three at once.

## Procedural notes — Session 24

- **`nodemailer` from an ESM service via `createRequire`.** `nodemailer` is CJS; loaded via `createRequire(import.meta.url)("nodemailer")` (the `jcs.ts`/`canonicalize` precedent) with `import type Nodemailer from "nodemailer"` for the types (`@types/nodemailer` devDep — the type counterpart of the ratified runtime dep, like S23's `@types/react`). Confined to `notify/transport.ts`; the tests never load it (the notify interface is injected, not imported). `nodemailer` 8.0.10 has **zero peer-deps, zero runtime-deps** (clean install).
- **Injection over global services for the notify seam.** The detect path takes **no** `Services` object — every `detect/*` function opens its own per-call `pg.Pool` from `config.ingest.*`, and `DetectConfig` is pure data. So the in-memory fake transport reaches the cycle through a **`NotifyConfig` injected as an optional 2nd arg**, NOT through `services.ts` (which builds the prod `NotifyConfig` once at boot, dormant until a prod driver consumes it). This kept `DetectConfig` untouched and made the 4 `notify_ac` CI-able with no real socket.
- **`xmax = 0` is the create-vs-update discriminator for `DO UPDATE`, not `rowCount`.** `upsertAlert` reads `rowCount === 1` because it is `ON CONFLICT DO NOTHING` (a conflict affects zero rows). `upsertIncident` is `DO UPDATE` (a conflict affects one row), so `rowCount` cannot tell insert from update — `RETURNING (xmax = 0) AS inserted` (the Postgres system column) is the reliable signal. (SPEC-014 framed it as "mirroring `upsertAlert`" — true in spirit, but the mechanism differs; recorded so the next reader does not reach for `rowCount`.)
- **`markdownlint` local gate is mandatory before a docs push (re-learned the hard way).** S24's `MD028` escaped the local run (it was only flagged once the file was committed-and-pushed → CI red → the avoidable `2eed0d3` fix). The handoff close ran `markdownlint` locally first.
- **`gh` CLI is not installed in the agent environment.** CI monitoring used the REST API fallback (token via `git credential fill`, never logged) per CLAUDE.md §CI monitoring fallback; a `python3` JSON parser (stdin) extracted per-workflow status.

## Next decision — Session 25 (NOT decided here)

S24 closed MVP criterion 4 (email notify) and the ADR-layer Go→TS coherence debt. The next scope is **Manuel's to choose**, with a **cold re-anchor against the S24 anchor**. The map:

- **The detection prod-driver** — give `runDetectionCycle` a production caller (the Go `services/pipeline/` extraction / event firehose, ADR-0012). It is the **load-bearing gap** under criteria 1/2/4 — the pipeline is validated but does not run in prod. Strong candidate; the largest scope (brings the Go toolchain).
- **🔴 Tooling/CI sweep — Node 20 URGENT** (hard deadline **2026-06-16**): bump `actions/checkout` + `markdownlint-cli2-action`, pin the local `markdownlint` to CI. Small, time-boxed, **deadline-driven**.
- **SOAR playbook (criterion 6)** — the `services/soar/` placeholder; one operational dry-run + notification playbook. Pending, no SPEC.
- **Installation docs (criterion 7)** — the < 30-minute deploy runbook. Pending, no SPEC.
- **Out-of-band trust anchoring** — only if the **client operating model** is now defined (owner-gated; do **not** implement by inertia).
- **MinIO at-rest** — the last forensic increment (persist the on-demand report/export).

**Session method (for workflow continuity).** Architect-advisor (decides scope + reviews at gates, does **not** execute) ↔ Claude Code (executor, **READ-ONLY** except an approved diff) ↔ Manuel (relay + diff gate + local tests). The **repo is the source of truth**; nothing is ratified from memory — every anchor is verified against `main` before it is written. For any future `detect/` change, the **elevated `detect_ac_001` marquee green** is the standing gate — **rebuild the agent binary first** AND **run from an elevated terminal** (both are required preconditions; S24 hit the elevation trap).

## Invariants carried into Session 25

Binding for every session. Authoritative text in [CLAUDE.md](../CLAUDE.md) + the per-session procedural notes in [docs/engineering-notes.md](engineering-notes.md). All invariants from prior handoffs carry forward unchanged.

- The **thirteen numbered conventions** remain in force. S24 produced **no new numbered convention**.
- **CLAUDE.md §"Local pre-commit gate"** spans the three TypeScript workspaces. S24 touched **only `services/ingest`** (typecheck + biome + vitest, incl. the 4 `notify_ac` on testcontainers) — the `detect/` elevated marquee gate applies to any `detect/` change (S24's notify wiring touched `detect/index.ts`, validated locally incl. the marquee after rebuild + elevation).
- **CI monitoring after every push** (CLAUDE.md) — mandatory; verified ALL GREEN at every S24 SHA (`ts-ci` for the code gate; `markdown-lint` for the doc-layer commits).
- **Known CI debt co-locality** (Convention #13) — not invoked in S24 (every landing was green); the **two-commit SHA-placeholder pattern** is still used for **this handoff's** anchor.
- **Deployment-contract decisions are an owner STOP** (CLAUDE.md §Decision authority) — the forensic-pubkey trust anchoring is the live instance; the SPEC-014 SMTP/recipient values are a second (var names fixed, values operator-set).

## How Session 25 resumes

1. Read this document and prior handoffs ([docs/handoff-session-23.md](handoff-session-23.md) back through [docs/handoff-session-9.md](handoff-session-9.md)) plus [CLAUDE.md](../CLAUDE.md), [SPEC-014](specs/SPEC-014-incident-notification.md), [ADR-0017](adr/0017-incident-email-notification.md), and the forensic set ([SPEC-013](specs/SPEC-013-forensic-report-render.md), [SPEC-012](specs/SPEC-012-forensic-evidence-hashchain.md), [ADR-0016](adr/0016-forensic-evidence-hash-chain.md)). They are the binding contract.
2. Confirm `main` is at the Session 24 anchor and all workflows are green. `git status` should show the working tree clean (modulo `.claude/`). Known CI debt: zero rows. ADR catalog: **17** Accepted; SPEC catalog: **14** Accepted.
3. **MVP criterion 4 (email notify) is delivered at test-validated altitude** — the pipeline (detection → incident → notify) is validated but has **no production driver**. The next scope is **Manuel's to choose** — the detection prod-driver (the load-bearing gap; brings the Go toolchain), the **URGENT** Node-20 tooling sweep (deadline 2026-06-16), SOAR (criterion 6), install docs (criterion 7), out-of-band trust anchoring (owner-gated), or MinIO at-rest. Architect-Claude at the S25 opening determines scope by reading the contracts + consulting Manuel — this handoff inventories the options, it does not pick one.
4. **Before any next elevated `detect_ac_001` run:** rebuild the agent binary (`cargo build --release -p cg-agent`) **and run from an elevated terminal** — both are required; S24 hit the non-elevated trap (zero capture, symptom-identical to a stale binary).
5. **Run `markdownlint` locally before every docs push** — S24's `MD028` escaped to CI and cost an avoidable follow-up commit.
6. Architect-Claude's auto-memory carries narrative; this document plus prior handoffs plus the ADR/SPEC catalogs and engineering-notes carry the facts; the repo at `main` is the ultimate source of truth. Where any two disagree, re-verify against `main`.
