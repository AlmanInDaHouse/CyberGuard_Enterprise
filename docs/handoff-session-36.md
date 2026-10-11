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
