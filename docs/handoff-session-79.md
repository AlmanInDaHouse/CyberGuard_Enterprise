# Handoff — Session 79

Session 79, 2026-11-17, landed C10's third commit: `docs/operations/upgrade.md` upgrades the agent
on an endpoint with a newer setup, the console-era agent text goes, and re-enrollment is named as
not in the runbooks (`1a04d1b`, PR #145). After Manuel's elevated reading of its command, it also
landed #165, `configuration.md`'s in-place change of a key in `agent.toml` (`711f8cb`, PR #146),
which closes the debt. Commits 4 and 5 of
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
  - `b2835be`, the first close;
  - `711f8cb`, #165, fast-forwarded from PR #146;
  - the second close's commit, which completes this handoff.
- The branches `docs/c10-upgrade-agents` and `docs/c10-165-agent-key` are deleted, local and
  remote. No other branch is left.
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

## #165: landed (`711f8cb`)

The read-only workflow's critic and the `custody-165` reader found the in-place edit already in the
contract: SPEC-024 §Data contracts §2 keeps elevated maintenance in scope (lines 106-107), and the
Amendment 2026-11-04's *Effect*, which Manuel approved, has the runbook tell a rewrite of
`agent.toml` from its replacement and adds no setting (lines 1029-1033); the threat model's line 399
says administrators rewrite it. No SPEC is needed. The command waited on a reading, which Manuel ran
elevated after the first close (`C:\tmp\s79-read1.ps1`; its output pasted in chat and in
`C:\tmp\s79-read1.log`). On Windows PowerShell 5.1.26100 Desktop, Windows 10.0.26200, git HEAD
`b2835be`, on a probe file owned by SYSTEM with `D:PAI(A;;FA;;;SY)(A;;FA;;;BA)`:

```text
before sddl: O:SYD:PAI(A;;FA;;;SY)(A;;FA;;;BA)
before fileid: El identificador de archivo es 0x0000000000000000000300000022cabe
edited copy (BOM on purpose) first bytes: ef bb bf length 117
after sddl: O:SYD:PAI(A;;FA;;;SY)(A;;FA;;;BA)
after fileid: El identificador de archivo es 0x0000000000000000000300000022cabe
after first bytes: 5b 73 65 length 114
edited copy (UTF-16) first bytes: ff fe 5b length 230
after UTF-16 sddl: O:SYD:PAI(A;;FA;;;SY)(A;;FA;;;BA)
after UTF-16 fileid: El identificador de archivo es 0x0000000000000000000300000022cabe
after UTF-16 first bytes: 5b 73 65 length 114
agent2 before Copy-Item fileid: El identificador de archivo es 0x000000000000000000450000001bcd17
agent2 after Copy-Item fileid: El identificador de archivo es 0x000000000000000000450000001bcd17
```

Each `links:` line read 1. The script's label "Copy-Item over the file replaces it" is false by its
own log: `Copy-Item -Force` over the file kept its file ID, owner and DACL (that file already held
the same UTF-16 bytes, so what it wrote is not shown). The script's `takeown /D Y` failed on the
Spanish locale (it takes `S`); the probe was removed all the same (`probe removed: True`).

The section *Change a key on an endpoint* (`docs/operations/configuration.md`, under *The agent's
tuning*):

- checks the inputs' directory with `upgrade.md`'s block;
- stops the service, copies `agent.toml` to `$dir` and keeps the copy's hash;
- has the copy edited with an elevated editor that keeps no copy in the profile;
- writes the copy's text with `WriteAllText`, only while the service is stopped and `agent.toml`
  still has that hash, inside a `try` that starts the service only after a write that did not
  fail, and prints `written` or `not written`;
- deletes the copy.

It marks what comes from the reading, the code or neither. It names as not in the runbooks the
recovery of a custody refusal and of an `agent.toml` changed since the copy. `upgrade.test.mjs`
pins the four blocks.

## Decisions

### Manuel's

- None this session. M1 to M6, the repairs' reading and the CLAUDE.md proposal: no answer. Manuel
  ran the #165 reading after the first close and pasted its output, with no other message.

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
- #165, before its review: the staged text claimed "whatever encoding" (only UTF-8 with a BOM and
  UTF-16 with one were read, with ASCII text) and that `Restart-Service` reports a refused
  configuration (not read). Both were corrected before `520f25b`.
