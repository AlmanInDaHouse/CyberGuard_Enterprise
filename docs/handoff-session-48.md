# Handoff — Session 48

Session 48 opened on 2026-10-26 and closed on 2026-10-27. The work order
was the remaining F1 operations work: contract first, then executable
installation instructions, upgrade and the other runbooks, starting from
`docs/s47-operations` rebased onto `main`.

Manuel delegated the remaining choices and debt destinations for an
enterprise buyer. ADRs still require his explicit yes; he runs elevated
gates. His two answer slots were unfilled: this session's audit of S47
and R12 is delegated, not a claim that Manuel personally approved them.
His later instruction was "sigue" after a reviewer hit a usage limit.

## Start and resulting state

- Start: `main` = `origin/main` = `1a43b07`, clean tree, empty stash,
  one root worktree, no open PR; main CI `40395604396` was green.
- ADR/SPEC catalogs: 22/22. Known CI debt: none. Open debts: 52 at start,
  49 at close (listed below).
- The draft branch was `626b23d`; rebased locally onto `1a43b07` as
  `90b9d4e`. Its seven documents were consumed by the three implementation
  PRs below; the rewritten draft was not pushed. The source branch was retired
  locally and remotely after all seven revised documents landed.
- F1's server implementation and automated procedures have landed.
  Manual Windows endpoint validation still needs execution evidence.
  Criterion 7 is still pending F2–F4 and the manual release evidence.
  No public release or release tag was created.
- Pre-close main is `0e7ab9e`, pushed and ALL GREEN. The close change is
  documentation only; its final main SHA and CI result are reported in the
  session-close response and memory.

## Landed changes

Each logical change has its own branch and PR, one commit per push.
Landing used fast-forward and an aborting tree-equality guard. Before
landing, `git diff --stat main <branch>` contained only that change's files.

| Commit | PR | Change |
|---|---|---|
| `b04f2e4` | #63 | SPEC-023 third Amendment 2026-10-26 and catalog: later users/recovery, daemon/IP requirements, version floors, external ingest pair, logging, #134 and executable runbook verification |
| `9cad427` | #64 | `install.md`, its executable test and explicit Docker/Compose floors in deploy CI |
| `34a4f73` | #65 | `upgrade.md` and failure-path test: both writers stopped through both ordered migration commands (#127) |
| `6944ee6` | #66 | Remaining operations documents; TLS/SNI matrix, real syslog TLS, external pair placement/renewal/restore, private forensic verification and owner-role audit query |
| `0e7ab9e` | #67 | Release tag protection record (#135) |

The install test executes the document's server commands using their real
programs. It rejects an unclassified fence; verifies checksum/extraction
and actual `gh` refusal of an unattested fixture; uses a PTY for passwords,
all three roles, later admins and interrupted TOTP recovery; and checks
audit rows and password non-echo. An unattested fixture is not positive
published-provenance evidence.

Upgrade prepares and pulls before its outage, stops ingest and API, takes
backups with writers stopped, then runs `ingest-migrate` and `api-migrate`
in the foreground with `--exit-code-from`. Only both successful commands
permit writer restart. Tests fail each one-shot separately and prove no
writer restarted; a successful same-version run preserves the CA and
forensic key. They do not certify a future schema transition. Owner roles
can still take each other's advisory keys: #127 closes as an operational
mitigation, not removal of that capability.

Configuration covers every bundle variable, external certificate custody,
all-nine-service TLS syslog and its delivery limits. Recovery uses a
separate project and preserves the original volumes. Private host staging
plus `docker compose cp --archive` sets uid 1000 and key mode 0600 without
running a root maintenance container. The test verifies initial use,
renewal and replacement after recovery-volume removal, before restore.

