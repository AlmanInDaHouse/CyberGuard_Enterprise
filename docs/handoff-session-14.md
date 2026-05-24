# Handoff — End of Session 14

Canonical state-of-the-world at the close of Session 14. This document is the contract Session 15 resumes from. Session 14's scope was Phase 3.4 entirely — the harness RED phase that lands all nine SPEC-005 acceptance criteria as failing tests before any implementation. No decisions ratified, no SPECs status-flipped, no ADRs amended; only test code added. References to Sessions 10, 11, 12, and 13 supply the context this one does not replicate.

- **Anchor commit:** `b6e3445` (`docs(claude-md): substitute ts-ci debt SHA placeholder with 0c4d302 (Phase 3.4.F-B)`)
- **Branch:** `main`
- **Date:** 2026-05-23
- **CI verdict at anchor:** ALL GREEN
- **Known CI debt:** TWO rows — `rust-ci` declared on `733360c`, `ts-ci` declared on `0c4d302`. Both rows target Phase 3.5 implementation commit(s) for removal. First non-empty Known CI debt state in project history.
- **Working tree:** clean (only `.claude/` untracked, pre-existing local config)

## Phase 3.4 closed in full

Nine commits across one coordinated arc plus the handoff. First Phase in project history that closes with declared Known CI debt (rust-ci RED at the `main` HEAD prior to Phase 3.4.F-B; ts-ci RED at the `main` HEAD prior to Phase 3.4.F-B; both intentional per the harness-first invariant). Phase 3.4.F-B substituted the ts-ci debt SHA placeholder and brought `main` back to ALL-GREEN at the anchor, but the workflows for the prior commits remain RED-as-declared on their respective SHAs.

