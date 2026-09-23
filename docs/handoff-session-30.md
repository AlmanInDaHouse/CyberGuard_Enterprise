# Handoff — End of Session 30

Full state-of-the-world at the S30 close. Written so a cold or compacted session
recovers the thread in one read.

Session 30 delivered roadmap **Phase B1** (evaluator generalization) as a new
SPEC (**SPEC-015**, which amends SPEC-006 by scope) plus its implementation:
three code commits cherry-picked to `main`, one per push. No new ADR; the only
catalog change is the new SPEC. Catalogs: ADR 17 / SPEC 15. Known CI debt: ZERO.

## Anchor commits (all on main, pushed)

| SHA | What |
|---|---|
| `c6ee60a` | `docs(spec-015)`: detection evaluator generalization (SPEC-015 Accepted; amends SPEC-006 by scope). |
| `f006b9e` | `feat(ingest)`: Sigma condition parser (C1). |
| `1f9aa4a` | `feat(ingest)`: generalized Sigma subset evaluator (C2). |
| `18fc2d3` | `fix(ingest)`: reject an explicit `\|exact` modifier (C3). |
| (this commit) | handoff-30 + roadmap / README pointers + the CLAUDE.md marquee hardening. |

Four pushes, one per commit, CI green on each: `c6ee60a` with **markdown-lint**;
the three code commits with **ts-ci** (the ingest, api and dashboard jobs; in
the ingest job, **Build (tsc)**, **Entrypoints exist** and **vitest**).

## SPEC-015

- **Why a new SPEC that amends SPEC-006 by scope** (precedent SPEC-010 →
  SPEC-009), not an in-place amendment: CLAUDE.md's in-place amendment flow is
  for when the implementation *contradicts* an Accepted SPEC. B1 is a *planned
  extension* on which C and D build, not a contradiction — so it earns its own
  SPEC.
- **No ADR changes.** ADR-0012 §1 already covers the MVP detection slice hosted
  in TypeScript, and criterion 1 is MVP; the generalized evaluator stays within
  that jurisprudence.
- **The nine ratified decisions (brief):** (1) a new SPEC amending SPEC-006 by
  scope, not in place; (2) fields `Image` + `ParentImage` only — `CommandLine`
  / `User` rejected until B2; (3) four modifiers (exact = no modifier, endswith,
  startswith, contains), case-insensitive, wildcards rejected; (4) `and` /
  `or` / `not` / parentheses — no `1 of` / `all of` / aggregations; (5) an
  unknown parent ⇒ `ParentImage` items false (prefer a false positive to a
  false negative); (6) logsource dispatch, only `process_creation` implemented;
  (7) no read-model change in B1 (the marquee path is untouched); (8)
  fail-closed at load, fail-loud at boot; (9) doc-only gate first, code the next
  gate.
- **Review fixes before landing:** the Context paragraph made exact; the live
  sentences pinned to symbols rather than line numbers; the reserved words
  listed; empty values rejected (an empty `contains` would match every process).

## Phase B1 — delivered

- **C1 (`f006b9e`) — the pure condition parser** (`condition.ts` +
  `condition.test.ts`): the grammar (precedence `not` > `and` > `or`),
  identifier extraction, evaluation, and the fail-closed rejections.
  `UnsupportedRuleError` moved to `errors.ts`; `engine.ts` re-exports it so the
  existing imports are unchanged.
- **C2 (`1f9aa4a`) — the generalized evaluator** (`engine.ts` rewrite +
  `types.ts` + five `eval-ac_*` suites): fields, modifiers, named blocks, the
  parsed condition, and logsource dispatch. A matcher over a null field is
  `false` — no early return — so a `not filter_parent` clause still fires. The
  office rule evaluates identically.
- **C3 (`18fc2d3`) — a review finding.** The internal name of the no-modifier
  case (`"exact"`) leaked into the modifier resolver and accepted `Image|exact`;
  now an explicit `|exact` is rejected (Sigma has no `|exact`).

Gate: typecheck, lint and the non-elevated suite per commit — 58 → 76 → 104 →
105 tests. The elevated marquee was run by Manuel over `dbbc16b`, after
`cargo build --release -p cg-agent`: **37 files / 107 tests** green.

## Test baselines

The **non-elevated** suite is the full suite minus `spec-005-marquee` and
`detect-ac-001-marquee` (the two ETW marquees, `skipIf(platform !== "win32")`).
`ac-001-marquee` (SPEC-004 AC-001) has **no** `skipIf` and runs
everywhere, CI included. The **57** figure from S28 excluded all three
`*marquee*` files by a name filter — the same suite, a different filter. Today:
non-elevated **35 files / 105 tests**; elevated **37 / 107**.

## Accepted limitation

A pathologically nested `condition` (~2000 parentheses, or a ~10 000-term chain)
overflows the JS call stack and throws `RangeError` instead of
`UnsupportedRuleError`. It still **fails closed at startup**, because
`assertRulesLoadable` (`services/ingest/src/detect/driver.ts:141-147`) wraps any
load error and refuses to start the driver. Rules are author-written and pass
review; a depth cap would add a restriction SPEC-015 does not define. Recorded,
not fixed.

## Process notes

- The review-branch workflow (rule 5) was used for **code**, with a **fix
  commit on top** (C3) rather than an amend or a force-push.
- The literal SPEC-015 edits were verified **by hash** (the resulting file's
  SHA-256 matched the advisor's expected value).
- Some marquee commands were relayed **without a `cd`** and ran in
  `C:\Windows\System32`, where an elevated terminal starts. The procedure is
  hardened in the CLAUDE.md marquee section this session.

## Owner-STOP decisions pending (waiting on Manuel)

Unchanged from the handoff-29 list (ADR-0002 Go→TS reconciliation; the
criterion-7 deployment contract; forensic trust anchoring; B2 capture source;
the compose basename collision #12; the handoff-28 items; and the optional
Decision-authority process note). New for S31:

- **Which ten rules make up C is a product decision (owner).** The advisor
  proposes the rules and their contract; Manuel chooses.

## Debts

- **#1–#11:** unchanged — see [handoff-session-26.md](handoff-session-26.md)
  §Debts.
- **#12–#14:** unchanged — see [handoff-session-27.md](handoff-session-27.md)
  §Debts.
- **#17–#19:** unchanged — see [handoff-session-28.md](handoff-session-28.md)
  §Debts.
- **#20:** unchanged — see [handoff-session-29.md](handoff-session-29.md)
  §Debts.
- **#21 — `NotImplementedError` (`services/ingest/src/detect/errors.ts`) is dead
  code.** No `throw` from the SPEC-006 implementation, and two historical
  references in comments of `detect-ac-002` and `detect-ac-006`. Cleanup in a
  future sweep.

The format drift between `rules/tests/README.md` and its fixture is part of C's
work, not a separate debt.

Known CI debt: ZERO rows.

## How Session 31 resumes

1. Read this handoff, the prior handoffs and CLAUDE.md; confirm the `main` tip.
2. Next by dependency: **C** (criterion 1, the ten rules). First step: the
   advisor proposes the rules and their contract; Manuel chooses (a product
   decision).
3. Changes under `rules/windows/` trip the elevated-marquee gate (CLAUDE.md, the
   SPEC-006 procedure, step 6), and ADR-0005's harness obligation asks for a
   paired scenario per rule.
4. **D** is also unblocked but heavier (agent capture + successor SPECs +
   per-class ADRs).