The data-protection document inventories the technical assets and access,
retention and deletion limits. The organisation's DPO still owns its
record of processing and policy. Forensic instructions use private exports,
the auditor's UID and a read-only, networkless verifier. The operator's
audit query uses the existing API schema-owner role, not the superuser.
Neither document claims protection against prior rewriting by compromised
ingest (#124), nor cryptographic coverage of unsigned incident metadata.

## Decisions and audit

### Anticipated, delegated

- Keep later-user CLI creation and recovery until F3's invitations (#98),
  with explicit temporary secret custody. Alternative: require pages not
  yet built. The SPEC explicitly supersedes SPEC-008's API-only wording.
- Rootful Linux Engine 29.1.2 / Compose 2.40.3 floors, patched supported
  versions, merged daemon configuration and a real remote IPv4 check.
  Alternative: let the CI runner's version define support implicitly.
- Keep an operator's ingest pair in the existing named volume and preserve
  its actual issuer as the endpoint trust anchor. Alternative: a new mount,
  helper image or silently substituting the internal CA.
- Quiesce writers through both migrations and preserve original recovery
  volumes. Alternative: general `compose up`, which can start ingest before
  API migration finishes, or destructive recovery over the original stores.
- Provide verified TLS syslog with bounded local cache and explicit loss/
  availability limits. Alternative: describe local rotation as retention
  or imply a durable, lossless logging queue.
- Keep forensic exports private to the auditor and restrict the offline
  container. Alternative: world-readable exports for image uid 1000.
- Freeze matching version-tag creation/update/deletion with no bypass.
  Alternative: a required-reviewer release environment; the freeze stops the
  workflow earlier and needs no second eligible reviewer. Administrators can
  still edit it; this is not protection against malicious administrators.

### Reactive corrections

- CI's previous runner used Docker 28.0.4 / Compose 2.38.2. Pin the two
  official setup actions to commit SHAs and configure the contracted floor;
  do not lower the contract to match the old runner.
- Recovery retry deletes `ingest_certs` with the recovery volumes. Repeat
  external-pair placement before `restore.sh`, and test that exact route.
- Real syslog rejection exposed that Docker 28.5.2 interprets the presence
  of `syslog-tls-skip-verify` as disabling verification even for `"false"`.
  Omit it; wrong issuer/name now fail. The SPEC states the verification
  requirement, not Docker's internal parsing behavior.
- A root/CAP_CHOWN maintenance helper was replaced with private host staging
  and archived copy, keeping the normal container privilege contract.
- An audit read with `postgres` contradicted the SPEC's superuser scope.
  Use `cg_api_owner`, whose existing ownership permits the read; no grant
  changes. Its test needs the suite's random project and test override.

### S47 decisions and R12, delegated audit

Checked S47's decisions against the four Dockerfiles, notices generator,
release workflow/scripts and tests, migration runners, compose, setup and
proxy configuration. Retain the nine labels, no sharp, Node notices,
pinned BuildKit/scanner, maximum provenance, reproducible bundle, bounded
lock waits, ordered one-shots, `.env` project/address controls and HTTP 421.
S48 supplies the missing operational paths and #134 contract/tests.

R12's SBOM/notices claims describe the implemented release workflow; they
do not constitute a licence audit of every shipped component. #130 remains
before first release. The blanket actions-by-tag sentence is corrected:
release/deploy actions are SHA-pinned, while #113 covers other workflows.
No risk is newly accepted, and no ADR is amended or ratified in S48.

## Self-review and fresh-context reviews

Every reviewer received CLAUDE.md's literal fixed prompt with only its
paths/SHAs filled in, in a fresh context. After each review, branch,
`git status` and worktrees were checked. No reviewer mutation landed. The release-protection document had two
passes, no List-A blocker: clarified that the timed installation uses the
published bundle with the first release. Its historical API snapshot is
session evidence, not reproducible from repository files alone; the reviewer
also checked the public active-rule page. The close review is recorded below.

| Change / matrix | Reviews and disposition |
|---|---|
| Contract: each repeated claim and source/default/CLI path checked | Two passes, no List-A item. Added explicit API-only supersession; CI-floor and later implementation evidence remained prospective. Stale CLI header becomes #136. |
| Install: block inventory/execution, PTY, roles, audit, recovery, tools, daemon validation | Three passes. Fixed shell-language/fence bypass, later-admin coverage and floor checks. Remote route and first-release evidence remain manual; companions/TLS subsequently completed. |
| Upgrade: stopped writers, both failure paths, ordered success, CA/key preservation | Three passes. Fixed external issuer overwrite, blanket legacy database deletion and a Map helper read as a property. Same-version evidence is not upgrade compatibility proof. |
| Operations: deploy_ac_008/019 TLS matrix in `stack.test.mjs` and `images.test.mjs`; deploy_ac_010 private files and audit read; deploy_ac_021 docs plus `operations.test.mjs` executing pair and logging | Five completed fresh-context passes; final pass at `6944ee6` found no substantiated contradiction. Fixed retry pair omission, exact description comparison while preserving its semantic assertions, superuser read and maintenance privilege concern. Added actual TLS collector with negative trust/name cases. |

The final coverage matrix: automated requirements covered; the remote
production-client route is weak as automation and explicitly manual;
positive first-release provenance and the Windows endpoint/timed install
have no new execution evidence here. Prose accuracy was checked against
the source, not manufactured as text-matching assertions. Extra invocation
hardening uses existing privileges; no runtime authentication, grants,
cryptography or agent collection changed.

## Gates and CI

The full local deploy gate built all four images, ran every bundle test in
`cg-dind`, stopped it, ran the release OCI/bundle dry run, licence checker,
Markdown lint and whitespace check. Repeated before each implementation
push to `main`; none of the failed unpublished trees was pushed. Docs-only
changes ran their complete applicable Markdown/whitespace gate before each
push. No build ran from a worktree.

Literal successful output, final implementation:

```text
# tests 113
# suites 16
# pass 113
# fail 0
# cancelled 0
# skipped 0
# todo 0
# duration_ms 359712.974921
RELEASE DRY RUN GREEN 6944ee60e370e417fd1a408a0a63fdc18ae3a962
ℹ tests 11
ℹ pass 11
ℹ fail 0
ℹ skipped 0
licences admitted: 431 packages
Summary: 0 issues in 0 files
```

Earlier final gates: install 104/104, upgrade 105/105; release dry run 11/11
for both, licence checker 431 admitted. Before-main repetitions passed:
install 104/104 (211823.535539 ms), upgrade 105/105 (269855.856449 ms).
Runtime TS/Rust paths did not change; those suites were not rerun and their
baseline remains handoff-47's. No elevated capture/detection gate was
triggered, no agent was compiled locally, and no elevated result is claimed.
Deploy CI did build the unchanged Linux agent as its workflow requires.

Local tools: Docker Desktop Engine 29.1.2 / Compose 2.40.3-desktop.1;
nested test daemon 28.5.2 / Compose 2.40.3, Node 22.23.4, gpg 2.4.9,
gh 2.72.0. Nested execution is not production-floor certification. CI log `40426201075` records Engine 29.1.2, Compose 2.40.3, gpg 2.4.4 and
gh 2.102.0; its bundle result is 113/113 and dry run 11/11. The floor is
explicitly asserted, rather than inferred from the runner image.

Unpublished failures were diagnosed and fixed: missing GNU `sha256sum`
option in BusyBox (installed coreutils in cg-dind), unhandled draft command,
`compose create --no-deps` (use `up --no-start --no-deps`), test Map access,
blocking synchronous Docker logs in the Node TLS collector (make calls
asynchronous), wrong-issuer acceptance with the skip option, and the audit
test's missing project/override and an incorrectly restored description
prefix (`CyberGuard` instead of the original `CyberGuard's`): `76ec82f`
had four label failures plus six cascading audit/forensic failures (103/113
passed); `1cc481c` fixed the project and passed the whole stack but retained
the four label failures (109/113). Both are fixed in `6944ee6`. A daemon-readiness race in temporary launchers was fixed by
waiting for `docker info`. A reviewer hit a tool usage limit, then resumed
on Manuel's "sigue" and completed; no absent review was counted as passed.

All these runs ended in success; there is no Known CI debt:

| Change | PR runs | Main run |
|---|---|---|
| Contract `b04f2e4` | markdown `40412069086` | markdown `40412133142` |
| Install `9cad427` | deploy `40415414725`, markdown `40415414760` | markdown `40416682173` |
| Upgrade `34a4f73` | deploy `40418863510`, markdown `40418861793` | markdown `40419753809` |
| Operations `6944ee6` | deploy `40426201075`, markdown `40426201042` | markdown `40427164189` |
| Release protection `0e7ab9e` | markdown `40427609568` | markdown `40427663625` |

The close commit's Markdown workflow is also checked on its full SHA after
both pushes; its final SHA/run is recorded in memory and the close response.

## Debts

Closed: #95 (technical personal-data inventory, 6944ee6; the DPO's
organisation-specific record remains its responsibility), #127 (`34a4f73`,
procedure/failure tests), #134 (`b04f2e4` + 6944ee6), #135
(active GitHub ruleset `25923038`, read back with no bypass;
  [release-protection.md](security/release-protection.md)).

