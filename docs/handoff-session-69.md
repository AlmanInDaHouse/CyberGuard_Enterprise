# Handoff — Session 69

Session 69, 2026-11-12 and 13, wrote C7's code, the setup bundle, against SPEC-024 Amendment
2026-11-12 (second), from probe #123's code. A read of WiX's source found that point 8's failure
page could not name the exit code with the unmodified standard bootstrapper application. Manuel
chose the log and its lines, and approved SPEC-024 Amendment 2026-11-12 (third). The setup, its
build checks, its theme, `package-ci`'s `bundle` job and its host test landed on `main`, the test
seen red first on two probe pull requests. #156 and #157 closed with it. The wizard did not start.

## Start and resulting state

- Start: `main` = `origin/main` = `568b7e4`, clean, no stash, no branch but `main`, no PR; the
  tip's CI ALL GREEN (markdown-lint); 22 ADRs / 23 SPECs, 57 open debts (57 rows), no Known CI
  debt. The repo agreed with Session 68's handoff. 3.8 GB of 16.2 GB of memory free, 34 GB on
  `C:`; Docker Desktop down, no container touched; builds with `-j 2`, `CARGO_INCREMENTAL=0` and
  debug info off.
- Resulting `main`, in order:
  - `99fcd17`, SPEC-024 Amendment 2026-11-12 (third) and its catalog line (docs, after Manuel's
    approval, squashed from the owner-review branch `review/s69-failure-page`, marker removed);
  - `fd2d546`, cg-setup's `DependentLoadFlags` (point 10);
  - `9dc5703`, the MSI test's helpers as `tests/package_host/`;
  - `fb03f68`, the MSI's same-version upgrade (ICE61 suppressed) and its test build
    (`CgInjectFailure`), with the `msi` job's custom-action check;
  - `3adfcd2`, the setup (`deploy/windows/bundle/`), its version checks and matrix, `projects.test.mjs`,
    `package_bundle_host.rs`, the `bundle` job and CLAUDE.md's facts;
  - this close's commit.
- PR #124 shows merged (its head reached `main` by fast-forward) and `feat/c7-bundle` is deleted; the probe pull requests #125 and #126 are closed
  without landing and their branches deleted. No other branch and no worktree is left.
- Catalogs: 22 ADRs / 23 SPECs; the SPEC catalog gains the Amendment 2026-11-12 (third)'s line.

## What landed

### SPEC-024 Amendment 2026-11-12 (third) (`99fcd17`)

The failure page names Burn's log, by its path and a link, and two of its lines: the failed step's
"Applied execute package: (step), result:" and the run's "Apply complete, result:", an HRESULT
`0x8007xxxx` carrying the code in its last four hexadecimal digits; the "Exit code" line, written
once `setup.exe` exits and before any restart, records the code. No control shows the system's
text of the code, though a message box of the interactive route may. A declined elevation exits
1602, not the 1223 the second amendment's point 4 had unread (Burn's source). The window shows no
image; WiX's default logo is carried unshown. *Tests*: the setup's test reads the "Exit code" line
and the failed step's line.

### The code (`fd2d546` to `3adfcd2`)

- **cg-setup** links with `/DEPENDENTLOADFLAG:0x800` on Windows MSVC targets;
  `package_setup_executable` reads the image's load configuration (`fixtures::dependent_load_flags`),
  seen red first locally (`Some(0)`).
- **The MSI**: `MajorUpgrade AllowSameVersionUpgrades="yes"`, `<SuppressIces>ICE61</SuppressIces>`
  alone, with its reason; the `CgInjectFailure` define, empty in a release build, whose test build
  has one deferred action that fails before `InstallFinalize`; `package_msi_host` reads, through
  msi.dll, no custom action in the release MSI and that one in the test build.
