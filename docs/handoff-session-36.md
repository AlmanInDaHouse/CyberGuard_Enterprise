# Handoff — Session 36 (in progress)

Session 36 is open. This file is completed at the session's close (CLAUDE.md
*Session protocol*, §At the close); until then it holds the debts the
session's commits cite, so that each citation resolves at its own SHA.

## Start

- `main` = `origin/main` = `cbd3742`, no stash, no branch or PR open, CI
  green; catalogs ADR 19 / SPEC 20; Known CI debt ZERO.
- The working tree was not clean:
  `schemas/cges/v0.1/classes/1001_filesystem_activity.json` was modified at
  03:42 on 2026-10-11, ten minutes after the S35 close commit. The change is
  formatting only (the JSON expanded, the final newline removed) and the
  parsed content equals `HEAD`'s. No commit of this session includes it.

## Progress (in session)

Anchor commits on `main`, one per push, CI green on each:

| SHA | What |
|---|---|
| `cf54945` | `docs(spec-017)`: Amendment 2026-10-11 — the dispatch callback's locks and output (debt #44); this handoff started, debts #52 and #53. |
| `a293c27` | `docs(claude-md)`: the local TypeScript gate runs ts-ci's build steps (debt #45). |
| (this commit) | `.github/workflows/README.md` describes the four workflows (debt #28); this progress record. |

Waiting on Manuel:

- **Owner review, branch `adr/s36-0012-classes-flow`** (`3d88b5c`, pushed,
  no PR): ADR-0012 Amendment 2026-10-11 (debt #48), its subject marked `NOT
  YET RATIFIED`. Self-reviewed: own pass, then the fixed contract prompt on
  `f185bf9`, whose list A (§7's "the only class flowing in v0.1") the
  branch's text resolves. After the ratification, the same staleness goes:
  `SPEC-006:40` (a SPEC-006 amendment), and the destinations "roadmap §D"
  for rules over other classes in `services/ingest/src/detect/engine.ts:30`
  and `:351`, `SPEC-018:56` and `roadmap.md:101` (debt #54).
- **Debt #49 — no defect found.** The agent's resolved `windows-sys`
  features gained one, `Win32_System_EventLog`: `Win32_Security` was already
  on through `Win32_Security_Authorization` and `Win32_Security_Cryptography`
  (`windows-sys-0.59.0/Cargo.toml:146,148`), so the line `f191a1b` added
  names a feature already enabled. ADR-0019:24 ("the feature that exposes
  them") and :150 ("a feature") are true. Proposed: close #49 with no ADR
  change.
- **B2 diagnostics** (elevated, redacted): `C:\tmp\s36-b2-4688.ps1`,
  `s36-b2-peb.ps1`, `s36-b2-etw.ps1`, each writing a `.txt` of aggregates.
- **The modified `1001_filesystem_activity.json`** in the working tree: restore
  it with `git checkout --` once Manuel confirms it is not his.

Debt record changes so far:

- **#44 — CLOSED** (`cf54945`). **#45 — CLOSED** (`a293c27`).
- **#2 — CLOSED, no skew.** `markdownlint-cli2-action@v23` resolves to 23.2.0,
  whose `markdownlint-cli2` is 0.22.1, the version CLAUDE.md pins locally.
  The tag floats within v23; a later v23 release could move it.
- **#14 — CLOSED.** handoff-27 recorded that no live instance remained; the
  one left is Class H-inline.
- **#28 — CLOSED** (this commit).
- **#54 — Stale destinations "roadmap §D" for rules over other classes.**
  Phase D delivered capture and storage, no rule (`roadmap.md:217`);
  `engine.ts:30`, `:351`, `SPEC-018:56` and `roadmap.md:101` still send
  network and login rules to §D. Destination: after ADR-0012's Amendment
  2026-10-11 is ratified, the docs in a docs commit and `engine.ts` with the
  next change under `services/ingest/src/detect/`.

## Debts

New this session:

- **#52 — The process callback writes a `trace` line per record.**
  `agent/cg-agent/src/etw/session.rs` logs `dispatch callback fired` at
  `trace` for every record the Kernel-Process provider delivers. At the
  default level, `info`, it is not written. When `log.level` enables `trace`
  for the agent's ETW target, it is a line of output per event inside the
  dispatch callback, which ADR-0009 (§Consequences, §Compliance) and SPEC-005
  NFR-005-001 forbid. SPEC-017's Amendment 2026-10-11 does not restate it
  into §Operational §6. Destination: the next change under
  `agent/cg-agent/src/etw/` removes the line and corrects the module
  comments that say the callback does no I/O (`session.rs`, `dispatch.rs`,
  `ring.rs`).
- **#53 — The AC-006 line lacks `activity_id` and `event_id`.** SPEC-005
  AC-006 and its §Observability table require the `error` line of a process event
  dropped for an empty name to carry the `ProcessID`, the `activity_id`, the
  `event_id` and the reason; `agent/cg-agent/src/etw/ring.rs` writes `pid`
  and `reason` only, and `process_ac_006_name_log_and_drop.rs` asserts those
  two. Destination: the same change as #52.
