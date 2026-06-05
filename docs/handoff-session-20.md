# Handoff — End of Session 20

Canonical state-of-the-world at the close of Session 20. This document is the contract Session 21 resumes from; it is written to let a **cold or compacted** session recover the thread in one read — nothing here assumes the Session 20 conversation.

Session 20 delivered **the forensic event drill, step 1** — the first concrete step of the title promise *"an exportable forensic report on the first incident"* (`blueprint.md:33`). `GET /v1/incidents/:id/events` serves a timeline of the raw `cges_events` underlying an incident's alerts: the project's **first read that crosses the Postgres → ClickHouse boundary**, served behind the human session. It is a small, bounded read-depth increment — not a phase — delivered through the **READ-ONLY audit → approved-diff-gate → atomic-land** flow rather than a harness-first RED/GREEN arc. All `ts-ci` jobs are green (including `check-api` now starting a ClickHouse container on Linux) and Known CI debt stays at zero.

- **Anchor commit:** `7b723f6` (`docs(handoff): Session 20 close`) — substituted into the placeholder by an immediate follow-up commit per the two-commit anchor pattern (cf. Session 19 `03a6eb1`/`d16ebca`).
- **S20 GREEN delivery SHA (lands the drill + ADR-0015 + SPEC-010):** `0480459` (`feat(spec-010): forensic event drill -- incident -> raw cges_events timeline`).
- **Branch:** `main`
- **Date:** 2026-06-05
- **CI verdict at the GREEN SHA (`0480459`):** `ts-ci` **success** — all three jobs green: `api — tsc + biome + vitest` (the drill ran against a real ClickHouse testcontainer), `ingest — tsc + biome + vitest`, `dashboard — next build + tsc + biome + vitest`. `markdown-lint` success. `schema-validation` + `rust-ci` not triggered (path filters — neither `schemas/cges/**` nor `agent/**` touched), which count toward ALL GREEN. Verdict: **ALL GREEN**.
- **Known CI debt:** **ZERO rows** ([CLAUDE.md](../CLAUDE.md) §Known CI debt). S20 added none — the work landed atomically at the diff gate, so there was no RED-by-design phase to co-locate.
- **Working tree:** clean (only `.claude/` untracked, pre-existing local config).
- **Catalogs:** ADR — 15 entries (`0001`–`0015`), all Accepted ([docs/adr/README.md](adr/README.md)). SPEC — 10 entries (`SPEC-001`–`SPEC-010`), all Accepted ([docs/specs/README.md](specs/README.md)).

## Session 20 commit arc

Unlike Sessions 18–19 (multi-commit harness-first RED → GREEN arcs), Session 20's deliverable is a **single atomic landing**. The work was a bounded read-depth increment: it was scoped by a READ-ONLY repo audit, written and held at a **diff gate** for Manuel's architectural review, validated by Manuel running the **full elevated api suite locally green** (Docker — the drill ACs need a ClickHouse testcontainer), and only then landed in one commit. The doc artifacts (ADR-0015, SPEC-010, the SPEC-009 amendment) were ratified at that same gate and flipped Proposed→Accepted in the landing.

| Commit | SHA | Scope / why |
|---|---|---|
| **S20 landing (forensic drill step 1)** | **`0480459`** | ADR-0015 + SPEC-010 (both Accepted) + the SPEC-009 `:34` co-located amendment + the code + the three `drill_ac` tests + the compose wiring + the `pnpm-lock` update. One SHA: docs + diff + tests + infra, reviewed at the diff gate and landed atomically. |
| Session 20 close (this handoff) | `7b723f6` | `docs(handoff): Session 20 close` — anchor SHA filled by the two-commit follow-up. |

## Forensic drill (step 1) — delivery declaration

Step 1 of the forensic report is **delivered in full**.

**Delivered:**

