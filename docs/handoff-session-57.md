# Handoff — Session 57

Session 57, 2026-11-07, implemented install_ac_013, the legacy migration
(SPEC-024 Amendment 2026-11-05), and closed debt #146. Two short amendments
came first or out of the reviews, each approved by Manuel: SPEC-024
Amendment 2026-11-07 (the client certificate's PEM, #146) and Amendment
2026-11-07 (second, the migration's residuals, with the threat model's). The
code landed by fast-forward as `747926e` (#146) and `fb904ba` (the
migration), after a probe pull request, four rounds of fresh-context review,
CI ALL GREEN on each SHA and a green elevated gate. The session was cut once
by the usage limit and resumed from the note it left in the auto-memory.

## Start and resulting state

- Start: `main` = `origin/main` = `09fb08c`, clean, no stash, only `main`, no
  PR open; `main`'s tip CI green (`markdown-lint → success`); 22 ADRs / 23
  SPECs; open debts 53; Known CI debt none. The repo agreed with the handoff
  and the opening prompt.
- Host at start: C: 27 GB free, 0.9 GB of RAM free, Docker up (29.1.2).
- Resulting `main`: `845e74a` (PEM amendment), `1b1ffc2` (residuals
  amendment), `747926e` (#146), `fb904ba` (the migration), and this close
  commit. No branch but `main`, no PR open.
- Catalogs: 22 ADRs / 23 SPECs (SPEC-024 amended twice in place; the threat
  model amended).

## The amendments

- **SPEC-024 Amendment 2026-11-07, the client certificate's PEM (`845e74a`).**
  In installed mode the identity step accepts no certificate document that
  the reader TLS uses refuses: code 3 in the server's answer, code 5 at the
  identity step, before capture, where it was code 6 after it. I first
  wrote it as my own amendment; the fresh reviewer showed that SPEC-024's
  Amendment 2026-11-05 counts "the codes' new causes" among Manuel's points
  and that my description of the TLS reader was false in both directions
  (List A, two entries). Rewritten to state the requirement, not the
  reader's internals, and asked: Manuel, 2026-11-07, "Sí, apruebo".
- **SPEC-024 Amendment 2026-11-07 (second), the migration's residuals, with
  the threat model's (`1b1ffc2`).** Out of the code reviews: an entry
  created in a file transaction (TxF) before item 2.1 can be committed past
  the closed DACL, so the ratified "no ordinary user can create an entry in
  them" holds for handles only; the names are read again just before the
  first rename, and what a transaction commits later is neither read nor
  used (by scope on §Data contracts §2's "must prohibit unprivileged
  creation"). Named streams are neither listed nor read; item 2.3 reads no
  file above 16 MiB; the failures with no code are code 5; a record that
  cannot be read is a third state that deletes nothing; what "each refusal"
  reaches. Four text rounds (fixed prompt); the last left one List A, the
  by-scope clause, applied before asking. Manuel, 2026-11-07, "Sí, apruebo".

## The implementation (`747926e`, `fb904ba`)

- **#146 (`747926e`).** `x509::certificates_in` reads the document as TLS
  does and its error quotes nothing of it; the token rewrite reads
  `agent.toml` from a handle the custody check passed; the effective-access
  probe gains `FILE_DELETE_CHILD` and `<P>`; an ingest test
  (`services/ingest/test/install-ac-005-issued-cert-fixture.test.ts`) issues
  a certificate again and compares the subject and SPKI with the agent's
  fixture, which nothing regenerated in CI.
- **The migration (`fb904ba`).** `agent/cg-agent/src/service/migration.rs`:
  `parse_start_arguments`, `prepare` (items 1 to 4 and 6, and the data
  directories' check), `copies` (item 5), all over an `Environment` that the
  tests point at a temporary root with the current user in SYSTEM's place
  and a `Watch` that injects a failure or a crash at each `Point`, with no
  Cargo feature. `custody.rs` gains the primitives: every open of a checked
  object and every operation that can wait runs under `bounded`
  (`CancelSynchronousIo` after 30 s), `SetKernelObjectSecurity` (no
  propagation), file IDs, a read that excludes writers with a 16 MiB bound,
  deletion through the checked handle (POSIX semantics, read-only ignored),
  the case-sensitivity flag, the host name. `service/windows.rs` passes the
  SCM start arguments, runs `prepare` under `block_in_place`, renews
  `START_PENDING`'s cap at each step, adds item 6's reason to code 2 on a
  console layout, and runs `copies` after `RUNNING` under the lock. Two
  `windows-sys` features of the agent (`Win32_System_IO`,
  `Win32_System_SystemInformation`); no crate. `docs/operations/install.md`
  gains the migration's codes.
- **Tests.** Unelevated, over temporary roots: `install_ac_013_route.rs` (65)
  and `install_ac_013_interrupted.rs` (73), with real opportunistic locks
  (read-handle, read-write-handle, batch) and real file transactions. The
  SCM target `install_ac_013_migration.rs` (10) enrolls a real console agent
  as the runner's Administrator and migrates it with each choice. The tests
  were written by a workflow from the SPEC against a stubbed API, audited
  sentence by sentence, then fixed; against the stub 118 of 121 failed for
  the missing behaviour (`d9a0af0`, the author's record), the three others
  being a source guard and two tests that only CI can run.

## Decisions

### Anticipated (Claude Code)

- **The migration as one module with an injectable environment.** Rejected:
  testing through the SCM alone, which no local terminal runs.
- **Tests from the SPEC by an independent workflow, against a stub.**
  Rejected: writing them myself next to the code, the self-confirmation the
  compensating controls guard against.
- **`SetKernelObjectSecurity` through the checked handle (#147).** Rejected:
  `SetSecurityInfo`, which propagates to children.
- **A bounded thread and `CancelSynchronousIo` for every wait (#147).**
  Rejected: `NtCreateFile` with `FILE_COMPLETE_IF_OPLOCKED`, which covers
  opens and not renames, deletions or security changes.
- **Each migration step renews `START_PENDING`'s 120 s cap (#147).**
  Rejected: an unbounded cap, which would hide a start that hangs.
- **The record's rewrite through `migration.json.rewrite-tmp` (#147)**, the
  hygiene's mechanism.
- **The SCM target's red evidence by probe PR #88.**

### Reactive corrections

- The first PEM text was mine and described the reader wrongly; it went to
  Manuel (above).
- Rendering descriptors with `GetNamedSecurityInfoW` recomputes the
  inherited flags against the parent, so after closing `<P>` a child's DACL
  looked changed; the fixtures read through the handle.
- Round 1 (fixed prompt, FFI, security): an unbounded attribute open let a
  batch oplock on a planted `agent.migrating` hang every start before the
  lock (both lenses, reproduced); a refusal's list of what is restricted was
  wrong on a continued migration and after the second link read; an
  unreadable record was taken for an absent one and its directory deleted;
  a read-only copy was never removed; TxF (above). Fixed in `5a84e33`, the
  TxF part through the amendment.
- Round 2: the refusal line lied on a retry after a refused attempt, so the
  trail became a reading of the objects at refusal time; between the renames
  an unreadable record no longer named both directories (my regression); the
  re-read compared counts, not names. Fixed in `b3908a6`.
- Round 3: my round-2 edit dropped the record rewrite's bound (regression,
  reproduced); the repair offered between the renames would strand the host;
  the refusal line followed a junction at the data root. Fixed in
  `afe0f7b`.
- Round 4: a required file gone since the check offered "start the migration
  again", which the next start refuses; the line followed a junction at
  `<P>`; four behaviours unpinned by tests. Fixed in `10384f9` and
  `ef3e57d`.
- **The console guard in `serve`** (a console layout whose `agent.toml`
  passes every check, with no `identity\`, refused with code 2) was removed:
  it narrowed item 6's "keeps the outcome" with no amendment (memory
  *contract-narrowing checks*), and only an elevated administrator's edit
  reaches that state.
- The first elevated gate ran in an unelevated PowerShell (`net session`
  exit 2, `ac-001-marquee` skipped, the `--ignored` tests failing for
  privilege); the script only recorded the elevation. It now aborts, and
  Manuel ran it again elevated.
- A `sed` replacement in a test matched two call sites, and a chained
  command pushed the probe branch before the compile failure was seen (no
  PR was open, so no CI ran); fixed and force-pushed to my own branch.
- The review fix commits were squashed into `fb904ba` on the second
  amendment: `61af701` had one red SCM test, and every landed commit is
  green at its SHA.

### Manuel's decisions in chat

- 2026-11-07: "Sí, apruebo" (SPEC-024 Amendment 2026-11-07, the PEM).
- 2026-11-07: "Sí, apruebo" (SPEC-024 Amendment 2026-11-07, second, and the
  threat model's).

## Self-review (CLAUDE.md compensating control 5)

My matrix (contract: SPEC-024 Amendments 2026-11-05 and 2026-11-07, both;
`route` and `interrupted` are `agent/cg-agent/tests/install_ac_013_*.rs`,
`scm` is `install_ac_013_migration.rs`):

| Criterion | Test | Verdict |
|---|---|---|
| Item 1: the arguments, code 2, no value repeated | route:163, route:200 | covered |
| Item 1: no migration without a console layout or one in progress | route:2189, scm:1869 | covered |
| Item 2: order, lock first | route:340, route:381 | covered |
| 2.1 closing, no propagation, refusals | route:450, route:526; scm:1114 | covered |
| 2.2 what is there, repairs | route:686, route:820 | covered |
| 2.3 one read, writers excluded, config, anchor, identity | route:999, route:1181, route:1285 | covered |
| 2.4 restriction in place | route:1395 | covered |
| 2.5 record first, staged files, generated configuration | route:1671, route:1768 | covered |
| 2.6 commit, reversal | route:2051; interrupted (reversal tests) | covered |
| Item 3 record and item 4 states | interrupted:444, :527, :1180, :1509, :857 | covered |
| Item 5 copies, both choices | interrupted:1745, :1775, :923; scm:1770, :1777, :1788 | covered |
| Item 6 default refusal | route:2189; scm:1057 | covered |
| Item 7 evidence, no secret or SID | interrupted:2086, :2107 | covered; the runbook's disclosure is the package's (#87) |
| Preamble: 30 s cancellation | interrupted:883, :903; route:1596; scm:1515 | covered |
| Second amendment: TxF, streams, bound, re-read | interrupted:695, :813, :832, :673; route:1459, :1485 | covered |
| PEM amendment | x509_identity.rs:47, :82; install_ac_005_identity_writes.rs:348, :377, :422 | covered |
| "A non-v3 certificate stays code 6" | none | weak: read in `x509.rs`, no v1 fixture |

Fresh-context reviews (Workflow, isolated worktrees, CLAUDE.md's fixed
prompts plus lenses): round 1 fixed prompt, FFI and security on
`845e74a..61af701`; round 2 text, fixed prompt and a regression lens; rounds
3 and 4 the fixed prompts on text and code. Every List A and every
contradicted or missing verdict was fixed (above) or recorded here. Recorded,
not fixed: the token rewrite's window in which an elevated administrator's
edit is lost (elevated only); `Unprivileged` on the migrating route refuses
with an invented Win32 error 5 (unreachable as LocalSystem); a refusal line
can understate what is restricted when an object cannot be read within a
second; the 30 s bound reads as up to about 35 s with the cancellation's
retries. The reviewers share my model: a second reading, not a second judge.

## Gates and CI

- **Local gate** on `fb904ba`: `cargo fmt --all -- --check` clean; clippy
  `-D warnings` clean; `cargo test --all` **350 passed, 0 failed, 11
  ignored**; `node .github/scripts/agent-version-matrix.mjs` "agent version
  matrix: all steps passed"; `cargo audit` 0; ingest `tsc --noEmit` and
  `biome check .` clean. markdownlint 0 issues on every changed document.
- **The probe, PR #88** (`a165124` = `845e74a` + the SCM tests, the step
  with `continue-on-error`): run 42327365022, Windows job 128265647603. The
  10 tests of `install_ac_013_migration` and the 2 new cases of
  `install_ac_004_custody_service` failed for the missing behaviour (code 13
  without item 6's reason, no `RUNNING` with the arguments, arguments
  ignored without a console layout, no wait on the oplock; code 6 where 3
  or 5 is due). Closed without landing, branch deleted.
- **CI, full SHAs:** `845e74a`, `1b1ffc2` markdown-lint → success;
  `aae657a` and `747926e` ALL GREEN; `61af701` FAILURES (rust-ci, one SCM
  test, the in-place line of other recorded arguments; fixed);
  `5a84e33`, `b3908a6`, `afe0f7b`, `fb904ba` ALL GREEN. On `fb904ba` the
  SCM step ran 9 + 11 + 10 tests, all passed (run 42367093631).
- **Elevated gate s57-gate1, green.** `C:\tmp\s57-gate1.ps1`, pure ASCII,
  `Parser::ParseFile` 0 errors; Manuel ran it the second time elevated.
  Literal lines from the UTF-16 logs:
  - `net session exit: 0`; `docker engine: UP`; `ServerVersion=29.1.2`.
  - `HEAD  = fb904bab7c01b38e1f3ea0872bb458ea664cafd1`;
    `TREE  = 6f38a00c94aaaaf78ea2096c2de3f5d60a18cd15`.
  - `release exe: 2026-11-07T12:17:19.3934887+01:00  commit: 2026-11-07T12:01:47+01:00`.
  - `--ignored`: 11 passed, 0 failed (`auth_ac_012`, `capture_ac_006`,
    `net_ac_008`, `net_ac_009`, `ac_004`, `ac_007`, `ac_009`, and
    install_ac_009's four).
  - vitest: `Test Files  86 passed (86)`, `Tests  504 passed | 2 skipped
    (506)`, `Duration  469.61s`.
  - `detect_ac_001 captured image_file_name — probe:
    C:\Users\manul\AppData\Local\Temp\cg-detect-probe-Ig9wtF\winword.exe;
    child: C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe`;
    `detect_ac_001 drained 57 rows in 1 cycles; capture checked at 35.318 s,
    drained at 41.242 s`; `"detect_marquee_elapsed_seconds":41.254`;
    `spec_005 captured image_file_name — launch: C:\Windows\System32\cmd.exe;
    terminate: cmd.exe`.
  - `logman query CGAgent-KernelProcess -ets`: "No se encontró el Conjunto
    de recopiladores de datos."
  - `=== s57-gate1 done 2026-11-07T12:36:08.9394057+01:00 ===`.

## Landing

PR #89's branch held two commits on `1b1ffc2`, `main`'s tip, so `main` was
fast-forwarded to the same SHAs CI and the gate ran on, one per push: the
guard checked that each commit's parent was `main`'s tip. `747926e`, then
`fb904ba`, CI green on each. PR #89 was closed with a comment naming them,
and the branch deleted, local and origin.

## Test baselines at close

- Non-elevated `cargo test --all`: 350 passed, 0 failed, 11 ignored.
- The SCM suite in CI: 30 tests (9 + 11 + 10).
- Elevated `--ignored`: 11 passed.
- Elevated vitest: 86 files, 504 passed, 2 skipped.
- Unelevated, the four capture marquees fail with exit 1 on this host
  (CLAUDE.md *Environment facts*); not re-run this session.

## Debts

- Closed:
  - **#146**, by `747926e`: the PEM rule (with its amendment), the token
    rewrite's read from a checked handle, `FILE_DELETE_CHILD` on the
    parents, the ingest test that keeps the fixture in step. The hygiene
    temporary's access is closed by reason: the file moved over
    `agent.toml` is that temporary, whose owner and DACL the SCM tests
    assert after the rewrite.
  - **#147**, by `fb904ba`: each point settled as in *Decisions*, plus the
    unparsable record (beside a console layout a torn record is no record;
    in the data root it is code 2), a legacy object whose file ID is not
    recorded (not reached, never touched), a `%ProgramData%` other than
    `C:\ProgramData` (the paths come from the OS; a console set up
    elsewhere is not a console layout at the service's path), codes 13 and
    5 (the runbook's paragraph), the runner's isolation (`FixedDataRoot`
    moves the tree aside and restores it). The repairs' elevation needs
    (`takeown /A`, `icacls`) move to #87's runbook item.
- Destination changed: **#87**, narrowed: install_ac_013's implementation
  has landed. Open for the package's half (install_ac_001/011/015: the
  default refusal in setup, the route that starts the service with its
  arguments, the configuration and anchor owned by SYSTEM) and the runbook
  that documents the migration (item 7's disclosure, the repairs'
  commands).
- Opened: none.
- Open at close: **51**.

## Waiting on Manuel

Nothing blocks. The packaging change (install_ac_001/015) can restore WiX
7.0.1 from nuget.org, which he confirmed in S56.

## How Session 58 resumes

1. Run the start protocol (CLAUDE.md). `main` is this handoff's close
   commit; no PR, no branch but `main`; 51 open debts; 22 ADRs / 23 SPECs.
   Check C:, RAM and Docker before builds.
2. F2's next item: **the package** (install_ac_001/015, and #87's half):
   WiX 7.0.1 and the pins of `docs/handoff-session-56.md` *Track A*
   (`global.json`, the tool manifest, the lock file, the hash check,
   `setup-dotnet` in CI, the licence allowlist under MS-RL), the MSI that
   leaves `<P>` alone on the migration route, the setup that starts the
   service with the migration's arguments, and the migration's runbook.
3. Keep the release freeze.

## Environment and memory

- The usage limit cut the session once; the auto-memory note
  `project-session-57-in-progress.md` carried the state across and is
  retired at this close.
- The elevated gate script now aborts when PowerShell is not elevated.
- Windows facts learned here go to `reference-windows-acl-facts`.
