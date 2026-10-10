# Handoff — End of Session 33

Full state-of-the-world at the S33 close. Written so a cold or compacted session
recovers the thread in one read.

Session 33 delivered roadmap **Phase H** — late events in detection — as a new
SPEC (**SPEC-018**, which amends SPEC-006 by scope and ADR-0012 in place) plus
its implementation: two commits cherry-picked to `main`, one per push. No new
ADR. Catalogs: ADR 17 / SPEC 18. Known CI debt: ZERO.

## Anchor commits (all on main, pushed)

| SHA | What |
|---|---|
| `1b1990b` | `docs(spec-018)`: SPEC-018 Accepted; ADR-0012 amended in place; catalogs; roadmap §H contract. |
| `9ea6a07` | `feat(ingest)`: the `ix_arrived_at` skip index on `cges_events` (C1). |
| `f2d36ae` | `feat(detect)`: the arrival cursor for the read-model (C2). |
| (this commit) | handoff-33; roadmap, README and CLAUDE.md refresh. |

Three pushes before this commit, one per commit; Claude Code reported CI green
on each (`markdown-lint` on `1b1990b`, `ts-ci` on `9ea6a07` and `f2d36ae`).

## SPEC-018

- **Owner delegation.** Manuel delegated the three owner decisions explicitly
  ("elige tú", 2026-10-09); the advisor decided, and Manuel's ratification of
  SPEC-018 ratified them (its §Ratification record): the vehicle (ADR-0012 §7
  amended in place, SPEC-006 amended by scope); no lateness horizon; the cursor
  starts at the beginning after the upgrade.
- **What it fixed** (read at `7a9160a`): the read-model advanced by event
  `time`, with one watermark per org, so an event that arrived with a `time` at
  or before the watermark was never evaluated. Every agent is written under the
  org `default`, so this was any installation with two agents.
- **The design.** A cursor `(arrived_at, event_id)`, ordered and compared by
  ClickHouse; a forward read without `FINAL`, behind a 5000 ms settle margin
  measured on ClickHouse's clock; the `dedup_key` makes a repeated match a
  no-op; a `minmax` skip index on `arrived_at`; migration `0007` replaces
  `last_time` with the cursor's two columns.
- **Measured before the design** by the advisor on ClickHouse 24.8.14.39,
  outside the repo's harness (SPEC-018 §Context 6): the rows of one `INSERT`
  share one `arrived_at`; a slow insert became visible after a later, fast one,
  with the smaller `arrived_at`; the index prunes only when the bound on
  `arrived_at` is a predicate of its own.
- **Transport.** SPEC-018 and its companion doc edits reached Claude Code as a
  patch file verified by SHA-256 (relay rule 2), as in S31 and S32.

## What the document review surfaced

The docs patch went through four versions. Each stop was a statement that
Claude Code's reading could not square with the repo or with the rest of the
text; the design did not change between versions.

1. **v1 — three statements.** "The `dedup_key` derives from the event alone":
   it carries the rule's id. A citation of SPEC-006 §Operational §6 for what
   SPEC-007 §Operational §6 and SPEC-014 §Data contracts §1 state. "Only
   `created_at` records when detection processed it": an alert also has
   `updated_at`, and joining an incident sets the incident's.
2. **v2 — one.** §Operational §6 said that an event already alerted "changes
   nothing" when it is re-read; that does not hold once a rule that also
   matches the event has been added.
3. **v3 — one.** The text now stated the effect per match (a rule matching an
   event), but §In scope still said "evaluation is idempotent".
4. **v4 — ratified.** Every sentence on the subject uses one of two phrasings
   already checked against the code. Squashed to `main` as `1b1990b`; the four
   review branches were deleted.

The lesson: when a revision changes how an effect is framed, search the whole
document for the old framing, not only the lines of the diff.

## Phase H — delivered

- **C1 (`9ea6a07`).** The ClickHouse bootstrap adds `ix_arrived_at` with an
  `ALTER TABLE … ADD INDEX IF NOT EXISTS` after the `CREATE TABLE`, which is
  unchanged. Test: `late_ac_007`.
- **C2 (`f2d36ae`).** Migration `0007_detect_arrival_cursor` (`last_arrived_at`
  text and `last_event_id` uuid, both `NOT NULL` with defaults; `last_time`
  dropped). `readNewEvents(config, cursor, limit)` returns the events and the
  cursor of its last row; `getCursor` and `advanceCursor` replace
  `getWatermark` and `advanceWatermark`. `SETTLE_MARGIN_MS` is 5000, and
  `DetectConfig.settleMarginMs` overrides it for tests; `buildDetectConfig`
  leaves it unset. Tests: `late_ac_001` to `late_ac_006`; the existing
  read-model, parent-resolution and detection suites moved to the cursor.
- **`late_ac_008`** has no file of its own: it is `detect_ac_005` as amended,
  the suites that run a detection cycle, and the elevated gate.
- **The margin in tests.** The test helper `detectConfig` injects a margin of
  0, because the synthetic tests insert in series. `detect_ac_001` and the
  driver-tick test in `detect-driver.test.ts` run with the production margin.
- **One existing test was inverted, with the advisor's approval.** In
  `read-model.test.ts`, "FINAL collapses at-least-once duplicate rows" became
  the resend that is read again: SPEC-018 §Operational §3 states the opposite
  of what the old test asserted.
- **No red commit.** The new tests travel in the commit that makes them green.
  Claude Code ran `late_ac_001` and `late_ac_002` against `main`'s code first
  and reported both failing on `eventsEvaluated` (0 where 2 was expected).
