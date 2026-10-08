# Handoff — End of Session 32

Full state-of-the-world at the S32 close. Written so a cold or compacted session
recovers the thread in one read.

Session 32 delivered roadmap **Phase G** — agent capture on the normal run
path — as a new SPEC (**SPEC-017**, which amends SPEC-005, SPEC-003 and SPEC-001
by scope, and ADR-0004 and ADR-0011 in place) plus its implementation: fourteen
commits cherry-picked to `main`, one per push. It also added roadmap **§H**
(late events in detection) and gave the agent a Windows CI job. No new ADR.
Catalogs: ADR 17 / SPEC 17. Known CI debt: ZERO.

## Anchor commits (all on main, pushed)

| SHA | What |
|---|---|
| `0864469` | `docs(spec-017)`: SPEC-017 Accepted; ADR-0004 and ADR-0011 amended in place; catalogs; roadmap §G contract, new §H, D blocked by G and H. |
| `9250f68` | `feat(agent)`: ETW session start result and clean stop (G1). |
| `eee1e18` | `feat(agent)`: render captured events once (G2). |
| `db41d33` | `feat(agent)`: device path to Win32 translation (G3). |
| `8c7ee35` | `feat(agent)`: capture and at-least-once delivery on the secure path (G4). |
| `89eac2d` | `feat(agent)`: cache sweep and events_lost poll (G5). |
| `0feecec` | `test(agent)`: repair the real-ETW tests (G6). |
| `df8f452` | `feat(ingest)`: accept a full event batch; post events in CI (G7). |
| `4989e32` | `fix(agent)`: owner-only DACL by SID for the identity artifacts (G7b). |
| `ed31ef6` | `test(agent)`: AC-010 reads the DACL as full SIDs (G7e). |
| `43a3a66` | `test(agent)`: AC-010 reads the DACL without a PowerShell child (G7f). |
| `9bb0a7d` | `fix(capture)`: the Terminate carries the image base name (G7c). |
| `b9ab4f7` | `test(agent)`: AC-009 pressure without the keyword filter (G7d). |
| `4634e2a` | `ci`: build and test the agent on Windows; run ts-ci on agent changes (G8). |
| `24e0d04` | `docs`: capture gate and live text (G9). |
| (this commit) | handoff-32; SPEC-017 and ADR-0011 amendments 2026-10-09; README scorecard, roadmap, CLAUDE.md and agent README refresh. |

Fifteen pushes before this commit, one per commit, CI green on each, no retry.
The `rust-ci` Windows job runs on `main` from `4634e2a` on.

## SPEC-017

- **Owner delegation.** Manuel delegated the four owner decisions explicitly
  ("elige tú", 2026-10-04); the advisor decided, and Manuel's ratification of
  SPEC-017 ratified them (its §Ratification record): an unelevated agent exits
  with code 9; scope is the honest core; the realized wire is the contract;
  late events are their own phase (§H).
- **What it fixed** (observed at `e19f782`): only the test-mode path opened the
  ETW session; that loop dropped a batch on a transient failure, sent nothing
  while idle and skipped the going-offline handshake; the agent emitted ETW's
  device path untranslated; and no CI job compiled the Windows code.
- **Transport.** SPEC-017 and its companion doc edits reached Claude Code as a
  patch file verified by SHA-256 (relay rule 2), as in S31.

## Phase G — delivered

- **G1–G5:** `EtwSession::open` returns once the session has started or failed
  (exit 9 on Win32 errors 5 / 1314, exit 1 otherwise) and stops cleanly; each
  event is rendered once at batch formation; the device prefix map
  (`QueryDosDeviceW`, UNC rule, longest prefix at a separator); `run_secure`
  captures and delivers at least once (`delivery.rs`: up to 1024 events or
  5 s per batch, one POST in flight, the same POST retried, liveness on the
  SPEC-001 timeline, `going_offline` on shutdown); the 60 s cache sweep and the
  `events_lost` poll. `run_test_mode` and `CG_AGENT_TEST_MODE` are gone.
- **G6:** the real-ETW tests are `#[ignore]`d with one documented elevated
  command (`cargo test -p cg-agent -- --ignored --test-threads=1`).
- **G7:** ingest accepts a 4 MiB heartbeat body; CI posts events
  (`capture_ac_011`).
