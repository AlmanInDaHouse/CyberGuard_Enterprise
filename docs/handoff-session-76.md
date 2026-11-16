# Handoff — Session 76

Session 76, 2026-11-16, settled debt #163: the custody check read the protected bit as
`GetSecurityInfo` recomputes it, not as the file system stores it. SPEC-024 Amendment 2026-11-16
(second) (`9f2a780`) reads every protected DACL the SPEC requires as stored, and the fix
(`d333442`) reads the owner and DACL with `GetKernelObjectSecurity`. C10, the runbooks, was read and
planned, not written: a read-only map of what the runbooks must say left 67 open questions, six of
them Manuel's, so the work is split into five commits for Session 77 and later (*C10's plan*).
No tag was pushed: the release freeze stays.

## Start and resulting state

- Start: `main` = `origin/main` = `2f703b9`, clean, no stash, no branch but `main`, no PR; the
  tip's CI ALL GREEN (markdown-lint, package-ci); 22 ADRs / 23 SPECs, 57 open debts (57 rows), no
  Known CI debt. The repo agreed with Session 75's handoff. 6.4 GB of 15.8 GB of memory free,
  28.7 GB on `C:`; Docker Desktop down (Manuel's `aura_*` containers not running, not touched).
- Manuel's prompt held no answer on #160 or #162, which stay waiting.
- Resulting `main`, in order, each pushed alone with its CI green:
  - `9f2a780`, SPEC-024 Amendment 2026-11-16 (second), straight to `main` (docs);
  - `d333442`, the custody fix, fast-forwarded from PR #142;
  - this close's commit.
- PR #142 (`fix/163-stored-protection`) shows merged: GitHub closed it when `main` reached its head.
  The branch is deleted. No other branch is left.
- Catalogs: 22 ADRs / 23 SPECs, unchanged; `docs/specs/README.md` gains the 2026-11-16 second line.

## #163: the reading

Read on this host, unelevated (Windows 11 26200), with a throwaway C# probe through PowerShell's
`Add-Type` in the session's scratchpad: each object read stored (`GetKernelObjectSecurity` through a
`READ_CONTROL` handle) and as MARTA reports it (`GetSecurityInfo` through the same kind of handle,
`GetNamedSecurityInfoW`). `ME` is the user's SID; control `0x8004` is self-relative with a DACL,
`0x9004` adds `SE_DACL_PROTECTED`, `0x9404` adds `SE_DACL_AUTO_INHERITED`.

Parent `D:P(A;OICI;FA;;;ME)`, children with explicit DACLs (first run):

| Child, SDDL at creation | Stored | `GetSecurityInfo` |
|---|---|---|
| `D:(A;OICI;FA;;;ME)` | `0x8004 D:(A;OICI;FA;;;ME)` | `0x8004 D:(A;OICIID;FA;;;ME)` |
| `D:P(A;OICI;FA;;;ME)` | `0x9004 D:P(...)` | `0x9004 D:P(...)` |
| `D:AI(A;OICI;FA;;;ME)` | `0x8004 D:(...)` (`AI` dropped) | `0x8004 D:(A;OICIID;...)` |
| none (inherits) | `0x8004 D:(A;OICI;FA;;;ME)` | `0x8004 D:(A;OICIID;...)` |

Then the parent's DACL set through `SetNamedSecurityInfoW` with an inheritable `(A;OICI;FR;;;WD)`
added: every child but the `P` one became `0x8404 D:AI(A;OICIID;FA;;;ME)(A;OICIID;FR;;;WD)`; then
`icacls <parent> /grant *S-1-5-32-545:(OI)(CI)(RX)` added `BU` to the same children.

Parent `D:P(A;;FA;;;ME)(A;OICIIO;FA;;;ME)(A;OICI;FA;;;SY)` (second run):

| Child, SDDL at creation | Stored | `GetSecurityInfo` |
|---|---|---|
| `D:(A;OICI;FA;;;ME)` | `0x8004 D:(A;OICI;FA;;;ME)` | `0x9004 PROTECTED D:P(A;OICI;FA;;;ME)` |
| `D:(A;OICI;FA;;;ME)(A;OICI;FA;;;SY)` | `0x8004 D:(...)(...)` | `0x8004 D:(A;OICIID;...)(A;OICIID;...)` |
| file `D:(A;;FA;;;ME)` | `0x8004 D:(A;;FA;;;ME)` | `0x9004 PROTECTED D:P(A;;FA;;;ME)` |
| file `D:(A;;FA;;;ME)(A;;FA;;;SY)` | `0x8004` | `0x8004 D:(A;ID;...)(A;ID;...)` |

