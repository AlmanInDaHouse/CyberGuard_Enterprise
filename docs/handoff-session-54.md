# Handoff — Session 54

Session 54, 2026-11-03, continues F2 under SPEC-024. It runs the probe pull
request for the SCM/SYSTEM tests, implements **install_ac_007** (the agent
as a Windows service) and **install_ac_010** (its rotating JSON log, with
debt #137), each through a fresh-context review in two rounds, and drafts
**#141**'s texts for Manuel. The code is on two stacked pull requests, both
CI ALL GREEN; **neither has landed**: they touch the capture path and wait
for the elevated gate, which Manuel has not run yet (*Waiting on Manuel*).
`main` carries only this handoff's close commit.

## Start and resulting state

- Start: `main` = `origin/main` = `010a0d8`, clean, no stash, only `main`
  on origin, no PR open. `main`'s tip CI green (`markdown-lint →
  success`). Catalogs 22 ADRs / 23 SPECs; open debts 50; Known CI debt
  none. Repo and `docs/handoff-session-53.md` agreed. C: 33 GB free, about
  1.1 GB of RAM free, Docker Desktop down (not needed: no local marquee).
- Resulting `main`: this session's close commit on `010a0d8`; no code.
- Open branches, each named here with its reason:
  - `feat/s54-install-ac-007`, PR **#81**, tip `fca6303`: install_ac_007.
    CI ALL GREEN on `3e301d6` and `fca6303`. Waits for the elevated gate.
  - `feat/s54-install-ac-010`, PR **#83** (base: `feat/s54-install-ac-007`),
    tip `73a7d92`: install_ac_010 and #137 (`e687837`) and the second review
    round (`73a7d92`). CI ALL GREEN on both. Waits for the elevated gate and
    for #81.
  - `review/s54-141-texts`, no PR, tip `b482cd5`: #141's four amendments,
    each commit `NOT YET RATIFIED`. Not ready for Manuel's reading: its
    fresh-context review runs once #81 is on `main` (the texts describe the
    service mode, which `main` does not have yet).
- Closed without landing: probe PRs **#80** and **#82** (below), their
  branches deleted.
- Catalogs unchanged: 22 ADRs / 23 SPECs.

## The probes (CLAUDE.md compensating control 1; SPEC-024 Amendment 2026-11-01, point 4)

- **Probe 1, PR #80**, `e1ebc7c` on `main`: the test-only Cargo feature
  `scm-tests`, `tests/install_ac_007_service.rs` (8 tests) and a rust-ci
  Windows step with `continue-on-error`. Run 41637475623, Windows job
  125922202173: the 8 ran as the runner's Administrator and **all 8
  failed for the missing service**: five with `StartServiceW` Win32 1053
  (the binary did not run as a service), three with `STOPPED (0, 0)` where
  `(1066, 2)`, `(1066, 2)` and `(1066, 10)` were asserted.
- **Probe 2, PR #82**, the review commits' SCM tests against `3e301d6`
  (the tree without the fixes): run 41643354060, Windows job 125942203693,
  at `9df9315`. 11 tests: **2 failed for the missing behaviour**
  (`install_ac_007_the_service_refuses_the_plain_path_with_code_2`: the
  pre-fix service reached `RUNNING` on the plain path;
  `install_ac_010_the_service_log_is_protected_json_lines`: no `agent.id`
  on the starting line, #137), and 9 passed: they strengthen assertions on
  behaviour `3e301d6` already had. A last push (`c344f89`) carried the
  final test file; the PR was closed on the `9df9315` evidence.

## What the two pull requests implement

Contract: SPEC-024 §Data contracts §2–§3, §Operational §3 and its second
Amendment 2026-11-01 (items 1–4), SPEC-002 Amendment 2026-10-27, SPEC-017
Amendment 2026-10-27; debt #137.

- **`--service`** (`main.rs`, `src/service/`): the process goes to the SCM's
  dispatcher before the configuration is read; without a start by the SCM,
  code 11 and item 2's line. In service mode a relative `--config` is code
  2, a configuration without `[enrollment]` (the plain path) is code 2, and
  a config error's entry names the file and the TOML line and column, never
  a line of the file (`ConfigError::Parse` carries the position).
- **States**: `START_PENDING` until the configuration, the instance lock and
  the file log are done; `RUNNING` accepting `STOP` and `SHUTDOWN` alone,
  before identity, enrollment and capture. The control handler takes no
  lock; a status thread reports `STOP_PENDING` (looking every 250 ms) and
  advances a pending checkpoint every 5 s with a 15 s wait hint, up to caps
  (120 s start, 75 s stop) past which a hang shows as hung.
- **Stop**: the run is a task beside a watchdog that ends the service 60 s
  after `STOP`; the run's own budget is 58 s (`run_secure_within`), so it
  reports what was pending and how many events it lost; the teardown runs in
  one blocking task. `STOPPED` 0 after a clean stop, else 1066 with the
  cause's code (1–10, 12 deadline, 13 log). Console `run_secure` is
  unchanged (no budget).
- **Enrollment** in service mode: rounds every 60 s instead of exit 4, a
  stop prompt at any point; an incomplete identity set fails closed (code
  5) without enrolling or overwriting.
- **Application log**: entries 1000–1003 under source `CyberGuardAgent`;
  1003's count is "unknown" when the watchdog, not the run, ended the stop.
- **File log**: `%ProgramData%` from `SHGetKnownFolderPath`;
  `CyberGuard\agent\logs` created with protected DACLs or checked by handle
  (no reparse point; owner SYSTEM or Administrators; no other allowed
  principal); the file opened without following a reparse point, refused if
  hard-linked. One active `agent.jsonl` and five archives of ≤10 MiB;
  rotation renames the active file first, so a reader that blocks it moves
  nothing, and only a rotation failing for 60 s is persistent; records of
  ≥64 KiB replaced by a bounded JSON diagnostic; a persistent write failure
  stops the service like `STOP` and reports code 13.
- **#137**: "agent starting" carries `agent.version`, `agent.id` (with
  `agent.id_source = configuration`), `agent.platform`, `agent.hostname`,
  `server.url` and `heartbeat.interval_seconds`; "agent identity ready"
  then carries the server-issued id.
- `windows-sys` gains `Win32_System_Services`, `Win32_UI_Shell` and
  `Win32_System_Com` (features of the existing dependency; no new crate).
- rust-ci's Windows job: a step runs `cargo test -p cg-agent --features
  scm-tests --no-fail-fast --test install_ac_007_service --
  --test-threads=1`. CLAUDE.md *Environment facts* names the feature.
  Runbook `docs/operations/install.md`: codes 11–13, the service's
  refusals, the log's bounds.

## Decisions

### Anticipated (Claude Code)

- **SCM through `windows-sys`** (Manuel's delegation for
  `Win32_System_Services`), plus `Win32_UI_Shell` and `Win32_System_Com` to
  resolve `%ProgramData%` through the OS. Rejected: the `ProgramData`
  environment variable (SPEC-024 §Data contracts §2 asks for the OS).
- **`scm-tests` with `[[test]] required-features`**, and the CI step naming
  each target. Rejected: `#[ignore]` plus a name filter (rejected by
  SPEC-024 Amendment 2026-11-01, point 4).