- **Landing.** The review branch `review/s33-h-arrival-cursor` (tip `b7bdbe2`,
  draft PR #3) landed unchanged: the tree of `main` at `f2d36ae` equals the
  tree of `b7bdbe2`. PR #3 was closed without merging and the branch deleted.

## The elevated gate (reviewed tip `b7bdbe2`, Manuel, 2026-10-10)

- HEAD `b7bdbe2`, the branch up to date with origin, no local changes.
- vitest: **49 files / 163 tests** passed, nothing skipped; `ac-001-marquee`
  ran, so the terminal was elevated.
- `detect_ac_001` passed in 46 887 ms with the production margin (its timeout
  is 60 s) and logged both image paths in Win32 form. The SPEC-005 marquee
  logged `marquee_elapsed_seconds` 40.046 against a budget of 45.
- `cargo test -p cg-agent -- --ignored --test-threads=1`: the first attempt did
  not compile (`can't find crate`, and `required to be available in rlib
  format` for dependency crates). Repeated later in the same terminal it
  compiled, with only `cg-agent` listed as compiling, and `capture_ac_006`
  (elevated shutdown), `process_ac_004`, `process_ac_007` and `process_ac_009`
  passed: 4 passed, 0 failed. The cause of the first failure was not
  determined.
- The repeated cargo run carries no `git rev-parse` line of its own. No file
  under `agent/` changed in S33, so those four tests ran the agent code that
  `7a9160a` has.

## Test baselines

- vitest, Windows: elevated **49 / 163**. Unelevated, by Claude Code's report:
  46 files passed, 1 skipped (`ac-001-marquee`) and 2 failed (the two capture
  marquees: the agent exits with code 9), of 49; 160 tests passed, 1 skipped
  and 2 failed, of 163.
- Rust: as in handoff-32; no file under `agent/` changed.

## Notes for the record

- **S33-03 ran once before its docs had landed.** The implementation prompt was
  first launched while SPEC-018 was not on `main`; Claude Code stopped at that
  precondition and wrote nothing.
- **Where SPEC-018 was short** (Claude Code's report; none needed an
  amendment). It does not say the two new columns are `NOT NULL`. Its
  §Operational §1 is pseudo-SQL: it gives no parameter types, and it does not
  warn that aliasing `event_id` or `arrived_at` in the projection would shadow
  the columns in the `WHERE` and the `ORDER BY`. It does not say how
  `detect_ac_001` lets the margin pass (a fixed wait of the margin plus 1 s
  after the agent exits), nor how `late_ac_007` gets a table without the index
  (`DROP INDEX` on a throwaway database).
- **Pruning on the real query** (Claude Code's report, a throwaway container):
  over 1 M rows in 20 parts, the forward read pruned to 2 of 20 parts and 12 of
  120 granules; without the bound on `arrived_at` as its own predicate it
  pruned nothing.
- **A late alert moves an old incident up.** Joining an incident sets its
  `updated_at`, and the api orders the incident list by it (SPEC-018
  §Operational §4).

## Owner-STOP decisions pending (waiting on Manuel)

Unchanged from handoff-32, minus the ADR-0012 §7 amendment, resolved by
SPEC-018.

## Debts

- **#1–#14, #17–#21, #24–#30:** unchanged — see
  [handoff-session-26.md](handoff-session-26.md) to
  [handoff-session-32.md](handoff-session-32.md).
- **#31 — An event's `time` is not validated or bounded at ingest.** The
  schema accepts any string, and a value the route cannot convert makes it
  answer 500. SPEC-018 §Out of scope sends this here.
- **#32 — A 500 that is not connectivity is resent without bound.** The agent
  treats a 5xx as transient and retries a POST that carries events until it is
  delivered or rejected (SPEC-017 §Operational §3), so that agent delivers
  nothing else meanwhile. With #31, one event whose `time` cannot be converted
  blocks its agent. SPEC-018 §Out of scope sends this here.
- **#33 — The driver's default interval is 10 000 ms; NFR-006-001 says
  5000 ms.** `INGEST_DETECT_INTERVAL_MS` in `services/ingest/src/config.ts`.
  SPEC-018 §Out of scope sends this here.
- **#34 — An incident's `cg_mitre` is set by the first alert processed.** The
  upsert's `DO UPDATE` leaves it out, so later alerts add no technique. With
  SPEC-018 the first alert processed is the first to arrive. SPEC-018 §Out of
  scope sends this here.
- **#35 — The driver's drain ceiling.** Ten batches of 1000 rows per org per
  tick (`MAX_ITERATIONS_PER_TICK`, `BATCH_LIMIT`). SPEC-018 §Out of scope
  sends this here.
- **#36 — The ClickHouse image tag floats.** `clickhouse/clickhouse-server:24.8`
  in the dev compose and in both test backends pins no patch release; the
  advisor's measurements were taken on 24.8.14.39.
- **#37 — `detect_ac_001` runs a single cycle.** It reads at most 1000 rows of
  the org `default`, the oldest arrivals first, and the other marquees write to
  that org in the same run. It was so before SPEC-018 too, in `time` order.
  Destination: drain until a cycle returns fewer rows than the limit.

Known CI debt: ZERO rows.

## How Session 34 resumes

1. Read this handoff, the prior handoffs and CLAUDE.md; confirm the `main` tip.
2. Next by dependency: **D** (network 4001 + login 3002). It needs successor
   SPEC(s) to SPEC-005 and per-class ADRs (roadmap §D); a new ADR is an
   owner-STOP.
3. After D: **B2**, **E** and **F**.
