# Handoff — End of Session 11

Canonical state-of-the-world at the close of Session 11. This document is the
contract Session 12 resumes from. Session 11's scope was Phase 3.1 only — the
three-commit ADR-0011 introduction arc — so this handoff is intentionally
shorter than Session 10's. References to Session 10's handoff supply the
context this one does not replicate.

- **Anchor commit:** `7717b49` (`docs(adr,engineering-notes): accept ADR-0011 (per-class CGES jurisprudence -- Process Activity v0.1)`)
- **Branch:** `main`
- **Date:** 2026-05-23
- **CI verdict at anchor:** ALL GREEN
- **Known CI debt:** none
- **Working tree:** clean (only `.claude/` untracked, pre-existing local config)

## Phase 3.1 closed in full

Three commits in one coordinated arc, all green on first push, no force-pushes,
no amendments-of-amendments. Subject to the audit-first discipline at maximum
load: Phase 3.1.1's audit of ADR-0006 inverted the original framing (an
amendment to ADR-0006 was wrong scope; per-class jurisprudence belongs in a
dedicated ADR), and the rest of the phase executed against the inverted plan.

| Phase | SHA | Scope |
|---|---|---|
| 3.1.A | `a52a841` | ADR-0006 hygiene fix (in-document Convention #5 stale ADR-0008 reference; `Last updated: 2026-05-23` added; `1007 Process Activity` added to examples list for symmetry) |
| 3.1.B | `e6d5b69` | Repo-wide Convention #5 sweep (ADR-0004 lines 159+189 past-tensed; ADR-0005 line 159 rewrite past-tensing ADR-0006 + replacing stale ADR-0007 reservation with descriptive language; engineering-notes Convention #5 extension recorded) |
| 3.1.C | `7717b49` | ADR-0011 ratified Accepted (per-class CGES jurisprudence pattern; Process Activity v0.1 as first instance; six §Decision parts; A1-A8 alternatives); README.md catalog row + four constraint-bearing dep-graph edges; engineering-notes operational bullet on full-SHA polling |

CI workflows exercised in Session 11: `markdown-lint` only. `rust-ci`, `ts-ci`,
and `schema-validation` skipped via path filters on every Session 11 commit
(no `agent/`, `services/ingest/`, or `schemas/` changes). They remain at the
state they held at session start.

## ADRs at session close — Session 11 delta only

Full catalog and dependency graph in [docs/adr/README.md](adr/README.md). Only
documents touched in Session 11:

| ADR | Session 11 action |
|---|---|
| [0004](adr/0004-agent-server-protocol.md) | Lines 159 + 189 past-tensed at `e6d5b69` (Convention #5 sweep). Status unchanged (`Accepted`); no `## Amendment` event (provenance hygiene). |
| [0005](adr/0005-detection-rules-and-ml-in-parallel.md) | Line 159 rewritten at `e6d5b69` (past-tense ADR-0006 with `cg_*` fulfillment acknowledgement; stale ADR-0007 reservation replaced with descriptive language). Status unchanged. |
| [0006](adr/0006-cges-ocsf-alignment.md) | Three hygiene touches at `a52a841` (header `Last updated`; 1007 added to examples list; line 133 stale ADR-0008 reservation neutralised). Status unchanged. |
| [0011](adr/0011-cges-process-activity-v0-1.md) | **Accepted this session** at `7717b49`. Per-class CGES jurisprudence pattern (first instance: Process Activity v0.1). Six §Decision parts covering meta-pattern, no-`cg_raw_ref` policy, single-schema-with-activity-id-discriminator, ETW field mapping, PPID race resolution (schema relaxed + agent normative stricter), and `process.uid` recipe (byte-level format spec). |

Status flips, amendments, and supersessions in Sessions 1-10: see
[docs/handoff-session-10.md](handoff-session-10.md) §ADRs at session close.

## SPECs at session close

No SPECs touched in Session 11. State unchanged from Session 10's handoff. SPEC-005
remains forthcoming (Phase 3.3).

## Engineering-notes Session 11 additions

Authoritative text in [docs/engineering-notes.md](engineering-notes.md) under
`## Session 11 (2026-05-23)`. Two bullets:

1. **Convention #5 extension** — the cross-reference sweep on reserved-ADR
   repurposing is repo-wide, performed in the commit that ratifies the new ADR
   meaning. Closes the procedural debt surfaced by Phase 3.1.1's audit
   (Session 10's ADR-0008 ratification swept only ADR-0004, missing ADR-0006
   and ADR-0005). Extension to the existing Convention #5 from Session 10, not
   a new tenth convention; the count stays at nine + extensions.
2. **Operational: poll CI workflows by full SHA, not short SHA.** The GitHub
   Actions REST API `head_sha` query parameter requires the full hex SHA.
   Phase 3.1.A's polling for short SHA `a52a841` returned `NO_RUNS_YET` for ~30
   consecutive polls before the script exited; Phase 3.1.B's polling for full
   SHA `e6d5b69226080bc6d58a61d7fd0e963e1b808e71` was terminal on poll 1.
   `git rev-parse HEAD` before constructing the REST API URL.

The nine Session 10 conventions remain authoritative and unchanged. Reference
Session 10's handoff §Engineering-notes for the full enumeration.

## D-decision status delta

Only D-decisions whose status changed in Session 11. Full table in
[docs/handoff-session-10.md](handoff-session-10.md) §D-decisions status.

| D | Topic | Session 10 status | Session 11 status |
|---|---|---|---|
| D2 | CGES Process Activity mapping + 4 sub-decisions | PENDING | **PARTIALLY DONE** (jurisprudence ratified in ADR-0011 covering D2.B no-`cg_raw_ref` policy, D2.C single-schema discriminator, D2.D ETW field mapping, plus PPID race resolution and `process.uid` recipe; D2.A `event_id` capture-time was already realised in ADR-0009 §Decision part 1; CommandLine PII covered by ADR-0006 §Out-of-scope + future SPEC-005 reference; activity_id v0.1 narrowing handled by ADR-0011 §3's permissive-schema + narrow-agent pattern. Schema realisation of ADR-0011 §5 + §4 pending Phase 3.2) |

D1, D3a, D3b, D4, D5, D6, D7 — unchanged from Session 10's recorded state.

## Phase 3.2 opening protocol — Session 12

Phase 3.2 is the schema work realising ADR-0011 §5 + §4 on `schemas/cges/v0.1/`.
First Session 12 commit that exercises the `schemas/` path filter and triggers
the `schema-validation` workflow.

**Edit list (concrete):**

- `schemas/cges/v0.1/objects/process.json` — relax `"required"` from
  `["pid", "name"]` to `["pid"]` (per ADR-0011 §5 schema change). Add three
  new properties: `created_time`, `exit_code`, `uid`. Format / type constraints
  per ADR-0011 §4 mapping table and §6 recipe.
- `schemas/cges/v0.1/classes/1007_process_activity.json` — possibly add
  `if`/`then`/`else` constructs to make `exit_code` only required when
  `activity_id` is `2`. Per ADR-0011 §3's "if needed" wording, this is a
  chat-decidable detail at Phase 3.2 opening (one option: omit the
  conditional and document the v0.1 emission contract in SPEC-005 instead;
  another option: encode the conditional at the schema layer).
- Test fixtures: `schemas/cges/v0.1/examples/01_process_start.json` already
  exists from Session 3. Adding a Terminate example fixture (e.g.,
  `02_process_terminate.json`; activity_id=2, with `exit_code` populated)
  and an unresolved-parent example (e.g.,
  `03_process_start_unresolved_parent.json`; Launch with
  `parent_process.pid` only, no `parent_process.name`) would give the
  schema-validation CI coverage of both PPID race branches and both
  activity_id branches.

**Fine-grained micro-decisions for the Phase 3.2 opening chat gate:**

- `process.uid` JSON Schema constraint shape: `pattern` regex matching the
  `<agent_id_canonical>:<pid_decimal>:<created_time_unix_nanos_utc>` format
  from ADR-0011 §6? Length bounds? Plain `type: string` with the byte-level
  contract enforced by SPEC-005 fixtures only?
- `exit_code` type: NTSTATUS-permissive `int32` (Windows returns LONG which
  is `int32_t` signed)? Or unsigned 32-bit per some other convention? Or
  plain `integer`?
- `created_time` type: `format: date-time` (RFC 3339, as elsewhere in CGES)
  or `format: date-time` plus a more constrained UTC-only pattern?
- Conditional schema constructs for activity_id-discriminated optionality
  (the §3 "if needed" question above).

Each is a micro-decision with SPEC-005 test fixture implications. Resolve in
chat before applying.

**CI workflow drift consideration.** The `schema-validation` workflow last
ran green at SHA `71e06ef` (Session 3, 2026-05-20). Workflow file and schemas
unchanged since. Possible npm-install version drift (`ajv-cli` / `ajv-formats`)
across the interval — Phase 3.2's first commit catches it if real; fix
co-locates per standing ratification protocol.

## Known follow-ups

Co-located with their generating documents per Convention 1. No Session 11
follow-up was generated that requires a separate index entry beyond what
Session 10's handoff already records. Reference
[docs/handoff-session-10.md](handoff-session-10.md) §Known follow-ups for the
canonical list.

One new entry — Phase 3.2 micro-decisions (the four bullets above). Resolved
when Phase 3.2 opens; not a long-running follow-up.

## Invariants carried into Session 12

Binding for every session. Authoritative text in [CLAUDE.md](../CLAUDE.md) +
Session 10 additions in [docs/engineering-notes.md](engineering-notes.md)
§Session 10 + Session 11 additions in §Session 11. All invariants from
[docs/handoff-session-10.md](handoff-session-10.md) §Invariants carry forward
unchanged. Session 11's additions:

- **Convention #5 extension** (repo-wide cross-reference sweep at the
  ratification commit, not scoped to a single document being amended).
- **Operational** (poll CI workflows by full SHA via `git rev-parse HEAD`,
  not short SHA).

The nine Session 10 conventions remain in force as recorded.

## How Session 12 resumes

1. Read this document and [docs/handoff-session-10.md](handoff-session-10.md)
   (for everything Session 11 does not replicate) and [CLAUDE.md](../CLAUDE.md).
   They are the binding contract.
2. Confirm `main` is at `7717b49` and all CI is green before starting.
   `git status` should show working tree clean (modulo `.claude/`).
3. Open Phase 3.2 opening chat gate: surface the four fine-grained
   micro-decisions listed above for chat ratification BEFORE drafting the
   schema edits. Same pattern as Session 11's Phase 3.1.C gates.
4. Apply schema edits + test fixture additions per the ratified
   micro-decisions. Atomic commit; `schema-validation` workflow fires; poll
   by full SHA; report verdict.

Auto-memory directory carries narrative; this document plus Session 10's
handoff plus the ADR/SPEC catalogs and engineering-notes carry the facts;
the repo at `main` is the ultimate source of truth. Where any two disagree,
re-verify against `main`.