- #165's first review, at `520f25b`, blocking:
  - the copy, which may hold the token, went into an unchecked `$dir`;
  - the agent could enroll between the copy and the write-back, which then wrote the used token
    back for good (an enrolled agent only logs a stale token);
  - the keys were said to stay through an upgrade without `upgrade.md`'s "enrolled" condition.
  Fixed in `84ca95a`: the directory block, `Stop-Service` first, the editor's elevation, the token's
  removal as best effort.
- #165's second review, at `84ca95a`, blocking:
  - a code 2 also comes after RUNNING (a token removed before enrollment);
  - a service started again during the edit (1067, a failed stop) let the write go on, while
    `Start-Service` on it did nothing and `sc.exe` showed `4 RUNNING`;
  - a custody refusal made "correct the copy" loop;
  - "copies the editor's bytes" and a named editor went beyond the reading.
  Fixed in `8389d4c` with a guard on the service's state.
- #165's third review, at `8389d4c`, blocking:
  - a .NET exception inside the guarded block left `Start-Service` to run (a reviewer showed it on
    PowerShell 5.1 here), and a skipped write looked like a written one;
  - a state check alone let a start that enrolled and stopped again write the stale copy back.
  Fixed in `29ec192`: the copy's hash in the guard, the `try`, the printed outcome.
- #165's fourth review, at `29ec192`, blocking: re-baselining the hash after a write error could
  take the agent's rewrite as the operator's. Fixed in `4568ff3`: a failed write gets the block run
  again, and a changed file's recovery is named as not in the runbooks. Also `$before -and`, and
  the 1067 restart's cancellation marked not read.
- #165's fifth review, at `4568ff3`: the operations lens found no blocking point. The fixed-prompt
  reviewer found two of wording, fixed in the squashed commit's tree: "writes nothing" after an
  error that may follow the truncation, and "writes only while" for a check made just before the
  write.
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
| #165: a route sets the keys on a service host | `upgrade.test.mjs`, the #165 test (four blocks, in order) | covered (seen red on `b2835be`'s `configuration.md`, the section missing) |
| #165: a rewrite in place, never a replacement | same: each block equal to its text, the directory block to `upgrade.md`'s, no other write into `%ProgramData%` | covered for the commands; what `WriteAllText` keeps rests on the reading, outside CI |
| #165: the token's copy kept as SYSTEM and Administrators keep `agent.toml` | same (the directory block) | weak: the printed SDDL is text, as in `upgrade.md` |

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
- #165 (five passes, each the fixed code prompt, the fixed text prompt or both, and an evidence or
  operations lens): the blocking points **fixed** (*Reactive corrections*). Not blocking, and
  recorded:
  - The evidence for `WriteAllText` is a reading outside CI, on a probe under `C:\tmp`. The guarded
    block as landed has run only unelevated, on scratch files, with the service replaced:
    install_ac_016's run on the isolated machine (M3) covers it.
  - The check comes before the write, with an instant's window for an SCM restart: stated in the
    text.
  - Windows 11's Notepad keeps tab state in the profile, a reviewers' claim, not read: the text asks
    for an editor that keeps no copy, names none, and the token's copy is listed below for Manuel.
  - An ANSI copy with non-ASCII text, or UTF-16 without a BOM, is not refused: the text asks for
    UTF-8 or UTF-16.
  - The ban regex in the test cannot see every replacement verb: each block is pinned exactly.
  - The section lets any key change, not only the tuning keys: it says what a changed `[server]` or
    `[enrollment]` key can do.
  - The test lives in `upgrade.test.mjs`, as the plan put it.

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
- #165, local, at each fix's tree: `node --test` over `upgrade.test.mjs` and `operations.test.mjs`:
  7 passed, 3 skipped (stack), 0 failed; markdownlint "Summary: 0 issues in 0 files"; the write
  block parsed by PowerShell 5.1's parser with 0 errors, and its shape run unelevated on scratch
  files (a missing copy: `not written`, no start; a changed file: `not written`). Red phase:
  the #165 test against `b2835be`'s `configuration.md` failed on the missing section
  (`C:\tmp\s79-165-red.txt`).
