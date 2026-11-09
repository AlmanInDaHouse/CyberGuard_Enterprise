# Handoff — Session 58

Session 58, 2026-11-09, started the Windows package (install_ac_001/015 and
debt #87's half). It landed three things: the pins of WiX 7.0.1 and the .NET
SDK with their own hash check (`858c777`), the package's contract, SPEC-024
Amendment 2026-11-09 with the threat model's, approved by Manuel (`f31aca3`),
and the agent's static C runtime and Event Log message table (`3f04ca7`), after
a green elevated gate. A spike on a hosted runner, closed without landing,
gave the amendment its evidence. The MSI, the bundle, `cg-setup` and the
service's adoption are not written: this is a checkpoint at a green commit.

## Start and resulting state

- Start: `main` = `origin/main` = `a1cc1fd`, clean, no stash, only `main`, no
  PR open; tip CI green; 22 ADRs / 23 SPECs; open debts 51; Known CI debt
  none. The repo agreed with the handoff.
- Resulting `main`: `858c777` (pins, PR #90), `f31aca3` (the amendment),
  `3f04ca7` (C runtime and message table, PR #92), and this close commit. No
  PR open.
- Catalogs: 22 ADRs / 23 SPECs (SPEC-024 and the threat model amended in
  place).
- Branches and worktrees left: the local branch `worktree-wf_2d2e581a-62e-2`
  and the worktree `.claude/worktrees/wf_2d2e581a-62e-2` hold nothing
  unlanded; the session ended working in that worktree, whose path git
  ignores, so it could not remove itself. S59 removes them, and any
  `.claude/worktrees/agent-*`, after checking that each is clean.

## What landed

### The pins (`858c777`, PR #90)

- `deploy/windows/`: `global.json` (SDK 10.0.403, `rollForward: disable`,
  `WixToolset.Sdk` 7.0.1), `nuget.config` (nuget.org alone, source mapping),
  `.config/dotnet-tools.json` (`wix` 7.0.1), `pins/Pins.csproj`
  (`PackageDownload` of four packages at `[7.0.1]`), `pins/pins.json` (the
  catalog's SHA-512 of each), `pins/verify-pins.mjs`, `pins/audit-pins.mjs`.
- `verify-pins.mjs` hashes each retained `.nupkg` against the catalog value
  before anything loads it, and asks the `wix` tool for its build and its EULA
  ID (`wix7`). It refuses when `DOTNET_CLI_TELEMETRY_OPTOUT` is not `1`.
- `.github/workflows/package-ci.yml` (new); `dependency-audit.yml`'s
  `nuget-advisories` job over `deploy/windows/pins/**`;
  `.github/scripts/check-licences.mjs` reads nuget pins, and
  `licences-allowed.json` names `WixToolset.Sdk` and
  `WixToolset.BootstrapperApplications.wixext` under MS-RL;
  `docs/security/dependency-review.md`, *The Windows package's pins*.
- The lock file (`RestorePackagesWithLockFile`, `RestoreLockedMode`) goes with
  the first `.wixproj`, in C6 below: `Pins.csproj` has no `PackageReference`,
  so it has nothing to lock.

### The contract (`f31aca3`)

SPEC-024 Amendment 2026-11-09, items 1 to 8: `cg-setup` on stock Burn; the
unattended inputs; preflight's states; the MSI; configure; the service's
adoption on `--adopt-staged-config` and `--trust-anchor-sha256=`; the outcome
and the exit codes (0, 20-31, 40-45); the failure actions. The threat model
gains its Amendment 2026-11-09. CLAUDE.md's control §1 and *Local pre-commit
gate* extend the probe exception to the package's host-acting tests in
`package-ci`. No ADR changes, and no privilege or `cg-agent` dependency is
added. Purge is not settled and stays with install_ac_012.

### The executable (`3f04ca7`, PR #92)

- `.cargo/config.toml`: `+crt-static` for `x86_64-pc-windows-msvc`; `rust-ci`
  watches `.cargo/**`.
- `agent/cg-agent/build.rs`: a message table for entries 1000 to 1003, each
  entry its one insertion string, in the `.res` file it already wrote.
- `agent/cg-agent/tests/package_executable.rs`, seen failing first: the imports
  named `VCRUNTIME140.dll` and six `api-ms-win-crt-*`, and entry 1000 had no
  message.
- `docs/security/dependency-review.md`, *What is not covered*: a fix to the
  static runtime arrives only with a rebuild.

## The spike (PR #91, closed without landing)

`refs/pull/91/head` = `57162685a0b481d9b5258f0aa4c60c12d91b782f` keeps the
authored MSI and bundle: fetch it (`git fetch origin pull/91/head`) to recreate
them. It used the agent's executable as a stand-in for `cg-setup`, so
`cg-setup`'s own exit codes are untested. On a hosted `windows-latest` runner:

- The service path is `"C:\Program Files\CyberGuard\Agent\cg-agent.exe"
  --service --config "C:\ProgramData\CyberGuard\agent\agent.toml"`, LocalSystem,
  `AUTO_START`, `STOPPED` after the install; the default service DACL gives
  IU and SU query only; the MSI does not create `<P>`; the event source
  registers.
- A rerun and an upgrade run both `ExePackage`s, an uninstall neither. A
  downgrade exits 1638 with no package run. A preflight failure exits 2 with
  no MSI. A configure failure exits 2 and keeps the MSI; a silent retry runs
  preflight and configure.
- `bal:Condition` exits `0x81f40001` rather than 28, so preflight is the sole
  authority and the condition is not used.
- The MSI does not set the failure actions (`RESET 0`): configure does.
- The extracted payload digests equal the raw executable's.

Authored values to keep stable when the MSI lands: the package `UpgradeCode`
`AF9D0A74-7206-49F9-B391-645F1C1F05AD`, the bundle's
`B3D1FE6C-5F20-4E58-9B7A-3A7C8E1D4F10`, and the component `Guid`
`625894A2-2E34-41A3-B15D-407A6AC45566`. Build notes: the output is at
`msi/bin/Release/`; `dotnet build --no-incremental` is needed when the version
changes; two `ExePackage`s of one executable need distinct `CacheId`s.

## Decisions

### Anticipated (Claude Code)

- **Hash the retained `.nupkg` against the catalog's `packageHash`.**
  Discarded: the lock file's `contentHash` and `.nupkg.sha512`, because
  `dotnet tool` writes the nuspec's hash into the latter for `wix`, so neither
  is the file's own. It lives in `verify-pins.mjs`.
- **`PackageDownload` in a project of its own.** Discarded: `PackageReference`
  in a real project, which would load the package before it is hashed.
- **`WixToolset.UI.wixext` left out of the pins.** Discarded: pinning it now;
  nothing uses it (the wizard comes later).
- **The advisory job reads NuGet's vulnerability index.** Discarded:
  `dotnet list package --vulnerable`, which reports `PackageReference` items
  and `Pins.csproj` has none.
- **A spike on a hosted runner before the contract.** Discarded: writing the
  amendment from WiX's sources alone; the runner settled how the chain behaves
  (*The spike*).
- **The C runtime is linked statically.** Discarded: the setup installing the
  Visual C++ redistributable, one more elevated package and failure path. Its
  cost is in `dependency-review.md`. Disclosed to Manuel before it landed.
- **The message table is written by `build.rs`.** Discarded: `mc.exe` and
  `rc.exe`, external tools the build does not need.
- **Stop after C2.** Discarded: starting the adoption. It is SYSTEM code that
  needs a probe pull request and several review rounds; the migration it
  resembles took all of S57.

### Reactive corrections

- `wix eula --help` exits nonzero: the verifier reads stdout through
  `spawnSync`.
- WiX: `WIX0400` on `bal:Condition`, `WIX8000` on a duplicate `CacheId`, an
  MSI path of `bin/Release`, and `MSB3030` on a second version without
  `--no-incremental`.
- A rebase replaying the old pins commit conflicted: aborted, then
  `git rebase --onto`.
- An earlier statement that Manuel had rejected `SeRestorePrivilege` was
  wrong: `docs/handoff-session-56.md:106-108` records only the design choice,
  and the reason was the panel's and Claude Code's (`:210-212`). Corrected in
  chat before he decided.
- The review of `858c777` made the licence check admit a nuget pin by name
  alone; trusting the field `pins.json` declares let a carried pin claiming MIT
  through.
- The review of `3f04ca7` replaced a denylist of runtime names, which would
  pass another non-OS DLL, with an allowlist, and changed the tests' names and
  a citation (*Self-review*).

### Manuel's decisions in chat

All on 2026-11-09:

- "Sí, arranca el paquete": start the package, including the first use of
  nuget.org he had confirmed in S56.
- "Sí, cg-setup (Recomendado)": the setup's elevated code is a new Rust
  workspace member on stock Burn.
- "D: el servicio adopta (Recomendado)": the service adopts the staged
  configuration, over `SeRestorePrivilege` in the helper.
- "Acepto a-e (Recomendado)": the operator contract.
- "Sí, PR sonda (Recomendado)": the probe for `package-ci`.
- "Sí, apruebo": the amendment's text at `9743d71`, which landed as `f31aca3`
  with the marker stripped and his words added.

## Self-review (CLAUDE.md compensating control 5)

### The pins (`858c777`)

Own pass over the diff, then a fresh-context reviewer with the fixed code
prompt. Findings and dispositions, all fixed before landing:

- The licence check let a carried pin claiming MIT through: admitted by name
  alone. Tests added, seen failing first (4 of 19).
- `consistency()` missed a closing-tag form and `PackageReference`: refused.
- The telemetry opt-out was not enforced: `verify-pins.mjs` refuses.
- Stale texts updated, and a comment in `package-ci.yml` corrected.

### The amendment (`f31aca3`)

Three rounds with the fixed SPEC prompt, the first two with a code lens as
well. Every List A item was applied:

- Preflight's states and their precedence, with a reparse point refused first.
- A directory failing the service's own check refused (27), and `agent.legacy`
  with a data root and no identity.
- The migration route reads no file under `<P>` on its two states.
- The record on every route but a migration's; the retry after a partial
  adoption (final files deleted, the configuration first); the codes of the
  adoption's steps; the repair and upgrade carve-out; the bootstrap file's
  disclosure in the threat model.

There was no fourth round, so the third round's three fixes were not reviewed
again; Manuel was told so. List B, not blocking, with its destination:

| Point | Destination |
|---|---|
| The launch condition stops accidents, not a determined administrator; a direct `msiexec` repair skips preflight and configure | Runbook (C10) |
| Burn's pass-through codes can overlap 23 and 31 | Runbook (C10) |
| Item 6's refusals name codes but no line texts | C3 |
| Repair on a host with neither file is 22 | C5 test |
| `deploy/windows/test` has no feature gate; `package-ci`'s paths must add `agent/cg-setup/**`; `cargo test --all` reaches the new member; a version matrix for `cg-setup` | C4, C6 |
| Restart result 45 against 3010 | C7 |
| Purge has no route | install_ac_012 |

### The executable (`3f04ca7`)

Claude Code's matrix:

| Criterion | Test | Verdict |
|---|---|---|
| No runtime dependency beyond the OS (SPEC-001 NFR-004; SPEC-024 §Data contracts §1) | `agent/cg-agent/tests/package_executable.rs:97`, `:112` | covered on the test build; the release binary by the gate's string check |
| Message table for entries 1000 to 1003 (Amendment 2026-11-09, item 4) | `package_executable.rs:141`, `:165-173` | covered |
| The source's message file is `cg-agent.exe` | none | not this change: the MSI (C6) |

The fresh reviewer built the tip and read both binaries with its own parsers.
It found no contradicted behaviour. Its list and the dispositions:

- *False citation*: `.cargo/config.toml` and the test cited §Operational §1.
  Fixed, to SPEC-001 NFR-004 and §Data contracts §1.
- *Weak*: a denylist of runtime names. Fixed: an allowlist of 13 DLLs, plus the
  API sets other than the C runtime's.
- *Missing*, the names `install_ac_001` and `install_ac_010` claimed those
  criteria. Fixed: renamed `package_*`, and the header says they are
  preconditions.
- *Not run*: `FormatMessageW` with language 0. Added; it passes on this
  Spanish host and on the runner.
- *The comment "as the Event Viewer asks for it"* claimed more than the call
  shows. Reworded.
- *The release asset has no import or table check*: recorded for C8. The
  elevated gate read the release binary by hand.
- *CRT servicing moves to the project*, and *CRT code ships in the binary, which
  the licence check does not read*: recorded in `dependency-review.md`; whether
  the second needs a review of its own is not determined.
- *The flag is wider than the executable*: intended, `cg-setup` needs it.
- *The elevated gate applies because the agent relinks*: run, below.

The fixes changed a test and comments, so no further fresh pass was made.

## Gates and CI

- Local Rust gate on `3f04ca7`: `cargo fmt --check` clean, `clippy -D warnings`
  clean, `cargo test --all` "binaries 80, passed 352, failed 0, ignored 11",
  "agent version matrix: all steps passed".
- Local package gate on `858c777`: `node --test "deploy/windows/test/*.test.mjs"`
  green, restore, `verify-pins.mjs` and `--tool` green.
- Elevated gate `C:\tmp\s58-gate1.ps1`, run by Manuel on
  HEAD `3f04ca7ff9428b536eb6b2e7044e7bf31556e4da`, tree
  `c908e71c49e6e976fc6ab076521551d2875f7985`; `net session exit: 0`; Docker
  29.1.2:
  - `release exe: 2026-11-09T22:47:20.7308602+01:00  commit:
    2026-11-09T22:36:06+01:00`.
  - `VCRUNTIME140 in release exe: False`;
    `api-ms-win-crt- in release exe: False`.
  - `--ignored`: 11 passed, 0 failed.
  - vitest: `Test Files  86 passed (86)`, `Tests  504 passed | 2 skipped
    (506)`, `Duration  471.72s`. No `ac-001-marquee` skip.
  - `detect_ac_001 drained 55 rows in 1 cycles; capture checked at 35.289 s,
    drained at 39.199 s`; `"detect_marquee_elapsed_seconds":39.218`.
  - Probe `...\cg-detect-probe-vy3lx5\winword.exe`, child
    `C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe`; `spec_005
    captured image_file_name — launch: C:\Windows\System32\cmd.exe; terminate:
    cmd.exe`.
  - `logman`: no `CGAgent-KernelProcess` session left.
- CI: `858c777` ALL GREEN on the branch and on `main`. `f31aca3`
  `markdown-lint → success`. `3f04ca7` on the pull request: `dependency-audit`,
  `markdown-lint`, `ts-ci`, `rust-ci`, `deploy-ci`, all `success`; on `main`:
  `dependency-audit`, `markdown-lint`, `ts-ci`, `rust-ci`, all `success`.
- The weekly `dependency-audit` ran on `858c777` on 2026-11-09: `success`.

## Landing

- `858c777`, fast-forward after CI green on the branch.
- `f31aca3`, owner review on `docs/spec-024-package-contract` (compare URL,
  `NOT YET RATIFIED`), then the marker stripped and fast-forwarded; the diff
  against `9743d71` is the ratification words alone.
- `3f04ca7`, fast-forward; the elevated gate ran on that very commit.
- The owner-review branch and both feature branches are deleted; PRs #90, #91
  and #92 are closed.

## Test baselines at close

- Non-elevated `cargo test --all`: 352 passed, 0 failed, 11 ignored.
- The SCM suite in CI: unchanged, 30 tests (9 + 11 + 10).
- Elevated `--ignored`: 11 passed.
- Elevated vitest: 86 files, 504 passed, 2 skipped.
- `deploy/windows/test`: green (25 tests with the licence test).
- Unelevated, the four capture marquees fail with exit 1 on this host
  (CLAUDE.md *Environment facts*); not re-run this session.

## Debts

- Closed: none.
- Destination changed: **#87**, narrowed: the package's contract has landed
  (SPEC-024 Amendment 2026-11-09), and the agent's binary is ready for the MSI.
  Open for the implementation: the adoption, `cg-setup`, the MSI and bundle
  (install_ac_001/011/015) and the migration's runbook.
- Opened: none. The review's residuals are in the plan below or in
  `dependency-review.md`.
- Open at close: **51**.

## Waiting on Manuel

Nothing blocks. The release-tag freeze stays. Later, the isolated machine runs
what no hosted runner can: a real UAC token, the wizard's window, and the 60
seconds against a real ingest.

## How Session 59 resumes

1. Run the start protocol (CLAUDE.md). `main` is this handoff's close commit;
   no PR; 51 open debts; 22 ADRs / 23 SPECs. Remove the worktrees named above.
   Check C: (26 GB free at this close), RAM and Docker before builds.
2. The contract is `SPEC-024` Amendment 2026-11-09. Continue, one commit per
   push, each through a PR, contract tests first:

   | # | Contents | Gates |
   |---|---|---|
   | C3 | `layout.rs`, the read-only classification of the host's state; the service's adoption: `parse_start`, the bounded custody read, item 6, tests over temporary roots, and the SCM target with a probe PR | Rust gate, rust-ci, fresh review after each fix round, elevated gate if a gated path changes |
   | C4 | `agent/cg-setup`: bootstrap, validation, TOML | Rust gate, `cargo audit` |
   | C5 | Staging, the SCM client, the observer, the verbs | Rust gate, hosted tests with a probe PR |
   | C6 | The MSI and `package-ci`'s install tests; lock files on the `.wixproj`s; `agent/cg-setup/**` in its paths | Hosted, probe PR |
   | C7 | The bundle, a token canary, upgrade, downgrade, an injected failure | Hosted, probe PR |
   | C8 | `release.yml`'s six assets, the source archive, the MS-RL notices (#130), version checks, and the import and message-table check on the published binary | Linux checks, dry run |
   | C9 | Purge, with install_ac_012's decision | Hosted, probe PR |
   | C10 | `install.md`, `upgrade.md`, the migration's runbook (item 7's disclosure, `takeown /A`, `icacls`, `-norestart`) | markdownlint |

3. Not settled by experiment yet, to be tested where the plan reaches them:
   creating `O:BA` files unprivileged from a UAC administrator; SYSTEM's
   re-own and rename with its read-back; `MoveFileEx` with a held handle;
   `SERVICE_START` denied to a filtered administrator and a standard user; the
   token canary absent from logs and Burn's state; the failure actions after
   an upgrade and a rollback; an MSI stop past 30 seconds; the message text
   through a registered source; the wizard's window (isolated machine only).
4. Keep the release freeze.

## Environment and memory

- `C:` has 26 GB free, 98% used. `target/` in the main checkout and in the
  worktree are separate; removing the worktree frees the latter.
- The session worked from the worktree after its shell entered it; the elevated
  gate ran in the main checkout, detached at the tested SHA, and restored it.
- The auto-memory note `project-session-58-in-progress.md` is retired at this
  close.
