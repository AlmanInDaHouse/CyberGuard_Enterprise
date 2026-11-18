# Handoff — Session 81

Session 81, 2026-11-18, wrote C10's fourth commit, the migration's runbook and the repairs of the
agent's directories, from Session 80's design and Manuel's answers Q1 to Q6; drafted, on an
owner-review branch, SPEC-024 Amendment 2026-11-18 (Q4) and the threat model's Amendment
2026-11-18 (M4); and had Manuel run reading 4 elevated. Two rounds of fresh-context review found,
in the first, a race between a listing and the command that acts on a name, and in the second,
command injection through a planted name an operator pastes into a block. The repair primitives
need a redesign that does not fit this session with quality (`feedback-checkpoint-over-rushing`):
nothing of C10 commit 4 or the amendments landed. Both branches are pushed, with no PR. No tag was
pushed: the release freeze stays.

## Start and resulting state

- Start: `main` = `origin/main` = `f80a221`, clean, no stash; `docs/c10-migration` (`29d233f`, on
  `241d14e`, pushed, no PR) the one other branch, held by the worktree `C:\Users\manul\cg-s80-c4`; no
  PR; the tip's CI ALL GREEN (markdown-lint); 22 ADRs / 23 SPECs, 58 open debts (58 rows), no Known
  CI debt. The repo agreed with Session 80's handoff. 4.0 GB of 15.8 GB of memory free, 22 GB on
  `C:`; Docker Desktop down (Manuel's `aura_*` containers not touched).
- Resulting `main`: this close's commit, on `f80a221`.
- Branches, both pushed with no PR, so that no workflow ran on them:
  - `docs/c10-migration` at `1cf1b12`, rebased on `f80a221`: `51ba811` and `2c9f74e`, #167's two
    commits of Session 80 rebased, then `1cf1b12`, C10 commit 4 (*Migrate a console installation*
    and *Repair the agent's directories* in `install.md`, `upgrade.md`'s *Agents*, the tests). Its
    worktree is still `C:\Users\manul\cg-s80-c4`.
  - `docs/spec-024-amendment-2026-11-18` at `da98449`, on `f80a221`: `87dfae6`, SPEC-024
    Amendment 2026-11-18 with its catalog line, and `da98449`, the threat model's Amendment
    2026-11-18 with the three lines of the Amendment 2026-10-15 it updates in place; both subjects
    `NOT YET RATIFIED`. Not yet shown to Manuel for his OK: the second review found list-A points in
    both (below).
- Catalogs unchanged on `main`: 22 ADRs / 23 SPECs.

## What was written (on the branches)

### C10 commit 4 (`1cf1b12`)

- *Migrate a console installation*: item 7's disclosures; the steps before (a restart and another
  administrator's session, the variables block, the listings from the parent down, the owners, no
  agent running, the inputs' directory, the fingerprints from the change record, the choice); the
  setup's command with `CG_MIGRATE_LEGACY` and `CG_TRUST_ANCHORS`; its result by the setup's code
  (0, 24, 26 to 28, 42 to 44) and by the service's line, each repair constant of `migration.rs`
  quoted; the retry (`-repair` after 42 or 43, not read, #167's mark); the evidence line; *Enroll
  anew instead* (the console's identity files taken and copied aside, `<P>` removed, a new token);
  the console executable's removal.
- *Repair the agent's directories*: rules (no recursion, no `takeown`/`icacls` on `identity`, each
  directory closed to SYSTEM and Administrators, checked from its parent's listing and only then
  listed with each name between brackets, names ending with a dot or a space or holding `%` not
  acted on, every removal checked by `Test-Path`); the find block; the closing blocks; a foreign
  file's owner; files, file links, junctions and directory links, their read-only variants; a file
  with another link; a directory holding nothing of the agent's, removed from the top down; the
  case-sensitive lookup; what the purge left.
- The rows and sentences that said the migration and the repairs were "not in this document yet"
  (`install.md` `:7`, `:336`, `:467`, `:468`, `:483`, `:631`, `:734`, `:762`, `:763`, `:766`,
  `:767`) point to them; `upgrade.md`'s *Agents* migrates, then enrolls again by the purge, or
  enrolls anew.
- Tests: `install.test.mjs` reads indented endpoint blocks, places each under its H2 section,
  pins the migration's command, the safety rules, the closing sequence and the bracketed listing,
  the repair constants against `migration.rs`, the disclosures and the routes of the amendment;
  `upgrade.test.mjs` pins the new route. Red: the new tests on the text before them, 4 failed
  (`C:\tmp\s81-c4-red.txt`); 25 mutants of the new text, each killed
  (`C:\tmp\s81-c4-mutants.txt`, harness `C:\tmp\s81-mutants.mjs`).

### SPEC-024 Amendment 2026-11-18 (`87dfae6`)

Point 1 (Q4, Manuel's choice of 2026-11-17): a suspected compromise takes item 6's route without
a migration; any other console installation, another CA's anchor or an expired identity included,
is migrated first. Points 2 to 5, Claude Code's proposals for Manuel's approval: no recorded
fingerprints, a refusal a restart and a retry do not clear (code 1, code 5 for a file held or not
the one checked or above the bound), and a file with another name outside `<P>` take item 6's
route; "the old agent removed on the server" reads as the agent left there until its certificate
expires or an operator deletes its row (R5).

### The threat model's Amendment 2026-11-18 (`da98449`)

R1 closed, reopening if install_ac_016's run fails, its line at `:316` and the lines `:292` and
`:312` updated in place (Q1); the Amendment 2026-11-05's "removal of the old agent on the server"
read with the SPEC's point 5; R17's citation and remediation corrected (#164); §1 Elevation of
Privilege, the repairs' commands; §1 Information Disclosure, the token in an edited configuration
(#165); Logging, the Application log's text kept after an uninstallation (#168); R17's addition;
accepted threats: the in-place repair on an enrolled host (Q2), the key kept aside (Q3), what the
runbook does not repair (Q5).

## Reading 4 (Manuel, 2026-11-18, 21:32)

`C:\tmp\s81-read4.ps1` (SHA-256 `669B3CC8EF8BA522A61399B3551374D914F551F9C9E446964D25698C4369CFB5`),
elevated, Windows 10.0.26200, PowerShell 5.1.26100.9818, es-ES, git HEAD `d7dda8b`; log
`C:\tmp\s81-read4.log` (UTF-16) and `.utf8.log`; probes and canaries removed at its end.

- G1, G2: a directory another account planted, holding a foreign file and a foreign junction (G2 a
  foreign subdirectory too), taken (`takeown /A`, `icacls /grant BA:(OI)(CI)F`), its entries removed
  one by one, then `cmd /c rmdir "$d"` exit 0, gone; canaries intact.
- G3: a directory symbolic link with a foreign DACL: `rmdir` refused (5); `takeown /A`,
  `icacls /grant BA:F /L`, `rmdir` exit 0, gone, target intact.
- G4: a junction that admits Administrators: `rmdir` exit 0, gone.
- G5: the in-place DACL repair on `O:NS D:P(NS)(BU read)`: the SID print by `GetAccessRules(...,
  SecurityIdentifier)` names `S-1-5-20` and `S-1-5-32-545` in full; result `O:SY
  D:PAI(BA)(SY)`; `identity`, `key.dat`, `agent.toml` unchanged; `logs` (explicit SY and BA,
  unprotected) became `D:PAI` with its entries; `/remove:g` of an absent SID exit 0, no change.
- G6: `Copy-Item` of a file whose DACL admits no administrator: "Acceso denegado", the loop goes
  on, exit 0; source unchanged; `CreateDirectory` on an existing directory prints its foreign owner.
- G7: `fsutil hardlink list` prints both names, exit 0, when the other directory admits
  Administrators.
- G8: a foreign junction named `CyberGuard` lists as one row `Directory, ReparsePoint`.
- G9: listing a directory that admits no administrator: "Acceso denegado a la ruta de acceso ...";
  `(Get-Acl).Owner` still prints.
- G10: read-only planted entries resist `Delete` and `rmdir`, even taken, until the attribute is
  cleared: a file (`takeown`, `icacls /grant BA:F`, `SetAttributes Normal`), a file link (`/L`,
  `attrib -r /L`), a directory (`SetAttributes Directory`), a junction (`attrib -r /L`); targets
  and their attributes unchanged.
- G11: the console agent's identity files (`O:BA D:P(SY)(NS)`) are not copied by another
  administrator; after `takeown /A` and `icacls /grant BA:R` the five copy, byte-identical, with
  the aside directory's protection.
- G12: a file another account owns, with a second name: the SID print, the DACL, both names, the
  other name deleted, one name left.

Reading 5 (`C:\tmp\s81-read5.ps1`), the closing blocks with a planted `Deny` entry and
`/remove:d`, a junction swapped in before the closing, and the bracketed listing with `key.dat.`
and a `key.dat` decoy ending with a space, was written and handed to Manuel; it had not run at the close. Session 82
reads its log if it exists, and rewrites it for the redesigned primitives.

## The reviews

Fixed prompts (CLAUDE.md compensating control 5) with three lenses, twice, each reviewer isolated in
a worktree, given no summary (runs `wf_394db70d-ba0`, `wf_7326d8a1-629`; journals under the
session's `subagents/workflows/`).

### Round 1, on `25686cb` and `d7dda8b`

Fixed in `1cf1b12` and `87dfae6`/`da98449`: the console's identity files not copyable by another
administrator (also Claude Code's own pass; G11); the variables lost at each restart; a junction at
`C:\CyberGuardSetup` sent to a procedure that follows it; *Enroll anew* after a committed migration
(44); no `server-ca.pem` for the new installation; the entry 1002 pointer; the code-6/7 lines after
a commit; a 27 caused by an open DACL undiagnosable (the find block); the 26 on the migration route;
the code-2 repair `server.heartbeat_url`; the purge's 28 contradicting point 1; no-fingerprints
missing from `install.md`; the race between a listing and the command (each directory closed before
it is listed); another link's names in directories another account controls; names a trailing dot
or space normalises; `Deny` entries; R5's "or an operator deletes its row"; R1 still open in the
Amendment 2026-10-15's list; the TM amendment ahead of the runbook (landing order); the `#167` test
whose `indexOf` was vacuous.

### Round 2, on `1cf1b12` and `da98449` — open, for Session 82

Blocking:

1. **Command injection through a planted name** (security lens). Blocks ask the operator to paste a
   name into `$e = '<path>'` or `$l = '<path>'` and run `cmd /c rmdir "$l"`: a name holding `'`
   leaves the PowerShell string (the reviewer parsed `a'+(iex(...))+'`, no error), one holding `&`
   or `^` is read by `cmd` (checked with `cmd /c echo`). Names from `fsutil hardlink list` are
   unbracketed. Homoglyphs and right-to-left marks pass the bracketed listing.
2. **A file's security changed before its other names are known**: the hard-link blocks, the
   read-only take form and `endpoint-console-take` change owner, DACL or attributes of a file whose
   other name may be outside; closing a directory propagates to a planted hard-linked child.
3. **Enroll anew acts in `$data` unclosed** (code, text, security and operations reviewers), and
   loops when `$data` admits no administrator and its owner cannot be confirmed.
4. **"No name outside `<P>` is removed"** (SPEC point 4, TM `:516`, `install.md:1178`) is false
   literally: the console executable, `bootstrap.toml` and `C:\CyberGuardSetup` are removed outside;
   it means "no other name of a hard-linked file".
5. **The TM's #168 correction** clashes with SPEC-024 Amendment 2026-11-15, point 7
   (`SPEC-024:3137-3138`), which keeps "without their message text".
6. **The TM cites point 5** for "without the migration", which is point 1's.
7. **Closing `C:\ProgramData`**: `install.md:1083` can be read as closing the parent when
   `C:\ProgramData\CyberGuard` is a link.
8. **Listings' expected attributes**: on this host children of `C:\ProgramData` list as `Directory,
   NotContentIndexed`; the text says `Directory`.
9. **After the purge route** the console executable's removal prints `False` twice, not `False`
   then `True`.

Not blocking, recorded for the redesign: the code-13 row without no-fingerprints; `rmdir`'s
"(read, one level ... included)" rests on G2's earlier close form; the 43 that commits; entries
1002 at each boot; the dead ends a dot-or-space name and `agent.migrating\identity` leave; the 28
with a bootstrap file; `$dir` reset by the upgrade's first block in step 6; an OWNER RIGHTS `Deny`;
`GetAccessControl` throwing "operación no válida" rather than access denied; the case-sensitive
repair without a close; a console that never enrolled (two files); the committed layout in the
listings; the tests' weak points (point 1 in *Enroll anew*, point 4's route, `upgrade.md`'s
conditions, item 5, the 2026-11-07 (second) disclosures, item 3 of 2026-11-09); posture: point 1
reinstalls on the suspected host (Manuel's choice).

### Claude Code's matrix (at `1cf1b12`)

| Criterion | Test | Verdict |
|---|---|---|
| 2026-11-05 item 1, 2026-11-09 item 2: the setup's migration command | `install.test.mjs`, the C10 test (command options, exactly two variables, no bootstrap or purge) and the `Start-Process` loop | covered |
| Item 2: each repair line quoted as `migration.rs` writes it | the C10 test, constants read from the source | covered |
| Item 6: files kept aside, `<P>` removed, a new token | the aside, take and copy pins | weak: step 3 and 4 not pinned; contradicted by round 2's item 3 |
| Item 7: the disclosures | the C10 test, nine phrases | covered |
| 2026-11-18 points 1 to 5 | phrases in *What the migration does not undo* and *Enroll anew* | weak (round 2) |
| #166: repairs for 27, 13, 46, 47 | links and the absence of the placeholders | weak: new gaps worded otherwise |
| #167: the marks | the #167 test, its `indexOf` fixed | covered |
| Safety rules | 25 mutants killed | covered for what the rules state; round 2 found rules missing |

## Decisions

### Manuel's

None new in this session; Q1 to Q6 of 2026-11-17 were applied (`docs/handoff-session-80.md`).

### Anticipated (Claude Code)

- **The two sections in `install.md`** (Session 80's design): discarded a separate `migration.md`
  with its own marker scheme.
- **Reading 4 extended with G9 to G12 before Manuel ran it** (the read-only entries, `/remove:g` of
  an absent SID, the listing of a directory without access, the SID print, the take before the
  copy): discarded a fifth reading for them.
- **SPEC-024 Amendment 2026-11-18 before the runbook, the threat model's after it** (each commit
  true at its SHA): discarded one owner-review commit landing after the runbook, which would land
  code before its contract.
- **Points 2 to 5 of the SPEC amendment proposed to Manuel** rather than decided: they widen item
  6's route and read item 6 and item 7, on his list.
- **The checkpoint**: discarded a third round of patches on the same primitives, which round 2
  showed to need a redesign.

### Reactive corrections

- The repair table's column read "the line ends with": from the instance lock on, each refusal ends
  with the in-place sentence (`migration.rs` `with_state`); now "the line holds".
- Bash heredocs lost backslashes in edit scripts three times (`feedback-bash-heredoc-backslashes`);
  each failed its exact-match guard before writing, and was rewritten with the Write tool.
- Reordering the owner-review branch, a `git reset --soft` kept the old threat-model text in the
  index, and a `git checkout` overwrote the revised working copy; the revised text was rebuilt from
  `C:\tmp\s81-tm-am.md` and `C:\tmp\s81-fix13.mjs` and checked before `da98449`.
- The mutation harness's first rerun printed the previous round's file (an `&&` chain stopped
  before the run); rerun, three anchors fixed, 25 of 25 killed.

## Gates and CI

- `markdownlint-cli2` 0.23.2 on `install.md`, `upgrade.md`, SPEC-024, `docs/specs/README.md`,
  `threat-model.md`: "Summary: 0 issues in 0 files" on each.
- `node --test --test-concurrency=1` over `install.test.mjs`, `upgrade.test.mjs`,
  `operations.test.mjs` at `1cf1b12`'s tree: 18 tests, 14 passed, 4 skipped (the stack's).
- `cargo test -j 2 -p cg-setup --test install_ac_002_setup_runbook` at `25686cb`'s tree: 2 passed.
  Not rerun at `1cf1b12`; deploy-ci's full local bullet not run: the branch does not land.
- CI: the two branches have no PR, so no workflow ran on `1cf1b12` or `da98449`. `main`: this close's
  commit, as its post-push report gives it.

## Test baselines at close

Unchanged on `main` from Session 80: no code, test or runbook changed there.

## Debts

- Closed: none. Opened: none; round 2's findings are within #87, #166 and #167.
- Destinations changed: #87, #164, #166, #167 and #168 point to Session 82's redesign of C10
  commit 4 on `docs/c10-migration` and the owner-review branch, then their landing.
- Open at close: **58**.

## Waiting on Manuel

- Reading 5 (`C:\tmp\s81-read5.ps1`), if he wishes before Session 82: its closing, `Deny` and
  decoy cases stay useful to the redesign; Session 82 may replace it.
- His OK on SPEC-024 Amendment 2026-11-18 (points 2 to 5 included) and the threat model's, once
  Session 82 has fixed round 2's list-A points and shown him the compare URL.
- The release freeze stays; Authenticode stays ADR-0022's Open question 1; M3 waits.

## How Session 82 resumes

1. Run the start protocol. `main` is this close's commit; `docs/c10-migration` (`1cf1b12`) and
   `docs/spec-024-amendment-2026-11-18` (`da98449`) are pushed with no PR; 58 open debts; 22 ADRs /
   23 SPECs. Read `C:\tmp\s81-read5.log` if Manuel ran it.
2. Redesign the repair primitives so that no operator types, pastes or passes to `cmd` a name an
   ordinary user chose: blocks that enumerate a closed directory with .NET, compare each name
   ordinally with the exact names the product allows, and act on the enumerated objects through
   `\\?\` paths, without `cmd`; `fsutil hardlink list` read first, a file's security changed only
   when every name it has lies in a closed directory; *Enroll anew* closing `<P>` and `$data`
   before it takes or copies anything. A design workflow with independent designs and a judge, as
   Session 80's, then reading 6 for the new primitives.
3. Fix round 2's list (above), the texts' points 4 to 6 included, and run the fixed prompts and the
   security lens again until no blocking point; then the compare URL to Manuel for the two texts.
4. Land in order: `87dfae6` (after his OK), the C10 PR (#167's two commits and commit 4, deploy-ci
   and rust-ci's local bullets run), then `da98449`; close #87, #164, #166, #167 and #168 as they
   settle.

## Environment and memory

- Docker Desktop stayed down; nothing of Manuel's was touched.
- `C:\tmp` holds the readings `s81-read4.ps1` (and its logs) and `s81-read5.ps1`, the red phase
  `s81-c4-red.txt`, the mutants `s81-mutants.mjs` and `s81-c4-mutants.txt`, the texts
  `s81-tm-am.md`, `s81-spec-am.md`, `s81-repair-new.md`, and the edit scripts `s81-fix*.mjs`.
- The worktree `C:\Users\manul\cg-s80-c4` holds `docs/c10-migration`.
- Updated memory: the session's close; a feedback entry on runbooks that make an operator paste
  names another account chose.
