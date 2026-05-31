# Handoff — End of Session 17

Canonical state-of-the-world at the close of Session 17. This document is the contract Session 18 resumes from. Session 17's scope was **Phase 5: Detection MVP** (Route A) — the first detection slice: read events from ClickHouse `cges_events` → normalize → evaluate a Sigma rule → score → persist alerts to Postgres. Phase 5 is **delivered in full**: all six `detect_ac` acceptance criteria are green (002–006 in CI, 001 validated developer-local), the harness-first RED-by-design phase has ended, Known CI debt is back to zero rows, and the end-to-end marquee was validated with a real `cg-agent` capturing live ETW. References to Sessions 10–16 supply the context this handoff does not replicate.

- **Anchor commit:** `<ANCHOR-SHA>` (`docs(handoff): Session 17 close`) — substituted in the follow-up commit per the two-commit anchor pattern (cf. Session 16 `77a340e`/`2264826`).
- **Branch:** `main`
- **Date:** 2026-05-31
- **CI verdict at anchor:** ALL GREEN on `main` workflows (`ts-ci`, `schema-validation`, `markdown-lint`). Developer-local SPEC-006 marquee `detect_ac_001` validated GREEN (real agent, elevated + Docker Desktop). Developer-local SPEC-005 marquee remains 8/8 GREEN (carried from Session 16).
- **Known CI debt:** ZERO rows. The single `ts-ci` debt row (the harness-first RED-by-design, declared at `24e0e4a`) was removed at `34aa436` (5e) — the same SHA that turned `ts-ci` green — per the debt co-locality rule.
- **Working tree:** clean (only `.claude/` untracked, pre-existing local config).

## Phase 5 commit arc

| Commit | SHA | Scope |
|---|---|---|
| AC-009 warm-up | `39dce4b` | `fix(spec-005)`: AC-009 trace pump topology `start()` + `process_from_handle()` (the Session 16 open follow-up). rust-ci GREEN. **Closes the AC-009 end-to-end follow-up.** |
| ADR-0012 accept | `daf4625` | Normalize-before-correlate pipeline Accepted. Amends ADR-0002 (Rule 3 — transitory TS correlate seam, honestly declared) + ADR-0003 (retention row — alerts Postgres-only for MVP). +7 dep-edges, catalog. |
| SPEC-006 accept | `91676e5` | Detection MVP Accepted. One Sigma rule `rule.office_spawns_script_host` (Office `ParentImage` → script-host `Image`; MITRE T1059/T1566). severity_id=4 (High), heuristic 0.9 → final 0.9. Documented production false-negative (already-running Office). |
| Harness-first RED | `24e0e4a` | `test(spec-006)`: `detect_ac_002..006` + rule + scenarios land RED (NotImplementedError). Known CI debt row (`ts-ci`) co-located in this SHA. |
| RED SHA fill | `304c786` | Two-commit pattern: substitutes the debt placeholder with `24e0e4a`. |
| 5a alerts table | `1fc2264` | Migration `0002_alerts` (ADR-0012 §6 shape; FK→agents; UNIQUE(dedup_key); CHECK constraints incl. severity 0..6 hardening). `migration-0002-alerts.test` 11/11 GREEN. |
| 5b read-model | `359580a` | `read-model.ts`: `readNewEvents` (FINAL + parent-pid self-join) + watermark; migration `0003_detect_watermark` (`last_time` TEXT for ns precision). ClickHouse alias-shadow gotcha fixed (`AS event_time`). `read-model.test` 4/4 GREEN. |
| 5c rule evaluator | `2f2a465` | `engine.ts`: `loadRules`/`parseRule` with zod `.strict()` loud-rejection (UnsupportedRuleError); `evaluateRule` pure, `\|endswith` over `Image`/`ParentImage`. Dep `yaml` (zero incremental surface). `engine.test` 8/8 GREEN. |
| 5d scorer | `fcc82f2` | `scorer.ts`: renormalized weighted-sum over ADR-0005 active-source weights; round to 3 dec (numeric(4,3); IEEE-754 fix). **`detect_ac_006` closed — first detect_ac green; debt narrowed 002–006 → 002–005.** `scorer.test` 5/5 GREEN. |
| 5e detection cycle | `34aa436` | `runDetectionCycle` wired (read-model → engine → scorer → `upsertAlert` → watermark) + `alerts.ts` (`buildDedupKey`, UUIDv7 alert_id via existing `uuid@11`, ON CONFLICT DO NOTHING). `detect_ac_002–005` closed in CI; `enrollTestAgent` makes the synthetic tests FK-faithful. alert.json `source_events` pattern relaxed. **Known CI debt row REMOVED — RED-by-design ENDS.** |
| Fixture refresh | `44fd345` | `examples/04` dedup_key → realized `agent_id::rule_id::process_name::bucket`; `examples/05` final_score 0.71 → 0.88. Documentary exactness; all 8 examples still validate. |
| Marquee validated | `884b40a` | CLAUDE.md §Developer-local SPEC-006 marquee validation: pending → VALIDATED (2026-05-31, 44/44, `detect_ac_001` 40171 ms). |