- **The route — `GET /v1/incidents/:id/events` (SPEC-010 §Operational §1).** A nested read endpoint on `services/api` (`services/api/src/read/routes.ts`), behind SPEC-008's `makeRequireSession` preHandler, org-scoped to `session.org_id`, read-only (no role gate, no CSRF — NFR-009-004). Nesting precedent: `auth/routes.ts:63` (`/v1/users/:id/role`). A malformed / non-existent / cross-org `:id` → generic `404 { error: "not_found" }`, mirroring the incident-detail handler (`read/routes.ts:36-49`). Returns `{ incident_id, events: TimelineEvent[] }` ordered by event time ascending.
- **The cross-store join (SPEC-010 §Operational §2).** `getIncidentEventTimeline(pool, ch, orgId, id)` (`services/api/src/read/queries.ts`) resolves the incident → its `alert_ids` → the alerts' `source_events` in **Postgres** (reusing the `getIncidentDetail` set-membership resolution), flattens + dedups the event ids in TS (the repo's canonical array handling — there is no `array_agg`/`unnest` precedent), then reads `cges_events` by `event_id` in **ClickHouse** (`WHERE org_id = {org:String} AND toString(event_id) IN ({ids:Array(String)})`, `FINAL`, `ORDER BY time ASC`, `JSONEachRow` — the `read-model.ts:136-158` array-membership pattern). The join `cges_events.event_id = ANY(alerts.source_events)` is value-match (version-agnostic, ADR-0012 §4); no DB-level FK crosses the stores.
- **The read-model — `TimelineEvent` / `EventTimeline` (`services/api/src/read/types.ts`).** Projects only the **populated-in-v0.1** `cges_events` columns: `event_id`, `agent_id`, `activity_id`, `process_pid`, `process_uid`, `process_name`, `image_file_name`, `process_parent_pid`, `event_time`. The `process_command_line` / `subject_user_sid` columns are `DEFAULT ''` and structurally empty in v0.1 (`read-model.ts:57-60`) — not projected.
- **3 acceptance criteria, green:**
  - **drill_ac_001** (marquee — the real cross-store join Postgres → ClickHouse): an incident whose alert's `source_events` point at seeded `cges_events` rows returns `200` with the events in `time` ASC, org-scoped (`services/api/test/drill-ac-001-timeline-join.test.ts`).
  - **drill_ac_002** (session + org — SECURITY): no/invalid `cgsess` → `401` before the handler; a cross-org incident → `404` (no existence oracle).
  - **drill_ac_003** (empty vs absent): an existing incident with no resolvable events → `200 { events: [] }` (not 404); a non-existent or non-UUID `:id` → `404`.
- **ADR-0015 (Accepted) — the boundary.** `services/api` acquires a **read-only ClickHouse client**, a singleton on `Services` built from `API_CH_*` (`services/api/src/services.ts`, the request-path pattern, `ingest/routes/heartbeat.ts:55`); `ingest` keeps sole **write** ownership of `cges_events`. The client is **lazy** (no connection until the first query), so the three existing Postgres read routes do not degrade if ClickHouse is down. `Services.close()` migrated to `Promise.allSettled([pool.end(), redis.quit(), ch.close()])` (aligns with `ingest/services.ts:45`).
- **SPEC-009 `:34` amended by-scope.** The drill SPEC-009 deferred-with-destination ("a read-depth increment") is delivered; a co-located `## Amendment 2026-06-05` records it ([specs/SPEC-009-read-slice.md](specs/SPEC-009-read-slice.md)). **AC-neutral:** `source_events` was added only to the internal `AlertRow` / `ALERT_COLS`, never to `ResolvedAlert` / `IncidentDetail`, so the dashboard contract is byte-for-byte unchanged. SPEC-009 stays **Accepted**.