- **The setup**: `Bundle.wxs` as point 1 (preflight rolled back by `reverse`, the MSI with
  `CG_SETUP_CHAIN=1`, a vital boundary, configure permanent and detected by no condition, three
  variables not persisted, no condition or search), each step's `CacheId` this build's own
  (`CgBuildId`, a GUID per build; see *Reactive corrections*); the theme (`bundle/theme/`, adapted
  from WiX's HyperlinkTheme under its MS-RL notice): no licence, no image, no control on the
  progress page, a failure page as the third amendment says; `Directory.Build.targets` refuses an
  MSI or a `cg-setup.exe` that is missing or of another version (an inline task reads the MSI's
  ProductVersion through msi.dll, read-only, and cg-setup's version resource);
  `test/bundle-version-matrix.mjs`; `projects.test.mjs` over point 1, the variables, the window and
  the targets.
- **The host test** `agent/cg-setup/tests/package_bundle_host.rs` (`host-tests`, `package-ci`'s
  `bundle` job alone), nine tests against cg-agent's test servers (`TlsMockServer::start_for_enrolled`,
  new; their crates as dev-dependencies at cg-agent's requirements, no package in `Cargo.lock`):
  a fresh installation operational with the token found nowhere but the bootstrap file and the
  configuration, Burn's cached payloads the raw executables with cg-setup's flag, an
  uninstallation and a reinstallation with no new enrollment; refusals 22, 25, 40 and a refused
  Burn variable, with the reversal; an upgrade and a downgrade refused with 1638; the restores after
  1603 and 30, and configure's 44 with no reversal and the older preflight's 29; the second build of
  the version (#157), its cache and a repair of it; a repair; an uninstallation with a final request
  that never answers under a 55-second timeout (#156); a changed payload refused before preflight;
  only the MSI's test build carrying a custom action. Each run reads Burn's last line, the exit code.
- CLAUDE.md: the `bundle` job in *Environment facts*; the bundle's matrix and build in the local
  gate's *Windows package* bullet.

## The probes (closed without landing)

| Probe | Run | Legs and red evidence |
|---|---|---|
| #125, `1e497a2` (on `9987657`) | 43322963617 | injected failure off: the restore test (1603 expected, 0) and the custom-action test; no `CG_SETUP_CHAIN`: every installing test, 1603 at the fresh install; configure not permanent: the fresh test (Configure's plan `rollback: Uninstall`) and the restore test (`Applied rollback package: Configure` after 44); the guard (`RUNNER_ENVIRONMENT=self-hosted` in the step's script): 8 tests refused by the guard without acting, the custom-action test, which acts on nothing, passing |
| #126, `926b744` (on `b426af2`) | 43326686971 | the steps' `CacheId` per version (the pre-fix authoring): the second-build test red at "the later build's preflight stays cached" (0 folders), its `WIX_UPGRADE_DETECTED` and one-MSI assertions passing before it |

Across #125's legs, every test not targeted by a leg's mutant passed against the real code. The
static checks were seen red locally against nine mutants of `Bundle.wxs` and the theme.

## Decisions

### Anticipated (Claude Code)

- **A research workflow of WiX v7.0.1's source and the probe's Burn logs before writing the
  theme and the assertions** (three readers and a critic). Discarded: asserting from memory of
  Burn's behaviour.
- **The assertions on Burn's message ids and the probe's literal lines.** Discarded: the probe's
  substring filters.
- **The payloads' digests read in Burn's package cache.** Discarded: `-layout`, which copies only
  the exe, and `wix burn extract`, the build tool's reading rather than Burn's.
- **The setup's version checks before WiX compiles, by an inline task.** Discarded: the bundle's
  version bound to the MSI's (`!(bind.packageVersion.Agent)`) with an after-build check, which
  leaves an output behind.
- **The MSI change in its own commit (`fb03f68`) and the helpers' move in another (`9dc5703`).** Discarded:
  one commit for C7.

### Reactive corrections

- **Point 8 could not be met** (no variable holds the code): Manuel's choice and the third
  amendment, seven fixed-prompt passes. Their List A items, each applied: the MSI's line carries an
  HRESULT, not the code; the reversal's and the restore's lines; the line prefix; a declined
  elevation is 1602; results outside `0x8007`; the "Exit code" line is written after the window
  closes, and is not last when the computer restarts. The seventh pass's fix was not reviewed again;
  Manuel was told.
- **The theme's `.wxl` would have become a build localization** (the SDK's `**/*.wxl`): excluded.
- **The steps' `CacheId` per version** let a second build's uninstall of the first delete the later
  build's cache (code review pass 1, probe #126): a GUID per build.
- **Heredocs through the Bash tool dropped backslashes again** in `projects.test.mjs`; fixed with
  the Edit tool.
- **The first push of the branch hit GitHub "Internal Server Error" twice;** the third went
  through.
- **Claude Code reported the background script pushing the rebased series stopped for low
  memory;** it had not stopped: it pushed the fourth commit once the third was ALL GREEN, as
  written. The landing on `main` then went by hand.

### Manuel's decisions

- 2026-11-12, in chat: the failure page names the log and its lines ("Log y línea"), over the
  application's text of the code and a library of the project's own.
- 2026-11-13, in chat: SPEC-024 Amendment 2026-11-12 (third) approved ("Apruebo").

## Self-review (CLAUDE.md compensating control 5)

Claude Code's matrix, each *Tests* line of the second amendment and the third's → its test:
fresh/operational and canary → `package_bundle_a_fresh_installation_…`; refusals with the
reversal → `…refusals_pass_through_with_the_reversal`; configure's failure without it and the
restores → `…failed_upgrades_restore_the_installed_setup`; upgrade and 1638 →
`…an_upgrade_and_a_refused_downgrade`; same version → `…a_second_build_…`; repair → `…a_repair_…`;
stalled uninstallation → `…waits_out_a_final_request_…`; uninstall and reinstall, payload
digests, system-directory loading → the fresh test; changed payload → `…a_changed_payload_…`;
custom actions → `…only_the_test_msi_…` and `package_msi_only_the_test_build_…`; the exit-code and
step lines → `Host::setup` and `assert_failed_step`. All covered.

Fresh-context code reviews (fixed prompt): pass 1 on `9987657`, pass 2 on `b426af2`; no criterion
contradicted. Dispositions:

| Finding | Disposition |
|---|---|
| CacheId shared across builds | fixed (GUID per build), probe #126 red against the old authoring |
| Repair, upgrade: steps and `CG_SETUP_CHAIN` not asserted | fixed |
| The restore after 44: host unchanged | the record's absence asserted |
| #157 by proxy | the earlier setup's log now finds its MSI not `Present` |
| The reversal's result in the restores | asserted `0x0` |
| "Exit code" not asserted last | asserted last |
| The changed payload accepts e310 or e312 and asserts no registration | recorded: the contract claims refusal before preflight, asserted; its doc comment corrected |
| The canary's unreadable files only printed | recorded: the identity and files in use; listed in the log |
| The stall's `going_offline` not observed directly | recorded: the 40–120 s bound fails if the timeout is ignored |
| `CgInjectFailure` could be set on a release build | **#158**, C8's release job |
| Theme: FilesInUse page, help text, hardcoded copyright | help text fixed; the rest to the isolated machine and point 5's publisher review |
| Setup builds not reproducible (`CgBuildId`) | recorded: bundle and product codes already differ per build |

## Gates and CI

- Local Rust gate on the bundle tree: `cargo fmt --all -- --check` clean; clippy
  `--all-targets --all-features -D warnings` clean (it compiles the host tests); `cargo test --all`
  "binaries 95 passed 542 failed 0 ignored 11"; "agent version matrix: all steps passed"; `cargo
  audit` exit 0, "3 allowed warnings found" (pre-existing; `Cargo.lock` gains no package). Later
  edits touched only the host-test target and package files, recompiled with clippy each time.
- Windows package, local: `node --test` "pass 27"; "pins verified: 4 packages at 7.0.1"; "msi
  version matrix: all cases passed" (14); "bundle version matrix: all cases passed" (10); the MSI,
  its test build and the setup built, "0 Advertencia(s)", "0 Errores"; nine static mutants "every
  mutant is caught".
- The elevated gate does not apply: no path of it changed (`agent/cg-agent/tests/common/` is test
  code).
- CI, PR #124: `a30e2ff`, `cee2904`, `aa1fee1`, `3843505` ALL GREEN, `3843505`'s `bundle` job "9
  passed; 0 failed" (run 43329395643) and its matrix 10 PASS; after the rebase onto `99fcd17`,
  `fd2d546`, `9dc5703`, `fb03f68` and `3adfcd2` ALL GREEN; the same commits fast-forwarded onto `main`.
- `main`: `99fcd17` markdown-lint `success`, ALL GREEN; `fd2d546` to `3adfcd2`, each pushed alone; this
  close's commit, as its post-push report gives it.

## Test baselines at close

- Non-elevated `cargo test --all`: 542 passed, 0 failed, 11 ignored, in 95 binaries.
- `package-ci`: `msi` 5 tests and its matrix of 14; `bundle` 9 tests and its matrix of 10; `host`
  1; `deploy/windows/test` 27.
- Elevated `--ignored` and vitest: not run, unchanged at S58's.

## Debts

- Closed:
  - **#156**, by `3adfcd2`: the bundle's stalled-uninstallation test (SPEC-024 Amendment 2026-11-12
    (second), point 7);
  - **#157**, by `fb03f68` and `3adfcd2`: `AllowSameVersionUpgrades` and the second-build test (point 9).
- Destination changed: **#87**, narrowed: the bundle's chain, the route that starts the service
  with its arguments end to end, landed (`3adfcd2`); open for the migration's runbook (C10).
- Opened: **#158**, a release build given `-p:CgInjectFailure=1` would carry the test build's
  failing custom action; `package-ci` checks only the MSIs it builds. Destination: C8's release job
  builds the MSI without the define and checks the MSI it publishes has no custom action.
- Open at close: **56**.

## Residuals with their destination

- The window, its failure page, a real elevation prompt (Retry/Cancel, then 1602), Alt+F4 during
  the chain, the files-in-use dialogs, the floor's builds: the isolated machine.
- The runbook (C10): point 4's codes and the third amendment's lines, each code in decimal and
  hexadecimal, 1602 for a declined elevation, the restore's message box.
- `0x80091007` for a changed payload is Burn's source, not read on a runner.

## Waiting on Manuel

Nothing blocks. The release-tag freeze stays.

## How Session 70 resumes

1. Run the start protocol (CLAUDE.md). `main` is this handoff's close commit. There is no PR. The
   counts are 56 open debts and 22 ADRs / 23 SPECs.
2. **The wizard** (SPEC-024 Amendment 2026-11-09, item 1): `cg-setup`'s Win32 dialog with a masked
   field feeding the same validation and staging, after the unattended route; its window from a
   Burn child is the isolated machine's to read, and if it is not visible and in the foreground
   item 1's choice reopens for the wizard alone.
3. Then C8 (the release, with #158 and #149), C9 and C10 (the runbook with the residuals above and
   #87's migration items).
4. Keep the release freeze.

## Environment and memory

- `target\release\` holds `cg-agent.exe` and `cg-setup.exe` built with `CG_RELEASE_VERSION=1.2.3`,
  cg-setup with the new load flag.
- Memory ran short twice (Claude Code stopped background work); Docker stayed down.
- New memory: `reference-burn-wix-facts.md`.
