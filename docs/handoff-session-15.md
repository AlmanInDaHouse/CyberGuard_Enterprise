# Handoff — End of Session 15

Canonical state-of-the-world at the close of Session 15. This document is the contract Session 16 resumes from. Session 15's scope was Phase 3.5 (implementation to GREEN) and the diagnostic + fix iterations within Phase 3.5.I that revealed an architectural gap in the ETW capture layer. Phase 3.5 is **partially delivered**: implementation work complete, end-to-end ETW capture validation on Windows deferred to Phase 4 per a chat-ratified scope boundary at iteration 5 of Phase 3.5.I. References to Sessions 10–14 supply the context this handoff does not replicate.

- **Anchor commit:** `390337a` (`fix(spec-005): park spawned thread to keep ETW trace tuple alive (Phase 3.5.I-FIX2)`)
- **Branch:** `main`
- **Date:** 2026-05-27
- **CI verdict at anchor:** ALL GREEN on `main` workflows. Developer-local SPEC-005 marquee on Windows RED at runtime (ETW dispatch callback does not fire; see §Phase 3.5.I — root cause analysis).
- **Known CI debt:** TWO rows. `rust-ci` declared on `733360c` — **eligible for removal but not removed this session** (compile target green at anchor; ETW-Windows-only AC-004-hit/AC-007/AC-009 tests still `cfg_attr(not(target_os = "windows"), ignore)` skipped on Linux runner; effective rust-ci result is GREEN on every Phase 3.5 commit). `ts-ci` declared on `0c4d302` — **stays declared**; reflects the developer-local marquee gap that Phase 4 is scoped to close.
- **Working tree:** clean (only `.claude/` untracked, pre-existing local config)

## Phase 3.5 substantive scope delivered

Implementation work delivered across the session, organized by commit arc:

### β arc — Agent ETW module + emission + startup wiring

| Commit | SHA | Scope |
|---|---|---|
| β1 | `fab79c1` | `cg_agent::etw` data types (`CapturedEvent` 9 fields, `ActivityId`, `OpenError`, `EventRing` with `new`/`new_for_test`/`enqueue_or_drop`/`events_dropped_total`/`snapshot_events`), `format_process_uid(agent_id, pid, created_time_nanos) -> String` per AC-003 + ADR-0011 §6. |
| β2 | `c8f4b87` | `CreatedTimeCache` (HashMap-backed, periodic sweep) per SPEC-005 §Operational 2. `cges::emit_process_activity` + `emit_process_activity_with_cache` (serde conditional-skip on None for `exit_code`; integer-nanos for `created_time`). `startup::handle_etw_open_result` returning `StartupAbort { exit_code: i32, stderr_message: String }` per AC-002. `EtwError` enum (`#[error(transparent)] Etw(#[from] EtwError)` arm on `AgentError`). Clippy `zombie_processes` allow on AC-009 test scaffolding. |
| β3 | `ba3917e` | Windows `EtwSession::open()` real impl via `ferrisetw::UserTrace + Provider::by_guid(KERNEL_PROCESS_GUID)` typestate builder; `events_lost(session_name) -> Result<u32, u32>` via raw `windows-sys` `ControlTraceW(EVENT_TRACE_CONTROL_QUERY)` per ADR-0008 §Decision part 2. `run_test_mode<F>` test entry point. AC-009 cfg fix. **rust-ci RED → GREEN at this commit.** Known CI debt row not removed at this SHA (preserved per phased closure invariant). |

### α arc — ts-ci-windows workflow (later removed via Path D)

| Commit | SHA | Scope |
|---|---|---|
| α main | `4082707` | New `.github/workflows/ts-ci-windows.yml` with `workflow_dispatch` trigger + marquee `skipIf` gate on Linux runner. |
| α biome fix | `f169734` | Biome auto-fix follow-up on `skipIf` wrap formatting. **ts-ci GREEN at this commit.** |

### γ arc — ClickHouse cges_events DDL