New #136: `services/api/src/cli/create-user.ts`'s header still says users
after the first are created through the API alone. Runtime supports the
accepted later-user CLI contract. Correct the stale comment with the next
F3 user-management/CLI change and its workspace gate; it does not block
the tested command or these runbooks.

Delegated destinations changed in the live register:

| Debt | Destination / reason |
|---|---|
| #4 | F2's first capture/gate change: assert positive captured-event evidence before diagnosing a cursor failure (S48 delegated). |
| #7 | After F3's user/forensic work: propose shared JCS ownership and a contract before extraction; any new-component ADR still needs Manuel's explicit yes (S48 delegated destination). |
| #21 | Pre-release documentary/code-comment sweep with #136 (S48 delegated). |
| #25 | F4 pre-release load measurements: measure connection churn before changing client lifetime (S48 delegated). |
| #26 | F3's incident/alert-page contract: specify technique accumulation and severity-escalation notification before implementation (S48 delegated). |
| #32 | F4 pre-release delivery hardening with #114: contract first for permanent rejection versus retry, preserving evidence of dropped batches (S48 delegated). |
| #35 | F4 pre-release load measurements with #25/#61; measure backlog/drain behavior before changing the contracted ceiling (S48 delegated). |
| #37 | F2's first capture/gate change with #4: drain the fixture until a cycle returns fewer rows than the limit (S48 delegated). |
| #39 | Pre-release documentary/schema-example sweep (S48 delegated). |
| #42 | F4 agent renewal/delivery work: measure and implement connection reuse under its contract (S48 delegated). |
| #46 | Before F2's first runtime change: reconcile the living gate rules; until then satisfy the conservative combination, never claim an expired gate valid (S48 delegated). |
| #50 | F2/F4 isolated Windows fault test; Manuel runs the elevated service-restart experiment, not the development host during ordinary work (S48 delegated destination). |
| #59 | F2 release-validation matrix: obtain Windows 10 and Server 2019 execution evidence before claiming those targets validated; external machines remain required (S48 delegated). |
| #61 | F4 pre-release load gate with #25/#35; the earlier 'before F' destination was missed and is superseded (S48 delegated). |
| #97 | A future multi-host deployment, outside the MVP, as ADR-0022 §3 already requires; align the register and roadmap to that decision. No new risk acceptance. |
| #113 | Pre-release CI-hardening sweep: pin remaining actions to commit SHAs; release/deploy workflows already do so (S48 delegated). |
| #124 | F4 pre-release threat-control proposal for evidence rewriting; retain the disclosed limitation until a mitigation is contracted and tested. Any required ADR needs Manuel's yes; no risk acceptance by delegation (S48). |
| #125 | F4 pre-release database/logging threat-control proposal and regression evidence; do not claim hostile sessions cannot expose parameters. Any required ADR needs Manuel's yes; no risk acceptance by delegation (S48). |
| #132 | Pre-release packaging sweep: decide and test the local-build documentation URL without changing published revision links (S48 delegated). |
| #133 | Pre-release image-content sweep: exclude traced TypeScript only after proving the standalone runtime still works (S48 delegated). |

