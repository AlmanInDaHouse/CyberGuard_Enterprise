# Handoff — Session 59

Session 59, 2026-11-09 to 2026-11-10, did C3 of the package (SPEC-024
Amendment 2026-11-09): the read-only classification of the host's state
(`e7067be`, item 3) and the service's adoption of the staged configuration
(`8994b50`, item 6), with the SCM target's red evidence from a probe pull
request. On the way, `ubuntu-latest` began serving Ubuntu 26.04 and broke
deploy-ci's image build; its images job is pinned to `ubuntu-24.04`
(`ae4110c`, #149). C4 (`cg-setup`) did not start: it would not have fitted
with the quality bar in this session.

## Start and resulting state

- Start: `main` = `origin/main` = `ab0f534`, clean, no stash; the local
  branch `worktree-wf_2d2e581a-62e-2` and its worktree, both clean with
  nothing unlanded, removed as `docs/handoff-session-58.md` asked; no PR
  open; tip CI green; 22 ADRs / 23 SPECs; open debts 52; Known CI debt none.
  The repo agreed with the handoff.
- Resulting `main`: `ae4110c` (the runner pin, PR #97), `e7067be` (layout,
  PR #94), `8994b50` (adoption, PR #96), then this close's commits. No PR
  open, no branch but `main`, no worktree left.
- Catalogs: 22 ADRs / 23 SPECs, unchanged.

## What landed

### The host's state (`e7067be`, PR #94)

- `agent/cg-agent/src/service/layout.rs`: `classify` returns item 3's first
  state that applies (untrusted, migration in progress, console layout,
  enrolled, pending, fresh). `Names` reads `<P>` by a handle that does not
  follow a reparse point, and the names of `<P>` and the data root from
  their listings, so no object under them is opened, `identity\` included.
  A data root that cannot be listed leaves the earlier states readable.
- `agent/cg-agent/src/custody.rs`: `Shape`, `shape` and `listing`.
- `agent/cg-agent/tests/package_layout.rs` (17 tests).

### The adoption (`8994b50`, PR #96)

- `migration::parse_start` and `Start` (`Plain`, `Migrate`, `Adopt`):
  `--adopt-staged-config` once with one or more fingerprints, the
  migration's form and exact-set rule; the migration's own parse unchanged.
- `agent/cg-agent/src/service/adoption.rs`, items 6.1 to 6.6 in order:
  the names (code 2 on a console layout or a migration in progress, 13 on a
  data root that cannot be listed or a reparse point), the lock, the name
  `identity` (5), no staged configuration (2), a legacy directory that
  counts (5, the Amendment 2026-11-05, item 4); each staged file read once
  through `custody::read_held`, a handle that shares read alone and is held
  until the renames; the configuration's checks (2) and the anchor's exact
  set (6); `<P>` and the data root closed as item 2.1 does (13); each file
  made SYSTEM's through its handle with its link count read again (5); the
  names read again, any final file deleted through a handle, the
  configuration first, and the two renames without replacing, the
  configuration last (5).
- `service/windows.rs`: an adoption runs before `migration::prepare`, under
  one lock closure that takes the lock once; the unprivileged lock line names
  the route.
- `InstalledPaths::staged_config` and `staged_anchor`.
- Tests: `agent/cg-agent/tests/package_adoption.rs` (38, unelevated, a
  failure and a crash injected at each of item 6's points), and the SCM
  target `agent/cg-agent/tests/package_adoption_service.rs` (5, `scm-tests`,
  in rust-ci's Windows step), with `open_service_to_start` in
  `tests/scm/mod.rs`.

### The runner pin (`ae4110c`, PR #97)

`.github/workflows/deploy-ci.yml`: the images job runs on `ubuntu-24.04`;
the release dry run stays on `ubuntu-latest`.

## The probe (PR #95, closed without landing)

`5e967ce` = `ab0f534` with the SCM target, its `[[test]]` entry and the
helper, and rust-ci's SCM step replaced by `continue-on-error` over that
target alone: run 42719395282, Windows job 129509186414. Literal lines:

- `test package_adoption_a_start_with_the_arguments_adopts_and_enrolls ... FAILED`
  (`CgAgentTest-6604-0 did not reach state 4 within 60s: state 1, exit 1066 / 2`);
- `test package_adoption_the_arguments_on_a_console_layout_are_code_2 ... FAILED`
  (entry 1002: `a start argument is neither --migrate-legacy nor --trust-anchor-sha2...`);
- `test package_adoption_the_arguments_on_an_enrolled_host_are_code_5 ... FAILED`
  (`left: (1066, 2)`, `right: (1066, 5)`);
- `package_adoption_a_start_without_the_arguments_leaves_the_staged_files`
  and `package_adoption_the_staged_files_and_the_start_right_are_denied_to_others`
  passed: neither depends on the implementation (a start without arguments,
  and the planted files' and the default service DACL's access).

## Decisions

### Anticipated (Claude Code)

- **The classification in `cg-agent`'s `service::layout`**, read by the
  adoption's step 6.1 now and by `cg-setup` in C4. Discarded: a copy in
  `cg-setup`, which the service could not share.
- **Names from the listings, no object under `<P>` opened.** Discarded:
  opening each name, which an opportunistic lock makes wait and which would
  open `identity\`.
- **A host that cannot be read is `Untrusted`** (27 for preflight).
  Discarded: code 40, "could not check its own state"; C4 maps the states to
  codes and may revisit it.
- **The adoption's lines:** argument refusals use the Amendment 2026-11-05,
  item 1's `cg-agent: invalid migration arguments:` prefix, as item 6.1
  says; the anchor is `cg-agent: tls: client configuration failed: adoption:`;
  code 5 is the identity line and 13 the file log's. Discarded: a new
  `invalid adoption arguments` prefix (S58's List B left item 6's lines to
  C3).
- **A reparse `<P>` or data root is 13 before step 3**, as item 2.1's
  condition, so nothing is read through it. Discarded: code 2 in item 6.3's
  order, which reads the data root's names through the reparse point.
- **A legacy directory that counts beside a data root with no identity is 5
  before any change** (the Amendment 2026-11-05, item 4, which item 6's
  *Effect* does not except). Found in Claude Code's own pass. Discarded:
  letting `prepare` refuse after the adoption committed.
- **The names read again under the lock.** Discarded: the one reading item
  6.1 places before it.
- **Test names `package_*`**, as S58's: preconditions of install_ac_001/011,
  not those criteria.
- **The SCM target's red evidence by probe PR #95.**
- **deploy-ci's images job pinned to `ubuntu-24.04`.** Discarded: changing
  the dashboard's build without knowing what 26.04 changes, and pinning the
  release dry run too, which passed on 26.04 and is the only CI signal for
  `release.yml`'s path on that label (the pin's reviewer).

### Reactive corrections

- Long Bash heredocs failed (the auto-memory's note): edits went through
  node scripts written with the Write tool, re-cut twice where rustfmt had
  reflowed the text a script searched for.
- The layout review: a data root that could not be listed hid a migration in
  progress (fixed with `unlisted`), and three weak tests (fixed). The
  adoption reviews: step 6 read an unlistable data root as having no
  `identity` (fixed: code 5), a refusal beside an interrupted migration
  lacked the Amendment 2026-11-07 (second), item 5's sentence (fixed), and a
  line said the opposite of its failure (fixed). Each fix's test was seen
  failing without it.
- The local gate on `51f177c` was killed by the system for low memory
  during the version matrix, its last step; Manuel stopped his k8s stack
  and the matrix ran again, green.
- deploy-ci failed on `51f177c` with `[Error: kill EACCES] { errno: -13,
  code: 'EACCES', syscall: 'kill' }` in the dashboard's `next build`. A
  re-run failed the same way, and so did a re-run of `5eb0ceb`, green
  before: the passing run was on `Image: ubuntu-24.04`, `Version:
  20261101.360.1`, the failing ones on `Image: ubuntu-26.04`, `Version:
  20261101.186.1`, same code and base image. The pin's run passed. The
  pin's commit message was reworded twice before landing, once to cite the
  runs right and once when its scope narrowed.
- The stack was rebased onto the pin; `git diff --quiet` of each rebased
  commit against the gated one, but the pin's file, returned 0 for both
  ("layout tree equals the gated one but the pin", "adoption tree equals
  the gated one but the pin"), so the local gates carry; CI ran on each
  rebased SHA.

### Manuel's decisions in chat

- 2026-11-10, on the matrix the system killed: "A, ya paré el k8s, relanza
  la matriz".

## Self-review (CLAUDE.md compensating control 5)

### The host's state (`e7067be`)

Claude Code's matrix (item 3, as `classify` implements it; `package_layout.rs`):

| Criterion | Test | Verdict |
|---|---|---|
| A reparse point refused first, not followed | `a_reparse_point_is_refused_first_and_not_followed` | covered |
| By name, `identity\` not opened | `identity_is_not_opened`, `names_are_read_without_opening_them` | covered |
| Migration in progress | `a_staging_directory_is_...`, `a_legacy_name_with_no_data_root_...`, `..._is_no_migration` | covered |
| Console layout, before the check | `any_identity_file_directly_...`, `..._comes_before_the_directories_check`, `..._count_whatever_they_are` | covered |
| Untrusted | `..._no_directory_is_untrusted`, `..._failing_the_services_check_...`, `a_legacy_directory_beside_...`, `..._cannot_be_listed_...` | covered |
| Enrolled, pending, fresh | `an_identity_name_is_enrolled`, `a_kept_legacy_...`, `a_configuration_without_identity_...`, `a_host_with_nothing_is_fresh` | covered |
| Exit codes, inputs, staging, record, stop | none | `cg-setup`'s (C4) |

Fresh reviewer, on `5369fa9`: no contradiction but one: the first that
applies did not hold when the data root could not be listed (fixed). Weak,
fixed: `agent.legacy` read by name whatever it is, its state's order against
the two earlier ones, and the negatives for the setup's own files. Not
blocking, recorded: a swap to a junction between the shape and the listing
yields at most a refusal, which item 3 accepts; "unreadable is untrusted" is
the decision above. The second fresh pass below covered these fixes.

### The adoption (`8994b50`)

Claude Code's matrix (item 6; `package_adoption.rs`, `PS` the SCM target):

| Criterion | Test | Verdict |
|---|---|---|
| Arguments, form, exact set | `takes_the_flag_once_...`, `refuses_arguments_of_any_other_form`, PS `..._adopts_and_enrolls` | covered |
| 6.1 code 2 before the lock, read again under it, item 5's sentence | `a_console_layout_is_code_2_...`, `a_migration_in_progress_...`, `names_are_read_again_under_the_lock`, `a_refusal_beside_a_migration_...`, PS `..._console_layout_are_code_2` | covered |
| 6.2 the lock, code 10 | `takes_the_lock_before_anything_changes` | weak: an injected closure |
| 6.3 identity 5, no configuration 2 | `a_name_identity_is_code_5`, `..._comes_before_no_staged_configuration`, `no_staged_configuration_is_code_2`, PS `..._enrolled_host_are_code_5` | covered |
| 6.4 custody, the reads, the checks | `..._failing_custody_is_code_2/6`, `..._of_another_owner_...`, `..._held_for_writing_...`, `..._under_an_oplock_...`, `..._symbolic_link_...`, `..._for_what_it_says_...`, `..._not_matching_exactly_...`, `..._cannot_be_changed_while_it_is_held` | covered |
| 6.5 close, restrict, links | `directories_that_cannot_be_closed_...`, `a_link_made_while_restricting_...`, PS `..._adopts_and_enrolls` | covered |
| 6.6 names, deletions, renames | `a_name_identity_that_appears_...`, `a_listing_that_fails_stops_step_6`, `a_final_that_cannot_be_deleted_...`, `a_rename_never_replaces`, `a_final_reparse_point_...`, `a_fresh_host_adopts_in_the_contracts_order` | covered |
| A failure and a crash at each step | `a_failure_at_each_step_...`, `a_crash_at_each_step_...`, `retries_after_an_interruption` | covered |
| A start without arguments never adopts | `a_start_without_arguments_adopts_nothing`, PS `..._without_the_arguments_...` | covered |
| *Tests*: access and `SERVICE_START` | PS `..._denied_to_others` | covered, with a positive control |
| *Tests*: the real `cg-setup` stages with no privilege | none | C4/C5 |

The adoption had three review passes with the fixed code prompt, and a
read-only workflow of three lenses (custody, crash and retry, test
strength), each finding given a refuter:

- On `fc95307` the fixed reviewer and the workflow agreed on step 6 reading
  an unlistable data root as no `identity`, and the workflow on item 5's
  missing sentence; the reviewer on the inverted line. All fixed (see
  *Reactive corrections*). Weak points fixed then: the anchor's rename
  without replacing, the handles held through the deletions, the file log
  not opened before an adoption, `SERVICE_START` with a positive control,
  the names read again under the lock, a symbolic link to a passing file,
  a directory of a console or configuration name, `agent.legacy` beside a
  file `agent`.
- On `51f177c`, after the fixes: no contradiction. Two deviations, recorded:
  a reparse `<P>` is 13 ahead of 6.3's 2 (the decision above), and steps 1
  to 3 do not renew `START_PENDING`'s cap, which only a stalling handle on
  `<P>` could exhaust. No further pass followed, since those fixes changed
  no code.
- Weak, not blocking, with their reason and destination: the real lock's
  code 10 on the adoption route is the migration's closure, unexercised by
  an SCM test, as on the migration route (C5's SCM tests); no privilege is
  sampled at `RUNNING`, and `src` holds no `AdjustTokenPrivileges`;
  "runs on its earlier `agent.toml`" and "code 2 at the absent file" are
  asserted at the files and `prepare`, the service's read being the
  Amendment 2026-11-04's, tested by its own SCM tests; the owner refusal is
  shown for the configuration, the anchor sharing `staged_policy`; the
  product service's DACL is the MSI's (C6); `classify`'s directory policy is
  the caller's (C4 passes the service's); the checks before 6.5 and the
  renames by name: a swap through a parent junction before the close is
  stopped by the next start's custody check (codes 2 and 6).

### The runner pin (`ae4110c`)

Contract: the defect statement. Fresh reviewer: the release dry run did not
fail on 26.04 and its pin would hide `release.yml`'s path (fixed: only the
images job is pinned); the comment pointed to a debt row not yet written
(fixed: no pointer); `docs/specs/SPEC-023-production-deployment.md:342`
says the bundle's tests run on `ubuntu-latest` and `release.yml` stays
there (recorded in #149).

## Gates and CI

- Local Rust gate on `5eb0ceb` (layout): `FMT_OK`; clippy `Finished`, no
  warning; "binaries 81, passed 368, failed 0, ignored 11"; "agent version
  matrix: all steps passed".
- Local Rust gate on `51f177c` (adoption): `FMT_OK`; clippy `Finished`, no
  warning; "binaries 82, passed 407, failed 0, ignored 11"; the matrix,
  re-run: "agent version matrix: all steps passed".
- The elevated gate does not apply: `git diff --name-only ab0f534 8994b50`
  touches none of its paths (no `etw/`, `cges/`, `logon/`, `delivery.rs`,
  `paths.rs`, `lib.rs`, `envelope.rs`, `services/`, `rules/`).
- CI: `5369fa9` and `fc95307` ALL GREEN (superseded). `51f177c`: rust-ci,
  ts-ci, dependency-audit `success`, its SCM step with the five
  `package_adoption_service` tests `ok` (run 42727463861); deploy-ci
  `failure` (run 42727464231, both attempts, the runner, above). `ae4110c`
  on its PR: deploy-ci `success`; on `main`: deploy-ci `success`.
  `e7067be` on its PR: ts-ci, rust-ci, deploy-ci `success`; on `main` ALL
  GREEN. `8994b50` on its PR: dependency-audit, ts-ci, deploy-ci, rust-ci
  `success`; on `main` ALL GREEN.

## Landing

- `ae4110c`, `e7067be`, `8994b50`, each a fast-forward of `main`, one per
  push, CI green on the branch before and on `main` after. PRs #94, #96 and
  #97 closed, #95 closed unlanded; their branches deleted.

## Test baselines at close

- Non-elevated `cargo test --all`: 407 passed, 0 failed, 11 ignored.
- The SCM suite in CI: 35 tests (30 + `package_adoption_service`'s 5).
- Elevated `--ignored` and vitest: not run this session, unchanged at S58's.
- Unelevated, the four capture marquees fail with exit 1 on this host
  (CLAUDE.md *Environment facts*); not re-run.

## Debts

- Closed: none.
- Destination changed: **#87**, narrowed: the service's adoption has landed
  (`8994b50`). Open for `cg-setup`, the MSI and bundle (install_ac_001/011/015)
  and the migration's runbook.
- Opened: **#149**, `ubuntu-latest` serves Ubuntu 26.04, where the
  dashboard's `next build` dies with `kill EACCES` under deploy-ci's plain
  `docker build`; that job is pinned to `ubuntu-24.04` (`ae4110c`). Open:
  the cause; `release.yml`'s jobs stay on `ubuntu-latest`, whose image build
  only the release dry run, on buildx's container builder, still exercises;
  `docs/specs/SPEC-023-production-deployment.md:342`'s `ubuntu-latest`.
  Reopens when the cause is known, or before C8 changes `release.yml`.
- Open at close: **53**.

## Waiting on Manuel

Nothing blocks. The release-tag freeze stays. The k8s stack Manuel stopped
for the matrix can be started again.

## How Session 60 resumes

1. Run the start protocol (CLAUDE.md). `main` is this handoff's close
   commit; no PR; 53 open debts; 22 ADRs / 23 SPECs. Check memory before a
   full gate: with the k8s stack up the system killed the gate's matrix.
2. The contract is SPEC-024 Amendment 2026-11-09. Continue, one commit per
   push, each through a PR, contract tests first:

   | # | Contents | Gates |
   |---|---|---|
   | C4 | `agent/cg-setup`: bootstrap file, validation, the staged TOML; preflight's states from `service::layout::classify` with the service's directory policy, their codes (20-31) | Rust gate, `cargo audit` |
   | C5 | Staging, the SCM client, the observer, the verbs; an SCM test of code 10 on the adoption route with the real lock; the real `cg-setup` staging with no privilege | Rust gate, hosted tests with a probe PR |
   | C6 | The MSI and `package-ci`'s install tests (the product service's DACL and `SERVICE_START`); lock files on the `.wixproj`s; `agent/cg-setup/**` in its paths | Hosted, probe PR |
   | C7 | The bundle, a token canary, upgrade, downgrade, an injected failure | Hosted, probe PR |
   | C8 | `release.yml`'s six assets, the source archive, the MS-RL notices (#130), version checks, the import and message-table check on the published binary; #149 before it | Linux checks, dry run |
   | C9 | Purge, with install_ac_012's decision | Hosted, probe PR |
   | C10 | `install.md`, `upgrade.md`, the migration's runbook | markdownlint |

3. Item 6's residuals for the steps that reach them: steps 1 to 3 renew no
   `START_PENDING` cap; a reparse `<P>` is 13 ahead of 6.3's 2. S58's list
   of what no experiment has settled yet stands.
4. Keep the release freeze.

## Environment and memory

- The system killed a background gate for low memory with the k8s stack up
  (4.4 GB free of 15.8); with it stopped, 4.7 GB free, the matrix passed.
- `C:` had 28 GB free after removing S58's worktree.
- `node` on this host resolves `/tmp` as `C:\tmp`, unlike Git Bash: files
  for both go under `/c/tmp`.
