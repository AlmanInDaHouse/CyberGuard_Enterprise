# Handoff — Session 77

Session 77, 2026-11-16, landed C10's first commit: `docs/operations/install.md` installs the
Windows endpoint with the agent's setup instead of the console agent (`fdd0ea5`, PR #143), with
`install.test.mjs` and a `cg-setup` test that runs the runbook's bootstrap file through the setup's
parser. Manuel ran an elevated reading of the runbook's blocks. Commits 2 to 5 of *C10's plan*
(`docs/handoff-session-76.md`) remain; 2 is the only one that waits on no answer of Manuel's. No
tag was pushed: the release freeze stays.

## Start and resulting state

- Start: `main` = `origin/main` = `07811a6`, clean, no stash, no branch but `main`, no PR; the
  tip's CI ALL GREEN (markdown-lint); 22 ADRs / 23 SPECs, 56 open debts (56 rows), no Known CI
  debt. The repo agreed with Session 76's handoff. 5.5 GB of 15.8 GB of memory free, 26 GB on
  `C:`; Docker Desktop down (Manuel's `aura_*` containers not running, not touched).
- Manuel's prompt held no answer on M1 to M6, which stay waiting.
- Resulting `main`, in order, each pushed alone with its CI green:
  - `fdd0ea5`, C10 commit 1, fast-forwarded from PR #143 (GitHub shows it merged);
  - this close's commit.
- The branch `docs/c10-install-setup` is deleted, local and remote. No other branch is left.
- Catalogs: 22 ADRs / 23 SPECs, unchanged.

## What landed (`fdd0ea5`)

The branch carried four commits (`39183c8`, `521c38a` then `c1d30ec`, `0ee8e72`, `3e2b386`), each
pushed alone with CI green but the last; before landing they were squashed into one commit whose
tree is `3e2b386`'s (`git diff --quiet 3e2b386 fdd0ea5`), pushed with `--force-with-lease`, its CI
ALL GREEN, and fast-forwarded.

- `docs/operations/install.md`:
  - the release's Windows assets: the setup, and the agent's executable, whose `SHA256SUMS` line is
    the installed binary's digest; the verify block attests the setup and the executable (four
    lines); neither is code-signed (ADR-0022 Open question 1);
  - the endpoint's prerequisites: the floor (build 17763), Windows PowerShell 5.1, an existing
    `C:\ProgramData\CyberGuard` refused (27), an existing `C:\Program Files\CyberGuard` unchecked
    (SPEC-024 Amendment 2026-11-12, point 3); a console installation's refusal (28, or 27 when it
    never enrolled) and its R1 disclosure kept until the migration's runbook;
  - `C:\CyberGuardSetup`, created with `O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)` by
    `Directory.CreateDirectory(path, DirectorySecurity)` and checked through its printed line; the
    setup copied into it and checked there; the fingerprints from the server's record, not the
    anchor's last line (#150's destination, in part);
  - the bootstrap file: the token read by `Read-Host -AsSecureString` and written into a file
    created new (`FileMode.CreateNew`) with `O:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)`; its deletion;
  - the unattended command, `Start-Process ... '-quiet', '-norestart', '-log', $log,
    "CG_BOOTSTRAP=..." -Wait -PassThru`, and the wizard's, with the disclosures of the Amendments
    2026-11-12 (second), point 10, and 2026-11-13, points 5, 6 and 8;
  - the result table: every code of `cg-setup` on an installation and Burn's (0, 3010, 3017, 1602,
    1638, 1603, 0x80091007, 20 to 31, 40 to 45), each in decimal and hexadecimal with the
    endpoint's state and its retry; the reversal acting on an earlier record, an upgrade's restore;
  - Burn's log: the four steps, the three lines, the `0x8007` decoding;
  - the checks: the installed binary's digest, `sc.exe` (`query`, `qc`, `qfailure`,
    `qfailureflag`, `sdshow`), the Application log, the agent's log lines (`enrollment response
    received` with `expires_at`, `agent identity ready`, `signed heartbeat sent`);
  - install_ac_016's read-only inspection: `auditpol` by GUID, the per-user count by SIDs, the dump,
    page-file, hibernation and Windows Error Reporting settings (`cg-agent.exe` and `cg-setup.exe`,
    SYSTEM's and the administrator's keys), with the evidence the criterion lists;
  - the service's lifecycle and files; the agent's exit-code table for the service (code 4 is the
    console's; code 13's stale migration clause fixed); the console route's text removed; the
    `Â§` mojibake fixed.
- `deploy/docker/test/install.test.mjs`: the fifteen endpoint blocks classified and ordered; a
  test of the endpoint's commands (the setup's options, `Start-Process -Wait -PassThru`, the token's
  two uses, the SDDLs and `CreateNew`, no write under `ProgramData`, the binary's digest, the audit
  GUIDs, `LocalDumps`); a test of the result table against `agent/cg-setup/src/codes.rs` (but
  `PURGE_*`) and Burn's codes; the verify block's fixture with the setup.
- `agent/cg-setup/tests/install_ac_002_setup_runbook.rs`: the runbook's bootstrap here-string,
  expanded as PowerShell expands it, through `parse_bootstrap` and `check_staged`, its anchor where
  the copy step puts it, an absolute Windows path; and `export-ca`'s output with its trailing line
  read as its certificate alone (a guard: it passes on the old code).
- `.github/workflows/rust-ci.yml`: runs on a change to `docs/operations/install.md`.
- `docs/operations/configuration.md:532`, `docs/operations/README.md:5`: the setup writes
  `agent.toml`; the agents come from the setup.

## The elevated reading (Manuel, `C:\tmp\s77-read1.ps1`, at `0ee8e72`)

The script ran the document's own blocks, elevated, on a probe directory under `C:\tmp`, removed
at its end:

- `Directory O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA) 0`, and `... 1` once a file was in it;
- `bootstrap.toml: O:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)`, first bytes `5b 73 65` (no BOM);
  `nodefaultadminowner` empty; a copied file `O:BAD:(A;ID;FA;;;SY)(A;ID;FA;;;BA)`;
- `auditpol /get /r`: a localized CSV header and one row per GUID (`Aciertos`, `Aciertos y
  errores`), exit 0; `auditpol /list /user /r` with no per-user policy prints one localized
  sentence and no account, exit 0: the count became the SIDs of `/list /user /v /r` (`3e2b386`);
- `Get-WinEvent` on an unregistered source answers with an error, not "no events" (`3e2b386`);
- `sc.exe sdshow` elevated adds an `S:` part: the runbook compares the `D:` part;
- `reg query` of an absent value answers `ERROR: El sistema no ha podido encontrar ...`.

Two lines were the probe script's own defect, not the runbook's: the block's `$bootstrap`
overwrote the script's variable of the same name, so the second run read the TOML as code (`No se
encuentra el tipo [server]`) and `Remove-Variable` met an optimized local. `CreateNew`'s refusal
of an existing file was read unelevated earlier (`IOException`).

## Decisions

### Manuel's

- None this session. M1 to M6: no answer.

### Anticipated (Claude Code)

- **The inputs' directory created with its security** (`Directory.CreateDirectory` with a
  `DirectorySecurity`), its owner, DACL and emptiness printed. Discarded: `New-Item` then `icacls
  /inheritance:r`, which leaves a window where an ordinary user can create an entry or hold a
  handle opened with the inherited rights (#150's last residual).
- **The bootstrap file created new with an explicit owner and DACL** (`FileStream` with
  `FileMode.CreateNew` and a `FileSecurity`), the token from `Read-Host -AsSecureString`.
  Discarded: `Set-Content` into the directory (owner by policy, `Object creator` refused with 23)
  and a typed `$token = '...'` (PSReadLine's history keeps it).
- **`Start-Process -Wait -PassThru`**: read on this host, the call operator does not wait for a
  windowed program and leaves `$LASTEXITCODE` empty; `Start-Process` returned 3010, and an HRESULT
  as -2146889721. Discarded: `&`.
- **`-log` on every run, inside the inputs' directory**, never under `<P>`. Discarded: Burn's
  default, whose folder for an elevated administrator is not read.
- **A Rust test that reads the runbook**, with rust-ci triggered by `install.md`: the setup's own
  parser and the service's checks on the documented file. Discarded: the node test alone, which
  only matches text.
- **The code table checked against `codes.rs`** in `install.test.mjs`, `PURGE_*` excluded until
  commit 2.
- **The heading `On the endpoint: install and run the agent` kept**, so that `upgrade.md`'s anchors
  resolve until commit 3 rewrites its agents' part.
- **The console route removed from `install.md`**, its R1 disclosure kept for existing console
  hosts. Discarded: keeping it as a development route (SPEC-023 Amendment 2026-10-27 made it
  temporary until this runbook).
- **Squash before landing** (one commit, its tree the last tested one). Discarded: four
  fast-forwards, each with a CI round on `main`.
- **Commit 2 left to Session 78** (`feedback-checkpoint-over-rushing`): it alone is unblocked, and
  it needs its own reading and review rounds.

### Reactive corrections

- **Lost backslashes.** A node edit sent through a Bash heredoc wrote `C:ProgramDataCyberGuard`
  (`39183c8`, line 61); both the fresh reviewer and the adversarial pass found it. Later edits were
  written with Write (`feedback-bash-heredoc-backslashes`, which recurred); a commit message lost
  `\t` the same way and was rewritten from a file.
- The adversarial pass and three fresh reviews corrected the table and its prose (`c1d30ec`,
  `0ee8e72`): the reversal acting on an earlier record (left by 40, 41, 45, a failed deletion or a
  crash); an upgrade's restore after 20 to 31; 1602 during the MSI and during configure; 3010's
  restart reason; 3017's other causes; 20's floor (a Windows before 10 does not run `cg-setup`);
  26's unreadable configuration; 27's "any access"; 31 without entry 1003; 40 after the stop;
  42 without 4, since the service waits 60 seconds between enrollment rounds; 43's other states;
  44's other causes; 45's leftovers; the failure page's two lines; Windows PowerShell 5.1; the
  wizard's closed window (Amendment 2026-11-13, point 8); the token's best-effort removal; the
  administrator's WER keys; another `$dir`'s parents.
- The elevated reading changed the per-user count and the `Get-WinEvent` sentence (`3e2b386`).

## Self-review (CLAUDE.md compensating control 5)

### Claude Code's matrix

Contract: SPEC-024's runbook obligations (§Data contracts §1, §3; §Operational §1, §5; install_ac_002,
015, 016; the Amendments 2026-11-01 points 4 and 5 to 2026-11-16); SPEC-023 deploy_ac_021 and its
Amendment 2026-10-26 (third), item 7; #150's destination.

| Criterion | Test | Verdict |
|---|---|---|
| Every server block executed; every fence classified | `install.test.mjs` (classification, stack run in deploy-ci) | covered (seen red on `main`'s document) |
| The setup's and the executable's attestation; an unattested fixture refused | `install.test.mjs` stack run (four verify lines, three `OK`) | covered |
| The runbook's bootstrap file is one the setup takes; its configuration the service's | `install_ac_002_setup_runbook.rs` | covered (seen red: "the runbook has no endpoint-directory block") |
| The trailing `sha256:` line is no certificate and no fingerprint | same | covered (a guard) |
| `-quiet -norestart -log`, the wizard without `-quiet`; `Start-Process -Wait` | `install.test.mjs` endpoint test | covered (red on `main`'s document) |
| The token read without echo, written into a file created new with its protection; no token on a command line | same; Manuel's reading of the SDDLs | covered as text; the SDDLs read elevated |
| Every code with its hexadecimal form | `install.test.mjs` table test against `codes.rs` | covered (red on `main`'s document) |
| Each code's state and retry | none | weak: reviewed three times against the SPEC, not tested |
| The wizard's and the launch route's disclosures | none | weak: text |
| The installed binary's digest; install_ac_016's inspections | `install.test.mjs` (the commands' text); Manuel's reading | weak: not run against an installed agent (install_ac_016, the isolated machine) |

### The fresh reviewers (the fixed prompt, isolated worktrees) and the adversarial pass

- At `39183c8`: blocking, the lost backslashes and "Unchanged" on an upgrade, **fixed**; R1 for
  console hosts, **kept**; "after its elevation prompt", the wizard's path, `agent.id`'s key,
  **fixed**; Windows PowerShell 5.1, **named**; CLAUDE.md's local gate for `install.md`, **recorded**
  below (Manuel's).
- A three-agent adversarial pass at `39183c8` (35 findings over the table, Burn's log and the rest):
  all **taken**, as *Reactive corrections* lists.
- At `521c38a`: line 421 against the table, the token's order, the reversal after the MSI, the
  evidence fields, the codes outside the table, `README.md:5`, an absolute path in the test,
  **fixed**; `agent.toml.setup` after an uninstallation, codes 14 and 15, the purge step's own
  failure: commit 2; `upgrade.md`: commit 3.
- At `c1d30ec`: the earlier record's codes, 27 for a console host never enrolled, 1602 during the
  MSI, `Remove-Variable token`, the administrator's WER keys, `$dir`'s parents, **fixed**;
  `upgrade.md` (commit 3), `threat-model.md:357` (#164), a deployment tool's bootstrap file (M5),
  tuning `agent.toml` on a service host (#165), recorded.

## Gates and CI

- Local gate at `39183c8`: `cargo fmt --all -- --check` clean; `cargo clippy --all-targets
  --all-features -- -D warnings` "Finished"; `cargo test --all` 99 binaries, 617 passed, 0 failed,
  11 ignored; `node .github/scripts/agent-version-matrix.mjs` "agent version matrix: all steps
  passed"; `node --test` install, operations and upgrade: 5 passed, 4 skipped (root and stack);
  markdownlint "Summary: 0 issues in 0 files". After it the Rust changes were one test file: `cargo
  fmt --check` clean, `cargo clippy -p cg-setup --all-targets --all-features` "Finished",
  `install_ac_002_setup_runbook` 2 passed, at `c1d30ec`'s and `3e2b386`'s trees.
- No elevated gate: no path of *Developer-local SPEC-005 marquee validation* §5 changed.
- CI on PR #143: `39183c8`, `c1d30ec`, `0ee8e72` and `fdd0ea5` each markdown-lint, ts-ci,
  deploy-ci, rust-ci, package-ci success, ALL GREEN. `fdd0ea5` on `main` (push): markdown-lint,
  ts-ci, rust-ci, package-ci success, ALL GREEN (deploy-ci has no push trigger). This close's
  commit: as its post-push report gives it.

## Test baselines at close

Non-elevated `cargo test --all`: 99 binaries, 617 passed, 11 ignored. Elevated `--ignored` 7 and
vitest 86 files, 504 passed, 2 skipped, unchanged from Session 72.

## Debts

- Closed: none. #150's destination is met for the bootstrap file and the anchor's directory; it
  closes with commit 5, as planned.
- Opened:
  - **#164**: `docs/security/threat-model.md:357` (R17) cites `install.md` for the console
    runbook's `New-Item` and its code-13 remedy, which `fdd0ea5` removed. Threat-model text is
    Manuel's; destination: commit 4's threat-model proposal (M4).
  - **#165**: no runbook route sets `agent.toml`'s tuning keys on a service host: the bootstrap
    file takes five keys, and an edit that replaces the SYSTEM-owned file is refused at the next
    start. Destination: commit 3 (an in-place edit with its custody), or a SPEC if a route is new.
- Open at close: **58**.

## Waiting on Manuel

- **M1** (#160's text), **M2** (#162's order), **M3** (the isolated machine: install_ac_016 runs
  these endpoint steps there), **M4** (R1 and #87, and now #164), **M5** (deployment tools: how a
  tool writes the protected bootstrap file is part of it), **M6** (re-enrollment), as Session 76
  put them.
- **CLAUDE.md's local gate**: a change to `docs/operations/install.md` now runs rust-ci's
  `install_ac_002_setup_runbook`; *Integration path* §1 and the *Local pre-commit gate* do not say
  so. Proposed: the *Rust* bullet gains "or `docs/operations/install.md`", and such a change goes
  through a branch (your prompt already asked it for the runbooks). A rule change, so yours.
- The release freeze stays; Authenticode stays ADR-0022's Open question 1.

## How Session 78 resumes

1. Run the start protocol (CLAUDE.md). `main` is this handoff's close commit. There is no PR. The
   counts are 58 open debts and 22 ADRs / 23 SPECs.
2. Manuel's answers on M1 to M6 and the CLAUDE.md proposal, if given.
3. **C10, commit 2, the uninstallation and the purge** (*C10's plan* in
   `docs/handoff-session-76.md`): a section or document for the default uninstallation and what it
   keeps (`agent.toml.setup` included, Amendment 2026-11-09, item 6); `setup.exe -uninstall
   CG_PURGE=yes -norestart -log <file outside <P>> [-quiet]` with `Start-Process -Wait -PassThru`;
   codes 46 and 47 in the result table (`install.test.mjs` then drops its `PURGE_*` exclusion) and
   the service's 14 and 15 in the agent's; what the purge is not (Amendment 2026-11-15, point 8);
   the step's runs (Amendment 2026-11-16); `operations.test.mjs` if a document is added. Same
   cycle: branch, reading where needed, fresh review with the code prompt, the adversarial pass,
   CI green per push, squash, fast-forward.
4. Then commits 3 to 5 as answers allow. Keep the release freeze.

## Environment and memory

- Docker Desktop stayed down; nothing of Manuel's was touched.
- `C:\tmp` holds `s77-read1.ps1` and its log (UTF-16, `s77-read1.log`, overwritten by Manuel's
  run), `s77-ci.sh` (the CI poll), `s77-test-all.out`; `s76-c10-journal.json` stays as C10's map.
  The session's scratchpad holds the fact maps and the probes.
- `target\` holds debug builds without debug info (`CARGO_PROFILE_*_DEBUG=0`).
- Updated memory: the session's close; a reference on the endpoint's PowerShell facts.