- **G7b / G7e / G7f — owner-only ACL** (SPEC-002 NFR-003; below).
- **G7c:** ETW's ProcessStop carries only the image's base name (below).
- **G7d:** the `events_lost` test (below).
- **G8:** `rust-ci` gains a `windows-latest` job (format, lint, unelevated
  tests); `ts-ci` runs on `agent/**`.
- **G9:** the elevated gate and live text in CLAUDE.md and the agent README.

## What the Windows runner and the elevated gate surfaced

The review went through three branches; each red is recorded with what was
observed and what was deduced.

1. **PR #1, `review/s32-g-capture` (tip `3b78251`).** The first Windows job
   failed in `enroll_ac_010`: after `harden()`, `cert.pem` held an explicit
   `BUILTIN\Administrators:(F)` entry. The old `harden()` ran `icacls
   /inheritance:r /grant:r`, which strips inherited entries but keeps other
   explicit ones. The old test matched English principal names, so its passes
   on a Spanish Windows (`BUILTIN\Administradores`) proved nothing. **G7b**
   sets a protected DACL with exactly two entries, by SID, through
   `SetNamedSecurityInfoW`.
2. **Manuel's first elevated gate ran on `3b78251`** (2026-10-04, before CI was
   green). Two test defects: the SPEC-005 marquee asserted Win32 form on the
   Terminate, which carries `cmd.exe`; and `process_ac_009` saw
   `events_lost = 0` with 216 callbacks. Fixed by **G7c** (Win32 form asserted
   on the Launch; the Terminate must not be in device form) and **G7d** (the
   test session takes every Kernel-Process event, adaptive bursts, `logman
   query` in the failure message).
3. **PR #2, `review/s32-g-capture-v2`.** Red again in `enroll_ac_010`, now with
   the right DACL (`D:PAI(A;;FA;;;LA)(A;;FA;;;SY)`): the runner user is the
   built-in Administrator (RID 500), which SDDL abbreviates as `LA`, and the
   test compared SID text. **G7e** read the DACL with `Get-Acl` from a
   `powershell.exe` child; it failed on the runner with
   `CouldNotAutoloadMatchingModule` — the job runs under PowerShell 7, and the
   5.1 child is deduced to inherit its `PSModulePath`. **G7f** reads the DACL
   with `icacls /save` and normalises each SID with `ConvertStringSidToSidW` /
   `ConvertSidToStringSidW`; a test checks that `SY`, `BU` and `LA` normalise.
   A regression test (an explicit `BUILTIN\Users` entry before `harden()`)
   failed against the old `harden()` locally.
4. **`review/s32-g-capture-v3` (tip `4933dfb`).** G7e and G7f must land before
   G8, so the fourteen commits were cherry-picked in landing order: patch-ids
   equal in the fourteen pairs, tree equal to the green v2 tip. It landed
   unchanged (final guard: `git diff --quiet 4933dfb main` → 0).

Not in any red but worth keeping: once `enroll_ac_010` stopped halting the
binary run, the test binaries after it — `enroll_ac_011`, `enroll_ac_012`,
`mtls_ac_001`–`009` and `process_ac_002`–`009` — ran on the runner for the
first time, with no failure. Some run no test there: `enroll_ac_012` is
Unix-only, and the real-ETW tests stay ignored (`process_ac_007` and
`process_ac_009` run none, `process_ac_004` one of its two).

## The elevated gate (reviewed tip `4933dfb`, Manuel, 2026-10-08)

- HEAD `4933dfb`, clean tree; `target\release\cg-agent.exe` newer than every
  file under `agent/`.
- `enroll_ac_010`: 3 passed, elevated.
- `--ignored`: `capture_ac_006` (elevated shutdown), `process_ac_004`,
  `process_ac_007` and `process_ac_009` passed — the last for the first time
  with real ETW (5.73 s).
- vitest: **42 files / 154 tests**, nothing skipped; the SPEC-005 marquee,
  `detect_ac_001` and `ac-001-marquee` ran.

**Path form.** The gate on `3b78251` logged the captured images in Win32 form:

```text
C:\Users\manul\AppData\Local\Temp\cg-detect-probe-R8X15k\winword.exe
C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe
```

On `4933dfb` both marquees assert the Launch's path in Win32 form and passed.
Debt #22 is closed.

