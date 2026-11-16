# Handoff — Session 75

Session 75, 2026-11-15 to 2026-11-16, finished C9, the purge, under SPEC-024 Amendment 2026-11-15:
the review residuals of `cg-setup purge` (`6311f58`), then the bundle's `Purge` step with
`package-ci`'s tests (`ad8050d`). Probe #138 found that the step as point 2 gave it, detected
present by a condition always true, left a refused installation's setup registered; probe #139
read the step detected by `WixBundleInstalled`, which fixes it but makes an installation's rollback
run the step's uninstallation; SPEC-024 Amendment 2026-11-16 (`205c5f4`, Manuel's choices and
approval) detects the step once the setup is installed and makes `cg-setup purge` honour `yes`,
given an action, only with Burn's uninstallation; probe #140 read that guard, its mutant purging
the data on a failed upgrade's rollback. No tag was pushed: the release freeze stays.

## Start and resulting state

- Start: `main` = `origin/main` = `89a73b7`, clean, no stash, no branch but `main`, no PR; the
  tip's CI ALL GREEN (markdown-lint); 22 ADRs / 23 SPECs, 57 open debts (57 rows), no Known CI
  debt. The repo agreed with Session 74's handoff. 4.9 GB of 15.8 GB of memory free, 29 GB on
  `C:`; Docker Desktop down (Manuel's `aura_*` containers not running, not touched).
- `C:\tmp\s74-probe-b` was empty and no longer held; removed (`rmdir`).
- Manuel's prompt arrived cut and repeated; it held no answer on #160 or #162, which stay waiting.
- Resulting `main`, in order, each pushed alone with its CI green:
  - `6311f58`, the review residuals, fast-forwarded from PR #137;
  - `205c5f4`, SPEC-024 Amendment 2026-11-16, straight to `main` (docs);
  - `ad8050d`, the setup's purge step and its tests, fast-forwarded from PR #141;
  - this close's commit.
- PR #137 (`feat/c9-purge-package` at `6311f58`) shows merged: GitHub closed it when `main`
  reached its head, so the branch's next pushes ran no CI until PR #141 was opened for them.
  PR #141 shows merged; the branch is deleted. Probes #138, #139 and #140 are closed without
  landing; their branches are deleted. No other branch is left.
- Catalogs: 22 ADRs / 23 SPECs, unchanged; `docs/specs/README.md` gains the 2026-11-16 line.

## What landed

### The review residuals (`6311f58`)

The residuals Session 74's review of `cg-setup purge` left for C9's third commit
(`docs/handoff-session-74.md:170-176`):

- `scm::wait_stopped`'s loop is `scm::await_stopped`: a `STOPPED` read at the end of the 300 s,
  the last reading, is read with its codes; only another state at the end is `Ended::Still`
  (point 3.5's "any other state at the 300 seconds").
- `cg-setup.exe purge`, with and without `--confirm=`, checks the platform (20) and the elevation
  (21) first; a value other than empty or `yes` is 40 before the elevation.
- The SCM test of a purge process that dies reports `START_PENDING` for 2 s first, so that
  `StartServiceW` never sees the death (read as a refused start, 46).
- The purge's junction tests (`install_ac_012_purge.rs`) name a junction as a reparse point, and a
  junction in place of a directory targets one holding a file of a name the purge removes there.
- Comments: the 46 of a refused start (`verbs.rs`), `set_start_type`'s doc comment, the headers of
  `cli.rs`, `package_cli.rs`, `package_verbs.rs` and the SCM tests.

### SPEC-024 Amendment 2026-11-16 (`205c5f4`)

The step's detection: present once the setup is installed (`WixBundleInstalled`); an installation
of a setup not installed runs its installation, `cg-setup.exe purge`, which changes nothing; a
repair runs nothing; every uninstallation runs it before the MSI. The setup's action: the step's
uninstallation passes `--action="[WixBundleAction]"`, and `cg-setup purge` honours
`--confirm=yes`, given an action, only with Burn's uninstallation, 4 (read: 4 uninstallation, 6
installation, 8 repair), so that an installation's rollback, which carries `CG_PURGE`, never
purges; with no action, as an administrator runs it, `--confirm=` alone decides.

### The setup's purge step (`ad8050d`)

- `deploy/windows/bundle/Bundle.wxs`: the variable `CG_PURGE`, not persisted; the step `Purge`
  after the rollback boundary and before configure, per machine, vital, detected by
  `WixBundleInstalled`, `InstallArguments="purge"`, `UninstallArguments="purge
  --confirm=&quot;[CG_PURGE]&quot; --action=&quot;[WixBundleAction]&quot;"`; the theme's failure
  text names `Purge`.
- `agent/cg-setup/src/cli.rs`: `--action=` of ASCII digits, at most once; `yes` confirms with 4 or
  with no action.
- The release check: `extractSetup` exported from `check()`; the synthetic manifest carries the
  three `cg-setup.exe` payloads, each refused on its own; a test of the built files reads the three
  under their steps' folders, each the `cg-setup.exe` built.
- `agent/cg-setup/tests/package_bundle_host.rs`, `package-ci`'s `bundle` job, six new tests:
  a default uninstallation keeps `agent.toml`, `server-ca.pem`, `logs\`, the data root and `<P>`
  with their owner, group and DACL, and the reinstallation enrolls nothing, its identity line the
  first agent ID, its heartbeat verified under the enrolled key; `-uninstall CG_PURGE=yes` removes
  the product and its data, a reinstallation then 22; junctions planted as `logs\` and as
  `planted`, each to a canary, left, 47, the step's rollback `purge` exiting 0, then removed and
  the purge run again, 0; `CG_PURGE=no`, 40, the agent the same process; an upgrade with
  `CG_PURGE=yes` whose configure fails, its rollback `--confirm="yes" --action="6"`, 0, the data
  kept; an installation, a second build, a repair and an upgrade with `CG_PURGE=yes` never purging.
  The fresh installation's plan and payload checks and the second build's cache check name the
  purge step. `package_host/mod.rs` gains `descriptor_of`.
- `cg-setup.exe`'s command lines Burn does not form run under a filtered token (the review of
  `6311f58`).
- `deploy/windows/README.md` and `agent/cg-setup/README.md` name the step and `--action=`.

## The probes (closed without landing)

- **#138** (`probe/c9-purge-package`, `de027d3`; run 43774679884): the purge's bundle tests in
  seven legs, the step as the Amendment 2026-11-15 gave it. Leg real: 4 passed, 1 failed, a
  refused reinstallation leaving the setup registered (the defect above). Mutant legs, each red
  for its defect: control (no step) 5 of 5 failed; ignored (no `CG_PURGE` in the step) 5 of 5;
  persisted (`Persisted="yes"`) "CG_PURGE is not persisted" and the cascade; guard (the unconfirmed
  purge goes on) 5 of 5, `agent.toml` removed by a default uninstallation; value (any value
  confirms) the 40 test; junction (the custody's opens follow a reparse point) the canary removed.
- **#139** (`probe/c9-purge-detect`, `86db52b`; run 43776864823): the whole bundle suite with
  `WixBundleInstalled`, 16 passed; the readings the amendment quotes, `WixBundleAction` 4, 6 and 8,
  and the failed upgrade's rollback `purge --confirm=""`.
- **#140** (`probe/c9-purge-action`, `4a6d4f8`; run 43780331019): leg real, the whole suite with
  the guard, 17 passed in 2902 s; leg action (`cg-setup` ignoring `--action=`), the rollback test
  failed, `agent.toml` removed: the rollback with `yes` purged without the guard.

The unelevated tests were seen failing locally first: `package_verbs`'s wait test against the old
reading (`Still(1)`); `package_setup_executable`'s elevation rows against the unconfirmed return
moved before the elevation; `install_ac_012`'s junction tests against the custody's opens without
`FILE_FLAG_OPEN_REPARSE_POINT` (2 failed); its order tests against `agent.toml` removed after the
identity (8 failed); `package_cli`'s action test against `cli.rs` without `--action=`;
`projects.test.mjs` against the chain without the step (3 failed).

## Decisions

### Manuel's

- 2026-11-16, in chat, on probe #138's defect: "WixBundleInstalled (Recomendado)".
- 2026-11-16, in chat, on the installation's rollback: "Acción de Burn (Recomendado)".
- 2026-11-16, in chat, on SPEC-024 Amendment 2026-11-16 at `fa09dc8`: "Doy mi OK y mi sí".
- 2026-11-16, in chat, after the local gate was stopped for memory: "MEMORIA LIBERADA" (read as
  leave to run it again).
- #160 and #162: no answer; both stay waiting.

### Anticipated (Claude Code)

- **Two commits on the branch**: the residuals (Rust only) apart from the package. Discarded: one
  commit, which would have mixed a reviewed library fix with the package's first test run.
- **The release check's count stays a test, not a check**: the setup's three payloads are read by
  a test of the built files; `payloadProblems` still requires one or more. Discarded: a check of
  exactly three, a narrowing of the Amendment 2026-11-14, point 2.
- **Mutants as patches on the probe branch** (`probe/legs/*.patch`, `git apply` in the job), not
  a switch in the product. Discarded: a cfg or variable that turns off a reparse check in a SYSTEM
  service.
- **A junction in place of `logs\`** beside the `planted` one: a junction of an unknown name is
  never opened, so the junction mutant would survive it. The test stops the agent to move `logs\`.

### Reactive corrections

- Probe #138: the step detected present left a refused installation's setup registered; STOP to
  Manuel, then the amendment.
- Probe #139's rollback reading: an installation's rollback would carry `CG_PURGE=yes`; STOP to
  Manuel, then the guard.
- The first fresh pass on the amendment (A1: the refusal rule is the Amendment 2026-11-09, item 3;
  A2: two sentences of the 2026-11-15 missing from *Effect*), the second (the catalog's "only with
  4" false with no action): fixed before the approval.
- The review of `581c336`: `Bundle.wxs`'s header, a test message and `package_cli.rs`'s header
  still described the step before the amendment; fixed in `ad8050d` (amended before landing).
- PR #137 closed by GitHub when `main` reached `6311f58`; PR #141 opened for the package.
- The heredoc backslash defect twice (a test's `purge\cg-setup.exe`, a comment's `logs\`); fixed
  with Edit.
- The local gate was stopped once for memory (`link.exe` 0xc0000142); run again after Manuel freed
  memory.

## Self-review (CLAUDE.md compensating control 5)

### Claude Code's matrix

`6311f58` (contract: the residuals' statements, Amendment 2026-11-15, points 3 and 5):

| Criterion | Test | Verdict |
|---|---|---|
| A `STOPPED` at the 300 s read with its codes | `package_verbs.rs` `package_verbs_purge_wait_reads_a_stop_at_its_end_with_its_codes` | covered (seen red) |
| Platform, then elevation, unconfirmed 0 after them | `package_setup_executable.rs` elevation and floor rows | covered for the order (seen red); the 0 is the bundle job's |
| 40 before the elevation | `package_setup_executable_a_command_line_burn_did_not_form_is_40` | covered |
| A death after the start is 47 | SCM `package_scm_purge_15_another_code_a_death_or_the_end_is_47` (`pending_ms` 2000) | covered, avoids the race rather than testing it |
| Junctions not followed | `install_ac_012_a_junction_as_a_directory…`, `…as_an_entry…` | covered (seen red) |

`ad8050d` (contract: Amendment 2026-11-15, points 1, 2, 7, *Tests*' package-ci part; Amendment
2026-11-16):

| Criterion | Test | Verdict |
|---|---|---|
| The step's attributes, order, variable, theme text | `projects.test.mjs` | covered (seen red) |
| Three payloads, each checked | `release-windows.test.mjs` synthetic and built-files tests; the fresh test's digests | covered |
| `-uninstall CG_PURGE=yes` purges, Burn's log `--confirm="yes"` | `package_bundle_purge_removes_the_product_and_its_data` | covered (red on control, ignored) |
| Reinstallation 22 | same | covered |
| Default uninstallation keeps the data and identity | `package_bundle_purge_a_default_uninstallation_keeps_the_data_and_the_identity` | covered (red on guard); the agent ID part is weak (the mock issues one ID) |
| Junction left, 47, rollback 0, rerun 0 | `package_bundle_purge_leaves_a_planted_junction_then_completes` | covered (red on junction); "the service stopped" weak (the test stops it) |
| Another value 40, nothing changed | `package_bundle_purge_another_value_is_40_and_changes_nothing` | covered (red on value); "nothing changed" is five descriptors and the PID |
| No installation, second build, repair or upgrade purges | `package_bundle_purge_an_installation_upgrade_repair_or_second_build_never_purges` | covered (red on persisted, guard); the repair's plan line not asserted |
| A refused installation leaves nothing registered or cached | `package_bundle_refusals_pass_through_with_the_reversal`, the wizard's | covered (red on probe #138's real leg) |
| An installation's rollback purges nothing | `package_bundle_purge_an_installation_s_rollback_purges_nothing` | covered (red on probe #140's action leg) |
| `--action=` | `package_cli_purge_confirms_only_with_the_uninstallation` | covered (seen red) |

### The fresh reviewers (the fixed prompts, isolated worktrees)

- Code at `6311f58`: no contradiction. Weak: the unconfirmed 0 (the bundle job), the 46 comment;
  list 3: the unfiltered 40 rows could run a verb on an elevated runner, **fixed** (filtered token,
  in `ad8050d`).
- SPEC text at `ae5df2e`: A1 and A2 **fixed**; list B taken but B9 (the threat model's *Context*,
  a record).
- SPEC text at `0da683e`: A1 (the catalog line) **fixed**; B1, B4, B5, B6 and B7 taken.
- Code at `581c336`: no contradiction; `Bundle.wxs`'s header, a test message, `package_cli.rs`'s
  header **fixed**; recorded: the junction test stops the agent itself, "data kept" by existence,
  the 90-minute job (17 tests in 48 min on probe #140), an empty `--action=` failing open (the
  contract's choice; Burn formats the variable).
- Code at `ad8050d`, after the fixes: no contradiction; the same weak items; the README not naming
  the installation's run of the step (fixed in this close) and `package_bundle_host.rs`'s header
  not naming the rollback test (recorded).

## Gates and CI

- Local gate at `6311f58`: `cargo fmt --all -- --check` clean; `cargo clippy --all-targets
  --all-features -- -D warnings` "Finished"; `cargo test --all` 98 binaries, 613 passed, 0 failed,
  11 ignored; `node .github/scripts/agent-version-matrix.mjs` "agent version matrix: all steps
  passed".
- Local gate at `581c336` (the tree of `ad8050d` but three comments and the amendment's docs),
  after Manuel freed memory: fmt clean, clippy "Finished"; `cargo test --all` 98 binaries, 614
  passed, 0 failed, 11 ignored; the version matrix "all steps passed"; `node --test
  "deploy/windows/test/*.test.mjs"` 85 tests, 76 passed, 0 failed, 9 skipped; "pins verified: 4
  packages at 7.0.1" twice; "licences admitted: 431 packages"; the release build, every check
  PASS, "release build: cg-agent-1.2.3-x86_64-pc-windows-msvc.exe, cyberguard-agent-1.2.3-x64.msi,
  cyberguard-agent-1.2.3-x64-setup.exe, wix-7.0.1-source.tar.gz"; `release-windows.test.mjs` on
  the built files 33 passed, 0 failed, 0 skipped; "release check matrix: all cases passed";
  "bundle version matrix: all cases passed"; markdownlint "Summary: 0 issues in 0 files". At
  `ad8050d`: the node tests 76 passed, 0 failed, 9 skipped; `package_cli` 9 passed; "bundle
  version matrix: all cases passed".
- No elevated gate: the change touches no path of *Developer-local SPEC-005 marquee validation*
  §5 (`custody.rs` changed only in a probe's mutant).
- CI on PR #137 at `6311f58`: ts-ci, deploy-ci, rust-ci, package-ci success, ALL GREEN; on
  `main`'s push of `6311f58`: ts-ci, rust-ci, package-ci success, ALL GREEN. `205c5f4` on `main`:
  markdown-lint success, ALL GREEN.
- CI on PR #141 at `ad8050d`: markdown-lint, ts-ci, deploy-ci, rust-ci, package-ci success (the
  `bundle` job "17 passed; 0 failed"), ALL GREEN. On `main`'s push of `ad8050d`: markdown-lint, ts-ci, rust-ci,
  package-ci success, ALL GREEN.
- This close's commit: as its post-push report gives it.

## Test baselines at close

Non-elevated `cargo test --all`: 98 binaries, 614 passed, 11 ignored. `package-ci`'s `bundle`
job: 17 tests. Elevated `--ignored` 7 and vitest 86 files, 504 passed, 2 skipped, unchanged from
Session 72.

## Debts

- Opened: none. Closed: none. Narrowed: none.
- Recorded, not debts: the junction test stops the agent itself (the purge's stop of a running
  service is the default and `yes` tests' and the SCM tests'); the repair's plan line; the default
  uninstallation's agent-ID check (the enrollment count proves no re-enrollment);
  `package_bundle_host.rs`'s header not naming the rollback test.
- Open at close: **57**.

## Waiting on Manuel

- **#160's text** and **#162's order**, as Session 72 put them.
- The release freeze stays; Authenticode stays ADR-0022's Open question 1.

## How Session 76 resumes

1. Run the start protocol (CLAUDE.md). `main` is this handoff's close commit. There is no PR. The
   counts are 57 open debts and 22 ADRs / 23 SPECs.
2. Manuel's answers on #160 and #162, if given.
3. **#163**: read what `GetSecurityInfo` reports of `SE_DACL_PROTECTED` for a directory created
   with `D:(A;OICI;FA;;;<user>)`, and whether a DACL inheritance can still reach passes the
   configuration and identity policies (`custody::check`); a fix to a check is code (branch, PR,
   tests seen red first) and, under the capture path's files, the elevated gate.
4. **C10, the runbooks**, as Session 73's plan lists it (`docs/handoff-session-73.md:200-203`):
   `install.md`'s endpoint steps on the setup, `upgrade.md`, the purge's command
   (`setup.exe -uninstall CG_PURGE=yes -norestart -log <file outside <P>> [-quiet]`) and codes
   (20 to 47, 14 and 15), what point 8 says the purge is not, the migration's runbook (#87's items,
   the console executable), #150's, #160's disclosure, the residuals of Sessions 69 to 75, and the
   endpoint verification of the installed binary's digest. From the Amendment 2026-11-16: an
   installation runs the purge step with no option; `-unsafeuninstall` purges nothing; a manual
   `cg-setup purge --confirm=yes` with no action purges.
5. Keep the release freeze.

## Environment and memory

- Docker Desktop stayed down; nothing of Manuel's was touched.
- `C:\tmp\s75-rel` (the local release build, 1.2.3, not a release), `C:\tmp\s75-logs`,
  `C:\tmp\s75-probe` (the probes' logs) and `C:\tmp\s75-gate*.out` were removed at close.
- `target\` holds debug builds without debug info (`CARGO_PROFILE_*_DEBUG=0`) and 1.2.3 release
  builds.
- Updated memory: the session's close; a fast-forward of `main` to a PR's head closes that PR as
  merged; `reference-burn-wix-facts.md` (probes #138 to #140).
