# Handoff — Session 65

Session 65, 2026-11-11, did step e of C5's plan
(`docs/handoff-session-64.md#how-session-65-resumes`): SPEC-024 Amendment
2026-11-11 in the libraries, then `cg-setup.exe` and its three verbs. It
settled #155 with Manuel, with two exit readings the implementation
surfaced, in SPEC-024 Amendment 2026-11-11 (second) and the threat model's
Amendment 2026-11-11. Steps f and g did not start.

## Start and resulting state

- Start:
  - `main` = `origin/main` = `482bb0f`, clean, with no stash, no branch but
    `main` and no PR open;
  - the tip's CI was green (markdown-lint; the rest skipped by path);
  - 22 ADRs / 23 SPECs, 58 open debts (58 rows) and no Known CI debt. The
    repo agreed with the handoff.
- Before the first gate, 6.8 GB of 15.8 GB of memory was free and `C:` had
  45.6 GB free (`target\` held 6.9 GB). Docker Desktop was down; nothing
  this session needed it. The gates ran with `-j 2`, `CARGO_INCREMENTAL=0`
  and the profiles' debug info off.
- Resulting `main`, in order:
  - `52443e3`, SPEC-024 Amendment 2026-11-11 (second) and the threat
    model's Amendment 2026-11-11 (docs, the owner-review branch
    `spec/s65-reversal-disclosed` squashed);
  - `4856d9d`, the Amendment 2026-11-11 in the libraries (pull request
    #110, its two commits squashed);
  - `4ca972e`, `cg-setup.exe` and its verbs (pull request #112, its three
    commits squashed);
  - this close's commit.

  Pull requests #110 and #112 are closed and their branches deleted, local
  and on origin. The probe pull requests #109, #111 and #113 are closed
  without landing and their branches deleted. The owner-review branch is
  deleted. No other branch is left, and no worktree.
- Catalogs: 22 ADRs / 23 SPECs; SPEC-024 gains the Amendment 2026-11-11
  (second) and its catalog line; the threat model gains its Amendment
  2026-11-11.

## What landed

### SPEC-024 Amendment 2026-11-11 (second) and the threat model's Amendment 2026-11-11 (`52443e3`)

Readings Manuel chose on 2026-11-11, in chat, each the recommended option;
text approved the same day ("Sí, apruebo") after six fresh text reviews:

1. **#155.** A reversal starts the product service only from a record that
   passes the record's checks; every other reversal starts nothing. The
   disclosed cases of the Amendment 2026-11-10's *Effect* widen: the third
   to every such reversal, a reversal that ends before its start and one
   whose start fails; the fourth to those that change nothing; the first to
   a record configure leaves (its 40 and 45, a 41, a failed deletion).
   Where such a reversal follows a preflight that stopped the service, it
   comes from an administrator's change, from what the disclosed cases name,
   from an ordinary user's lever the threat model already names, or from a
   failure with no change; the setup's failure is what the operator reads.
2. **31 for the stop and the rewrite.** A preflight whose stop is 31 and
   whose rewrite then fails exits 31, its line naming both.
3. **The record after the start.** Configure deletes the record only after
   a start the SCM accepted; a 41 leaves it. A deletion that fails keeps the
   outcome's code, 0 included, with one line naming the record left.
4. **The threat model** (§1 Denial of Service): the ordinary user's levers
   of its Amendments 2026-11-05 and 2026-11-07 can also make a setup's
   reversal change nothing, or fail with 40 before any start, leaving the
   agent stopped until a later setup or an administrator starts it, or
   Windows at the next restart. Manuel chose, in chat, to amend the threat
   model rather than disclose it in the SPEC alone or start the service.

### The Amendment 2026-11-11 in the libraries (`4856d9d`, PR #110)

- `scm::stop` returns `Stop { found_running, result }`: whether its first
  reading found the service in a state other than `STOPPED`, also with 31.
- `staging::Attempt::after_stop`: the record rewritten in place when the
  stop found the service running and the record says false (opened without
  following a reparse point, sharing nothing, checked as configure opens it,
  written whole, its end set at the new length); 40 for a failed rewrite,
  31 kept with both named; then a refusal's removal. `Step::RewriteRecord`.
- `staging::Reversal::Removed { start, failed }` and `Reversal::finish`: the
  start after the removals, also after a failed one; 40 whatever the start's
  code. `scm::start_if_stopped`: a `STOPPED` service started with no
  argument, 40 when refused or unreadable, `ERROR_SERVICE_ALREADY_RUNNING`
  as no start needed.

### `cg-setup.exe` and its verbs (`4ca972e`, PR #112)

- `cli::parse`: the command line of item 9; anything else 40, naming no
  value.
- `verbs::{preflight, configure, reverse}` on a `Machine` (paths, service
  name, installed binary), each in its item's order; `Machine::system()`
  from `%ProgramData%`, `%ProgramFiles%` (`custody::program_files`, new) and
  `CyberGuardAgent`.
- `staging::configure`: `<P>` and the data root held for configure's whole
  run, the record read with its checks, each staged file checked as the
  adoption opens it without reading it (`open_checked`); `delete_record`.
- `src/main.rs`: the platform's floor first, then the command line (40),
  then each verb's elevation; `--version` anywhere.
- `build.rs`: the agent's version resource with `OriginalFilename`
  `cg-setup.exe` and no message table, through
  `agent/cg-agent/build_resource.rs`, now cg-agent's encoding too (its
  `version.res` byte-identical, `01c1fcf4…`, before and after).
- The version matrix also rebuilds and tests `cg-setup` for each input.

## Red evidence (CLAUDE.md compensating control 1)

- **Unelevated, against stubs or the pre-fix code (local):**
  - e-1: "25 passed; 9 failed", seven at `not implemented`, two at the
    reversal's old `Err`;
  - the verbs: package_cli "0 passed; 5 failed", package_verbs "0 passed;
    7 failed", all at `not implemented`;
  - the executable: an empty `main` and no `build.rs`, "0 passed; 5
    failed";
  - the review's fixes, against `a1ed63f`'s `main.rs` and `staging.rs`: the
    floor test `left: 40, right: 20`; `configure_reads_no_token` failed;
  - mutations killed: the rewrite's share mode widened, a start refusal
    kept at 41, the data root not held during configure's checks.
- **Probe #109 (`f98c3e3`):** the stop's reading always false and the
  reversal's start a no-op: "1 passed; 7 failed", each at `found_running:
  false` vs `true`, `Ok(false)` vs `Ok(true)` or `assert_refused`.
- **Probe #111 (`aefec49`, after `4d46087` showed a test-host bug):** six
  mutations of the verbs: "0 passed; 5 failed" (the stop's reading dropped,
  the reversal's start dropped, `[[]]` vs the adoption's arguments, the
  record not deleted, the failure actions after the start).
- **Probe #113 (`25499fe`, then `e4810aa`):** the review's SCM tests: the
  stop's 31 dropped, the state at the check not recorded, the conflicting
  service ignored, then the migration's arguments dropped (`[[]]`) and an
  unrecordable log position ignored (`left: 44, right: 40`).

## Decisions

### Anticipated (Claude Code)

- **`scm::stop` returns its reading with its result** (`Stop`), so that a
  31 still reports it. Discarded: `Result<bool, _>`, which loses the
  reading on 31.
- **`Attempt::after_stop` composes the rewrite and the stop's result** in
  the library, testable unelevated. Discarded: composing in `main`.
- **`ERROR_SERVICE_ALREADY_RUNNING` at the reversal's start is no start
  needed**, the state having left `STOPPED` since the reading.
- **The verbs are a library on a `Machine`**, so that unelevated tests run
  them on temporary roots and the SCM tests against test services;
  `main.rs` only dispatches.
- **The shared version-resource encoding is a `#[path]` module**
  (`agent/cg-agent/build_resource.rs`) included by both build scripts.
  Discarded: a build-dependency crate (a new crate) and a copy.
- **The SCM verbs tests use a copy of the test executable with a version
  resource of this package's version** as the installed binary, so that
  preflight passes the conflict and downgrade checks against a real test
  service.
- **The executable's verb tests run under a filtered token**
  (`CreateRestrictedToken` with Administrators deny-only and
  `CreateProcessAsUserW`), so that they stop at 21 also in rust-ci's
  elevated job; the floor's order is tested with `__COMPAT_LAYER=WIN7RTM`,
  which lowers `RtlGetVersion` (verified: 20 before 21).

### Reactive corrections

- **The first review of `c1d7b9e`** (fixed prompt, a Win32 lens, a
  test-falsification lens, refuters; 6 agents): a share mode widened to
  write and delete and a start refusal kept at 41 survived the tests
  (confirmed); new holders and a 41 case in `ad3f511`. The 31 + rewrite
  precedence, contradicting point 1's literal 40, went to Manuel (point 3).
- **Probe #111's first run** showed the SCM verbs test host's service dying
  (1067): the test service writes into `logs`, which only the agent
  creates; the host now creates it.
- **The review of `ad3f511..a1ed63f`** (13 agents): the command line was
  refused before the floor (40, not 20, below the floor), and configure
  read the staged configuration, the token, whose read left unzeroed
  copies; both fixed in `25511cb`. Nine test gaps confirmed by refuters
  became tests; three were refuted.
- **The fresh pass on the fixes** (`a1ed63f..ca0ed37`): the holds test was
  weak (a parent with an open descendant cannot be renamed anyway), two
  test comments were stale, a literal held a raw newline; fixed before the
  push.
- **Probe #113's first run** showed the log-position SCM test passing
  against its mutation by a race; the test now waits and asserts the
  service still `STOPPED`, and M8 and M9 were re-probed apart.
- **The amendment's text, six fresh reviews.** Round 1: "name only" and an
  overstated cause; round 2: a failed record check said to be 0, the
  catalog widening the fourth case; round 3: an inverted claim and "failures
  the setup reports" for exit 0; round 4: the threat model did not hold
  the ordinary user's levers' effect on the reversal (to Manuel); round 5:
  "by its exit alone" against the 40 line, a citation; round 6: none.

### Manuel's decisions

- 2026-11-11, in chat: #155 "Caso declarado (Recomendado)"; the 31 and
  the rewrite "31, línea nombra ambos (Recomendado)"; the record's deletion
  "El del resultado (Recomendado)"; the threat model "Enmendar threat model
  (Recomendado)".
- 2026-11-11, in chat, the text of `fcefae7`: "Sí, apruebo".
- 2026-11-11, in chat: the executable tests that run `cg-setup.exe` under
  a filtered token stay in the local gate and rust-ci, "Sí, no actúan
  (Recomendado)": they stop at 21 or 20 before reading the host, which is
  not running `cg-setup` against it (CLAUDE.md's package-ci rule).

## Self-review (CLAUDE.md compensating control 5)

Contract: SPEC-024 Amendment 2026-11-10, items 1, 5 to 9 and *Tests*; the
Amendment 2026-11-11; its second; the Amendment 2026-11-09, items 5 and 7.

| Criterion | Test | Verdict |
|---|---|---|
| 11-11.1 the stop reports its first reading, 31 included | `setup_service` `..._the_stop_reports_its_first_reading`, the 75 s tests | covered |
| 11-11.1 the rewrite: in place, sharing nothing, checked, whole, one way | `package_staging` `..._rewrites_the_record_to_running`, `..._shares_nothing`, `..._checks_the_record_as_configure_opens_it`, `..._goes_one_way` | covered |
| 11-11.1 a rewrite that fails is 40 after the removal; 31 kept (second, 3) | `..._a_rewrite_that_fails_is_40_after_the_removal`, `..._a_31_rewrites_the_record_before_its_removal` | covered |
| 11-11.2 the reversal's start after the removals, 40 when refused | `package_staging` `..._finish_starts_only_when_the_record_asks`; `setup_service` `..._the_reversals_start_*`, the verbs tests | covered |
| 11-10.9 the command line, 40 otherwise, no value quoted | `package_cli` (5) | covered |
| 11-10.1 the floor first, for each verb and a malformed command line | `package_setup_executable` `..._the_floor_comes_before_any_other_check` | covered |
| 11-10.9 each verb's elevation | `..._each_verb_checks_the_elevation_first` | covered |
| 11-10.9 `--version`; the version resource; no message table | `..._answers_its_version`, `..._carries_the_agents_version_resource`, the matrix | covered |
| 11-10.1 no static `IsWow64Process2` in the shipped binary | `package_preflight` `..._is_wow64_process2_is_resolved_at_run_time` | covered |
| Preflight's order: 26 and 29 before the staging; the stop's 31 | `package_verbs` downgrade; `setup_service` `..._conflicting_service_is_26_...`, `..._not_proved_stopped_is_31` | covered |
| #153 through the verb | `..._records_a_service_started_after_its_check`, `..._records_a_service_running_at_its_check` | covered |
| Configure's checks in order, holds, no token in hand, 40 leaves the record | `package_verbs` (order, holds, staged checks, no read, another binary) | covered |
| Configure's start, deletion and outcome (second, 4) | `setup_service` `..._configure_starts_with_the_records_fingerprints_and_observes`, `..._a_record_left_keeps_the_outcomes_code`, `..._a_refused_start_is_41_...`, `..._migration_route_...`, `..._unrecordable_log_position_is_40` | covered |
| `main`'s line with exit 0 (second, 4) | none on the binary: configure cannot run here | weak: the single error arm writes every line |
| `%ProgramFiles%` not reported is 40 | none | missing: not producible |
| 11-10 *Tests*: preflight then reverse on a hosted runner | step g | missing: next session |

Dispositions that change no code:

- **A record whose `created` names staged files while its `fingerprints`
  are empty** passes its checks; configure then checks no staged file and
  starts with no argument. Only an administrator can write it.
- **`open_checked` asks for `GENERIC_READ`** on the staged configuration
  though configure reads nothing: a share mode binds only with a data
  access, and it is what excludes writers.
- **`cg-setup`'s `-dev` text is its own package version**, while
  `--version` is cg-agent's; a test holds them equal.
- **The reversal's `ERROR_SERVICE_ALREADY_RUNNING` arm** has no test: no
  seam reaches the window between the reading and the start.

## Gates and CI

- Local Rust gate, main checkout, `-j 2`, debug off:
  - `c1d7b9e` (tree `05fc2a7`): `FMT_OK`; clippy clean; "binaries 91 passed
    520 failed 0 ignored 11"; matrix "all steps passed";
  - `ad3f511` (tree `0e4fea5`): the same, "binaries 91 passed 520";
  - `1a0afd3` (tree `b202adf`): "binaries 93 passed 532"; matrix passed;
    its amend `1129c5f` changed only the SCM test target (fmt and clippy
    rerun);
  - `a1ed63f` (tree `0702a35`): "binaries 95 passed 537"; the matrix with
    "PASS setup version tests" for each input;
  - `ca0ed37` (tree `4875f4b`): "binaries 95 passed 541"; matrix passed;
    its amends to `25511cb` changed only test files (cg-setup's tests "13
    passed 134 failed 0", fmt and clippy rerun).

  No `Cargo.lock` or manifest dependency changed; `cargo audit` did not
  run.
- The elevated gate does not apply: the diff touches none of its paths
  (`agent/cg-setup/`, `agent/cg-agent/build.rs` and `build_resource.rs`,
  `custody.rs`'s known folders, the version matrix).
- CI:
  - PR #110: `c1d7b9e` and `ad3f511`, every workflow `success`;
  - PR #112: `1129c5f`, `a1ed63f` and `25511cb`, every workflow `success`;
  - the squashed `4856d9d` and `4ca972e` on their branches, every workflow
    `success`;
  - `main`: `52443e3` markdown-lint `success`, ALL GREEN; `4856d9d`
    ts-ci and rust-ci `success`, ALL GREEN; `4ca972e` dependency-audit,
    ts-ci and rust-ci `success`, ALL GREEN; this close's commit as its
    post-push report gives it.

## Test baselines at close

- Non-elevated `cargo test --all`: 541 passed, 0 failed, 11 ignored, in 95
  binaries.
- The SCM suites in CI: cg-agent's 35, unchanged; cg-setup's 32 (19 + 13).
- Elevated `--ignored` and vitest: not run this session, unchanged at S58's.

## Debts

- Closed:
  - **#152** and **#153**, by `4856d9d` and `4ca972e` (the Amendment
    2026-11-11 implemented and wired);
  - **#155**, by `52443e3` (disclosed, Manuel's choice).
- Opened: none.
- Unchanged: **#154**, **#87**, **#150**.
- Open at close: **55**.

## Waiting on Manuel

Nothing blocks. The release-tag freeze stays.

## How Session 66 resumes

1. Run the start protocol (CLAUDE.md). `main` is this handoff's close
   commit. There is no PR. The counts are 55 open debts and 22 ADRs / 23
   SPECs.
2. **f, the SCM test of code 10 on the adoption route with the real lock**,
   in cg-agent's `package_adoption_service`, as
   `docs/handoff-session-61.md#how-session-62-resumes` gives it.
3. **g, `package-ci`'s host test**: the real `cg-setup` `preflight` then
   `reverse` on a fresh hosted runner, with a bootstrap and anchor the test
   creates and a token from which every privilege but
   `SeChangeNotifyPrivilege` is deleted (`PrivilegesToDelete`); `package-ci`
   builds `cg-setup` (its paths gain `agent/**`, `Cargo.*` and the
   toolchain); the guard follows `refuse_unless_disposable_runner`; the red
   run on a probe pull request. `package_setup_executable.rs`'s
   `run_filtered` is a start for the token.
4. Then C6 to C10 as `docs/handoff-session-59.md` orders them, with the
   wizard in C7.
5. Keep the release freeze.

## Environment and memory

- Reviewer worktrees were created under `.claude\worktrees\` by the
  workflows; each was clean and was removed with `git worktree remove`,
  and their branches, at `main`, deleted.
- A review workflow re-run with a script whose commit was fixed in it
  reviewed the wrong text; it was stopped and re-run with the new SHA.
- `| tail` after `markdownlint-cli2` hid its exit code once, and a commit
  with a lint error reached the owner-review branch; it was amended. The
  lint's exit is now checked on its own.
- `__COMPAT_LAYER=WIN7RTM` in a child's environment lowers the version
  `RtlGetVersion` reports to 6.1; `CreateProcessAsUserW` with a restricted
  copy of the caller's token works unelevated.
