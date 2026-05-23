# Handoff — End of Session 12

Canonical state-of-the-world at the close of Session 12. This document is the
contract Session 13 resumes from. Session 12's scope was Phase 3.2 only — two
commits realising the schema deliverables of ADR-0011 — so this handoff is
proportionally similar in size to Session 11's. References to Sessions 10
and 11 supply the context this one does not replicate.

- **Anchor commit:** `59db12f` (`feat(schemas): realise ADR-0011 §4-§6 in CGES v0.1 objects/process.json (Phase 3.2.B)`)
- **Branch:** `main`
- **Date:** 2026-05-23
- **CI verdict at anchor:** ALL GREEN
- **Known CI debt:** none
- **Working tree:** clean (only `.claude/` untracked, pre-existing local config)

## Phase 3.2 closed in full

Two commits in a coordinated arc, both green on first push, no force-pushes,
no amendments-of-amendments. Phase 3.2.A landed the §4 jurisprudence
strengthening before the schema realisation in Phase 3.2.B so that the
schema commit referenced contract anchors already present in repo.

| Phase | SHA | Scope |
|---|---|---|
| 3.2.A | `3b1c423` | ADR-0011 in-place amendment 2026-05-23. §4 row 1 (`process.created_time`) gains explicit format declaration (integer nanoseconds since Unix epoch, UTC strict; deliberate OCSF divergence per the §6 determinism contract; pattern anchor on ADR-0006 §Deliberate divergence from OCSF applied at per-class scope). §4 row 2 (`process.exit_code`) gains explicit width declaration (signed int32; Windows `ExitStatus`/`LONG` semantics; NTSTATUS framing). `Last updated: 2026-05-23` added (first amendment to ADR-0011). Status stays `Accepted`. |
| 3.2.B | `59db12f` | Schema realisation. `objects/process.json` `required` relaxed from `["pid", "name"]` to `["pid"]` per ADR-0011 §5. Three new optional properties added: `uid` (string with regex pattern + length bounds 58–67 chars per §6 recipe), `created_time` (integer-or-null with `minimum: 0` per §4 amendment part (a)), `exit_code` (signed int32 with explicit bounds per §4 amendment part (b)). `classes/1007_process_activity.json` confirmed no-op via `$ref` composition (per Micro-decision 4: no conditional schema constructs for activity_id-discriminated optionality; the v0.1 emission contract lives in SPEC-005 §AC). Two new test fixtures: `examples/02_process_terminate.json` (activity_id=2, exit_code=0, resolved parent) and `examples/03_process_start_unresolved_parent.json` (activity_id=1, exit_code absent, parent_process pid-only). `01_process_start.json` regression-safe under the updated schema. |

Both commits green on first push; no Known CI debt at any point;
`schema-validation` first re-fire since Session 3 (`71e06ef`, 2026-05-20) went
green on poll 2.

CI workflows exercised in Session 12: `markdown-lint` (Commit A) and
`schema-validation` (Commit B). `rust-ci` and `ts-ci` skipped via path
filters on both commits.

## ADRs at session close — Session 12 delta only

Full catalog and dependency graph in [docs/adr/README.md](adr/README.md). Only
documents touched in Session 12:

| ADR | Session 12 action |
|---|---|
| [0011](adr/0011-cges-process-activity-v0-1.md) | **Amended in-place** at `3b1c423` (2026-05-23). §4 mapping-table rows 1 and 2 gain explicit format/width declarations (integer-nanos UTC; signed int32). Status stays `Accepted`; `Last updated: 2026-05-23` added (first amendment, same-day as the original ratification at `7717b49`). The amendment scope was strict (two §4 rows only); §Compliance line 187 deliberately not edited per the chat-ratified α scope decision — the third stricter-than-schema agent normative (exit_code conditional emission) is documented in SPEC-005 §AC, not in ADR-0011 §Compliance, avoiding duplicate sources of truth. |

No new ADRs ratified in Session 12. Status flips, amendments, and
supersessions in Sessions 1–11: see
[docs/handoff-session-10.md](handoff-session-10.md) §ADRs at session close and
[docs/handoff-session-11.md](handoff-session-11.md) §ADRs at session close.

## SPECs at session close

No SPECs touched in Session 12. State unchanged from Session 11's handoff.
SPEC-005 remains forthcoming (Phase 3.3, Session 13).

## Schemas at session close — Session 12 delta