After the parent gained `(A;OICI;FR;;;WD)` through `SetNamedSecurityInfoW`, the first and third
children were stored `0x9404 D:PAI(...)` (MARTA wrote the protection it had reported), the second
and fourth `D:AI(...ID...)(A;OICIID;FR;;;WD)`; `icacls /grant` then reached only the latter.
Third run: a file stored `0x8004 D:(A;;FA;;;ME)` became `0x9404 D:PAI(A;;FA;;;ME)` after `icacls
<file> /inheritance:r`.

So `GetSecurityInfo`, even through a handle, recomputes a DACL not stored auto-inherited against
the parent's current DACL: protected when one of the parent's inheritable entries is missing,
inherited and unprotected when all are there. The check passed such an object as protected, and
its verdict depended on the parent. A fresh reviewer reproduced the first and third kinds with
`Get-Acl` and `icacls`.

## What landed

### SPEC-024 Amendment 2026-11-16 (second) (`9f2a780`)

Wherever SPEC-024 requires a protected DACL, it is the DACL as stored, read through the checked
handle; every owner and allow-entry check reads the same stored descriptor, and the hygiene
replacement copies it. No policy changes; what the product writes is created or set with `P`, so
none of it changes verdict; an object an administrator gave an unprotected explicit DACL is now
refused, and `icacls <path> /inheritance:r` stores it protected. Taken by Claude Code, as #163's
destination asked, before the fix.

### The custody fix (`d333442`)

- `agent/cg-agent/src/custody.rs`: `security_of` reads with `GetKernelObjectSecurity` into a
  buffer the `Descriptor` owns (`stored`), growing on `ERROR_INSUFFICIENT_BUFFER`; `Descriptor`
  frees with `LocalFree` only what SDDL built; `GetSecurityInfo` and `SE_FILE_OBJECT` are gone
  from the product.
- `agent/cg-agent/tests/install_ac_004_custody.rs`: `install_ac_004_a_dacl_stored_unprotected_is_refused`
  (premises: stored without `P`, reported with `P`; the directory and the file refused by `check`,
  the file by `read_checked` and `replace_keeping_security`, unchanged, no temporary file; a
  directory with `P` under the same parent passes); the replacement test reads its copy stored
  protected; a comment that said a DACL given at creation is protected corrected.
- `agent/cg-agent/tests/custody_fixtures/mod.rs`: `stored_dacl`, `GetFileSecurityW`.
- Two test comments with the same false premise (`install_ac_004_custody_service.rs`,
  `scm/mod.rs`).

## Decisions

### Manuel's

- None this session. #160 and #162: no answer; both stay waiting.

### Anticipated (Claude Code)

- **Read the whole descriptor as stored** (`security_of` for every caller), not only the control
  bit in the protected branch. Discarded: a second read of the control beside `GetSecurityInfo`'s,
  two reads of one object where #146 asked for one, and a copy that would carry MARTA's view.
- **A buffer the descriptor owns**, not `LocalAlloc`. Discarded: enabling `Win32_System_Memory`
  for one call.
- **The premise asserted in the test** (`GetNamedSecurityInfoW` reports `P`), so that the test
  shows the case the old read passed; it reads Windows, not the product, and fails loudly if
  Windows changes. Discarded: a test that only creates an unprotected object, which would also
  pass against the old code when the parent's entries match.
- **No propagation test.** Whether a parent's change reaches such an object is Windows' behaviour,
  read above; the product now refuses the object whatever the parent does.
- **C10 not started.** The map's 67 open questions, six of them Manuel's, did not fit a quality
  rewrite in this session (`feedback-checkpoint-over-rushing`).

### Reactive corrections

- **The amendment after the fix.** Both fresh reviews of the code found the fix narrowed a check
  without the amendment #163's destination asks for; the fix had been pushed as conformance to "a
  protected DACL". The amendment was written, reviewed twice, landed first, and the branch rebased
  onto it.
- The first review: the comments said "none of the parent's inheritable entries" where the reading
  is "one of them is missing"; fixed. The replacement's stored protection asserted.
- The text reviews: the list of protected objects made open-ended (`migration.json`, the setup's
  record), the owner and entries read as stored for every policy, the replacement named, masks as
  well as accounts, a directory's outcome as its check gives it, the handoff named as the record.
