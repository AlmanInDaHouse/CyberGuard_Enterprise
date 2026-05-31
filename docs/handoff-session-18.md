# Handoff — End of Session 18

Canonical state-of-the-world at the close of Session 18. This document is the contract Session 19 resumes from. Session 18 had two arcs: **(1)** ratifying the four pending convention candidates (#10–#13) into the numbered catalog, and **(2)** **Phase 6 = A (Incident grouping MVP)** — the next link in the detection pipeline (`alert → incident`, Blueprint §9): grouping distinct correlated alerts into incidents. Phase 6 A is **delivered in full**: ADR-0013 + SPEC-007 ratified, the harness-first RED→GREEN arc complete, all five `incident_ac` acceptance criteria green (four CI-able, one green-guard), the elevated suite 49/49 green with both real-chain marquees, and Known CI debt back to zero.

- **Anchor commit:** `5df7f72` (`docs(handoff): Session 18 close`) — substituted into the placeholder by the follow-up commit per the two-commit anchor pattern (cf. Session 17 `91ad7de`/`ae273e2`).
- **Phase 6 A GREEN delivery SHA:** `bc3e604` (`feat(spec-007): incident grouping logic — incident_ac_002-005 GREEN, debt removed`).
- **Branch:** `main`
- **Date:** 2026-05-31
- **CI verdict at the GREEN SHA (`bc3e604`):** `ts-ci` success (47 passed / 2 skipped / 0 failed — the 2 skipped are the `detect_ac_001` + `spec-005` marquees, `skipIf(win32)` on Linux); `markdown-lint` success; `schema-validation` skipped (path-filtered — `incident.json`'s `cg_mitre` `$ref` was validated green at the schema-changing SHA `d76ea5c`). Verdict: ALL GREEN.
- **Elevated developer-local suite (Docker + admin shell + clean ETW slot):** 49/49 green, including **both** real-chain marquees — `spec-005` (2 ClickHouse rows) and `detect_ac_001` (1 Postgres alert, with grouping now wired in the real path).
- **Known CI debt:** ZERO rows. The `ts-ci` RED-by-design row (opened at `d76ea5c`) was removed at `bc3e604` — the same SHA that turned `ts-ci` green — per the debt co-locality rule (Convention #13).
- **Working tree:** clean (only `.claude/` untracked, pre-existing local config).

## Session 18 commit arc

| Commit | SHA | Scope |
|---|---|---|
| Conventions #10–#13 ratified | `82bb710` | Promotes the four pending candidates (S16 #10 release-binary-rebuild, #11 fmt-in-gate; S17 #12 fix-the-test-not-the-schema, #13 debt-co-locality) to numbered conventions in engineering-notes §Session 18; creates CLAUDE.md §"Local pre-commit gate" (#11); §16/§17 candidate notes tagged PROMOTED. Candidate catalog now empty. |
| ADR-0013 Proposed | `49a2f68` | Incident correlation windowing — event-time basis, distinct from the dedup bucket. Drafted Proposed for the gate. |
| ADR-0013 Accepted | `6e21663` | Flip → Accepted + atomic landing: co-located *Amendment to ADR-0012* (the correlation window is two distinct tunables, not one — §8's "share one tunable" superseded); README catalog row; dep-edge `0013 → 0012 (amends in part)`. ADR-0003 non-edge stays a §Neutral bullet. |
| SPEC-007 Proposed | `30889e9` | Incident grouping MVP. Drafted Proposed; passed a 4-lens adversarial review (all-minor findings applied). Co-located errata: ADR-0013's FK convention mis-cite `#3 / #12` → `#12`. |
| SPEC-007 Accepted | `ba58651` | Flip → Accepted + the `event_time` backfill grain honesty note (§Data contracts §1); README catalog row. |
| Harness-first RED | `d76ea5c` | Structure (migration 0004 `event_time` + backfill, 0005 `incidents`, `incident.json` `cg_mitre` `$ref`, `upsertAlert` event_time population) + harness (`incidents.ts` stub, 5 `incident_ac` tests). `incident_ac_001` green-guard; `002–005` RED via `upsertIncident` NotImplemented. Known CI debt (`ts-ci`) co-located with a `<RED-SHA>` placeholder. |
| RED SHA fill | `d3b4344` | Two-commit pattern: substitutes the debt placeholder with `d76ea5c`. |
| RED regression fix | `68c95f8` | The mandatory CI inspection caught a 5th failure: `migration-0002-alerts.test.ts`'s own `insertAlert` fixture broke on `event_time NOT NULL` (RED-by-breakage, not by-design). Fixed production-faithfully (fixture carries `event_time`); writer audit confirmed only `upsertAlert` + this fixture write to `alerts`. After this, `ts-ci` fails ONLY on `incident_ac_002–005`. |
| **GREEN delivery** | **`bc3e604`** | **`upsertIncident` grouping logic implemented; `incident_ac_002–005` GREEN; Known CI debt row REMOVED — RED-by-design ENDS.** `upsertAlert` returns the new `alert_id`; the cycle groups newly-persisted alerts (sibling step); declarative `grouping_key` + `ON CONFLICT DO UPDATE` with triage preservation. |

## Phase 6 A delivery declaration

Phase 6 A is **delivered in full**.

**Delivered:**

- **Incident grouping slice:** alerts → group by `grouping_key = <org>::<agent>::<canonical_tactics>::<window_bucket>` (event-time windowed, 1800 s, ADR-0013) → create-or-update one `incidents` row per group, accreting `alert_ids` without clobbering human triage. Hosted in the same transitory TS seam as Phase 5 (`services/ingest/src/detect/`, `runDetectionCycle` sibling step); incidents Postgres-only (ADR-0003).
- **`incident_ac_001`** (green-guard, CI-able): `cg_mitre` is non-empty on the **persisted** alert row — the first test to assert the persisted column, closing the gate-zero "populated, proved by code-trace + zod invariant, not by a row" nuance from the Phase-6 audit.
- **`incident_ac_002–005`** (CI-able, Linux ts-ci): grouping N→1, no-overgroup (distinct tactic / out-of-window), triage preservation, production-faithful FK. Green by correct logic — `incident_ac_005` by enrolling the agent, the FK never weakened (Convention #12).
- **Marquees revalidated elevated** with the grouping wired into the real path: `detect_ac_001` (real `cg-agent` ETW → alert → now also an incident) and `spec-005`, both green; `detect_ac_002–006` + `migration-0002` no regression.
- Known CI debt: zero rows. The RED-by-design `ts-ci` row was opened (`d76ea5c`) and closed (`bc3e604`) within Phase 6, co-located at both ends.

**Structure realised (the contracts SPEC-007 specified):**

- Migration `0004_alerts_event_time`: `event_time timestamptz NOT NULL` on `alerts` (event-occurrence time, ADR-0013 §1). New rows populated at write from the source event time (`eventUnixSeconds`, the single parse shared with the dedup bucket); pre-existing rows backfilled from the `dedup_key` bucket (5-min grain — accepted bounded debt, §Known follow-ups).
- Migration `0005_incidents`: the `incidents` table (Postgres-only) + `grouping_key` UNIQUE + `agent_id` FK → `agents` (production-faithful) + `alert_ids` cardinality CHECK.
- `incident.json`: `cg_mitre` field added (`$ref ../common/cg_mitre.json` — genuine mirror; all 8 CGES examples still validate, incl. `06_incident_grouped`).

## Conventions ratified — Session 18

The convention-candidate catalog (four entries pending since Sessions 16–17) is **cleared**. Promoted to numbered Conventions **#10–#13** (engineering-notes §Session 18):

- **#10** — rebuild the pre-compiled binary before a binary-backed test (S16 note 1).
- **#11** — the local pre-commit gate includes formatting, not just check+clippy; codified in CLAUDE.md §"Local pre-commit gate" (S16 note 2).
- **#12** — a synthetic test that trips a DB integrity constraint is fixed by satisfying the production precondition, never by weakening the constraint (S17 note 3).
- **#13** — Known CI debt co-located with the red-turning SHA, removed in the green-turning SHA; no path-filter splitting (S17 note 4; numbered pointer to the binding CLAUDE.md rule).

The nine Session 10 conventions + Session 11's Convention #5 extension + the full-SHA polling operational bullet remain in force. The catalog now holds thirteen numbered conventions.

## ADRs at session close — Session 18 delta

One new ADR, one amendment:

- **ADR-0013** (incident correlation windowing): **NEW — Accepted at `6e21663`**. Decides the windowing *basis* (event-time, never insert-time, §1) and *separateness* (an own correlation window distinct from and wider than the 300 s dedup bucket, §2); leaves the window *value* to SPEC-007. Mechanism (materialized column vs ClickHouse join) explicitly deferred to SPEC-007.
- **ADR-0012** (normalize-before-correlate): §8 amended (co-located, `6e21663`) — the correlation window is two distinct tunables, not the single shared tunable §8 originally framed; the 300 s dedup bucket is unchanged (backward-compatible).
- Errata: ADR-0013 §Out of scope's FK convention citation corrected `#3 / #12` → `#12` (`30889e9`).

ADR catalog: 13 entries, all Accepted.

## SPECs at session close — Session 18 delta

- **SPEC-007** (Incident grouping MVP): **NEW — Accepted at `ba58651`**. References ADR-0013 (no re-decide). Specifies the `event_time` column + backfill, the `cg_mitre` field, the `incidents` table + `grouping_key`, the create-or-update lifecycle with triage preservation, the FK, the 1800 s window, the seam, and five ACs. `§Operational §3` declares the `user` grouping dimension absent in v0.1 (empty `subject_user_sid`).

SPEC catalog: 7 entries, all Accepted.

AC validation status at session close:

| AC | Status | Gate |
|---|---|---|
| incident_ac_001 (cg_mitre persisted) | GREEN (ts-ci) | CI-able — green-guard, closes the gate-zero nuance |
| incident_ac_002 (grouping N→1) | GREEN (ts-ci) | CI-able |
| incident_ac_003 (no over-group) | GREEN (ts-ci) | CI-able |
| incident_ac_004 (triage preservation) | GREEN (ts-ci) | CI-able |
| incident_ac_005 (production-faithful FK) | GREEN (ts-ci) | CI-able |
| detect_ac_001 + spec-005 marquees | GREEN | Developer-local elevated (revalidated with grouping wired) |

## Known follow-ups

Co-located with their generating documents. New or changed in Session 18:

- **`event_time` historical backfill grain (accepted bounded debt).** Pre-migration `alerts` rows inherit the `dedup_key` 5-minute bucket grain; new rows are exact. The ±5-min uncertainty bites only at the edges of the 1800 s grouping window, only for incidents over pre-migration alerts, and never grows (post-migration alerts are exact). Destination: exact historical precision would need the rejected ClickHouse-join backfill (SPEC-007 §Data contracts §1); revisit only if it bites.
- **`user` grouping dimension absent in v0.1.** `subject_user_sid` is structurally empty (Kernel-Process provider, ADR-0012 §3), so grouping collapses to host(agent)+tactic+window, without `user`. Destination: a user-SID-bearing provider (same forward path as ADR-0012 §5's `dedup_key` subject omission). Recorded in SPEC-007 §Operational §3.
- **Window-boundary artifact.** A correlated chain straddling a 1800 s window edge splits into two incidents (the same posture as ADR-0012 §5's dedup bucket boundary). Destination: a sliding-window correlation increment if boundary splits prove noisy.
- **Per-tactic-set vs per-individual-tactic grouping.** v0.1 groups by the canonical tactic-set as one token. Destination: revisit when the rule count grows (SPEC-007 §Open questions).
- **Writer-audit lesson (process).** Adding a NOT NULL column to a shared table requires auditing ALL writers (production + test fixtures), not just the production path — the `migration-0002` fixture regression (`68c95f8`) came from not doing so; caught by the mandatory post-push CI inspection. Recorded in engineering-notes §Session 18 (a convention candidate).

Carried-forward follow-ups from prior sessions that remain open: the Phase 0 spike executable preservation decision (Session 16); Session 13's dep-graph SPEC-node convention; Session 12's ajv-cli pinning note. None were actioned in Session 18.

## Hook to C (the chained successor)

Phase 6's decided sequence is **A → C** (incidents → user-facing slice). A leaves these ready for C:

- **The `incidents` data model** (`0005_incidents`: `incident_id`, `status` enum, `alert_ids`, `cg_mitre`, `assigned_to`, `window_start`) is exactly what C's dashboard "Incident detail with grouped alerts + MITRE mapping" view (dashboard README) consumes. C reads `incidents` + `alerts`; it does not re-derive grouping.
- **`event_time` on `alerts`** is materialized — C's alert timeline / sort-by-occurrence is unblocked.
- **`status`/`assigned_to`** on both `alerts` and `incidents` are the mutable triage state C's analyst workflow mutates (the triage-preservation invariant guarantees automated grouping won't clobber a human's edits).

C's **open preconditions** (NOT addressed by A):

- **Human auth-store does not exist.** SPEC-002 is *agent* enrollment (mTLS); there is no user/OTP/RBAC store. C's "create a user with OTP … from the dashboard" (Blueprint §18) is a greenfield ADR + SPEC with a first-class security surface (OTP secret storage, session/token, RBAC enforcement).
- **alert → source-event drill-down hits the `event_id` v4/v7 mismatch.** The agent emits `event_id` UUIDv4; `alert.json`'s `source_events` was relaxed to version-agnostic for the MVP. A drill from an alert to its ClickHouse `cges_events` rows is affected; the v4/v7 reconciliation is ADR-0009/ADR-0011 domain (flagged in ADR-0012 §6).
- **No API/WebSocket surface yet.** `services/api/` and `dashboard/` are README-only placeholders.

## Invariants carried into Session 19

Binding for every session. Authoritative text in [CLAUDE.md](../CLAUDE.md) + the per-session procedural notes in [docs/engineering-notes.md](engineering-notes.md) §Session 18. All invariants from prior handoffs carry forward unchanged.

New in Session 18: the thirteen numbered conventions (1–9 Session 10, #5 extended Session 11, #10–#13 Session 18) + the CLAUDE.md §"Local pre-commit gate" (the per-workspace gate mirroring CI: `cargo fmt --all -- --check` + `clippy` + `cargo test`; `pnpm typecheck` + `lint` + `test`; `task validate-schemas`). The standing developer-local marquee gate (SPEC-005 + SPEC-006) applies to any change under `services/ingest/src/detect/` — exercised this session (the GREEN impl touched the detection path; Manuel revalidated both marquees elevated before push).

## How Session 19 resumes

1. Read this document and prior handoffs ([docs/handoff-session-17.md](handoff-session-17.md) through [docs/handoff-session-10.md](handoff-session-10.md)) plus [CLAUDE.md](../CLAUDE.md), [ADR-0013](adr/0013-incident-correlation-windowing.md), and [SPEC-007](specs/SPEC-007-incident-grouping-mvp.md). They are the binding contract.

2. Confirm `main` is at the Session 18 anchor and all workflows are green. `git status` should show working tree clean (modulo `.claude/`). Known CI debt: zero rows.

3. Phase 6 scope continues with **C** (the user-facing slice — API auth + dashboard) per the ratified A→C sequence, OR another candidate if Manuel re-prioritises (detection breadth toward the 10-rule MVP bar; packaging per ADR-0010 §part 2; the Go correlate extraction when the firehose ADR lands). Architect-Claude at Session 19 opening determines scope by reading the contracts + consulting Manuel. C's open preconditions (auth-store greenfield, event_id v4/v7, no API surface) are above.

4. Architect-Claude's auto-memory carries narrative; this document plus prior handoffs plus the ADR/SPEC catalogs and engineering-notes carry the facts; the repo at `main` is the ultimate source of truth. Where any two disagree, re-verify against `main`.