| Commit | SHA | Scope |
|---|---|---|
| γ main | `ae20d15` | `cges_events` DDL inline in `bootstrapClickHouse()` per Anomaly 2 of Phase 3.5.0 discovery. First failed: `ReplacingMergeTree(event_id)` rejected because `event_id` is UUID (not numeric version column required by ReplacingMergeTree). |
| γ fix | `d845c5c` | `ReplacingMergeTree(event_id)` → `ReplacingMergeTree(arrived_at)`; `event_id` stays in ORDER BY. **ts-ci GREEN.** D6 DDL fully landed. |

### ts-schema commit

| Commit | SHA | Scope |
|---|---|---|
| ts-schema | `3691f63` | Events[] wire shape across both workspaces. `agent/cg-agent/src/envelope.rs::HeartbeatEnvelope` gains `#[serde(default, skip_serializing_if = "Vec::is_empty")] pub events: Vec<CgesEvent>`. `services/ingest/src/schemas.ts::CgesProcessActivitySchema` + `InnerEnvelopeSchema.events: z.array(...).optional().default([])`. `tests/common::start_test_agent` implementation that the AC-004 test references. **rust-ci + ts-ci GREEN.** |

### ts-handler commit

| Commit | SHA | Scope |
|---|---|---|
| ts-handler | `4035c03` | `services/ingest/src/routes/heartbeat.ts` extended with `events[]` persistence path. `isConnectivityError` (previously dead code per Anomaly 3 of Phase 3.5.0 discovery) wired into the cges_events insert error handling. `nanosToChDateTime` helper added. **ts-ci GREEN — Phase 3.5 implementation work complete at this SHA.** |

### Phase 3.5.H — Path D ratified

After ts-handler GREEN, Manuel invoked the `ts-ci-windows.yml` workflow manually on `4035c03`. It FAILED at vitest globalSetup with "Could not find a working container runtime strategy" — testcontainers@10.28.0 cannot detect Docker runtime on hosted `windows-latest` GHA runners. Empirically falsified the assumption that testcontainers works identically on windows-latest as ubuntu-latest. Three paths considered + rejected: A (install Docker on windows-latest — fragile), B (self-hosted runner — exits MVP scope), C (restructure without testcontainers — material engineering), E (accept permanent RED — violates removal contract).

| Commit | SHA | Scope |
|---|---|---|
| Path D | `72432e4` | `.github/workflows/ts-ci-windows.yml` removed entirely. `CLAUDE.md` ts-ci Known CI debt row's Target column rewritten (Path D dual-gate resolution: Linux ts-ci stays GREEN via marquee skipIf; Windows marquee validates developer-local with Docker Desktop). New CLAUDE.md subsection "Developer-local SPEC-005 marquee validation" with procedure (`Docker Desktop` + `cd services/ingest && pnpm test`); `<THIS_COMMIT_SHA>` placeholder substituted with `38f1def` (Path D close SHA after lint follow-up). |
| Path D MD034 fix | `38f1def` | Bare URL → autolink syntax follow-up. |

### Phase 3.5.I — Diagnostic + fix iterations (5 rounds; closed at Option β chat ratification)

Phase 3.5.I opened when Manuel ran the developer-local marquee on Windows + Docker Desktop and received an unexpected failure at the persistence assertion. Five iterations followed, each surfacing a deeper failure layer than the previous:

| Iteration | SHA | Scope | What it revealed |
|---|---|---|---|
| 3.5.I-DIAG | `331fd63` | Three `console.info(JSON.stringify(...))` blocks in `spec-005-marquee.test.ts` surfacing `events_length`, `probe_pid`, `probe_pid_type`, `probe_events_length`, and a sample of first 3 events. | `events_length: 0` in BOTH non-elevated and elevated PowerShell. ETW privilege as root cause **ruled out**. |
| 3.5.I-DIAG2 | `4dd461c` | Three more diagnostic blocks: `marquee_agent_stderr_full`, `marquee_agent_stdout_full`, `marquee_prepare_agent_context`. | Agent stdout showed 40× `signed heartbeat sent` with sequence_number 1→40; zero ETW lifecycle logs. **Smoking gun: the cg-agent binary spawned by marquee-agent.ts was invoking `run_secure` (heartbeat-only path), not `run_test_mode` (β3's ETW capture path).** Phase 3.5.F implemented `run_test_mode` in `lib.rs` but `main.rs` (binary entry point) was never edited to route to it. |
| 3.5.I-FIX | `0c63d54` | Five files (+191/-64). New `EventRing::drain_events()` method (clears VecDeque vs snapshot which doesn't — β3 had latent N×duplication bug that would have surfaced at first events drained). Full `run_test_mode` rewrite to use `SecureSender` (mTLS), `build_envelope + seal_envelope` (signed outer envelope), `ring.drain_events()`, `config.server.heartbeat_target()` (two-port topology), identity actually used. `main.rs::CG_AGENT_TEST_MODE` env var dispatch as first-branch precedence. `marquee-agent.ts` `etwEnabled?: boolean` flag → `CG_AGENT_TEST_MODE=1` in child env. `spec-005-marquee.test.ts` passes `etwEnabled: true`. **Both rust-ci + ts-ci GREEN.** Chat-ratified Option 2 (refactor `run_test_mode` to mTLS) vs Option 1 (refactor `run_secure`) vs Option 3 (apply brief as-is and iterate). | |
| 3.5.I-FIX rerun | (same) | Manuel reran developer-local marquee. STILL RED with `events_length: 0`. Agent stdout NEW signature: `agent starting → test mode (CG_AGENT_TEST_MODE=1) → enrollment → identity persisted → test mode heartbeat loop entered → [SILENCE for 40s]`. main.rs dispatch confirmed working; run_test_mode entry confirmed; ETW capture not happening. | |
| 3.5.I-DIAG3 | `3cbe845` | Exhaustive INFO-level tracing in `etw/session.rs` (open entry/exit + trace.start invoked/result + per-dispatch callback fired with event_id), `etw/ring.rs::drain_events` (conditional log on `drained_count > 0 \|\| dropped_total > 0`), `lib.rs::run_test_mode` (per-tick "drain tick" with count; per-batch "events drained; building envelope"). **Material API discovery by Claude Code during clippy compile:** `ferrisetw::UserTrace::start()` returns `Result<(UserTrace, PROCESSTRACE_HANDLE), TraceError>` — NOT `Result<(), TraceError>` as β3 + AC-009 test code assumed via `let _ = trace.start()`. **β3 was dropping the trace + handle tuple immediately on Ok.** UserTrace's Drop impl terminates the ETW session, so Kernel-Process events stop flowing ~500µs after open, ring stays empty. DIAG3 preserved drop-on-Ok semantic per "no production logic change" constraint; surfaced the finding in the log message itself: `"trace.start completed Ok (tuple dropped per current β3 pattern)"`. | |
| 3.5.I-DIAG3 rerun | (same) | Manuel rerun confirmed Hypothesis B empirically: agent stdout showed `trace.start completed Ok (tuple dropped per current β3 pattern)` followed by exactly 40× `drain tick drained_count=0` and ZERO `dispatch callback fired` lines. | |
| 3.5.I-FIX2 | `390337a` | Single-file edit to `etw/session.rs`. Replaced underscore-prefixed `(_kept_trace, _kept_handle)` binds with named `(kept_trace, kept_handle)` + `std::thread::park()` immediately after the success log; trailing `let _ = kept_handle` (Copy type per clippy `dropping_copy_types` lint) + `drop(kept_trace)` as documented unreachable cleanup. **rust-ci GREEN; clippy clean; 14 unit tests pass.** | |
| 3.5.I-FIX2 rerun | (same) | **Hypothesis B falsified empirically.** Developer-local marquee STILL RED with identical signature: `trace.start completed Ok; parking thread to keep ETW session alive` (new log line confirms binary rebuilt; Manuel verified SHA `390337a` + `findstr "thread::park"` confirms source + `cargo build --release --bin cg-agent` completed 1m 14s) followed by 40× `drain tick drained_count=0` and ZERO `dispatch callback fired` lines. Park() prevents tuple drop confirmed; but ETW dispatch callback STILL does not fire even with session held alive. **Second-layer architectural issue underneath β3's tuple-drop bug.** | |

After 3.5.I-FIX2 falsification, chat-gate decision: continue iterating (α) vs defer ETW capture validation to Phase 4 (β) vs reduce marquee scope (γ). **Manuel ratified Option β.** Phase 3.5 closes partially delivered. Phase 4 opens with a dedicated ETW capture investigation sub-phase.

## Phase 3.5.I — root cause analysis at handoff close

The first-layer issue (β3's UserTrace tuple drop) is **identified, fixed, and validated** at `390337a`. The fix landed correctly; agent binary rebuilt correctly; new log line `trace.start completed Ok; parking thread to keep ETW session alive` appears as expected.

The second-layer issue **remains diagnosed but not resolved**. After park() keeps the trace tuple alive, ETW Kernel-Process events still do not reach the dispatch callback. Three candidate hypotheses (no chat ratification on which is correct — Phase 4 investigation will determine):

1. **ferrisetw `UserTrace` API may not be the correct entry point for Microsoft-Windows-Kernel-Process.** Kernel-Process is a kernel-mode trace provider. ferrisetw may expose a separate `KernelTrace` API (or equivalent) that requires distinct construction. The Phase 0 spike at `docs/spikes/2026-05-23-etw-process-events.md` documented 7649 lost events under pressure — so the spike DID receive events. **Phase 4 step 1: read spike notes verbatim to identify the exact ferrisetw API path used by the spike code.**

2. **Win32 `StartTraceW` may return success without subscribing if `SeSystemProfilePrivilege` is not held by the calling token.** Manuel ran the marquee in both standard and elevated PowerShell sessions; both produced identical empty-ring behavior. Elevated PowerShell may surface a different token boundary than the spawned cg-agent subprocess's token (UAC token boundaries). Phase 4 step 2: investigate token-level privilege check at trace.start() time; possibly engage ADR-0010 §Decision part 3's SYSTEM trampoline fallback.

3. **Kernel-Process provider GUID may require additional enable flags (`EVENT_TRACE_FLAG_PROCESS`) that ferrisetw's `Provider::by_guid()` does not set by default.** Microsoft-Windows-Kernel-Process is technically a kernel logger; subscribing via the generic provider GUID may not enable per-process Launch/Terminate events. Phase 4 step 3: verify the provider GUID + flags combination against Microsoft docs and the Phase 0 spike code.

**Confidence ranking at handoff close** (architect-Claude's best guess; Phase 4 audit-first sub-audit may reorder):

- 50% Hypothesis 1 (KernelTrace API distinction)
- 35% Hypothesis 3 (provider enable flags missing)
- 15% Hypothesis 2 (privilege issue surfacing differently than DIAG ruling-out suggested)

## Diagnostic logging surface at handoff close

The following diagnostic logging remains in source and is **load-bearing for Phase 4 investigation**. It should not be removed until Phase 4 closure validates end-to-end ETW capture:

**`services/ingest/test/spec-005-marquee.test.ts` (added at Phase 3.5.I-DIAG `331fd63` + DIAG2 `4dd461c`):**

- 6 `console.info(JSON.stringify(...))` blocks: `marquee_pre_filter_events_count`, `marquee_pre_filter_first_three_events` (conditional), `marquee_post_filter_probe_events_count`, `marquee_agent_stderr_full`, `marquee_agent_stdout_full`, `marquee_prepare_agent_context`.

**`agent/cg-agent/src/etw/session.rs` (added at Phase 3.5.I-DIAG3 `3cbe845`):**

- INFO logs at `EtwSession::open` entry/exit.
- INFO logs at `trace.start invoked on spawned thread` + per-arm `trace.start completed Ok / failed`.
- INFO log per-dispatch `dispatch callback fired` with `event_id` field.

**`agent/cg-agent/src/etw/ring.rs::drain_events` (added at Phase 3.5.I-DIAG3 `3cbe845`):**

- Conditional INFO log when `drained_count > 0 || dropped_total > 0`.

**`agent/cg-agent/src/lib.rs::run_test_mode` (added at Phase 3.5.I-DIAG3 `3cbe845`):**

- Per-tick INFO log `drain tick` with `drained_count`.
- Per-batch INFO log `events drained; building envelope` with `sequence_number` + `event_count`.

Cleanup of these logs is a Phase 4 closure follow-up. Premature removal would lose the diagnostic surface that next-iteration investigation depends on.

## ADRs at session close — Session 15 delta

**No ADR changes in Session 15.** ADR catalog stable from Session 14's recorded state. ADR-0008 (ETW crate selection) + ADR-0009 (event delivery + buffer) + ADR-0010 (agent privilege model) + ADR-0011 (CGES Process Activity) all unchanged. Phase 4 may amend ADR-0008 or ADR-0010 depending on the ETW capture investigation's findings (e.g., if KernelTrace vs UserTrace distinction requires capturing a decision in ADR-0008; if SYSTEM trampoline becomes load-bearing, ADR-0010 §Decision part 3 amends accordingly).

## SPECs at session close — Session 15 delta

SPEC-005 stays `Accepted` (status flipped Session 13 at `80ef2f2`). The 9 ACs landed as RED test files in Session 14 are now in mixed state:

- **AC-002, AC-003, AC-005, AC-006, AC-008**: GREEN (cross-platform tests, pass on rust-ci ubuntu-latest runner; logic exercises agent-side implementation via Phase 3.5 β arc).
- **AC-004 (cache-miss), AC-007**: GREEN (synthetic injection paths via `tests/common::start_test_agent`).
- **AC-004 (cache-hit), AC-009**: Windows-only ignored on Linux runner (`cfg_attr(not(target_os = "windows"), ignore)`). Not validated end-to-end in Phase 3.5.
- **AC-001 (polyglot marquee)**: RED on developer-local Windows + Docker Desktop. Skipped on Linux ts-ci runner. Phase 4 closes.

No SPEC text touched in Session 15.

## D-decision status delta

| D | Topic | Session 14 status | Session 15 status |
|---|---|---|---|
| D6 | ClickHouse `cges_events` DDL | PARTIALLY DONE | **DONE** as of `4035c03` (ts-handler commit). DDL exists per γ arc; write path populates per ts-handler; schema validates per ts-schema. The literal `CREATE TABLE cges_events ...` lives inline in `bootstrapClickHouse()` per Anomaly 2 chat decision (no per-file migration folder; stays inline). |

D1, D2, D3a, D3b, D4, D5, D7 — unchanged from prior sessions. D2 + D7 closed in Session 13.

## Phase 3.5 partial-delivery declaration

Phase 3.5 closes with **substantive scope delivered but end-to-end marquee validation deferred**. The boundary is honest:

**Delivered:**

- 9 SPEC-005 ACs landed as test code (8 Rust + 1 TypeScript).
- `cg_agent::etw` module complete (data types, ring, cache, session, events_lost helper, format_process_uid).
- `cg_agent::cges` emission with conditional fields per AC-004/005.
- `cg_agent::startup::handle_etw_open_result` per AC-002.
- `cg_agent::run_test_mode` with full production-shaped mTLS + signed envelope transport.
- `agent/cg-agent/src/main.rs` env var dispatch (CG_AGENT_TEST_MODE).
- `services/ingest` schema acceptance for events[].
- `services/ingest` heartbeat handler events[] persistence path.
- `cges_events` ClickHouse DDL.
- `tests/common::start_test_agent` implementation for Rust integration tests.
- `marquee-agent.ts` `etwEnabled` flag.
- ferrisetw 1.2 UserTrace::start() API discovery + tuple-keep-alive fix (Phase 3.5.I-FIX2 via std::thread::park()).

**Not delivered (deferred to Phase 4):**

- End-to-end ETW Kernel-Process dispatch callback firing on Windows. The second-layer issue surfaced post-FIX2 (park() prevents tuple drop but dispatch still doesn't fire) is documented under §Phase 3.5.I — root cause analysis with three candidate hypotheses for Phase 4 investigation.
- Marquee 8/8 GREEN on developer-local Windows + Docker Desktop.
- Phase 4 closure of the ts-ci Known CI debt row.
- Phase 4 closure (cleanup) of the diagnostic logging surface.

## Engineering-notes Session 15 additions

Authoritative text in [docs/engineering-notes.md](engineering-notes.md) under `## Session 15 (2026-05-27)`. Four procedural notes; none promoted to convention status:

1. **Halt-and-re-prescribe pattern absorbed 6+ halts cleanly in Session 15.** Each halt followed the same shape: Claude Code surfaced the conflict pre-stage via the ask-user-input widget or compile error; architect-Claude requested a narrow read-only discovery sub-audit; re-prescription incorporated the verbatim signatures. Examples: testcontainers 0.23→0.27, common/mod.rs E0252 collision (carried forward from Session 14), Config field drift INGEST_CLICKHOUSE_*→INGEST_CH_*, ReplacingMergeTree(event_id) UUID rejection→arrived_at fix, ferrisetw UserTrace::start tuple return type discovery, clippy dropping_copy_types lint on Copy-typed PROCESSTRACE_HANDLE. The discipline scales.

2. **Two-commit pattern for SHA placeholders empirically validated across Phase 3.5.** Path D (`72432e4`) substituted `<THIS_COMMIT_SHA>` to `38f1def` via the standing two-commit pattern. Self-referential amend pattern stays deprecated.

3. **Auto-fix follow-up pattern stable across five formatter trips in Phase 3.5.** rustfmt (β arc), biome (×3 — α, ts-handler, DIAG2), markdown-lint MD034 (Path D). All applied as separate follow-up commits without amending parent SHAs. Known CI debt declarations' Declared-on SHAs preserved across formatter fixes.

4. **ETW capture validation requires architectural investigation beyond Phase 3.5 scope.** Five iterations within Phase 3.5.I (DIAG → DIAG2 → FIX → DIAG3 → FIX2) progressively narrowed the failure mode but did not close it. The first-layer issue (β3's UserTrace tuple drop) was correctly identified and fixed at `390337a`. The second-layer issue (dispatch callback does not fire even with session held alive) emerged post-fix and indicates a deeper API or privilege architectural question. Phase 4 opens with a dedicated ETW capture investigation sub-phase. Documentary — not promoted to convention status (project-specific; not cross-applicable).

The Session 14 procedural notes remain in force (verbatim discovery sub-audits, auto-fix follow-up pattern, two-commit SHA substitution). The Session 10 conventions + Session 11 extensions + Session 12/13/14 procedural notes all stay authoritative.

## Phase 4 opening protocol — Session 16

Phase 4's **first sub-phase** is the ETW capture investigation. The audit-first protocol applies; sub-audit scope:

1. **Read `docs/spikes/2026-05-23-etw-process-events.md` verbatim.** Identify the exact ferrisetw API path the spike used to subscribe to Microsoft-Windows-Kernel-Process. The spike empirically achieved 7649 lost events under pressure — meaning events DID reach the spike's callback. Compare to the current `cg_agent::etw::session.rs` invocation pattern.

2. **Investigate ferrisetw 1.2's KernelTrace vs UserTrace API distinction.** Read crate documentation, the source if needed. Determine whether Kernel-Process requires `KernelTrace` (or equivalent) construction instead of `UserTrace`.

3. **Investigate Win32 `StartTraceW` privilege boundaries.** SeSystemProfilePrivilege, SeDebugPrivilege, token-level checks at trace.start() time. Reference ADR-0010 §Decision part 3's SYSTEM trampoline as the documented fallback if user-token elevation proves insufficient.

4. **Surface architectural decisions for chat ratification.** The investigation may reveal: (a) the fix is a single-line API change; (b) the fix requires a structural refactor of `EtwSession`; (c) the fix requires invoking the SYSTEM trampoline path (which itself is non-trivial: service registration, IPC channel, restarted agent inside SYSTEM context). Each path has different scope; chat-gate before implementing.

5. **Drive AC-001 marquee to GREEN.** After the fix lands, Manuel reruns developer-local marquee. Validate 8/8 tests pass. Remove the ts-ci Known CI debt row in the same commit that flips the marquee to validated-GREEN.

6. **Cleanup of diagnostic logging surface** (Phase 4 closure follow-up commits). Remove the 6 console.info blocks in `spec-005-marquee.test.ts` + the INFO logs in `session.rs`/`ring.rs`/`lib.rs::run_test_mode`. Reduce noise to permanent-observability-level only.

**Phase 4 scope beyond ETW capture validation** is not specified at this handoff. Architect-Claude at Phase 4 opening determines whether additional Phase 4 sub-phases land in Session 16 (likely scope: implementation of additional SPEC-005 follow-up items, or SPEC-006 if scoped, or D-decision closure for any remaining D's) — but the ETW capture investigation is the load-bearing first sub-phase.

## Known follow-ups

Co-located with their generating documents per Convention 1. New in Session 15:

- **AC-009 test code `let _ = trace.start()` pattern.** The Phase 3.4.E AC-009 test (`agent/cg-agent/tests/process_ac_009_events_lost.rs`) still uses the underscore-prefix tuple-drop pattern that β3 also used. The pattern is identical to the bug fixed at Phase 3.5.I-FIX2 (`390337a`). AC-009 is Windows-only ignored on Linux, so the bug never surfaces in CI. **If Phase 4 exercises AC-009 end-to-end on Windows, the same park-based fix applies.** Out of scope for Session 15 close; flagged for Phase 4.

- **Diagnostic logging cleanup.** See §Diagnostic logging surface above. Six console.info blocks in spec-005-marquee.test.ts; INFO logs across session.rs/ring.rs/lib.rs::run_test_mode. Removal is a Phase 4 closure follow-up; premature removal loses investigative surface.

- **ETW capture validation on Windows.** The main follow-up. See §Phase 4 opening protocol for the investigation scope.

- **rust-ci Known CI debt row eligible for removal.** At anchor `390337a`, rust-ci is functionally GREEN (all cross-platform tests pass; Windows-only tests skip on Linux runner without failing). Per the row's removal contract, it can be removed in any SHA on Phase 4. **Recommended:** remove in the same SHA that closes ETW capture investigation (consolidates "rust-ci concerns settled" semantically).

All other follow-ups from prior sessions remain in their existing states.

## Invariants carried into Session 16

Binding for every session. Authoritative text in [CLAUDE.md](../CLAUDE.md) + Session-10 additions in [docs/engineering-notes.md](engineering-notes.md) §Session 10 + Session 11 additions in §Session 11 + Session 12 procedural validations in §Session 12 + Session 13 procedural notes in §Session 13 + Session 14 procedural notes in §Session 14 + Session 15 procedural notes in §Session 15 (all documentary; none promoted to convention). All invariants from prior handoffs carry forward unchanged. **No new invariants from Session 15.**

The nine Session 10 conventions + Session 11 Convention #5 extension + Session 11 full-SHA polling operational bullet remain in force as recorded.

## How Session 16 resumes

1. Read this document and [docs/handoff-session-14.md](handoff-session-14.md) and [docs/handoff-session-13.md](handoff-session-13.md) and [docs/handoff-session-12.md](handoff-session-12.md) and [docs/handoff-session-11.md](handoff-session-11.md) and [docs/handoff-session-10.md](handoff-session-10.md) (for the canonical references this handoff does not replicate) and [CLAUDE.md](../CLAUDE.md). They are the binding contract.

2. Confirm `main` is at `390337a` and the `markdown-lint` workflow is green at the anchor before starting. `git status` should show working tree clean (modulo `.claude/`). Both Known CI debt workflows (`rust-ci`, `ts-ci`) are GREEN on `main` at anchor; the underlying gap is developer-local Windows marquee not validated end-to-end (per §Phase 3.5 partial-delivery declaration).

3. Open Phase 4 with the ETW capture investigation sub-phase per §Phase 4 opening protocol. First action: architect-Claude reads `docs/spikes/2026-05-23-etw-process-events.md` (request Manuel paste the raw URL for `web_fetch` per Session 13 procedural note 2's friction-minimum pattern). Compare spike's ferrisetw API path against the current `cg_agent::etw::session.rs` invocation. Investigation drives a chat-gated decision on fix shape (API change vs structural refactor vs SYSTEM trampoline). Implementation follows ratification.

4. The two Known CI debt rows close in Phase 4: ts-ci on the SHA that validates marquee GREEN end-to-end; rust-ci on the same SHA or a separate consolidation SHA (architect-Claude's call).

5. Architect-Claude's auto-memory directory carries narrative; this document plus prior handoffs plus the ADR/SPEC catalogs and engineering-notes carry the facts; the repo at `main` is the ultimate source of truth. Where any two disagree, re-verify against `main`.
