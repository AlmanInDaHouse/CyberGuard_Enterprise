# Handoff — Session 63

Session 63, 2026-11-11, did steps b and c of C5's plan
(`docs/handoff-session-61.md#how-session-62-resumes`), under SPEC-024
Amendment 2026-11-10:

- step b, `cg-setup`'s client of the service control manager (items 3, 6
  and 8's failure actions and start);
- step c, the installed binary's version (item 4 and item 8's version).

Both are libraries, as step a was. Steps d to g did not start: the observer
(d) is a pull request of its own, with a test service that writes logs and
the agent's test of its two lines. The session closes at a green commit
with the plan below.

## Start and resulting state

- Start:
  - `main` = `origin/main` = `14eb401`, clean, with no stash, no branch but
    `main` and no PR open;
  - the tip's CI was green (markdown-lint; the rest was skipped by path);
  - 22 ADRs / 23 SPECs, 55 open debts (55 rows) and no Known CI debt. The
    repo agreed with the handoff.
- Before any gate, 5.1 GB of 15.8 GB of memory was free, with Manuel's
  `aura_cache` and `aura_db` up, and `C:` had 47 GB free (`target\` held
  4.5 GB). The gates ran with `-j 2`, `CARGO_INCREMENTAL=0` and the
  profiles' debug info off.
- Resulting `main`, in order:
  - `f081319`, the SCM client;
  - `341f534`, the first review's fixes;
  - `17082b6`, the second and third reviews' fixes;
  - `0ba4489`, the installed binary's version;
  - this close's commit.

  PRs #102 (b) and #104 (c) are closed and their branches deleted, local
  and on origin. The probe PRs #101 and #103 are closed without landing, and
  their branches are deleted. No other branch is left, and no worktree.
- Catalogs: 22 ADRs / 23 SPECs, unchanged.

## What landed

### The SCM client (`f081319`, `341f534`, `17082b6`, PR #102)

- **`agent/cg-setup/src/service.rs`**, with no host access:
  - `not_the_products` holds item 3's comparison: type exactly own process
    (`0x10`), account `LocalSystem` (ASCII case ignored), and the command
    line `"<binary>" --service --config "<config>"`. The two paths are
    compared equal but for ASCII case (`is_fixed_path`) and the rest
    exactly; the start type is not compared;
  - `start_arguments`: item 6's adoption arguments with the record's
    fingerprints, none with an empty list (item 8);
  - `migration_arguments` gives the Amendment 2026-11-05's;
  - the constants: the 75 s, the 60/120/300 s restarts and the 86400 s
    reset.
- **`agent/cg-setup/src/scm.rs`.** Each call takes the service's name and
  opens the service with the access it needs alone:
  - `Manager::open`: 40 if the SCM cannot be opened.
  - `check_product_service(scm, name, binary, config, route)`:
    - a configuration that cannot be read, or is not the product's, is
      26;
    - its state is read through a handle of its own, and a state that
      cannot be read is 40;
    - on the migration route, any state but `STOPPED` is 26;
    - no service is `Product::Absent`;
    - `Product::running()` is item 5's `service_running`, `STOP_PENDING`
      included.
  - `stop(scm, name)`:
    - no service, or `STOPPED`, needs no stop;
    - each control is sent on a thread of its own through a handle for
      `SERVICE_STOP` alone, because `ControlService` can block for 30 s;
    - the state is read every 250 ms through a handle for the status alone;
    - the control is sent again while the service cannot accept it (1052,
      1053, 1061, 1062), and any other error is 31 at once;
    - only a reading taken before the end, 75 s from the first request,
      proves `STOPPED`; otherwise 31.
  - `set_failure_actions(scm, name)`:
    - restart after 60, 120 and 300 s, the count reset after 86400 s;
    - a reboot message and a command set earlier are deleted;
    - the flag is false;
    - a failure is 40.
  - `start(scm, name, arguments)`: 41 if the SCM refuses.
- **Tests:**
  - `agent/cg-setup/tests/package_service_form.rs` holds 8 tests, run in
    every gate: the comparison, the arguments read back by the agent's own
    `parse_start`, and the constants.
  - `agent/cg-setup/tests/package_setup_service.rs` with `setup_service/`
    holds 12 SCM tests. They run against services of their own,
    `CgSetupTest-<pid>-<n>`:
    - the conflict on both routes;
    - an unreadable configuration (26) and an unreadable state (40);
    - `service_running` in `START_PENDING`, `RUNNING` and `STOP_PENDING`;
    - the stop: prompt, sent again during `START_PENDING` and while
      `RUNNING` accepts no control, a 65 s stop proved, 31 at 75 s from a
      first request the service refused, and 31 at 75 s while a control
      is still outstanding at the end;
    - the failure actions;
    - the start with the adoption's, none and the migration's arguments;
    - 41 for a disabled, a running and an absent service.
  - The test target has no libtest harness (`harness = false`). The SCM
    starts the same test executable as the service the tests control, with
    the product's command line form and a JSON behaviour file. Its `main`
    runs the tests otherwise, selected as libtest selects them. The suite
    takes about 300 s in CI.
- **Build and CI:**
  - `agent/cg-setup/Cargo.toml` gains the test-only `scm-tests` feature,
    which only that test target requires, and `Win32_System_Services`.
  - rust-ci's Windows job gains the step
    `cargo test -p cg-setup --features scm-tests --no-fail-fast --test package_setup_service -- --test-threads=1`.
  - CLAUDE.md's *Environment facts* say so.

### The installed binary's version (`0ba4489`, PR #104)

- **`agent/cg-setup/src/version.rs`:**
  - `read_installed`:
    - it reads the binary's own version resource
      (`GetFileVersionInfoExW` with `FILE_VER_GET_NEUTRAL`,
      `VerQueryValueW`): the file version, and the `ProductVersion` of its
      first translation;
    - no binary is none, and a binary whose version cannot be read is 40.
  - `downgrade(binary, own)`: above `X.Y.Z.0` is 29 (item 4).
  - `configure_version(binary, own, own_text)` (item 8):
    - this package's binary is `X.Y.Z.0` with `ProductVersion` equal to
      `cg-setup`'s own version;
    - otherwise 45 when Windows holds a rename onto it for the next
      restart, a binary whose version cannot be read included;
    - otherwise 40, renames that cannot be read included.
  - `pending_renames` reads `PendingFileRenameOperations` and `...2` each on
    its own, again when a value grows between its size and its read.
    `rename_destinations` reads pairs, and an empty source ends the list.
    `renames_onto` compares without the `!` and `\??\` prefixes, ignoring
    ASCII case only.
  - A refusal quotes the binary's `ProductVersion` escaped and bounded, so
    it stays one line.
  - `SETUP_VERSION` is `cg_agent::AGENT_VERSION`, which is built from the
    same input.
- **Tests:** `agent/cg-setup/tests/package_version.rs`, 12 tests run
  unelevated:
  - the comparisons, and the parsing on literal `REG_MULTI_SZ` values;
  - copies of the test executable without and with a version resource
    written by `UpdateResourceW`, whose numeric product version differs from
    its file version; a bad signature;
  - the two compositions;
  - the agent's own build read back as this package's;
  - the one-line refusal.
- `Cargo.toml` gains `Win32_System_Registry`. The crate's README and doc
  name both modules.

## Red evidence (CLAUDE.md compensating control 1)

- **Step b, pure tests.** With `product_command_line`, `not_the_products`,
  `adoption_arguments` and `migration_arguments` as `unimplemented!()`:
  "1 passed; 7 failed", each at `not implemented`. The one that passed is
  the constants.
- **Step b, SCM tests, probe PR #101 (`508f73d`).** The client's four
  functions were stubs and the step had `continue-on-error`: "0 passed; 11
  failed". Each test failed at `not implemented: <the function it tests>`,
  after its services were created and started.
- **Step b, the reviews' tests, probe PR #103 (`11b1ae8`).** `17082b6`'s
  tests ran against `f081319`'s client: "9 passed; 3 failed". The three
  failures are the three defects fixed:
  - `left: 26, right: 40`, the unreadable state;
  - "31 at 75 seconds while a control blocks, took 80.2513674s";
  - the command and reboot message left after the failure actions.

  An earlier run of that probe (`532f994`) had the blocking test passing
  against the old client. Its handler blocked 45 s, so no control was
  outstanding at the end. The handler now blocks 80 s, and the run above
  is red.
- **Step c.** With the module's eight public functions as `unimplemented!()`,
  9 of 9 failed at `not implemented`. Then three mutations were each seen
  failing:
  - reading the product version fields: 3 failed;
  - the compositions as stubs: 2 failed;
  - the unescaped `ProductVersion`: 1 failed.

  Removing the signature check survives, because `VerQueryValueW` already
  refuses a fixed part without its signature. The 40 is still asserted.

## Decisions

### Anticipated (Claude Code)

- **Libraries first, wired in step e.**
  - Discarded: calling the client from `preflight()` now. That needs the
    verbs, and `<ProgramFiles>` from the OS for the installed binary's path.
- **The test service is controllable.**
  - It first was a `[[bin]]` that required `scm-tests`. Discarded: the agent
    with a configuration that refuses, which stops promptly and never late
    or never; and `cmd.exe`, which never reports `RUNNING`.
  - Its line moved into the test target; see the reactive corrections.
- **One handle per need.** Each call opens the service for its own access,
  so a denied right fails that call alone. Discarded: one handle with every
  right.
- **`LocalSystem` with ASCII case ignored**, since Windows account names
  are not case-sensitive.
  - Discarded: an exact comparison, which refuses a service the SCM runs as
    SYSTEM.
  - Discarded: a lookup to a SID, which would widen the text's literal
    `LocalSystem` to its aliases.
- **The stop's codes.** It sends again on 1052, 1053, 1061 and 1062; any
  other error is 31 at once, since it cannot be proved stopped. A state
  that cannot be read during the stop is 31 (not proved stopped), while at
  item 3's check it is 40 (the host could not be read).
  - Discarded: waiting out the 75 s on every error.
- **The failure actions delete an earlier command and reboot message**
  (empty strings), so what remains is item 8's. Discarded: null pointers,
  which leave them.
- **The version's sources:**
  - `cg-setup`'s version is `cg_agent::AGENT_VERSION`, built from the same
    input. Discarded: a second `option_env!` in `cg-setup`.
  - The `ProductVersion` is the first translation's, from the neutral
    resource. Discarded: the localized lookup, which may read a `.mui`
    satellite.
  - A binary whose version cannot be read, with a rename held onto it, is
    45. Renames that cannot be read count as none, so the result is 40.

### Reactive corrections

- **The first review of `f081319`.** The fixed prompt, a Windows SCM lens
  and a test-falsification lens, with each lens finding refuted by a fresh
  agent: 11 agents. Fixed in `341f534`:
  - The 75 s could be overrun by a `ControlService` that blocks (confirmed).
    Each control now goes on its own thread, and only a reading taken before
    the end proves `STOPPED`.
  - The `[[bin]]` required the test-only feature, which the Amendment
    2026-11-01, point 4, gives to test targets alone ("Only test targets
    require the test-only Cargo feature they need"). The test target became
    harness-less and is its own service.
  - `adoption_arguments` gave `--adopt-staged-config` alone for an empty
    list. It is now `start_arguments`, which gives none.
  - A command and reboot message set earlier were left.
  - New tests: the 75 s from a refused first request (the origin was not
    pinned, confirmed); a blocking control; `running()`.
- **The second pass, over `14eb401..341f534`.** The fixed prompt and a
  Windows/concurrency lens with refuters. Fixed in `17082b6`:
  - The conflict check opened configuration and status together, so a
    denied status was 26 (confirmed). It is now 40, through its own handle.
  - The harness-less runner took libtest's option values as filters and ran
    every test on `--list` (confirmed). It now reads filters, `--skip`,
    `--exact`, `--ignored` and `--list` as libtest does.
  - The harness checks that a name it treats as absent is absent.
  - It refuses beside `CyberGuardAgent` before writing a behaviour file.
  - The stop's last sleep ends at the bound.
- **The third pass, over `341f534..dbd62c6`.** The fixed prompt, amended
  into `17082b6` before its push:
  - A service deleted between the two opens is `Absent`.
  - A stale doc.
  - A test's name now says what it asserts.
- **The probe's blocking test** was hardened from 45 s to 80 s (above).
- **Step c's first run.** `VerQueryValueW` gives a string's length in
  characters, so the reader read "1.2" for "1.2.3". It now takes the unit,
  and every value is kept within the block.
- **The review of step c.** The fixed prompt and a Win32/parsing lens with
  refuters, then a fresh fixed-prompt pass:
  - a value that grows between reads is read again (confirmed);
  - the test resource's product version differs from its file version
    (confirmed);
  - the neutral resource;
  - the two compositions;
  - the one-line refusal;
  - both rename values read each on its own;
  - the README.
- **ts-ci on PR #104** failed at the dashboard job's "Install
  dependencies": corepack could not fetch `pnpm-10.33.0.tgz` (`read
  ECONNRESET`), a network failure of the runner. The failed job was rerun
  (attempt 2) and succeeded. The change touches no TypeScript.

## Self-review (CLAUDE.md compensating control 5)

### Step b (contract: the Amendment 2026-11-10, items 3, 5's `service_running`, 6 and 8's failure actions and start, its *Tests* on SCM tests; the Amendment 2026-11-09, items 6 and 8; the Amendment 2026-11-01, point 4)

| Criterion | Test | Verdict |
|---|---|---|
| A configuration that cannot be read is 26 | `setup_service/mod.rs`, `package_scm_an_unreadable_configuration_is_26_and_an_unreadable_state_40` | covered for the open. A `QueryServiceConfigW` failure after a successful open cannot be produced: weak. |
| Type, account, command line; paths but for ASCII case; start type not compared | `..._each_difference_..._is_26`, `..._of_the_products_configuration_is_no_conflict`, `package_service_form.rs` | covered |
| Every route; migration and not `STOPPED` is 26 | each item-3 test on both routes; `..._service_running_in_every_state_but_stopped` | covered at the library. "Every route" of preflight is step e. |
| No service is no conflict | `..._no_product_service_..._needs_no_stop` | covered |
| SCM that cannot be opened is 40 | none | missing: not producible on the runner without changing the SCM's own security |
| `service_running`, `STOP_PENDING` included | `..._service_running_in_every_state_but_stopped` | covered. Its use in the record is step e. |
| The stop: sent if not `STOPPED`, sent again while not accepted, 75 s from the first request, 31 | `..._stop_proves_...`, `..._sends_the_control_again_...`, `..._waits_75_seconds_and_no_more`, `..._bound_holds_while_a_control_blocks` | covered. "31 after item 5's removal" is step e. |
| Failure actions with the flag false | `..._failure_actions_are_set_and_their_flag_cleared` | covered |
| The start's arguments, none with an empty list | `..._start_passes_its_arguments_or_none`, `package_service_form.rs` | covered |
| 41 | `..._a_start_the_scm_refuses_is_41` | covered |
| Test venue: own feature, rust-ci Windows alone, fail without acting off the runner, probe | `Cargo.toml`, `rust-ci.yml`, the harness's refusals, #101, #103 | covered |

### Step c (contract: the Amendment 2026-11-10, item 4 and item 8's first bullet; the Amendment 2026-11-01, point 2)

| Criterion | Test (`tests/package_version.rs`) | Verdict |
|---|---|---|
| Above `X.Y.Z.0` is 29; none is none; unreadable is 40 | `package_version_a_downgrade_is_29`, `..._preflights_downgrade_check`, `..._the_installed_binarys_version_resource` | covered |
| `X.Y.Z` is `cg-setup`'s build version | `..._the_setup_is_the_agents_version`, `..._the_numeric_version` | covered. The callers that pass it are step e. |
| This package's: `X.Y.Z.0` and `ProductVersion` | `..._configure_checks_the_installed_binary`, `..._configures_check_of_the_binary` | covered |
| 45 with a rename onto the binary, 40 otherwise | the pure check with a boolean; the parsing and the matching on literal values | weak: the live 45 path needs a rename planted in HKLM, which no unelevated test may write |
| The agent's real resource reads as this package's | `..._the_agents_build_is_this_packages` | covered in `cargo test --all`, which builds the agent. On its own it reads nothing when the agent is not built. |
| Every route, before the stop; configure starts nothing and leaves the record | none | step e |

Dispositions that change no code:

- **#153.** `service_running` is read at item 3's check, and the record
  precedes the stop. A service started between the two is stopped while
  the record says it was not running, so the reversal does not start it
  again. This is not among the four disclosed cases. Destination: step e,
  with #152.
- **The renames list ends at an empty source.** That is Windows's own
  form. A rename after such an entry would be missed (40 instead of 45).
- **`GetFileVersionInfoExW` follows a reparse point at the binary's path**,
  which `<ProgramFiles>` holds for administrators alone. The threat model's
  "elevated administration is not a threat boundary" covers it. Step e
  decides whether preflight checks that path.

## Gates and CI

- Local Rust gate, main checkout, `-j 2`, debug off:
  - on `17082b6`'s tree (`9d914b2`): `FMT_OK`; clippy `Finished`, with no
    warning; "binaries 89 passed 479 failed 0 ignored 11"; "agent version
    matrix: all steps passed"; "cargo audit exit 0";
  - on `0ba4489`'s tree (`ff5351c`): `FMT_OK`; clippy `Finished`;
    "binaries 90 passed 491 failed 0 ignored 11"; "agent version matrix:
    all steps passed"; "cargo audit exit 0".

  `cargo audit` reports RUSTSEC-2026-0440 and RUSTSEC-2026-0386 as allowed,
  as on `main`.
- The elevated gate does not apply: the diff touches none of its paths
  (`agent/cg-setup/` only, and rust-ci).
- CI:
  - PR #102 at `f081319`, `341f534` and `17082b6`: markdown-lint,
    dependency-audit, ts-ci, deploy-ci and rust-ci `success`. The SCM step
    gave "12 passed; 0 failed" at `17082b6`, in 300.11 s.
  - `main`, push runs:
    - `f081319`: ts-ci, rust-ci, markdown-lint, dependency-audit `success`,
      ALL GREEN;
    - `341f534`: the same four `success`, ALL GREEN;
    - `17082b6`: ts-ci and rust-ci `success`, ALL GREEN.
  - PR #104 at `0ba4489`: rust-ci, markdown-lint, dependency-audit and
    deploy-ci `success`; ts-ci `success` on attempt 2 (above).
  - `main` at `0ba4489` (push runs): dependency-audit, markdown-lint, ts-ci
    and rust-ci `success`, ALL GREEN;
  - this close's commit: as its post-push report gives it.

## Landing

- `main` was fast-forwarded from `14eb401`, one commit per push, each push's
  CI green before the next: `f081319`, `341f534`, `17082b6`, then
  `0ba4489`. The landed SHAs are the ones CI ran.
- PRs #102 and #104 are closed, and their branches deleted, local and on
  origin.

## Test baselines at close

- Non-elevated `cargo test --all`: 491 passed, 0 failed, 11 ignored, in 90
  binaries (cg-setup 84).
- The SCM suites in CI: cg-agent's 35, unchanged, and cg-setup's 12, new.
- Elevated `--ignored` and vitest: not run this session, unchanged at S58's.

## Debts

- Closed: none.
- Opened: **#153.** `service_running` is read at item 3's conflict check,
  and preflight writes the record before item 6's stop. A product service
  started between the two (by an administrator, or by a restart its failure
  actions schedule) is stopped by preflight while the record says it was
  not running. The reversal then leaves it stopped. This is not among the
  four disclosed cases. Destination: step e, with #152. Either the stop
  reports whether it found the service running and the record is written
  from that, or Manuel accepts it as a fifth disclosed case.
- Unchanged:
  - **#152** (step e);
  - **#87**, its `cg-setup`, MSI and bundle halves;
  - **#150**.
- Open at close: **56**.

## Waiting on Manuel

Nothing blocks. #152 and #153 are questions for step e, where the verbs
make the readings visible. The release-tag freeze stays.

## How Session 64 resumes

1. Run the start protocol (CLAUDE.md). `main` is this handoff's close
   commit. There is no PR. The counts are 56 open debts and 22 ADRs / 23
   SPECs.
2. Do the rest of C5, each step through a PR, one commit per push, with the
   contract tests first.
   - **d. The observer** (item 8's outcome), as a library in
     `agent/cg-setup/src/observer.rs`.
     - `LogPosition::record(log_dir)` takes the file ID and size of
       `agent.jsonl`, or its absence, and the IDs of the log files present
       (`agent.N.jsonl` from `ARCHIVES` down to 1, `agent.rotating.jsonl`,
       `agent.jsonl`; `cg_agent::service::rotation`).
     - Reads open without following a reparse point, sharing read, write
       and delete. The recorded file is read from its size, wherever
       rotation moved it, then each file whose ID was not present, oldest
       first, each up to its last line end. Lines are kept per file ID
       between polls.
     - `observe(scm, name, log_dir, position)` runs 60 s from the start
       request:
       - `STOPPED` is 42, with its exit codes in the line;
       - pending at the end is 43;
       - `RUNNING` without `agent identity ready` then `signed heartbeat
         sent` (`fields.message`), in order, is 44.
     - Unelevated tests on temporary log directories, written as rotation
       writes them, rotating between the position and the reads.
     - SCM tests: the test service gains behaviours that write those lines,
       rotate and stop with a code, as the observer's tests against test
       services of their own name.
     - The agent's test that pins both lines lands in cg-agent.
       `install_ac_007_service.rs:702` already reads "agent identity
       ready"; pin "signed heartbeat sent" too.
   - **e. The executable and its verbs (item 9).**
     - `src/main.rs`, and a `build.rs` that shares the agent's version
       resource encoding with its own `OriginalFilename` and no message
       table.
     - `<ProgramFiles>` from the OS, for the installed binary's path.
     - preflight's order: platform, elevation, state, inputs,
       `check_product_service`, `version::downgrade`, `staging::stage` with
       `service_running`, `scm::stop`, and `Attempt::refuse` on any later
       refusal.
     - configure's order: the record's checks, `version::configure_version`,
       `scm::set_failure_actions`, the log position, `scm::start` with
       `start_arguments`, the record's deletion, then `observe`.
     - `reverse`: `staging::reverse`, then `scm::start` with no argument
       when `start`.
     - `--version`; the import test reads `CARGO_BIN_EXE_cg-setup`; the
       floor's order is tested on the binary.
     - Settle **#152** and **#153** with Manuel before the verbs land: they
       are exit codes, the operator's contract.
   - **f** and **g** as `docs/handoff-session-61.md` gives them.
3. Then C6 to C10 as `docs/handoff-session-59.md` orders them, with the
   wizard in C7.
4. Keep the release freeze.

## Environment and memory

- The reviewers ran in isolated worktrees, which the workflows removed. One
  reviewer of step c built a `target\` of 799 MB in its worktree, which went
  with it.
- Bash heredocs and `node -e` scripts again mangled backslashes in Rust
  string literals. Those edits went through Edit (the auto-memory's rule).
