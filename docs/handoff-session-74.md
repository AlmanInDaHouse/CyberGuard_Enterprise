# Handoff — Session 74

Session 74, 2026-11-15, wrote the first two of C9's three commits, under SPEC-024 Amendment
2026-11-15 (the purge): the service's purge start, with a fix from its review, and `cg-setup purge`
with the codes 46 and 47. They landed on `main`. The third, the bundle's `Purge` step and the
package tests, is Session 75's: it did not fit with quality beside the reviews, the probe and the
landing of the first two. No uninstallation runs the purge yet. No tag was pushed: the release
freeze stays.

## Start and resulting state

- Start: `main` = `origin/main` = `8bef33e`, clean, no stash, no branch but `main`, no PR; the
  tip's CI ALL GREEN (markdown-lint); 22 ADRs / 23 SPECs, 56 open debts (56 rows), no Known CI
  debt. The repo agreed with Session 73's handoff. 2.0 GB of 15.8 GB of memory free, 30 GB on
  `C:`; Docker Desktop down (Manuel's `aura_*` containers not running, not touched).
- `C:\Users\manul\cg-s72-deps` was no longer held, and was removed (`rmdir`).
- Resulting `main`, in order, each fast-forwarded from PR #135 and pushed alone with its CI green:
  - `a2b2016`, the service's purge start;
  - `851664a`, its review's fix;
  - `c3307cb`, `cg-setup purge` and the codes 46 and 47;
  - this close's commit.
- PR #135 (`feat/c9-purge`) shows merged; its branch is deleted. Probe PR #136 is closed without
  landing; its branch is deleted. No other branch is left.
- Catalogs: 22 ADRs / 23 SPECs, unchanged.

## What landed

### The service's purge start (`a2b2016`, fixed by `851664a`)

`agent/cg-agent/src/service/purge.rs`, with `migration::parse_start`'s `Start::Purge` and the
purge's `Point`s, codes 14 and 15 and `ServiceExit::Purged` in `service/mod.rs`, and the purge start
in `service/windows.rs` before the configuration is read. `--purge` alone; with another argument
code 2, `cg-agent: invalid purge arguments: --purge is given with another argument`. The start takes
the instance lock (10, or 1 with `the purge's token cannot create the lock`), holds `<P>` and the
data root with `FILE_LIST_DIRECTORY` sharing no deletion after the file log's check (13, `cg-agent:
the purge's directories fail their check: <reason>`), reads the state with `layout::classify` (14
for an untrusted state, a migration in progress or a console layout), then walks point 5's names in
point 6's order: `agent.toml.setup`, `agent.toml`, `agent.toml.hygiene-tmp`; the legacy directory
under `agent.toml`'s policy, its `agent.toml` first; `identity\`; each of these steps ending the
purge at the first name it leaves; then the data root's other names, `logs\`, the data root and
`<P>`, going on past what it leaves. Each file is opened without following a reparse point, checked
(regular, one link, owner SYSTEM or Administrators) and deleted through that handle; each directory
is opened again after its hold closes, checked, listed and deleted only when empty. No removal
begins after `purge::BOUND`, 120 s from the lock; what it leaves or does not reach is named, at
most 20 names then their number (15). `purge::start` maps a panic and an injected crash to 15,
`cg-agent: the purge ended abnormally`. 0 writes entry 1001, `cg-agent: the purge removed the
product's data and left nothing`; any other code 1002. Never `RUNNING`: `START_PENDING` advances
with each step through the existing status thread.

`851664a` is the review's fix: past the first three steps the walk returned before it named `<P>`'s
other entries, and past the bound it never named the data root and `<P>` as not reached.

Tests: `agent/cg-agent/tests/install_ac_012_purge.rs`, 36, unelevated over temporary roots with the
current user in SYSTEM's place (`migration_fixtures`).

### `cg-setup purge` (`c3307cb`)