**Verification:** the full `services/api` suite is **green** (19 test files / 36 tests — `+5` over S19's 31: the 3 `drill_ac` files contribute 5 tests). Zero regression in `read_ac` / `auth_ac`. CI confirms the cross-store path on Linux: the `api` job runs `vitest` including the `drill_ac` tests, which seed a real ClickHouse testcontainer — its green proves `check-api` now starts ClickHouse and the join works end to end. The `dashboard` job green confirms the `IncidentDetail` / `ResolvedAlert` contract was not broken (`dash_ac` intact).

## Architecture decision Session 20 generated (the trail that must survive)

**ADR-0015 — Read-only ClickHouse reader in `services/api` (the forensic event-drill boundary). Accepted, `0480459`.** Not re-litigable without a new gate.

- **Decision.** `services/api` reads `cges_events` directly via a read-only ClickHouse client (singleton on `Services`, request-path pattern); `services/ingest` retains sole write/DDL ownership of `cges_events`. The cross-store join is served inside one api request.
- **Rejected alternative — endpoint-in-`ingest` + api proxy (A1).** It would introduce the **first inter-service network hop** in the system (the audit confirmed `services/api` has zero `fetch`/HTTP calls to `ingest`; they communicate only through the shared Postgres instance) **and** require replicating/proxying the human session auth (`makeRequireSession` + org-scope live only in `services/api`) into the **agent-mTLS** boundary (ADR-0014 §3), merging two trust boundaries the threat model keeps separate. A read-only CH client in api is the smaller, boundary-preserving change. A factory-per-call client (A2) was also rejected for the request path (that is ingest's *background* pattern; the request-path precedent is the injected singleton).
- **Neutral / scope.** Amends **SPEC-009 §Out of scope `:34` by scope** (not by contradiction). Does **not** supersede ADR-0003 (it adds a *reader*, it does not re-route storage) and does **not** supersede ADR-0014 (the human/agent boundary stands). No migration, no schema change — the data was already wired (`alerts.source_events uuid[]` ↔ `cges_events.event_id UUID`). Catalog edges recorded: `ADR-0015 → ADR-0003`, `ADR-0015 → ADR-0014`, `ADR-0015 → SPEC-009 (:34)`; `SPEC-010 → ADR-0015`, `SPEC-010 → SPEC-009 (:34)`.

## Forensic — remaining mini-roadmap (the S21 decision context)

The title promise *"exportable forensic report on the first incident"* (`blueprint.md:33`) now has its **step 1 (drill / timeline) delivered**. The remaining steps — verified against the repo, each greenfield except where noted — none blocks the others **except render, which needs the prior steps**:

1. **Severity / score aggregation per incident.** Today `incidents` persists `cg_mitre` (jsonb) but **NO** severity/score column — those live only per-alert (`alerts.severity_id`, `alerts.final_score`). Verified: `0005_incidents.ts` has no `severity`/`score` column. A forensic report header wants an incident-level severity/score, which would aggregate from the grouped alerts. **Small.**
2. **Evidence integrity / hash-chain at-rest.** The `evidence_n.hash` / `chain_n` / `root_signature` design is documented at `blueprint.md:527-535` but **zero hashing of stored content exists** today — "auditable" rests only on append-only tables + the agent's transport-layer Ed25519 signing (verified server-side, then discarded; never persisted). This is the step that makes the *"auditable"* promise honest. Conceptually greenfield; likely warrants its **own ADR** (the evidence model). Independent of the others.
3. **MinIO wiring.** MinIO is infra-declared in `docker-compose.dev.yml` + assigned the forensic-artifact / report / archive home by ADR-0003, but **zero consumer code** exists in any service (`services/forensic/` is a README-only placeholder). Independent of the others.
4. **PDF / HTML render of the report (timeline + MITRE).** Total greenfield — no render dependency in any manifest (no puppeteer/playwright/pdfkit/weasyprint/templating engine; `blueprint.md:393` names it as a future `report` stage). **Executable only once the prior steps exist** (it renders their outputs).

## Open questions (live)

- **SPEC-010 §Open questions (404 → 503).** A transient ClickHouse failure on `GET /v1/incidents/:id/events` currently returns `404` (mirroring `read/routes.ts:36-49`), conflating *evidence-inaccessible* with *evidence-absent*. Refine to `503` to distinguish a store failure from not-found. Deferred from step 1; **revisit when planning the next forensic step** ([specs/SPEC-010-forensic-event-drill.md](specs/SPEC-010-forensic-event-drill.md) §Open questions, item 3).

## Debts / deferred — with destination

- **DDL-mirror of `cges_events` in the api test harness (test-only).** `services/api/test/helpers/events-schema.ts` creates `cges_events` from a **verbatim copy** of ingest's DDL (`ingest/src/db/migrate.ts:134-153`). This is necessary because ingest's ClickHouse bootstrap is **private** and not a reusable migration file (unlike the Postgres migrations the api harness applies in-workspace). It **silently desynchronises if ingest's `cges_events` DDL changes**. **Trigger** (SPEC-010 §Open questions 1): replace with a shared ClickHouse-schema module (or an exported `bootstrapClickHouse`) on the first un-caught DDL drift, or when a second api↔ingest ClickHouse table is shared. A drift on a *projected* column is caught by `drill_ac_001` at the GREEN gate.

## Background pendings — untouched, non-blocking

- **Infra-dev chore: `dashboard/` in dev compose.** `dashboard/` is **not** in `infra/dev/docker-compose.dev.yml` (verified). The declared follow-up (S19) — wire it with `API_BASE_URL`/port mirroring how `services/api` was wired (`843405f`) — is a standalone dev-convenience chore in its own SHA; the dashboard talks HTTP to the api, so it is not part of any feature's definition-of-done.
- **Blueprint §MVP ↔ ADR/SPEC deferrals reconciliation.** A READ-ONLY audit this session established the gap is **smaller than feared**: nearly every MVP item not yet built is **deferred-with-an-explicit-destination** (email/notifier, SOAR engine, packaging per ADR-0010 §part 2, the full 10-rule detection bar per ADR-0012/SPEC-006, Linux/network rules), **not a silent contradiction**. The one promise-level tension was **forensic** (`blueprint.md:33`) — which step 1 (this session) begins to close.
- **`cases` ≠ `incident`.** There is **no** `cases` entity (no table, no migration — verified). The `Cases` dashboard view is **P1, deferred** (`blueprint.md:557` §13). In the MVP, `incidents` (with `status` / `assigned_to`) fills the human-work role; `cases` is a post-MVP deep-management view.
- **Carried forward, still open (none actioned in S20):** the forensic steps 2–4 above; detection breadth (1 rule vs the ~10-rule bar); SOAR / `email.send` (never opened, `services/soar/` README-only); ML/UEBA (`services/ml/` README-only; `ueba_score`/`ml_score` columns reserved); the Go correlate / event-firehose extraction (`services/pipeline/` README-only); plus the long-tail items from S13–S19 handoffs (agent `event_id` v4/v7 reconciliation, the Phase-0 spike preservation, the dep-graph SPEC-node convention, the ajv-cli pinning note, NFR-008-002 session absolute cap).

## Procedural notes — Session 20

Candidates for the engineering-notes / convention record at a future gate:

- **The READ-ONLY audit → diff-gate → atomic-land flow is a legitimate alternative to the harness-first RED/GREEN arc for a *bounded* increment.** S20's drill was small, well-bounded, and data-unblocked; it was specified by repo audit, held at a diff gate for architectural review, validated by Manuel's local elevated suite (the green gate), and landed in one SHA. No RED-by-design phase means no Known CI debt to co-locate. This is appropriate for a bounded read-depth increment with an Accepted-doc gate; it is **not** a license to skip harness-first RED for a new phase or a new public surface.
- **A required-config addition (`API_CH_URL`) can stay boot-resilient via a lazy client.** Making `API_CH_URL` required (mirroring `INGEST_CH_URL`) does **not** make ClickHouse a boot dependency: `@clickhouse/client`'s `createClient` is lazy, so a down ClickHouse fails only the drill route, never api boot or the Postgres reads. "Required config" ≠ "required-at-boot connectivity."
- **`source_events` projected into the shared `ALERT_COLS` but kept out of the response read-models** keeps one resolution path (the drill reuses `getIncidentDetail`'s alert resolution) while leaving the SPEC-009 contract untouched — the additive-internal-field technique that made the change AC-neutral for the dashboard.
- **`markdownlint` local-vs-CI version skew.** A local `markdownlint v0.40.0` flags `MD060/table-column-style` on pre-existing table separators that the CI action (`markdownlint-cli2-action@v16`) does not (main is green). When the local linter reports errors only on **lines you did not touch**, verify against `HEAD` before "fixing" — changing the separators would be churn that could itself break CI's expected style.

## Next decision — Session 21 (NOT decided here)

Session 20 closed a bounded increment, not a phase. The next scope is **Manuel's to choose**, with a **cold re-anchor against `0480459`**. The map, each with its repo hook:

- **Forensic step 2** (sub-decision: severity/score aggregation — *small*; the evidence/hash-chain ADR — the *"auditable"* honesty; or MinIO + render — the report artifact). The drill (step 1) is the input; the mini-roadmap above is the menu.
- **Triage-writes** (quick-win, no architectural fork): analyst acknowledge/assign on incident/alert. The model already preserves human triage (SPEC-007 create-or-update invariant) and CSRF is ready (SPEC-008); it makes the dashboard interactive. A named fast-follow.
- **Detection breadth** (1 rule → the ~10-rule bar). **More expensive than it looks:** the named target scenarios (Blueprint SC001–SC010) drag **agent telemetry that does not exist yet** (registry, network, logon, service-creation events — only process telemetry is captured today). Not incremental over the current evaluator alone.

**Session method (for workflow continuity).** Architect-advisor (decides scope + reviews at gates, does **not** execute) ↔ Claude Code (executor, **READ-ONLY** except an approved diff) ↔ Manuel (relay + diff gate + local elevated tests). The **repo is the source of truth**; nothing is ratified from memory — every anchor is verified against `main` before it is written.

## Invariants carried into Session 21

Binding for every session. Authoritative text in [CLAUDE.md](../CLAUDE.md) + the per-session procedural notes in [docs/engineering-notes.md](engineering-notes.md). All invariants from prior handoffs carry forward unchanged.

- The **thirteen numbered conventions** remain in force. S20 produced procedural-note candidates (above), not new conventions.
- **CLAUDE.md §"Local pre-commit gate"** spans the three TypeScript workspaces — `services/ingest`, `services/api`, `dashboard`. S20 touched only `services/api` (+ docs + infra compose); it did **not** touch `services/ingest/src/detect/`, so the developer-local SPEC-005/006 marquee gate was not exercised. Note for S21: `check-api` now also starts a **ClickHouse** testcontainer (the api suite gained the drill ACs), so the api job is slightly heavier on Linux CI but green.
- **CI monitoring after every push** (CLAUDE.md) — mandatory; verified ALL GREEN at `0480459` (ts-ci 3 jobs + markdown-lint).
- **Known CI debt co-locality** (Convention #13) — not invoked in S20 (atomic landing, no RED-by-design phase); the two-commit SHA-placeholder pattern is still used for **this handoff's** anchor.

## How Session 21 resumes

1. Read this document and prior handoffs ([docs/handoff-session-19.md](handoff-session-19.md) back through [docs/handoff-session-9.md](handoff-session-9.md)) plus [CLAUDE.md](../CLAUDE.md), [ADR-0015](adr/0015-readonly-clickhouse-reader-in-api.md), and [SPEC-010](specs/SPEC-010-forensic-event-drill.md). They are the binding contract.
2. Confirm `main` is at the Session 20 anchor and all workflows are green. `git status` should show the working tree clean (modulo `.claude/`). Known CI debt: zero rows. ADR catalog: 15 Accepted; SPEC catalog: 10 Accepted.
3. **The forensic title promise has its step 1 (drill/timeline) delivered.** The next scope is **Manuel's to choose** — forensic step 2 (the mini-roadmap above), the triage-writes quick-win, or detection breadth (with its agent-telemetry cost flagged). Architect-Claude at the Session 21 opening determines scope by reading the contracts + consulting Manuel — this handoff inventories the options, it does not pick one.
4. The deployment follow-up (dashboard in dev compose) is a small named chore that can land in any SHA when convenient.
5. Architect-Claude's auto-memory carries narrative; this document plus prior handoffs plus the ADR/SPEC catalogs and engineering-notes carry the facts; the repo at `main` is the ultimate source of truth. Where any two disagree, re-verify against `main`.
