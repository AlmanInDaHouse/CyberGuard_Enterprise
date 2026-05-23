# Handoff — End of Session 13

Canonical state-of-the-world at the close of Session 13. This document is the contract Session 14 resumes from. Session 13's scope was Phase 3.3 only — the SPEC-005 full draft across a ten-commit arc — so this handoff is proportionally similar in size to Session 12's. References to Sessions 10, 11, and 12 supply the context this one does not replicate.

- **Anchor commit:** `80ef2f2` (`docs(spec,adr,engineering-notes): ratify SPEC-005 (Phase 3.3.J)`)
- **Branch:** `main`
- **Date:** 2026-05-23
- **CI verdict at anchor:** ALL GREEN
- **Known CI debt:** none
- **Working tree:** clean (only `.claude/` untracked, pre-existing local config)

## Correction of record — Session 12 anchor SHA

Session 12's handoff document declares its anchor as `59db12f` (the Phase 3.2.B commit immediately preceding the handoff itself). The handoff was written pre-lint-fix; the formal Session 12 close at `ee430c0` tripped MD004 (continuation `+` in line wrap), which Claude Code auto-fixed in follow-up commit `2f0c2ce`. Substance of the Session 12 handoff is intact. The actual Session 12 close is `2f0c2ce`, and Session 13 resumed from `2f0c2ce` accordingly. Full record in [docs/engineering-notes.md](engineering-notes.md) §Session 13 procedural note 1; this is the handoff-level pointer.

## Phase 3.3 closed in full

Ten commits in one coordinated arc, all green on first push, no force-pushes, no amendments-of-amendments. Phase 3.3.A through 3.3.I delivered SPEC-005 incrementally as `Proposed`; Phase 3.3.J ratified at `Accepted` in one atomic commit per the Session 10 cascade pattern (ADR-0009 + ADR-0004 co-located), with co-located SPEC-001 amendment 2026-05-23 (sequence_number semantics under multi-POST).

