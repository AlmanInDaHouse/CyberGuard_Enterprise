# Handoff — End of Session 16

Canonical state-of-the-world at the close of Session 16. This document is the contract Session 17 resumes from. Session 16's scope was Phase 4 (ETW capture investigation + end-to-end marquee validation). Phase 4 is **delivered in full**: the SPEC-005 AC-001 marquee is 8/8 GREEN developer-local, Known CI debt is at zero rows, and all diagnostic logging has been reduced to permanent observability level. References to Sessions 10–15 supply the context this handoff does not replicate.

- **Anchor commit:** `77a340e` (`docs(handoff): Session 16 close`)
- **Branch:** `main`
- **Date:** 2026-05-29
- **CI verdict at anchor:** ALL GREEN on `main` workflows. Developer-local SPEC-005 marquee 8/8 GREEN (two consecutive runs, zombie reclaim validated, elevated PowerShell per ADR-0010 §Decision part 1).
- **Known CI debt:** ZERO rows. `rust-ci` row was removed in a prior session (eligible for removal since Session 15). `ts-ci` row removed at `c020122` (Phase 4 closure commit) via Fallback 2 per ADR-0010 §Decision part 3 Amendment 2026-05-29: marquee validated developer-local, not in CI.
- **Working tree:** clean (only `.claude/` untracked, pre-existing local config)

## Phase 4 commit arc

| Commit | SHA | Scope |
|---|---|---|
| ETW capture chain | `70d8e3e` | Five fixes: `WINEVENT_KEYWORD_PROCESS` keyword, `start()` + `process_from_handle()` pump topology, `reclaim_zombie()`, `ImageName` property name, `ExitCode` property name. |
| String-nanos wire | `459e040` | `time` and `created_time` changed from JSON integer to JSON string on the signed envelope wire. BigInt-based `stringNanosToChDateTime` on ingest side. Zod schemas, JSON Schemas, examples, AC-004 test updated. ADR-0011 Amendment 2026-05-28 (wire encoding). |
| Stable process.uid | `fc8f170` | `format_process_uid` uses `cached_created_time` (not `event.etw_timestamp_nanos`) for Terminate events. ADR-0011 §6 and SPEC-005 §Operational §2 amended (false ETW EventRecord premise corrected). |
| fmt follow-up | `3581b60` | `cargo fmt` auto-fix on `events_lost_impl.rs` (follow-up to `70d8e3e`). |
| Phase 4 closure | `c020122` | `ts-ci` Known CI debt row removed. ADR-0010 §Decision part 3 Amendment 2026-05-29 (Fallback 2). SPEC-005 §In scope line 29 amended. CLAUDE.md §Developer-local marquee validation updated. |
| Logging cleanup | `305295e` | Diagnostic logging reduced to permanent observability: per-event `dispatch callback fired` INFO → TRACE; two lifecycle logs INFO → DEBUG; `drain tick` removed; 4 test console.info blocks removed. |
| Engineering notes | `4d63e68` | Session 16 procedural notes: release binary gotcha, cargo fmt gate, eight-layer diagnostic archaeology, gate-before-marquee pattern. |

## Phase 4 delivery declaration

Phase 4 is **delivered in full**. Contrast with Session 15's partial delivery (implementation complete, end-to-end validation deferred):

**Delivered:**

- ETW Kernel-Process capture chain end-to-end: agent opens session → ProcessTrace pump runs on dedicated thread → dispatch callback fires for ProcessStart/ProcessStop events → events enqueued to ring → drained → CGES Process Activity emitted with string-encoded nanos → signed envelope → mTLS POST → ingest validates signature → persists to ClickHouse `cges_events`.
- AC-001 (polyglot marquee): 8/8 GREEN developer-local. Two consecutive runs with zombie reclaim validated. Validated from elevated PowerShell with Docker Desktop running per CLAUDE.md §Developer-local SPEC-005 marquee validation.
- AC-004 (cache-hit): validated end-to-end. `process.uid` stable across Launch/Terminate via `CreatedTimeCache`. `process.created_time` matches byte-for-byte between Launch and Terminate rows.
- Known CI debt: zero rows. Both historical rows closed.
- Diagnostic logging reduced to permanent observability level.
- Engineering notes recorded (four procedural notes, two convention candidates).

