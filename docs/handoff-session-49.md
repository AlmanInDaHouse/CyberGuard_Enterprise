# Handoff — Session 49

Session 49, 2026-10-27, starts F2 with its installer/service contract and
resolves the gate-rule conflict before any runtime change. Manuel delegated
decisions in the project's interest; ADRs still need his explicit yes, and
he runs elevated gates. No ADR was changed or ratified in this session.

## Start and resulting state

- Start: `main` = `origin/main` = `cbe40c9f15db0580b46459e21f054353a0247133`,
  clean, empty stash, only the root worktree and `main`, no open PR. Both
  Markdown runs on that SHA were successful (`40428817454`, `40428911283`).
- Catalogs: 22 ADRs / 22 SPECs at start; 22 / 23 after SPEC-024. Known CI
  debt: none. Open debts: 49 at start, 48 at close.
- #46 is closed by `f07fa39`, before F2 implementation. SPEC-024 and four
  accompanying SPEC amendments landed as `765e122`. F2's implementation
  remains pending; #87/#131 are contracted, not fixed in executable code.
- No runtime, workflow, dependency or executable runbook changed. No Rust,
  TypeScript, package or elevated gate was run or carried forward as current
  evidence. This session delivers the requested contract-first start of F2.
- Pre-close main is `765e12215a13ad662eb1a695224e787771af2378`, pushed and
  ALL GREEN. The close is a separate docs-only branch/PR; its final SHA and
  PR/main CI runs are recorded in the session-close response and auto-memory.
- Release ruleset `25923038` remains active for `refs/tags/v*`, forbidding
  creation/update/deletion with no bypass actors. No version tag, release,
  service installation, host-policy change or reboot was performed.

## Landed changes

Each change uses one branch/PR and one commit per push. Landing is a
fast-forward with an aborting full-tree comparison to the reviewed PR tip;
`git diff --stat main <branch>` was checked first. Both applicable local
gates and PR/main CI passed. Working branches were retired after landing.

