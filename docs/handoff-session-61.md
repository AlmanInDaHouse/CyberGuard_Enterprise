# Handoff — Session 61

Session 61, 2026-11-10, started C5 of the package. It landed SPEC-024's
Amendment 2026-11-10, which Manuel approved, and the first piece of code
under it, the Windows version floor's check (item 1). The rest of C5 did not
start: the staging, the record and the reversal, the SCM client, configure
and its observer, and the verbs of the executable. It would not have fitted
this session at the quality bar, so the session closes at a green commit
with the plan below.

## Start and resulting state

- Start: `main` = `origin/main` = `28db599`, clean, no stash, no branch but
  `main`, no PR open; tip CI green (markdown-lint, the rest skipped by
  path); 22 ADRs / 23 SPECs; open debts 55 (55 rows); Known CI debt none.
  The repo agreed with the handoff. Memory before any gate: 1.7 GB free of
  15.8, with the CyberGuard dev stack and two of Manuel's own containers
  (`aura_cache`, `aura_db`) up and `vmmemWSL` holding about 5 GB. Claude
  Code stopped the five `cyberguard-dev-*` containers, which freed about
  0.5 GB, and left Manuel's containers alone. `C:` had 23 GB free.
- Resulting `main`, in order:
  - `db594a8`, the amendment;
  - `dc858ad`, the amendment's approval record;
  - `70a2861`, the floor;
  - this close's commit.

  No PR is open. No branch but `main` is left, and no worktree.
- Catalogs: 22 ADRs / 23 SPECs; SPEC-024 gains its Amendment 2026-11-10
  (`docs/specs/README.md`).

## What landed

### SPEC-024 Amendment 2026-11-10 (`db594a8`, `dc858ad`)

It answers the questions C4 left and C5 needs answered before its code.

- **Item 1, the floor.** Code 20 below Windows 10.0 build 17763, read with
  `RtlGetVersion`. `IsWow64Process2` is resolved after that check. A Windows
  before 10 does not load Rust's standard library, so setup.exe reports its
  package's failure there, not 20.
- **Item 2, #151's readings,** stated as the code takes them. A
  `%ProgramFiles%` that Windows does not report is 40.
- **Item 3, the conflicting service (26).** A `CyberGuardAgent` service
  whose type, account or command line is not the product's. It is checked
  on every route, and on the migration route a service in any state but
  `STOPPED` is 26 too.
- **Item 4, the downgrade (29).** Read from the installed binary's version
  resource, on every route.
- **Item 5, the staging and `setup-attempt.json`.**
  - The record's form: `format`, `created`, `fingerprints` and
    `service_running`, with no path and no secret.
  - `<P>` and the data root are held open without following them and
    without sharing deletion, and are checked through those handles.
  - Order: the directories, the record, the anchor, the configuration.
  - Removal: the staged files, the record, then the empty directories.
- **Item 6, the stop (31)** within 75 seconds.
- **Item 7, the reversal** as `cg-setup reverse`. It reads the host as
  preflight does and changes nothing on a host preflight would not touch.
- **Item 8, configure.**
  - Its checks of the record and the staged files.
  - The installed binary's version, and 45 from a pending rename onto it.
  - The failure-actions flag set to false.
  - The start arguments.
  - The observer: `agent identity ready`, then `signed heartbeat sent`,
    with the log position followed across rotation.
- **Item 9, the command line:** `preflight`, `configure`, `reverse` and
  `--version`, and a version resource of its own.
- **Tests:**
  - `cg-setup`'s SCM tests follow the Amendment 2026-11-01, point 4.
  - The real `cg-setup` runs `preflight` and then `reverse` in
    `package-ci`, with every privilege but `SeChangeNotifyPrivilege`
    deleted from its token.
