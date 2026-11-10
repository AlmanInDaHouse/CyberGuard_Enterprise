# Handoff — Session 62

Session 62, 2026-11-10, did step a of C5's plan
(`docs/handoff-session-61.md#how-session-62-resumes`). That step is the
staging, `setup-attempt.json` and the reversal, as a library, under SPEC-024
Amendment 2026-11-10, items 5 and 7. Steps b to g did not start. Step b needs
a probe pull request on the Windows runner and a test service of its own,
and it would not have fitted this session at the quality bar. The session
closes at a green commit with the plan below.

## Start and resulting state

- Start:
  - `main` = `origin/main` = `378e0bd`, clean, with no stash, no branch but
    `main` and no PR open;
  - the tip's CI was green (markdown-lint; the rest was skipped by path);
  - 22 ADRs / 23 SPECs, 54 open debts (54 rows) and no Known CI debt. The
    repo agreed with the handoff.
- Memory before any gate: 4.5 GB free of 16.2, with Manuel's
  `aura_cache` and `aura_db` up and the CyberGuard dev stack down. `C:` had
  13 GB free, and `target\` held 41 GB. `cargo clean --profile dev` freed
  40.6 GB, and `C:` then had 53 GB free. The gates ran with `-j 2`,
  `CARGO_INCREMENTAL=0` and the profiles' debug info off.
- Resulting `main`, in order:
  - `6eaf8cb`, the staging, the record and the reversal;
  - `5d51514`, the self-review's fixes;
  - this close's commit.

  PR #100 is closed. Its branch is deleted, local and on origin. No other
  branch is left, and no worktree.
- Catalogs: 22 ADRs / 23 SPECs, unchanged.

## What landed

### The staging, the record and the reversal (`6eaf8cb`, `5d51514`, PR #100)

- **`agent/cg-setup/src/record.rs`.** `setup-attempt.json` as item 5 states
  it.
  - `Record` holds `created` (a set of `product_root`, `data_root`,
    `staged_anchor` and `staged_config`), `fingerprints` and
    `service_running`.
  - `to_bytes` writes exactly the four keys, with `format` 1.
  - `parse` refuses:
    - more than 64 KiB, what is not JSON, and what is not an object;
    - a missing key, another key or a key given twice (a derived reader of
      the four keys);
    - a `format` other than the integer 1;
    - a name that is not one of the four;
    - a value that is not a fingerprint;
    - a `service_running` that is not a boolean.

    No reason quotes a value of the file.
- **`agent/cg-setup/src/staging.rs`.**
  - `SetupCustody::system()`:
    - the service's check of `<P>` and the data root (`Policy::shared()`);
    - the staged files' check, as item 6.4 of the Amendment 2026-11-09
      opens them;
    - `O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)` for the directories it
      creates;
    - `O:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)` for the record and the staged
      files.
  - `stage(setup, files, service_running)`, in this order:
    1. It holds `<P>`, then the data root. Each is opened without following
       it, for listing, attributes and security, sharing read and write but
       not delete, and checked through that handle. Only a route that
       stages creates a missing one, new.
    2. It removes the earlier `agent.toml.setup`, `server-ca.pem.setup` and
       record by name, without following them.
    3. It writes the record, then the anchor, then the configuration, each
       created new with the files' security.

    The codes:
    - 27: a name that fails the check, one that is there at the creation, a
      directory under a staged name, or a directory that is absent on the
      route that starts;
    - 40: a failure of the setup's own state.

    A record over 64 KiB is refused (40) before it is written. A failure
    from the record on goes through `Attempt::refuse`.
  - `Attempt` is `must_use`. `Attempt::refuse` removes, in this order:
    1. the staged files that the record names;
    2. the record;
    3. the data root and `<P>`, each only if the record names it and it is
       empty.

    It stops at the first failure, so the record stays while a staged file
    does. It returns the refusal's own code, its line naming what could not
    be removed.
  - `read_record` opens the record without following it, sharing read
    alone. It then checks the record's shape, owner and DACL and its size
    (16 MiB), and then its form.
  - `reverse` reads the names first.
    - Names that cannot be read are 40. That includes a data root whose
      listing fails, in item 3's order of states.
    - Then the state, by `classify_names`. An untrusted state, a migration
      in progress, a console layout or no data root is `Unchanged` (exit
      0).
    - It holds `<P>` and the data root as `stage` does. A hold that fails
      is `Unchanged`.
    - No record is `Unchanged`.
    - A record that passes its checks is removed as `refuse` removes it,
      and `Removed { start: service_running }` is returned.
    - A record that fails its checks: each staged name and the record are
      tried, the ones that cannot be removed are left, and the result is 40.
  - A `Watch` is told of each `Step`. The tests inject a failure, or a
    crash (a panic), at each one.
- **`agent/cg-agent`**, with no change of behaviour:
  - the custody helpers that `cg-setup` needs become `pub`: `Handle`,
    `into_file`, `file_information`, `open_existing`, `check_shape`,
    `shape`, `check_security`, `file_id`, `create_new_directory`,
    `delete_through` and `entries`;
  - `layout::classify` splits into `Names::read` and `classify_names`, and
    `Names::data_root()` is added.
- **`agent/cg-setup/Cargo.toml`:** `serde` (derive) and `serde_json`, at
  cg-agent's requirements. No new third-party crate: `Cargo.lock` gains two
  edges only.
- **Tests:** `agent/cg-setup/tests/package_staging.rs`, 27 tests,
  unelevated on temporary roots, with the current user standing for SYSTEM
  and Administrators.

## Red evidence (CLAUDE.md compensating control 1)

- With `stage`, `Attempt::refuse`, `read_record`, `reverse`,
  `Record::to_bytes` and `Record::parse` as `unimplemented!()`: 19 failed,
  0 passed, each at `not implemented: <function>`.
- `package_staging_held_directories_cannot_be_renamed` failed against the
  first implementation, which held the directories for attributes and
  security alone. Its error was `Win32 error 3`: the data root had been
  renamed while it was held. Windows records a handle's sharing only when
  the handle holds a data access, and adding `FILE_LIST_DIRECTORY` to the
  hold fixed it. With `FILE_SHARE_DELETE` added to the hold's sharing, the
  test fails again (a mutation check).
- The self-review's fixes, against `6eaf8cb`'s source with `5d51514`'s
  tests: 3 failed, 24 passed:
  - `package_reversal_of_a_refused_record_tries_every_name`;
  - `package_staging_a_record_of_another_form_is_refused`, at "a key
    twice";
  - `package_staging_a_record_larger_than_64_kib_is_not_written`.

## Decisions

### Anticipated (Claude Code)

- **A library first, with the executable later (step e).** Discarded:
  wiring `stage` into `preflight()` now. That needs the SCM client's
  `service_running` and the stop (step b), and the downgrade (step c).
- **The record as a set of names and a set of fingerprints.** Duplicates in
  `created` collapse. No relation between `created` and `fingerprints` is
  checked. Discarded: a cross-field check, which the text does not state;
  adding one would narrow the contract.
- **Config before anchor in the removal,** the reverse of their creation, so
  the token goes first. Discarded: the creation's order.
- **A directory under a staged name** is a name with the directory
  attribute, a junction included, and it is never deleted. Discarded:
  deleting a junction's link, which is harmless, but the name is not the
  setup's.
- **The route that starts creates nothing.** A missing `<P>` or data root
  there is 27, since item 5 creates only "on a route that stages".
  Discarded: creating them and naming them in the record.
- **A failure at the record's own write removes the directories created,**
  through `refuse`, so a partial record is never left. Discarded: leaving
  them, as a crash does.
- **A crash is a panic in the tests.** No cleanup lives in `Drop`, so an
  unwinding stage leaves what a killed process leaves. Discarded: a
  `Crash` variant in production code.

### Reactive corrections

- **The hold lists the directory** (`FILE_LIST_DIRECTORY`). With
  attributes and security alone, the directories could be renamed while
  held (the red evidence above).
- **The first review (fixed prompt, a custody adversary and a Windows
  semantics lens, each lens finding verified by a fresh refuter, 5
  agents).** The two lens findings were confirmed and fixed in `5d51514`:
  - The reversal of a refused record stopped at the first name it could
    not delete. It now tries every name.
  - `stage` could write a record over 64 KiB, which its own reader then
    refuses. That happens from about 886 fingerprints, and the bootstrap
    caps none. It is now 40 before the write.

  Also from the fixed prompt:
  - a key given twice passed `parse`, because a map keeps the last value,
    while the text says "exactly these keys". It is now refused;
  - `Attempt` became `must_use`;
  - seven tests were added. They cover `<P>` created new, a crash at each
    removal step, a failure at each reversal step with what stays
    asserted, and records refused for an unprotected DACL, their size and
    a reparse point.
- **The second, fresh pass after the fixes** (fixed prompt, on
  `378e0bd..5d51514`) found nothing contradicted. Its open readings are
  under *Self-review*.
- **The close's CI poll** asked for seven runs of a SHA whose runs it
  groups by name, at most five. It was stopped after `6eaf8cb`'s push to
  `main` had landed, and rerun on the push runs alone.

## Self-review (CLAUDE.md compensating control 5)

Contract: the Amendment 2026-11-10, items 5 and 7, item 8's checks of the
record, and its *Tests*' sentence on the staging and the reversal.

| Criterion | Test (`agent/cg-setup/tests/package_staging.rs`) | Verdict |
|---|---|---|
| Earlier staged files and record removed first | `..._what_an_earlier_attempt_left_is_removed_first`, `..._the_route_that_starts_writes_the_record_alone` | covered |
| `<P>` and the data root held without following, sharing no deletion, checked through the handle | `..._held_directories_cannot_be_renamed`, `..._a_junction_planted_after_the_read_is_27`, `..._directories_that_fail_the_check_are_27` | covered |
| Created new: a name there by then, or failing the check, is 27 | `..._a_directory_is_created_new`, `..._a_name_at_the_product_root_by_its_creation_is_27` | covered for a name there; weak for a new directory that fails the check (no step between the creation and the check) |
| A directory under a staged name is 27; names not followed | `..._a_directory_under_a_staged_name_is_27_and_nothing_is_written` | covered |
| Order: directories, record, anchor, configuration | `..._a_fresh_host_gets_..._in_that_order` | covered |
| The route that starts writes the record alone | `..._the_route_that_starts_writes_the_record_alone` | covered |
| The record's form, at most 64 KiB | `..._the_record_has_exactly_its_four_keys`, `..._a_record_of_another_form_is_refused`, `..._a_record_larger_than_64_kib_is_not_written` | covered |
| No path, no secret | `..._a_fresh_host_gets_...` | covered |
| `service_running`, `STOP_PENDING` included | none | missing: computed by the SCM client (step b) |
| A refusal after the record removes, in order, only what it names and only if empty, before returning | `..._a_refusal_after_the_record_removes_what_it_names_in_order` | covered for the library. "Before preflight returns" rests on the caller (step e). |
| A crash before the record, or between removals, is accepted next time | `..._after_a_crash_at_each_step_...`, `..._after_a_crash_during_the_removal_...` | covered |
| The reversal's states, 0 | `package_reversal_changes_nothing_where_preflight_would_not_stage` | covered |
| Names that cannot be read, 40 | `package_reversal_of_a_host_whose_names_cannot_be_read_is_40` | covered |
| Held directories failing the check, 0 | `package_reversal_a_junction_planted_after_the_read_changes_nothing`, `..._a_held_product_root_that_fails_the_check_changes_nothing` | covered |
| No record, 0 | `package_reversal_without_a_record_changes_nothing` | covered |
| A record that passes: removal in order, start flag | `package_reversal_removes_what_the_record_names_and_says_whether_to_start` | covered. The start itself is step b/e. |
| A record that fails: each name removed, 40 | `package_reversal_of_a_record_that_fails_its_checks_...`, `..._tries_every_name`, `..._more_records_that_fail_their_checks` | covered. The owner and the 16 MiB bound are weak: the owner is checked by the agent's `check_security`, which the agent's custody tests cover. |
| A directory under a name left, 40; a failed deletion, 40 | `package_reversal_a_directory_under_a_named_file_or_a_failed_deletion_is_40`, `package_reversal_a_failure_at_each_step` | covered |
| A failure injected at each step | the two crash tests, `..._a_failure_at_each_step_...` (staging), `package_reversal_a_failure_at_each_step` | covered. In `refuse`, only `Remove(StagedAnchor)` is injected as a failure, and every removal step as a crash. |

The two passes' dispositions that change no code:

- **A hold that cannot be opened** (a sharing violation or a timeout) is 27
  in `stage` and `Unchanged` in `reverse`. That follows item 3's reading,
  where `classify` takes a check that cannot be made as untrusted.
- **The production SDDLs** (`O:BA`) run in no local test. Their test is
  step g's `package-ci` run, as the amendment's *Tests* say.
- **The custody API made public** widens `cg-agent`'s library surface but
  changes no privilege. `file_id` is for the observer (step d).
- **Two readings the text does not settle** are recorded as #152 for
  step e:
  - a reversal whose deletion fails after the record's removal leaves a
    service the record found running stopped;
  - a refusal whose removal fails keeps the refusal's code.

## Gates and CI

- Local Rust gate on `5d51514`'s tree (main checkout, `-j 2`, debug off):
  - `FMT_OK`;
  - clippy `Finished`, with no warning;
  - "binaries 88 passed 471 failed 0 ignored 11";
  - "agent version matrix: all steps passed";
  - "cargo audit exit 0" (with RUSTSEC-2026-0440 and RUSTSEC-2026-0386, as
    on `main`).
- The elevated gate does not apply. The diff touches none of its paths:
  `custody.rs` and `service/layout.rs` are outside them.
- CI:
  - PR #100 at `6eaf8cb`: markdown-lint, dependency-audit, ts-ci,
    deploy-ci and rust-ci `success`;
  - PR #100 at `5d51514`: the same five `success`;
  - `main` at `6eaf8cb` (push runs): ts-ci, markdown-lint, dependency-audit
    and rust-ci `success`, ALL GREEN;
  - `main` at `5d51514` (push runs): dependency-audit, ts-ci and rust-ci
    `success`, ALL GREEN;
  - this close's commit: as its post-push report gives it.

## Landing

- `main` was fast-forwarded from `378e0bd`, the branch's base, one commit
  per push: `6eaf8cb`, then `5d51514`. The landed SHAs are the ones CI ran.
- PR #100 is closed. Its branch is deleted, local and on origin.

## Test baselines at close

- Non-elevated `cargo test --all`: 471 passed, 0 failed, 11 ignored
  (cg-setup 64).
- The SCM suite in CI: 35 tests, unchanged.
- Elevated `--ignored` and vitest: not run this session, unchanged at S58's.

## Debts

- Closed: none.
- Opened: **#152**. SPEC-024 Amendment 2026-11-10 leaves two readings
  unsettled:
  - item 7: a reversal whose deletion fails after the record's removal
    exits 40 and does not start a service the record found running. The
    flag is lost with the record, and the case is not among the four
    disclosed ones;
  - item 5: a refusal whose removal then fails keeps the refusal's code
    (31, for example) rather than 40, and its line names the failure.

  Destination: step e, where the verbs make both visible to Burn. Either
  Manuel confirms the readings, or an amendment settles them. Exit codes
  are the operator's contract.
- Unchanged:
  - **#87**, its `cg-setup`, MSI and bundle halves;
  - **#150**, which C5 does not reopen: the staging reads no input file.
- Open at close: **55**.

## Waiting on Manuel

Nothing blocks. #152 is a question for step e. The release-tag freeze
stays.

## How Session 63 resumes

1. Run the start protocol (CLAUDE.md). `main` is this handoff's close
   commit. There is no PR. The counts are 55 open debts and 22 ADRs / 23
   SPECs. Check the memory and `C:` before a full gate: a full
   `cargo test --all` rebuilds `target\debug` after this session's clean.
2. Do the rest of C5 in the order of `docs/handoff-session-61.md`'s plan,
   from step b. Each step goes through a PR, one commit per push, with the
   contract tests first.
   - **b. The SCM client** (items 3, 6, and item 8's failure actions and
     start). It takes the service's name.
     - It reads the configuration (type, account and command line, the
       paths compared as `is_fixed_path` compares them) for 26.
     - It sets `service_running` for any state but `STOPPED`, which is
       what `stage` takes.
     - The stop: within 75 s, sending the control again while the service
       cannot accept it.
     - The failure actions, with `ChangeServiceConfig2W` and the flag set
       to false.
     - The start, with arguments.

     It needs `Win32_System_Services` in `cg-setup`, a `scm-tests` feature
     with `[[test]]` targets, and rust-ci's Windows step
     `cargo test -p cg-setup --features scm-tests --no-fail-fast --test <target> -- --test-threads=1`.
     The *Environment facts* sentence lands with it. A test service needs
     a controllable binary: one that accepts a stop late, or never. That
     could be a test-only `[[bin]]` behind the feature, or `cg-agent` with
     a config that refuses. Decide this first. The red run is on a probe PR
     with `continue-on-error`, closed without landing.
   - **c, d, e, f, g** as `docs/handoff-session-61.md` gives them. Step e
     calls `staging::stage` after the conflict, the downgrade and
     `service_running`, `Attempt::refuse` on 31 and on any later refusal,
     and `staging::reverse` for `reverse`. It also settles #152.
3. Then C6 to C10 as `docs/handoff-session-59.md` orders them, with the
   wizard in C7.
4. Keep the release freeze.

## Environment and memory

- `target\debug` was cleaned at the start (40.6 GB). The gates ran in the
  main checkout. The reviewers ran in isolated worktrees, which the
  workflow removed.
- Bash heredocs failed again on a long body with quotes. The tests were
  appended through a file written with Write (the auto-memory's rule).
