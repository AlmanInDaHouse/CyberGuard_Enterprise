# Handoff — Session 50

Session 50, 2026-10-31 to 2026-11-01, starts F2's implementation from Accepted
SPEC-024: the build-time product version (#131, install_ac_014), and the
`detect_ac_001` marquee's capture-first and drain fixes (#4, #37) with the
first change that needed the capture gate. Manuel's opening instruction
(2026-10-31): "Toma las decisiones delegadas en beneficio del proyecto y
regístralas. Los cambios de ADR necesitan mi sí explícito; el gate elevado lo
corro yo." He ran the elevated gate. No ADR was changed or ratified.

## Start and resulting state

- Start: `main` = `origin/main` = `10d1c2df1fc0f9df528742bfca596b562c05571a`,
  clean, empty stash, only the root worktree and `main`, no open PR. Its two
  Markdown runs were green (`40433947369` push, `40433889939` PR). The weekly
  `dependency-audit` and `deploy-ci` schedule runs of 2026-10-26 were green.
- Catalogs: 22 ADRs / 23 SPECs at start and at close (two SPEC amendments, no
  new SPEC). Known CI debt: none. Open debts: 48 at start, 49 at close.
- Landed, in order, each one commit per push with CI green before the next:
  `de3bf0c`, `7297412`, `fd51dca`, `9e68d56`. Pre-close `main` is
  `9e68d5692d14ddf18305db9c4dfdc3709e32d654`, ALL GREEN. The close is a
  separate docs-only branch and PR; its SHA and CI go in auto-memory and the
  close response.
- Release ruleset `25923038` stays active on `refs/tags/v*`. No version tag,
  release, service installation, host-policy change or reboot was made.

## Landed changes

| Commit | PR | Change |
|---|---|---|
| `de3bf0c` | [#72](https://github.com/AlmanInDaHouse/CyberGuard_Enterprise/pull/72) | SPEC-006 Amendment 2026-10-31: detect_ac_001 proves capture first and drains (#4, #37); by scope, SPEC-018 late_ac_008 |
| `7297412` | [#73](https://github.com/AlmanInDaHouse/CyberGuard_Enterprise/pull/73) | The marquee: agent ran its window; Launches (probe and child, Win32 form) before any detection assertion; settle margin on ClickHouse's clock; drain to fewer than `BATCH_LIMIT` rows (cap 20); NFR-006-005 elapsed line from the probe's spawn, written in a `finally`, and its 60 s asserted; 90 s timeout; CLAUDE.md facts |
| `fd51dca` | [#74](https://github.com/AlmanInDaHouse/CyberGuard_Enterprise/pull/74) | SPEC-024 Amendment 2026-10-31: each release-version component is a SemVer numeric identifier; any other input fails the build |
| `9e68d56` | [#75](https://github.com/AlmanInDaHouse/CyberGuard_Enterprise/pull/75) | #131's install_ac_014: `version.rs` (`option_env!("CG_RELEASE_VERSION")`, validated at compile time, else `<Cargo>-dev`) in CLI, enrollment, both heartbeats and the "agent starting" line; `install_ac_014_version.rs` (5 tests) and `enroll_ac_003`; `agent-version-matrix.mjs` in both rust-ci jobs and the local Rust gate; `release.yml` passes the tag's version and checks `--version` |

Unelevated, `detect_ac_001` failed with `expected [] to have a length of 1 but
got +0`; it now fails with `the agent exited on its own before its window
ended, exit code 9 (9: a capture source refused for privilege at startup, as
for an unelevated agent)`. Before `9e68d56`, the four install_ac_014 tests
failed on `0.1.0` against `0.1.0-dev`, and the three malformed builds then in
the matrix compiled (`FAIL build ... exit 0`). `install_ac_014_binary_secure_path`,
added after a review, was run red only at the close (2026-11-01), after
`9e68d56` had landed: a deviation from compensating control §1's order
(red before the code), found by the close review. It ran in a temporary
worktree at `fd51dca`, the tree without the implementation, with `9e68d56`'s
test files: all five tests failed for the missing behaviour, that one at
`install_ac_014_version.rs:229` with `left: Some("0.1.0")` and
`right: Some("0.1.0-dev")` (`test result: FAILED. 0 passed; 5 failed`). A
mutation reading the variable at run time also made it fail.

Not implemented this session: install_ac_015 (PE version resource, MSI limits,
setup/MSI/raw payload agreement, asset checksums and provenance), the service mode, SYSTEM custody (#87), the
installer, and every other SPEC-024 criterion. F2 is in progress, not done.

## Decisions

### Anticipated, delegated

- **Version mechanism.** `option_env!` in one module, with a `const` assertion
  that fails the build on a malformed input; rustc records the variable in its
  dependency info, so cargo rebuilds on set, change and removal. Rejected a
  `build.rs` (another build step for the same effect) and a runtime read
  (forbidden by §1).
- **Test of the embedded value.** The expected value is derived from the
  contract at run time; a matrix script runs the version tests without the
  input, with `7.8.9`, `7.8.10` and without it again in one target directory,
  and checks malformed inputs fail. Rejected nested cargo invocations from a
  test (target-directory lock) and asserting `AGENT_VERSION` against itself.
- **Lifecycle field.** `agent.version` on "agent starting"; the line's other
  SPEC-001 fields are #137, rather than widening #131.
- **#4/#37 with the first capture-gate change.** A stacked PR below #131's.
  Rejected folding them into #131's commit.

### Reactive corrections

- **Version form.** The first review of #131's code found the leading-zero
  check stricter than §1's text. Canonical `X.Y.Z` with no leading zero was
  then written as SPEC-024's amendment, which landed before the code; MSI's
  numeric limits stay with packaging (install_ac_015). Rejected checking MSI
  limits in the build (not the contract's split) and accepting leading zeros
  (two spellings of one product version).
- cargo stopped at the first failing test binary, so install_ac_014 had not
  run in the first red matrix: `--no-fail-fast` in the script.
- Reviews of the SPEC-006 amendment (five passes): the marquee never wrote
  NFR-006-005's line (added); the margin was waited on the host's clock while
  the read applies it on ClickHouse's (now polled on ClickHouse's clock);
  "other marquees wrote earlier" was false (vitest orders files from its
  cache); the Win32 requirement covers only the probe and the child; "runs
  only elevated" became "passes only elevated"; a quotation of `SPEC-018:211`
  was not literal.
- Reviews of the marquee code: elapsed line in a `finally` with the measured
  value; capture causes in the missing-Launch messages; the panic check no longer
  prints the agent's stderr (SPEC-020 §Operational §11); CLAUDE.md's
  "indistinguishable from a dirty watermark" made historical.
- Reviews of #131's code: a mutation reading the variable at run time passed
  the in-process tests, so a binary-level test with a decoy in the agent's
  environment covers enrollment (and, on Linux, the secure heartbeat); the
  elevation check reads `TokenElevation` instead of `net session`; the matrix
  moved after the Windows `--ignored` step and joined the local Rust gate;
  `07.8.9` added; stale comments and the workflows README fixed.
- Reviews of the SPEC-024 amendment (five passes): "no tag exists" was false
  (two `archive/` tags; now "no version tag"); the images and bundle jobs
  still publish under a malformed tag (stated; #139); the Effect names what it
  narrows; a sentence describing tests not yet in its tree became a
  requirement.
- The two-tree gate: the SPEC-006 amendment requires a green gate on A's own
  tree and #131 changes the secure path, so `s50-gate1.ps1` checked out each
  commit detached and ran the full gate twice.

### Manuel's decisions

None beyond the opening delegation quoted above. He ran the elevated gate
("gate terminado", 2026-11-01): that was P2, the gate script, proposed with
P1, P3 and P4, which wait for his answer.

## Self-review and fresh-context reviews

Every reviewer got CLAUDE.md's literal prompt with only paths and SHAs, in an
isolated worktree. Branch, status and worktrees were checked after each; no
reviewer change landed. Reviewers shared the session scratchpad (one wrote
`matrix.log`, a name this session did not use for evidence).

| Change | Passes | Final |
|---|---|---|
| SPEC-006 amendment | 5 contract | List A empty at `de3bf0c` |
| Marquee (`7297412`) | 3 code | One contradiction recorded as #138 (no elapsed line when vitest's timeout abandons the test or the run fails before the probe, against SPEC-006's "on every run"); weak points below |
| SPEC-024 amendment | 5 contract | List A empty at `fd51dca` |
| #131 (`9e68d56`) | 4 code | No contradiction; weak points below |

Own matrix, #131 (install_ac_014 and §1):

| Criterion | Test | Verdict |
|---|---|---|
| CLI shows the version, no config | `install_ac_014_version.rs` `install_ac_014_cli_version` | covered (decoy in env) |
| Enrollment | `install_ac_014_enrollment_request`, `install_ac_014_binary_secure_path`, `enroll_ac_003` | covered (binary with decoy on Linux CI and an unelevated Windows terminal; the elevated Windows CI job skips the binary check) |
| Plain heartbeat and lifecycle | `install_ac_014_plain_heartbeat_and_lifecycle` | covered (binary with decoy) |
| Secure heartbeat | `install_ac_014_secure_heartbeat`; binary on Linux | covered; weak on Windows (binary skipped elevated, exits 9 unelevated) |
| Dev value; rebuild set/changed/removed | matrix steps | covered (CI Linux and Windows) |
| Malformed or empty fails the build | matrix (`""`, `v7.8.9`, `7.8`, `07.8.9`); `version.rs` unit tests | covered |
| Release job: tag's version, exact match | `release.yml` | weak: not runnable while tags are frozen; first-release evidence |

Own matrix, marquee: amendment items 1–4 and the elapsed line map to the
test's sections (1)–(4) and its `finally`; all ran green in the elevated gate.

Dispositions not fixed, recorded (each with why it does not block):

- The marquee's elapsed line is missing when vitest's timeout abandons the
  test, and before the probe's spawn there is no interval to measure; every
  run that reaches the probe and ends inside the test writes it, and the
  gate's runs did (#138).
- The probe is found by PID alone, and a clock step after the settle wait
  could end the drain early; the gate's runs captured both Launches and
  drained in one cycle (#138).
- "ParentImage resolved" is implied by the rule match, as before this change.
- #131 secure heartbeat on Windows (weak): the binary check is skipped
  elevated and the agent exits 9 unelevated; the value is the one constant
  every path reads, Linux CI checks the binary's secure heartbeat, and the
  in-process test runs on Windows.
- #131 release job (weak): not runnable while tags are frozen; the build's
  refusal and the `--version` comparison it relies on are exercised by the
  matrix, and the step's first run is first-release evidence.
- No criterion asserts the tag-form check before publication (#139); the
  freeze stops every `v*` tag.

Also recorded: `rust-ci` uses the
runner's Node, not pinned, while its actions are pinned by major tag (#113;
the script uses Node's standard library only); on
Windows `set X=` and `$env:X=''` delete a variable, so a hand-run "empty"
build becomes `-dev` (the release job's `--version` check catches it); the
envelope version is asserted only without the build input (separate
constants).

## Gates and CI

### Elevated gate s50-gate1 (Manuel, run 2026-10-31 23:14–23:35 +01:00, reported 2026-11-01; logs `C:\tmp\s50-gate1-*`)

`net session exit: 0`; start branch clean. Per tree:

```text
== tree A
HEAD 729741231f75169c94f2c47606148a66fddedb8c
tree da7aca0dbe927b8e6d18b26301778e69744698a5
status lines: 0
service CyberGuardAgent: absent
binary fresher than sources: True
version: cg-agent 0.1.0
cargo test exit: 0
 Test Files  85 passed (85)
      Tests  503 passed | 2 skipped (505)
pnpm test exit: 0
detect_ac_001 drained 64 rows in 1 cycles; capture checked at 35.278 s, drained at 38.059 s
{"level":"info","detect_marquee_elapsed_seconds":38.074,"alerts_read":true}
== tree B
HEAD 9e68d5692d14ddf18305db9c4dfdc3709e32d654
tree 4187cd3426d2d2dd841a7d285b36ba3d91f4e882
status lines: 0
service CyberGuardAgent: absent
binary fresher than sources: True
version: cg-agent 0.1.0-dev
cargo test exit: 0
 Test Files  85 passed (85)
      Tests  503 passed | 2 skipped (505)
pnpm test exit: 0
detect_ac_001 drained 56 rows in 1 cycles; capture checked at 35.278 s, drained at 40.185 s
{"level":"info","detect_marquee_elapsed_seconds":40.199,"alerts_read":true}
```

Each tree's `--ignored` run passed the seven tests (`auth_ac_012`,
`capture_ac_006`, `net_ac_008`, `net_ac_009`, `process_ac_004`,
`process_ac_007`, `process_ac_009`). The two `image_file_name` lines:

```text
A: detect_ac_001 captured image_file_name — probe: C:\Users\manul\AppData\Local\Temp\cg-detect-probe-FZsgsN\winword.exe; child: C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe
B: detect_ac_001 captured image_file_name — probe: C:\Users\manul\AppData\Local\Temp\cg-detect-probe-SCyPmz\winword.exe; child: C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe
```

In both trees: other-rule
alerts 0, all five marquees passed, no `CGAgent` session left, service
`CyberGuardAgent` absent. The two skips are the same as S46's
(`servercert-ac-renewal`, `deploy-ac-014`). Landing guards compared each
gated commit's tree with its gated tree (the docs-only commits had none); the PowerShell `NativeCommandError`
lines wrap git's and cargo's informational stderr.

### Local gates

- Rust at `f7eac42` (identical to `9e68d56` in `agent`, `.github`,
  `CLAUDE.md`, `services`): fmt and clippy clean; `cargo test --all` 67 test
  binaries and the doc-tests (S45's baseline, inherited by S49, was 66
  binaries; one binary added), 146 passed / 7 ignored; `agent version matrix: all steps
  passed`.
- Ingest unelevated at `ccc93d1` (`agent/cg-agent/src` and `services/`
  identical to `9e68d56`): typecheck, lint,
  build, entrypoints ok; `Test Files 4 failed | 80 passed | 1 skipped (85)`,
  `Tests 4 failed | 498 passed | 3 skipped (505)`: the four capture marquees
  with exit 9, identical to S47's baseline.
- Markdown: markdownlint-cli2 0.23.2, 0 issues on each changed file.

### CI (full SHAs queried)

| Commit | PR runs | Main runs | Verdict |
|---|---|---|---|
| `de3bf0c` | md `41225624424` | md `41225668372` | ALL GREEN |
| `7297412` | ts `41225723945`, md `41225723944`, deploy `41225723935` | md `41250867930`, ts `41250867936` | ALL GREEN |
| `fd51dca` | md `41227072236`, deploy `41227072274`, ts `41227072423` | md `41253077329` | ALL GREEN |
| `9e68d56` | md `41227799936`, ts `41227799884`, rust `41227799996`, deploy `41227799905` | md `41253109187`, ts `41253109174`, rust `41253109166` | ALL GREEN |

`deploy-ci` runs on pull requests, weekly and on demand, not on a push to
`main`. Both rust-ci jobs ran the version
matrix. A background CI poll was stopped by the harness under memory
pressure; the same status was read in the foreground. PRs were created over
REST with the configured credential; no token was printed.

### Test baselines at close

Rust 67 test binaries plus doc-tests / 146 passed / 7 ignored; ingest elevated 85 files / 503
passed / 2 skipped; ingest unelevated as above. API and dashboard unchanged
since S47 (56 / 390, 12 / 141), not rerun locally; deploy not rerun locally,
deploy-ci green on PRs #73–#75 (it reads `release.yml`).

## Debts

Closed: **#4** and **#37** by `7297412` (contract `de3bf0c`), green in s50-gate1.

Narrowed: **#131**, install_ac_014 by `9e68d56`; install_ac_015 remains with
the packaging change and waits for P1.

New:

- **#137 — "agent starting" lacks SPEC-001's other fields.** It carries
  `agent.version` only. Destination: F2's service logging change
  (install_ac_010), for the console line as well.
- **#138 — `detect_ac_001` residuals.** A ClickHouse clock step after the
  settle wait can end the drain early; the probe is found by PID alone; no
  elapsed line on a vitest timeout (against SPEC-006's "on every run") or
  before the probe; the test's header comment still says the other marquees
  wrote earlier. Destination: the next change to the marquee or the gate.
- **#139 — no criterion orders the version check before publication.**
  install_ac_015 requires release packaging to reject invalid and
  out-of-range versions, but the images and bundle jobs do not wait for the
  agent's: they push and attest even when the agent build refuses a malformed
  tag, and an in-form tag beyond MSI's limits (`v300.0.0`) passes every job
  today, so the GitHub Release would be created. Destination:
  install_ac_015's release-workflow change, after the SPEC-024/SPEC-023
  amendment that orders the check before any publication. The freeze holds
  meanwhile.

Open at close (49): #5, #7, #21, #24, #25, #26, #29, #31, #32, #35, #38, #39, #42, #50, #51, #52, #53, #54, #56, #58, #59, #61, #62, #64, #65, #66, #73, #83, #87, #88, #89, #97, #98, #99, #102, #110, #113, #114, #119, #124, #125, #130, #131, #132, #133, #136, #137, #138, #139.

Debts #50 (isolated fault test) and #59 (Windows 10 / Server 2019 execution) are
unchanged: no evidence from this development host substitutes for them.

## Waiting on Manuel

- **P1, packaging tool (SPEC-024 Open question 3; licence, fee, toolchain).**
  Recommended: WiX v7, exact patch pin, as a .NET local tool with
  `-acceptEula wix7`; the OSMF fee applies above US$10k annual revenue
  (FireGiant's OSMF page). Needs .NET SDK 8+ in the Approved local toolchain.
  Alternative: WiX v5.0.2 (MS-RL, predates the fee, not the current line).
  Burn's binaries in `setup.exe` join #130's notices. The decision also covers
  how the PE version resource is written: a resource step in the agent's
  build is a new build dependency of `cg-agent`, his decision too.
- **P3, where real SCM/SYSTEM tests run.** Proposed: `#[ignore]`d tests that
  create a uniquely named temporary service and always delete it, run in
  rust-ci's Windows job (built-in Administrator, disposable runner); on his
  machine only if he agrees. The gate's `--ignored` run would include them,
  so they need a filter of their own (a name prefix the gate skips).
- **P4, SPEC-024 Open questions 4–5 before F2 endpoint validation.** SPEC-024
  asks for a proposed disposition; it touches what the product tells an
  operator about its own coverage (the security posture), so it waits for him.
  Proposed: keep ADR-0019/0020's boundaries (no agent audit-policy reporting,
  no product dump writer); the endpoint runbook inspects read-only `auditpol`
  by subcategory GUID, `CrashControl` and WER `LocalDumps`, recorded with the
  installation evidence; agent reporting reopens with F3's fleet view. No ADR
  change.

## Environment and memory

C: had 41.10 GiB free at start and 31.03 to 33.15 GiB during the gate; host free RAM
4.93 GiB at start, 0.56 to 2.8 GiB with Docker and builds. Docker Desktop was
not running at start and was started (Engine 29.1.2), an allowed operation;
the five `cyberguard-dev` containers came up healthy; other projects were
untouched. No tool was installed. Builds used `CARGO_INCREMENTAL=0` and
`-j 2`. Auto-memory gets `project-session-50-close.md` and its index line; the
elevated-gate reference records the two-tree pattern.

## How Session 51 resumes

1. Start protocol (CLAUDE.md). Recheck C:, memory and Docker before builds;
   with Docker up, `-j 2`; keep background jobs few (the harness reaps them
   under memory pressure).
2. If Manuel answered P1/P3/P4, record his words and act: P1 yes → add the
   toolchain rows, then install_ac_015 (PE resource, MSI limits, package
   agreement) under `deploy/windows/`, and #139's pre-publication check after
   the SPEC-024/SPEC-023 amendment that states it as a criterion.
3. Otherwise continue F2 without the package (real-SCM acceptance evidence
   for install_ac_007/009 waits for P3; F2's endpoint validation for P4): the `--service` SCM mode with
   STOP/SHUTDOWN, the 60 s stop deadline and failure status (install_ac_007),
   the single-instance guard before ETW reclaim (install_ac_009), rotated
   private JSON logs and Application-log diagnostics (install_ac_010, #137),
   and SYSTEM custody from creation (#87, install_ac_004/005/013), with
   `windows-sys` features of the existing dependency. Contract-first tests,
   one PR per change, the literal reviews again after fixes, the capture gate
   for the capture path, one gate tree per landed tree that needs it.
4. Keep #50/#59 explicit, the release freeze active until F2–F4, #130 and
   Manuel's publication approval, F3 pages/invitations and F4 renewal where
   they are. Close with handoff-51, register, README, roadmap, memory, clean
   pushed main and ALL GREEN.

## Close review

The close received the literal contract review six times. Each pass's List A
was fixed before the next; the fifth and sixth passes found List A empty. The
sixth pass's List B items on #139's wording, the Windows limit of the binary
enrollment check and the order of that test's red run were applied as wording
without a seventh pass. Recorded, not changed: the self-review matrices name
tests without `path:line` and summarize the reviewers' lists; the debt
register's "Sweep" definition points at a resume section with no sweep; the
local clone holds a third `archive/` tag that origin does not.
