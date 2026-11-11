# Handoff — Session 64

Session 64, 2026-11-11, did step d of C5's plan
(`docs/handoff-session-63.md#how-session-64-resumes`), configure's observer,
under SPEC-024 Amendment 2026-11-10, item 8's outcome. It also settled with
Manuel the exit readings that step e needs: SPEC-024 Amendment 2026-11-11
(#152, #153 and the observer's unreadable state). Step e did not start: it
implements that amendment as well as the verbs, and does not fit this
session at the quality bar. The session closes at a green commit with the
plan below.

## Start and resulting state

- Start:
  - `main` = `origin/main` = `d617581`, clean, with no stash, no branch but
    `main` and no PR open;
  - the tip's CI was green (markdown-lint; the rest was skipped by path);
  - 22 ADRs / 23 SPECs, 56 open debts (56 rows) and no Known CI debt. The
    repo agreed with the handoff.
- Before any gate, 6.7 GB of 15.8 GB of memory was free and `C:` had 47 GB
  free (`target\` held 6.1 GB). Docker Desktop was down; nothing this
  session needed it. The gates ran with `-j 2`, `CARGO_INCREMENTAL=0` and
  the profiles' debug info off.
- Resulting `main`, in order:
  - `fe1a512`, SPEC-024 Amendment 2026-11-11 (docs, straight to `main`);
  - `f0e37c3`, configure's observer (PR #106, one commit landed by
    `merge --squash`);
  - this close's commit.

  PR #106 is closed and its branch deleted, local and on origin. The probe
  PRs #105, #107 and #108 are closed without landing, and their branches
  deleted. The owner-review branch `spec/s64-exit-readings` is deleted. No
  other branch is left, and no worktree.
- Catalogs: 22 ADRs / 23 SPECs; SPEC-024 gains the Amendment 2026-11-11
  and its catalog line.

## What landed

### SPEC-024 Amendment 2026-11-11 (`fe1a512`)

Readings Manuel chose on 2026-11-11, in chat, one question each, the
recommended option each time; text approved the same day ("Sí, apruebo")
after three fresh text reviews:

1. **#153.** Item 6's stop reports whether its first reading found the
   product service in a state other than `STOPPED`. When it did and the
   record says `service_running` false, preflight rewrites the record in
   place (opened sharing nothing, checked as item 8 opens it, truncated),
   after the stop and before a later refusal's removal. A failed rewrite is
   40. The rewrite goes one way.
2. **#152, the reversal.** With a record that passes its checks, the
   reversal takes `service_running` before it removes anything and, when
   true, starts the service after its removals, also when they stopped at
   one that failed (that removal is 40). A service not `STOPPED` needs no
   start; a start the SCM refuses is 40, not 41 (41 holds only after the MSI
   committed).
3. **#152, a refusal.** A refusal whose removal fails keeps its own code,
   its line naming the removal's failure (`Attempt::refuse` already does
   this).
4. **The observer.** After an accepted start, a state that cannot be read,
   or a service that no longer exists, is 44; at the end, any state but
   `STOPPED` and `RUNNING` is 43; a log position that cannot be recorded
   before the start is 40.

The third disclosed case widens to a preflight that exits 31, or cannot
rewrite its record, after its stop request, and, on the migration route,
a service started after item 3's check.

### Configure's observer (`f0e37c3`, PR #106)

- **`agent/cg-setup/src/observer.rs`**:
  - `LogPosition::record(log_dir)`: the active file's ID and size, or its
    absence, and the IDs of the log files present (`log_names()`, oldest
    first: `agent.5.jsonl` to `agent.1.jsonl`, `agent.rotating.jsonl`,
    `agent.jsonl`). A log directory that is not a directory, a reparse
    point included, or a log that cannot be listed, is 40.
  - `LogReader::poll(wait)`:
    - the directory is opened without following it, checked to be a
      directory and held for the poll (`FILE_LIST_DIRECTORY`, sharing read
      and write);
    - every name is opened without following it and sharing read, write and
      delete; a name that is not a regular file with one link is not a log
      file;
    - a poll reads only when two listings in a row agree on names and IDs,
      and reads the recorded file first, from its recorded size, then each
      file whose ID was not present, oldest first, each up to its last line
      end, kept by file ID between polls; it stops at a file that moved
      since the listing;
    - a line counts when it is a JSON object whose `fields.message` is the
      text; a line of 64 KiB or more is passed over.
  - `observe(scm, name, log_dir, position, started)`: 60 s from `started`.
    The state is read every 500 ms, the log after each reading of
    `RUNNING`, each open bounded at 2 s. `STOPPED` is 42 with both exit
    codes. Once the end has passed, one more reading decides: `RUNNING` is
    read once more for the lines written up to the end and is 44 without
    both, any other state 43. A state that cannot be read after the start
    is 44.
- `scm.rs`: `Manager::service`, `Service` and `Service::status` become
  `pub(crate)`.
- **Tests:**
  - `agent/cg-setup/tests/package_observer.rs`, 22 unelevated tests on
    temporary log directories: the names and the position; the recorded
    size; the order of the lines; a JSON line's message only; a line once it
    ends and across a read chunk; an overlong line; rotation by hand, one
    rename at a time, and through the agent's own `RotatingLog` with a
    reader polling throughout while the writer holds the file as the
    service does; a file followed by ID under one name; new files oldest
    first; the recorded file first; presence by ID; junctions, directories
    and hard links at log names; a log directory that is a junction or a
    file.
  - SCM tests in `setup_service/mod.rs`, against the test executable as its
    own service (`log_dir`, `log` steps `line`, `rotate`, `sleep_ms`,
    `log_while_pending`, `stop_with`; the active file held as the agent
    holds it): operational with a rotation between the lines; 42 for code 13
    and for a clean stop; 43 pending at the end, from a start request 15 s
    before the start; 43 for a pending service whose two lines appeared; 44
    out of order; 44 with no line; 44 for a state that cannot be read after
    the start.
  - `agent/cg-agent/tests/install_ac_007_service.rs`, `install_ac_010`: a
    `signed heartbeat sent` line at INFO with a 2xx `response_status`,
    after the identity line, in the service's own log (it now waits for two
    heartbeats, so the first one's line is written).
- rust-ci's comments cite the probes; no step changed.

## Red evidence (CLAUDE.md compensating control 1)

- **Unelevated, against stubs.** With the module's API as `unimplemented!()`:
  "0 passed; 16 failed", each at `not implemented`.
- **Probe #105 (`d59bd9e`).** Stubs, the agent's line renamed
  `signed heartbeat delivered (probe)`, the steps continuing on error:
  - the 16 unelevated tests failed at `not implemented: LogPosition::record`;
  - the four SCM observer tests failed at the same panic;
  - `install_ac_010` failed: "a \"signed heartbeat sent\" line after the
    identity line", the log holding the renamed lines;
  - `install_ac_009_a_held_lock_stops_the_service_with_code_10` also failed
    there, unrelated to the change: "a refused instance stops no ETW
    session, the agent's own name included", `left: [false, true]`. It
    passed on #106 at `1710e62`, `60761d7` and `364b925`. Recorded as #154.
- **The review's unelevated tests, against mutations (local).** Each
  mutation of `observer.rs` was killed: no `FILE_SHARE_WRITE` (2 tests),
  `read_to` keyed by name (2), the heartbeat constant changed (1), the
  chunk's partial line dropped (1), name order without the recorded file
  first (1). Against `1710e62`'s observer, only the recorded-file-first test
  failed, the defect fixed.
- **Probe #107 (`6426175`).** Four mutations of `observe()`: 43/44 from the
  log, the log read while pending, `started` ignored, 42 only for a nonzero
  code. "14 passed; 4 failed": the clean stop gave 43, the 15 s origin
  took 75 s, the pending service with both lines gave `Ok` after 9.6 ms,
  the RUNNING service with no line gave 43.
- **Probe #108 (`dc1eb85`).** The unreadable state against the pre-fix 40:
  `left: 40, right: 44`.

## Decisions

### Anticipated (Claude Code)

- **File IDs from `cg_agent::custody::file_id`** (volume serial and the
  64-bit file index), as the service's custody uses them. Discarded:
  `FILE_ID_INFO`'s 128-bit ID, which only ReFS needs; `%ProgramData%` is on
  the system volume.
- **Two agreeing listings before a poll reads.** Each rename, create or
  delete falls wholly before or wholly after the reads of its names, so two
  identical listings are the directory at one instant. Discarded: creation
  times, which NTFS name tunnelling copies onto a new active file.
- **Stop at a file that moved since the listing**, leaving it and the files
  after it to the next poll, so no line is read before one written
  earlier.
- **A name that is not a regular file with one link is not a log file**, as
  the agent refuses a hard-linked log. Discarded: reading it.
- **The log directory held per poll** (no delete sharing): the agent never
  holds or renames it after `RUNNING`, and its migration refuses a data root
  that holds `logs`. Discarded: no hold (a junction swapped in mid-poll).
- **A line of 64 KiB or more is passed over**, the agent's record bound.
- **The SCM test service is the test executable**, as in S63, with log
  behaviours; the agent's real rotation is tested unelevated.

### Reactive corrections

- **The first review of `1710e62`** (the fixed prompt, a Win32/concurrency
  lens, a test-falsification lens, a refuter for each lens finding; 8
  agents). Fixed in the second commit:
  - every open of a poll could wait the whole remaining window, and the end
    was classified on the state read before it (confirmed): opens are
    bounded at 2 s, and one more reading after the end decides;
  - a poll after the end could find both lines and still report 44, with a
    reason its own reading contradicted (confirmed): fixed with the above;
  - the log is read only after a reading of `RUNNING`;
  - the recorded file is read first, as item 8 orders it;
  - six mutations survived the tests (confirmed): new tests above.
  - Refuted: that two agreeing listings can be a false snapshot (the
    argument above).
  - Not reachable: a hold on `logs` failing the migration's rename of the
    data root; the migration refuses a data root that holds `logs`
    (`migration.rs` `data_root_names`, checked again before the rename).
  - To Manuel: 40 for a state unreadable after the start (point 4).
- **The second pass over `d617581..60761d7`** (the fixed prompt and a
  regression lens with a refuter). The lens's one finding was refuted, but
  it showed that the first fix's comment justified the `RUNNING` rule with
  the migration's rename, which is not reachable; the comment now gives the
  contract's reason. Lines written in the last poll interval were missed:
  `RUNNING` at the end is now read once more.
- **The amendment's text, three fresh reviews.** The first found two List A
  defects (the next preflight does not remove directories; the crash windows
  of the rewrite), the second two more (41 after a reversal contradicts
  "the MSI having committed"; the next preflight is 27 on a directory under
  a staged name and removes nothing on the migration route), the third none.
- **The step's three commits were rebased onto `fe1a512` and squashed** into
  `364b925`, so the branch landed with one CI cycle; the guard was
  `git diff --cached --quiet`.

### Manuel's decisions

- 2026-11-11, in chat, four questions, each the recommended option: #153
  "Parada informa + reescribe"; #152 (a) "Arrancar igualmente, salir 40";
  #152 (b) "El de la negativa"; the observer "44".
- 2026-11-11, in chat, the amendment's text: "Sí, apruebo".

## Self-review (CLAUDE.md compensating control 5)

Contract: SPEC-024 Amendment 2026-11-10, item 8's outcome and *Tests*; the
Amendment 2026-11-09, item 7; the Amendment 2026-11-11, point 4; the
Amendment 2026-11-01, point 4.

| Criterion | Test | Verdict |
|---|---|---|
| The two lines, each a JSON line whose `fields.message` is the text | `package_observer_the_two_lines_are_the_agents`, `..._only_a_json_lines_message_counts` | covered |
| A test of the agent pins both | `install_ac_010_the_service_log_is_protected_json_lines` (CI only) | covered |
| Position: the active file's ID and size or absence, the IDs present | `..._records_the_active_files_size_and_the_files_present`, `..._a_new_file_at_an_old_files_name_is_read`, `..._files_present_before_the_start_are_not_read` | covered |
| Read without following a reparse point | `..._a_name_that_is_not_a_log_file_is_not_read`, `..._a_log_directory_that_is_not_a_directory_is_40_and_not_read` | covered for junctions; a file symlink needs a privilege no unelevated test holds: weak |
| Sharing read, write and delete | `..._reads_the_active_file_while_the_agent_holds_it`, `..._follows_the_agents_own_rotation` | covered for write and for rename; a delete of an archive held open is not reached (five rotations): weak |
| The recorded file from its size, wherever rotation moved it | `..._follows_the_recorded_file_through_rotation`, `..._reads_the_recorded_files_tail_before_the_files_after_it`, `..._follows_a_file_read_under_one_name_by_its_id`, the SCM operational test | covered |
| Then each new file, oldest first | `..._reads_new_files_oldest_first`, `..._reads_the_recorded_file_before_the_new_files` | covered |
| Each up to its last line end | `..._a_line_counts_once_it_ends`, `..._a_line_across_a_read_chunk_counts` | covered |
| 60 s from the start request | the SCM 43 test with the start 15 s after `started` | covered |
| `STOPPED` in them is 42 | the SCM 42 test, codes 13 and 0 | covered |
| Pending at the end is 43 | the two SCM 43 tests | covered |
| `RUNNING` without both lines in order is 44 | the two SCM 44 tests | covered; the last 500 ms window is not testable without timing: weak |
| Operational is `RUNNING`, then the two lines | the SCM operational test, the pending-with-lines 43 | covered |
| Point 4: unreadable state after the start is 44 | the SCM unreadable-state test | covered for an open the DACL denies; a status read failing after the open is not producible: weak |
| Point 4: a position not recorded before the start is 40 | `..._a_log_directory_that_is_not_a_directory_is_40_and_not_read` | covered at the library; "configure starts nothing" is step e |
| Venue: test-only feature, rust-ci Windows alone, probes | `Cargo.toml`, rust-ci, #105, #107, #108 | covered |

Dispositions that change no code:

- **A delete under a held archive on an older build.** Where `DeleteFileW`
  keeps a delete-pending name until the last handle closes, a reader holding
  `agent.5.jsonl` while the agent rotates for the fifth time makes the next
  rename fail; the agent retries every second for 60 s. It needs about
  50 MiB of log within the observation.
- **The log directory's shape only.** The observer checks `logs` is a
  directory and no reparse point; the components above it are followed.
  Step e passes a `log_dir` under the data root configure holds.
- **A service deleted after it reports `STOPPED`** is 42 or 44 by which
  reading comes first.
- **The reversal's residuals the third text review listed** (an untrusted
  host, a failed hold, a record refused for a reason other than a crash
  leave stopped a service the record found running) predate this amendment:
  #155.

## Gates and CI

- Local Rust gate, main checkout, `-j 2`, debug off:
  - `1710e62` (tree `edcab2f`): `FMT_OK`; clippy clean; "binaries 91
    passed 507 failed 0 ignored 11"; matrix "all steps passed"; "cargo
    audit exit 0";
  - `60761d7` (tree `1a10f9a`): `FMT_OK`; clippy clean; "binaries 91 passed
    513 failed 0 ignored 11"; matrix "all steps passed";
  - `364b925` (tree `bacc1cc`): `FMT_OK`; clippy clean; "binaries 91 passed
    513 failed 0 ignored 11"; matrix "all steps passed"; "cargo audit exit
    0".

  `cargo audit` reports RUSTSEC-2026-0440 and RUSTSEC-2026-0386 as allowed,
  as on `main`, and a third allowed warning new since S63:
  RUSTSEC-2026-0486, `rustix` 1.1.4, unsound (xattr functions on a
  zero-length buffer), dated 2026-10-27.
- The elevated gate does not apply: the diff touches none of its paths
  (`agent/cg-setup/`, a cg-agent test file, rust-ci's comments).
- CI:
  - PR #106: `1710e62`, `60761d7` and `364b925`, every workflow `success`.
    At `60761d7` the cg-setup SCM step gave "18 passed; 0 failed" in
    582.81 s; at `364b925`, "19 passed; 0 failed" in 583.02 s.
  - `main`: `fe1a512` markdown-lint `success`, ALL GREEN; `f0e37c3`
    markdown-lint, dependency-audit, ts-ci and rust-ci `success`, ALL GREEN;
    this close's commit as its post-push report gives it.

## Test baselines at close

- Non-elevated `cargo test --all`: 513 passed, 0 failed, 11 ignored, in 91
  binaries.
- The SCM suites in CI: cg-agent's 35, unchanged in number
  (`install_ac_010` stronger); cg-setup's 19 (12 + 7).
- Elevated `--ignored` and vitest: not run this session, unchanged at S58's.

## Debts

- Closed: none. #152 and #153 now have their contract (the Amendment
  2026-11-11) and close with step e's code.
- Opened:
  - **#154.** `install_ac_009_a_held_lock_stops_the_service_with_code_10`
    failed once in rust-ci's Windows job, on probe #105 (`d59bd9e`): the
    unrelated ETW session it starts was not found after the refused
    instance stopped (`left: [false, true]`). It passed in the three runs
    after. Cause not diagnosed; destination: the next run that fails it, or
    a sweep that reads the session's `logman` output on failure.
  - **#155.** The reversal leaves stopped a service the record found
    running when it changes nothing (an untrusted host, a migration in
    progress, a console layout, no data root, a hold that fails its check)
    or when the record fails its checks for a reason other than a crash;
    none of these is among the disclosed cases (third text review of the
    Amendment 2026-11-11, List B 1; the Amendment 2026-11-10, item 7).
    Destination: step e, with Manuel: a disclosed case, or a start.
- Unchanged: **#152**, **#153** (step e); **#87**; **#150**.
- Open at close: **58**.

## Waiting on Manuel

Nothing blocks. #155 is a question for step e. The release-tag freeze
stays.

## How Session 65 resumes

1. Run the start protocol (CLAUDE.md). `main` is this handoff's close
   commit. There is no PR. The counts are 58 open debts and 22 ADRs / 23
   SPECs.
2. **Step e, the executable and its verbs** (the Amendment 2026-11-10,
   item 9, with the Amendment 2026-11-11), each change through a PR, one
   commit per push, contract tests first:
   - first the Amendment 2026-11-11 in the libraries, with the probe rule
     for the SCM tests:
     - `scm::stop` returns whether its first reading found the service not
       `STOPPED` (point 1);
     - `staging`: `Attempt::rewrite_running()` rewrites the record in place
       (opened sharing nothing, checked as `read_record` opens it, truncated;
       a failure is 40 after the removal), with a `Watch` step for failure
       injection; `reverse` returns the start with a removal's failure
       (point 2: e.g. `Reversal::Removed { start, failed: Option<Refusal> }`),
       and an already running service needs no start;
   - then `src/main.rs` and `build.rs` (the agent's version resource with
     its own `OriginalFilename`, no message table); `<ProgramFiles>` from
     the OS; the verbs and their order as
     `docs/handoff-session-63.md#how-session-64-resumes` gives them, with
     configure's `LogPosition::record` (40, nothing started) before
     `scm::start` and `observer::observe` after it, `log_dir` under the
     data root configure holds; `reverse`'s start with no argument, 40 if
     refused; `--version`; the import test from `CARGO_BIN_EXE_cg-setup`;
     the floor's order on the binary;
   - settle **#155** with Manuel before the verbs land.
3. Then **f** and **g** as `docs/handoff-session-61.md` gives them, and C6 to
   C10 as `docs/handoff-session-59.md` orders them, with the wizard in C7.
4. Keep the release freeze.

## Environment and memory

- Reviewer worktrees were created at `main`; each reviewer checked out the
  change itself. Four were left detached after the workflows and were
  removed after `git status` showed them clean; their branches pointed at
  `d617581` and were deleted.
- A `node` edit that passed a replacer function where a string was expected
  inserted the function's source; it was repaired with Edit. Heredocs again
  turned `\n` in a Rust string into a newline once.
- Two gate runs wrote the same `test.log` at once and gave a false count
  (94 binaries); the gate was rerun alone.