All other existing destinations remain. #73 and #99 still need an ADR
ratification; #7 may need one. #124/#125 remain disclosed open risks, not
accepted risks. #50/#59 depend on isolated Windows execution evidence.
No debt is treated as closed merely by assigning a destination.

Open at close (49): #4, #5, #7, #21, #24, #25, #26, #29, #31, #32, #35, #37, #38, #39, #42, #46, #50, #51, #52, #53, #54, #56, #58, #59, #61, #62, #64, #65, #66, #73, #83, #87, #88, #89, #97, #98, #99, #102, #110, #113, #114, #119, #124, #125, #130, #131, #132, #133, #136.

## Environment at close

At start C: free 38.89 GiB, host RAM free 1.81 GiB of 15.8, Docker VM
7.65 GiB. Stopped the five CyberGuard dev containers to free memory;
other projects and Kubernetes untouched. Installed github-cli, util-linux
and coreutils only inside the existing cg-dind test environment. No application
data volumes deleted. At close C: has about 36.26 GiB free; host free RAM
about 0.65 GiB and the WSL VM working set about 5.97 GiB after restoring the
five dev containers. All five are healthy. `cg-dind` is stopped and the
temporary `cg-release` builder removed. No other project was stopped or deleted.

## How Session 49 resumes

1. Follow CLAUDE.md start protocol, this handoff, C: space and Docker RAM.
2. Start F2's installer/service-mode SPEC under ADR-0022/ADR-0010. Include
   #87 key custody, #131 release version and the existing endpoint gaps;
   keep renewals in F4 and invitations/pages in F3. Resolve #46's gate-rule
   coherence before F2's first runtime change. Contract before code.
