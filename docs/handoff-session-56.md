# Handoff — Session 56

Session 56, 2026-11-05, ran two tracks. Track A: the packaging report, with
the exact WiX v7 and .NET SDK versions, their commands and channels. Manuel
confirmed the three host changes, and the .NET SDK 10.0.403 is installed;
no `dotnet`, `wix` or nuget restore has run. Track B: install_ac_013, the
legacy migration (the rest of #87). Its contract is SPEC-024's Amendment
2026-11-05, with the threat model's. It went through eight fresh-context
review rounds, was approved by Manuel ("Sí, apruebo") and landed as
`5979f61`. Its implementation has not started. Alongside, debt #146's code
residuals landed as `cd4ab4c` (PR #87), after CI ALL GREEN and a green
elevated gate; #146 stays open, narrowed.

## Start and resulting state

- Start: `main` = `origin/main` = `bd44b5c`, clean, no stash, only `main`,
  no PR open; `main`'s tip CI green (`markdown-lint → success`); 22 ADRs /
  23 SPECs; open debts 52; Known CI debt none. The repo agreed with the
  handoff and the opening prompt.
- Host at start: C: 41 GB free, about 0.4 GB of RAM free, Docker up
  (29.1.2).
- Resulting `main`: `5979f61` (the amendment), `cd4ab4c` (#146's
  residuals), and this close commit. No branch but `main`, no PR open.
- Catalogs: 22 ADRs / 23 SPECs (SPEC-024 amended in place; the threat model
  amended).

## Track A: the packaging report and the .NET SDK

Researched from nuget.org, GitHub, docs.firegiant.com, learn.microsoft.com,
.NET's `releases.json` and `winget-pkgs`, by a two-stage workflow (research,
then an independent check of each fact). Nothing was executed for it.

- **WiX v7: 7.0.1** (tag `v7.0.1`, commit `642f1a0f`, 2026-10-26; a
  security release that drops a vulnerable .NET assembly). `wix`,
  `WixToolset.Sdk`, `WixToolset.Util.wixext`, `WixToolset.UI.wixext` and
  `WixToolset.BootstrapperApplications.wixext` are all at 7.0.1. The
  README of `WixToolset.Bal.wixext` says it was renamed to
  `WixToolset.BootstrapperApplications.wixext`. EULA ID `wix7`, OSMF EULA
  v1.1, accepted in the build by `<AcceptEula>wix7</AcceptEula>`. No GHSA
  or CVE affects 7.x.
- **.NET SDK: 10.0.403** (runtime 10.0.14, LTS, end of support
  2028-11-14). `wix` 7.0.1 targets `net8.0` with `RollForward=Major`;
  .NET 8 and 9 end support on 2026-11-10. WiX publishes no exact SDK
  version, so the pin is the project's choice.
- **Proposed pins** (for the packaging change): `global.json`
  `{"sdk":{"version":"10.0.403","rollForward":"disable"},"msbuild-sdks":{"WixToolset.Sdk":"7.0.1"}}`;
  `.config/dotnet-tools.json` with `wix` 7.0.1 and `dotnet tool restore`;
  the extensions as `PackageReference` 7.0.1 with
  `RestorePackagesWithLockFile` and `RestoreLockedMode`. The lock file
  hashes only `PackageReference` items. Neither `dotnet tool restore` nor
  the MSBuild SDK resolver is documented to check a hash, so the packaging
  change brings its own SHA-512 check for `wix` and `WixToolset.Sdk`
  (SPEC-024 Amendment 2026-11-01, point 1). CI would use
  `actions/setup-dotnet` with `global-json-file`, pinned by SHA.
- **Risk recorded:** Microsoft Update services standalone SDK installs and
  by default removes the previous version. A servicing that replaces
  10.0.403 breaks a `rollForward: disable` pin until the pin is bumped.
- **Manuel's confirmation**, 2026-11-05: "confirmo esas 3 cosas". The three
  are the winget machine-wide SDK install (system `PATH`), the first use of
  nuget.org for WiX, and `setx` of the telemetry variable.
- **Executed:**
  - `setx DOTNET_CLI_TELEMETRY_OPTOUT 1`; the user environment reads `1`.
  - `winget install --id Microsoft.DotNet.SDK.10 --exact --version 10.0.403 --source winget --scope machine --silent --accept-package-agreements --accept-source-agreements`;
    winget printed "El hash del instalador se verificó correctamente" and
    "Instalado correctamente". It waited on the UAC prompt for over ten
    minutes.
  - Verified: `C:\Program Files\dotnet\sdk\10.0.403` exists, the machine
    `PATH` has `C:\Program Files\dotnet\`, and `dotnet --version` prints
    `10.0.403`.
  - Nothing from nuget.org has been restored yet: that is the packaging
    change's first step.

## Track B: SPEC-024 Amendment 2026-11-05, the legacy migration (`5979f61`)

The contract for install_ac_013. The service, as SYSTEM, migrates a
console-layout host during `START_PENDING` when it is started with
`--migrate-legacy=remove|keep` and `--trust-anchor-sha256=…` as SCM start
arguments. It works through these steps:

1. Closes `<P>` and the data root to SYSTEM and Administrators.
2. Checks the console installation: owners SYSTEM or Administrators, one
   link each, no extra entry, the identity consistent and unsealed on this
   host, the anchor matching the fingerprints given.
3. Reads each file once through a handle that excludes writers.
4. Restricts the files in place to SYSTEM and Administrators, so an
   elevated console agent still runs.
5. Records the request in a SYSTEM-owned staging directory.
6. Stages a new data root from the verified bytes.
7. Commits by two renames.
8. Completes or reverses an interrupted migration from which directories
   exist and the reversal it records.
9. Keeps every old copy restricted, the identity and the legacy directory
   to SYSTEM alone, and deletes them with `remove`.

It adds no new code. The threat model's R1 and R17 gain the console host's
wider exposure, the route and the residuals. The installation runbook's
code-13 repair no longer removes a console identity.

- **Design.** A workflow judge panel produced three designs: A, the
  service as SYSTEM asked by start arguments; B, an elevated CLI with
  `SeRestorePrivilege`; C, a CLI that stages and a service that adopts.
  Three lenses scored them: security, contract fit and buildability. A won
  with 7/7/7, against B's 6/5/5 and C's 6/5.5/6. A is the only design that
  enables no privilege (ADR-0010 §Compliance), changes no ADR and adds no
  exit code.
- **Manuel's design choices**, 2026-11-05, in one AskUserQuestion with four
  questions, each answered with the recommended option:
  1. the service as SYSTEM, asked by SCM start arguments;
  2. `remove|keep` required, with no default;
  3. after the commit, completion forward without arguments;
  4. the threat model in the same review branch.
- **Reviews.** CLAUDE.md's fixed SPEC/ADR-text prompt plus a Windows
  security lens, in isolated worktrees, ran eight rounds:
  - **Round 1** (`0c453aa`): staging/legacy names counted on an unclosed
    `<P>`; a data root owned by the enrolling account; a reversal that
    could be blocked; the enrolling account taking ownership of copies; a
    plantable identity; codes and order clashing between items. The fix
    was a rewrite: restriction before the commit, and a record written
    first.
  - **Round 2** (`9a92b4b`): `WRITE_OWNER` on copies; deletion through
    file-ID handles; a crash between a rename and its record; tampering
    after the check; a user-owned unelevated enrollment with no route; a
    planted name changing R17's code. The fix: close the directories
    first, read each file once, derive the state from which directories
    exist, report copies without stopping, and an owner repair.
  - **Round 3** (`6be3a36`): the record's progress marks; arguments
    between the renames; a torn record; the threat model's code; a
    directory handle opened before the closure.
  - **Round 4** (`4c03513`): a staging directory without a record had
    three outcomes; a failed rename back; no `<P>` at all; refused hosts
    had no route; `nodefaultadminowner`; a hard link made between 2.2 and
    2.4.
  - **Round 5** (`99243f2`): my round-4 rewrite of item 4 made the console
    data root "not count", a regression. Also an oplock hang, and a planted
    leftover temporary forcing re-enrollment.
  - **Round 6** (`2044b37`): a hard-linked temporary redirecting a
    privileged security change, an integrity issue that was fixed; oplocks
    on security changes, renames and deletions (bounded at 30 s); the
    `CREATOR OWNER` entry.
  - **Round 7** (`9fce138`): the planted-entry repair was missing from the
    SPEC; the legacy directory held identity files under a
    SYSTEM+Administrators DACL; a hard link made from outside; a
    pre-existing `WRITE_DAC` handle on a directory.
  - **Round 8** (`4feb429`): the fixed-prompt reviewer found **no List A**.
    The security lens found two points: ownership is not authorship
    (disclosed as a residual) and the repair's session condition
    (reworded). Both were applied without a ninth round, which the
    approval question said.
  - Every List A of rounds 1 to 7 was fixed in the next revision. The
    List B points left open are in *Debts*, #147.
- **Who approved it.** It extends the second Amendment 2026-11-01 and the
  Amendment 2026-11-04, which Manuel approved, and touches his list: the
  operator interface, the security posture, the threat model, data about
  people (the enrolling account's SID kept in `migration.json` on the host)
  and the codes' new causes. One AskUserQuestion carried the compare URL,
  those points and the extension of his third choice (a crash between the
  two renames also completes forward). His answer, 2026-11-05: **"Sí,
  apruebo"**. Landed with the marker stripped and the approval recorded in
  the SPEC, the threat model and the catalog.
- **Landing.** The review branch was squashed to one commit (`77ff3b7`;
  the revisions reviewed are the SHAs listed above) and landed by
  `git merge --squash`. Guard: `git diff --cached
  --quiet review/s56-legacy-migration` gave GUARD OK, and `git diff --quiet
  … HEAD` gave TREE EQUAL. The result is **`5979f61`**. CI
  `markdown-lint → success`, ALL GREEN. The branch was deleted, local and
  origin.

## #146's residuals (`cd4ab4c`, PR #87)

- `custody::replace_keeping_security` checks and copies the descriptor of
  one `GetSecurityInfo` read (`check_descriptor`, through
  `GetSecurityDescriptorOwner`/`GetSecurityDescriptorDacl`).
- New tests (`tests/install_ac_004_custody_residuals.rs`,
  `tests/x509_identity.rs`, `tests/install_ac_002_installed_config.rs`):
  - the DER reader over a certificate that the ingest server's own
    `issueClientCert` issued (fixture `custody_fixtures/server_issued_client_cert.txt`
    and its generator `issue_client_cert.mts`, named `.txt` because
    `.gitignore` ignores `*.pem`);
  - a callback ACE (`XA`, type 9) refused. NTFS stores an `OA` ACE without
    GUIDs as a plain `A`, so an `OA` would not exercise the check;
  - an unbuildable SDDL creates nothing, and neither does an owner-SYSTEM
    descriptor, which Windows refuses (1307);
  - a dangling file symlink at a `CREATE_NEW` name is refused (80) without
    creating its target;
  - a leftover hygiene temporary linked to an existing file is deleted
    without touching that file. Under `GITHUB_ACTIONS` this test fails
    rather than skips when the token cannot create a symlink, so rust-ci's
    green shows it ran; locally it skips (1314);
  - a failed move leaves the original and no temporary;
  - non-ASCII case variants (U+0131, U+212A) of a fixed path are refused.
- **Taken out after review:** refusing a `cert.pem` with a malformed later
  PEM block. It is stricter than Amendment 2026-11-04, item 5 ("validates
  nothing else"), so it is a contract change (memory
  *contract-narrowing checks*). It stays in #146 for an amendment.
- **Removed after review:** a `FILE_DELETE_CHILD` unelevated test, which
  exercised the same allow-entry rule as an existing one.

## Decisions

### Anticipated (Claude Code)

- **Track A, pin .NET 10.0.403 with `rollForward: disable`.** Rejected:
  .NET 8.0.x, the `wix` tool's TFM, which ends support on 2026-11-10;
  `latestPatch`, which is not the exact pin SPEC-024 asks for.
- **Track A, `WixToolset.BootstrapperApplications.wixext`, not
  `Bal.wixext`.** Rejected: the renamed package.
- **Track B as an amendment that goes to Manuel.** It extends the
  amendments he approved and touches the operator interface. Rejected:
  treating it as Claude Code's own SPEC amendment.
- **Design A**, by the judge panel above. Rejected: B, because enabling
  `SeRestorePrivilege` breaks ADR-0010 §Compliance; C, because its trigger
  is persistent state on disk and the key is unsealed in the
  administrator's process.
- **#146 in its own PR before the migration's implementation.** Its
  destination named that change, but the migration had not started.
  Rejected: folding it into the migration's PR, which would have held the
  fix back a whole session.
- **The DER reader's fixture issued by the real `issueClientCert`**, run
  through `tsx` from `services/ingest`. Rejected: a fixture built by the
  agent's own test PKI, which would not show the server's encoding.

### Reactive corrections

- The first push of #146 (`5f51150`) lacked the fixture, because
  `.gitignore` ignores `*.pem`; no PR was open, so no CI had run. Fix:
  amended with the fixture renamed to `.txt`, and force-pushed to my own
  branch before the PR was opened (`ec577d4`).
- The `OA` ACE fixture passed the check, and the stored SDDL showed NTFS
  had made it an `A`. Fix: `XA` (type 9), and the test now asserts that
  the fixture holds `(XA;`.
- Code review round 1 found the PEM strictness was a contract change, the
  symlink test unable to fail, the `FILE_DELETE_CHILD` test redundant and
  the injected failure trivial. Fixed in `37a5e0a`.
- Code review round 2 found the symlink test's "discriminating" claim
  wrong (`CREATE_NEW` refuses an existing name either way) and a skip
  invisible in CI. Fixed in `51d587e`.
- Contract review rounds 1 to 8, as above. Round 5 caught my round-4
  regression.
- I launched one review round (`w6y0ki4om`) with the previous SHA, because
  the round-2 script had the SHA built in. I stopped it and relaunched it
  with the right SHA.
- Bash heredocs mangled backslashes four times, in edit scripts, a test
  and a commit message. Each was redone through the Write or Edit tool
  (memory *Bash heredoc backslashes*).
- The amendment's install.md change makes the operations runbook a changed
  input, which the reuse rule for elevated-gate evidence does not allow
  (CLAUDE.md *Elevated gate evidence validity*, condition 1). So the
  amendment landed first, #146 was rebased and squashed onto it, and the
  gate ran on the final tree (`53b8ec3`).

### Manuel's decisions in chat

- 2026-11-05: "confirmo esas 3 cosas" (the SDK by winget machine-wide,
  nuget.org's first use for WiX, the telemetry `setx`).
- 2026-11-05: the four design choices (the recommended options).
- 2026-11-05: "Sí, apruebo" (SPEC-024 Amendment 2026-11-05 and the threat
  model's).

## Self-review and fresh-context reviews of #146

Claude Code's matrix (contract: #146's entry, `docs/handoff-session-55.md#debts`,
under SPEC-024 Amendment 2026-11-04):

| #146 item | Test | Verdict |
|---|---|---|
| Descriptor checked and copied in two reads | none (one read by construction, `custody.rs` `replace_keeping_security`) | weak: not observable; read in review |
| DER reader never run over a server-issued certificate | `x509_identity.rs` `install_ac_005_a_server_issued_certificate_is_read` | covered (a snapshot: nothing regenerates it in CI) |
| An ACE of another type | `install_ac_004_custody_residuals.rs` `install_ac_004_an_ace_of_another_type_is_refused` | covered |
| A file symlink at a `CREATE_NEW` name or as the leftover temporary | `install_ac_005_a_file_symlink_is_never_followed` | covered in CI (fails rather than skips there); skipped locally |
| An injected descriptor failure | `install_ac_005_a_descriptor_that_cannot_be_built_creates_nothing`, `install_ac_005_a_descriptor_windows_refuses_creates_nothing` | covered for creation; not for `GetSecurityInfo` failures |
| A non-ASCII case variant | `install_ac_002_a_fixed_path_differs_only_in_ascii_case` | covered at unit level |
| PEM rejected unlike TLS | none | open: a contract change (#146) |
| Token rewrite reads `agent.toml` by path | none | open (#146) |
| `FILE_DELETE_CHILD` on parents; the hygiene temporary's access | none | open: SCM probes (#146) |

Fresh-context code reviews (CLAUDE.md's fixed prompt, with an FFI lens in
round 1, in isolated worktrees):

- **Round 1 at `ec577d4`:** no blocker; the dispositions are in *Reactive
  corrections*.
- **Round 2 at `37a5e0a`:** the code is correct; the symlink test fixed as
  above; #146 narrowed, not closed.
- Reviewers' worktrees: one, `wf_db9ac4d3-f8b-1`, was left with its branch.
  It was checked clean and removed (`git worktree remove`, `git branch -D`).

## Gates and CI

- **Local gate** on `53b8ec3`'s tree: `cargo fmt --all -- --check` clean;
  clippy `-D warnings` clean; `cargo test --all` **207 passed, 0 failed,
  11 ignored**; `node .github/scripts/agent-version-matrix.mjs` printed
  "agent version matrix: all steps passed". markdownlint gave 0 issues on
  every changed Markdown file.
- **CI, full SHAs** (`ts-ci`, `rust-ci`, `deploy-ci` unless stated):
  - `ec577d4` ALL GREEN (runs 41993041008, 41993041087, 41993041030).
  - `37a5e0a` ALL GREEN (41994350540, 41994350517, 41994350523).
  - `51d587e` ALL GREEN (41995501470, 41995501891, 41995501744).
  - `53b8ec3` ALL GREEN (42018495967, 42018496040, 42018495906).
  - `5979f61` `markdown-lint → success`.
  - `cd4ab4c` (`ts-ci`, `rust-ci`) ALL GREEN.
- **Elevated gate s56-gate1, green.** `C:\tmp\s56-gate1.ps1` was pure
  ASCII and `Parser::ParseFile` reported 0 errors. Manuel ran it. Literal
  lines from the UTF-16 logs:
  - `net session exit: 0`; `docker engine: UP`; `ServerVersion=29.1.2`.
  - `HEAD  = 53b8ec388352fdc5cb6d25d5e84b7638ab2b0fd1`;
    `TREE  = 7b7406c21e77d2931c6afd89a58a062743b7755c`.
  - `release exe: 2026-11-05T12:33:34.5258113+01:00  commit: 2026-11-05T12:22:47+01:00`.
  - `--ignored`: 11 passed, 0 failed: `auth_ac_012_logons_on_the_real_log`,
    `capture_ac_006_elevated_shutdown_leaves_no_session`,
    `net_ac_008_a_real_connection_is_reported_with_its_probe`,
    `net_ac_009_an_accepted_connection_and_the_filter`,
    `ac_004_cache_hit_terminate_matches_launch_byte_for_byte`,
    `ac_007_parent_pid_only_when_parent_dead_at_child_launch`,
    `ac_009_events_lost_under_deliberate_etw_buffer_pressure`, and
    install_ac_009's four (`test result: ok. 4 passed; 0 failed; 0 ignored;
    0 measured; 8 filtered out`).
  - vitest: `Test Files  85 passed (85)`, `Tests  503 passed | 2 skipped
    (505)`, `Duration  462.73s`.
  - `detect_ac_001 captured image_file_name — probe:
    C:\Users\manul\AppData\Local\Temp\cg-detect-probe-aE6HHE\winword.exe;
    child: C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe`;
    `detect_ac_001 drained 56 rows in 1 cycles; capture checked at 35.293 s,
    drained at 39.13 s`; `"detect_marquee_elapsed_seconds":39.142`;
    `spec_005 captured image_file_name — launch: C:\Windows\System32\cmd.exe;
    terminate: cmd.exe`.
  - `logman query CGAgent-KernelProcess -ets`: "No se encontró el Conjunto
    de recopiladores de datos." No session was left.
  - `=== s56-gate1 done 2026-11-05T12:43:22.9869643+01:00 ===`.

## Landing

- PR #87 was one commit, `53b8ec3` (the review commits squashed, its tree
  equal to `51d587e`'s plus the amendment), on `5979f61`. It landed by
  `git merge --squash`: GUARD OK, TREE EQUAL. The result is **`cd4ab4c`**,
  tree `7b7406c`, the tree CI and the gate ran on. CI on `cd4ab4c`:
  `ts-ci`, `rust-ci` → success, ALL GREEN.
- PR #87 was closed with a comment naming the landed SHA, and the branch
  deleted, local and origin.

## Test baselines at close

- Non-elevated `cargo test --all`: 207 passed, 0 failed, 11 ignored.
- The SCM suite in CI: 19 tests (8 + 11), unchanged.
- Elevated `--ignored`: 11 passed.
- Elevated vitest: 85 files, 503 passed, 2 skipped.
- Unelevated, the four capture marquees fail with exit 1 on this host
  (CLAUDE.md *Environment facts*); not re-run this session.

## Debts

- Closed: none.
- Destination changed:
  - **#87:** the legacy migration's contract has landed (`5979f61`). The
    debt stays open for install_ac_013's implementation, the package's half
    (install_ac_001, 011 and 015: the default refusal, the route that starts
    the service with its arguments, and the configuration and anchor owned
    by SYSTEM), and the migration runbook's disclosure (item 7).
  - **#146:** narrowed by `cd4ab4c`. The one-read fix and the tests for the
    untested cases have landed. It stays open for:
    - the PEM strictness, which needs an amendment to Amendment 2026-11-04,
      item 5, since refusing a `cert.pem` TLS would refuse is stricter
      than "validates nothing else";
    - the token rewrite, which reads `agent.toml` by path;
    - the SCM probes of `FILE_DELETE_CHILD` on the parents and of the
      hygiene temporary's access;
    - the server-issued fixture, which nothing in CI regenerates.

    Destination: install_ac_013's implementation, which touches
    `custody.rs`, with the amendment first.
- Opened: **#147**, the List B points the eight contract rounds left open
  for install_ac_013's implementation to settle or record:
  - the `START_PENDING` cap of 120 s (`service/windows.rs:61`) against
    several 30-second cancellations;
  - cancelling a wait in a security change or rename (`CancelSynchronousIo`);
  - a security change that does not propagate (`SetKernelObjectSecurity`
    or `NtSetSecurityObject`, never `SetSecurityInfo`);
  - `migration.json`'s rewrite temporary;
  - a record that exists but does not parse;
  - a legacy object whose file ID is not recorded;
  - the repairs' elevation needs (`icacls /setowner` needs
    SeRestorePrivilege);
  - consoles whose `%ProgramData%` is not `C:\ProgramData`;
  - code 13's and code 5's stretched meanings;
  - test isolation on the runner.

  Destination: install_ac_013's implementation.
- Open at close: **53**.

## Waiting on Manuel

Nothing blocks. The packaging change (install_ac_001/015) can now restore
WiX 7.0.1 from nuget.org, which he confirmed. The migration's
implementation needs no new decision of his: its contract is approved.

## How Session 57 resumes

1. Run the start protocol (CLAUDE.md). `main` is this handoff's close
   commit; no PR, no branch but `main`; 53 open debts; 22 ADRs / 23 SPECs.
   Check C:, RAM and Docker before builds.
2. F2's next items, in either order:
   - **install_ac_013, the migration** (SPEC-024 Amendment 2026-11-05).
     First, a short amendment for #146's PEM strictness (Amendment
     2026-11-04, item 5), Claude Code's if it touches nothing on Manuel's
     list. Then the tests from items 1 to 7 and *Tests*: unelevated over
     temporary roots, and the SCM target `install_ac_013_migration` with a
     probe PR. Then the implementation in `service/`, `custody.rs` and
     `identity.rs`, settling #147's points, a fresh review, CI, and the
     elevated gate.
   - **The package** (install_ac_001/015) with WiX 7.0.1 and the pins of
     *Track A*: `global.json`, the tool manifest, the lock file, the hash
     check, `setup-dotnet` in CI, the licence allowlist under MS-RL, and
     the MSI that leaves `<P>` alone on the migration route.
3. Keep the release freeze.

## Environment and memory

- The .NET SDK 10.0.403 is installed machine-wide and
  `DOTNET_CLI_TELEMETRY_OPTOUT=1` is set in the user environment
  (CLAUDE.md *Environment facts*, updated).
- Heredocs mangled backslashes again; edit scripts were written with the
  Write tool and run with `node C:/tmp/...`.
- Workflow reviewers ran in isolated worktrees; one leftover was removed.
- The session-close auto-memory entry is `project-session-56-close.md`,
  indexed in `MEMORY.md`, with a feedback entry on contract review
  convergence and Windows facts added to `reference-windows-acl-facts`.