**Session 15 follow-ups resolved:**

- **ETW capture validation on Windows:** DONE — the main Phase 4 deliverable.
- **Diagnostic logging cleanup:** DONE at `305295e`.
- **rust-ci Known CI debt row:** was already removed in a prior session.
- **AC-009 `let _ = trace.start()` pattern:** NOT resolved. AC-009 (`process_ac_009_events_lost.rs:91`) still uses the tuple-drop pattern. The fix (`start()` + `process_from_handle()`) would apply, but AC-009 is Windows-only ignored on Linux CI and was not exercised end-to-end in Phase 4. Remains an open follow-up.

## ADRs at session close — Session 16 delta

Two ADRs amended:

- **ADR-0010** (agent privilege model): §Decision part 3 Amendment 2026-05-29. CI assumption resolved via Fallback path 2 (marquee moved out of CI). Dual constraint: Docker runtime unavailable on `windows-latest` hosted runners + `runneradmin` ETW privilege untested. The privilege assumption remains untested in CI — recorded honestly.
- **ADR-0011** (CGES Process Activity v0.1): Two amendments in Session 16:
  - **Amendment 2026-05-28 (wire encoding):** `process.created_time` and event `time` changed from integer to string-encoded nanos on the signed envelope wire. Supersedes part (a) of the 2026-05-23 amendment where they differ. Cause: IEEE 754 double-precision loss in `JSON.parse` on u64 values > 2^53.
  - **Amendment 2026-05-28 (§6 Cross-event stability):** the original text assumed both ETW Launch and Terminate EventRecords carry the process creation timestamp. Empirically false — the Terminate EventRecord carries the termination timestamp. Cross-event uid stability is achieved via `CreatedTimeCache`, not by ETW EventRecord assumption.

No other ADRs changed. ADR-0008, ADR-0009 unchanged.

## SPECs at session close — Session 16 delta

SPEC-005 stays `Accepted`. Three text amendments landed:

- **§Operational §2, first bullet:** amended — cache IS load-bearing for `process.uid` on Terminate events (original text incorrectly stated it is not).
- **§In scope, line 29:** same false premise corrected (second instance).
- **No AC text changes.** The ACs are normative contracts; their validation status lives in CLAUDE.md and this handoff.

AC validation status at session close:

| AC | Status | Gate |
|---|---|---|
| AC-001 (marquee) | **Validated developer-local** | 8/8 GREEN, elevated + Docker Desktop, Fallback 2 |
| AC-002 (privilege) | GREEN (rust-ci, synthetic) | CI |
| AC-003 (uid recipe) | GREEN (rust-ci, pure unit) | CI |
| AC-004 (cache-hit) | **Validated developer-local** | Elevated + Docker Desktop |
| AC-004 (cache-miss) | GREEN (rust-ci, synthetic) | CI |
| AC-005 (exit_code) | GREEN (rust-ci, cross-platform) | CI |
| AC-006 (process.name) | GREEN (rust-ci, synthetic) | CI |
| AC-007 (parent_pid) | GREEN (rust-ci, synthetic) | CI |
| AC-008 (ring overflow) | GREEN (rust-ci, pure unit) | CI |
| AC-009 (events_lost) | Windows-only ignored on Linux | **Not validated end-to-end** |

## D-decision status delta

No D-decision changes in Session 16. D1–D7 unchanged from Session 15. D2 + D7 closed in Session 13. D6 closed in Session 15 (`4035c03`). D1, D3a, D3b, D4, D5 unchanged.

D7 (45s budget): the marquee's `spec_005_marquee_complete` log with `marquee_elapsed_seconds` continues to validate the budget on each developer-local run. The D7 assertion is embedded in the AC-001 test code.

