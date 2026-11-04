# Handoff — Session 54

Session 54, 2026-11-03, continues F2 under SPEC-024. It runs the probe pull
request for the SCM/SYSTEM tests, implements **install_ac_007** (the agent
as a Windows service) and **install_ac_010** (its rotating JSON log, with
debt #137), each through a fresh-context review in two rounds, and drafts
**#141**'s texts for Manuel. The code went through two stacked pull
requests, both CI ALL GREEN. The first close (`266599f`) left them waiting
for the elevated gate; Manuel ran it on 2026-11-04, green on both trees, and
the session then **landed** them on `main` (*Landing after the elevated
gate*): install_ac_007 as `cc24b99`, install_ac_010 and #137 as `f4c6f28`
and `61b2f67`.

## Start and resulting state

- Start: `main` = `origin/main` = `010a0d8`, clean, no stash, only `main`
  on origin, no PR open. `main`'s tip CI green (`markdown-lint →
  success`). Catalogs 22 ADRs / 23 SPECs; open debts 50; Known CI debt
  none. Repo and `docs/handoff-session-53.md` agreed. C: 33 GB free, about
  1.1 GB of RAM free, Docker Desktop down (not needed: no local marquee).
- Resulting `main`: the first close commit `266599f`, the three landed
  commits `cc24b99`, `f4c6f28`, `61b2f67`, and the final close commit on
  them. PRs #81 and #83 closed, their branches deleted; `review/s54-141-texts`
  is the one branch left open (*Waiting on Manuel*).
- Open branches at the first close (`266599f`), each named with its reason:
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
- **The gate ran on the pre-rebase trees** (the first close's plan was to
  re-point it to the rebased tips first). Its evidence was carried to the
  rebased trees under *Elevated gate evidence validity* (*Landing after the
  elevated gate*). Rejected: a second elevated run on the rebased tips —
  about 25 minutes of Manuel's time to re-prove trees that differ only by
  four Markdown files no test reads, which the rule exists to cover.
- **The unelevated baseline is exit 1, not 9, on this host.** Found by the
  rule's condition 3; explained by S53's token finding; CLAUDE.md's
  environment fact updated. Rejected: leaving the fact as written (it would
  make the next condition-3 check look like an unexplained difference).

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
- **Elevated gate s54-gate1, green on both trees.** `C:\tmp\s54-gate1.ps1`
  (ASCII, parser-checked) runs two trees, A = `fca6303` and B = `73a7d92`,
  each with the release build, `cargo test --ignored --no-fail-fast` and the
  ingest suite with the marquees, and starts Docker Desktop first. The
  first close checked for its logs three times and none appeared; Manuel
  ran it on 2026-11-04 (`=== s54-gate1 done 2026-11-04T00:36:29.1152562+01:00
  ===`). Literal lines from the UTF-16 logs:
  - `net session exit: 0`; `docker engine: UP`, `ServerVersion=29.1.2`.
  - A: `HEAD  = fca63038fcd4af7f7e9d5c636c2bb3b3b6f6c384`, `TREE  =
    20e8e077f75fff662fa03cf2bb887e36ea443630`; `release exe:
    2026-11-04T00:16:03.8709543+01:00  commit: 2026-11-03T14:14:05+01:00`.
  - B: `HEAD  = 73a7d924737540f2cd21a384546736409c197974`, `TREE  =
    79acc55dfafdc90bd7126514de44654f4213272e`; `release exe:
    2026-11-04T00:26:41.0546181+01:00  commit: 2026-11-03T14:35:57+01:00`.
  - `--ignored`, each tree: 11 passed, 0 failed, no `FAILED` or `panicked`
    line: `auth_ac_012_logons_on_the_real_log`,
    `capture_ac_006_elevated_shutdown_leaves_no_session`,
    `net_ac_008_a_real_connection_is_reported_with_its_probe`,
    `net_ac_009_an_accepted_connection_and_the_filter`,
    `ac_004_cache_hit_terminate_matches_launch_byte_for_byte`,
    `ac_007_parent_pid_only_when_parent_dead_at_child_launch`,
    `ac_009_events_lost_under_deliberate_etw_buffer_pressure`, and
    install_ac_009's four real cases (`test result: ok. 4 passed; 0 failed;
    0 ignored; 0 measured; 8 filtered out`), `classify_token` among them.
  - vitest, each tree: `Test Files  85 passed (85)`, `Tests  503 passed |
    2 skipped (505)` (S53's baseline); A `Duration  475.31s`, B
    `Duration  467.06s`.
  - A: `detect_ac_001 captured image_file_name — probe:
    C:\Users\manul\AppData\Local\Temp\cg-detect-probe-iH5sbS\winword.exe;
    child: C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe`;
    `detect_ac_001 drained 47 rows in 1 cycles; capture checked at 35.321 s,
    drained at 38.638 s`; `"detect_marquee_elapsed_seconds":38.65`;
    `spec_005 captured image_file_name — launch: C:\Windows\System32\cmd.exe;
    terminate: cmd.exe`.
  - B: `detect_ac_001 captured image_file_name — probe:
    C:\Users\manul\AppData\Local\Temp\cg-detect-probe-3jGd4T\winword.exe;
    child: C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe`;
    `detect_ac_001 drained 49 rows in 1 cycles; capture checked at 35.28 s,
    drained at 39.652 s`; `"detect_marquee_elapsed_seconds":39.665`; the
    same `spec_005` line.
  - Sessions after each tree: `logman query CGAgent-KernelProcess -ets` →
    "No se encontró el Conjunto de recopiladores de datos." (none left).

## Landing after the elevated gate

- **Rebase.** `main` had moved by `266599f` (this handoff's first close,
  four Markdown files). `feat/s54-install-ac-007` → `13a7717`, `826fd90`;
  `feat/s54-install-ac-010` → `d8dfc8d`, `9406e91`. `git diff fca6303
  826fd90` and `git diff 73a7d92 9406e91` are each exactly `git diff
  010a0d8 266599f` (README.md, docs/debts.md, docs/handoff-session-54.md,
  docs/product/roadmap.md). Pushed one per push, the first of each branch
  with `--force-with-lease` against the old tip; CI **ALL GREEN** on each
  full SHA: `13a7717` and `826fd90` (markdown-lint, dependency-audit,
  ts-ci, rust-ci, deploy-ci), `d8dfc8d` and `9406e91` (markdown-lint, ts-ci,
  rust-ci, deploy-ci; dependency-audit not triggered).
- **Carrying the gate to the rebased trees** (CLAUDE.md *Elevated gate
  evidence validity*, different tree):
  1. The four changed files are Markdown outside `services/`, `agent/`,
     `dashboard/` and `rules/`; none is a runtime or test input: a grep of
     `services`, `agent`, `dashboard`, `.github`, `harness`, `infra` and the
     Taskfile for `debts.md`, `handoff-session`, `roadmap.md` and
     `README.md` finds only comments citing older handoffs and a bundle
     README written by `release-bundle.mjs`, not the root one.
  2. No dependency, build, workflow, harness, configuration or generated
     input changed (`--stat` above, no renames or deletions).
  3. The full non-elevated ingest suite (`C:\tmp\s54-unelev.sh`, release
     build then `pnpm test`, logs `C:\tmp\s54-unelev-<sha>.log`) at the
     tested and the rebased SHA of each pair: `fca6303`, `826fd90`,
     `73a7d92`, `9406e91` each `Test Files  4 failed | 80 passed | 1
     skipped (85)`, `Tests  4 failed | 498 passed | 3 skipped (505)`, the
     same four `FAIL` lines (identical digest). The four failures are the
     capture marquees, which on this host's token exit 1 before enrolling
     (`docs/handoff-session-53.md`, *The dev host's token controls ETW*), not
     the exit 9 CLAUDE.md described: explained, identical before and after.
     CLAUDE.md's environment fact now says so (this close).
  4. Same machine, same commands, the binary rebuilt from each tree.
- **Landed**, one push each, CI green between:
  - #81 by `git merge --squash` (guard `git diff --cached --quiet
    feat/s54-install-ac-007` passed) → **`cc24b99`**: rust-ci, ts-ci,
    markdown-lint, dependency-audit → success.
  - #83 by cherry-pick, `git diff --quiet <branch-sha> HEAD` passing before
    each push: `d8dfc8d` → **`f4c6f28`** (rust-ci, ts-ci, markdown-lint →
    success), `9406e91` → **`61b2f67`**.
  - PR #83 retargeted to `main` before #81's branch was deleted; PRs #81
    and #83 closed with a comment naming the landed SHAs; both branches
    deleted, local and origin.

## Test baselines at close

- `main` at `61b2f67` (tree = `9406e91`'s): non-elevated `cargo test
  --all` 181 passed / 0 failed / 11 ignored (on `73a7d92`, the same code);
  the SCM suite 11/11 in CI; ingest suite unelevated 4 failed / 498 passed /
  3 skipped (the four capture marquees, exit 1 on this host); elevated
  85 files, 503 passed, 2 skipped, `--ignored` 11 passed.

## Debts

- Closed: **#137**, by `f4c6f28` and `61b2f67` (PR #83 landed after the
  elevated gate).
- Opened (on the landed service's code):
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
- Changed: **#141**'s destination → `review/s54-141-texts`, rebased on the
  landed service and reviewed with a fresh context (*#141's texts*): Manuel's
  sí on the text.
- Open at close: **53**.

## #141's texts

`review/s54-141-texts` rebased onto `61b2f67` (`990837a`, `6854a1c`,
`aedc83e`, `ac78df3`), then the fixed SPEC/ADR-text reviewer (fresh context,
isolated worktree) at `ac78df3`; the fixes are `8b5a338`, pushed (no PR, so
no workflow ran; `markdownlint` 0 issues locally). Claude Code's own pass:
the citations it checked held but the two the reviewer found.

- **List A**, both fixed: ADR-0010's amendment cited §References' SPEC-005
  line as `:144` (its own inserted lines had moved it to `:166`); ADR-0019's
  reopen condition cited `roadmap.md:415-416` (F2) for F3's agents page
  (`:419-421`).
- **List B** and dispositions:
  1. ADR-0010's "does not validate" read as exhaustive → names #145's paths
     (fixed).
  2. R17 narrower than the repo: the console runbook's `New-Item` creates
     `C:\ProgramData\CyberGuard\agent` with an inherited ACL, so a service
     started on such a host is code 13 → R17 says so and sends that host's
     move to install_ac_013's migration (fixed).
  3. ADR-0010's Amendment 2026-11-02 names "the entry for the installer's
     elevated code" for the lock; the threat model puts it under Denial of
     Service and says so → recorded: the 2026-11-02 text is a ratified
     record, and the new amendment states where the entry is.
  4. #142 missing from the threat model → noted in its context (fixed).
  5. R17 without its debt → names #144 (fixed).
  6. The Amendment 2026-10-15's "R1–R16 below" beside a line now pointing to
     R17 → recorded: the pointer names the Amendment 2026-11-03.
  7. install_ac_016's runbook in the present tense → "not yet written, is
     to" (fixed, three places).
  8. SPEC-024 said its disposition changes no ADR, yet ADR-0020 §2's clause
     is amended → recorded and put to Manuel (*Waiting on Manuel*).
  9. `Last updated` 2026-11-03 → re-dated at landing if the sí falls on
     another day.
  10. The threat model adds an accepted threat (an upstream-compromised WiX
      release) where the 2026-10-15 policy accepted none → a security-posture
      decision on Manuel's list, put to him explicitly.
  11. WiX mitigations read as built → "To be mitigated… none of the
      following exists yet" (fixed).
  12. The texts assume #81/#83 landed → true since `cc24b99`/`61b2f67`.
  13. ADR-0020 and the threat model differ on the dump's contents → user
      SIDs added (fixed).
  14. ADR-0010's "own process" wording (fixed).
  15. The register named `b482cd5` → no SHA now (this close).
  16. No new catalog edge → consistent with earlier SPEC-024 amendments.

## Waiting on Manuel

1. **#141's texts**: the compare
   `https://github.com/AlmanInDaHouse/CyberGuard_Enterprise/compare/main...review/s54-141-texts`,
   five commits, each `NOT YET RATIFIED`. Three points to decide in it, all
   his: ADR-0020 §2's clause on dumps is amended (SPEC-024 had said no ADR
   changes); the threat model accepts an upstream-compromised WiX release
   (the first threat accepted since 2026-10-15); ADR-0010 states LocalSystem
   runs in CI. A sí on the text, or the changes he wants.