- CI on PR #146: `520f25b`, `84ca95a`, `8389d4c`, `29ec192`, `4568ff3` and the squashed `711f8cb`
  each markdown-lint and deploy-ci success, ALL GREEN. `711f8cb`'s tree is `4568ff3`'s with the
  fifth pass's wording (`git diff --quiet` against the local tip that carried it).

## Test baselines at close

Non-elevated `cargo test --all`: 99 binaries, 617 passed, 11 ignored (unchanged: no Rust changed).
Elevated `--ignored` 7 and vitest 86 files, 504 passed, 2 skipped, unchanged from Session 72.
`upgrade.test.mjs`: 5 static tests passed (#165's included), 1 skipped (stack);
`operations.test.mjs`'s link test passed.

## Debts

- Closed: **#165**, by `711f8cb` (*#165: landed*).
- Opened:
  - **#167**: `install.md` states as known two outcomes no test or reading has shown: the retries
    after 40 to 45 on an installation ("run the setup again") rerun the registered setup without
    `-repair`, whose run has not been read; and the setup's 27 "when the agent never enrolled
    there" holds only while the hand-made directory keeps its inherited DACL (one an administrator
    tightened is a pending host). Its uninstallation also runs the setup elevated from `$dir` after
    a hash check alone, where `upgrade.md` now checks the directory first. Destination: C10 commit
    4's pass over `install.md`, and install_ac_016's run (M3).
- Open at close: **59**.

## Waiting on Manuel

- **The token's resting places (threat model, yours).** On an endpoint that never enrolled, #165's
  route puts the token in places `docs/security/threat-model.md:399-400` does not list: the copy in
  `$dir` (SYSTEM and Administrators, as the bootstrap file there), the elevated PowerShell's memory,
  and any copy an editor keeps. The runbook discloses them. Whether the threat model records them,
  or the route is held back until after enrollment, is yours.
- **M1** (#160's text), **M2** (#162's order), **M3** (the isolated machine: install_ac_016 runs
  the endpoint steps there, the upgrade's blocks now too), **M4** (R1 and #87, #164), **M5**
  (deployment tools), **M6** (re-enrollment: `upgrade.md` and `install.md` now say it is not in the
  runbooks; the purge and a new setup with a new token is the route point 8 of the Amendment
  2026-11-15 describes), as Session 76 put them.
- **The repairs' elevated reading** for commit 4 (#87, #166), as Session 78 put it.
- **CLAUDE.md's local gate**: Session 77's proposal stands.
- The release freeze stays; Authenticode stays ADR-0022's Open question 1.

## How Session 80 resumes

1. Run the start protocol (CLAUDE.md). `main` is this handoff's second close commit. There is no
   PR. The counts are 59 open debts and 22 ADRs / 23 SPECs.
2. Manuel's answers on M1 to M6 and the repairs' reading, if given; then commit 4 (the migration's
   runbook, the repairs, #87, #166, #167) and commit 5 (#160 after M1, #150). Keep the release
   freeze.

## Environment and memory

- Docker Desktop stayed down; nothing of Manuel's was touched.
- `C:\tmp` holds `s79-facts-*.md` (the readers' fact map of the upgrade and #165),
  `s79-review1-*.md` to `s79-review3-*.md` (the reviews), `s79-red.txt` (the red phase),
  `s79-read1.ps1` and `s79-read1.log` (the #165 reading), `s79-165-config.md` and
  `s79-165-test.mjs` (#165's text and test as landed), `s79-165-red.txt` (its red phase),
  `s79-ci.sh` (the CI poll), the commit messages `s79-msg*.txt`.
- `target\` holds debug builds without debug info (`CARGO_PROFILE_*_DEBUG=0`).
- Updated memory: the session's close.