| Schema artifact | Session 12 action |
|---|---|
| `schemas/cges/v0.1/objects/process.json` | `required` relaxed (`["pid", "name"]` → `["pid"]`); three new optional properties added (`uid`, `created_time`, `exit_code`). Property order: identity (pid, name, uid) → lifecycle (created_time, exit_code) → existing (cmd_line, file, parent_process, user). Existing field positions otherwise preserved; inline `examples` block deliberately not touched (the `examples/*.json` fixtures are the canonical documentation). |
| `schemas/cges/v0.1/classes/1007_process_activity.json` | **No edit.** Class file composes `objects/process.json` via `$ref`; new fields and relaxed `required` propagate transitively. Per Micro-decision 4 (Session 12 opening), no `if`/`then`/`else`, no `oneOf` with discriminator, no conditional encoding of activity_id-discriminated optionality. |
| `schemas/cges/v0.1/examples/02_process_terminate.json` | **New.** activity_id=2, exit_code=0, fully-resolved parent_process. Demonstrates cross-event stability (process.uid nanos suffix matches process.created_time byte-for-byte) and cross-agent disambiguation (same cg_agent.agent_id prefix on process and parent_process). |
| `schemas/cges/v0.1/examples/03_process_start_unresolved_parent.json` | **New.** activity_id=1, exit_code absent (not null, absent), parent_process pid-only per ADR-0011 §5 agent normative ("no sentinel, no resolution flag"). |
| `schemas/cges/v0.1/examples/01_process_start.json` | **Regression-safe.** Pre-existing Session 3 fixture (pid + name + cmd_line + file, no created_time/exit_code/uid) validates cleanly under the updated schema — the relaxation and the additive optional properties are strictly backward-compatible. |

Other schema files (`event.json`, other classes, other objects, common
extensions) unchanged.

## Engineering-notes Session 12

**No new conventions, no extensions, no operational bullets.** Session 12 did
not surface new procedural jurisprudence that warranted engineering-notes
codification. The nine Session 10 conventions plus the Session 11 Convention #5
extension and the Session 11 full-SHA polling operational bullet remain
authoritative and unchanged.

Procedural archaeology from this session (worth recording for narrative
continuity, but not promoted to convention status) is in the next section.

## Session 12 procedural validations

Two patterns surfaced or confirmed during Session 12 work. These are
documentary notes, NOT new conventions. The protocols they validate were
already established in earlier sessions.

1. **UUIDv7 hex conversion error caught at chat-verification step before
   commit.** During the Phase 3.2.B surface-diff review, chat-provided hex
   values for the new fixtures' `event_id` UUIDv7 prefixes did not match the
   stated ms timestamps (chat hex resolved to ~2025-06-20; the intended
   ms values resolved to ~2026-05-19, a ~year discrepancy). Programmatic
   integer-arithmetic verification (Python) surfaced the discrepancy before
   commit; corrected values applied; pattern compliance against `event.json`
   regex confirmed. Lesson: chat-surfaced hex/timestamp calculations deserve
   programmatic verification before draft acceptance, especially when the
   resulting bytes will live in fixture files that downstream tooling and
   future readers reference as didactic ground truth.

2. **Audit-first discipline applied for the third time in series, cleanly.**
   Session 10 produced two audits (ADR-0004 + SPEC-003) that surfaced
   load-bearing reconciliation strategies before any amendment text was
   drafted. Session 11's Phase 3.1.1 audit of ADR-0006 inverted the original
   framing (an amendment to ADR-0006 was wrong scope; per-class jurisprudence
   belongs in a dedicated ADR-0011). Session 12's Phase 3.2.A audit of
   ADR-0011 §4 + §6 surfaced M1 (the proposed `cg_*`-namespace divergence
   anchor was a category mismatch; the correct anchor is ADR-0006
   §Deliberate divergence from OCSF, applied at per-class scope per
   ADR-0011 §Decision part 1). Three clean applications across three
   sessions confirm the audit-first protocol as load-bearing. Session 9's
   "lexical audit before any amendment text" remains the original
   declaration; this is its third validation in series.

## D-decision status delta

Only D-decisions whose status materially changed in Session 12. Full table
in [docs/handoff-session-10.md](handoff-session-10.md) §D-decisions status,
with Session 11 delta in [docs/handoff-session-11.md](handoff-session-11.md)
§D-decision status delta.

| D | Topic | Session 11 status | Session 12 status |
|---|---|---|---|
| D2 | CGES Process Activity mapping + 4 sub-decisions | PARTIALLY DONE (jurisprudence ratified at ADR-0011) | **PARTIALLY DONE (more done substantively)**. Jurisprudence amended at `3b1c423` (§4 format/width declarations); schema realisation at `59db12f` (relaxed required + three new properties + two new fixtures). Formal status remains PARTIALLY DONE pending SPEC-005 ratification (Phase 3.3) and agent-side ETW capture implementation (Phase 3.4–3.5). |

D1, D3a, D3b, D4, D5, D6, D7 — unchanged from Session 11's recorded state.

## Phase 3.3 opening protocol — Session 13

Phase 3.3 is the SPEC-005 full draft. Qualitatively different work from
Phase 3.2's format/schema/fixture authoring: nine concrete ACs to
concretise, a polyglot marquee at AC-001, normative behaviour for the
agent's ETW capture path, multi-paragraph out-of-scope rationale, and
several chat gates expected throughout the drafting flow. Reference
[docs/handoff-session-10.md](handoff-session-10.md) §Phase 3 opening
protocol bullet on Phase 3.3 for the preliminary AC outline; that outline
becomes the Session 13 opening prompt's input.