- **CLAUDE.md** (on Manuel's list, approved with the text):
  - The Windows CI job's rule names the tests of the agent or of
    `cg-setup`.
  - The local gate's exceptions name both.
  - Control §1 includes `cg-setup`'s SCM tests.
  - The package bullet and control §1 say "the real `cg-setup` executable".

### The floor's check (`70a2861`, PR #99)

- **`agent/cg-setup/src/platform.rs`:**
  - `WindowsVersion` (ordered major, minor, build) and `FLOOR` (10.0.17763).
  - `check_floor`, code 20 with a line that names the version read.
  - `windows_version`, through `RtlGetVersion`.
  - `check_platform`: the floor first, then `IsWow64Process2` through
    `GetModuleHandleW` and `GetProcAddress`. A missing export is 20.
- **`agent/cg-setup/Cargo.toml`:** windows-sys features
  `Wdk_System_SystemServices` and `Win32_System_LibraryLoader`; no new
  crate, `Cargo.lock` unchanged.
- **Tests:**
  - `install_ac_002_setup_inputs.rs`,
    `package_preflight_the_floor_is_windows_10_build_17763`: below, at and
    above the floor.
  - `package_preflight.rs`, `..._this_hosts_version_is_windows_own`:
    `RtlGetVersion` against the registry's `CurrentBuildNumber`, which
    `GetVersionEx` would not match without a manifest.
  - `package_preflight.rs`, `..._is_wow64_process2_is_resolved_at_run_time`:
    the test executable's import table names `kernel32.dll!GetProcAddress`
    and not `IsWow64Process2`.

## Red evidence (CLAUDE.md compensating control 1)

- `check_floor` and `windows_version` as `unimplemented!()`:
  `package_preflight_the_floor_is_windows_10_build_17763` failed at
  `not implemented: check_floor`, and
  `package_preflight_this_hosts_version_is_windows_own` at
  `not implemented: windows_version`.
- The implementation with a static `IsWow64Process2` call put back:
  `package_preflight_is_wow64_process2_is_resolved_at_run_time` failed with
  `IsWow64Process2 is imported statically`. The fresh reviewer, with a PE
  reader of its own, found `kernel32.dll!IsWow64Process2` in the pre-change
  build's import table and not in the new one.

## Decisions

### Anticipated (Claude Code)

- **One amendment, through owner review.** Its items 2, 3 and 7 change the
  operator contract that Manuel approved in the Amendment 2026-11-09.
  "Unclear is Manuel's" applies. Discarded: a self-amendment for the
  internal parts (the record's form, the verbs), which would have split one
  coherent contract.
- **The conflicting service (26):** type, account and command line, with
  the paths compared as the service compares them. The start type is not
  compared: a disabled service is 41. Discarded: comparing the start type
  too, which the MSI sets.
- **The downgrade (29) on every route**, the migration's included: the
  installed binary and the SCM are not files under `<P>`.
- **The record names keys, not paths,** so a record cannot direct a
  deletion elsewhere. Discarded: paths in the record.
- **The record before the staged files,** so a crash never leaves a staged
  token unnamed by a record. Discarded: the record last.
- **45 from `PendingFileRenameOperations` and `PendingFileRenameOperations2`,**
  the source Windows Installer writes for an in-use file. Discarded: leaving
  45 to Burn's 3010.
- **The import test on the test executable,** which links `check_platform`.
  The shipped binary does not exist yet; the test moves to
  `CARGO_BIN_EXE_cg-setup` when it lands.

### Reactive corrections

- **The first review (fixed prompt and two lenses, 22 agents)** found four
  blocking problems, each fixed in the text:
  - The record's directories could never be removed, because the record
    inside them was removed last. The order is now the staged files, the
    record, then the directories.
  - Point 4 covered `cg-agent`'s tests alone. It now covers `cg-setup`'s,
    with CLAUDE.md.
  - Below Windows 10, code 20 is not reachable: Rust 1.93's std imports
    `WaitOnAddress` and `ProcessPrng`.
  - The reversal deleted by name through an unchecked `<P>`, so an ordinary
    user could turn it into an elevated delete. It now reads the host as
    preflight does.

  Also fixed:
  - the failure-actions flag is set to false, not left unset;
  - 40's new causes are named in the Effect;
  - configure takes no `--bootstrap`;
  - the item numbers are qualified.
- **The second review (fixed prompt and an adversary lens)** found three
  blocking problems, each fixed:
  - A junction planted after the host read was followed. `<P>` and the data
    root are now held open, sharing no deletion, and checked through those
    handles, and created directories are created new.
  - The *Environment facts* sentence described a feature not yet in the
    repo. It was removed; it lands with that feature.
  - The local gate's exception list and control §1 did not name
    `cg-setup`'s tests.

  Also fixed: the `!` prefix of a pending rename, the ASCII case rule, 40
  for a reversal whose names cannot be read, and two more disclosed cases.
- **The third review (fixed prompt)** found four blocking problems, each
  fixed:
  - a wrong citation (the Amendment 2026-11-04's item 2, not item 1);
  - configure did not hold the data root that the Effect said every reader
    holds;
  - one host state had two exits in the reversal;
  - CLAUDE.md's two "only in this job" rules overlapped.
- **The test's registry path was mangled** by a Bash heredoc (the
  auto-memory's backslash rule) and fixed with the Edit tool.
- **`db594a8` landed without the approval lines.** They were edited after
  `git merge --squash` and never staged. Its tree is the reviewed `dfbc44c`
  (`git diff --quiet` held), but its text still said "awaits his approval"
  and its message said the approval was recorded. `dc858ad` added the two
  lines. `main`'s history was not rewritten.

### Manuel's decisions in chat

- 2026-11-10, on the amendment and CLAUDE.md read at `dfbc44c`: "Sí,
  apruebo".
- 2026-11-10, on the wizard, which no C-step held: "C7, con el bundle
  (Recomendado)". C5 keeps the unattended route alone, and the wizard is
  built with the bundle that launches it.

## Self-review (CLAUDE.md compensating control 5)

### The amendment

- Claude Code's pass checked each claim against itself and its citations,
  then ran three fresh rounds, each recorded above under *Reactive
  corrections*.
- Every List A entry of the fixed prompt was fixed. The third round's
  fixes are textual and were checked against the lines they cite.
- Disclosed in the text, as within "elevated administration is not a
  threat boundary":
  - a record a crash left behind drives a later reversal;
  - a second setup during an adoption replaces the staged files, which the
    service then refuses with code 2 or 6;
  - a configure 40 or 45 before its start leaves the service stopped;
  - a reversal on a host that became untrusted leaves the staged files.
- List B, open for the implementation:
  - whether Burn rolls back a preflight that failed itself (C7's bundle);
  - `--version` and the floor's order;
  - the wizard's trigger in item 9 (C7);
  - `service_running` for `STOP_PENDING` (stated);
  - the development versions of the two `Cargo.toml` files kept equal;
  - `package-ci`'s test run before any MSI in its job;
  - the LocalSystem spelling.

### The floor (contract: the Amendment 2026-11-10, item 1, and its *Tests*' first sentence)

| Criterion | Test | Verdict |
|---|---|---|
| 20 below 10.0.17763, compared below, at and above | `install_ac_002_setup_inputs.rs`, `..._the_floor_is_windows_10_build_17763` | covered |
| Read with `RtlGetVersion` | `package_preflight.rs`, `..._this_hosts_version_is_windows_own` | covered |
| The line names the version read | the floor's test | covered |
| `IsWow64Process2` resolved at run time | `..._is_wow64_process2_is_resolved_at_run_time` | covered on the test executable; the shipped binary with the verbs |
| Before any other check | none; `check_platform`'s order and `preflight()`'s | weak: code only until the verbs exist |
| A Windows 10 below the floor reaches the check | none end to end | weak: no guard on future imports |

The fresh reviewer (fixed code prompt, isolated worktree, on `70a2861`)
found nothing contradicted and the same weak rows. Its List 3 names only the
two features, the code-20 path for a missing export, the new public items
and the `reg.exe` query. Dispositions: the import test moves to the shipped
binary in C5's verbs commit, and the order is tested there. No further pass
followed, because no fix came from it.

## Gates and CI

- Local Rust gate on `70a2861`'s tree (worktree, shared target, `-j 2`):
  - `FMT_OK`;
  - clippy `Finished`, no warning;
  - "binaries 87 passed 444 failed 0 ignored 11";
  - "agent version matrix: all steps passed";
  - `cargo audit` exit 0, "2 allowed warnings" (RUSTSEC-2026-0440 and
    RUSTSEC-2026-0386, as on `main`).
- The elevated gate does not apply: the diff touches none of its paths.
- CI:
  - the owner-review branch had no PR, so no workflow ran;
  - `db594a8` and `dc858ad`, markdown-lint `success`;
  - PR #99 at `70a2861`: rust-ci, ts-ci, dependency-audit and deploy-ci
    `success`;
  - `main` at `70a2861`: rust-ci, ts-ci and dependency-audit `success`
    (deploy-ci runs on pull requests alone). ALL GREEN.

## Landing

- `db594a8`: a squash of the owner-review branch onto `main`, guard
  `git diff --quiet dfbc44c db594a8` held. `dc858ad`: the approval lines.
  The branch is deleted, local and on origin.
- `70a2861`: a fast-forward of `main` from `dc858ad`, its base, so the
  landed SHA is the one CI ran. PR #99 is closed, and its branch and
  worktree are removed.

## Test baselines at close

- Non-elevated `cargo test --all`: 444 passed, 0 failed, 11 ignored
  (cg-setup 37).
- The SCM suite in CI: 35 tests, unchanged.
- Elevated `--ignored` and vitest: not run this session, unchanged at S58's.

## Debts

- Closed: **#151**, by `db594a8` (the amendment's item 2).
- Opened: none.
- Unchanged:
  - **#87**, its `cg-setup`, MSI and bundle halves;
  - **#150**, which reopens if C5 reads the bootstrap file or the anchor
    again (it does not: configure reads no input file, item 8).
- Open at close: **54**.

## Waiting on Manuel

Nothing. The release-tag freeze stays.

## How Session 62 resumes

1. Run the start protocol (CLAUDE.md). `main` is this handoff's close
   commit. There is no PR. The counts are 54 open debts and 22 ADRs / 23
   SPECs. Check the memory before a full gate: S61 started at 1.7 GB free
   with Docker's WSL VM holding about 5 GB.
2. Do the rest of C5 under the Amendment 2026-11-10, one commit per push,
   each through a PR, contract tests first. Suggested order:
   - **a. Staging, the record and the reversal (items 5 and 7), as a
     library.** cg-agent's custody helpers become public where `cg-setup`
     needs them: `open_existing`, `delete_through`,
     `create_new_directory`, a held directory handle with no delete
     sharing, `file_id` and `file_information`. The record is JSON with
     `serde_json`, which cg-agent already has. The tests run unelevated on
     temporary roots, the current user standing for Administrators, with a
     failure injected at each step. They cover the junction planted after
     the read, a directory under a staged name, the record's form, the
     reversal on each state, and the order of removal.
   - **b. The SCM client (items 3, 6 and 8's failure actions and start),
     taking the service's name.** It needs the
     `Win32_System_Services` feature. It carries a `scm-tests` feature and
     `[[test]]` targets in `agent/cg-setup/Cargo.toml`, and rust-ci's
     Windows step gains `cargo test -p cg-setup --features scm-tests ...`.
     The *Environment facts* sentence about them lands in that commit. The
     tests run against test services (`CgSetupTest-<pid>-<n>`) for the
     conflict, the stop within 75 s and the retry, the failure actions read
     back (the flag false) and the start's arguments. Their red run is on a
     probe PR with `continue-on-error`, closed without landing.
   - **c. The downgrade (item 4) and configure's version and 45 (item 8).**
     `GetFileVersionInfoW` and `VerQueryValueW` read the installed binary;
     the tests use the agent's built binary and a copy whose version is
     changed. `PendingFileRenameOperations` and `...2` are read with
     `Win32_System_Registry`, with the parsing tested on literal values
     (the `!` and `\??\` prefixes).
   - **d. The observer (item 8).** The log position, rotation followed by
     file ID, the two lines in order, and 42, 43 and 44. The tests run on
     temporary log directories written as the rotation writes them. The
     agent's test that pins `signed heartbeat sent` (and `agent identity
     ready`) lands in cg-agent.
   - **e. The executable and its verbs (item 9).** `src/main.rs` and a
     `build.rs` that shares the agent's version resource encoding with its
     own `OriginalFilename` and no message table. `--version`. The import
     test reads `CARGO_BIN_EXE_cg-setup`, and the floor's order is tested
     on the binary.
   - **f. The SCM test of code 10 on the adoption route with the real
     lock,** in cg-agent's `package_adoption_service`.
   - **g. `package-ci`'s host test:** the real `cg-setup` `preflight` then
     `reverse`, with a restricted token that deletes privileges through
     `PrivilegesToDelete`. `package-ci` builds `cg-setup` (its paths gain
     `agent/**`, `Cargo.*` and the toolchain), the guard follows
     `refuse_unless_disposable_runner`, and the red run is on a probe PR.
3. Then C6 to C10 as `docs/handoff-session-59.md` orders them, with the
   wizard in C7 (Manuel, 2026-11-10).
4. Keep the release freeze.

## Environment and memory

- With Docker's WSL VM up, 2.2 GB stayed free after the dev stack stopped.
  The gate ran with `-j 2` and was not killed.
- Work ran in a second worktree (`../cg-s61`) with the main checkout's
  target directory, while the main checkout held the owner-review branch
  that the reviewers read.
