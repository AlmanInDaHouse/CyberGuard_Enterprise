# Handoff — Session 72

Session 72, 2026-11-14, wrote C8's contract, SPEC-024 Amendment 2026-11-14 with the threat model's,
which Manuel approved, and its code: the release builds, checks against each other and attests the
agent's executable, its MSI and setup and WiX's source archive in one Windows job, with a dry run in
`package-ci`. It also moved tokio and rustix past their advisories (#159), after three elevated
gates. No tag was pushed: the release freeze stays.

## Start and resulting state

- Start: `main` = `origin/main` = `8df6c07`, clean, no stash, no branch but `main`, no PR; the
  tip's CI ALL GREEN (markdown-lint); 22 ADRs / 23 SPECs, 58 open debts (58 rows), no Known CI
  debt. The repo agreed with Session 71's handoff. 3.3 GB of 15.8 GB of memory free, 22 GB on
  `C:`; Docker Desktop down (the first gate started it, with Manuel's `aura_cache` and `aura_db`,
  not touched).
- Resulting `main`, in order, each pushed alone with its CI green:
  - `0e7fe6b`, tokio 1.54.1 and rustix 1.1.6 (#159), fast-forwarded from PR #132;
  - `9ad26ea`, SPEC-024 Amendment 2026-11-14 and the threat model's, approved by Manuel;
  - `c0ad344`, `671f641`, `0eb51cb`, `df48f45`, C8's code, fast-forwarded from PR #133;
  - this close's commit.
- PRs #132 and #133 show merged; their branches are deleted. No other branch is left.
- Catalogs: 22 ADRs / 23 SPECs, unchanged in number; SPEC-024's catalog gains the amendment's line.

## What landed

### #159 (`0e7fe6b`)

`cargo update -p tokio -p rustix`: tokio 1.52.3 → 1.54.1 (RUSTSEC-2026-0386), rustix 1.1.4 →
1.1.6 (RUSTSEC-2026-0486), within their ranges. errno, tempfile and rustix re-resolved to
`windows-sys` 0.52.0 (test code) and nu-ansi-term to 0.61.2 (linked by the agent through
tracing-subscriber), versions already in the lock; no crate entered the lock or what the agent
compiles. `cargo audit` prints one allowed warning, RUSTSEC-2026-0440 (astral-tokio-tar, test-only
through testcontainers 0.27, no patched release), which Manuel accepted as #159 on 2026-11-14.
`docs/security/dependency-review.md` and #159's row say so.

### The contract (`9ad26ea`)

SPEC-024 Amendment 2026-11-14 (ten points) and the threat model's Amendment 2026-11-14: the agent
job makes the four Windows assets in one run with the pins verified and no cache (1); checks before
it attests, installing nothing: the executable's version resource, imports and message table,
`cg-setup.exe`'s version, the MSI's version, its one file, no `Binary` table and no custom action
(#158), the setup's version and copyright, and with the pinned `wix` tool the payloads' bytes (2);
`wix-<release>-source.tar.gz`, WiX's whole tree at the pinned commit and tree IDs plus the setup's
own sources and the theme's `LICENSE.TXT` (3); WiX's notices and `LICENSE.TXT` in the licence
report, which also reads `cg-setup`'s crates (4); six payload assets (5); `package-ci`'s dry run
(6); `ubuntu-24.04` and `windows-2025` (7, #149); a development build's pre-release flag (8, #140);
"unmodified" read as the engine's code (9); the setup's copyright keeping WiX's line (10). The
threat model corrects the Amendment 2026-11-03's "extension's custom actions", updates two living
lines (`:294`, R12) and accepts the SHA-1 residual of the archive's pin.

### C8's code (PR #133)

- `c0ad344`: `build_resource.rs` sets `VS_FF_PRERELEASE` in a development build of `cg-agent.exe`
  and `cg-setup.exe`; install_ac_014's and `package_setup_executable`'s tests read it in both
  builds through the version matrix (#140).
- `671f641`: `Bundle.wxs`'s copyright (point 10) and comments (point 9); WiX's `LICENSE.TXT` beside
  the theme (blob `d4d316e`, LF), the theme headers placing it; the allowlist's WixToolset.Sdk
  reason; `check-licences.mjs` reads cg-agent's and cg-setup's crates and writes the report's WiX
  section.
- `0eb51cb`: `.github/scripts/release-windows.mjs` `check` (14 checks) and `archive`, pure-JS PE
  readers, msi.dll through pwsh read-only, `wix burn extract` and `wix msi decompile -x`; the
  archive fetched by commit with git's object checks, isolated from git's system and global
  configuration, verified entry by entry and by the recomputed tree ID; `pins.json`'s `wixCommit`
  and `wixTree`; `release-windows.test.mjs` (real files and mutated copies, a synthetic delay-load
  PE) and `release-check-matrix.mjs`.
- `df48f45`: `release-windows.mjs build` (absolute paths, two cargo builds, the build environment
  scrubbed, empty `--out` and `--work`, a clean tree, the report carrying the archive's licence);
  `release.yml`'s agent job on `windows-2025` (the build step without the OIDC request variables),
  six assets in `SHA256SUMS` and the notes; `package-ci`'s `release` job and triggers; deploy-ci's
  dry run on `ubuntu-24.04`; `release-workflow.test.mjs`, `release-build.test.mjs`; the READMEs,
  `install.md`'s list, `dependency-review.md`, CLAUDE.md's local gate.

## Decisions

### Manuel's (2026-11-14, in chat)

- #160: no answer this session; it stays waiting.
- The archive: "Árbol entero + tema (Recomendado)"; the notices: "En licences-<v>.md
  (Recomendado)"; #159's astral-tokio-tar: "Aceptar como deuda (Recomendado)"; WiX's copyright line:
  "Conservarla en el recurso (Recomendado)".
- The amendment's text at `9a3b7ef`: "doy el si".

### Anticipated (Claude Code)

- **ADR-0022 §7 unchanged**: its release "also carries the agent's executable" excludes nothing;
  the archive's upstream fetch is named in the amendment for Manuel's eye. Discarded: an ADR
  amendment.
- **One Windows job** for the four assets, keeping SPEC-023 §12's jobs and grants. Discarded: a new
  job or grant (Manuel's).
- **Payloads compared with the pinned `wix` tool**, nothing installed. Discarded: `msiexec /a` (an
  installation's venue question).
- **The archive built from a git tree** (`read-tree` + `write-tree` + `git archive`), verified by
  re-reading it. Discarded: a vendored copy; a hand-written tar.
- **The setup's own sources in the archive** (`Bundle.wxs` and its build files), added after the
  first review for MS-RL's "source code to that file". Recorded in the amendment.
- **The implementation by three implementers in worktrees, then one for the review's fixes**, each
  with red evidence, integrated and autosquashed by Claude Code.

### Reactive corrections

- **WiX's `LICENSE.TXT` line endings**: the second review read CRLF (a Windows checkout); the blob
  is LF (0 CR, hash recomputed); the comparison is byte for byte, and the archive's tree is
  verified against the pin.
- **Gate 1's `ac-001-marquee` failure** ("the agent's ETW session CGAgent-KernelProcess is still
  running after the run and could not be stopped", exit code null): not reproduced in 16 isolated
  runs on two trees (gate 2) nor in a full gate 3; recorded as #161.
- **The relative `--out`** in `release.yml`, which MSBuild resolves from `deploy/windows`: the code
  review's List A; `build` resolves its paths and the dry run passes the same relative form.
- **The static no-cache test** let `actions/cache/restore`, `Swatinem/rust-cache` and `cache:
  pnpm` through: an allowlist of actions and of cache keys.
- **The delay-load reader** was never seen refusing: a synthetic PE test.
- **cg-agent built alone**, before cg-setup, so that feature unification never changes the released
  agent.
- **A copyright check stricter than the contract** (the "Burn engine:" prefix), and a full commit
  in the report instead of the short one: the first audits.
- Five review passes over the amendment (fixed prompt), plus three lenses on the first.

## Self-review (CLAUDE.md compensating control 5)

### The amendment (text prompt)

Five passes of the fixed text prompt, the first with three lenses (implementability, licensing,
coherence). Their List A, each fixed before Manuel read the text: the Burn engine's resource
rewritten by WiX's binder (point 10, his choice); the theme as two files and its `LICENSE.TXT`;
`OSMFEULA.txt` at the tree's root; the MSI's files and `Binary` table checked; the setup's own
version; a misattributed quote and the SHA-1 wording; local red evidence instead of a probe;
packaging on Manuel's list; #130's moved readings; R12 true before and after #132; the
"unmodified" family across three earlier amendments. The fifth pass's one borderline item, the
twin sentence in `dependency-review.md`, was #132's, which landed first.

### #159 (code prompt, two passes)

First: the register and the runbook disagreeing at the SHA, the `windows-sys` re-resolutions
undisclosed, which the agent compiles unstated; fixed in the same commit (`0e7fe6b`). Second: no
List A; the elevated gate pending (then gates 1 to 3); errno's test-only edge true on Windows alone,
which this close's commit writes in `dependency-review.md`.

### C8's code (code prompt, two passes, three lenses)

Claude Code's matrix (criterion → test → verdict), against the amendment:

| Point | Test | Verdict |
|---|---|---|
| 1 one job, pins before WiX, no define, no cache | `release-build.test.mjs` (steps, order, cargo twice, `-p:CgInjectFailure=`), `release-workflow.test.mjs` (allowlists of actions and cache keys, rustup) | covered |
| 2 the executable, `cg-setup.exe`, the MSI, the setup, the payloads | `release-windows.test.mjs` (real files and mutated copies, a synthetic delay-load PE), `release-check-matrix.mjs` (exact sets of failing checks) | covered; payloads outside a container and a second string table weak (below) |
| 3 the archive | `release-windows.test.mjs` (crafted archives, a local upstream, wrong tree and tag, git isolation), `pins.test.mjs` | covered |
| 4 the report's section, cg-setup's crates | `licences.test.mjs`, `deploy/docker/test/licences.test.mjs` | covered |
| 5 six assets, four attested, the notes | `release-workflow.test.mjs` | covered |
| 6 the dry run | package-ci's `release` job on PR #133 (every step green) | covered |
| 7 runners | `release-workflow.test.mjs` | covered |
| 8 the flag | `install_ac_014_version.rs`, `package_setup_executable.rs`, the version matrix | covered |
| 9, 10 texts, copyright | `projects.test.mjs`, `licences.test.mjs` | covered |

The reviewers: the first pass's List A (the relative `--out`, the cache test, the delay-load
reader) and its chosen List B items were fixed in the series before it was pushed; the second pass
found no contradicted criterion, two weak ones and the unasked hardening (the OIDC request
variables blank in the build step, the environment scrubbed, git isolated), recorded here.

Recorded, not blocking:

- **Payloads outside a container and a second string table** are neither compared nor read;
  neither occurs in today's build (#162).
- **`MsiEmbeddedUI`, `MsiEmbeddedChainer`, a `DownloadUrl` package, and the engine and
  `wixstdba.exe` against the nuget content**: defence in depth beyond point 2's wording (#162).
- **`images` runs alongside `agent`**, so a failed Windows job can leave attested images of a
  version with no release, as it could before; changing the order is packaging (#162, Manuel).
- **A rerun of the agent job** attests a second, different setup for the same tag (`CgBuildId`);
  the publish job takes one run's artifacts.
- **.NET's cache off** rests on setup-dotnet's default; the cache-key allowlist refuses any key.

## Gates and CI

- Elevated gate, #159 (`0e7fe6b`, tree `0649842`), in the worktree `cg-s72-deps`:
  - gate 1: `net session` exit 0; release exe `2026-11-14T16:02:14`; `--ignored` 7 passed (and
    `classify_token`); vitest "Test Files 1 failed | 85 passed (86)", "Tests 1 failed | 503
    passed | 2 skipped (506)": `ac-001-marquee`, "the agent's ETW session CGAgent-KernelProcess is
    still running after the run and could not be stopped … agent exit code null"; no session left
    after the suite (#161).
  - gate 2: `ac-001-marquee` alone, 8 times on `0e7fe6b` and 8 on `8df6c07` (its own release
    build): "8 passed, 0 failed of 8" each; no session after any run.
  - gate 3: gate 1's script on the same tree: "Test Files 86 passed (86)", "Tests 504 passed | 2
    skipped (506)"; `--ignored` 7 passed; "detect_ac_001 drained 48 rows in 1 cycles",
    `detect_marquee_elapsed_seconds` 39.139; SPEC-005 "launch: C:\Windows\System32\cmd.exe"; no
    session left. #132 landed on gate 3's evidence, its tree the tested one.
- No elevated gate for C8: `build_resource.rs` and the package are outside §5's paths, and no lock
  file changed.
- Local Rust gate (`6ed7991`, whose Rust is `c0ad344`'s): fmt and clippy clean; `cargo test --all`
  "binaries 97 passed 569 failed 0 ignored 11"; "agent version matrix: all steps passed".
- Local package gate: "pins verified: 4 packages at 7.0.1"; "msi version matrix: all cases
  passed"; "bundle version matrix: all cases passed"; "release check matrix: all cases passed";
  `node --test "deploy/windows/test/*.test.mjs"` with the built files "tests 84, pass 84, fail 0,
  skipped 0"; the end-to-end `build` with a relative `--out`: 14 checks PASS and "release build:
  cg-agent-1.2.3-x86_64-pc-windows-msvc.exe, cyberguard-agent-1.2.3-x64.msi,
  cyberguard-agent-1.2.3-x64-setup.exe, wix-7.0.1-source.tar.gz".
- CI: PR #132 `0e7fe6b` ALL GREEN; `main` `0e7fe6b` ALL GREEN; `9ad26ea` (markdown-lint) ALL
  GREEN. PR #133: `c0ad344`, `671f641`, `0eb51cb`, `df48f45` each ALL GREEN, pushed one per push;
  `df48f45`'s package-ci `release` job, the dry run, green in 8 minutes ("PASS the release's
  assets passes all 14 checks", "release check matrix: all cases passed"). `main` after each
  fast-forward, the runs of its push: `c0ad344` ts-ci, rust-ci, package-ci, ALL GREEN; `671f641`
  dependency-audit, package-ci, ALL GREEN; `0eb51cb` dependency-audit, package-ci, ALL GREEN;
  `df48f45` markdown-lint, dependency-audit, package-ci, ALL GREEN; this close's commit, as its
  post-push report gives it.

## Test baselines at close

- Non-elevated `cargo test --all`: 97 binaries; 569 passed, 0 failed, 11 ignored.
- Elevated: `--ignored` 7 (and `classify_token`); vitest 86 files, 504 passed, 2 skipped.
- `deploy/windows/test`: 84 tests with the built files (8 skip without them).
- `package-ci`: a fifth job, `release`.

## Debts

- Closed: **#131** (`df48f45`: install_ac_015 for the release's code; the published provenance is
  first-release evidence), **#140** (`c0ad344`), **#149** (`df48f45`: release.yml and deploy-ci's
  dry run pinned to `ubuntu-24.04`, SPEC-023's line by scope; the cause stays unknown, and a move
  to another image is a reviewed change of both), **#158** (`df48f45`).
- Narrowed: **#130** (its WiX part met by `671f641` and `df48f45`; five legal readings of MS-RL move
  to its first-release part), **#159** (`0e7fe6b`: astral-tokio-tar alone, accepted).
- Opened: **#161**, gate 1's `ac-001-marquee` failure, unreproduced (0 of 17 since): the harness
  ends the agent at 4 s and checks the session once after `logman stop`; destination: a stop that
  waits for the session to go, with a bound, when the harness next changes or the failure recurs.
  **#162**, the release's defence-in-depth residuals and the images job's order (above);
  destination: before the first release, the order to Manuel.
- Open at close: **56**.

## Waiting on Manuel

- **#160's text**, as Session 71 put it.
- **#162's order**: whether the images job waits for the agent job, so that a failed Windows build
  pushes no image of its version (recommended: yes, `needs: [licences, agent]`, which delays the
  images' start by the agent job's length on each release).
- The release freeze stays; Authenticode stays ADR-0022's Open question 1.

## How Session 73 resumes

1. Run the start protocol (CLAUDE.md). `main` is this handoff's close commit. There is no PR. The
   counts are 56 open debts and 22 ADRs / 23 SPECs.
2. Manuel's answers on #160 and #162, if given.
3. **C9, purge** (install_ac_012's decision), as Session 58's plan lists it: contract first, its
   tests in `package-ci` only, red on a probe pull request.
4. Then C10, the runbooks: `install.md`'s endpoint steps on the setup, `upgrade.md`, the migration's
   runbook (#87's items), #150's, the residuals of Sessions 69 to 71, and the endpoint verification
   of the installed binary's digest (§Data contracts §1).
5. Keep the release freeze.

## Environment and memory

- `target\release\` holds 1.2.3 builds from this session's local gates; none is a release.
- Docker Desktop stayed up with Manuel's `aura_*` containers alone, not touched.
- Background shells were reaped twice under memory pressure; the CI waits ran under Monitor.
- `C:\Users\manul\cg-s72-deps`, the elevated gates' worktree, is pruned from git but left as an
  empty directory that another process holds; remove it once nothing holds it.
- Updated memory: the session's close; `reference-ci-waits-under-memory-pressure.md`.
