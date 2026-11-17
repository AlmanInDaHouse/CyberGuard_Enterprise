# Handoff — Session 79

Session 79, 2026-11-17, landed C10's third commit: `docs/operations/upgrade.md` upgrades the agent
on an endpoint with a newer setup, the console-era agent text goes, and re-enrollment is named as
not in the runbooks (`1a04d1b`, PR #145). #165's in-place edit of `agent.toml` is written and
tested but not landed: its command waits on an elevated reading of Manuel's. Commits 4 and 5 of
*C10's plan* (`docs/handoff-session-76.md`) remain. No tag was pushed: the release freeze stays.

## Start and resulting state

- Start: `main` = `origin/main` = `92e5fc8`, clean, no stash, no branch but `main`, no PR; the
  tip's CI ALL GREEN (markdown-lint); 22 ADRs / 23 SPECs, 59 open debts (59 rows), no Known CI
  debt. The repo agreed with Session 78's handoff. 2.0 GB of 15.8 GB of memory free at the start
  (5.1 GB later), 23.4 GB on `C:`; Docker Desktop down (Manuel's `aura_*` containers not touched).
- Manuel's prompt held no answer on M1 to M6, the repairs' reading or the CLAUDE.md proposal,
  which stay waiting.
- Resulting `main`, in order, each pushed alone with its CI green:
  - `1a04d1b`, C10 commit 3, fast-forwarded from PR #145;
  - this close's commit.
- The branch `docs/c10-upgrade-agents` is deleted, local and remote. No other branch is left.
- Catalogs: 22 ADRs / 23 SPECs, unchanged.

## What landed (`1a04d1b`)

The branch carried `5074e0e`, `5453445`, `2d6edec`, `59a4155` and `c426b4c`, each pushed alone with
CI ALL GREEN (markdown-lint, deploy-ci, rust-ci); before landing they were squashed into one commit
whose tree is `c426b4c`'s (`git diff --quiet c426b4c 1a04d1b`), pushed with `--force-with-lease`,
its CI ALL GREEN, and fast-forwarded.

- `docs/operations/upgrade.md`, a section *Upgrading the agent on an endpoint* (SPEC-024
  §Operational §4, install_ac_011, the Amendment 2026-11-12 (second), points 3, 4, 9 and 10):
  - the newer setup and the agent's executable downloaded and attested; the inputs' directory
    checked, its attributes, owner and DACL printed as `install.md`'s block prints them at its
    creation, before a setup runs elevated from it (`upgrade: endpoint-directory`); the newer
    setup copied beside the installed one and checked (`endpoint-copy`);
  - the installation's command without `CG_BOOTSTRAP` (`endpoint-setup`), 30 with one; the steps
    (1638 and the digests before any step; 29 before the stop; the stop's 75 seconds; the MSI's
    one transaction; Restart Manager, read for a test's own process only; `Purge` changing
    nothing; `Configure`), and what an upgrade keeps, from the setup's code;
  - the upgrade's result: each code an upgrade ends with, the installed setup's restore after a
    failure marked read (0, 1638, 30, 1603, 44) or not read (29, 20 to 28, 31, 40 to 43, 45, 1602,
    and 3010, 3017 and a damaged setup on an upgrade), the SPEC's retries marked unread; the
    version in place named by its executable's digest, not entry 1000; two registered setups
    after a 44 (read), and no uninstallation before the upgrade completes;
  - Burn's restore lines (`endpoint-log`: `Applied rollback package`, `Applied non-vital
    package`, the bundle code in braces);
  - the check after the upgrade, scoped to an enrolled endpoint (an `agent identity ready` line and
    no `enrollment response received` since); the change record; the repair with `-repair` from
    the checked installed setup (`endpoint-repair`); no downgrade; the keys an administrator set,
    and a `[log] level` above `info` ending a later setup with 44; the window without events.
- The console-era agent text goes: `upgrade.md:5` ("installed and run by hand until its installer
  ships") and the console re-enrollment of *Agents* (`Copy-Item` and `Remove-Item` under
  `%ProgramData%`, a token edited into `agent.toml`). *Agents* now says that enrolling such an agent
  again is not in the runbooks yet, and keeps SPEC-023 §Operational §6's code-2 facts, which that
  SPEC says the upgrade document states.
- `docs/operations/install.md`: rows 30, 1602 and 1603, the dialog sentence and the restore
  paragraph point to the upgrade and the repair; the *Next* list names the agent's upgrade.
- `docs/operations/configuration.md`'s *The agent's tuning*, true of the service: `agent.toml` read
  at each start; the migration keeps the keys its legacy file set; the service's enrollment rounds
  60 seconds apart (the console exits 4); code 2 in entry 1002, not restarted; a level above `info`.
- `docs/operations/README.md`: `upgrade.md`'s row names the agent.
- `deploy/docker/test/upgrade.test.mjs`, four static tests: the agent's blocks against
  `install.md`'s (the directory and its printed form, the checksum, the setup's command without
  `CG_BOOTSTRAP`, `-repair`); the result table's codes against `codes.rs` and `install.md`'s table;
  the read and not-read marks in the endpoint's column; no console route and the re-enrollment
  sentence; every PowerShell fence classified, indented ones too.
  `deploy/docker/test/operations.test.mjs`: each link's anchor resolves to a heading.

## #165: written, not landed

The read-only workflow's critic and the `custody-165` reader found the in-place edit already in the
contract: SPEC-024 §Data contracts §2 keeps elevated maintenance in scope (lines 106-107), and the
Amendment 2026-11-04's *Effect*, which Manuel approved, has the runbook tell a rewrite of
`agent.toml` from its replacement and adds no setting (lines 1029-1033); the threat model's line 399
says administrators rewrite it. No SPEC is needed. What is not read is a command: the procedure in
`configuration.md` (*Change a key on an endpoint*: copy `agent.toml` to `$dir`, edit the copy,
`[System.IO.File]::WriteAllText` its text into the file, `Restart-Service`, delete the copy) rests on
.NET's `WriteAllText` keeping the file object, its owner and DACL and writing UTF-8 without a BOM.
`C:\tmp\s79-read1.ps1` reads that on a SYSTEM-owned probe file with a protected DACL (and a UTF-8
BOM and a UTF-16 copy, and `Copy-Item` for contrast); it did not run this session. The text and its
test wait outside the repository, at `C:\tmp\s79-165-config.md` and `C:\tmp\s79-165-test.mjs`, the
text with a `READING-PENDING` mark where the reading's result goes.

## Decisions

### Manuel's

- None this session. M1 to M6, the repairs' reading and the CLAUDE.md proposal: no answer.

### Anticipated (Claude Code)

- **Re-enrollment named as not in the runbooks** (M6 unanswered, as `docs/handoff-session-78.md`
  said). Discarded: the purge and a fresh setup with a new token as a taught route, which waits on
  M6.
- **The agent's upgrade in `upgrade.md`, linking `install.md` for codes, blocks and the window.**
  Discarded: repeating `install.md`'s table, which two tables would let drift; the test ties the
  upgrade's codes to `install.md`'s rows.
- **SPEC-023's code-2 facts kept** in *Agents* without their `[server]` example (false for the
  service's fixed paths). Discarded: removing them, which SPEC-023 §Operational §6 forbids.
- **#165 by the in-place edit, under the existing contract**, its command read before it lands.
  Discarded: a SPEC amendment (the contract already holds the route) and keys in the bootstrap
  file (a change to Amendment 2026-11-09's operator contract, Manuel's).
- **#165 split from commit 3** so that the upgrade lands while the reading waits
  (`feedback-checkpoint-over-rushing`).
- **Link anchors tested** (`operations.test.mjs`): the new text links by anchor, and no test read
  anchors before.

### Reactive corrections

- First review (the fixed-prompt reviewer and three lenses: the SPEC, the code, the operator), at
  `5074e0e`, blocking: the restore's "installed again" after 30 and 1603 (the older version was
  never removed); a "(read)" on Restart Manager closing an agent run by hand; the repair block run
  from an unset `$dir` and an unchecked setup; the retries after 40 to 45 given as known; an
  identity check with no "before"; two registered setups after a failed upgrade; a log-level claim
  for every setup run. Fixed in `5453445`, with about fifteen minor ones (the directory's DACL
  check, the executable attested, 45 against 3017, the record's deletion, the window's codes,
  `install.md`'s two sentences that taught the installed version without `-repair`, indented fences).
- Second review, at `5453445`, blocking, two from the first fixes: `install.md`'s 1602 retry sent
  an endpoint with nothing installed to `-repair`; "the upgrade takes no input or token" (false on
  a pending endpoint); the restore skipping the older MSI only "when Windows Installer still holds
  it" (the SPEC: installed or superseded). Fixed in `2d6edec`, with the directory check as its own
  block (attributes too), the digest naming the version, the scoped identity check.
- Third review, at `2d6edec`: the lens found no blocking point; the fixed-prompt reviewer asked
  the 3010, 3017 and damaged-setup rows to carry their marks. Fixed in `59a4155` with the lens's
  minor points (row 29, the digest's wording, the 45 and 1602 retries, a route after a failed
  directory check, the never-enrolled endpoint's 42).
- Final fixed-prompt pass, at `59a4155`: no contradiction of the contract; the log level's "not
  read" covered the two registered setups after a 44, which is read: fixed in `c426b4c`.
- Tests: `Re`**`move-Item`** matched a `Move-Item` ban (a word boundary added); bash quoting dropped
  backslashes twice in edit scripts (rewritten with the Edit tool before any commit).

## Self-review (CLAUDE.md compensating control 5)

### Claude Code's matrix

Contract: SPEC-024 §Operational §4, install_ac_011, the Amendment 2026-11-12 (second), points 3,
4, 9 and 10; SPEC-023 §Operational §6's agent bullet; C10's plan, commit 3.

| Criterion | Test | Verdict |
|---|---|---|
| The newer setup, without `CG_BOOTSTRAP`, `-quiet -norestart -log` outside `<P>` | `upgrade.test.mjs`, the agent's blocks test (equal to `install.md`'s command minus `CG_BOOTSTRAP`) | covered (seen red on `92e5fc8`'s documents) |
| The setup checked in the inputs' directory, the directory checked before an elevated run | same (the directory block and its printed form against `install.md`'s; the checksum) | covered |
| The codes an upgrade ends with | the result test against `codes.rs` and `install.md`'s table | covered (seen red) |
| Point 3: restores read and not read | same, the marks in the endpoint's column | covered for the rows it names; the causes are text |
| Point 9: repair with `-repair`; a second build | the repair block's equality; the repair's checks linked | covered; the second build text only |
| Point 10: elevated console or SYSTEM | none | weak: text |
| No console route; re-enrollment not in the runbooks (M6) | the console-route test | covered (seen red) |
| SPEC-023 §Operational §6's code-2 fact | same | covered |
| What an upgrade keeps; the window; no downgrade | none | weak: text, marked as from the code |
| Link anchors | `operations.test.mjs` | covered (a mutated anchor turns it red) |
| #165 | the test in `C:\tmp\s79-165-test.mjs` | not landed: waits on the reading |

### The fresh reviewers (the fixed prompt, isolated worktrees) and the adversarial lenses

- At `5074e0e`: the fixed-prompt reviewer and three lenses: their blocking points **fixed**
  (*Reactive corrections*); "entry 1001 does not count undelivered events" **refuted** (the
  code writes the count in 1001, `agent/cg-agent/src/service/mod.rs:143`); "a rebuild's
  `Copy-Item` overwrites the installed setup of the same name" **kept**: a release is built once
  from its tag; `anchors()` approximating GitHub's slugs **kept**: no document is affected, and a
  wrong anchor fails the test rather than passing it.
- At `5453445`: the fixed-prompt reviewer and a lens on the fixes: **fixed**; "`install.md`'s
  uninstallation runs elevated from `$dir` after a hash check only" **recorded** as part of #167.
- At `2d6edec`: the lens, no blocking point; the fixed-prompt reviewer's marks **fixed**.
- At `59a4155` (final fixed-prompt pass): no contradiction; one mark **fixed**; "the runbook does
  not limit the starting version to an adjacent one" **not blocking**: SPEC-024:279-281 is a
  release gate, and the runbook says no release-to-release upgrade has run; "`install.md`'s retries
  after 40 to 45 on an installation rerun the registered setup without `-repair`, unread"
  **recorded** as #167 (the text predates this commit).

## Gates and CI

- Local gate at `c426b4c`'s tree: `node --test` over `upgrade.test.mjs`, `install.test.mjs` and
  `operations.test.mjs`'s static tests: 10 passed, 0 failed (the stack and root tests run in
  deploy-ci); markdownlint "Summary: 0 issues in 0 files"; `cargo test -p cg-setup --test
  install_ac_002_setup_runbook`: 2 passed (at `5074e0e`'s tree; `install.md`'s blocks it reads did
  not change after it). The red phase, the new tests (#165's included) against `92e5fc8`'s documents: 4 failed (the
  three upgrade tests and #165's), 2 passed (the anchor test and the classification test, whose purpose is a later breakage; the
  anchor test turned red on a mutated anchor, the classification test on an indented unmarked
  fence), `C:\tmp\s79-red.txt`.
- No Rust, workflow or schema changed; no elevated gate: no path of *Developer-local SPEC-005
  marquee validation* §5 changed.
- CI on PR #145: `5074e0e`, `5453445`, `2d6edec`, `59a4155`, `c426b4c` and `1a04d1b` each
  markdown-lint, deploy-ci, rust-ci success, ALL GREEN (ts-ci and package-ci not triggered).
  `1a04d1b` on `main` (push), and this close's commit: as their post-push reports give them.

## Test baselines at close

Non-elevated `cargo test --all`: 99 binaries, 617 passed, 11 ignored (unchanged: no Rust changed).
Elevated `--ignored` 7 and vitest 86 files, 504 passed, 2 skipped, unchanged from Session 72.
`upgrade.test.mjs`: 4 static tests passed, 1 skipped (stack); `operations.test.mjs`'s link test
passed.

## Debts

- Closed: none.
- Opened:
  - **#167**: `install.md` states as known two outcomes no test or reading has shown: the retries
    after 40 to 45 on an installation ("run the setup again") rerun the registered setup without
    `-repair`, whose run has not been read; and the setup's 27 "when the agent never enrolled
    there" holds only while the hand-made directory keeps its inherited DACL (one an administrator
    tightened is a pending host). Its uninstallation also runs the setup elevated from `$dir` after
    a hash check alone, where `upgrade.md` now checks the directory first. Destination: C10 commit
    4's pass over `install.md`, and install_ac_016's run (M3).
- Changed: **#165**'s destination: C10 commit 3's second half, *Change a key on an endpoint*,
  staged as *#165: written, not landed* says, after `C:\tmp\s79-read1.ps1`'s elevated reading.
- Open at close: **60**.

## Waiting on Manuel

- **The #165 reading**: `powershell -NoProfile -ExecutionPolicy Bypass -File C:\tmp\s79-read1.ps1`
  in an elevated Windows PowerShell; it touches only `C:\tmp\s79-probe`, which it removes, and
  writes `C:\tmp\s79-read1.log`.
- **M1** (#160's text), **M2** (#162's order), **M3** (the isolated machine: install_ac_016 runs
  the endpoint steps there, the upgrade's blocks now too), **M4** (R1 and #87, #164), **M5**
  (deployment tools), **M6** (re-enrollment: `upgrade.md` and `install.md` now say it is not in the
  runbooks; the purge and a new setup with a new token is the route point 8 of the Amendment
  2026-11-15 describes), as Session 76 put them.
- **The repairs' elevated reading** for commit 4 (#87, #166), as Session 78 put it.
- **CLAUDE.md's local gate**: Session 77's proposal stands.
- The release freeze stays; Authenticode stays ADR-0022's Open question 1.

## How Session 80 resumes

1. Run the start protocol (CLAUDE.md). `main` is this handoff's close commit. There is no PR. The
   counts are 60 open debts and 22 ADRs / 23 SPECs.
2. If `C:\tmp\s79-read1.log` exists (UTF-16), read it: the probe's file ID, owner, DACL and link
   count equal before and after the write-back, and its first bytes not `ef bb bf` after both the
   BOM and the UTF-16 copies. If it holds, land #165 on a branch and PR: append
   `C:\tmp\s79-165-config.md` to `configuration.md`'s *The agent's tuning* with the log's result in
   place of `READING-PENDING`; insert `C:\tmp\s79-165-test.mjs` before the classification test of
   `upgrade.test.mjs`, with `const configuration = readFileSync(join(REPO,
   "docs/operations/configuration.md"), "utf8").replaceAll("\r", "");` after `install`'s line; see
   it red against this close's `configuration.md`; the fixed-prompt review; close #165. If the log
   shows another owner, DACL or file ID, or a BOM, stop: the route needs another command, read again.
   If the files under `C:\tmp` are gone, rewrite them from this handoff's *#165* section.
3. Manuel's answers on M1 to M6 and the repairs' reading, if given; then commit 4 (the migration's
   runbook, the repairs, #87, #166, #167) and commit 5 (#160 after M1, #150). Keep the release
   freeze.

## Environment and memory

- Docker Desktop stayed down; nothing of Manuel's was touched.
- `C:\tmp` holds `s79-facts-*.md` (the readers' fact map of the upgrade and #165),
  `s79-review1-*.md` to `s79-review3-*.md` (the reviews), `s79-red.txt` (the red phase),
  `s79-read1.ps1` (the #165 reading), `s79-165-config.md` and `s79-165-test.mjs` (#165's staged
  text and test), `s79-ci.sh` (the CI poll), the commit messages `s79-msg*.txt`.
- `target\` holds debug builds without debug info (`CARGO_PROFILE_*_DEBUG=0`).
- Updated memory: the session's close.