`cli.rs`: `purge` takes `--confirm=` at most once; absent or empty is not given, `yes` confirms,
any other value 40. `main.rs`: unconfirmed, it exits 0 after the platform and the elevation, before
`Machine::system()`. `verbs.rs::purge`: `<P>` absent 0; `classify` (27, 28); the product service
(26; absent 46); the stop (31); the start with `--purge`; `scm::wait_stopped`, reading every 250 ms
for `service::PURGE_WITHIN`, 300 s; `verbs::outcome` maps the end (0; a code of
`PURGE_BEFORE_REMOVAL`, 1, 2, 10, 13, 14, or a refused start 46, the service started again if it
was running, `verbs::restart_note` naming how; 15, another code, another Win32 error, an unread
state or any state at the end 47). `codes.rs`: `PURGE_REFUSED` 46, `PURGE_INCOMPLETE` 47.
`agent/cg-setup/README.md` names the verb.

Tests: `package_cli.rs` (2), `package_verbs.rs` (5, with `outcome` and `restart_note`),
`install_ac_002_setup_inputs.rs` (46 and 47 among item 7's codes), and six SCM tests in
`tests/setup_service/mod.rs`, whose test service stands in for the agent's purge start (`purge`:
`pending_ms`, `code`, `die`).

## The probe (PR #136, closed without landing)

`probe/c9-purge-scm`, rust-ci's Windows job, a step with `continue-on-error` (CLAUDE.md,
*Compensating controls* §1):

- `9e7f553` (run 43722314970): the five SCM tests against a verb that did nothing: 5 failed, for
  the behaviour missing (the service `RUNNING`, 4, where `STOPPED`, 1, was asserted; no refusal).
