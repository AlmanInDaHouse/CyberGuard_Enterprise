# Handoff — Session 67

Session 67, 2026-11-12, did C6 of F2's plan
(`docs/handoff-session-59.md#how-session-60-resumes`, restated by
`docs/handoff-session-66.md#how-session-67-resumes`): the agent's MSI, built
with WiX v7, and `package-ci`'s tests that install it. Before the code, an
audit of SPEC-024 found three points no amendment settled; SPEC-024's
Amendment 2026-11-12 settles them, with Manuel's approval.

## Start and resulting state

- Start:
  - `main` = `origin/main` = `e8cdf7d`, clean, with no stash, no branch but
    `main` and no PR open;
  - the tip's CI was green (markdown-lint; the rest skipped by path);
  - 22 ADRs / 23 SPECs, 55 open debts (55 rows) and no Known CI debt. The
    repo agreed with Session 66's handoff.
- Before the first gate, 5.2 GB of 15.8 GB of memory was free and `C:` had
  42 GB free (`target\` held 12 GB). Docker Desktop was down; nothing this
  session needed it, and no container was touched. The gates ran with
  `-j 2`, `CARGO_INCREMENTAL=0` and the profiles' debug info off.
- Resulting `main`, in order:
  - `8431aad`, SPEC-024 Amendment 2026-11-12 and CLAUDE.md's venue for the
    MSI's test (docs, straight to `main` after Manuel's approval);
  - `b96918c`, the MSI and its tests (pull request #122, its commits
    squashed, landed as a fast-forward with the SHA its CI ran on);
  - this close's commit.

  Pull request #122 is closed and its branch deleted, local and on origin.
  The probe pull request #121 is closed without landing and its branch
  deleted. The owner-review branch `review/c6-msi-contract` is deleted. No
  other branch is left, and no worktree.
- Catalogs: 22 ADRs / 23 SPECs, unchanged in number; the SPEC catalog gains
  the Amendment 2026-11-12's line.

## What landed

### SPEC-024 Amendment 2026-11-12 (`8431aad`)

- Point 1, the launch conditions: the setup's chain passes `CG_SETUP_CHAIN=1`;
  the MSI admits an installation only with it or on an installed product, and
  only per machine (`ALLUSERS` 1, `MSIINSTALLPERUSER` unset). It guards
  against a mistaken direct installation, not an elevated administrator.
- Point 2, the service's explicit DACL: Windows' default without
  `SERVICE_USER_DEFINED_CONTROL` for interactive and service users, its SDDL
  stated.
- Point 3, the binary's directories inherit their parent's security; an
  existing `<ProgramFiles>\CyberGuard` is within the administrator's
  boundary (the binary's policy, install_ac_001, install_ac_011 and
  §Operational §1's destination checks narrowed by scope).
- Point 4, the uninstallation's stop past Windows Installer's 30 seconds:
  open until C7 reads it (#156).
- Point 5, the publisher's name `CyberGuard`, provisional until ADR-0022's
  Open question 1.
- *Tests*: the MSI's test, a Cargo target of `cg-setup`, runs in
  `package-ci` and may create a local account (the Amendment 2026-11-10's
  *Tests* narrowed by scope). CLAUDE.md's *Windows package* and *Windows CI
  job* bullets say so.

### The MSI and its tests (`b96918c`, PR #122)

- `deploy/windows/Directory.Build.props`, for `.wixproj` projects alone: the
  accepted EULA (`wix7`), a locked restore against a committed
  `packages.lock.json`, x64, warnings as errors, and `ProductVersion` from
  `CG_RELEASE_VERSION`.
- `deploy/windows/Directory.Build.targets`: before WiX compiles, the version
  must be set, digits and dots only, equal to `ProductVersion`, and what
  `.github/scripts/release-version.mjs` prints for it; `AgentExe` must exist.
- `deploy/windows/msi/`: `CyberGuardAgent.wixproj`, its lock file and
  `Package.wxs`. The MSI takes no extension and no custom action:
  - per machine, `UpgradeCode` `AF9D0A74-…`, `MajorUpgrade` after
    `InstallInitialize`;
  - `<ProgramFiles>\CyberGuard\Agent\cg-agent.exe`, private directory ids;
  - the product service with point 2's DACL (`MsiLockPermissionsEx`);
  - its stop and removal on uninstall alone, and no start;
  - the Application log source as plain registry values;
  - the two launch conditions.
- `deploy/windows/test/msi-version-matrix.mjs`: 14 builds, 13 refused for
  their own reason and one at MSI's limits built.
- `deploy/windows/test/projects.test.mjs`: the projects' properties, lock
  files and pins.
- `agent/cg-setup/tests/package_msi_host.rs` (`host-tests`), four tests:
  - a direct installation refused;
  - a per-user installation refused (`ALLUSERS=2 MSIINSTALLPERUSER=1`, and an
    empty `ALLUSERS`);
  - a direct major upgrade refused;
  - the installation as the setup does it, then its uninstallation: the
    service's configuration, display name and exact DACL, read also by
    `cg-setup`'s check; a standard local user and a filtered administrator
    denied every control right, the registry keys' changes and writes to the
    binary and its directories, each allowed to query the service; the raw
    executable carried byte for byte at this version; the owners; the
    message table through the registered source; nothing under
    `%ProgramData%\CyberGuard`; the removals, which leave a file there.
- `package-ci.yml`: an `msi` job (the agent at 1.2.3, the pins, the matrix,
  the MSI at 1.2.3 and 1.2.4, the test, msiexec's logs as an artifact);
  `release-version.mjs` among its paths.
- CLAUDE.md's facts (the job, the target, the local gate's MSI build and
  matrix), and the threat model's as-built line on the MSI, both approved by
  Manuel with the amendment and landed with the code.

## Red evidence (CLAUDE.md compensating control 1)

- Local, the version check: with `Directory.Build.targets` removed, the
  matrix gave "msi version matrix: 10 failed" (`1.2`, `1.2.3.4`, `v1.2.3`
  and `01.2.3` built with exit 0). The `ProductVersion` override, added
  after the review, gave "FAIL a ProductVersion set otherwise ("1.2.3")
  refused: exit 0" against the earlier target.
- Local, `projects.test.mjs`: red without `RestoreLockedMode`, and red with
  the lock file untracked.
- Probe pull request #121 (`d8a246e`, closed without landing), one leg per
  mutant of `Package.wxs`, the test step unable to fail the job:
  - `none`: "4 passed";
  - `no-chain`: "a direct installation fails", `left: 0`, and the same for
    the direct major upgrade;
  - `no-per-machine`: "allusers-2: a per-user installation fails",
    `left: 0`;
  - `default-dacl`: "standard user …: SERVICE_USER_DEFINED_CONTROL on the
    service was granted", and for the filtered administrator;
  - `no-event-source`: "KEY_READ on …\EventLog\Application\CyberGuardAgent
    was refused: Win32 error 2";
  - `creates-data`: "the MSI creates nothing under C:\ProgramData\CyberGuard";
  - `unquoted-config`: the command line, and `cg-setup`'s check "its command
    line is not the product's" (26);
  - `demand-start`: start type `3`;
  - `loose-directory`: `FILE_WRITE_DATA`, `FILE_APPEND_DATA`,
    `FILE_WRITE_ATTRIBUTES`, `FILE_WRITE_EA` and `DELETE` on the binary,
    and `FILE_ADD_FILE`, `FILE_ADD_SUBDIRECTORY` and `DELETE` on its
    directory, granted to both tokens;
  - `service-kept`: the service left after the uninstallation, seen by the
    next tests' guard ("not established absent") and "PROBE service present:
    True";
  - `guard`: with `RUNNER_ENVIRONMENT` set to `self-hosted` in the step's
    script, "refusing to install the MSI outside a GitHub-hosted runner",
    then nothing present.
- The msiexec logs of the control run (`3f18bff`'s artifact) show each
  refusal with 1603 and its condition's message, each installation and
  removal with 0, and an empty `ALLUSERS` deleted ("Deleting ALLUSERS
  property").

## Decisions

### Anticipated (Claude Code)

- **The service's DACL explicit**, Windows' default without user-defined
  control. Discarded: Windows' default, which the build may change and which
  lets interactive users send user-defined controls.
- **The Event Log source as plain registry values.** Discarded:
  `util:EventSource`, an extension the MSI would need for two values.
- **The version check reuses `release-version.mjs`.** Discarded: a second
  copy of the rule in MSBuild.
- **The MSI's test a `cg-setup` target behind `host-tests`**, which uses
  `cg-setup`'s own check of the service and of the installed version.
  Discarded: a Node or PowerShell test, which cannot build the tokens.
- **One probe pull request with a matrix of mutants.** Discarded: one probe
  per mutant.

### Reactive corrections

- **The audit of the plan** (four lenses: contract, owner stops, WiX,
  Windows Installer): the props scoped to `.wixproj` (an unscoped
  `RestoreLockedMode` wrote a lock file for `Pins.csproj`); the per-machine
  launch condition (`ALLUSERS` moves `ProgramFiles64Folder` into the user's
  profile); an interactive logon only, with `INTERACTIVE` and the filtered
  token's groups asserted; owners; the registry keys' access; 1618 waited
  for; the logs kept. Three owner points went to the amendment.
- **Three fixed-prompt passes on the amendment:** facts ahead of code
  (split into a rule commit and a facts commit landed with the code); item
  4's other sentences; the narrowing by scope of the binary's policy and
  §Operational §1; the interrogate right in the *Tests*; the venue's
  narrowing named. The third pass's fixes were not reviewed again; Manuel
  was told.
- **The review of `88f5bee`:** the direct-upgrade test read the service
  with placeholder paths and could only fail; `ALLUSERS=""` reached msiexec
  escaped (`raw_arg` now); every `/i` is cleaned up; texts that restated
  item 4's old words.
- **`f8c9f1f` red in `package-ci`** (run 43255993386):
  `QueryServiceObjectSecurity: Win32 error 5`, the handle had no
  `READ_CONTROL`. Fixed in `3f18bff`, with the display name asserted and
  the MSI's stop on uninstall alone (`Stop="both"` let a direct repair stop
  the service, beyond point 4).
- **The final pass:** a `-p:ProductVersion=` override bypassed the check,
  and `CG_RELEASE_VERSION` reached cmd unfiltered; both refused now.

### Manuel's decisions

- 2026-11-12, on SPEC-024 Amendment 2026-11-12, CLAUDE.md's venue for the
  MSI's test, the threat model's as-built line and CLAUDE.md's facts:
  "Apruebo tal cual".

## Self-review (CLAUDE.md compensating control 5)

Contract: SPEC-024 §Data contracts §1 to §3, install_ac_001, the Amendment
2026-11-09 (item 4, *Tests*) and the Amendment 2026-11-12.

| Criterion | Test | Verdict |
|---|---|---|
| A version outside the release's form or MSI's limits refused | `msi-version-matrix.mjs` | covered |
| The agent's own version equals the package's | the install test (`configure_version`) at the fixture | weak: the build does not compare them; install_ac_015, C8 |
| Per machine, one `UpgradeCode` | the install test, the per-user test | covered |
| A higher version replaces, a lower is refused; early removal | none | missing: C7 (the bundle's upgrade and downgrade) |
| The service: name, display name, own process, automatic, LocalSystem, the command line | the install test | covered |
| Only SYSTEM and Administrators control it; ordinary users query | the install test (effect on two tokens, exact DACL) | covered; the filtered token is synthetic, a real UAC token stays with the isolated machine |
| The binary read and run, not changed, by ordinary users | the install test | covered on a host where the MSI creates the directories (point 3) |
| Nothing under `<P>` | the install test, `assert_nothing_installed` | covered |
| The Event Log source and its message file | the install test | covered |
| Direct installation and direct major upgrade refused | their tests | covered |
| Per-user installation refused | its test | covered |
| Point 4, the stop on uninstall | none | missing: open, #156 |
| Uninstall removes and leaves `<P>` | the install test | covered; `service-kept` seen red through the guard only |
| The guard | probe `guard` | covered |

Dispositions that change no code:

- **Repair, extraction, same-version rebuilds (#157), the upgrade's
  rollback and the bundle's upgrade and downgrade** are C7's.
- **The publisher's name** is provisional (point 5).
- **The default SACL:** whether `MsiLockPermissionsEx` keeps the service's
  failure-audit SACL is not read; the test reads the DACL. C7 may log it.
- **The per-machine condition refuses `ALLUSERS=1 MSIINSTALLPERUSER=1`**,
  which would install per machine: harmless strictness.

## Gates and CI

- Local Rust gate, main checkout, on the C6 tree: `FMT_OK`; clippy clean;
  95 binaries, "passed 541 failed 0 ignored 11"; "agent version matrix: all
  steps passed".
- Windows package, local: `node --test "deploy/windows/test/*.test.mjs"`
  "pass 22"; "pins verified: 4 packages at 7.0.1" (packages and tool); "msi
  version matrix: all cases passed"; the MSI built at 1.2.3 with an agent
  built at 1.2.3 ("cg-agent 1.2.3"), "0 Advertencia(s)", "0 Errores";
  `git status` clean after.
- `cargo audit` (a Cargo manifest changed; `Cargo.lock` did not): "3
  allowed warnings found".
- The elevated gate does not apply: the diff touches none of its paths (the
  one edit under `agent/cg-agent/` is a comment in a test).
- CI:
  - `8431aad` on `main`: markdown-lint `success`, the rest skipped, ALL
    GREEN;
  - PR #122: `88f5bee` and `f8c9f1f` `package-ci` `failure` (the two test
    defects above), `3f18bff`, `be3eba7` and `b96918c` every workflow
    `success`; `b96918c`'s `msi` job "4 passed" and "msi version matrix: all
    cases passed";
  - `b96918c` on `main` (push): package-ci, ts-ci, markdown-lint,
    dependency-audit and rust-ci `success`, deploy-ci not triggered, ALL
    GREEN;
  - this close's commit, as its post-push report gives it.

## Test baselines at close

- Non-elevated `cargo test --all`: 541 passed, 0 failed, 11 ignored, in 95
  binaries.
- `package-ci`: the `msi` job's 4 tests and its matrix of 14 builds; the
  `host` job's 1; `deploy/windows/test` 22.
- The SCM suites in CI: unchanged.
- Elevated `--ignored` and vitest: not run this session, unchanged at S58's.

## Debts

- Closed: none.
- Destination changed: **#87**, narrowed: the MSI's half (install_ac_001's
  MSI half) landed (`b96918c`). Open for the bundle (install_ac_011/015, the
  route that starts the service with its arguments end to end) and the
  migration's runbook.
- Opened:
  - **#156**, an uninstallation while the product service takes longer than
    Windows Installer's 30 seconds to stop (SPEC-024 Amendment 2026-11-12,
    point 4): read on a hosted runner with the bundle in C7, then settled
    before the bundle lands; a `cg-setup` stop step would amend item 1 and
    is Manuel's.
  - **#157**, two builds of the same version get two ProductCodes, and
    `MajorUpgrade` neither upgrades nor refuses the second: decide
    `AllowSameVersionUpgrades` or one ProductCode per version in C7, with
    its repair tests.
- Open at close: **57**.

## Waiting on Manuel

Nothing blocks. #156 comes back to him with C7's evidence. The release-tag
freeze stays.

## How Session 68 resumes

1. Run the start protocol (CLAUDE.md). `main` is this handoff's close
   commit. There is no PR. The counts are 57 open debts and 22 ADRs / 23
   SPECs.
2. **C7, the bundle**, as `docs/handoff-session-59.md#how-session-60-resumes`
   orders it: Burn's chain (`cg-setup` preflight, the MSI with
   `MsiProperty CG_SETUP_CHAIN=1`, the rollback boundary, configure), a token
   canary, upgrade and downgrade, an injected failure; repair and extraction;
   #156's reading and its decision with Manuel; #157; the wizard. The pinned
   bundle extensions go on the bundle's `.wixproj` with its lock file
   (`projects.test.mjs` checks them against `pins.json`).
3. Then C8, C9 and C10 in that order; C8 carries the agent's version against
   the package's (install_ac_015).
4. Keep the release freeze.

## Environment and memory

- `target\release\cg-agent.exe` was last built with `CG_RELEASE_VERSION=1.2.3`
  for the local MSI build; the elevated gate's `cargo build --release`
  rebuilds it without the input.
- Command lines with `${{ }}`, `$` or backslashes broke in `node -e` and
  heredocs again this session; the Edit and Write tools did not.