**Run on the wrong terminal.** One run of the gate was unelevated: the
marquees captured nothing in about 5 s and `ac-001-marquee` was skipped — its
`skipIf` is exactly "Windows and not elevated", so a skipped `ac-001-marquee`
is the tell. CLAUDE.md now asks for `net session` and the HEAD before the gate.

## Test baselines

- vitest, Windows: elevated **42 / 154**; unelevated 39 passed, 1 skipped
  (`ac-001-marquee`) and 2 failed (the two capture marquees: the agent exits
  with code 9), of 42 files / 154 tests.
- Rust, Windows unelevated: 52 binaries, 86 passed, 4 ignored (the real-ETW
  tests). Linux: 80 passed, 1 ignored.

## Notes for the record

- **The Terminate's image.** ETW's ProcessStop `ImageName` is the base name,
  not a path; the agent emits it unchanged, as SPEC-005 §Operational §3 does
  with anything no prefix matches. SPEC-017 and ADR-0011 said "Win32 form, or
  device form verbatim"; both are amended 2026-10-09 (this commit). Rules
  evaluate Launch events only, so detection is unaffected.
- **`events_lost` volume.** With the process keyword `0x10` (the agent's own
  filter) a callback sleeping 80 ms lost nothing over the whole test; without
  the filter the counter moved. The agent's 60 s poll still reports any loss.
- **The briefing's "last binary commit" was G7b;** it is G7c (comments only
  under `agent/cg-agent/src`). Claude Code checked the strict reading.
- **Local environment.** Claude Code's Linux mirror left the Docker image
  `rust:1.93.0` and the volumes `cg-linux-target` and `cg-cargo-registry` on
  Manuel's machine, kept on purpose.

## Owner-STOP decisions pending (waiting on Manuel)

Unchanged from handoff-31, minus the unelevated agent (resolved in SPEC-017:
exit code 9; roadmap Part (b) §7). New for H:

- **ADR-0012 §7 amendment.** Phase H changes the read-model watermark from
  event time to arrival; amending an ADR is owner-STOP.

## Debts

- **#1–#11:** unchanged — see [handoff-session-26.md](handoff-session-26.md)
  §Debts.
- **#12–#14:** unchanged — see [handoff-session-27.md](handoff-session-27.md)
  §Debts, with the stale anchors added in handoff-31.
- **#17–#21, #24–#26:** unchanged — see
  [handoff-session-28.md](handoff-session-28.md) to
  [handoff-session-31.md](handoff-session-31.md).
- **#22 — CLOSED** by G3 (`db41d33`); observed above.
- **#23 — CLOSED** by G4 (`8c7ee35`): capture runs on the secure path; the
  test-mode path is removed.
- **#27 — `ac-001-marquee` flake:** not reproduced in S32. It passed in the
  nine `ts-ci` runs of the session (four on the review branches' PRs, five on
  `main`; the other pushes do not trigger `ts-ci`) and in the elevated gates.
- **#28 — `.github/workflows/README.md` is stale:** still lists only
  `markdown-lint`; it now also misses the `rust-ci` Windows job.
- **#29 — The Terminate carries no path.** Its `image_file_name` is ETW's base
  name, and its `process.name` comes from it. Inferred, not observed: ETW takes
  that name from the kernel's 15-byte image name, so a long name may arrive
  truncated. The agent could carry the Launch's path to the Terminate through
  the created-time cache. Destination: the next agent phase that touches the
  cache.
- **#30 — ETW tests on the hosted runner.** SPEC-017 §Open questions 1 reopens
  now that the Windows job exists: try whether `windows-latest` can open an
  ETW session; if it can, move the four real-ETW tests into CI.

Known CI debt: ZERO rows.

## How Session 33 resumes

1. Read this handoff, the prior handoffs and CLAUDE.md; confirm the `main` tip.
2. Next by dependency: **H** (late events in detection). First step: Claude
   Code audits the read-model and its watermark read-only
   (`services/ingest/src/detect/read-model.ts`, ADR-0012 §7, SPEC-006
   §Operational §1); the advisor drafts the ADR-0012 amendment and a SPEC that
   amends SPEC-006 by scope. The ADR amendment is an owner-STOP.
3. H's gate: a CI test with two agents delivering out of order, plus the
   SPEC-006 marquee.
4. After H: **D** (network 4001 + login 3002), then **B2**, **E** and **F**.
