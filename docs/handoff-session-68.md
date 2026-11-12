# Handoff — Session 68

Session 68, 2026-11-12, opened C7 of F2's plan
(`docs/handoff-session-59.md#how-session-60-resumes`, restated by
`docs/handoff-session-67.md#how-session-68-resumes`), the setup bundle, with
its contract. An audit of SPEC-024 against WiX's Burn `v7.0.1` source found
points no amendment settled. A probe pull request read them on a hosted runner.
Manuel chose each in its recommended option, and SPEC-024's Amendment
2026-11-12 (second) and the threat model's Amendment 2026-11-12 record them,
with his approval. No code landed. The bundle's implementation is Session 69's,
from the probe's code.

## Start and resulting state

- Start:
  - `main` = `origin/main` = `cc4e114`, clean, with no stash, no branch but
    `main` and no PR open;
  - the tip's CI was green (markdown-lint; the rest skipped by path);
  - 22 ADRs / 23 SPECs, 57 open debts (57 rows) and no Known CI debt. The
    repo agreed with Session 67's handoff.
- Before the probe's local builds, 5.6 GB of 15.8 GB of memory was free and
  `C:` had 39.6 GB free. Docker Desktop was down; nothing this session needed
  it, and no container was touched. Builds ran with `-j 2`,
  `CARGO_INCREMENTAL=0` and the profiles' debug info off.
- Resulting `main`, in order:
  - `fe05912`, SPEC-024 Amendment 2026-11-12 (second), the threat model's
    Amendment 2026-11-12 and the catalog's line (docs, straight to `main`
    after Manuel's approval, from the owner-review branch
    `review/c7-bundle-contract`, its five commits squashed with the marker
    replaced by the approval);
  - this close's commit.

  The probe pull request #123 is closed without landing and its branch
  deleted; `refs/pull/123/head` = `766d4f8` keeps its code. The owner-review
  branch is deleted, local and on origin. No other branch is left, and no
  worktree.
- Catalogs: 22 ADRs / 23 SPECs, unchanged in number; the SPEC catalog gains
  the Amendment 2026-11-12 (second)'s line.

## The audit

A workflow of seven agents read SPEC-024 and Burn's source in five lenses
(contract, Burn, owner stops, threat model, plumbing), then a synthesis and a
completeness critic. Its findings that changed the plan:

- when Windows Installer asks for a restart, the standard bootstrapper
  application replaces `setup.exe`'s code by 3010 or 3017;
- Burn runs `reverse` after preflight's own failure, and runs the installed
  older setup again (the restore) when an upgrade fails;
- 1638 is Burn's related-bundle check, not Windows Installer's;
- Burn starts an `ExePackage` with no standard handles, so cg-setup's line is
  lost;
- a cancellation abandons a running cg-setup;
- the setup started without elevation copies its engine into the user's
  temporary folder before the prompt;
- #156's "a configure with `UninstallArguments=stop`" cannot work: a
  permanent package detected by no condition is never uninstalled.

The published Burn advisories GHSA-7wh2-wxc7-9ph5 and GHSA-rf39-3f98-xr7r are
fixed in WiX 4.0.4 and 4.0.5, before the pinned 7.0.1 (read on GitHub).

## The probe (PR #123, closed without landing)

`766d4f8`, run 43299813902, job "The setup on the host (probe readings)" and
its `burn-logs` artifact. The job built the agent, `cg-setup`, the MSI and the
setup at 1.2.4, a 1.2.4 whose MSI fails inside its transaction (a test-only
deferred action, `-p:CgInjectFailure=1`), a second 1.2.3 and 1.2.3. It then ran
`agent/cg-setup/tests/package_bundle_host.rs` (`host-tests`) against cg-agent's
test servers (`TlsEnrollMock`, and `TlsMockServer::start_for_enrolled`, new,
which verifies under the key the enroll mock last certified). The readings:

| Reading | Result |
|---|---|
| Fresh installation | exit 0 in 3.2 s; one enrollment; `RUNNING`; point 2's DACL; failure actions 60/120/300/86400; one installed-programs entry, the MSI hidden |
| Token canary | found in no log, temporary folder, package cache, Windows Installer cache, registry key or event log scanned |
| Refusals | 22, 25, 40 (a quote injected in `CG_BOOTSTRAP`) pass through; `reverse` runs after each; nothing registered or cached; cg-setup's line in no log; `WixBundleUILevel=4` ignored |
| Upgrade, downgrade | 1.2.3→1.2.4 exit 0 with no new enrollment; 1.2.3 over 1.2.4 exit 1638, no package run, same PID |
| Injected MSI failure, preflight's 30 on an upgrade | 1603 and 30; the 1.2.3 setup restored and its agent `RUNNING`, DACL and failure actions intact |
| configure's 44 on an upgrade | the restore's 1.2.3 preflight exits 29; both setups registered; the 1.2.4 agent runs |
| Second build of 1.2.3 | Burn upgrades the setup; one setup and one MSI left; the earlier MSI removed after the later committed |
| Repair (`-repair`) | changed binary, start type and failure actions restored; no new enrollment |
| Uninstall, final request stalled | with a 55 s request timeout, Windows Installer's stop took 55.6 s (its log), exit 0; with the default 10 s, 10.6 s |
| Run as SYSTEM | log in `C:\Windows\SystemTemp` (SYSTEM and Administrators only); `Package Cache` gives Users and Everyone RX |
| Restart pending | not read: Restart Manager closed the probe's own process, which held the binary |

## What landed

### SPEC-024 Amendment 2026-11-12 (second) and the threat model's (`fe05912`)

- Points 1 to 10: the chain as built, the reversal's reach, the restore,
  the exit codes (3010/3017, 1638, 29), cg-setup's line, a cancellation,
  #156 settled by the reading, the setup's window, #157 by
  `AllowSameVersionUpgrades`, the launch route and cg-setup's
  system-directory loading.
- *Tests*: `package-ci`'s bundle target against cg-agent's test servers,
  their crates as cg-setup's dev-dependencies.
- The threat model: the setup before its elevation, the package cache, the
  canary's reading, a cancelled or failed setup; two accepted threats (the
  interactive route's engine before elevation; an unpublished Burn
  vulnerability).

## Decisions

### Anticipated (Claude Code)

- **An audit workflow before any code.** Discarded: writing the bundle from
  the spike of S58, whose stand-in never ran `reverse`.
- **One reading probe before the contract.** Discarded: deciding from Burn's
  source alone. The probe settled #156 and showed the restore's 29.
- **The readings as a Rust target over cg-agent's test servers.** Discarded:
  a PowerShell script, which could not reach operational (0) and would be
  thrown away; the target is the landing test's start.
- **A deferred action behind a preprocessor define for the injected
  failure.** Discarded: the Util extension's `FailWhenDeferred`, a carried
  custom action the MSI does not otherwise need.

### Reactive corrections

- **The probe's restart reading died:** Restart Manager closed the test
  process that held `cg-agent.exe`. The 3010/3017 rule stands on Burn's
  source, said so in point 4.
- **Three fixed-prompt passes on the texts:**
  - pass 1, two reviewers: a wrong item, a threat-model section that does not
    exist, point 4's "30 seconds" not superseded, the restore's host changes,
    and a cancellation outside the closed list of causes;
  - pass 2: `reverse` claimed after configure (it does not run there), and a
    reversal after a refusal reading a record configure left;
  - pass 3: the restart's "until" without Windows' restart, the
    cancellation's interleavings, a refusal after the record, and 3017 against
    "whatever the restore does";
  - each pass's List A was applied. A fourth pass found no firm List A, only
    one borderline item (a quote in a variable breaks out and is refused with
    40). That fix and its List B fixes (`6f019b3`) were not reviewed again;
    Manuel was told.
- **Heredocs mangled a backslash again;** the edit went through the Edit
  tool.

### Manuel's decisions

On 2026-11-12, in chat, each in the recommended option:

- the setup's window: no licence acceptance, the project's own theme, the
  copyright provisional with the publisher;
- cg-agent's test servers as cg-setup's dev-dependencies. Their crates are
  axum, tokio, tokio-rustls, rustls, ed25519-dalek, base64 and serde_jcs, all
  already in `Cargo.lock`;
- #156 closed by the reading;
- item 7 amended for 3010/3017;
- `AllowSameVersionUpgrades` for #157;
- the restore declared and tested;
- the unattended route started elevated or as SYSTEM, with the disclosures.

He then approved both texts: "apruebo".

## Self-review (CLAUDE.md compensating control 5)