| Phase | SHA | Scope |
|---|---|---|
| 3.4.A | `733360c` | AC-003 (`process.uid` recipe pure unit) + AC-006 (`process.name` strict + log-and-drop synthetic injection) Rust tests landed; `testcontainers = "0.27"` added to `agent/cg-agent/` dev-dependencies; `rust-ci` Known CI debt declared in CLAUDE.md citing `<THIS_COMMIT_SHA>` placeholder. Self-referential amend pattern used for SHA substitution. |
| 3.4.A follow-up | `e10c971` | `cargo fmt --check` trip on three cosmetic diffs in verbatim test code; auto-fix follow-up commit `cargo fmt --all` on AC-003 + AC-006 test files. Pattern match to Session 12 markdown-lint follow-up `ee430c0 → 2f0c2ce`. |
| 3.4.B | `1a4bab2` | AC-002 (privilege clean-fail synthetic injection) + AC-005 (`process.exit_code` conditional emission with three pinned values 0/1/-1073741819 NTSTATUS) Rust tests landed; AC-006 struct literal extended with `exit_status: None` for cross-test compile consistency; CLAUDE.md `rust-ci` debt row's Declared-on-SHA corrected from `2dffe95` (the pre-amend transient SHA that the self-referential substitution pattern captured) to `733360c` (the actual on-main SHA of the original Phase 3.4.A commit). Self-referential amend pattern deprecated for future use per this correction. |
| 3.4.C | `31ff39a` | AC-004 (`process.created_time` integer-nanos + cache hit/miss) Rust tests landed; `tests/common/mod.rs` extended with `TestAgentHandle` + `start_test_agent` helpers (re-using existing `JoinHandle` and `Arc` in scope; inline `tokio::sync::oneshot::channel` to avoid module-level import changes); re-prescribed after import collision halt (the original prompt's verbatim block doubly-imported `Mutex` from `std::sync` and `tokio::sync` causing E0252, plus duplicate `Arc` and `Value` imports). The existing `common::MockServer` was reused directly for envelope capture instead of introducing a parallel `MockHeartbeatServer`. |
| 3.4.D | `27c45b9` | AC-007 (`parent_process` pid-only under PPID race; retry budget per NFR-005-005 = 3 attempts × 500 ms backoff) + AC-008 (`events_dropped_total` ring overflow with three pure-unit tests on synthetic injection via `EventRing::new_for_test(8)` bypassing NFR-005-002's production 65536 size) Rust tests landed. No `tests/common/mod.rs` changes — AC-007 reuses Phase 3.4.C scaffolding; AC-008 is pure-unit. |
| 3.4.E | `0236a57` | AC-009 (`events_lost` ETW pressure via side-channel helper per ADR-0008 §Decision part 2) Rust test landed; `ferrisetw = "1.2"` declared as target-conditional dev-dependency under `[target.'cfg(windows)'.dev-dependencies]`. First ferrisetw consumption in the repo. Test reproduces the spike's *principle* (pressure → loss → observable via helper), not the spike's *exact* buffer config (the spike's stated 1 KB × 2 buffers likely hit OS clamp at Win32 BufferSize minimum 4 KB; documented in the test file's preamble). |
| 3.4.F | `0c4d302` | **AC-001 polyglot marquee landed TypeScript-side** at `services/ingest/test/spec-005-marquee.test.ts` (vitest, reusing existing `prepareAgent` + `startIngest` + testcontainers scaffolding via globalSetup); `getCgesEvents` accessor + `CgesEventRow` interface added to `services/ingest/test/helpers/db.ts`; `ts-ci` Known CI debt declared in CLAUDE.md with placeholder `<TS_CI_DEBT_SHA_TO_SUBSTITUTE>`. Two-commit pattern adopted for SHA placeholder substitution (self-referential amend pattern deprecated per Phase 3.4.B correction). Re-prescribed after Config field drift halt: the original prompt's verbatim used non-existent field names (`INGEST_CLICKHOUSE_URL/USER/PASSWORD/DATABASE` + `INGEST_TLS_CERT_DIR` + URL construction from `INGEST_ENROLL_PORT/HEARTBEAT_PORT`); corrected to the actual `INGEST_CH_*` Zod-inferred fields + `server.enrollUrl/heartbeatUrl/caCertPem` direct properties per the canonical existing `ac-001-marquee.test.ts` pattern. Scope decision per the Phase 3.4.F-discovery sub-audit: AC-001 lives in `services/ingest/test/` (vitest-only), NOT in `agent/cg-agent/tests/process_ac_001_*.rs` — duplicating polyglot orchestration in Rust would add Docker build complexity without benefit. The Rust namespace `process_ac_NNN_*` carries AC-002 through AC-009 (agent-isolated ACs); AC-001 is polyglot orchestration and naturally lives at the vitest layer. |
| 3.4.F follow-up #1 | `5547dc4` | Biome lint trip on the new `spec-005-marquee.test.ts` (imports + format); auto-fix follow-up via `biome check --write` on the file. |
| 3.4.F follow-up #2 | `c949830` | Biome lint trip on the new `getCgesEvents` signature in `db.ts`; auto-fix follow-up via `biome check --write` on the file. |
| 3.4.F-B | `b6e3445` | **Phase 3.4 close.** CLAUDE.md `<TS_CI_DEBT_SHA_TO_SUBSTITUTE>` placeholder substituted with `0c4d302` (the SHA of Phase 3.4.F Commit A where the debt was originally declared; the two biome auto-fix follow-ups are cosmetic and do not shift the "declared on" semantics — pattern match to Phase 3.4.A's `733360c` being preserved across the `e10c971` fmt-fix follow-up). All-green at this anchor. |

CI workflows exercised in Session 14: `rust-ci` failed-as-declared at Phase 3.4.A through 3.4.E (six commits); `ts-ci` failed-as-declared at Phase 3.4.F + two biome follow-ups (three commits); `markdown-lint` ran on the CLAUDE.md-touching commits and stayed green; `schema-validation` skipped on every Session 14 commit. The Phase 3.4.F-B substitution commit landed ALL GREEN.

### Phase 3.4 RED status at anchor

The two RED workflows persist on `main` for the SHAs that introduced them:

- `rust-ci` RED on every `main` commit from `733360c` through the last Rust-affecting commit `0236a57` (Phase 3.4.E). Phase 3.4.F + follow-ups + 3.4.F-B did not change `agent/` paths, so `rust-ci` was skipped on those four commits and the RED state on `0236a57` is preserved as the last evaluated state on `main`.
- `ts-ci` RED on `0c4d302`, `5547dc4`, `c949830` (Phase 3.4.F + two biome follow-ups). Phase 3.4.F-B did not change `services/ingest/` paths, so `ts-ci` was skipped on `b6e3445` and the RED state on `c949830` is preserved as the last evaluated state on `main`.

Both REDs clear in Phase 3.5 — the implementation phase that lands the `cg_agent::etw` module + the `cges_events` ClickHouse DDL + the `services/ingest` envelope schema acceptance for `events[]`. The two Known CI debt rows are removed in the SHA(s) that flip each workflow back to success.

## ADRs at session close — Session 14 delta

**No ADR changes in Session 14.** Full catalog and dependency graph in [docs/adr/README.md](adr/README.md) unchanged from Session 13's recorded state. Session 14 was test-scaffolding only; no ADR text touched, no `Last updated:` fields advanced, no `## Amendment` events added.

## SPECs at session close — Session 14 delta

**No SPEC status changes in Session 14.** SPEC-005 stays `Accepted` (status flipped in Session 13 at `80ef2f2`); the nine ACs landed as RED test files in Session 14 are the *consumers* of SPEC-005 §AC, not changes to SPEC-005 itself. No SPEC text touched.

## D-decision status delta

**No D-decision changes in Session 14.** D2 + D7 closed in Session 13 (handoff Session 13 §D-decision status delta). D6 remains PARTIALLY DONE pending Phase 3.5's literal `cges_events` DDL commit. D1, D3a, D3b, D4, D5 unchanged.

Session 14 was test-scaffolding only — it consumed the decision surface settled by Sessions 10-13 (SPEC-005 + the four supporting ADRs) but did not extend or close any D-decisions. Phase 3.5 closes D6 when the DDL lands.

## Engineering-notes Session 14 additions

Authoritative text in [docs/engineering-notes.md](engineering-notes.md) under `## Session 14 (2026-05-23)`. Three procedural notes, none promoted to convention status:

1. **Self-referential amend pattern deprecated.** Phase 3.4.A used `git commit --amend --no-edit` to substitute the post-amend SHA into a CLAUDE.md placeholder reference; the substitution captured the pre-amend SHA (`2dffe95`) instead of the post-amend on-main SHA (`733360c`). Phase 3.4.B corrected the drift, and Phase 3.4.F adopted the **two-commit pattern** as the replacement: Commit A lands a placeholder; Commit B substitutes the placeholder with Commit A's actual on-main SHA. Standing pattern for SHA placeholders going forward. Pattern recorded in this session's engineering-notes; not promoted to Convention status (too narrow for cross-project applicability).

2. **Three halts in Phase 3.4 absorbed cleanly.** Phase 3.4.A (testcontainers version drift `0.23 → 0.27` after `cargo search` confirmation), Phase 3.4.C (`tests/common/mod.rs` import collision E0252 Mutex doubly imported), Phase 3.4.F (Config field drift `INGEST_CLICKHOUSE_*/INGEST_TLS_CERT_DIR → INGEST_CH_*/server.*Url`). All three halts followed the same shape: Claude Code surfaced the conflict pre-stage via the ask-user-input widget; architect-Claude requested a narrow read-only discovery sub-audit; re-prescription incorporated the verbatim signatures + field names. The common root cause across all three: prior discovery sub-audits reported file existence + function names but not verbatim signatures + field names. **Operational discipline for Phase 3.5+ (binding within CyberGuard, not promoted to Convention status):** when a prompt extends across a workspace boundary or touches shared scaffolding, the prior sub-audit must include verbatim type signatures + verbatim field names + verbatim invocation patterns from at least one existing consumer.

3. **Auto-fix follow-up pattern stable across formatters.** Phase 3.4.A established the pattern for `cargo fmt` follow-ups; Phase 3.4.F extended it to Biome (TypeScript). Both follow patterns: the formatter-fail trip happens post-push at the CI step; the fix is a separate follow-up commit (not an amend, not a force-push), the Known CI debt declaration's SHA reference does not shift across the formatter-fix follow-up. Three formatter-fix follow-ups landed in Session 14 (`e10c971`, `5547dc4`, `c949830`); each preserved its parent commit's RED-by-design exit point at the next CI gate (compile-fail E0432 for rust-ci; vitest runtime fail for ts-ci).

The nine Session 10 conventions + Session 11's Convention #5 extension + Session 11's full-SHA polling operational bullet + Session 12's two procedural validations + Session 13's two procedural notes remain authoritative and unchanged. No new conventions promoted in Session 14.

## Phase 3.5 opening protocol — Session 15

Phase 3.5 is the **implementation to GREEN** phase: lands the `cg_agent::etw` module + the `cges_events` ClickHouse DDL + the `services/ingest` envelope schema acceptance for `events[]`. First commits in project history to land actual security-telemetry implementation code (vs. test scaffolding in Session 14 or contract drafting in Sessions 10-13). Both Known CI debt rows are cleared in the SHA(s) that flip each workflow to GREEN.

**Implementation scope per the prescribed Phase 3.5 API surface (binding contract from Phase 3.4 tests):**

- `cg_agent::etw::format_process_uid(agent_id: &str, pid: u32, created_time_nanos: u64) -> String` (61-char nominal per AC-003; 58-67 character bounds per ADR-0011 §6)
- `cg_agent::etw::OpenError` enum with at least `PrivilegeNotHeld`, `AccessDenied` variants (AC-002)
- `cg_agent::etw::ActivityId` enum with `Launch = 1`, `Terminate = 2` (AC-005, AC-007)
- `cg_agent::etw::CapturedEvent` struct with 8 fields (pid, activity_id, image_file_name, parent_pid, command_line, subject_user_sid, etw_timestamp_nanos, exit_status: Option<i32>) (AC-004, AC-005, AC-006, AC-008)
- `cg_agent::etw::EventRing` with constructors `new(production_size)` and `new_for_test(size)`, methods `enqueue_or_drop(event)`, `events_dropped_total() -> u64`, `snapshot_events() -> Vec<CapturedEvent>` (AC-006, AC-008)
- `cg_agent::etw::CreatedTimeCache` with `new()` constructor and `consult_and_purge(pid: u32) -> Option<u64>` method (AC-004)
- `cg_agent::etw::EtwSession` type (placeholder reference exercised in AC-004 + AC-009)
- `cg_agent::etw::events_lost(session_name: &str) -> Result<u32, u32>` (AC-009; signature exactly per ADR-0008 §Decision part 2)
- `cg_agent::startup::handle_etw_open_result(Result<(), OpenError>) -> Result<(), StartupAbort>` with `StartupAbort { exit_code: i32, stderr_message: String }` (AC-002)
- `cg_agent::cges::emit_process_activity(&CapturedEvent) -> impl Serialize` (conditional `process.exit_code` serde-skip on None per AC-005)
- `cg_agent::cges::emit_process_activity_with_cache(&CapturedEvent, Option<u64>) -> impl Serialize` (`process.created_time` JSON null on None, integer nanos on Some per AC-004)
- A test-mode entry point invoked by `tests/common/start_test_agent` (currently `unimplemented!()` per Phase 3.4.C; Phase 3.5 implementation provides the entry)

**Implementation scope per the TypeScript side (binding contract from AC-001 marquee):**

- `services/ingest/src/schemas.ts` `OuterEnvelopeSchema/InnerEnvelopeSchema` extended to accept `events[]` carrying SPEC-005 wire shape per SPEC-003 amendment 2026-05-23 part (a)
- `services/ingest` heartbeat route handler extended with a `cges_events` persistence path (D6 closure — the literal `CREATE TABLE cges_events ...` DDL + the route-handler write code)
- `services/ingest/db/migrations/0002_cges_events.ts` (or equivalent migration file) carrying the ClickHouse DDL per ADR-0009's `ReplacingMergeTree(event_id)` + `(org_id, toYYYYMMDD(time))` partition + `(org_id, time, event_id)` order
- AC-001 marquee Linux-runner asymmetry: the SPEC-005 marquee currently fails on Linux runners at probe-spawn (cmd.exe ENOENT) rather than at the three documented RED layers; Phase 3.5 closure must address the runner architecture: either (a) configure `ts-ci` to use `windows-latest` for the SPEC-005 marquee path (likely via a workflow job matrix split), or (b) add a vitest `skipIf` gate to the SPEC-005 marquee paired with a separate Windows-only marquee job. Either approach lands in the same SHA that flips ts-ci to GREEN per the debt row's removal contract.

**Audit-first protocol applies at Phase 3.5 opening.** Phase 3.5 is implementation, not contract reconciliation, but it spans both workspaces (`agent/cg-agent/src/` for the Rust ETW implementation + `services/ingest/src/` for the schema + DDL + persistence path) and consumes the binding contract surface that Phase 3.4 tests prescribe. Architect-Claude's Phase 3.5 opening should include a discovery sub-audit narrower than Session 13's contract reconciliation but broader than Session 14's pure-discovery — covering at minimum: (i) any latent assumptions in the prescribed API surface that surfaced as test code but were not surfaced as ADR/SPEC text (these become Phase 3.5 implementation decisions that may need chat ratification); (ii) the actual scaffolding within `agent/cg-agent/src/` for module hierarchies, error types, and the entry-point function signature that `start_test_agent`'s `unimplemented!()` placeholder must conform to; (iii) the `services/ingest/src/db/` migration framework conventions for the new `cges_events` migration.

**Operational discipline carried into Phase 3.5 (per Session 14 procedural note 2):** discovery sub-audits must include verbatim signatures + field names + invocation patterns from existing consumers, not just file existence + function names. Three halts in Phase 3.4 establish the cost of skipping this; Phase 3.5 prompts pre-empt by audit-first.

**CI expectation when Phase 3.5 commits land:** the workflows that were RED-as-declared at Phase 3.4 close flip to success as the implementation lands. Order of clearance depends on implementation sequencing (architect-Claude's call at Phase 3.5 opening). Per the Known CI debt rows' removal contract, each row is removed in the same SHA that flips its workflow to success.

## Known follow-ups

Co-located with their generating documents per Convention 1. New in Session 14:

- **AC-001 marquee Linux-runner asymmetry.** The SPEC-005 marquee currently fails on Linux runners at probe-spawn (cmd.exe ENOENT) rather than at the three documented RED layers. Phase 3.5 closure must address the runner architecture per the two options enumerated in the Phase 3.5 opening protocol above. Not blocking for Phase 3.4 close; load-bearing for Phase 3.5 ts-ci GREEN.

- **Operational discipline: verbatim discovery sub-audits.** Three halts in Phase 3.4 surfaced the same root cause — prior discovery sub-audits reported file existence + function names but not verbatim signatures + field names. For Phase 3.5+ prompts, discovery sub-audits must include verbatim type signatures + verbatim field names + verbatim invocation patterns from at least one existing consumer before any verbatim test or implementation code is prescribed. Binding within CyberGuard; not promoted to Convention status.

All other follow-ups from prior sessions' handoff indexes remain in their existing states. Session 13's dep-graph SPEC-node convention follow-up remains open; Session 12's ajv-cli pinning note remains open.

## Invariants carried into Session 15

Binding for every session. Authoritative text in [CLAUDE.md](../CLAUDE.md) + Session-10 additions in [docs/engineering-notes.md](engineering-notes.md) §Session 10 + Session 11 additions in §Session 11 + Session 12 procedural validations in §Session 12 (documentary; not promoted to convention) + Session 13 procedural notes in §Session 13 (documentary; not promoted to convention) + Session 14 procedural notes in §Session 14 (documentary; not promoted to convention). All invariants from [docs/handoff-session-10.md](handoff-session-10.md) §Invariants and [docs/handoff-session-11.md](handoff-session-11.md) §Invariants and [docs/handoff-session-12.md](handoff-session-12.md) §Invariants and [docs/handoff-session-13.md](handoff-session-13.md) §Invariants carry forward unchanged. **No new invariants from Session 14.**

The nine Session 10 conventions + Session 11's Convention #5 extension + Session 11's full-SHA polling operational bullet remain in force as recorded.

## How Session 15 resumes

1. Read this document and [docs/handoff-session-13.md](handoff-session-13.md) and [docs/handoff-session-12.md](handoff-session-12.md) and [docs/handoff-session-11.md](handoff-session-11.md) and [docs/handoff-session-10.md](handoff-session-10.md) (for the canonical references this handoff does not replicate) and [CLAUDE.md](../CLAUDE.md). They are the binding contract.
2. Confirm `main` is at `b6e3445` and the `markdown-lint` workflow is green at the anchor before starting. `git status` should show working tree clean (modulo `.claude/`). Both Known CI debt workflows (`rust-ci`, `ts-ci`) are RED on prior `main` SHAs by design.
3. Open Phase 3.5 (implementation to GREEN). First action: read the prescribed Phase 3.5 API surface in §Phase 3.5 opening protocol above; architect-Claude conducts the discovery sub-audit (narrower than Session 13's contract reconciliation, broader than Session 14's pure-discovery) covering implementation decisions, scaffolding within `agent/cg-agent/src/`, and `services/ingest/src/db/` migration conventions. Per the operational discipline from Session 14 procedural note 2, the sub-audit surfaces verbatim signatures + field names + invocation patterns from existing consumers before any implementation code is prescribed.
4. The two Known CI debt rows are removed in the SHA(s) that flip each workflow to success. Order of clearance and granularity (one SHA per workflow vs. one SHA closing both vs. multi-commit closure per workflow) is architect-Claude's call at Phase 3.5 opening, informed by the implementation sub-audit.
5. Architect-Claude's auto-memory directory carries narrative; this document plus prior handoffs plus the ADR/SPEC catalogs and engineering-notes carry the facts; the repo at `main` is the ultimate source of truth. Where any two disagree, re-verify against `main`.
