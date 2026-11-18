# Handoff — Session 80

Session 80, 2026-11-17 and 18, took Manuel's answers on M1 to M6, the threat model's question
on #165's token and Session 77's CLAUDE.md proposal; landed SPEC-024 Amendment 2026-11-17 with the
threat model's (`a4bb95a`, M1 and M2, approved by Manuel), CLAUDE.md's local gate for the runbooks
(`c3f517f`, approved by Manuel), C10's fifth commit, the disclosures (`241d14e`, PR #148), and the
release's images job after the agent's (`12b1d0b`, PR #147). Manuel ran three elevated readings of
the repair commands C10's fourth commit is to teach, and answered its six questions (Q1 to Q6);
that commit's first part (#167) waits on `docs/c10-migration`, and its migration runbook, designed
(`C:\tmp\s80-c4-design.md`), is Session 81's. No tag was pushed: the release freeze stays.

## Start and resulting state

- Start: `main` = `origin/main` = `307e138`, clean, no stash, no branch but `main`, no PR; the
  tip's CI ALL GREEN (markdown-lint); 22 ADRs / 23 SPECs, 59 open debts (59 rows), no Known CI
  debt. The repo agreed with Session 79's handoff. 7.4 GB of 15.8 GB of memory free, 22 GB on
  `C:`; Docker Desktop down (Manuel's `aura_*` containers not touched).
- Resulting `main`, in order, each pushed alone with its CI green:
  - `a4bb95a`, SPEC-024 Amendment 2026-11-17 with the threat model's Amendment 2026-11-17;
  - `c3f517f`, CLAUDE.md;
  - `241d14e`, C10 commit 5, squashed from PR #148 (tree `d44fd04`);
  - `12b1d0b`, the release's images job after the agent's, fast-forwarded from PR #147;
  - this close's commit.
- Branches: `docs/c10-migration` (`29d233f`, on `241d14e`, pushed, no PR) holds C10 commit 4's
  first part, #167 (*Waiting branch*). Every other branch of the session is deleted, local and
  remote; PRs #147 and #148 are closed.
- Catalogs: 22 ADRs / 23 SPECs; `docs/specs/README.md` gains the Amendment 2026-11-17's line and
  its by-scope edge on SPEC-023.

## What landed

### SPEC-024 Amendment 2026-11-17 and the threat model's (`a4bb95a`)

- Point 1 (#160, M1): the Amendment 2026-11-13, point 6, and the threat model's entry that said
  `cg-setup` "zeroes its own copy after use" read: preflight zeroes what it reads and the
  configuration it stages, but other steps leave copies of the token freed unzeroed, among them the
  generated configuration's serialization and check, a token written with an escape, and a
  bootstrap file refused as invalid (24); the token may stay in `cg-setup`'s process on both
  routes; the runbook is to name these copies. The threat model adds an accepted threat for the
  bootstrap file's route.
- Point 2 (#162, M2): the release's images job waits for the agent job
  (`needs: [licences, agent]`), read into the Amendment 2026-11-14, point 2, and by scope into
  SPEC-023's first Amendment 2026-10-26, item 4.
- Four fixed-prompt text reviews on `docs/spec-024-amendment-2026-11-17` (`9e285a4..0cfe602`); the
  landed tree differs from `0cfe602` by the ratification words alone.

### CLAUDE.md (`c3f517f`)

Session 77's proposal and what nine fixed-prompt reviews made it say: the Rust bullet names
rust-ci's inputs and `docs/operations/install.md` with the one test that reads it; a bullet for
deploy-ci (its licence check, then `node --test` over `deploy/docker/test/*.test.mjs` but
`images.test.mjs`); `markdownlint` in *Integration path* §2; a Markdown document a CI test reads
(`docs/operations/`, `deploy/docker/README.md`) goes through a branch and a PR; the gate's
prediction stated with its limit (a workflow runs its jobs whole on any path of its filter, and
several filters are wider than their bullets). The ninth review's residual, that the list of wider
filters is not exhaustive, is stated by example and not iterated further
(`feedback-contract-review-convergence`).

### C10 commit 5, the disclosures (`241d14e`)

- `install.md`: #160's copies on both routes; the re-enrollment of an enrolled endpoint by the purge
  and a new token with the server's current anchor (code 7, a new server with another CA), the
  code-5 repair and `backup-and-restore.md`'s agents enrolled after a backup pointing to it; its
  residuals (the purge revokes nothing; the old agent stays on its server, its certificate valid
  until it expires or an operator deletes its row, R5 and R1; R17's 27 or 28 between the purge and
  the setup; not run, in CI or on an endpoint; an agent installed by hand not enrolled again so);
  the interactive route's message box; the elevation on a standard user's desktop, on both routes;
  keystrokes from a lower-integrity process not read; Windows keeping the Application log's entries
  and their text after an uninstallation, without the source's message table.
- `data-protection.md`: the service's log (path, bounds, access, the host name, the server's URL,
  the key's fingerprint), the agent run by hand, the Application log, the migration's record, what
  a default uninstallation keeps and what the purge leaves on the endpoint and the server.
- `upgrade.test.mjs` pins the re-enrollment sentences, seen failing on `307e138`'s `install.md`
  (`C:\tmp\s80-c5-red.txt`).

### The release's images job after the agent's (`12b1d0b`)

`release.yml`'s images job `needs: [licences, agent]`, its header and `.github/workflows/README.md`
in that order; `release-workflow.test.mjs` asserts that the images job, and the bundle after it,
depend on the agent job, and refuses a copy whose images job needs the licence check alone (seen
failing on `307e138`'s `release.yml`, `C:\tmp\s80-m2-red-kept.txt`).

## The elevated readings (Manuel, 2026-11-17)

Three scripts, each run elevated by Manuel on Windows 10.0.26200, PowerShell 5.1.26100.9818, es-ES,
on probe objects under `C:\tmp` that each removed at its end (logs UTF-16; summaries in
`C:\tmp\s80-read1-summary.md` and `C:\tmp\s80-read23-summary.md`). NetworkService stood for
another account (a SID outside the administrator's token), and `C:\tmp\s80-sdtool.exe` (source
`C:\tmp\s80-sdtool.cs`) read each object's descriptor by handle, without MARTA and without
following a reparse point, and set the probes' starting descriptors with SeRestorePrivilege in its
own process.

- Reading 1 (`s80-read1.ps1`, 23:21, HEAD `307e138`): `(Get-Acl -LiteralPath $p).Owner` prints an
  owner whose DACL denies Administrators read access; `takeown /F <x> /A` (no `/R`) re-owns the
  object named alone, a junction's or symbolic link's own descriptor, not its target, and a file by
  its full path inside a directory only SYSTEM opens; `icacls` acts on a link, never its target,
  with or without `/L`; `icacls /setowner *S-1-5-18` sets SYSTEM, no recursion, but only where
  Administrators already have access; `icacls /inheritance:r /grant:r` on a directory propagates
  to children Administrators can reach, not to protected ones nor a link's target; `rmdir` (no
  `/s`), `[IO.Directory]::Delete(p,$false)` and `.Delete()` remove a link alone; a file whose DACL
  denies Administrators is removed by `[IO.File]::Delete`, not `Remove-Item -Force`;
  `fsutil file setCaseSensitiveInfo <dir> disable` works; a running console executable is found by
  `Get-Process`/CIM and cannot be removed until it stops; a copy kept aside inherits `$dir`'s
  SYSTEM and Administrators.
- Reading 2 (`s80-read2.ps1`, 23:26, HEAD `307e138`): the order `takeown /A`, then `icacls`'s DACL,
  then `/setowner` repairs a file and a directory another account owns to `O:SY D:PAI(BA)(SY)`;
  propagation leaves children whose DACL excludes Administrators, protected ones (the identity and
  `agent.toml`) and link targets unchanged; a planted junction with a foreign DACL is not removed by
  `rmdir`; `fsutil hardlink list` prints `Error 5` for a name in a directory Administrators cannot
  open, and `[IO.File]::Delete` removes that name by its path; `rmdir /s /q` exits 0 although it
  removed nothing.
- Reading 3 (`s80-read3.ps1`, 23:29, HEAD `c3f517f`): a planted junction with a foreign DACL is
  removed by `takeown /F <j> /A`, `icacls <j> /grant '*S-1-5-32-544:F' /L`, `cmd /c rmdir <j>`,
  its target intact; takeown alone does not; a foreign file symbolic link by `[IO.File]::Delete`.

## Decisions

### Manuel's (2026-11-17, in chat)

- **M1** (#160): "Corregir el texto" — the text correction, not a zeroed secret type.
- **M2** (#162): "Sí, needs: [licences, agent]".
- **M3**: "Aún no" — the isolated machine is not available.
- **M4** (R1, #87, #164): "Sí, enmienda TM con commit 4".
- **M5**: "No, solo comando y tabla".
- **M6**: "Nombrar esa ruta y su residual".
- **The token's resting places of #165**: "Registrarlo en el TM", in commit 4's amendment.
- **CLAUDE.md**: "Sí, aplicar la propuesta", on an owner-review branch.
- **The texts**: "doy el ok a la enmeinda y a claude.md" (SPEC-024 Amendment 2026-11-17 with the
  threat model's, and CLAUDE.md), after running reading 2.
- **C10 commit 4** (`C:\tmp\s80-c4-design.md`, its §10), each the recommended option:
  - Q1: R1 closes with commit 4 and its threat-model amendment, reopening if install_ac_016's run
    fails;
  - Q2: the in-place repair of a foreign `<P>` or data root on an enrolled host, an accepted threat,
    with a restart and another administrator's session first;
  - Q3: item 6 keeps the key aside, an accepted threat, deleted once the record no longer needs it;
  - Q4: a suspected compromise goes straight to item 6's new enrollment, by a SPEC-024 amendment
    line; a foreign CA or an expired identity migrates first;
  - Q5: the unlistable hard-link name, `agent.migrating\identity` after a staging refusal, the
    untrusted states and mount points, accepted threats in the amendment;
  - Q6: reading 4 runs before commit 4 lands.

### Anticipated (Claude Code)

- **M1 and M2 in one SPEC-024 amendment** with the threat model's, on one owner-review branch.
  Discarded: two amendments, two readings for Manuel the same day.
- **The readings with NetworkService and a stored-descriptor helper.** Discarded: creating a local
  account on Manuel's host (an operation needing his OK) and reading by MARTA, which recomputes a
  DACL against the parent (`reference-windows-acl-facts`).
- **Commit 5 before commit 4.** Discarded: the plan's order, which would hold the disclosures, ready
  and reviewed, behind the readings and Q1 to Q6.
- **#167 as commit 4's first part, on its own branch.** Discarded: landing it with commit 5, whose
  review it had not had.
- **The release's local build run for a `needs:`-only change** (CLAUDE.md's *Windows package*
  bullet triggers it by path). Discarded: skipping it, a weakening of the gate.
- **C10 commit 4 left to Session 81** (`feedback-checkpoint-over-rushing`): its design needs reading
  4 and three review rounds at least, as S78 and S79's did.

### Reactive corrections

- CLAUDE.md, nine fresh reviews: the scope of §1 (`deploy/docker/README.md`), deploy-ci's bullet,
  its schedule and licence check, §2's markdownlint, package-ci's and other workflows' jobs run
  whole on their filters; an exception that would have dropped `cg-setup` from the Rust bullet was
  withdrawn (a weakening is Manuel's).
- The amendment, four reviews: the accepted threat for the bootstrap file's route; no claim on a
  secret type; the copies named as a list among others; an escaped token's; the *Effect*'s point 1.
- Commit 5, four reviews (the fixed code prompt and lenses): the Application log keeps its text;
  the host name in the log; what the purge leaves; re-enrollment after an expiry, not a CA change on
  the same server; R17; the code-5 repair taught removing the identity (now the re-enrollment route);
  a restored server; the agent's row blocked by alerts and incidents too (the clause removed).
- Readings: `icacls` on a file without an Administrators entry is refused (reading 1), so reading 2
  ordered `takeown /A` first; a foreign junction resists `rmdir` (reading 2), so reading 3.
- Edit scripts lost backslashes four times (`feedback-bash-heredoc-backslashes`); each was caught by
  a test or a grep and fixed with the Edit or Write tool before any commit.

## Self-review (CLAUDE.md compensating control 5)

### Claude Code's matrix

| Criterion | Test | Verdict |
|---|---|---|
| Amendment 2026-11-17 point 2: images and bundle wait for the agent job | `release-workflow.test.mjs`, the images-order test (and its refusal of a changed copy) | covered (seen red) |
| Point 1: the runbook names the copies on both routes | none (the amendment asks for no test) | weak: text, read against the code by three reviewers |
| M6: the route and its residual | `upgrade.test.mjs`, the M6 test (route, R5/R1, not run, 27 or 28, the old sentence gone) | covered (seen red) |
| SPEC-020 §11 in `data-protection.md` | none | weak: text, read against the code |
| Amendment 2026-11-12 (third) point 1, 2026-11-13 point 6 disclosures | none | weak: text |
| CLAUDE.md's deploy-ci bullet | its command run: 61 tests, 57 passed, 4 skipped; licences 431 | covered by running it |

### The fresh reviewers

- CLAUDE.md: nine fixed-prompt text reviews; their list-A points **fixed** but the last, whose
  remaining incompleteness is stated by example (above). List B **recorded**: `install.md`'s
  Linux branch of `install_ac_002_setup_runbook` not run locally; existence-only links from the
  runbooks to `threat-model.md`, `debts.md` and `infra/dev/README.md`; the `--ignore-scripts`
  install before the TypeScript gate.
- The amendment: four fixed-prompt text reviews, the last with no list-A point.
- Commit 5: the fixed code prompt four times and three lenses; every blocking point **fixed**.
  Recorded, not blocking: the threat model's Amendment 2026-11-15 says the entry "loses its message
  text once the MSI removes the source", which the runbook now states as the strings kept without
  the message table (**#168**, threat-model text, commit 4's amendment); `install.md:746`'s entry
  text after an uninstallation names no command that still reads it; the disclosures carry no test.
- C10 commit 4's design: two independent designs and a judge (`C:\tmp\s80-c4-design-a.md`, `-b.md`,
  `s80-c4-design.md`), with the judge's list of four claims both designs had wrong against the
  readings.

## Gates and CI

- Amendment and CLAUDE.md: `markdownlint-cli2` "Summary: 0 issues in 0 files" on each file.
- M2, local gate at `849a5a6`'s tree (identical but for docs to `12b1d0b`'s): `node --test
  "deploy/windows/test/*.test.mjs"` 86 tests, 77 passed, 9 skipped; the pins restored and verified;
  `release-windows.mjs build --version 1.2.3`: every check PASS and "the source archive is the
  pinned tree and the setup's sources"; `release-check-matrix.mjs`: "release check matrix: all
  cases passed"; `release-windows.test.mjs` with the built files: 33 tests, 33 passed;
  `release-version.test.mjs` and `release.test.mjs`: 11 passed.
- Commit 5, local gate at `d44fd04`: deploy-ci's tests but images, 61 tests, 57 passed, 4
  skipped; `check-licences.mjs` "licences admitted: 431 packages"; `cargo test -p cg-setup --test
  install_ac_002_setup_runbook` 2 passed; markdownlint 0 issues.
- CI: `a4bb95a` and `c3f517f` on `main`, markdown-lint success, ALL GREEN. PR #148: `413257f`,
  `aaeb7bd`, `d44fd04` each markdown-lint, deploy-ci, rust-ci success, ALL GREEN; `241d14e` on
  `main`: as its post-push report gives it. PR #147: `c01dc1f`, then, rebased on `241d14e`, `12b1d0b`,
  each markdown-lint, deploy-ci and package-ci success, ALL GREEN; `12b1d0b` on `main` (push): as
  its post-push report gives it.
- No Rust changed; no elevated gate (no path of *Developer-local SPEC-005 marquee validation* §5).

## Test baselines at close

Non-elevated `cargo test --all`: unchanged from Session 79 (no Rust changed). deploy-ci's tests but
images, on this host with Docker down: 61, 57 passed, 4 skipped. `deploy/windows/test/*.test.mjs`:
86, 77 passed, 9 skipped.

## Debts

- Closed:
  - **#160**, by `241d14e`: `install.md` names the copies on both routes (SPEC-024 Amendment
    2026-11-17, point 1).
  - **#150**, with no new commit: its destination was met by `fdd0ea5` (C10 commit 1: the inputs'
    directory created protected before the token, the anchor a local file in it), which Session 77
    recorded and Session 80's readers checked against `install.md`.
- Narrowed: **#162**, by `12b1d0b`: the job order is settled; the release's checks beyond the
  Amendment 2026-11-14's wording remain, before the first release.
- Opened: **#168**: the threat model's Amendment 2026-11-15 says an Application log entry "loses its
  message text once the MSI removes the source", while the entries keep their strings, which hold
  the text (`agent/cg-agent/build_resource.rs`, `service/windows.rs`), as `install.md` now says.
  Destination: C10 commit 4's threat-model amendment (threat-model text, Manuel's).
- Open at close: **58**.

## Waiting on Manuel

- **Reading 4** (Q6), before commit 4 lands: Session 81 writes it from the design's §11 with the
  blocks as written.
- **The threat-model amendment of commit 4** (M4, Q1 to Q5, #164, #165's resting places, #168):
  Session 81 drafts it on an owner-review branch; his explicit approval of its text.
- **Q4's SPEC-024 amendment line** (a suspected compromise straight to item 6): its text, his.
- The release freeze stays; Authenticode stays ADR-0022's Open question 1; M3 waits.

## How Session 81 resumes

1. Run the start protocol (CLAUDE.md). `main` is this handoff's close commit; `docs/c10-migration`
   (`29d233f`, on `241d14e`) is the one branch, with no PR. The counts are 58 open debts and 22
   ADRs / 23 SPECs.
2. C10 commit 4 from `C:\tmp\s80-c4-design.md` (its §1 to §9, with §0's and §9's corrections),
   on `docs/c10-migration` after its #167 commits: the two `install.md` sections (*Migrate a console
   installation*, *Repair the agent's directories*), their tests, `upgrade.md`'s *Agents*; with
   Manuel's Q1 to Q6 (above). Every removal checks its name is gone (`Test-Path`), never an exit
   code; no `rmdir /s`, `takeown /R` or `icacls /T`.
3. The SPEC-024 amendment line for Q4 and the threat-model amendment, on an owner-review branch,
   for Manuel's approval; reading 4 (`C:\tmp\s81-read4.ps1`) for Manuel to run elevated.
4. The fixed code prompt and lenses until no blocking point, then land, closing #87, #164, #166,
   #167 and #168 as the amendment and the runbook settle them. Keep the release freeze.

## Environment and memory

- Docker Desktop stayed down; nothing of Manuel's was touched.
- `C:\tmp` holds the fact maps `s80-facts-*.md`, the readings `s80-read{1,2,3}.ps1` and their logs
  and summaries, `s80-sdtool.cs` and `.exe`, the designs `s80-c4-design*.md`, the red phases
  (`s80-m2-red-kept.txt`, `s80-c5-red.txt`, `s80-167-red.txt`), the release build `s80-rel\`, and
  `s80-ci.sh`.
- The worktree `C:\Users\manul\cg-s80-c4` holds `docs/c10-migration`.
- Updated memory: the session's close.