## Phase 5 delivery declaration

Phase 5 is **delivered in full**.

**Delivered:**

- End-to-end detection slice: ClickHouse `cges_events` (ReplacingMergeTree, read with FINAL) → normalize (parent-pid self-join, schema-faithful column names) → evaluate `rule.office_spawns_script_host` (Sigma `|endswith` subset) → renormalized score → persist exactly one alert per `(rule, event)` to Postgres `alerts`, dedup-collapsed by `dedup_key` (ON CONFLICT DO NOTHING).
- `detect_ac_001` (marquee): validated developer-local + elevated. A real `cg-agent` captured a `winword.exe` stand-in spawning `powershell.exe` via ETW; the event reached ClickHouse; `runDetectionCycle` persisted exactly one alert to Postgres. 19 files / 44 tests GREEN, marquee 40171 ms wall-clock.
- `detect_ac_002–006`: GREEN in `ts-ci` on the Linux runner (false-positive suppression, dedup, status preservation, watermark advance, score renormalization). First time the real detection cycle runs in CI; `enrollTestAgent` + the `alerts.agent_id→agents` FK behave identically to local.
- Known CI debt: zero rows. The RED-by-design `ts-ci` row was opened and closed within Phase 5, co-located at its open (`24e0e4a`) and close (`34aa436`).
- Fixtures aligned to the realized contract (`44fd345`).

**Session 16 follow-ups resolved:**

- **AC-009 end-to-end / `let _ = trace.start()` pattern:** RESOLVED at `39dce4b` (Phase 5 warm-up). The production `start()` + `process_from_handle()` topology now applies to `process_ac_009_events_lost.rs`. (Note: AC-009 remains Windows-only ignored on Linux CI; the warm-up applied the known-correct fix, exercised via the same elevated developer-local path.)

## ADRs at session close — Session 17 delta

One new ADR, two amendments:

- **ADR-0012** (normalize-before-correlate pipeline): **NEW — Accepted at `daf4625`**. Owns the concern ADR-0005 §Compliance flagged as "currently unowned." Defines: §4 renormalized scoring (active-source weights), §5 `dedup_key`, §6 Postgres `alerts` table, §7 read-model query, §8 correlation window (300s). Declares the transitory TS correlate seam honestly (production home is Go `services/pipeline/` per ADR-0002; extracted when the firehose ADR lands).
- **ADR-0002** (language/runtime): §Rule 3 amended — the MVP correlate slice lives transitorily in the TS ingest, not Go, with the extraction trigger named.
- **ADR-0003** (storage/retention): retention row amended — alerts are Postgres-only for the MVP (mutable triage state), superseding the prior "Postgres + ClickHouse" row for alerts. Honest co-located amendment per the amendment-audit discipline.

Two contradictions surfaced in the step-1 audit and **flagged-not-actioned** (deferred to their owners): ADR-0011 §4 ETW mapping flag (DEFER); the event_id v4-vs-alert.json-v7 mismatch (alert.json `source_events` relaxed to version-agnostic as a temporary MVP unblock at `34aa436`; the v4/v7 reconciliation is ADR-0009/ADR-0011 domain).

No other ADRs changed.

## SPECs at session close — Session 17 delta

- **SPEC-006** (Detection MVP): **NEW — Accepted at `91676e5`**. References ADR-0012 (no re-decide). One rule, six ACs, `§Operational` (incl. the documented already-running-Office production false-negative; the `process_command_line` / `subject_user_sid` empty-by-provider-limit note from the audit), `§Out of scope` (each deferral with a named destination).
- **SPEC-005**: unchanged in Session 17 except the AC-009 follow-up resolution (`39dce4b`).

AC validation status at session close:

| AC | Status | Gate |
|---|---|---|
| detect_ac_001 (marquee) | **Validated developer-local** | Real agent, ETW, elevated + Docker Desktop, 40171 ms |
| detect_ac_002 (false-positive suppression) | GREEN (ts-ci) | CI |
| detect_ac_003 (dedup → 1 alert) | GREEN (ts-ci) | CI |
| detect_ac_004 (status preservation) | GREEN (ts-ci) | CI |
| detect_ac_005 (watermark advance) | GREEN (ts-ci) | CI |
| detect_ac_006 (score renormalization) | GREEN (ts-ci) | CI |

## Ratified decisions recap (Phase 5)

The three ask-firsts ratified by Manuel and honoured by ADR-0012:

- **(a)** Event source = ClickHouse direct, NOT NATS (the firehose ADR gates on throughput evidence that does not exist yet).
- **(b)** `alerts` table = Postgres, NOT ClickHouse (mutable triage state).
- **(c)** correlate seam = transitory TS-ingest, NOT Go now (declared honestly in ADR-0012; Go toolchain NOT added to the Approved local toolchain).