## Known follow-ups

Co-located with their generating documents per Convention 1. New or changed in Session 16:

- **AC-009 end-to-end on Windows.** `process_ac_009_events_lost.rs:91` still uses `let _ = trace.start()` (the tuple-drop pattern fixed in the production code at `70d8e3e`). The fix (`start()` + `process_from_handle()`) applies identically. AC-009 is Windows-only ignored on Linux CI. Not exercised end-to-end in Phase 4 — deferred to a future session that exercises Windows-only Rust tests locally or in CI. Low risk: the test creates its own standalone UserTrace session independent of the agent's ETW path.

- **Phase 0 spike executable preservation.** The spike executable at `c:\tmp\etw-spike\` (local, ephemeral) was the empirical source of truth for all eight Phase 4 failure layers. Its API usage is documented in [docs/spikes/2026-05-23-etw-process-events.md](spikes/2026-05-23-etw-process-events.md) (permanent, in-repo). Decision pending: whether to commit the spike source under `docs/spikes/` or a `tools/` directory for future reference, or accept the documentation as sufficient. Manuel's call.

- **Convention candidates from engineering-notes Session 16.** Two procedural notes (release binary rebuild gate, cargo fmt in pre-commit gate) are marked as convention candidates in [docs/engineering-notes.md](engineering-notes.md) §Session 16. Pending Manuel's ratification — not promoted until explicitly approved.

All other follow-ups from prior sessions remain in their existing states. Session 13's dep-graph SPEC-node convention follow-up remains open. Session 12's ajv-cli pinning note remains open.

## Invariants carried into Session 17

Binding for every session. Authoritative text in [CLAUDE.md](../CLAUDE.md) + Session-10 additions in [docs/engineering-notes.md](engineering-notes.md) §Session 10 + Session 11 additions in §Session 11 + Session 12 procedural validations in §Session 12 + Session 13 procedural notes in §Session 13 + Session 14 procedural notes in §Session 14 + Session 15 procedural notes in §Session 15 + Session 16 procedural notes in §Session 16 (two convention candidates pending ratification; two documentary). All invariants from prior handoffs carry forward unchanged.

The nine Session 10 conventions + Session 11 Convention #5 extension + Session 11 full-SHA polling operational bullet remain in force as recorded. **No new conventions from Session 16** — two candidates (release binary rebuild gate, cargo fmt gate) are pending Manuel's ratification per engineering-notes §Session 16.

## How Session 17 resumes

1. Read this document and prior handoffs ([docs/handoff-session-15.md](handoff-session-15.md) through [docs/handoff-session-10.md](handoff-session-10.md)) for the canonical references this handoff does not replicate, and [CLAUDE.md](../CLAUDE.md). They are the binding contract.

2. Confirm `main` is at `77a340e` and all workflows are green. `git status` should show working tree clean (modulo `.claude/`). Known CI debt: zero rows.

3. Phase 5 scope is **not specified at this handoff**. Architect-Claude at Session 17 opening determines scope by reading the contracts + consulting Manuel. Candidate scope items the repo suggests (no pre-commitment, no ordering):
   - **Packaging SPEC** (per ADR-0010 §Decision part 2 deferral): Windows Service registration, MSI installer, service lifecycle, auto-restart, log rotation / Event Log integration. The closest bottleneck to "deployable outside a developer machine."
   - **SPEC-005 remaining follow-ups**: AC-009 end-to-end on Windows; spike executable preservation decision.
   - **SPEC-006** (if scoped by Manuel): next telemetry SPEC or next product feature.
   - **D-decision closure**: any remaining open D-decisions from prior sessions.
   - **Convention ratification**: the two Session 16 candidates from engineering-notes.

4. Architect-Claude's auto-memory directory carries narrative; this document plus prior handoffs plus the ADR/SPEC catalogs and engineering-notes carry the facts; the repo at `main` is the ultimate source of truth. Where any two disagree, re-verify against `main`.