| Commit | PR | Change |
|---|---|---|
| `f07fa39` | [#69](https://github.com/AlmanInDaHouse/CyberGuard_Enterprise/pull/69) | One conservative elevated-evidence validity rule (#46), exact PowerShell tree command, explicit S27 historical frame |
| `765e122` | [#70](https://github.com/AlmanInDaHouse/CyberGuard_Enterprise/pull/70) | SPEC-024; SPEC-001/002/017/023 amendments and catalog, before implementation |

SPEC-024 contracts an offline setup `.exe` wrapping an MSI, a LocalSystem
service using the existing secure runtime, SYSTEM custody before secret-file
creation, distinct package/SCM/telemetry readiness, a 60-second stop deadline,
recovery, rotated private logs, repair/upgrade/uninstall and explicit migration.
Its 16 acceptance criteria include effective access with standard/filtered
tokens, interrupted writes and rollback, real SCM and real ingest, packaged
payload identity, and the Windows execution matrix. These are future tests,
not passing tests claimed by the contract.

Version is contracted once at build time through `CG_RELEASE_VERSION`:
release `X.Y.Z`, otherwise `<Cargo package version>-dev`, consistently in
CLI, enrollment, heartbeat and lifecycle fields. It adds setup/MSI release
assets alongside the raw executable, with checksum/provenance requirements
and exact payload/version agreement. The release workflow is still unchanged
and will not satisfy the new requirements until F2 implementation.

The contract explicitly carries the inherited audit-policy reporting and
endpoint-dump questions from ADR-0019/0020. Read-only operator inspection is
required; deciding new agent reporting or changing an ADR remains separate.
Neither host dumps nor a missing logon audit policy is assumed safe.

## Decisions

### Anticipated, delegated

- Reconcile #46 by the conservative intersection of the rules, not by
  relaxing the elevated gate. Same-tree reuse needs an aborting comparison;
  different-tree reuse additionally needs documentation-only changes outside
  runtime/test inputs and matching full unelevated suite results. Environment
  and binary applicability are required in both cases. Reject comment-only
  source reuse and old exceptions as precedent (`f07fa39`).
- Exercise ADR-0010's express packaging delegation: LocalSystem/MSI with an
  offline `.exe` wrapper. Reject a scheduled task or shell-only installer.
  The privilege baseline and ADR text remain unchanged (SPEC-024).
- Keep machine-scope DPAPI but make SYSTEM ownership and effective filesystem
  access the custody boundary before any sensitive write. Reject the enrolling
  user's SID and post-write hardening as production custody (SPEC-024).
- Separate committed package, running supervisor and verified telemetry;
  enroll outside MSI rollback. Reject treating SCM RUNNING as server success.
- Preserve identity on repair/upgrade/default uninstall; migration/purge are
  explicit. Reject silent key regeneration and claims of remote revocation.
- Inject the product version once at build time, test with a version unlike
  Cargo's and reject incompatible MSI values. Reject stale Cargo metadata,
  runtime overrides or version truncation (#131).
- Preserve F3 pages/invitations, F4 renewal/hardening and the release freeze.
  Reject declaring F2 or criterion 7 delivered from a SPEC alone.

### Reactive corrections

- A reviewer executed unquoted `HEAD^{tree}` in PowerShell and got a Git
  failure. Quote it as `git rev-parse 'HEAD^{tree}'`; specify both guard
  operands and abort on nonzero, rather than leaving a bare diff command.
- Frame the old prod-driver section as S27 history: its "true today" clause
  is not a claim about the already-running detection driver. Preserve the
  historical verification record instead of rewriting its facts.
- SPEC-002 said `agent.id` was optional while the parser requires UUIDv7.
  Amend that clause explicitly. It is a placeholder only for enrollment-backed
  operation; the plain development path transmits it directly (#119 partial).
- Clarify that package **payload** hashes match, not the hashes of distinct
  outer `.exe`/MSI artifacts. Name FR-011's console/service retry distinction.
- Carry forward the audit/dump questions a second review found omitted; require
  read-only preflight and an explicit disposition before F2 endpoint validation
  is declared complete, without silently settling an ADR question.

### Manuel's decisions

The opening instruction delegated project-benefit choices and preserved his
ADR yes and elevated execution. No additional ratification, risk acceptance,
gate execution or publication approval is inferred from that delegation.

## Self-review and fresh-context reviews

Every reviewer received CLAUDE.md's literal contract-review prompt with only
paths filled in, in a fresh context (`fork_turns=none`), without session
reasoning. Branch, status and worktrees were checked after reviews; no reviewer
mutation landed and no additional worktree was created.

| Change | Own check | Fresh passes and dispositions |
|---|---|---|
| #46 | All three living clauses point to one rule; exact-tree proof and conditional reuse distinguished; no runtime/source exemption | Three passes. Fixed PowerShell quoting, guard operands and environment applicability; framed inherited "today" wording historically. Final List A empty. Existing "run only elevated" wording means elevation required to pass; Environment facts already states the precise four failures/one skip. No rule changed to accept an unelevated pass as elevated evidence. |
| SPEC-024 and amendments | Cross-checked paths, current config/identity/version code, ADR-0010 delegation, release asset list, teardown ordering, all 16 criteria and repeated claims; local links resolve | Three passes. Fixed required/optional ID conflict and plain-path precision, payload-digest wording, retry supersession and omitted inherited questions. Final List A empty. Nominal latency/load fixture must be defined with install_ac_003's implementation; real Windows, adjacent-version, platform and publication proof remain required and absent. Remaining #119 drift is explicitly open. |

No implementation criterion is scored "covered" by documentation. The
self-review is documentary agreement/source checking, not a substitute for
the later criterion-to-test matrix or elevated gate.

## Gates and CI

The full applicable gate for these changes is Markdown/whitespace. It passed
before each push; the contract additionally ran the complete repository
Markdown glob and a local Markdown-link target check. Literal successful
output before contract landing:

```text
markdownlint-cli2 v0.23.2 (markdownlint v0.41.1)
Linting: 150 files
Summary: 0 issues in 0 files
LOCAL SPEC LINKS PASS
TREE EQUALITY PASS
```

`git diff --check` exited 0 without output. The close adds one Markdown file;
its full-repository lint covers 151 files. No executable code changed, so the
runtime suites were not rerun; no historical elevated gate is claimed valid
for a new implementation under #46.

| Change | PR markdown run | Main markdown run | Verdict |
|---|---|---|---|
| #46 `f07fa39` | `40431386136` | `40431431717` | ALL GREEN |
| Contract `765e122` | `40433191833` | `40433254895` | ALL GREEN |

The GitHub connector listed PRs but rejected PR creation with integration
403. The already-configured local Git credential and REST fallback from
CLAUDE.md created them; no new token was issued or logged. CI reads use REST
because the connector's workflow helper exposes only PR-triggered runs.
Initial branch pushes had no workflow until their PR opened; both PR and
main events were then verified separately on the full SHA. No failed tree
was pushed, no Known CI debt accepted and no release workflow was invoked.

### Inherited test baselines, not new S49 evidence

- Rust: S45, 66 binaries, 139 passed / 7 ignored; unchanged since then.
- API: S47, 56 files / 390 tests passed.
- Ingest unelevated: S47, 85 files (80 passed / 4 failed / 1 skipped),
  498 tests passed / 4 capture failures (agent exit 9) / 3 skipped.
- Dashboard: S47, 12 files / 141 tests passed.
- Elevated runtime: last evidence referenced by S47/S48 is S46 gate 1;
  validity for any future candidate must be assessed under the current rule.
- Deploy: S48 full gate 113/113; release dry run 11/11; licence checker
  431 admitted. Those counts are historical and were not rerun in S49.

## Debts

Closed: **#46**, `f07fa39`, one conservative current validity rule, before
any F2 runtime change. No new debt opened.

Still open, now contracted: **#87** (SPEC-024 install_ac_004/005/013;
SYSTEM custody, creation-window and legacy-copy limits), **#131**
(install_ac_014/015; runtime/build version and package agreement).
**#119** remains open for F4: only its `agent.id` optional/required discrepancy
is reconciled by SPEC-002's S49 amendment. No runtime fix is implied.
Other destinations from S48 remain, including #4/#37 with the first F2
capture/gate implementation, #50 isolated fault evidence, #59 external
Windows compatibility evidence, and #130 before release.

Open at close (48): #4, #5, #7, #21, #24, #25, #26, #29, #31, #32, #35, #37, #38, #39, #42, #50, #51, #52, #53, #54, #56, #58, #59, #61, #62, #64, #65, #66, #73, #83, #87, #88, #89, #97, #98, #99, #102, #110, #113, #114, #119, #124, #125, #130, #131, #132, #133, #136.

## Environment and memory

Docker Desktop responded at start, Engine 29.1.2, 52 running containers,
7.65 GiB VM memory. C: had about 36.26 GiB free; host free physical RAM
about 1.23 GiB of 15.8 GiB, with Docker active. A later pre-close snapshot
had 36.24 GiB disk free and 2.57 GiB free RAM. No build or image cleanup was
needed. The five `cyberguard-dev` containers remained running/healthy;
other projects and Docker settings were untouched. No tool was installed.

Auto-memory gets `project-session-49-close.md` and its index line. The live
elevated-gate reference now records the single #46 rule, quoted PowerShell
tree expression and one-line script procedure; its old #63 orphan-cleanup
workaround is marked historical because the current helper cleans sessions.
The debt index points to the live 48-row register. S48's handoff/memory
remain historical; no Obsidian mirror is used for this project.

## How Session 50 resumes

1. Follow CLAUDE.md, this handoff, and the start protocol. Recheck C:, host
   memory and Docker before builds. With Docker use `-j 2` and
   `CARGO_INCREMENTAL=0`; reduce dev/test debug information if disk requires.
2. Start F2 implementation from Accepted SPEC-024. First isolate #131:
   acceptance tests proving non-Cargo release version and dev/rebuild behavior,
   then implementation; package-version checks follow with the MSI work.
   A runtime change needs its complete workspace gate and any required
   elevated evidence; #46 is already reconciled, not an excuse to reuse an
   expired run. Keep #4/#37 with the first capture/gate change.
3. Continue service/custody and package changes with one branch/PR per logical
   change, contract criteria failing locally for missing behavior before code,
   one commit per push, complete local gate before both pushes, literal
   fresh-context reviews (again after fixes), aborting tree guard and CI green.
4. Resolve SPEC-024's packaging-tool selection before using it. Carry the
   audit-policy/dump questions to explicit disposition before declaring F2
   endpoint validation complete; an ADR change needs Manuel's specific yes.
   #59/#50 still require external/isolated Windows evidence; do not reboot
   or crash the development machine as an ordinary automated step.
5. Manuel runs any elevated gate via one line invoking
   `C:\tmp\s50-gate<N>.ps1`; read its logs and actual tree before landing.
   No gate from this session exists to inherit.
6. Keep renewals in F4 and invitations/pages in F3. Keep release protection
   active until F2–F4, #130 and Manuel's explicit publication approval.
   Published provenance and the timed installation still need first-release
   evidence. Close with handoff-50, register, README, roadmap, memory,
   clean pushed main and ALL GREEN.

## Close review

Own pass: compare debt rows/count and catalog counts with the handoff; verify
anchor commits, changed paths, CI IDs and gate output; distinguish prospective
F2 criteria from implementation and inherited runtime baselines. The README
and roadmap report contract acceptance, not F2 delivery. ADR text, executable
operations documents and release settings are unchanged. The close receives
the same literal fresh-context contract review before landing.

That review found no List-A blocker and independently counted the 48 open
rows, with #87/#131 still open. Its List B distinguishes external/session
evidence (ruleset/CI reads, reviewer calls, host measurements and memory)
from repository-verifiable claims; those are backed by this session's tool
results. The install_ac_003 nominal fixture remains an implementation task,
as recorded above. Older roadmap "today" wording is in explicitly completed
phase/S27 context, not a new current-state assertion; no historical record
was rewritten. No correction was required after this close review. The
final close SHA and PR/main CI IDs go in auto-memory and the close response.