Decide-and-communicate confirmed during implementation: renormalized scoring over active sources (ADR-0005 default weights); 5-minute dedup bucket from the **event** time (not `now()`); `dedup_key = agent_id::rule_id::process_name::bucket`; one alert per matching `(rule, event)`, no early-exit, dedup-collapsed; FK fix was production-faithful (`enrollTestAgent`), the `alerts.agent_id→agents` FK was NOT weakened.

## Known follow-ups

Co-located with their generating documents. New or changed in Session 17:

- **Already-running-Office production false-negative.** SPEC-006 §Operational §2: v0.1 does not enumerate pre-existing processes, so an Office instance already running before the ETW session opens is not captured as a parent → the rule misses it. A green marquee does NOT imply production coverage of this case. Destination: an initial-process-enumeration increment.
- **ML detection path / ML dedup_key format.** Out of scope for the MVP (SPEC-006 §Out of scope). `examples/05`'s `dedup_key` was intentionally left in the legacy host-based form because no realized ML dedup format exists yet to align it to. Destination: the ML/UEBA scoring SPEC.
- **`process_command_line` / `subject_user_sid` empty by provider limit.** The Kernel-Process provider declares neither field on any event version (audit-confirmed via the provider manifest). Rules must key on `process_name` / `image_file_name` / `process_parent_pid`. Destination: SPEC-006 §Operational (recorded); revisit if a command-line-bearing provider is added.
- **Transitory TS correlate seam → Go extraction.** ADR-0012 names the trigger (the firehose ADR). Until then the slice lives in `services/ingest/src/detect/`. Destination: the future event-firehose ADR + `services/pipeline/`.

Carried-forward follow-ups from prior sessions that remain open: the Phase 0 spike executable preservation decision (Session 16); the two Session 16 convention candidates pending ratification (release binary rebuild gate, cargo fmt gate); Session 13's dep-graph SPEC-node convention; Session 12's ajv-cli pinning note. None were actioned in Session 17.

## Invariants carried into Session 18

Binding for every session. Authoritative text in [CLAUDE.md](../CLAUDE.md) + the per-session procedural notes in [docs/engineering-notes.md](engineering-notes.md), whose [§Session 17](engineering-notes.md#session-17-2026-05-31) records Phase 5's four procedural notes (two documentary: ClickHouse SELECT-alias column shadow, IEEE-754 tail into a fixed-precision column; two convention candidates: integrity-constraint-failure → fix-the-test-not-the-schema, Known-CI-debt SHA co-locality). All invariants from prior handoffs carry forward unchanged.

New invariant codified into CLAUDE.md during Phase 5: the **harness-first red phases and debt co-locality** rule (the Known CI debt declaration must live in the same SHA that turns the workflow red; the impl commit that turns it green removes the row in that same SHA). This was the implicit lesson of Sessions 6–7, exercised cleanly in Phase 5 (debt opened at `24e0e4a`, closed at `34aa436`).

The nine Session 10 conventions + Session 11 Convention #5 extension + full-SHA polling operational bullet remain in force. **No new conventions ratified in Session 17.** The convention-candidate catalog now holds four entries pending Manuel's ratification: two from Session 16 (release binary rebuild gate, cargo fmt in the pre-commit gate) and two from Session 17 (integrity-constraint-failure → fix-the-test, debt SHA co-locality — see engineering-notes §Session 17).

## How Session 18 resumes

1. Read this document and prior handoffs ([docs/handoff-session-16.md](handoff-session-16.md) through [docs/handoff-session-10.md](handoff-session-10.md)) plus [CLAUDE.md](../CLAUDE.md), [ADR-0012](adr/0012-normalize-before-correlate-pipeline.md), and [SPEC-006](specs/SPEC-006-detection-mvp.md). They are the binding contract.

2. Confirm `main` is at the Session 17 anchor and all workflows are green. `git status` should show working tree clean (modulo `.claude/`). Known CI debt: zero rows.

3. Phase 6 scope is **not specified at this handoff**. Architect-Claude at Session 18 opening determines scope by reading the contracts + consulting Manuel. Candidate scope items the repo suggests (no pre-commitment, no ordering):
   - **Detection breadth**: more Sigma rules / event classes (network, registry, file); the already-running-Office enumeration increment; stateful multi-event correlation.
   - **Alert surfacing**: the incidents + grouping table, a WS dashboard, or an API to read alerts.
   - **Go correlate extraction** (when the firehose ADR lands): move the transitory TS seam to `services/pipeline/`.
   - **Packaging SPEC** (still deferred per ADR-0010 §Decision part 2): Windows Service registration, MSI installer, lifecycle.
   - **Convention ratification / spike preservation**: the carried-forward Session 16 items.

4. Architect-Claude's auto-memory directory carries narrative; this document plus prior handoffs plus the ADR/SPEC catalogs and engineering-notes carry the facts; the repo at `main` is the ultimate source of truth. Where any two disagree, re-verify against `main`.
