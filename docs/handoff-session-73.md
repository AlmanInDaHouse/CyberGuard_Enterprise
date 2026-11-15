# Handoff — Session 73

Session 73, 2026-11-15, wrote C9's contract: SPEC-024 Amendment 2026-11-15, the purge, with the
threat model's Amendment 2026-11-15, after the reading probe #134 and ten fixed-prompt review
passes; Manuel made its choices and approved its text. No code landed: C9's implementation is
Session 74's, from the plan below. No tag was pushed: the release freeze stays.

## Start and resulting state

- Start: `main` = `origin/main` = `7d4780f`, clean, no stash, no branch but `main`, no PR; the
  tip's CI ALL GREEN (markdown-lint); 22 ADRs / 23 SPECs, 56 open debts (56 rows), no Known CI
  debt. The repo agreed with Session 72's handoff. 3.4 GB of 15.8 GB of memory free, 15.9 GB on
  `C:`; Docker Desktop up with Manuel's `aura_cache` and `aura_db`, not touched.
- `C:\Users\manul\cg-s72-deps` is still held by another process (`rmdir`: "Device or resource
  busy"); left as it is.
- Resulting `main`: `d3c3e3b`, the amendment and the threat model's, one commit straight to
  `main` from the owner-review branch `review/c9-purge-contract` (approved at `21f0344`), then
  this close's commit.
- PR #134 (the probe) is closed without landing; its branch and the review branch are deleted.
  No other branch is left. Its code stays at `refs/pull/134/head` (`c553c81`).
- Catalogs: 22 ADRs / 23 SPECs, unchanged in number; SPEC-024's catalog gains the amendment's
  line.

## What landed

### The probe (PR #134, `c553c81`, never landed)

Run 43659558742, job "The setup on the host", its readings step (unable to fail the job). The
bundle gained a `Purge` `ExePackage` detected present (`DetectCondition="1"`) after the rollback
boundary, whose `cg-setup purge` stub recorded its arguments and the host's state and exited
with a code the test left for it. `pins` was red, by design: `projects.test.mjs` pins the chain.

| Reading | Result |
|---|---|
| Uninstallation with `CG_PURGE=yes` (the file it was installed from) | the step before the MSI, `--confirm="yes"`, the service `RUNNING`, the binary present; exit 0 |
| Uninstallation without it | `--confirm=""` |
| The registered entry's `QuietUninstallString` | `--confirm=""` |
| Installation, repair (`CG_PURGE=yes`) | plan `execute: None`; the step not run; its payload cached at installation |
| Upgrade 1.2.3→1.2.4, second build, with `CG_PURGE=yes` | the newer step not run; the replaced setup's step run with `--confirm=""` while the newer agent ran |
| A step exiting 1, then 46 | the setup exits 1 and 46; setup, MSI and service stay; Burn's rollback ran the step with no argument: 40 |

### The contract (`d3c3e3b`)

SPEC-024 Amendment 2026-11-15, nine points: the route, `setup.exe -uninstall CG_PURGE=yes`, not
persisted (1); the chain's step, vital, detected present, between the boundary and configure,
whose rollback runs `purge` with no option (2); `cg-setup purge --confirm=`, which without `yes`
reads nothing under `<P>` and exits 0, and with it reads the state (27, 28), checks the service
(26; absent: 46), proves the stop (31), starts the service with `--purge` and maps its code to 0,
46 (nothing removed; the service started again if it was running) or 47 (may have removed, or
unread) (3); the service's purge start: the lock (10, 1), the directories' check (13 with its own
line), the state (14), no configuration, file log, ETW or `RUNNING`, removals bounded to 120 s
with 30 s cancellable opens, 15 for a name left, the 120 s or any abnormal end (4); a closed list
of the product's names, through handles that follow no reparse point (5); the order
`agent.toml.setup`, `agent.toml`, `agent.toml.hygiene-tmp`, the legacy directory, the identity,
each only once the one before left nothing, then the rest (6); the record, Burn's log and entries
1001/1002 (7); what it is not (8); a host uninstalled without the purge (9). *Tests* and *Effect*
as in the text. The threat model's amendment adds to §1, Logging and monitoring, R1, R5, R17 and
the accepted threats, and reads two sentences with a limit.

## Decisions

### Manuel's (2026-11-15, in chat)

- The route: "setup.exe -uninstall CG_PURGE=yes (Recomendado)".
- Who removes: "El servicio como SYSTEM con --purge (Recomendado)".
- The logs: "Se borran con el resto (Recomendado)".
- The surface: "Solo línea de comandos (Recomendado)".
- ADR-0008's reclaim: "Lectura por alcance (Recomendado)": the purge start, which opens no
  capture session, reclaims none; no ADR text changes.
- The text at `21f0344`: "Apruebo".
- #160 and #162: no answer this session; both stay waiting.

### Anticipated (Claude Code)

- **A reading probe before the text.** Discarded: deciding from Burn's source alone (S68's
  precedent). It settled the order, the variable's reach, the upgrade and the failure.