Claude Code's own pass: each claim marked "read" was checked against the
probe's job log and artifact (the 55.6 s and 10.6 s from the MSI logs'
`ServiceControl` timestamps), and each claim from Burn's source is marked as
such. The fresh-context passes and their dispositions are under *Reactive
corrections*. List B items not applied, with their destination:

| Point | Destination |
|---|---|
| A service that stops reporting progress, a request timeout above 55 s | not read; the agent's deadline bounds the stop |
| The restore after 40 to 43, 45 and other refusals | not read, stated so; S69's tests read what a runner can |
| 1602, 1223, the floor's builds, a real UAC prompt | the isolated machine |
| The SPEC-024 catalog row stops at 2026-11-04 | pre-existing; the amendment lines carry the record |
| CLAUDE.md's facts for the bundle's job | with the code (S69) |
| The canary did not cover the MSI's properties or the environment | S69's canary test, by name |

## Gates and CI

- Only Markdown landed. `markdownlint-cli2` on the three files: "Summary: 0
  issues in 0 files".
- The probe's local builds: clippy clean on `cg-setup` and `cg-agent`
  (`--all-targets --all-features`); `node --test` of `deploy/windows/test` "pass 22"; the MSI,
  the failing MSI and the setup built with "0 Advertencia(s)", "0 Errores".
  The probe's CI: every workflow `success` (its readings step could not fail).
- The elevated gate does not apply: nothing on its paths landed.
- CI on `main`: `fe05912` markdown-lint `success`, ALL GREEN; this close's
  commit, as its post-push report gives it.

## Test baselines at close

Unchanged from Session 67: non-elevated `cargo test --all` 541 passed, 0
failed, 11 ignored in 95 binaries; `package-ci`'s `msi` 4 and matrix 14, `host`
1, `deploy/windows/test` 22.

## Debts

- Closed: none.
- Destination changed:
  - **#156**: settled by the reading (SPEC-024 Amendment 2026-11-12 (second),
    point 7); closes with the bundle's code and its test (S69).
  - **#157**: settled for `AllowSameVersionUpgrades` (point 9); closes with
    the bundle's code and its test (S69).
- Opened: none.
- Open at close: **57**.

## Waiting on Manuel

Nothing blocks. The release-tag freeze stays.

## How Session 69 resumes

1. Run the start protocol (CLAUDE.md). `main` is this handoff's close
   commit. There is no PR. The counts are 57 open debts and 22 ADRs / 23
   SPECs.
2. **C7's code**, against SPEC-024 Amendment 2026-11-12 (second), from
   `git fetch origin pull/123/head` (`766d4f8`):
   - `deploy/windows/bundle/` (`CyberGuardSetup.wixproj`, `Bundle.wxs`, its
     lock file) with the project's theme (point 8: no licence, no image, no
     cancellation during progress, a failure text naming the code and log);
   - `Directory.Build.targets`' bundle checks (`Msi`, `SetupExe`, their
     versions) and a bundle version matrix;
   - `projects.test.mjs` over the bundle's attributes (point 1);
   - `Package.wxs`: `AllowSameVersionUpgrades` with ICE61 suppressed and
     its reason, and the `CgInjectFailure` test define, with a job check that
     the release MSI has no custom action;
   - cg-setup's build: `/DEPENDENTLOADFLAG:0x800` and a test reading it
     (point 10);
   - `package_bundle_host.rs` turned from readings into the *Tests'*
     assertions, the MSI test's helpers shared, then seen red on a probe pull
     request with mutants (the injected failure off, no `CG_SETUP_CHAIN`,
     configure not permanent, the guard);
   - the `bundle` job in `package-ci`;
   - CLAUDE.md's facts;
   - the fresh-context reviews, and #156 and #157 closed with the code.
3. Then the wizard (item 1, the isolated machine for its window), C8, C9 and
   C10 in that order; C10's runbook carries point 4's code table and point
   10's launch route.
4. Keep the release freeze.

## Environment and memory

- `target\release\` holds `cg-agent.exe` and `cg-setup.exe` built with
  `CG_RELEASE_VERSION=1.2.3`; the elevated gate's `cargo build --release`
  rebuilds the agent without the input.
- Heredocs through the Bash tool dropped a backslash again (`%SystemRoot%\SystemTemp`);
  the Edit and Write tools did not.