- The heredoc backslash defect once more (a node script with `identity\`); written with Write.
- Session 55's memory that "an explicit DACL at creation comes out protected" was MARTA's view;
  `reference-windows-acl-facts.md` corrected.

## Self-review (CLAUDE.md compensating control 5)

### Claude Code's matrix

Contract: debt #163; SPEC-024 Amendment 2026-11-04, item 3; Amendment 2026-11-16 (second).

| Criterion | Test | Verdict |
|---|---|---|
| A DACL stored unprotected, reported protected, is refused (directory, file) | `install_ac_004_custody.rs` `install_ac_004_a_dacl_stored_unprotected_is_refused` | covered (seen red: "the directory: ()") |
| The read and the replacement refuse it, nothing written | same | covered for the file; not seen red on their own (the run stops at the directory) |
| A stored-protected DACL passes under the same parent | same | covered for a directory |
| The replacement copies a stored-protected DACL | `install_ac_005_the_replacement_keeps_owner_and_dacl` | covered (passes on the old code too: a guard) |
| What the product writes keeps its verdict | the suites (615 local), rust-ci's SCM and `--ignored` steps, package-ci's host, MSI and bundle jobs | covered by the green runs |
| The real policies (SYSTEM) with an unprotected object | none: unelevated tests cannot make SYSTEM the owner | weak; the protected branch does not depend on the policy |
| The read's resize loop | none | weak, recorded |

### The fresh reviewers (the fixed prompts, isolated worktrees)

- Code at `d0971c8`: no contradiction; 2a, the narrowing without an amendment, **fixed** (the
  amendment); 3a, the comment's condition, **fixed**; the replacement's stored check **added**.
- Code at `2102108`: no contradiction; the same 2a point (**fixed**); weak, recorded: the real
  policies, the read's own red, the propagation question.
- SPEC text at `5040618`: list A empty; B1 to B4 **taken**, B5 recorded, B6 (Manuel's line, below),
  B7 the order (**taken**: amendment first, branch rebased).
- SPEC text at `6577c0a`: list A empty; B1, two test comments, **fixed** in the rebased commit; B2,
  B4, B5, B7, B8 **taken**; B3 recorded; B6, the decision's line, is in *Waiting on Manuel*.

## Gates and CI

- Local gate at `d0971c8`: `cargo fmt --all -- --check` clean; `cargo clippy --all-targets
  --all-features -- -D warnings` "Finished"; `cargo test --all` 98 binaries, 615 passed, 0 failed,
  11 ignored; `node .github/scripts/agent-version-matrix.mjs` "agent version matrix: all steps
  passed". At the rebased tree: fmt clean, clippy "Finished", `cargo test --all` 98 binaries, 615
  passed, 0 failed, 11 ignored. markdownlint on the amendment "Summary: 0 issues in 0 files".
- No elevated gate: `custody.rs` is not among the paths of *Developer-local SPEC-005 marquee
  validation* §5.
- CI on PR #142: `d0971c8` ts-ci, deploy-ci, rust-ci, package-ci success, ALL GREEN; `2102108`
  the same four, ALL GREEN; `d333442` ts-ci, deploy-ci, rust-ci, package-ci success, ALL GREEN. `9f2a780` on `main`: markdown-lint success, ALL
  GREEN. `d333442` on `main`: ts-ci, rust-ci, package-ci success, ALL GREEN.
- This close's commit: as its post-push report gives it.

## Test baselines at close

Non-elevated `cargo test --all`: 98 binaries, 615 passed, 11 ignored. Elevated `--ignored` 7 and
vitest 86 files, 504 passed, 2 skipped, unchanged from Session 72.

## Debts

- Closed: **#163**, by `d333442` after SPEC-024 Amendment 2026-11-16 (second).
- Opened: none. Narrowed: none.
- Open at close: **56**.

## C10's plan

A read-only workflow (six readers and a critic over SPEC-024, the runbooks and their tests, the
debts and Sessions 69 to 75) mapped what the runbooks must say. Its output stays outside the
repository, at `C:\tmp\s76-c10-journal.json` (a reading aid, not a record; regenerate it if it is
gone or `main` moved). What it found that shapes the work:

- `docs/operations/install.md`, `upgrade.md` and the documents' list are test inputs
  (`deploy/docker/test/install.test.mjs`, `operations.test.mjs`, `upgrade.test.mjs`; deploy-ci runs
  on `docs/operations/**`), and `install.md`'s blocks carry `deploy_ac_021` markers: every C10
  commit goes through a branch and a PR, with the tests it changes.
- Once `install.md`'s endpoint steps are the setup, they run on Manuel's isolated Windows machine
  with install_ac_016, not in the elevated gate (SPEC-024 Amendment 2026-11-01, point 4, which
  amends SPEC-023 deploy_ac_021 by scope).
- Facts to state exactly as the SPEC marks them: "not read" outcomes stay marked; 1602 (not 1223)
  for a cancellation or a declined elevation; a cancellation during preflight runs no reversal
  (the Amendment 2026-11-13); the failure-actions flag reads `FALSE`; the failure page names four
  steps (`Preflight`, `Agent`, `Purge`, `Configure`); three `cg-setup.exe` payloads.

The commits, each on its own branch and PR, one commit per push:

1. **`install.md`, the endpoint on the setup**: the release's Windows assets and their
   verification, the installed binary's digest against the raw executable's `SHA256SUMS` line
   (SPEC-024:2808-2809), prerequisites and floor, the bootstrap file and the anchor in a local
   directory only SYSTEM and Administrators reach, protected before the token (#150), the
   unattended command (`-quiet -norestart -log <file outside <P>>`) and the wizard, each code with
   the host's state and its safe retry (the Amendment 2026-11-13, point 9), the logs, the service's
   checks (`sc.exe`, the Application log's 1000 to 1003), install_ac_016's read-only audit-policy and
   dump inspections; `install.test.mjs` with it; the `Â§` mojibake fixed.
2. **The uninstallation and the purge**: the default uninstallation and what it keeps; `setup.exe
   -uninstall CG_PURGE=yes -norestart -log <file outside <P>> [-quiet]`; codes 20 to 47, 14 and 15;
   what the purge is not (point 8); the installation's run of the step, `-unsafeuninstall` purging
   nothing, a manual `cg-setup purge --confirm=yes` with no action purging (Amendment 2026-11-16);
   `operations.test.mjs` if a document is added.
3. **`upgrade.md`'s agents**: the newer setup with no bootstrap (30 with one), downgrade 29, the
   restores read and not read, the re-enrollment route (Manuel's M6 below).
4. **The migration's runbook** (#87's items: item 7's disclosure, the repairs with `takeown /A` and
   `icacls`, the console executable's removal), after an elevated reading of the repair commands
   by Manuel's terminal (`icacls /setowner *S-1-5-18`, junctions); R1 per Manuel's M4.
5. **The disclosures**: #160 (after M1), Sessions 69 to 75's residuals, `data-protection.md`'s
   agent logs; closes #87, #150 and #160 at their destinations.

Questions the readers left that the repo answers are Claude Code's, decided in each commit:
placement (a separate document for the removal and the migration, or sections of `install.md`),
`-log` outside `<P>` on every run, Burn's default log, the purge's manual path disclosed rather than
taught, the bootstrap file's owner on hosts whose administrators' objects are owned by their
creator. Several need a reading first (the repairs, an anchor file with `export-ca`'s trailing
`sha256:` line, Burn's default log path).

## Waiting on Manuel

- **M1, #160's text**, and **M2, #162's order**, as Session 72 put them.
- **M3, the isolated Windows machine**: install_ac_016's runbook validation, F2's completion, the
  wizard's window reading, #59 and #145 wait for it.
- **M4, R1 and #87** (`docs/security/threat-model.md:374`, `docs/debts.md`): whether the threat
  model's R1 closes when the migration's runbook lands; a threat-model text, proposed with commit 4.
- **M5, deployment tools**: whether the runbook says more than the SPEC's unattended command and
  code table (detection rules, exit-code mapping for Intune or Configuration Manager); recommended:
  no, which needs no text from him.
- **M6, re-enrollment of a service-installed agent** (a new server CA, a restore with another CA):
  the only route the contract leaves is a purge and a fresh setup with a new token, the old agent
  left on the server (R5); recommended: the runbook names that route and its residual.
- **The line of Amendment 2026-11-16 (second)**: taken as Claude Code's (no policy changed, the
  debt's own destination); the second text reviewer noted that custody sits near the security
  posture. Manuel may audit or reverse it.
- The release freeze stays; Authenticode stays ADR-0022's Open question 1.

## How Session 77 resumes

1. Run the start protocol (CLAUDE.md). `main` is this handoff's close commit. There is no PR. The
   counts are 56 open debts and 22 ADRs / 23 SPECs.
2. Manuel's answers on M1 to M6, if given.
3. **C10, commit 1** (*C10's plan*): read SPEC-024's amendments from 2026-11-09 to 2026-11-16 and
   `install.md` with its tests; write the endpoint on the setup; the fresh-context review with the
   code prompt (the runbook's tests are code); land by fast-forward, then a new PR for anything
   left on the branch.
4. Then commits 2 to 5 as answers and readings allow. Keep the release freeze.

## Environment and memory

- Docker Desktop stayed down; nothing of Manuel's was touched.
- The probe's scripts stay in the session's scratchpad; its directories were removed. `C:\tmp`
  holds `s76-c10-journal.json` (C10's map), `s76-ci.sh` (the CI poll) and the gate outputs
  `s76-test-all*.out`.
- `target\` holds debug builds without debug info (`CARGO_PROFILE_*_DEBUG=0`).
- Updated memory: the session's close; `reference-windows-acl-facts.md` (MARTA's recomputation).