- **The file log in install_ac_007, with handle-checked custody**, since
  `RUNNING` needs logging. Owner Administrators is accepted beside SYSTEM
  (an elevated administrator is not a threat boundary, §Data contracts
  §2). Rejected: an unchecked log directory (a SYSTEM write an ordinary
  user could redirect).
- **The plain path stays the console's**; the service runs the secure path
  alone. Rejected: the service running plain HTTP (§Operational §3, "no
  silent plain HTTP").
- **A dispatcher failure other than 1063 is code 1**, not 11. Rejected: 11
  for every failure (only 1063 says the SCM did not start it).
- **Code 13 overrides only a clean stop**; a failure of its own keeps its
  code.
- **64 KiB** as the record bound (the SPEC says "bounded").

### Reactive corrections

- **Round 1** (fixed-prompt reviewer, an FFI/security pass and an SCM
  protocol pass, at `3e301d6`) → `fca6303`: the deadline from `STOP` (the
  watchdog, the 58 s budget, the teardown in one blocking task); the
  invented "0" count → "unknown"; the lock-free handler; the checkpoint
  caps; the plain path refused; the incomplete identity set; the biased
  stop-first selects; handler-registration failure → entry 1002 and exit;
  `OsString`; the token-redacting `Debug`; the runbook's code-13 path (a
  node edit had eaten its backslashes).