- `b7dfe7d` (run 43724630148): the SCM tests as the review fixed them against the verb before its
  fixes (`80a7ecf`'s): 4 passed; 2 failed on the defects, 47's line "still pending, in state 2, 20
  seconds after              its start" and 46's "the reversal could not start the service".

The unelevated tests were seen failing locally first: `install_ac_012_purge`, 29 of 32 against a
stub (`purge::run` taking the lock alone; `parse_start` without `--purge`); the review fix's two,
against `a2b2016`'s walk; `cg-setup`'s, 4 of 5 against a stub verb (the fifth, `<P>` absent is 0,
holds of the stub), and the run of spaces in 46's and 28's lines against `80a7ecf`.

## Decisions

### Manuel's

- None this session. #160 and #162: no answer; both stay waiting.

### Anticipated (Claude Code)

- **Two of three commits this session.** C9's third commit (the bundle step, its mutant probe and
  `package-ci`'s tests) did not fit with quality beside the reviews, the probe and three landings of
  about an hour of CI each (the checkpoint rule Manuel validated). Discarded: all three in a rush.
  The first two are a library and a verb with no consumer until the third, as C5's steps were.
- **The purge's `Environment` is the migration's**, with the bound passed beside it and the purge's
  points added to `migration::Point`: the service's `ServiceWatch` and the tests' `Script` serve it
  unchanged. Discarded: an `Environment` of the purge's own.
- **The unconfirmed purge returns in `main.rs`, before `Machine::system()`**, so that it reads no
  known folder either. Discarded: inside the verb.
- **The SCM probe's stub was a variable check** (`CG_PROBE_PURGE_IMPLEMENTED`), which keeps every
  line reachable for clippy; the unelevated verb tests were skipped in that probe's `cargo test`.
- **The checkpoint edge is not fixed**: both reviews of the service found that `START_PENDING`'s
  120 s cap could stop the checkpoint during `classify` if opportunistic locks under `<P>` held
  five opens for 30 s each, which needs an administrator; the SCM acts on no checkpoint and
  `cg-setup` reads only the state. Recorded, not a debt.

### Reactive corrections

- A test asserted that `agent.toml.setup` never outlives `agent.toml` on a fresh host that had no
  `agent.toml`: the precondition was added (`Had`).
- A test's premise was wrong: a legacy directory created with a DACL whose SDDL has no `P` passes
  `agent.toml`'s policy as `custody` reads it, as the migration's `counts` does; the test now uses
  a protected DACL allowing Users. The reading of the protected bit is #163.
- The review of (a): the walk's early returns (fixed in `851664a`, with two tests seen red).
- The review of (b): four lines held runs of spaces (a `\` continuation inside a JS template
  literal of the edit script, which JS drops with the newline but not the next line's indentation);
  1066 with code 0 read as 47; 15's line claimed entry 1002 names what was left for an abnormal end
  too; a `STOPPED` read at the deadline read as "still pending"; 46's restart line named "the
  reversal". Fixed in `c3307cb` (folded before its first push), with `outcome` and `restart_note`
  made pure and unit-tested, and an SCM test of a restart that fails.
- The heredoc backslash defect hit once more (a test file's `C:\\x`); the edits went through
  scripts written with Write.

## Self-review (CLAUDE.md compensating control 5)

### Claude Code's matrix (contract: SPEC-024 Amendment 2026-11-15)

| Criterion | Test | Verdict |
|---|---|---|
| P4 `--purge` alone / with another argument, code 2's line | `install_ac_012_purge.rs` `purge_alone…`, `purge_with_another_argument…` | covered (the `failed(2, …)` glue is `serve`'s existing one) |
| P4 lock first, once; 10 and 1 | `purge_within` (lock before any point), `a_refused_lock…` | weak: `purge_start`'s lock mapping has no test of its own |
| P4 `<P>` and data root: no reparse, check, 13 and its line | `a_junction_as_p_or_the_data_root_is_code_13`, `a_console_layout_whose_directories_fail_is_code_13` | covered |
| P4 held sharing no deletion | `p_and_the_data_root_are_held_sharing_no_deletion` | covered |
| P4 14 for untrusted, migration in progress, console layout, nothing changed | `states_the_purge_refuses_are_code_14…` | covered |
| P4 no config, log, ETW, `RUNNING` | none (structural: the early return in `serve`) | weak: the bundle test in C9's third commit runs the real service |
| P4 30 s cancellation | `an_open_held_by_an_oplock_is_cancelled_and_left`, `a_file_in_use…` | covered for opens; a blocked deletion not driven |
| P4 120 s bound, not reached named | `no_removal_begins_after_the_bound` | covered |
| P4 abnormal end 15 | `an_abnormal_end_is_code_15` | covered |
| P5 closed list, ASCII case | `every_name_is_removed_in_point_6_s_order`, `names_are_compared_but_for_ascii_case` | covered |
| P5 directories' checks | junction tests, `a_legacy_directory_that_fails_its_policy_is_left`, `a_directory_allowing_others_is_left` | covered |
| P5 files: link, owner, reparse | `a_file_with_a_second_link…`, `a_file_of_another_owner…` (CI elevated), junction entries | covered |
| P5 other entries left, named, unopened | `an_unknown_name_and_directory…`, `names_left_in_the_data_root_and_in_p_are_all_named` | covered |
| P5 directory re-listed before removal | `an_entry_that_appears_before_a_directory_s_removal_is_left` | covered |
| P6 order, step stops, crash and failure at each step, states read by 3.2, plain start not code 5, rerun | the six `…_interrupted_at_each_step_goes_on`, the three `…_left_ends_the_purge_at_its_step` | covered |
| P7 1001/1002, 20 names, no content | `the_entries_1001_and_1002`, `the_line_names_at_most_20…`, `no_line_quotes_a_file` | covered (weak for content: one case) |
| P3 `--confirm=` | `package_cli_purge_takes_confirm_once`, `…refuses_any_other_command_line` | covered |
| P3 unconfirmed reads nothing | none (`main.rs` before `Machine::system()`) | weak: the bundle test in C9's third commit runs it |
| P3.1 to 3.3 | `package_verbs_purge_with_no_product_root_is_0`, `…refuses_the_states…`, `…no_product_service_is_46`, SCM `…conflicting…26…31` | covered |
| P3.4 31 leaves the service | SCM `…one_not_stopped_31` (`STOP_PENDING` after) | covered |
| P3.5 mapping, restart, wait | `package_verbs_purge_outcome…`, `…restart_note…`, SCM `…ends_0_is_0`, `…46_and_restarts`, `…is_47`, `…refuses_is_46`, `…restart_that_fails…` | covered; an unread state is unit-tested only |

### The fresh reviewers (the fixed code prompt, isolated worktrees)

- (a) at `a2b2016`: List 2a and 2b, the walk's early returns: **fixed** (`851664a`). 2c, the
  checkpoint edge: recorded (above). Weak items: the `windows.rs` glue, the 30 s path, the lock
  mapping: recorded; the bundle test covers the glue end to end.
- (a) again at `851664a`, after the fix: no contradiction; the checkpoint edge again (recorded);
  `purge::run` public in the library `cg-setup` links (nothing calls it there; an elevated
  administrator is no boundary); the unreachable `Stop::Crashed` arm in `purge_start`; a panic in
  a `custody::bounded` worker read as a name left, not an abnormal end (15 either way).
- (b) at `80a7ecf`: the runs of spaces, 1066/0, 15's line, `STOPPED` at the deadline, the
  restart's text, the README, 46/47 not among item 7's codes, weak SCM assertions: **all fixed** in
  `c3307cb`. Not fixed: the unread-state path in an SCM test (unit-tested through `outcome`).
- (b) again at `c3307cb`, after the fixes: no contradiction. Recorded, for C9's third commit, which
  touches these tests again: a `STOPPED` read at or after the 300 s reads as 47 (conservative; a
  rerun is 0); `set_start_type` sits under `set_command_line`'s doc comment; `package_cli.rs`'s
  header does not name `purge`; the unconfirmed route and `cg-setup.exe purge`'s platform and
  elevation order have no executable test (the bundle job runs the first); a purge process that
  dies before it connects reads as a refused start, 46, which removed nothing; the SCM test of a
  death may flake if `StartServiceW` sees it (it passed on probe #136).

## Gates and CI

- Local Rust gate at `a2b2016`: `cargo fmt --all -- --check` clean; `cargo clippy --all-targets
  --all-features -- -D warnings` "Finished"; `cargo test --all` 98 binaries, 601 passed, 0 failed,
  11 ignored; `node .github/scripts/agent-version-matrix.mjs` "agent version matrix: all steps
  passed". At `851664a`: 98 binaries, 605 passed, 0 failed, 11 ignored. At `c3307cb`: 98
  binaries, 612 passed, 0 failed, 11 ignored; the version matrix "all steps passed";
  `markdownlint-cli2 agent/cg-setup/README.md` "Summary: 0 issues in 0 files".
- No elevated gate: the change touches no path of *Developer-local SPEC-005 marquee validation* §5
  (only `service/`, `cg-setup`).
- CI on PR #135, each SHA pushed alone: `a2b2016` ts-ci, deploy-ci, rust-ci, package-ci success;
  `851664a` the same four success; `c3307cb` markdown-lint, ts-ci, deploy-ci, rust-ci,
  package-ci success (rust-ci's Windows job ran the six purge SCM tests). Each ALL GREEN.
- CI on `main`'s pushes: `a2b2016` ts-ci, rust-ci, package-ci success; `851664a` ts-ci,
  rust-ci, package-ci success; `c3307cb` markdown-lint, ts-ci, rust-ci, package-ci success. Each
  ALL GREEN. This close's commit: as its post-push report gives it.
- Probe #136: `9e7f553` and `b7dfe7d`, every workflow success, the probe step's red read from
  the Windows job's log (above).

## Test baselines at close

Non-elevated `cargo test --all`: 98 binaries, 612 passed, 11 ignored. Elevated `--ignored` 7 and
vitest 86 files, 504 passed, 2 skipped, unchanged from Session 72; `deploy/windows/test` unchanged.

## Debts

- Opened: **#163**, `custody::check` passed a directory created with the SDDL
  `D:(A;OICI;FA;;;<user>)` against a policy with `protected: true`, while `GetFileSecurityW`
  renders its DACL without `P` (Session 74's test, now rewritten): read what `GetSecurityInfo`
  reports of `SE_DACL_PROTECTED` and whether a DACL inheritance can still reach passes the
  configuration and identity policies.
- Closed: none. Narrowed: none.
- Open at close: **57**.

## Waiting on Manuel

- **#160's text** and **#162's order**, as Session 72 put them.
- The release freeze stays; Authenticode stays ADR-0022's Open question 1.

## How Session 75 resumes

1. Run the start protocol (CLAUDE.md). `main` is this handoff's close commit. There is no PR. The
   counts are 57 open debts and 22 ADRs / 23 SPECs.
2. Manuel's answers on #160 and #162, if given.
3. **C9's third commit**, on a branch `feat/c9-purge-package` with a PR, under SPEC-024 Amendment
   2026-11-15, points 1, 2, 7 and *Tests*' `package-ci` part:
   1. `deploy/windows/bundle/Bundle.wxs`: `<Variable Name="CG_PURGE" Type="string" Value=""
      bal:Overridable="yes" />` beside the others; after the `RollbackBoundary`, before
      `Configure`, `<ExePackage Id="Purge" CacheId="cg-setup-purge-$(var.ProductVersion)-$(var.BuildId)"
      PerMachine="yes" Vital="yes" DetectCondition="1" SourceFile="$(var.SetupExe)"
      Name="purge\cg-setup.exe" InstallArguments="purge"
      UninstallArguments="purge --confirm=&quot;[CG_PURGE]&quot;" />` (probe #134's form,
      `refs/remotes/origin/pr134`, plus `InstallArguments`); the comments' "none persisted" and
      chain description updated.
   2. `bundle/theme/CyberGuardTheme.wxl`'s `FailureCodeText`: "for Preflight, Agent, Purge or
      Configure".
   3. `deploy/windows/test/projects.test.mjs`: the chain's order with `ExePackage:Purge` after the
      boundary, its attributes and the variable.
   4. The release check (`.github/scripts/release-windows.mjs`, `deploy/windows/test/release-*`):
      the setup now carries three `cg-setup.exe` payloads; its fixtures and the matrix
      (`release-check-matrix.mjs`) with a third.
   5. `agent/cg-setup/tests/package_bundle_host.rs` (or a target of its own in the `bundle` job,
      with CLAUDE.md's facts): an operational installation, then `-uninstall CG_PURGE=yes`, 0, with
      `<P>`, the service, the binary, the MSI and the setup gone and Burn's log holding
      `--confirm="yes"`; a reinstallation with no input, 22; a junction to a canary planted in the
      data root, 47, the product installed, the service stopped, the step's rollback exiting 0,
      then the junction removed and the purge run again, 0; `CG_PURGE=no`, 40, nothing changed,
      the agent `RUNNING`; an upgrade, a repair and a second build with `CG_PURGE=yes`, the data
      kept and no `--confirm="yes"` in Burn's logs; install_ac_012's first half strengthened (a
      default uninstallation keeps `agent.toml`, `server-ca.pem`, `logs\`, the data root and `<P>`
      with their security descriptors, and the reinstallation enrolls nothing and keeps the agent
      ID and key). Probe #134's test (`agent/cg-setup/tests/package_purge_probe.rs` at
      `refs/remotes/origin/pr134`) shows how to drive Burn and read its log.
   6. Seen red first on a probe pull request (the Amendment 2026-11-12's *Tests*): a control leg
      (the package without the step) and mutant legs, each a `cg-agent` or `cg-setup` build of its
      own: the junction followed (the purge's file open without `FILE_FLAG_OPEN_REPARSE_POINT`),
      `agent.toml` removed after the identity, `CG_PURGE` ignored (the step with no
      `UninstallArguments` value), an upgrade that purges, and the guard. The probe also reads what
      #134 did not: a fresh installation with `CG_PURGE=yes`, and the rollback running
      `InstallArguments` `purge` (expected: `cg-setup.exe purge`, 0).
   Then the self-review in two halves, its fixes, another fresh pass, and landing one commit per
   push, CI green each.
4. Then C10, the runbooks, as Session 73's plan lists it, with the purge's command and codes 46,
   47, 14 and 15, what point 8 says it is not, and #163 if it changes a check.
5. Keep the release freeze.

## Environment and memory

- Docker Desktop stayed down; nothing of Manuel's was touched.
- `C:\tmp\s74-probe-b`, the probe's worktree, is removed from git but its empty directory is held
  by a process of this session's shell; remove it next session.
- `target\` holds debug builds without debug info (`CARGO_PROFILE_*_DEBUG=0`).
- Updated memory: the session's close.