## How Session 55 resumes

1. Start protocol (CLAUDE.md). `main` is this handoff's final close commit
   on `61b2f67`; no PR open; one branch, `review/s54-141-texts` (`8b5a338`,
   no PR); 53 open debts; 22 ADR / 23 SPEC. Check C:, RAM and Docker before
   builds.
2. If Manuel has given his sí on #141: rebase the branch onto `main`, strip
   `NOT YET RATIFIED` from each subject, re-date `Last updated` if needed,
   land the commits by cherry-pick with the tree guard (docs, so straight
   to `main` once `markdownlint` passes), close #141 in the register, delete
   the branch. If he asked for changes, make them on the branch first.
3. Then #87 custody with #142 (install_ac_002/004/005): contract check,
   tests seen red, branch and PR, one commit per push, the probe PR for any
   new SCM/SYSTEM test, the fresh-context review, and the elevated gate,
   since it touches the identity on the capture path.
4. Then packaging (install_ac_015): the .NET SDK and WiX first use reported
   with exact version and command before it runs. Keep the release freeze.

## Environment and memory

C: about 28 GB free at close; non-elevated builds with `-j 2`,
`CARGO_INCREMENTAL=0` and debug info off. The Bash tool's heredocs again
altered backslashes in edit scripts (memory `feedback-bash-heredoc-backslashes`):
write such scripts with the Write tool and run them with `node C:/tmp/...`
(a `/c/tmp/...` path resolves to `C:\c\tmp` in node). The session-close
auto-memory entry is `project-session-54-close.md`, indexed in `MEMORY.md`.