**Drafting concerns to honour:**

- Status `Proposed` until chat ratifies, then `Accepted` in the same
  commit per the standing SPEC ratification protocol.
- Marquee timeout per D7: **45 s threshold** with **elapsed-time logging
  in CI** at AC-001. Logging is part of the marquee's definition, not an
  optional add-on.
- Dispatch-callback NFR with explicit citation to the Phase 0 spike's
  empirical evidence (**7649 events lost under 80 ms synchronous callback
  + 1 KB × 2 buffers**; see
  [docs/spikes/2026-05-23-etw-process-events.md](spikes/2026-05-23-etw-process-events.md)).
  The NFR is not negotiable; it locks the architecture's reason-to-exist.
- Six out-of-scope items per the Session 10 handoff's preliminary
  outline (other ETW providers; other event types beyond Launch and
  Terminate; persistent disk-backed buffer; Windows Service packaging;
  cross-platform agent support; PII redaction). Each gets explicit
  rationale; deferral targets where known.
- CommandLine PII accepted-risk decision lands in SPEC-005's
  out-of-scope section per ADR-0011 §Out-of-scope and ADR-0006 §Out-of-scope
  (Blueprint §17.11 deferral).
- ClickHouse `cges_events` DDL per D6 lands here (the engine ReplacingMergeTree
  is locked by ADR-0009; the partition/order tuple lands in SPEC-005).

**Audit-first protocol applies.** Before drafting any AC text, audit
ADR-0008 (ETW crate selection), ADR-0009 (delivery semantics + buffer),
ADR-0011 (per-class jurisprudence), ADR-0010 (privilege model),
SPEC-003 (envelope), and ADR-0006 §Out-of-scope (PII deferral) for the
contract surface SPEC-005 inherits. Surface reconciliation strategy
before drafting per the protocol's three clean applications.

**CI expectation when SPEC-005 commits:** `markdown-lint` fires;
`rust-ci`, `ts-ci`, `schema-validation` skip via path filters
(SPEC-005 is `docs/specs/` only). Phase 3.4 (harness RED) and 3.5
(implementation) will fire the code-side workflows.

## Known follow-ups

Co-located with their generating documents per Convention 1. No Session 12
follow-up was generated that requires a separate index entry beyond what
Sessions 10 + 11 already record, with one addition:

- **Proactive `ajv-cli` / `ajv-formats` version pinning in
  `.github/workflows/schema-validation.yml`.** The first re-fire after 3
  days (Session 3 `71e06ef` → Session 12 `59db12f`) went green with
  unpinned tooling — but the drift risk is latent. If a future minor or
  major release breaks JSON Schema draft 2020-12 compatibility, the
  workflow would fail unexpectedly on a `schemas/`-touching commit
  that hadn't planned for it. Chat decision when energy or data warrant;
  not blocking.

All other follow-ups from Session 10's handoff index and Session 11's
addition remain in their existing states. The four Phase 3.2 micro-decisions
that Session 11's handoff flagged are now closed (resolved by the Session 12
opening prompt's pre-ratification + realised in `3b1c423` + `59db12f`).

## Invariants carried into Session 13

Binding for every session. Authoritative text in [CLAUDE.md](../CLAUDE.md) +
Session 10 additions in [docs/engineering-notes.md](engineering-notes.md)
§Session 10 + Session 11 additions in §Session 11. All invariants from
[docs/handoff-session-10.md](handoff-session-10.md) §Invariants and
[docs/handoff-session-11.md](handoff-session-11.md) §Invariants carry
forward unchanged. **No new invariants from Session 12.**

The nine Session 10 conventions + Session 11's Convention #5 extension
(repo-wide cross-reference sweep at the ratification commit) + Session 11's
full-SHA polling operational bullet remain in force as recorded.

## How Session 13 resumes

1. Read this document and [docs/handoff-session-11.md](handoff-session-11.md)
   and [docs/handoff-session-10.md](handoff-session-10.md) (for the
   canonical references this handoff does not replicate) and
   [CLAUDE.md](../CLAUDE.md). They are the binding contract.
2. Confirm `main` is at `59db12f` and all CI is green before starting.
   `git status` should show working tree clean (modulo `.claude/`).
3. Open Phase 3.3 opening chat gate: surface the audit-first reconciliation
   of the inherited contract surface (ADRs 0006, 0008, 0009, 0010, 0011 +
   SPEC-003) for SPEC-005 BEFORE drafting any AC text. Same pattern as
   Sessions 10, 11, and 12's audit-first applications.
4. Draft SPEC-005 incrementally per ratified chat gates (preamble; each
   AC; out-of-scope rationale; NFRs; ratification commit). Atomic commits
   per gated unit if scope warrants; one large commit if cohesion warrants
   — architect's call at draft time.

Auto-memory directory carries narrative; this document plus Session 10's
and Session 11's handoffs plus the ADR/SPEC catalogs and engineering-notes
carry the facts; the repo at `main` is the ultimate source of truth.
Where any two disagree, re-verify against `main`.
