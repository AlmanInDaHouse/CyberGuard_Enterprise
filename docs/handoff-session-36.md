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
| `f37b243` | `.github/workflows/README.md` describes the four workflows (debt #28). |
| `be69547` | `docs(adr-0012)`: Amendment 2026-10-11 — classes 4001 and 3002 flow end to end (debt #48), from the owner-review branch `adr/s36-0012-classes-flow`, ratified by Manuel. Its tree equals the reviewed tip `3d88b5c` rebased (`f2fc6c6`) plus one sentence, the ratification record. |
| (this commit) | SPEC-006, SPEC-015 and SPEC-016 Amendments 2026-10-11, which follow ADR-0012's; roadmap §B1; this progress record. |

Manuel's decisions this session, in chat:

- 2026-10-11, asked whether to ratify ADR-0012's amendment and to close
  debt #49 without amending ADR-0019: "1-2 elige tú en base al proyecto".
  Claude Code chose to land the amendment and to close #49; Manuel then
  ratified the amendment ("si").
- 2026-10-11: the reformatted `1001_filesystem_activity.json` in the working
  tree was not his work to keep ("restaura"); Claude Code restored it with
  `git checkout --`.
- 2026-10-11, B2 decision 1, the capture source: option 1, the kernel's
  process events as the single source of process events ("creo que lo mejor
  ligado a lo que es cyberguard y destinado a ser vendido a empresas … es la
  primera opción").
- 2026-10-11, B2 decision 2, what is kept of the command line: option B, the
  text as the source delivers it (at most 1024 characters), with secret
  values replaced on the agent by a closed list of patterns before it leaves
  the host, visible markers for "redacted" and "possibly truncated", and a
  retention of 365 days from arrival for the command line ("opcion B si es
  la recomendacion, vamos a intentar llegar a la perfeccion total en el
  proyecto").
- 2026-10-11, B2 decision 3, the user: "1 + i" — the SID of the process's
  own user only, no name; the wire field renamed `process.user_sid` and the
  column `process_user_sid` before it carries data, the old always-empty
  column left in place.
- 2026-10-11, B2 decision 1 corrected: the third probe (below) showed that
  the kernel's process event carries no image path, so it cannot be the
  single source. Proposed to Manuel as option 1′ — Kernel-Process stays the
  source of process events and their path, the kernel's process events add
  the command line and the user SID, joined on the agent by PID and event
  timestamp — he accepted it: "sigo tu recomendacion sobre esa decision".
- ADR-0020 is drafted on the owner-review branch `adr/s36-0020-cmdline-user`,
  waiting for his "sí". Its review changed two things he had decided the
  shape of, both stated in its Decision record: the renamed field is
  `process.user.uid` (the OCSF path the newer classes use on the wire)
  rather than the `process.user_sid` proposed to him, and the parent at the
  edge is its own contract inside §B2, after SPEC-021.

B2, the facts measured on Manuel's machine (Windows 11 Home 10.0.26200,
workgroup), each from a script or probe that wrote only redacted
aggregates (`C:\tmp\s36-b2-*.txt`):

- **Security 4688.** Process Creation is not audited and
  `ProcessCreationIncludeCmdLine_Enabled` is absent. The log held 112 records
  4688, from 2026-10-06 to 2026-10-11, in 14 per boot over 8 boots: the boot
  processes (`smss.exe`, `csrss.exe`, `wininit.exe`, `lsass.exe`, …), all
  `S-1-5-18`, none with a command line. The log is circular and its window
  has moved since S35, so these are not the same 112 records S35 counted;
  they are of the kind that explains them.
- **Reading a process's command line from outside it**
  (`NtQueryInformationProcess`, class 60). Elevated, 347 of 354 running
  processes were read, at about 32 µs each. A `cmd /d /c rem` probe was read
  0 and 10 ms after `Start-Process` returned and could not be read from 50 ms
  on (`0xC000010A`).
- **Kernel-Process** (the agent's provider). ProcessStart v4 carries no
  command line and no user SID. With the SID enable property, no header SID
  was seen in two runs.
- **Process lifetimes.** Of the processes that started and ended in a 90 s
  window, 42 of 53 lived under 50 ms, median 35 ms.
- **The kernel's process events through ferrisetw 1.2's `KernelTrace`**, a
  session of its own name, opened while another tool held the "NT Kernel
  Logger":
  - `CommandLine`, `UserSID`, `ImageFileName` and `ParentId` were read in
    every record (404 of 404), with no schema error, and the SIDs decoded to
    their string form;
  - all 20 probes were seen at Start with their marker;
  - the session's rundown (DCStart) listed the 357 processes running when it
    opened;
  - the command line is cut at 1024 characters in Start, End and DCStart
    alike (probes of 3000, 6000 and 8000 characters all arrived with 1024);
  - `UserSID` is the process's own user, not its creator's: in 72 records it
    differs from the parent's (services of `S-1-5-19` and `S-1-5-20`, and the
    user's processes, under `S-1-5-18` parents);
  - delivery latency: p50 about 0.73 s, p90 and p99 about 1.0 s, the
    session's flush timer;
  - of the rundown's 352 non-empty command lines, 2 contained the word
    "token" and none matched a password pattern (counts only).
- **The kernel's `ImageFileName` against Kernel-Process's `ImageName`** (third
  probe, both sessions at once for 40 s): the kernel's holds the base name
  only — no backslash in 81 of 81 — and does not truncate it (a 52-character
  file name arrived whole); Kernel-Process's holds the device path in 80 of
  80. Matched by PID, the base names agree in 79 of 79, and the two event
  timestamps differ by at most 0.01 ms. Their arrival at the callbacks differs
  by a median of 0.14 ms; at the 10th percentile the kernel's arrived 794 ms
  earlier, so in a tenth of the pairs about 0.8 s earlier or more; at the
  90th percentile the Kernel-Process event arrived 0.02 ms earlier. Neither
  longest lead was measured.

Debt record changes so far:

- **#44 — CLOSED** (`cf54945`). **#45 — CLOSED** (`a293c27`). **#28 —
  CLOSED** (`f37b243`). **#48 — CLOSED** (`be69547`).
- **#49 — CLOSED, no defect.** The agent's resolved `windows-sys` features
  gained one, `Win32_System_EventLog`: `Win32_Security` was already on
  through `Win32_Security_Authorization` and `Win32_Security_Cryptography`
  (`windows-sys-0.59.0/Cargo.toml:146,148`), so the line `f191a1b` added names
  a feature already enabled. ADR-0019:24 and :150 are true. Delegated by
  Manuel, decided by Claude Code: no ADR change.
- **#2 — CLOSED, no skew.** `markdownlint-cli2-action@v23` resolves to 23.2.0,
  whose `markdownlint-cli2` is 0.22.1, the version CLAUDE.md pins locally.
  The tag floats within v23; a later v23 release could move it.
- **#14 — CLOSED.** handoff-27 recorded that no live instance remained; the
  one left is Class H-inline.
- **#54 — Stale destinations "roadmap §D" for rules over other classes.**
  Phase D delivered capture, delivery and storage, and no rule. This commit
  amends SPEC-006 (`:40`), SPEC-015 (`:25`, `:37`, `:38`; `:16` and `:110`
  stated) and SPEC-016 (`:43`) and restates `roadmap.md` §B1; `SPEC-018:56` is not stale (SPEC-019 `:10` moved it by
  scope). Still stale: `services/ingest/src/detect/engine.ts:30` and `:351`
  ("roadmap §D") and `rules/windows/README.md:5` ("rules over other event
  classes wait for roadmap §D"). Destination: B2's implementation branch,
  which runs the elevated gate; a change under `rules/` alone would void a
  green `detect_ac_001` run (CLAUDE.md *Developer-local SPEC-006 marquee
  validation*).

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