- **Round 2** (fixed-prompt reviewers of `3e301d6..fca6303` and
  `fca6303..e687837`) → `73a7d92`: the starting line's placeholder id
  (`agent.id_source`, "agent identity ready"); rotation that a reader blocks
  (rename the active file first, retry, 60 s persistence, no archive lost
  on retries); the partial-write newline; the missed wake-up (250 ms poll);
  entry 1000 only when `RUNNING` was reported; `try_exists`; the 13-remap
  as a tested function; entry 1003's wording.
- The rotation tests first wrote past the bound before the blocking reader
  was taken (records are ~1.1 KB): resized to 8500 records.

### Manuel's decisions in chat

None this session beyond the opening prompt's standing ones (the probe,
`windows-sys` for the SCM, landing F2 with its gates).

## Self-review and fresh-context reviews

Claude Code's matrix (contract → test):

| Criterion | Tests | Verdict |
|---|---|---|
| `--service` without the SCM → 11, before the config | `tests/install_ac_007_service_cli.rs` | covered |
| Relative `--config` / plain path / invalid config → 2, never `RUNNING`, entry 1002 | `install_ac_007_service.rs` (three SCM tests); `install_ac_007_stop.rs` | covered |
| `RUNNING` accepts `STOP`+`SHUTDOWN` alone | SCM delivery test | covered |
| Pending checkpoint ≤10 s, hint ≤30 s | `assert_pending_progress`, ≥5 advances in the 60 s stop | covered |
| `STOP` prompt during delivery, request in flight, retry backoff, between rounds | four SCM tests; four portable tests | covered |
| Teardown before the final send | ETW session read when `going_offline` arrives | covered (ETW only; the logon subscription is not observed) |
| 60 s deadline → 12, entry 1003 with pending work and count | SCM stalled test; portable budget tests | covered (the watchdog and teardown-expiry paths: missing, #145) |
| Normal stop leaves no session, restart works | SCM delivery test, two rounds | covered |
| Held lock → 10, no session stopped (the agent's own name included) | SCM held-lock test | covered |
| Untrusted data directory → 13 before `RUNNING` | SCM planted-directory test | covered |
| Real `SHUTDOWN` | none | missing by venue (SPEC-024 point 4; #145) |
| Log: 1 + 5 × 10 MiB, valid bounded JSON, protection kept | `install_ac_010_logs.rs`; SCM log test (`O:SYD:P(A;;FA;;;SY)(A;;FR;;;BA)`) | covered (a rotated production archive under the SCM: not exercised) |
| Persistent write/rotation failure → 13 | `install_ac_010_logs.rs` (callback, remap) | weak: the service wiring is not run under the SCM |
| #137 fields | binary test; SCM log test | covered |

Fresh-context reviewers (Agent/Workflow, isolated worktrees, CLAUDE.md's
fixed prompt; extra lenses in round 1). Their lists and dispositions:

- **Round 1, fixed prompt at `3e301d6`.** List 1 weak/missing: `SHUTDOWN`
  (venue, recorded #145), teardown ordering (fixed: session read at the
  final request), deadline from `STOP` (fixed), truthful 1003 (fixed),
  teardown-expiry path (recorded #145), `START_PENDING` until… (fixed:
  `assert_never_ran`), code 13 (fixed: planted-directory test; persistence
  in #83), the code-10 "stops no session" (fixed). List 2: deadline (fixed);
  invented count (fixed); plain path (fixed); incomplete identity (fixed);
  custody before reading identity (recorded #142); persistent write failure
  and rotation (fixed in #83); runbook path (fixed). List 3: backstop
  (replaced by the watchdog); ACL effects (recorded: BA owner accepted; an
  identity directory under `agent\` would inherit BA full control, #142);
  the new `windows-sys` features (recorded above).
- **Round 1, FFI/security lens.** No blocker or major. F1 path re-open
  after the handle check (recorded: safe under ProgramData's default DACL);
  F2 squatting DoS (recorded #144, threat model R17 in #141's draft); F3
  identity inheritance (#142); F4 lossy path (fixed); F5 panic (fixed:
  the run is a task, a panic is code 1); F6 forgeable entries (recorded:
  SPEC-024 item 4 already calls them diagnostics, not evidence); F7 token
  in `Debug` (fixed, tested).
- **Round 1, SCM protocol lens.** F1 caps (fixed); F2 backstop vs
  synchronous work (fixed: task + watchdog); F3 handler registration
  (fixed); F4 count (fixed); F5 stop as identity ends (fixed); F6 handler
  mutex (fixed: lock-free); F7 biased select (moot); T1–T5 (fixed:
  advances asserted, entries waited for; T2, the round wait watched 15 s,
  recorded as weak).
- **Round 2, fixed prompt, `3e301d6..fca6303`.** Weak/missing: delivery
  retry backoff not timed (recorded), logon subscription at the final send
  (recorded), the 58/60 s split (fixed: 1003's wording), watchdog/teardown
  paths (#145), the code-10 own-name session (fixed), `RUNNING` before
  identity observation (recorded weak), caps vs the 10 s clause (recorded:
  the caps keep a hung state from looking alive, as §Operational §3's "not
  a permanently pending service" asks), lost wake-up (fixed),
  `Path::exists` (fixed), handler-registration exit without `STOPPED`
  (recorded: no handle to report with), installed-mode paths (#142),
  Debug untested (fixed). List 3's new causes of codes 1 and 2 are within
  those codes' meanings (runbook updated).
- **Round 2, fixed prompt, `fca6303..e687837`.** The placeholder `agent.id`
  (fixed); rotation failure treated as persistent at once (fixed); the
  partial-write duplicate (fixed with a newline; residual #143); an
  oversized pre-rotation file archived whole (#143); the remap untested
  (fixed); entry 1000 after a refused `RUNNING` (fixed); `pub mod rotation`
  on every platform (recorded: its tests run on Linux too); the 64 KiB
  wording (fixed).
- The reviewers' worktrees were removed; the branch and status were checked
  after each round. The reviewer shares Claude Code's model: a second
  reading, not a second judge.

## Gates and CI

- **Local gate** (unelevated), on `73a7d92`: `cargo fmt --all -- --check`
  (clean); `cargo clippy --all-targets --all-features -- -D warnings`
  (clean); `cargo test --all`: 72 binaries, **181 passed, 0 failed, 11
  ignored**; `node .github/scripts/agent-version-matrix.mjs`: "agent
  version matrix: all steps passed"; `markdownlint` 0 issues. The SCM tests
  are not run locally (they refuse outside a GitHub-hosted runner).
- Local red evidence, by mutation of the implementation and restore:
  install_ac_007_stop/cli (7 of 10 failed for the neutralised behaviour; 4
  regression guards held), the two new checks (2/2), install_ac_010 (8/8,
  then the round-2 tests 4/4).
- **CI**, full SHAs: `3e301d6`, `fca6303`, `e687837`, `73a7d92` all
  **ALL GREEN** (rust-ci, ts-ci, markdown-lint, deploy-ci; dependency-audit
  where the commit touched a manifest). The SCM step: 8/8 (82.85 s), 10/10
  (81.46 s), 11/11 at `73a7d92` (82.06 s, job 125950604023).
- **Elevated gate: not run.** `C:\tmp\s54-gate1.ps1` (ASCII, parser-checked)
  runs two trees, A = `fca6303` and B = `73a7d92`, each with the release
  build, `cargo test --ignored --no-fail-fast` and the ingest suite with the
  marquees, and starts Docker Desktop first. Session 54 checked for its
  logs three times; none appeared.

## Test baselines at close

- `main` (`010a0d8` + this handoff): unchanged from Session 53.
- Branch tip `73a7d92`, non-elevated: 181 passed / 0 failed / 11 ignored;
  the SCM suite 11/11 in CI.

## Debts

- Closed: none (#137's fix is on #83, not landed).
- Opened (all on the open branches' code, live when they land):
  - **#142** — The service accepts any identity and trust-anchor paths
    (relative ones resolve against `System32`), with no installed-mode
    validation of SPEC-024 §Data contracts §2 and no custody check before it
    reads the identity (SPEC-017 Amendment 2026-10-27); an identity directory
    created under `CyberGuard\agent` would inherit Administrators' full
    control. Destination: install_ac_002/004/005 with #87.
  - **#143** — Log residuals: a write that fails after writing part of a
    record leaves that part as one invalid line; an active file already
    above 10 MiB (from a build before rotation) is archived whole.
    Destination: install_ac_010's next change, or accepted at F2's close.
  - **#144** — An ordinary user who creates `%ProgramData%\CyberGuard`
    before installation keeps the service from starting (fails closed,
    code 13); threat model R17 in #141's draft. Destination: SPEC-024's
    package (install_ac_011/015).
  - **#145** — Service paths no hosted runner exercises: a real `SHUTDOWN`,
    the deadline expiring in the capture teardown or at the watchdog, a
    persistent log failure under the SCM, a rotated production archive.
    Destination: the isolated Windows machine's experiments (install_ac_008,
    install_ac_016; #50/#59).
- Changed: **#137**'s destination → PR #83 (`73a7d92`), closing when it
  lands. **#141**'s destination → `review/s54-141-texts` (`b482cd5`): its
  fresh-context review after #81 lands, then Manuel's sí.
- Open at close: **54**.

## Waiting on Manuel

1. **The elevated gate**: `powershell -NoProfile -ExecutionPolicy Bypass
   -File C:\tmp\s54-gate1.ps1`, in an elevated PowerShell with the repo on
   any branch and a clean tree. If `main` moves before it runs (it moved
   with this close commit), Session 55 rebases the branches and re-points
   the script first (*How Session 55 resumes*).
2. **#141's texts**, after their fresh-context review: the owner-review
   branch `review/s54-141-texts`, read on GitHub; a sí on the text.

## How Session 55 resumes

1. Start protocol (CLAUDE.md). `main` is this handoff's close commit; open:
   PR #81 (`feat/s54-install-ac-007`, `fca6303`), PR #83
   (`feat/s54-install-ac-010`, `73a7d92`, based on #81's branch), and the
   branch `review/s54-141-texts` (`b482cd5`, no PR); 54 open debts; 22 ADR /
   23 SPEC. Check C:, RAM and Docker before builds.
2. **Rebase** both branches onto `main` (only this handoff's docs moved it),
   push them one commit per push (the first with `--force-with-lease`), CI
   ALL GREEN on each, and re-point `C:\tmp\s54-gate1.ps1`'s A and B to the
   rebased tips of #81 and #83 (or write `s55-gate1.ps1`), parser-checked,
   ASCII. Give Manuel the one line.
3. With the gate's logs green for both trees: land #81 (squash, the
   `git diff --cached --quiet` guard against its tip) then #83's two
   commits by cherry-pick with the tree guard, one push each, CI green
   between; close the PRs, delete the branches. Close #137 in the register.
4. Rebase `review/s54-141-texts` onto the new `main`, run the fixed
   SPEC/ADR-text reviewer on it, fix List A, push, and give Manuel the
   compare URL for his sí. Open the debt row for R17 only if #144 does not
   already carry it (it does).
5. Then, in order: #87 custody with #142 (install_ac_002/004/005), then
   packaging (install_ac_015, the .NET SDK and WiX first use reported with
   version and command before it runs). Keep the release freeze.

## Environment and memory

C: about 29 GB free at close; non-elevated builds with `-j 2`,
`CARGO_INCREMENTAL=0` and debug info off. The Bash tool's heredocs again
altered backslashes in edit scripts (memory `feedback-bash-heredoc-backslashes`):
write such scripts with the Write tool and run them with `node C:/tmp/...`
(a `/c/tmp/...` path resolves to `C:\c\tmp` in node). The session-close
auto-memory entry is `project-session-54-close.md`, indexed in `MEMORY.md`.