| Phase | SHA | Scope |
|---|---|---|
| 3.3.A | `1edcf78` | SPEC-005 introduced as `Proposed`. §Preamble + §Motivation + §Scope (In + Out shortform) + §Acceptance criteria opener with AC-001 (polyglot marquee; D7 45 s threshold; elapsed-time logging in CI; CI privilege assumption first-use validation per ADR-0010 §Decision part 3) + §References scaffold. |
| 3.3.B | `d997f98` | AC-002 (clean-fail on insufficient privilege per ADR-0010 §Decision part 1; exit code `9` introduced by SPEC-005) + AC-003 (`process.uid` byte-level recipe pinned by fixture + two boundary assertions at pid=1 and pid=4294967295 anchoring ADR-0011 §6's 58–67 character bound). |
| 3.3.C | `db72933` | AC-004 (`process.created_time` integer-nanos UTC + cache hit/miss for Terminate retention) + AC-005 (`process.exit_code` conditional emission — absent at activity_id=1, present-as-int32 at activity_id=2; three pinned values including NTSTATUS-as-negative-signed-int32 `0xC0000005` case). |
| 3.3.D | `902cffd` | AC-006 (top-level `process.name` strict normative + log-and-drop on absence per ADR-0011 §5 agent normative) + AC-007 (`parent_process` pid-only under PPID race; schema-relaxed + agent-permissive per the §5 dual-layer pattern). |
| 3.3.E | `43e5d17` | AC-008 (`events_dropped_total` visibility under ring overflow per ADR-0009 §Decision part 3; synthetic injection at ring enqueue boundary) + AC-009 (`events_lost` ETW buffer pressure via side-channel helper per ADR-0008 §Decision part 2; reproduces spike's 1 KB × 2 buffers + 80 ms callback + 200-process bursts conditions). §Acceptance criteria section closed. |
| 3.3.F | `5f34b00` | §Scope > Out of scope expanded from shortform to full per-item rationale subsection. Six items each with what-deferred + why-deferred-now + deferral target. CommandLine PII item carries the explicit accepted-risk decision with the triple cross-reference chain ADR-0006 §Out-of-scope → ADR-0011 §Out-of-scope → SPEC-005 §Out-of-scope item 6. |
| 3.3.G | `2b2ca7d` | §Operational subsection added between §Scope and §Acceptance criteria, carrying three agent-side mechanisms in dependency order: (1) ETW timestamp → UTC nanoseconds conversion (deterministic stateless formula with calendar-arithmetic constant); (2) PID-keyed volatile cache for `process.created_time` retention (scoped per ADR-0011 §6 — PID-keyed not uid-keyed; populated at Launch as sibling to ring enqueue; consulted and purged at Terminate; periodic sweep bounds memory); (3) kernel device path → Win32 path translation (`QueryDosDeviceW`-built mapping cached at agent startup; fallthrough cases enumerated for UNC, junctions, mount points, removable media, empty `ImageFileName`). |
| 3.3.H | `0ccf607` | §Non-functional requirements subsection added between §Operational and §Acceptance criteria, with seven NFRs under the `NFR-005-NNN` namespace: dispatch-callback constraint (load-bearing, anchored to spike empirical evidence); ring sizing triple `(65536, 1024, 5000)`; `events_dropped_total` envelope-side transport surface in `body.agent`; D7 marquee 45 s restatement + CI logging contract; AC-007 retry tolerance (3 attempts × 500 ms backoff); cache sweep cadence (60 s); volume-change subscription deferral restatement. |
| 3.3.I | `c7cddf3` | §Failure modes + §Observability + §Ratification record placeholder + §References full list closed out in one atomic move. §Failure modes carries terminal exits table (exit code `9` for ETW privilege; codes 1-8 inherited; runtime exit for worker / cache thread panics) + non-fatal conditions table (envelope rejection, ring overflow, events_lost observation, cache sweep evictions, path-translation fallthrough) with throttling policies. §Observability lists ETW-capture-specific lifecycle log events including the marquee elapsed log per NFR-005-004. §References lists seven ADRs + three predecessor SPECs + three RFCs + ETW Microsoft Learn schema + Phase 0 spike + engineering-notes + Blueprint §7 + §17.11. |
| 3.3.J | `80ef2f2` | **Ratification commit.** SPEC-005 status flipped `Proposed` → `Accepted`; §Ratification record populated with 11 load-bearing decisions (G1, G5, NFR-005-001/002/004/005/006, FILETIME constant, fallthrough cases, exit code `9`, co-located SPEC-001 amendment). SPEC-001 amendment 2026-05-23 co-located (sequence_number semantics under multi-POST; narrows wording scope rather than overriding; SPEC-001-only deployments retain original 1-POST-per-interval semantics). `docs/specs/README.md` catalog row added (5 SPECs Accepted). Four ADRs §References sweeped per Convention #5 extension: `SPEC-005 (forthcoming)` → markdown link in ADR-0008, ADR-0009, ADR-0010, ADR-0011. §Consequences blocks not touched per Convention #8 (point-in-time records). Handoffs not touched per same convention. `docs/engineering-notes.md` Session 13 entry added. **Action 4 (dep-graph edges) deferred** per Convention #7 chat ratification — the dep-graph is ADR-only; adding SPEC nodes would invent edge notation that did not previously exist and was not load-bearing for the SPEC-005 ratification. |

CI workflows exercised in Session 13: `markdown-lint` only across all ten commits. `rust-ci`, `ts-ci`, and `schema-validation` skipped via path filters on every Session 13 commit (no `agent/`, `services/ingest/`, or `schemas/` changes). They remain at the state they held at Session 12 close.

## ADRs at session close — Session 13 delta

Full catalog and dependency graph in [docs/adr/README.md](adr/README.md). Only documents touched in Session 13:

| ADR | Session 13 action |
|---|---|
| [0008](adr/0008-etw-crate-selection.md) | §References sweeped at `80ef2f2` (Phase 3.3.J): `SPEC-005 (forthcoming)` → markdown link. §Consequences and other sections not touched. Status unchanged (`Accepted`); no `## Amendment` event (Convention #5 extension provenance hygiene — sweep-only, not amendment). |
| [0009](adr/0009-event-delivery-and-buffer.md) | Same as ADR-0008. §References sweeped; rest unchanged. |
| [0010](adr/0010-agent-privilege-model-mvp.md) | Same as ADR-0008. §References sweeped; rest unchanged. |
| [0011](adr/0011-cges-process-activity-v0-1.md) | Same as ADR-0008. §References sweeped; rest unchanged. (§Context line 12's "SPEC-005 (forthcoming)" mention preserved verbatim per Convention #8 — that line documents the framing at ADR ratification time, point-in-time record, not provenance to be swept.) |

No new ADRs ratified in Session 13. Status flips, amendments, and supersessions in Sessions 1–12: see prior handoffs.

## SPECs at session close — Session 13 delta

| SPEC | Title | Status / Session 13 action |
|---|---|---|
| [001](specs/SPEC-001-agent-heartbeat.md) | Agent heartbeat | **Amended in-place 2026-05-23** at `80ef2f2` (co-located with SPEC-005 ratification per Session 10 cascade pattern). Amendment scope: sequence_number semantics under SPEC-005 multi-POST. The amendment narrows the original prose's scope (SPEC-001-only deployments retain 1-POST-per-interval semantics; SPEC-005-active deployments use multi-POST semantics) rather than overriding it. Status stays `Accepted`; `Last updated: 2026-05-23`. |
| [005](specs/SPEC-005-agent-process-telemetry-windows-etw.md) | Agent process telemetry — Windows ETW Kernel-Process | **Accepted this session** at `80ef2f2`. First SPEC under which the agent emits actual security telemetry rather than pure liveness signals. Carries §Preamble + §Motivation + §Scope (In + Out full rationale) + §Operational (three agent-side mechanisms) + §NFR (seven NFR-005-NNN entries) + §Acceptance criteria (AC-001 marquee + AC-002 through AC-009) + §Failure modes + §Observability + §Ratification record (11 entries) + §References. First consumer of ADR-0008, ADR-0009, ADR-0010, ADR-0011, and SPEC-003 amendment 2026-05-23 part (a). |

SPEC-002, SPEC-003, SPEC-004 — unchanged from Session 12's recorded state.

## Engineering-notes Session 13 additions

Authoritative text in [docs/engineering-notes.md](engineering-notes.md) under `## Session 13 (2026-05-23)`. Two procedural notes, neither promoted to convention status:

1. **Correction of record: Session 12 anchor SHA.** Detailed above in §Correction of record. The actual Session 12 close is `2f0c2ce` (lint-fix follow-up of `ee430c0`), not `59db12f` as the Session 12 handoff document records. Substance of the Session 12 handoff is intact.

2. **Architect-Claude raw URL acceptance constraint (operational reminder).** Architect-Claude's `web_fetch` tool requires raw URLs to be provided explicitly in the conversation by the user (URLs constructed from repo identifier alone are rejected by the tool's PERMISSIONS_ERROR check). For audit-first sessions where architect-Claude needs to read multiple repo artifacts, the friction-minimum pattern is: user pastes a block of raw URLs in one message; architect-Claude fetches them in declared order. Claude-platform constraint, not a project constraint; recorded for Sessions 14+ where audit-first reading of repo artifacts beyond the handoff documents is needed.

The nine Session 10 conventions + Session 11's Convention #5 extension + Session 11's full-SHA polling operational bullet + Session 12's two procedural validations remain authoritative and unchanged. No new conventions promoted in Session 13.

## D-decision status delta

Only D-decisions whose status materially changed in Session 13. Full table in [docs/handoff-session-10.md](handoff-session-10.md) §D-decisions status, with Session 11 delta in [docs/handoff-session-11.md](handoff-session-11.md) §D-decision status delta and Session 12 delta in [docs/handoff-session-12.md](handoff-session-12.md) §D-decision status delta.

| D | Topic | Session 12 status | Session 13 status |
|---|---|---|---|
| D2 | CGES Process Activity mapping + 4 sub-decisions | PARTIALLY DONE (more done substantively) | **DONE**. Closed by SPEC-005 ratification at `80ef2f2`. Full closure chain: jurisprudence ratified in ADR-0011 at `7717b49` (Session 11) + §4 amendment at `3b1c423` + schema realisation at `59db12f` (Session 12) + agent emission specification + ACs in SPEC-005 (Session 13). Agent-side ETW capture implementation lands in Phase 3.5; that is the literal first-write-against-the-spec implementation, distinct from the jurisprudence-and-specification work that D2 tracked. |
| D7 | Marquee timeout 45 s + elapsed-time logging in CI | PENDING | **DONE**. Closed by NFR-005-004 (45.0 s wall-clock threshold + `marquee_elapsed_seconds` logged at `info` on every AC-001 run, visible in CI output) + AC-001 (the marquee itself, polyglot end-to-end with real `cg-agent` + real `services/ingest/` + real Postgres/ClickHouse/Redis via testcontainers). |

D1, D3a, D3b, D4, D5 — unchanged from prior sessions. D6 (ClickHouse `cges_events` DDL) remains PARTIALLY DONE: engine `ReplacingMergeTree(event_id)` locked by ADR-0009 + SPEC-005 §Acceptance criteria reference; partition `(org_id, toYYYYMMDD(time))` and order `(org_id, time, event_id)` documented across ADR-0003 + ADR-0009; the literal `CREATE TABLE cges_events ...` DDL string lands in Phase 3.5 when the first ingest implementation commit happens against the harness's testcontainers ClickHouse.

## Phase 3.4 opening protocol — Session 14

Phase 3.4 is the **harness RED** phase per the standing invariant: all nine SPEC-005 ACs as failing tests **before** any implementation lands. First Session 14 commit that exercises the `rust-ci` workflow since Session 9 (the `rust-ci` baseline was last green at SHA `f407c05`); first Session 14 commit that exercises the `ts-ci` workflow since Session 9 also.

**Test layout (proposed; subject to architect-Claude refinement at Phase 3.4 opening):**

- `agent/cg-agent/tests/process_ac_001_marquee.rs` — AC-001 polyglot marquee. Testcontainers-driven: Postgres + ClickHouse + Redis + `services/ingest/` Node container + the `cg-agent` binary built at test setup. The marquee per the harness-first invariant: real ETW capture, real envelope, real ingest, real persistence. Most complex test in the SPEC; lands RED first (no agent code yet to capture ETW, no ingest schema yet to persist).
- `agent/cg-agent/tests/process_ac_002_privilege.rs` — AC-002 clean-fail on insufficient privilege. Synthetic `Err` injection at the ETW open boundary; CI-elevation-independent.
- `agent/cg-agent/tests/process_ac_003_uid_recipe.rs` — AC-003 `process.uid` byte-level recipe pinned by fixture. Pure unit test on the formatter; no testcontainers; smallest test in the SPEC.
- `agent/cg-agent/tests/process_ac_004_created_time_cache.rs` — AC-004 `process.created_time` integer-nanos + cache hit/miss. Two integration tests (cache-hit nominal lifecycle; cache-miss bypass-Launch).
- `agent/cg-agent/tests/process_ac_005_exit_code_conditional.rs` — AC-005 conditional emission with three pinned values (0, 1, NTSTATUS `0xC0000005` as `-1073741819`).
- `agent/cg-agent/tests/process_ac_006_name_log_and_drop.rs` — AC-006 synthetic event injection at ring enqueue boundary with empty `process.name`; two assertions on log + envelope absence.
- `agent/cg-agent/tests/process_ac_007_parent_pid_only.rs` — AC-007 PPID race; 3-retry × 500 ms backoff per NFR-005-005.
- `agent/cg-agent/tests/process_ac_008_dropped_total.rs` — AC-008 N+K injection into ring sized N; three assertions (exact K, FIFO drop of oldest K, monotonic counter).
- `agent/cg-agent/tests/process_ac_009_events_lost.rs` — AC-009 reproduces spike conditions (1 KB × 2 buffers + 80 ms in-callback sleep + three 200-process bursts); three assertions on `events_lost` (> 0, monotonic, helper success).

The 80 ms sleep in AC-009 is wired via a Rust `cfg(test)` flag or a constructor parameter on the dispatch callback for test-only use; production code MUST NOT contain the sleep. This is gated at Phase 3.5; AC-009's test code defines the hook.

**Drafting concerns to honour:**

- **Harness-first RED is the *whole* SPEC's ACs landing as failing tests in one or more atomic commits before any implementation.** Per the standing invariant: "the harness never downgrades to mocks; the marquee uses real `cg-agent`, real `services/ingest/`, and real Postgres/ClickHouse/Redis via testcontainers, with the documented escalation order if testcontainers can't reach the Docker socket."
- **Known CI debt declaration co-located with the RED phase.** Per CLAUDE.md §Harness-first red phases and debt co-locality: the *Known CI debt* table entry MUST live in the **same SHA** that turns the workflow red. Splitting across commits is not permitted.
- The harness RED commit(s) turn `rust-ci` and `ts-ci` red. Both must be declared in the *Known CI debt* table in `CLAUDE.md` in the same SHA(s). When Phase 3.5 (implementation to GREEN) closes, the entries are removed in the same SHA(s) that flip the workflows back to green.
- Testcontainers escalation order per the invariant: testcontainers → image pin → GHCR mirror → backends started via `task dev:up` by test setup. If testcontainers cannot reach the Docker socket during AC-001 development, fall through the escalation.
- **CI privilege assumption first-use validation.** Per AC-001 + ADR-0010 §Decision part 3: AC-001's first CI run is the validation. If `windows-latest`'s `runneradmin` cannot open the Kernel-Process provider (Win32 error code `5` or `1314`), one of ADR-0010's two named fallback paths fires (SYSTEM trampoline or move marquee out of CI). The choice is deferred until the contradiction surfaces; do not pre-pick.

**Audit-first protocol may or may not apply.** Phase 3.4 is harness construction, not contract drafting. The Session 13 audit-first pass already settled the inherited contract surface for Phase 3.3; Phase 3.4 consumes that surface but does not re-audit it. The audit-first protocol applies when a phase opens with contract reconciliation; Phase 3.4 opens with test code writing. Architect-Claude's call at Phase 3.4 opening whether any narrow audit is needed (e.g., does the testcontainers setup need a fresh look at `infra/dev/docker-compose.dev.yml`); default expectation is no.

**CI expectation when Phase 3.4 commits land:** `markdown-lint` skips (no markdown changes); `rust-ci` and `ts-ci` turn **RED by design** (the harness-first invariant); `schema-validation` skips (no `schemas/` changes unless DDL artifacts land here, which they should not — DDL lands in Phase 3.5). Phase 3.5 turns the workflows green.

## Known follow-ups

Co-located with their generating documents per Convention 1. New in Session 13:

- **Dep-graph SPEC-node convention.** The Phase 3.3.J ratification commit deferred adding SPEC-005 to the ADR dep-graph because the existing graph is ADR-only and adding SPEC nodes would have invented edge notation not previously in use. If a future SPEC ratification surfaces a constraint-bearing relationship that the ADR-only dep-graph cannot capture, a Convention #7 amendment ratifying SPEC nodes (and the inbound/outbound edge semantics for them, including whether SPEC→SPEC edges exist) is the principled path. Not blocking; surface when need exists.

All other follow-ups from prior sessions' handoff indexes remain in their existing states. Session 12's ajv-cli pinning note remains open (latent drift risk; not triggered in Session 13 because no `schemas/` changes).

## Invariants carried into Session 14

Binding for every session. Authoritative text in [CLAUDE.md](../CLAUDE.md) + Session-10 additions in [docs/engineering-notes.md](engineering-notes.md) §Session 10 + Session 11 additions in §Session 11 + Session 12 procedural validations in §Session 12 (documentary; not promoted to convention) + Session 13 procedural notes in §Session 13 (documentary; not promoted to convention). All invariants from [docs/handoff-session-10.md](handoff-session-10.md) §Invariants and [docs/handoff-session-11.md](handoff-session-11.md) §Invariants and [docs/handoff-session-12.md](handoff-session-12.md) §Invariants carry forward unchanged. **No new invariants from Session 13.**

The nine Session 10 conventions + Session 11's Convention #5 extension + Session 11's full-SHA polling operational bullet remain in force as recorded.

## How Session 14 resumes

1. Read this document and [docs/handoff-session-12.md](handoff-session-12.md) and [docs/handoff-session-11.md](handoff-session-11.md) and [docs/handoff-session-10.md](handoff-session-10.md) (for the canonical references this handoff does not replicate) and [CLAUDE.md](../CLAUDE.md). They are the binding contract.
2. Confirm `main` is at `80ef2f2` and all CI is green before starting. `git status` should show working tree clean (modulo `.claude/`).
3. Open Phase 3.4 (harness RED). First action: review the proposed test layout above; architect-Claude refines if needed; if testcontainers setup is in question, narrow audit on `infra/dev/docker-compose.dev.yml` opens. Otherwise proceed directly to writing the nine AC tests. Per the harness-first invariant, all nine ACs land as failing tests before any implementation. Known CI debt declaration co-located with the RED commit(s) per CLAUDE.md §Harness-first red phases and debt co-locality.
4. Architect-Claude's auto-memory directory carries narrative; this document plus prior handoffs plus the ADR/SPEC catalogs and engineering-notes carry the facts; the repo at `main` is the ultimate source of truth. Where any two disagree, re-verify against `main`.
