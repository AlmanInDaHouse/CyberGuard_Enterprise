# Handoff — Session 78

Session 78, 2026-11-16 to 2026-11-17, landed C10's second commit: `docs/operations/install.md`
removes the agent from an endpoint, by the default uninstallation or by the purge (`14afeb3`,
PR #144), with `install.test.mjs`'s checks against `cg-setup`'s and the service's code. Commits 3 to 5
of *C10's plan* (`docs/handoff-session-76.md`) remain; each waits on an answer or a reading of
Manuel's. No tag was pushed: the release freeze stays.

## Start and resulting state

- Start: `main` = `origin/main` = `fb9511c`, clean, no stash, no branch but `main`, no PR; the
  tip's CI ALL GREEN (markdown-lint, and the scheduled deploy-ci and dependency-audit); 22 ADRs /
  23 SPECs, 58 open debts (58 rows), no Known CI debt. The repo agreed with Session 77's handoff.
  5.3 GB of 15.8 GB of memory free, 26.3 GB on `C:`; Docker Desktop down (Manuel's `aura_*`
  containers not running, not touched).
- Manuel's prompt held no answer on M1 to M6 or on the CLAUDE.md proposal, which stay waiting.
- Resulting `main`, in order, each pushed alone with its CI green:
  - `14afeb3`, C10 commit 2, fast-forwarded from PR #144;
  - this close's commit.
- The branch `docs/c10-uninstall-purge` is deleted, local and remote. No other branch is left.
- Catalogs: 22 ADRs / 23 SPECs, unchanged.

## What landed (`14afeb3`)

The branch carried `7174544` and `2c68c36`, each pushed alone with CI ALL GREEN (markdown-lint,
deploy-ci, rust-ci); before landing they were squashed into one commit whose tree is `2c68c36`'s
(`git diff --quiet 2c68c36 14afeb3`), pushed with `--force-with-lease`, its CI ALL GREEN, and
fast-forwarded.

- `docs/operations/install.md`, a section *Remove the agent from an endpoint* (SPEC-024
  §Operational §4, install_ac_012, Amendments 2026-11-15 and 2026-11-16):
  - the setup to uninstall with: the installed version's, from the inputs' directory, checked
    again against `SHA256SUMS`; the installed programs' entry never purges, and may start
    unelevated (marked not read);
  - the default uninstallation, `-uninstall -quiet -norestart -log`, with `Start-Process -Wait
    -PassThru`: the `Purge` step with no confirmation, Windows Installer's stop and removals, what it
    keeps with its owners and DACLs (`agent.toml` and its token when not removed,
    `agent.toml.setup` and the staged files, the temporary files, `identity\`, `logs\`,
    `migration.json` and `agent.legacy` when a migration kept it), and the reinstallation that loads
    the same identity without a token;
  - the purge, `-uninstall CG_PURGE=yes -quiet -norestart -log`: the value's rules, the routes that
    never purge (`-unsafeuninstall` marked by contract, not read), the log and any shell kept out of
    `<P>`, a check block afterwards (`Test-Path`, `sc.exe query`, since the step runs only while the
    setup is installed), the step's five steps, the service's purge start, the closed list of names
    and its order, what it leaves and 15;
  - what the purge is not (point 8, the ordinary user's levers as the threat model names them, the
    manual routes disclosed and not taught, a cancellation's wait before a rerun), a host uninstalled
    without the purge (point 9), the window without events;
  - the uninstallation's result table: 0, 3010, 3017, 1602, 1603, 20, 21, 26, 27, 28, 31, 40, 46,
    47, each with the endpoint's state and its retry; the step's failure and rollback lines in
    Burn's log; entry 1002 read only where the purge start wrote one.
- The setup's result gains 46 and 47; the agent's exit codes gain 14 and 15 and a paragraph on the
  purge start's codes (2 for `--purge` with another argument or a `--config` not the agent's, 10 or
  1, 13, 14, 15; 1001's text; the restart of a dead purge process).
- The setup's 27 and the service's 13 no longer teach the removal of the directory: a recursive
  removal can follow a junction planted in it, and `identity` is SYSTEM's alone (#166).
- `install.md`'s `Purge` step line, the Application log's source after an uninstallation, and
  `agent.toml.setup` kept by a default uninstallation (the threat model's line 400, "the runbook
  discloses this").
- `docs/operations/README.md`: `install.md`'s row names the removal.
- `deploy/docker/test/install.test.mjs`: the removal's five blocks classified, under their own
  section; the uninstall and purge commands (`-uninstall`, `CG_PURGE=yes` once, `-quiet`,
  `-norestart`, `-log` naming `$log` under the inputs' directory, no `-unsafeuninstall`); the check
  block verbatim; every `codes.rs` code, `PURGE_*` included, in the setup's result; the
  uninstallation table's exact codes, and 46's service codes against `PURGE_BEFORE_REMOVAL`; the
  agent's exit codes 0 to 15 against `service/mod.rs`, with the purge start's lines from
  `purge.rs`, `migration.rs` and `mod.rs`; the purge's names against `purge.rs`'s constants.

## Decisions

### Manuel's

- None this session. M1 to M6 and the CLAUDE.md proposal: no answer.

### Anticipated (Claude Code)

- **The removal in `install.md`, as a section of its own** before *Later users*. Discarded: a new
  document, whose blocks no test classifies (`install.test.mjs` reads `install.md` alone) and whose
  codes would repeat `install.md`'s tables.
- **An uninstallation result table of its own**, with 46 and 47 in the setup's result as short rows
  pointing to it. Discarded: purge causes added to the installation's rows, which mixes two routes
  in one cell.
- **`-norestart` on the uninstallation too** (the reason of the Amendment 2026-11-09, item 2; the
  SPEC names it for the purge only). Discarded: Burn's default, which may restart the computer.
- **The setup to uninstall with is the installed version's, from the inputs' directory.**
  Discarded: the package cache's registered setup, whose path no test reads.
- **The manual purge routes disclosed, not taught** (`cg-setup.exe purge --confirm=yes`,
  `--purge`), as C10's plan said.
- **Tests that read the code**: the names from `purge.rs`, the codes from `codes.rs`,
  `service/mod.rs` and `service.rs`, so that the runbook fails when the code moves.
- **Commit 2 alone this session** (`feedback-checkpoint-over-rushing`): commit 3's centre is M6's
  route, commit 4 waits on M4 and an elevated reading, commit 5 on M1.

### Reactive corrections

- The first review (a fixed-prompt reviewer and three lenses: the SPEC, the code, the operator)
  found, blocking: 47 stated as "the service stopped" (a dead purge process is restarted, and after
  300 seconds the start may still run); the purge's 0 read as the data removed although the step
  runs only while the setup is installed; manual removals that could follow a junction or take
  ownership of the identity; and 46's restart that can fail. Fixed in `2c68c36` with the check
  block, the service's states, the withdrawn removals; and about twenty minor ones (the code-2 and
  code-1 causes, 31's and 40's causes, the staged and temporary files, `agent.legacy` only when
  kept, `agent.toml`'s token, 1003's count, the wait before a rerun, the ordinary user's levers as
  the threat model names them, the installed programs' entry, 3010 in the check, a shell in `<P>`).
- The second review found the same hazard in the row I had edited, the setup's 27 (a removal
  "as itself first" with no command): that row and the service's 13 now teach no removal (#166).
- The third found 47's restart delay (60, 120 or 300 seconds) and the wait condition, 46's route
  with no service, and the installed programs' entry's unelevated start unmarked: fixed.
- Tests: `codeRows` stopped only at its own heading level, `-log` was not tied to `$log`, the
  uninstallation table admitted extra codes, the blocks' order check passed vacuously for the
  removal's blocks (a mutation now turns it red), the `codes.rs` guard was weakened: fixed.

## Self-review (CLAUDE.md compensating control 5)

### Claude Code's matrix

Contract: SPEC-024 §Operational §4, install_ac_012, Amendment 2026-11-15 (points 1 to 9),
Amendment 2026-11-16; the threat model's Amendment 2026-11-15 and its line 400.

| Criterion | Test | Verdict |
|---|---|---|
| The purge's command (`-uninstall CG_PURGE=yes -quiet -norestart -log`, `Start-Process -Wait -PassThru`) | `install.test.mjs`, endpoint commands test | covered (seen red on `fb9511c`'s document) |
| The default uninstallation carries no `CG_PURGE`, neither `-unsafeuninstall` | same | covered |
| Burn's log outside `<P>` (point 7) | same (`$log` under `$dir`, `-log` naming it) | covered |
| The purge checked afterwards | same (the block verbatim) | covered as text; its reading weak |
| 46 and 47 in the setup's result | the result test against `codes.rs` | covered (seen red) |
| The uninstallation's codes, and 46's service codes | same, against `PURGE_BEFORE_REMOVAL` | covered for codes; causes and states weak (text, three reviews) |
| The service's 14 and 15 and the purge start's lines | the exit-codes test against `service/mod.rs`, `purge.rs`, `migration.rs` | covered (seen red) |
| Point 5's closed list | the names test against `purge.rs` | covered (seen red) |
| Point 6's order; points 8 and 9; the window; `agent.toml.setup` kept | none | weak: text |
| Point 6's repair; the console executable's removal | none | missing: commit 4 (#166, #87) |
| The blocks run on an endpoint (install_ac_016) | none | missing: M3 |

### The fresh reviewers (the fixed prompt, isolated worktrees) and the adversarial lenses

- At `7174544`: the fixed-prompt reviewer and three lenses, as *Reactive corrections* gives them:
  all **taken**, but the repairs (**deferred**: commit 4, #166) and "the identity's descriptor is
  not compared" (**not blocking**: the bundle host test reads its reuse).
- At `d2c3e41`: the fixed-prompt reviewer (no contradiction of the contract; four imprecisions,
  **fixed**) and a lens (two blocking in the setup's 27, **fixed**; fifteen minor, **fixed** but
  "codes 0 to 10 are not read from the code", **kept**: they are the console's, outside
  `service/mod.rs`, and the test asserts the rows run 0 to 15 in order; and "the owner and DACL each
  object had", **kept**: §Operational §4's contract, the bundle host test reading five of them).
- At `261b8ef`: the fixed-prompt reviewer: no contradiction of the contract; five imprecisions,
  **fixed**; "the test reads the code by its declarations' format", **kept**: a reformat turns it
  red, which is its purpose.

## Gates and CI

- Local gate at `2c68c36`'s tree: `node --test deploy/docker/test/install.test.mjs
  operations.test.mjs upgrade.test.mjs`: 7 passed, 0 failed, 4 skipped (root and stack);
  markdownlint "Summary: 0 issues in 0 files"; `cargo test -p cg-setup --test
  install_ac_002_setup_runbook`: 2 passed. The red phase, against `fb9511c`'s document: 5 failed, 1
  skipped (`C:\tmp\s78-red.txt`); the round-2 tests against `7174544`'s document: 2 failed.
- No Rust, workflow or schema changed; no elevated gate: no path of *Developer-local SPEC-005
  marquee validation* §5 changed.
- CI on PR #144: `7174544`, `2c68c36` and `14afeb3` each markdown-lint, deploy-ci, rust-ci success,
  ALL GREEN (ts-ci and package-ci not triggered). `14afeb3` on `main` (push): markdown-lint,
  rust-ci success, ALL GREEN (deploy-ci has no push trigger). This close's commit: as its post-push
  report gives it.

## Test baselines at close

Non-elevated `cargo test --all`: 99 binaries, 617 passed, 11 ignored (unchanged: no Rust changed).
Elevated `--ignored` 7 and vitest 86 files, 504 passed, 2 skipped, unchanged from Session 72.
`install.test.mjs`: 5 passed, 1 skipped (was 3 and 1).

## Debts

- Closed: none.
- Opened:
  - **#166**: the runbook gives no removal or repair, safe against a planted junction and without
    taking ownership of the identity, for an untrusted `%ProgramData%\CyberGuard` or `agent`
    directory (the setup's 27, the service's 13, the purge's 27 and its 46 for 13 or 14) nor for a
    name the purge leaves (47; SPEC-024 Amendment 2026-11-15, point 6, "until the runbook's repair
    (C10) removes it"); `14afeb3` withdrew the removal the setup's 27 and the service's 13 taught.
    Destination: C10 commit 4, with the repairs' elevated reading (#87).
- Open at close: **59**.

## Waiting on Manuel

- **M1** (#160's text), **M2** (#162's order), **M3** (the isolated machine: install_ac_016 runs
  the endpoint steps there, now the removal's five blocks too), **M4** (R1 and #87, #164), **M5**
  (deployment tools), **M6** (re-enrollment: the purge and a new setup with a new token is the
  route point 8 describes; `install.md` states its facts and still says re-enrollment is not in it),
  as Session 76 put them.
- **The repairs' elevated reading** for commit 4 (#87, #166): `takeown /A`, `icacls /setowner
  *S-1-5-18`, and a removal that does not follow junctions, on a probe directory, as S77's reading.
- **CLAUDE.md's local gate**: the proposal of Session 77 stands (a change to
  `docs/operations/install.md` runs rust-ci's `install_ac_002_setup_runbook`).
- The release freeze stays; Authenticode stays ADR-0022's Open question 1.

## How Session 79 resumes

1. Run the start protocol (CLAUDE.md). `main` is this handoff's close commit. There is no PR. The
   counts are 59 open debts and 22 ADRs / 23 SPECs.
2. Manuel's answers on M1 to M6, the repairs' reading and the CLAUDE.md proposal, if given.
3. **C10, commit 3, `upgrade.md`'s agents** (*C10's plan*): the newer setup with no bootstrap (30
   with one), downgrade 29, the restores read and not read, the window without events
   (`install.md`'s section), #165's in-place edit of `agent.toml` with its custody or its SPEC; the
   re-enrollment route only with M6's answer, otherwise `upgrade.md`'s console route removed and
   re-enrollment named as not in the runbooks. Same cycle: branch, fixed-prompt review and lenses,
   CI green per push, squash, fast-forward.
4. Then commit 4 (the migration's runbook, the repairs, #87, #166) after the elevated reading and
   M4, and commit 5 (#160 after M1, #150). Keep the release freeze.

## Environment and memory

- Docker Desktop stayed down; nothing of Manuel's was touched.
- `C:\tmp` holds `s78-facts-*.md` (the readers' fact map of the purge and the uninstallation),
  `s78-review1-*.md` and `s78-review2-*.md` (the reviews), `s78-red.txt` (the red phase),
  `s78-ci.sh` (the CI poll), the commit messages `s78-msg*.txt`.
- `target\` holds debug builds without debug info (`CARGO_PROFILE_*_DEBUG=0`).
- Updated memory: the session's close.
