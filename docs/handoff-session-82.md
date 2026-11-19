# Handoff — Session 82

Session 82, 2026-11-18 to 2026-11-19, redesigned the repair primitives of C10's fourth commit.
Independent designs and their judges, then an adversarial check of a design without DACL writes,
showed that the runbook cannot repair the agent's directories safely with Windows' own commands.
Manuel decided that the primitives live in a new verb of `cg-setup`, and chose its delivery, its
privilege posture and what it keeps. SPEC-024's Amendment 2026-11-18 was revised and its Amendment
2026-11-19, the verb, written, with the threat model's Amendment 2026-11-18 rewritten around it, on
the owner-review branch, through three rounds of the fixed prompt and a security lens, the third in this session's next commit. No code changed; nothing of the branches landed. No
tag was pushed: the release freeze stays.

## Start and resulting state

- Start: `main` = `origin/main` = `33a1719`, clean, no stash; `docs/c10-migration` (`1cf1b12`) and
  `docs/spec-024-amendment-2026-11-18` (`da98449`) pushed with no PR; no PR open; the tip's CI ALL
  GREEN (markdown-lint); 22 ADRs / 23 SPECs, 58 open debts (58 rows), no Known CI debt. The repo
  agreed with Session 81's handoff. `C:\tmp\s81-read5.log` did not exist: reading 5 never ran. 3.3
  GB of 15.8 GB of memory free, 19.6 GB on `C:`; Docker Desktop down (Manuel's `aura_*` containers
  not touched).
- Resulting `main`: this close's commit, on `33a1719`.
- Branches, both pushed with no PR, so that no workflow ran on them:
  - `docs/spec-024-amendment-2026-11-18`, rebased on this close: the SPEC commit (SPEC-024
    Amendments 2026-11-18, revised, and 2026-11-19, the catalog's line and the
    SPEC-024 → SPEC-023 edge, install_ac_017's row) and the threat model's commit, both subjects
    `NOT YET RATIFIED`. Its worktree is `C:\Users\manul\cg-s82-am`.
  - `docs/c10-migration` at `1cf1b12`, unchanged: Session 81's commit 4, whose repair blocks the
    verb replaces (C8 below). Its worktree is still `C:\Users\manul\cg-s80-c4`.
- Catalogs unchanged on `main`: 22 ADRs / 23 SPECs.

## The design

Three workflows, each reviewer and designer given no summary of the session's reasoning; their
returns are under `C:\tmp`:

1. **Repair primitives** (`wf_cbe8b429-c35`): four designs (pure .NET; fixed functions with a
   plan; security first; minimal surface), three judges (security, operability, contract) and a
   synthesis, `C:\tmp\s82-design.md` (the designs `C:\tmp\s82-wf-0.txt` to `-3.txt`, the judges'
   verdicts `-4.txt` to `-6.txt`). The judges disagreed (operability: the minimal design; contract
   and security: the designs with a compiled helper); the synthesis recommended a C# helper,
   compiled by `Add-Type` in the runbook, enabling the backup and restore privileges.
2. **A design without DACL writes** (`wf_6292fa56-981`), Claude Code's, which only renamed or
   deleted listed names: refuted (`C:\tmp\s82-e-verdict.md`, the experiments
   `C:\tmp\s82-experiments-e.md`).
3. **The verb** (`wf_35181f7d-2a9`), after Manuel's choice: three designs (minimal, operator,
   security), two judges (security: the security design, 8.2, no blocking flaw; contract: the
   minimal one), and a synthesis, `C:\tmp\s82-verb-design.md` (`C:\tmp\s82-verb-wf-0.txt` to
   `-5.txt`).

### What was read unelevated (Windows 11 build 26200, Windows PowerShell 5.1, es-ES)

The readings SPEC-024's Amendment 2026-11-19 cites, each by a subagent under its own
`C:\tmp\s82-*` directory, as their returns record:

- A DACL that `icacls` or .NET's `SetAccessControl` writes to a directory is written to each child
  whose DACL is not protected, a hard link's target included (the first workflow's designs A to D,
  their e1, E1/E3, E2, exp 2); also through a child the caller could not list (its security judge's
  w2: `icacls A /grant me:(OI)(CI)F` rewrote a hard-linked file's DACL through an unlistable child);
  and for a grant that does not inherit (`icacls D /grant me:(S)`, the second workflow's J2).
- Renaming or deleting an object whose DACL admits the caller nothing fails through Windows' commands
  and .NET, which ask for `SYNCHRONIZE`; the parent's `FILE_DELETE_CHILD` does not grant it (the
  second workflow, its contract and experiments refuters).
- A path acted on after a listing follows a junction put in place of a directory on it, `\\?\`
  included: a `File.Delete` and a `Directory.Move` landed in the junction's target (its security
  refuter's X1).
- An open through `NtCreateFile` asking `DELETE` and `FILE_READ_ATTRIBUTES` alone, relative to a held
  parent, without following a reparse point, is granted on a file, a read-only file, a hard link, a
  junction and a directory that admit the caller nothing; a rename through it into another held
  directory keeps the descriptor, attributes and contents, a junction moved as a link; a name
  collision is `0xC0000035`; a deletion through it removes a file or a junction, and fails for a read-only file (`0xC0000121`); `FileHardLinkInformation` reads a file's names with parent file IDs
  through `FILE_READ_ATTRIBUTES`; a hold without `FILE_SHARE_DELETE` refuses another open for
  deletion (Win32 32); a directory beneath which a file is open is not renamed (the third workflow's
  designs and synthesis, its S1; the first's x2b, x3).
- PowerShell's `-eq`, `-ceq` and `-ccontains` treat `agent` + U+00AD + `.toml` as `agent.toml`; an
  ordinal comparison does not (x4).

## Decisions

### Manuel's (2026-11-19, in chat, each by choosing an option of a question)

- Where the primitives live: "Verbo cg-setup (Recomendado)", over a C# helper in the runbook and
  Windows' commands alone.
- Q1, the verb's delivery: "Asset propio (Recomendado)", `cg-setup-X.Y.Z-x86_64-pc-windows-msvc.exe`,
  over a Burn variable and the package cache's copy.
- Q2, privileges: "P0, ninguno (Recomendado)", over `SeTakeOwnershipPrivilege` around owner writes.
- Q4 to Q8: "Sí, todo (Recomendado)": item 6's files restricted, hashed and moved with `<P>`; the
  `legacy` mode; the staging directory moved with `<P>`; what the verb moves kept in the inputs'
  directory with no removal given and no file written; item 2.2's "owner SYSTEM" read as
  Administrators.
- Q3, Q9, Q10: "Como se propone (Recomendado)": the name `custody`, its modes, plan and codes 48 and
  49, the executable in a closed inputs' directory, the revision of 2026-11-18 in place and the new
  Amendment 2026-11-19; no reserved `<P>` after `aside`; an optional read-only `show` on his host
  before the runbook lands.

These are choices, not the approval of the texts, which waits for the compare URL.

### Anticipated (Claude Code)

- **A design workflow before any text** (Manuel's instruction): discarded patching Session 81's blocks.
- **A design without DACL writes put to refutation before Manuel saw the options**: it would have
  avoided a compiled helper and a product change; refuted (F1, F2), so not offered as viable.
- **The fixed prompt and the security lens on the texts as committed**, each reviewer in its own
  worktree.
- **One SPEC commit holding both amendments**: the revised point 4 of 2026-11-18 cites 2026-11-19,
  which a separate earlier commit would cite before it existed.
- **The threat model's commit lands after the verb and the runbook** (its *Status* says so):
  discarded landing it with the SPEC, which would state controls not yet on `main`.
- **The custody host test in `package-ci`, creating an account**, argued in the SPEC and named in
  CLAUDE.md when it lands, under Manuel's approval of the amendment: discarded rust-ci's Windows
  job, where a test that runs the real `cg-setup` against the host may not run.

### Reactive corrections

- The verb's synthesis said the console identity files "read on Manuel's host" are owned by
  Administrators; reading 4's G11 read probes made for it. The SPEC does not carry that claim.
- Round 1's rule that the data root "counts" when SYSTEM owns it, which round 2 showed to refuse every item 2.2 repair after item 2.1's close; replaced by the service's layout (*The reviews*).
- The second review's script still named the first round's SHA when relaunched; stopped and run again
  on the new tip.

## The reviews

The fixed prompt for a SPEC or ADR text (CLAUDE.md compensating control 5) and a security lens, each
reviewer in its own isolated worktree, given no summary, on the branch's two commits as
committed (runs `wf_a3163459-2a8`, `wf_246eddbd-4c4`; their lists `C:\tmp\s82-review-r1.json`,
`C:\tmp\s82-review-r2.json`). Round 3, on the branch rebased on this close, follows in this
session's next commit.

### Round 1, on `91c2855` (fixed prompt 8, security lens 5 list-A points)

Fixed in `f567535`:

- `docs/handoff-session-82.md` cited and absent (both): this handoff; the branch rebased on it.
- The threat model states the runbook and the verb's tests as present (both): its *Status* lands it
  with, or after, the runbook, once install_ac_017 passes.
- The custody host test creates an account in `package-ci`, which CLAUDE.md gives the MSI's test
  alone: the SPEC argues the venue and has CLAUDE.md name it when it lands, under Manuel's approval
  of the amendment.
- `aside` refusing names any account could plant; "counts" unreadable without a privilege for the
  legacy directory: a rule on when the data root and those directories are the agent's (revised
  again in round 2).
- Point 3's "closed" against point 7's "holds"; the catalog's "outside `<P>`" narrower than point 4;
  48 for an in-use refusal after an act (now 49).
- The token named as what keeps the verb out of `identity\` and `logs\`, which an administrator's
  token can open for deletion (security): a rule of the verb's, which its tests assert.
- What `legacy` keeps, said readable by SYSTEM alone, where a copy another account's handle changed
  stays open to it (security).
- A DACL's conditional entries and a refusal's path printed unescaped (security): every name, path
  and DACL escaped.
- A file's owner changed without the owner check item 2.2 requires (security): by the plan that
  printed it, once checked.

### Round 2, on `f567535` (fixed prompt 6, security lens 5 list-A points)

Fixed in the branch's commits rebased on this close:

- Round 1's rule that a data root "counts" when SYSTEM owns it with its closed DACL also holds for a
  console data root after item 2.1's close, so `in-place` refused every item 2.2 repair (both); and
  an Administrators-owned service data root did not count, so `identity\` would have moved
  (security). Now a data root is of the service's layout when it holds none of the console's
  identity files and SYSTEM or Administrators owns it; there `identity`, `logs` and the purge's
  names are never touched and `aside` refuses; in any other data root they are planted.
- The threat model's unconditional "never `identity\`" (both): conditioned on that layout.
- A file with another link given owner Administrators without the owner check (security): the same
  plan and check as for a directory; a file two product names share is not repaired.
- What the verb moves kept in the inputs' directory, which the runbook would later remove or
  re-secure (security): the SPEC forbids it and the threat model records why.
- The tests' convention misattributed to the migration's tests; the Amendment 2026-11-05's R17
  addition not corrected for the new routes; the verb's reading of item 4 for a directory
  Administrators cannot read stated as item 4's (fixed prompt).
- From list B, taken: the plan's prefix names the version and the mode; a token with one of the four
  privileges already enabled is refused; a DACL printed between brackets.

List B of both rounds, not taken, is for the implementation (C1 to C9): what `show` prints first when
many names are planted; the floor build's primitives (a new debt at C9); a staging or legacy
directory planted closed to Administrators before item 2.1, which the verb keeps (an accepted
threat); an unelevated console agent beside the verb; the enrolling account's SID taken from the
change record, not from a DACL; and the wording points.

## Gates and CI

- `markdownlint-cli2` 0.23.2 on SPEC-024, `docs/specs/README.md`, `threat-model.md` at each
  revision, and on this close's files: "Summary: 0 issues in 0 files".
- CI: the branches have no PR, so no workflow ran on them. `main`: this close's commit, as its
  post-push report gives it.

## Test baselines at close

Unchanged on `main` from Session 81: no code, test or runbook changed there.

## Debts

- Closed: none. Opened: none.
- Destinations changed: #87, #164, #166, #167 and #168 point to SPEC-024 Amendment 2026-11-19's verb,
  after Manuel's OK of the owner-review branch: its implementation (C1 to C7 below), C10 commit 4
  rewritten on it (C8), then the threat model's amendment (C9).
- Open at close: **58**.

## Waiting on Manuel

- His OK on the owner-review branch, SPEC-024 Amendments 2026-11-18 and 2026-11-19 and the threat
  model's Amendment 2026-11-18, by the compare URL in this session's last message. It covers the
  custody host test's account in `package-ci` and its later naming in CLAUDE.md.
- `C:\tmp\s82-cleanup.ps1`, elevated, which removes two probe trees Session 82's experiments left
  under `C:\tmp` (`s82-e-x1`, `s82-v-min`) whose directories admit no administrator:
  `powershell -NoProfile -ExecutionPolicy Bypass -File C:\tmp\s82-cleanup.ps1`, log
  `C:\tmp\s82-cleanup.log`.
- Reading 5 is withdrawn: the verb replaces the blocks it read. No reading 6.
- The release freeze stays; Authenticode stays ADR-0022's Open question 1; M3 waits.

## How Session 83 resumes

1. Run the start protocol. `main` is this close's commit; `docs/spec-024-amendment-2026-11-18` and
   `docs/c10-migration` (`1cf1b12`) are pushed with no PR; 58 open debts; 22 ADRs / 23 SPECs.
   Read `C:\tmp\s82-cleanup.log` if Manuel ran it.
2. If Manuel has given his OK on the branch, land its SPEC commit on `main` with the marker stripped
   (CLAUDE.md *Integration path* §3, §4), keep the threat model's commit on the branch; if he asked
   for changes, make them, review again, and send the URL again.
3. Then implement the verb as `C:\tmp\s82-verb-design.md` §9 orders it, each commit through a PR,
   true at its SHA, one per push: C1 the names module in `cg-agent`; C2 the handle primitives in
   `agent/cg-setup/src/custody/nt.rs`; C3 the reading (show and dry runs, every act 48); probe A,
   red, closed; C4 `in-place`; C5 `aside`, `legacy`, `executable`, then probe B (mutants); C6 the
   host test in `package-ci` and CLAUDE.md's naming of it; C7 the release asset; C8 C10 commit 4
   rewritten on the verb (`docs/c10-migration` restacked, round 2's non-blocking list of
   `docs/handoff-session-81.md` with it); C9 the threat model's amendment. The design's §8 is the
   test plan, its §10 the residuals, the reviews' list-B points above its open questions.