3. Keep release protection active until F2–F4, #130 and Manuel's explicit
   publication approval. The remote-IP and endpoint checks still need manual
   evidence; positive published provenance and the timed installation run
   with the first release, from its published artifacts.
4. Use each debt's recorded destination. ADR changes need Manuel's yes;
   elevated gates use a one-line `C:\tmp\s49-gate<N>.ps1` script run by him.
5. Continue one branch/PR per change, one commit per push, complete local
   gate before each push, clean-context fixed-prompt reviews and repeat
   after fixes, aborting landing guard and CI ALL GREEN.

## Close review

Self-review: debt rows and open count checked against the numbered list above;
anchor commits and file scopes against git; gate counts and CI against saved
logs/API responses; README login status against its route handlers and tests;
R12's action-pinning sentence against the workflow files. Historical handoffs
and ADRs were not edited. Manual evidence remains explicit, and no deferred
debt is represented as implemented. The first fixed-prompt close review found four contradictions in living
documents: the threat model's old application-only audit claim (production
runtime grants now enforce append-only), the roadmap's already-ratified
TypeScript decision, owner-pending destinations for #124/#125, and #97's
obsolete F destination. Corrected each against migration0007/tests, the
Accepted ADR-0002 amendment and ADR-0022 §3, and this session's delegated
destinations. The remaining Blueprint mismatch stays #5. The reviewer
confirmed the 49-row count. Historical run/machine evidence remains a
session record; manual release evidence stays pending. A second fresh pass
checks the corrected close before landing.

The second pass found no substantiated blocker and confirmed exact agreement
between the register's 49 rows and this list. Nonblocking observations:
the roadmap's existing "fire-and-forget" term means failure containment in
ADR-0017, while ADR-0021 explicitly awaits execution; retain that defined term.
The threat model's 8 h / 24 h session bounds remain true when configuration
shortens them. Neither wording observation changes a contract or opens a
defect. Historical/external evidence and the same-version upgrade/manual
release limits retain the dispositions above. Branch, status and worktrees
were checked after both passes; only the root checkout exists.
