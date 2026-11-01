# Handoff — Session 51

Session 51, 2026-11-01, continues F2 under SPEC-024. It lands Manuel's three
delegated decisions as SPEC-024's Amendment 2026-11-01 (P1 packaging tool
and the executable's version resource, P3 where the real service and SYSTEM
tests run, P4 Open questions 4 and 5), #139's release version check, and
the executable's PE version resource. The service mode, the instance lock,
the service logs and SYSTEM custody are not implemented. The service's
outcomes for install_ac_007, install_ac_009 and the Application-log half of
install_ac_010, the second amendment, are parked on a branch because they
change two ADRs; custody (#87) and the file logs keep their Accepted
contract in SPEC-024 §Operational §2 and §3.

Manuel's delegation, given after S50's close and recorded in auto-memory:
"esas opciones la decida claude code en beneficio del proyecto" (P1, P3,
P4). ADR changes, Authenticode, publication, payments and accepting a EULA
in his name stay his. The session's opening prompt was a text S50 wrote for
him; it was read as input (CLAUDE.md *Integration path* §5), and what binds
is what he said himself.

## Start and resulting state

- Start: `main` = `origin/main` = `c2230915eda81d414956be5df20c6ca3f63e2712`,
  clean, empty stash, one worktree, no open PR. The tip's runs were green
  (markdown-lint `41260417496` push, `41260376949` PR). Catalogs 22 ADRs /
  23 SPECs; open debts 49; Known CI debt none. C: 33 GB free, RAM 2.0 GB
  free, Docker Desktop up (Engine 29.1.2).
- Landed, in order, each one commit per push with CI green before the next:
  `46c6cb5` (docs, straight to `main`), `d9932a6` (PR #77), `cf59c12`
  (PR #78). Each code commit landed by fast-forward of the exact commit CI
  ran, so the landed tree is the tested one. The close is a docs-only commit
  on `main`; its SHA and CI go in auto-memory and the close response.
- Branches left open, both on origin, no PR (no workflow runs on them):
  - `review/s51-claude-md-scm-tests` (`187b2a1`, `NOT YET RATIFIED`): the
    CLAUDE.md change SPEC-024's point 4 requires before any SCM/SYSTEM test
    or its probe (compensating control §1, the local gate's third
    exception, the Windows CI bullet). Waits for Manuel's review.
  - `docs/s51-service-outcomes` (`1f3979f`): the parked second amendment
    (the service's outcomes), with Manuel's two approvals of its content.
    Reference text for S52; not to be landed as is. It is cut from
    `c223091`, so its first amendment is an earlier version of the landed
    one.
- Catalogs at close: 22 ADRs / 23 SPECs (one SPEC amendment). Release
  ruleset `25923038` active; no version tag, release, service installation,
  host-policy change or reboot.

## Landed changes

| Commit | PR | Change |
|---|---|---|
| `46c6cb5` | — | SPEC-024 Amendment 2026-11-01 (P1, P3, P4, #139's order); by scope SPEC-023 §12, its first Amendment 2026-10-26 item 4, its Amendment 2026-10-27 and deploy_ac_021; CLAUDE.md toolchain rows (.NET SDK, WiX Toolset v7); roadmap Part (b) §10 resolved |
| `d9932a6` | [#77](https://github.com/AlmanInDaHouse/CyberGuard_Enterprise/pull/77) | #139: `.github/scripts/release-version.mjs`; `release.yml`'s first `version` job (`contents: read`), which `licences` needs, comparing the printed version with the tag's; deploy-ci's path filter; `deploy/docker/test/release-version.test.mjs` |
| `cf59c12` | [#78](https://github.com/AlmanInDaHouse/CyberGuard_Enterprise/pull/78) | Point 2: `agent/cg-agent/build.rs` writes the VERSIONINFO `.res` by hand and links it into `cg-agent.exe` (Windows MSVC); `install_ac_014_pe_version_resource`; the version matrix's refusals above 65535 (each position) and the 65535 edge |

## Decisions

### Delegated by Manuel (2026-11-01), taken by Claude Code

- **P1, the packaging tool.** WiX Toolset v7 at one pinned release.
  Rejected: WiX v5.0.2 (past its consumer security fixes, 2026-02-05,
  while Burn runs elevated on endpoints); WiX v6 (fixes end 2027-02-05,
  OSMF v1.0); NSIS and Inno Setup (no MSI); a hand-written MSI (more code to
  audit, no bootstrapper). The binaries are licensed to the project under
  the OSMF EULA v1.1 and redistributed under MS-RL (the EULA's §4, read on
  nuget.org's `wix` 7.0.1 licence page). The release therefore carries
  MS-RL's text in its notices (#130) and those files' source as a sixth
  `SHA256SUMS` asset. The
  allowlist admits them by name. nuget pins are checked by content hash, and
  an advisory check joins `dependency-audit`, all with the packaging change.
- **P1, the version resource.** The agent's own `build.rs`, with no crate
  and no tool. Rejected: a resource crate (a new build dependency of the
  elevated agent), `rc.exe` (an SDK tool the build would have to find),
  post-build stamping (a new tool). This reclassifies handoff-50 P1's "a
  resource step in the agent's build is a new build dependency of
  `cg-agent`": a build script with no crate adds none.
- **P3, the venue.** rust-ci's Windows job alone, under a test-only Cargo
  feature that `cargo test --all` and the elevated gate do not build
  (clippy compiles it), guarded to a GitHub-hosted runner,
  with a probe pull request as the red run. Rejected: a name filter in the
  gate's command, Manuel's machine, a mocked SCM.
- **P4.** Open questions 4 and 5 are resolved for F2 within ADR-0019's and
  ADR-0020's boundaries. The endpoint runbook inventories, read-only:
  - the Logon and Account Lockout subcategories, queried by GUID;
  - how many accounts have a per-user audit policy, without their names;
  - CrashControl, the page file and hibernation;
  - WER's `LocalDumps` and its consent and upload policy.

  Rejected: a startup audit-policy check (it changes ADR-0019's boundary),
  and the product turning dumps off (a change to host policy).

### Manuel's decisions in chat (2026-11-01)

- On the red run of CI-only tests: "Sí, PR sonda en CI (Recomendado)".
- On WiX v7's EULA: "Acepto EULA y cuota futura (Recomendado)".
- On #139's order, which SPEC-023 classes as packaging: "OK, compruébalo
  antes (Recomendado)".
- On the second amendment (service outcomes): "OK a la segunda enmienda
  (Recomendado)".
- On tokens that control ETW outside ADR-0010's model: "Rechazarlos al
  arrancar (Recomendado)".
- "sigue", mid-session.

### Anticipated (Claude Code)

- **The second amendment parked.** The instance lock needs two ADR changes:
  - ADR-0008 §Decision part 2: only a start that holds the lock reclaims,
    and an unlocked start stops only the session its own privilege failure
    left;
  - ADR-0010 §Decision part 1: ETW-control tokens outside its model are
    refused, and an unlocked start refused by an existing session becomes
    code 9.

  Rejected: more patches to the SPEC alone, since ten reviews kept finding
  ADR-level conflicts; and landing the service without the lock's
  contract, since install_ac_009 needs it.
- **#139.** A dedicated first job and script, its tests reading
  `release.yml`'s graph. Rejected: folding the check into the licence job
  (a different concern) and a YAML parser dependency.
- **Landing by fast-forward** of the exact CI-tested commit. It is
  stricter than a cherry-pick with a tree guard.

### Reactive corrections

- **The contract** (18 passes). Fixed along the way:
  - the PE strings (the version, not the printed line);
  - the code-1 and service-only causes;
  - the venue's scope against §Test scenarios;
  - the probe red run, which Manuel approved;
  - the toolchain rows;
  - #139's authority (his OK);
  - the WiX fee and redistribution licence;
  - the sixth asset and deploy_ac_021 by scope;
  - the roadmap sync;
  - a package-test clause added in pass 16 and reverted in pass 18.

  The instance lock was redesigned three times before it was parked.
- **#139** (2 passes). The first test let eight mutations through
  (`if: always()`, `continue-on-error`, `|| true`, job ids outside its
  regex); the step could fail open; deploy-ci's filter missed the script.
  The fix is an exact comparison with the tag, a rule of no condition or
  tolerated failure, and the filter. Nine mutations are now caught.
- **PE resource** (3 passes):
  - the refusal is now tested in each position, plus the 65535 edge;
  - the strings are read unaligned;
  - `+` is stripped like `-`;
  - the panic message is neutral;
  - a `VS_FF_PRERELEASE` flag on dev builds was dropped, since it is not
    in the contract (#140).

## Self-review and fresh-context reviews

Every reviewer got CLAUDE.md's literal prompt with only paths and SHAs, in
an isolated worktree. No reviewer change landed. One stash was created and
dropped inside the #139 worktree (`git stash` is repository-wide); the stash
is empty at the close.

| Change | Passes | Final |
|---|---|---|
| SPEC-024 Amendment 2026-11-01 (with the second until pass 10) | 18 contract | List A empty at `46c6cb5` |
| Second amendment (service outcomes) | 10 contract | Parked: ADR-0008 and ADR-0010 conflicts |
| #139 (`d9932a6`) | 2 code | No contradiction |
| PE resource (`cf59c12`) | 3 code | No blocking defect |

Own matrix, #139 (install_ac_015's version clause, point 3):

| Criterion | Test | Verdict |
|---|---|---|
| Absent, invalid, out-of-range tag refused | `release-version.test.mjs` refused list, CLI cases (no argument included) | covered |
| Check before every other job, `contents: read` | job-graph and permissions tests | covered (static) |
| Nothing pushed, attested or published on failure | no-condition rule; exact comparison step | covered (static); no run under the freeze |
| Mismatched versions between artifacts | none | missing: packaging (#131) |

Own matrix, point 2 (install_ac_014):

| Criterion | Test | Verdict |
|---|---|---|
| Numeric versions `X.Y.Z.0`, dev Cargo's | `install_ac_014_pe_version_resource` in four matrix steps | covered |
| Strings equal `--version`'s version | the same, against `expected_agent_version()` | covered (through the shared value) |
| Refreshed on input change or removal | matrix order without, 7.8.9, 7.8.10, without | covered |
| Component above 65535 fails the Windows build | matrix, each position; edge builds | covered |
| No crate or tool | `Cargo.toml`/`Cargo.lock` unchanged | covered by inspection |

Dispositions not fixed, recorded:

- The tag `v` alone would pass the step's comparison, but the tag filter
  `v[0-9]+.[0-9]+.[0-9]+` never starts a run for it.
- The tests read `release.yml` statically; no release runs under the freeze.
- A dev build's numbers equal the same-numbered release's (#140).
- `release.yml` checks only `--version` of the shipped executable, not its
  resource, and a `windows-gnu` build gets no resource (#131's remainder).
- The extra strings (`FileDescription`, `OriginalFilename`, `ProductName`
  "CyberGuard Agent") are not in the contract. `ProductName` matches the
  service's display name (§Data contracts §3).

## Gates and CI

No elevated gate ran: no change touched its paths. `cf59c12`'s build script
changes the bytes of every Windows binary, so no earlier elevated evidence
carries across it (CLAUDE.md *Elevated gate evidence validity*, condition 2).

Local gates:

- Markdown: markdownlint-cli2 0.23.2, 0 issues on each changed file.
- #139: `release-version.test.mjs` 6/6. Red against a stub accepting every
  tag and the workflow without the job, before review added the sixth test:
  `tests 5, pass 1, fail 4`.
  Mutations of `release.yml`: 9/9 caught.
- Rust at `cf59c12`'s tree: fmt and clippy clean; `cargo test --all` 68
  results (67 test binaries and the doc-tests), 147 passed / 7 ignored;
  `agent version matrix: all steps passed`. Red before `build.rs`: the test
  failed with `ERROR_RESOURCE_TYPE_NOT_FOUND` (1813), and the `70000.0.0`
  binary build exited 0.

CI (full SHAs queried):

| Commit | PR runs | Main runs | Verdict |
|---|---|---|---|
| `46c6cb5` | — | md `41321040240` | ALL GREEN |
| `d9932a6` | deploy `41322000118`, md `41322000183` (first push `807f291`: deploy `41321100815`, md `41321100852`) | md `41322766651` | ALL GREEN |
| `cf59c12` | rust `41326087811`, ts `41326087741`, deploy `41326087746` (earlier pushes `efa877f`, `c37e6bd` green) | rust `41326848962`, ts `41326848957` | ALL GREEN |

## Test baselines at close

Rust: 67 test binaries plus the doc-tests, 147 passed / 7 ignored. The
ingest suite is unchanged since S50 and was not rerun here. Its baselines
are 85 files / 503 passed / 2 skipped elevated, and 4 failed / 498 passed /
3 skipped unelevated. API and dashboard are unchanged.

## Debts

- Closed: **#139** by `d9932a6` (contract `46c6cb5`).
- Narrowed: **#131**, the PE resource by `cf59c12`. What remains goes with
  the packaging change (install_ac_015):
  - MSI's limits in the package;
  - the setup/MSI/raw payload agreement;
  - checking the shipped executable's resource;
  - checksums and provenance.
- Widened: **#130**, WiX's binaries in the setup and MSI under MS-RL, and
  their source asset.
- New:
  - **#140, a development build's numbers equal a release's.** The resource
    of a dev build carries Cargo's `X.Y.Z.0` and no prerelease flag, so a
    reader of the numbers alone cannot tell it from the same-numbered
    release. Destination: the packaging amendment for install_ac_015
    (propose `VS_FF_PRERELEASE`).
  - **#141, texts behind F2's decisions.** They are Manuel's to review:
    - `docs/security/threat-model.md:294` says "There is no installer", and
      the threat model has no entry for the installer's third-party code
      (Burn, custom actions), which runs elevated;
    - its `:234` still defers dumps to the deployment contract;
    - ADR-0010 says "LocalSystem … is not run in CI", false once the SCM
      tests land;
    - ADR-0019 Open question 1 and ADR-0020 Open question 4 still read
      "Reopen: the deployment contract", which SPEC-024 now meets for F2.

    Destination: Manuel's review, with S52's ADR-0008 and ADR-0010
    amendments.

Open at close (50): #5, #7, #21, #24, #25, #26, #29, #31, #32, #35, #38, #39, #42, #50, #51, #52, #53, #54, #56, #58, #59, #61, #62, #64, #65, #66, #73, #83, #87, #88, #89, #97, #98, #99, #102, #110, #113, #114, #119, #124, #125, #130, #131, #132, #133, #136, #137, #138, #140, #141.

Debts #50 (isolated fault test) and #59 (Windows 10 / Server 2019) are
unchanged; this session produced no evidence for either.

## Waiting on Manuel

1. **The CLAUDE.md owner review**, on GitHub:
   `https://github.com/AlmanInDaHouse/CyberGuard_Enterprise/compare/main...review/s51-claude-md-scm-tests`.
   No SCM/SYSTEM test or probe comes before his "sí".
2. **The ADR basis of the parked second amendment.** S52 drafts it on an
   owner-review branch:
   - ADR-0008: only a start holding the instance lock reclaims, and an
     unlocked start stops only the session its own privilege failure left;
   - ADR-0010: tokens that control ETW outside its model are refused, and an
     unlocked start refused by an existing session is code 9.

   His sí on those texts is needed; his approval of the content is quoted
   above.
3. **#141's texts**: the threat model's missing entry for the installer's
   elevated third-party code, its dump deferral, ADR-0010's CI sentence, and
   the reopen conditions of ADR-0019 Open question 1 and ADR-0020 Open
   question 4.
4. **First-use confirmations**, when packaging starts: the .NET SDK by
   winget (system `PATH`) and WiX from nuget.org.

## How Session 52 resumes

1. Start protocol (CLAUDE.md). Recheck C:, memory and Docker before builds;
   `-j 2`, `CARGO_INCREMENTAL=0`, debug info off.
2. If Manuel said "sí" to `review/s51-claude-md-scm-tests`, land it as
   *Integration path* §3 says, with the marker stripped.
3. Draft ADR-0008's and ADR-0010's amendments on an owner-review branch,
   with `docs/s51-service-outcomes`'s second amendment reconciled with them.
   Contract reviews, then Manuel's sí; the ADRs land before the SPEC
   amendment, and that lands before code.
4. Then install_ac_009, the instance lock:
   - the mechanism: a private namespace with an Administrators boundary
     (S51 probe: unelevated `CreatePrivateNamespaceW` is refused with
     error 5), and the mutex's existence as the lock;
   - the tests: an unelevated leftover gives 9 and survives; elevated, a
     held lock refuses; a second binary exits 10 before enrolling; an
     orphan is reclaimed while another session runs on;
   - the red run on a stub tree in the elevated gate script, then the full
     gate on the landing tree.
5. Then install_ac_007 (the service), install_ac_010 with #137 (logs), and
   #87 (custody). The SCM tests run in CI after the probe pull request.
   Packaging (install_ac_015, #131, #140) starts once Manuel confirms the
   first use of the .NET SDK and WiX.
6. Keep #50/#59 explicit and the release freeze active. Close with
   handoff-52, the register, README, roadmap, memory, a clean pushed `main`
   and ALL GREEN.

## Environment and memory

C: had 33–36 GB free; RAM 2–3 GB free with Docker up. No tool was
installed, and no Docker container was started or stopped. Two read-only
probes ran on the host:

- `CreatePrivateNamespaceW` with an Administrators boundary, unelevated:
  `ERROR_ACCESS_DENIED` (5). The namespace is transient.
- `auditpol /list /subcategory:* /v`, which confirmed the Logon and
  Account Lockout GUIDs.

Auto-memory gets `project-session-51-close.md` and a feedback entry: split
an amendment that crosses ADR boundaries early.