- **The service as the remover, through a start argument.** The DACLs (`identity\` SYSTEM only,
  `logs\` Administrators read only) and ADR-0010's no-privilege rule for `cg-setup` leave no
  other product route. Discarded: `cg-setup` with `SeRestorePrivilege` (an ADR change); a
  runbook of `takeown` and `icacls` (unproven against junctions); an MSI custom action (#158).
- **The proposals Manuel's approval decided:** the codes 46/47 and 14/15, the refusal of a
  migration in progress, the closed list of names, point 9's route, the console executable left
  to C10's runbook.

### Reactive corrections (the review passes)

- Pass 1: a partial purge on a migrated host left states preflight refuses (27, 28) or the start
  reads as code 5: the order became the legacy directory before the identity, each step only
  once the one before left nothing; the purge start excepted from the migration's "every start";
  the closed list of names; a quote's parsing.
- Pass 2: the threat model's claims (directories removed, "never an enrollment" only after
  `agent.toml`), the legacy names, a reversed migration's staging directory, the timing.
- Pass 3: the start's order against the Amendment 2026-11-04, item 3, the untrusted state as 14,
  R17's limit, the failure page's `Purge`.
- Pass 4: the staged file before `agent.toml` (the adoption could adopt it), 27 or 28, the
  rollback's reading.
- Passes 5 to 10: `agent.toml`'s removal as the line for a restarted start, any object at a step's
  name, an abnormal end as 15 (the service maps a panic to 1 today), the Amendment 2026-11-09's
  staged files and the Amendment 2026-11-05's default refusal excepted, what 15's line names, the
  SCM's fields, §Operational §2 for the deletions, the registered setup, a console layout's 13 or
  14, and ADR-0008's reclaim (Manuel's reading).
- The heredoc backslash defect hit twice (a commit of the second pass carried the SPEC alone and
  was amended before any push); the edits then went through script files.

## Self-review (CLAUDE.md compensating control 5)

The text prompt, ten passes of three independent reviewers each, fresh context, isolated
worktrees. Every List A was fixed before Manuel read the text. Pass 10: two reviewers found no
List A; the third's, ADR-0008's reclaim, went to Manuel. The fixes of pass 10 were not reviewed
again, and Manuel was told. Claude Code's own half: each claim against the code it cites
(`identity.rs:483-485`, `log_file.rs:32-34`, `layout.rs`, `migration.rs`, `rotation.rs`,
`codes.rs`, `supervise` in `service/windows.rs`), which found the `agent.toml`-left case (pass 1).

Recorded, not blocking (List B kept for the code):

- The probe did not read: a fresh installation with `CG_PURGE=yes`; the rollback with the step's
  `InstallArguments` `purge`; what Burn does when a replaced setup's step fails during an
  upgrade. The code's probe reads the first two.
- Every uninstallation now depends on the step exiting 0, a missing cached payload included
  (disclosed in the SPEC and the threat model).
- A panic must become 15: the service has no `catch_unwind` around the purge today (`supervise`
  maps a joined task's panic to 1).
- The `START_PENDING` checkpoint every 10 s while an open waits up to 30 s needs its own thread.
- An unelevated "file of another owner" fixture needs a narrowed owner policy, as
  `install_ac_013_route.rs` does.
- The SPEC-024 row of `docs/specs/README.md:46` still stops at the Amendment 2026-11-04 (older
  than this session).
- The threat model's Amendment 2026-11-07 (`:389`) names R17's sentence "the Denial of Service
  entry's"; a ratified record, not edited.

## Gates and CI

- No code: no local Rust, TypeScript or package gate ran for a landing, and no elevated gate.
  The probe's commit passed `cargo fmt --all -- --check`, `cargo clippy -p cg-setup --all-targets
  --all-features -j 2 -- -D warnings` ("Finished `dev` profile") and the local MSI and setup builds
  ("Compilación correcta. 0 Advertencia(s) 0 Errores").
- markdownlint: "Summary: 0 issues in 0 files" on the three documents at each commit.
- CI: PR #134 `c553c81`: dependency-audit, ts-ci, deploy-ci, rust-ci success; package-ci failure
  in `pins` alone (by design), its readings step run. `main` `d3c3e3b`: markdown-lint success, ALL
  GREEN; this close's commit, as its post-push report gives it.

## Test baselines at close

Unchanged from Session 72: non-elevated `cargo test --all` 97 binaries, 569 passed, 11 ignored;
elevated `--ignored` 7 and vitest 86 files, 504 passed, 2 skipped; `deploy/windows/test` 84 tests
with the built files.

## Debts

- Closed: none. Narrowed: none. Opened: none. The amendment's unread readings are the code's
  probe's, not a debt.
- Open at close: **56**.

## Waiting on Manuel

- **#160's text** and **#162's order**, as Session 72 put them.
- The release freeze stays; Authenticode stays ADR-0022's Open question 1.

## How Session 74 resumes

1. Run the start protocol (CLAUDE.md). `main` is this handoff's close commit. There is no PR. The
   counts are 56 open debts and 22 ADRs / 23 SPECs.
2. Manuel's answers on #160 and #162, if given.
3. **C9's code**, under SPEC-024 Amendment 2026-11-15, on a branch `feat/c9-purge` with a PR, one
   commit per push, each true and green at its SHA, acceptance tests written from the amendment
   and seen failing first:
   1. **The service's purge** (`agent/cg-agent`): `Start::Purge` in
      `service/migration.rs::parse_start` (alone; another argument code 2 with
      `cg-agent: invalid purge arguments: --purge is given with another argument`); a module
      `service/purge.rs` with an `Environment` like the migration's (paths, custody policy, wait,
      a `Watch` for injected failure and crash) implementing points 4 to 6: the lock first, the
      directories' check (13, `cg-agent: the purge's directories fail their check: <reason>`),
      `layout::classify` (14), the closed list of point 5 through `custody::open_existing`,
      `check_shape`, an owner check and `delete_through`, directories held with
      `FILE_LIST_DIRECTORY` and no delete share, re-listed before removal, the order of point 6,
      the 120 s bound, codes 0/14/15; in `service/windows.rs` the purge start before the
      configuration's read, `START_PENDING` with a checkpoint thread, a panic as 15
      (`cg-agent: the purge ended abnormally`), entries 1001/1002 (15's line names at most 20
      names). Tests on temporary roots, unelevated (local gate and rust-ci), as point *Tests*
      lists. The capture path is not touched; if `lib.rs`, `etw/` or another §5 path changes,
      the elevated gate applies.
   2. **`cg-setup purge`** (`agent/cg-setup`): `cli.rs` (`--confirm=` once; empty or `yes`; other
      40); `verbs.rs` point 3 (no `Machine::system()` without `yes`; 27/28 from the state; 26 and
      the absent service's 46; the stop's 31; the start with `--purge`, a wait of 300 s reading
      every second; the mapping to 0/46/47 and the restart after 46); `codes.rs` 46 and 47. Unit
      tests; the mapping in rust-ci's SCM target (`package_setup_service`, the test service
      standing in for the agent's purge), seen red on a probe pull request.
   3. **The package and its tests** (`deploy/windows`, `agent/cg-setup/tests`): `Bundle.wxs`'s
      `CG_PURGE` and `Purge` step with `InstallArguments="purge"` and its BuildId cache folder;
      the theme's `FailureCodeText` with `Purge`; `projects.test.mjs`'s chain; the release
      check's fixtures with a third `cg-setup.exe`; `package_bundle_host.rs` (or a target of its
      own in the `bundle` job) for *Tests*' package cases, install_ac_012's first half
      strengthened; seen red first on a probe pull request with mutant legs (the junction
      followed, the staged file or `agent.toml` after the identity, `CG_PURGE` ignored, an
      upgrade that purges, the guard) and a control leg; CLAUDE.md's facts when the target or
      job changes.
   Then the self-review in two halves (matrix criterion → test → verdict; the fixed code prompt to
   a fresh reviewer), its fixes, another fresh pass after them, and landing by fast-forward or
   cherry-pick, one commit per push, CI green each.
4. Then C10, the runbooks: `install.md`'s endpoint steps on the setup, `upgrade.md`, the purge's
   command and codes (20 to 47, 14 and 15), the migration's runbook (#87's items, the console
   executable), #150's, #160's disclosure, the residuals of Sessions 69 to 73, and the endpoint
   verification of the installed binary's digest.
5. Keep the release freeze.

## Environment and memory

- Docker Desktop stayed up with Manuel's `aura_*` containers alone, not touched.
- `target\release\` holds 1.2.3 builds from Session 72; none is a release. The probe's
  implementer compiled into the main checkout's `target` (debug).
- The reviewers' downloads and the probe's local build under `C:\tmp` were removed.
- Updated memory: the session's close; `reference-burn-wix-facts.md` (probe #134's readings).
