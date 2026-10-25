# Handoff — Session 46

Session 46 opened and closed on 2026-10-25. Manuel pasted the prompt
Session 45 left. Its three answer slots arrived unfilled, and he answered
them when asked (below). The prompt's order was the rest of F1's code
(SPEC-023) on branches, one pull request per change:

1. the images: the four non-root with a health check, the rules and
   playbooks read-only, the dashboard's standalone server;
2. the bundle with its tests and `deploy-ci.yml`: the proxy with HSTS, the
   key-value store's ACL, the ClickHouse users, backup and restore, and
   #122;
3. the release and the licence check (#112);
4. `docs/operations/`, with the upgrade document handoff-45 asks for;
5. the root compose stubs (#12).

The delegation stood as in Sessions 38 to 45: what depends on Manuel is
decided for an enterprise buyer and recorded as delegated; an ADR still
needs his "sí", and the elevated gate is his to run.

This session delivered items 1 and 2 whole, and item 3's licence check.
The release workflow, `docs/operations/` and the compose stubs remain, in
that order (*How Session 47 resumes*). On the way, a high advisory in zod
turned the images' pull request red; zod 4 replaced zod 3 with every input
read as before, under the elevated gate.

## Start

- `main` = `origin/main` = `3ba9f3b`, tree clean, no stash, no branch but
  `main`, no pull request, no worktree. CI of `3ba9f3b`: `markdown-lint →
  success`.
- Catalogs ADR 22 / SPEC 22, Known CI debt none, `docs/debts.md` open 56,
  as handoff-45 states.
- `C:` had 59 GB free.

Manuel's decisions this session, in chat, when asked about the three
unfilled slots (2026-10-25):

- The threat model: "Todavía no (Recommended)": not audited yet.
- His dev database: "Recréala tú (Recommended)". Claude Code recreated it
  (below).
- Host port 8080: "Otro puerto en mi .env (Recommended)":
  `CG_INGEST_ENROLL_PORT=18080` in his `infra/dev/.env`, with a comment.
- Later, when a gate was stopped for want of memory: "memoria liberada",
  and the gate ran again.

## Progress

Landed on `main`, in order, each with CI green on its pull request and on
`main`:

| Commit | What | PR |
|---|---|---|
| `9c06fd2` | SPEC-023 Amendment 2026-10-25 (the migration lock, #123's citation, the dashboard's Windows build, the project name, deploy_ac_004/005's place) | — |
| `aa56b49` | zod 4.6.7 in ingest and the api, every input read as before (GHSA-mrhx-c7xc-m3w5) | #50 |
| `03b7976` | The Amendment of `9c06fd2`, restored (`aa56b49` had removed it) | — |
| `2217725` | #122: the one-shot's lock wait bounded; advisory locks closed to the runtime roles | #51 |
| `7385f32` | The four product images, non-root with a health check, and `deploy-ci.yml` | #49 |
| `b794f52` | The bundle: production compose, `setup.sh`, the stores' credentials, the edge | #52 |
| `4f5b86e` | SPEC-023 Amendment 2026-10-25, the second (the report's text, the `app` network's addresses, the proxy's protocols and its answer to another host) | — |
| `5694e1d` | deploy_ac_009 to 011 through the bundle | #53 |
| `072e0d2` | `backup.sh` and `restore.sh` (deploy_ac_012) | #54 |
| `867ac68` | The licence check (#112's first half) | #55 |

### zod 4 (PR #50)

`dependency-audit` turned PR #49 red with GHSA-mrhx-c7xc-m3w5, a high
advisory in zod below 4.6.6 (memory exhaustion validating oversized
collections); no zod 3 release is patched. The fix was the major upgrade
(`docs/security/dependency-review.md`, step 4), keeping what zod 3 read:
`event_id` as any 8-4-4-4-12 hexadecimal string (`z.guid()`), the
heartbeat's integers by `Number.isInteger`, the IP addresses by zod
3.25.76's two patterns (MIT, `NOTICE` carries the attribution), the six
store URLs by `new URL()`, the configuration's reasons on zod 4's issue
shapes, and the rule loader's absent `cg_detection_source` and unknown
keys. Two reviewers ran differentials of the base on zod 3 against the tip
on zod 4 (millions of envelopes, addresses and identifiers, 1,068 rules):
no difference in what is accepted or passed on. The rule loader's other
messages are zod 4's words, which the commit states. It changes the
capture path's schemas, so it landed after elevated gate 1 (*Gates*).

### The migration lock (#122, PR #51)

`migrate.js` waits for its advisory lock up to 300 s (`lock_timeout` on
that statement alone) and then fails with
`cg-<service> migrate: fatal: the migration lock is held by another
session: waited 300 s`; the bundle's Postgres initialisation revokes the
sixteen lock-taking advisory functions from `PUBLIC` and grants them to the
two owner roles. deploy_ac_004 (api suite) refuses each of the sixteen to
both runtime roles, lets the owners take a lock, and refuses each service's
own run as its runtime role at `pg_try_advisory_lock`, held or not; the
bounded wait is tested on a database of its own, which it leaves unchanged.

### The images (PR #49)

ingest, the api and the dashboard on `node:22.23.4-alpine3.24` pinned by
digest, the proxy on `caddy:2.11.7-alpine` pinned by digest; uid 1000, the
proxy uid 10001; `LICENSE` and `NOTICE` in `/usr/share/doc/cyberguard`; the
four OCI labels; a `HEALTHCHECK` each; ingest's rules, playbooks and their
schema root's; the dashboard's Next standalone server (off Windows, SPEC-023
Amendment 2026-10-25 item 4); the proxy's Caddy without its file
capability, HTTP/1.x and 2 alone, HSTS, the redirect on 8080.
`deploy-ci.yml` builds the four from the commit on a pull request and runs
`deploy/docker/test/` against them. `dev:up` builds before it starts.

### The bundle (PR #52)

`deploy/docker/docker-compose.yml` (project `cyberguard-enterprise`; the
networks `public`, `edge`, `app`, `data` and `agents`; the dashboard's
fixed address `10.254.254.2` on `app`, outside Docker's range; every
product container non-root, read-only, every capability dropped; every
generated secret required; the two migration one-shots as the owner roles
and the ClickHouse administrator, the services as their runtime
credentials); `.env.example` and `setup.sh` (POSIX `sh`); Valkey's ACL file
written from the environment (`default` off, `cg_ingest` on `nonce:*`,
`cg_api` on its keys); ClickHouse's users (`cg_ingest`; `cg_api` read-only,
`cges_events` alone, class 1007 alone), its system logs kept seven days and
read only by a grant, the backup disk. `deploy/docker/test/` drives it as
root: the compose's contract, `setup.sh`, and a stack brought up from the
bundle.

### The bundle's other criteria (PRs #53 and #54)

deploy_ac_009 (a Linux debug build of `cg-agent`, built by `deploy-ci.yml`,
enrolls through the bundle and heartbeats; both listeners present one
certificate naming each server name), deploy_ac_010 (the forensic key's
fingerprint, an export requested on the `app` network, the verifier from
the api's image with no network), deploy_ac_011 (the retention's time-to-live
through `CG_RETENTION_*`, the bounds refused by the one-shot and ingest),
deploy_ac_012 (`backup.sh` and `restore.sh`: two files encrypted with gpg as
they leave their containers, the passphrase on a file descriptor; a restore
that verifies both files first and refuses non-empty stores; a new stack
restored with the same fingerprints, the agent's heartbeats accepted and
the incident listed). `clickhouse-client` reads the administrator from the
container's environment, in the scripts and in the initialisation.

### The licence check (PR #55)

`.github/scripts/check-licences.mjs` over the production pnpm dependencies
and `cg-agent`'s runtime crates for its Windows target, against
`.github/scripts/licences-allowed.json`, in `dependency-audit.yml`'s new
licences job; `deploy/docker/test/licences.test.mjs` fails it on a fixture.
437 packages admitted on the repository (438 on CI's Linux install). The
allowlist admits the permissive licences, and five packages under their own
licence: among them sharp's libvips (LGPL-3.0-or-later), which the dashboard
image ships though the dashboard uses no `next/image`, and `caniuse-lite`
(CC-BY-4.0), neither with its notice in the image (debt #129). The SBOM is
the release's.

### Manuel's dev stack

At the start Claude Code added `CG_INGEST_ENROLL_PORT=18080` to his
`infra/dev/.env` and recreated his dev database's volume. The stack's images
were four weeks old (`up -d` does not rebuild) and migrated into `public`;
they were rebuilt and the volume recreated again, after which the schemas
`ingest` and `api` held the tables. `dev:up` now builds (PR #49). His
`cyberguard-dev_cg_ingest_certs` volume was written by a root-run ingest:
with the non-root image, its `0600` key is unreadable (*What waits on
Manuel*).

## Gates

`C:\tmp\s46-gate1.ps1` (Manuel, elevated, 2026-10-25 17:28–17:43), on
`aa56b49`, PR #50's tip:

```text
No hay entradas en la lista.
net session exit: 0
aa56b497e87cd54e1f00e063a44891e2d8931544
status lines: 0
    Finished `release` profile [optimized] target(s) in 1m 08s
cargo build exit: 0
test auth_ac_012_logons_on_the_real_log ... ok
test capture_ac_006_elevated_shutdown_leaves_no_session ... ok
test net_ac_008_a_real_connection_is_reported_with_its_probe ... ok
test net_ac_009_an_accepted_connection_and_the_filter ... ok
test ac_004_cache_hit_terminate_matches_launch_byte_for_byte ... ok
test ac_007_parent_pid_only_when_parent_dead_at_child_launch ... ok
test ac_009_events_lost_under_deliberate_etw_buffer_pressure ... ok
cargo test exit: 0
pnpm install exit: 0
detect_ac_001 captured image_file_name — probe: …\winword.exe; child: C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe
{"event":"spec_005_marquee_complete","marquee_elapsed_seconds":35.381,"budget_seconds":45,"verdict":"pass"}
spec_005 captured image_file_name — launch: C:\Windows\System32\cmd.exe; terminate: cmd.exe
 Test Files  85 passed (85)
      Tests  499 passed | 2 skipped (501)
pnpm test exit: 0
logman exit: 0
```

The release binary was built by the gate (17:29:49); the two skipped tests
are the POSIX-mode ones, as in S45; the marquees ran (detect_ac_001 46.7 s,
auth_ac_013 25.3 s, net_ac_010 25.3 s, SPEC-005 40.4 s, ac-001 4.7 s); no
`CGAgent` session was left. The gate covers zod 4 (`aa56b49`). Later
commits touch `services/` (#122's `migrate.ts`, the Dockerfiles, the test
helpers) but no path the elevated gate guards; the next change to one runs
the gate again.

Two of Claude Code's background commands were stopped by Claude Code for
want of memory while the session was idle: the watcher of the gate's last
log, and the local gate of #122 at `2217725` (with Docker's other projects
running, about 770 MB of 16 GB were free). Neither was restarted until
Manuel freed memory and said so; an orphaned vitest of the stopped gate was
stopped by hand and its test containers reaped.

Local gates, before each push, in the workspaces each change touched: the
api and ingest suites (*Baselines*); the dashboard's build, typecheck, lint
and tests; the dev compose's check and render; markdownlint on each changed
document; and `deploy/docker/test/` in a docker-in-docker container
(`docker:28-dind`, the images loaded from the host, the repository by `git
archive`) as root, the Linux agent run in a `debian:trixie-slim` container
on the host network, since Alpine's `gcompat` lacks a symbol the glibc build
needs. The last full run, at `6e520b5` (PR #54 before its second pass):
70 of 70 (images 28, compose 14, setup 9, stack 19); after the second
pass's fixes, the stack's 19 again, and `deploy-ci` on `072e0d2`.

## Self-reviews

Each code change: Claude Code's matrix, then a reviewer with a fresh
context and CLAUDE.md's fixed prompt, then another pass after the fixes;
each SPEC text: Claude Code's pass, then the fixed text prompt. The reports
are summarised here; every *weak*, *missing* or *contradicted* verdict and
every list-A entry was fixed or is recorded below with its reason.

- **SPEC-023 Amendment 2026-10-25** (`9c06fd2`): two passes; list A (the
  fatal line's `Error:` prefix, eight of sixteen functions, two clashing
  sentences, Kysely's key under a re-grant) fixed before it landed.
- **zod 4** (PR #50): two passes, differentials of zod 3 against zod 4;
  no behavioural difference. Weak: the rule loader's messages, stated in
  the commit; the outer `sequence_number` and the Network and Authentication
  `event_id`s reached by no wire test (the differential covers them).
- **#122** (PR #51): Claude Code's matrix found the runtime role's refusal
  untested (added; a mutation showed `cg_ingest` gets `42501` from Kysely's
  table without the revocation, so the test asserts the function's name).
  First pass: "having changed nothing" missing, the held-lock case weak, the
  README silent: fixed. Not fixed, with reason: the five minutes are
  asserted by the constant alone (a five-minute run is out of proportion);
  the bound covering one statement is true by construction (the lock's
  connection is not the migrator's); the command's exit 1 is by composition
  (no environment seam for the wait, deliberately); `migrateFatalLine`
  drops the class prefix from every failure line (the means to print the
  amendment's line; no document quoted the old form). Second pass: three
  sentences of the README and comments fixed. The residue is debt #127.
- **The images** (PR #49): first pass (R15's promise the non-root image
  cannot keep, ownership of `/app`, the syntax directive and `--from`
  images unchecked, the Windows start-up test) fixed and squashed in.
  Second pass: the dashboard's command checked; HTTP/3 turned off and
  tested; the threat model's cross-cutting line corrected against a listing
  of the images; two comments untrue at the commit's SHA fixed. Not fixed:
  ingest's and the api's image health checks run by no test (the bundle's
  compose runs the same commands to healthy in deploy_ac_003); the proxy's
  inherited Caddy labels (debt #128); `dev:up`'s rebuild leaves dangling
  images (a stale image ran old code, which is worse); the HSTS answer to
  another host (SPEC-023's second Amendment 2026-10-25, item 3).
- **The bundle** (PR #52): first pass fixed (the one-shots' inherited
  health check, the stores' readiness over TCP, `setup.sh`'s reading of
  `CG_TLS_MODE` and its refusal before any change, ClickHouse's system
  tables, the row policy named for its users, the tests that asserted less,
  R7's and R10's remainders as debts #124 and #125). ClickHouse's
  `system.tables` stays readable by every user (ClickHouse's own rule), so
  the test probes the log tables for `ACCESS_DENIED`; the image's own
  `config.xml` already requires the grant, so the bundle's setting pins it.
  Second pass: `setup.sh` joined an added variable to a `.env` without its
  final newline, corrupting the last secret (reproduced; fixed and tested);
  the refusals asserted by their codes; `.env.example` and §8 corrected.
  Not fixed, with reason: `api-migrate` waits for Postgres alone (the only
  store it uses); the Compose minimum version (the operations documents);
  the fixed `app` subnet (SPEC-023's second Amendment 2026-10-25, item 2).
- **deploy_ac_009 to 011** (PR #53): first pass (the report's text, the
  60 s bound, the agent's start and identity read, a bounded `compose run`,
  `cargo build --locked`, the verifier's chain head) fixed or moved to the
  second amendment, item 1. Second pass: the key's base64url, one
  certificate on both listeners, the PDF's first bytes, the workflow
  catalog fixed. Not fixed: `MATERIALIZE TTL` is not told apart from the
  merge (the criterion asks "once merged"); each bound tried on one
  variable; the session token and the ClickHouse password on test command
  lines (test-only, throwaway stack).
- **deploy_ac_012** (PR #54): first pass, **blocking**: under `set -e`, a
  failed first command of a pipeline never wrote its status, so the second
  pipeline read the first one's `0`, and a truncated archive or one gpg
  found manipulated passed. Fixed: each status recorded and the file
  emptied before each pipeline; `restore.sh` decrypts both files before it
  stops or restores anything, and refuses non-empty stores before it stops
  a service; tests for both refusals. Second pass: `.env` readable only by
  `setup.sh`'s user (said and checked), a complete backup kept when the
  archive's deletion fails, `restore.sh`'s output checked for the
  passphrase. Not fixed: gpg 2.2.7 or later (the operations documents); a
  here-document is a temporary file under bash before 5.1; concurrent runs
  in one second are refused, not locked.
- **SPEC-023 Amendment 2026-10-25, the second** (`4f5b86e`): three passes.
  The first two found the proxy's behaviour stated too precisely (another
  TLS server name is refused only when the certificate does not cover it;
  the health check speaks HTTP/1.0; Docker's range forgotten in the
  override): the text now states requirements. The third found ADR-0022
  §2's HSTS sentence read more narrowly than its letter; the amendment says
  how it reads it, for Manuel's audit.
- **The licence check** (PR #55): first pass. Not contradicted. Weak: the
  check reads CI's glibc install, not what an image ships (the musl build
  of sharp's libvips in the dashboard's); the release half (the check in the
  release, its report as an asset) and the SBOM are the release's (#112).
  Fixed: `docs/security/dependency-review.md` and the threat model (its
  runbook line, R12) said licences were checked by hand alone; `cargo
  metadata --locked`; an ecosystem that yields no package fails; a
  malformed `WITH` refused; the install without its scripts; a timeout.
  Recorded: admitting LGPL-3.0-or-later and CC-BY-4.0 into a published
  image is a delegated decision for Manuel's audit, and the notices the
  images lack are #129.
  Second pass: the main guard compared an unresolved path, so a run through
  a link checked nothing and exited 0 (fixed: the real path); the
  repository's check now also runs in `deploy-ci.yml`, deploy_ac_019's
  place; a named package is admitted under the general list too, not only
  under its own licence; the new job's checkout keeps no credentials; the
  runbook's job count and triggers.

## Decisions

### Claude Code's, delegated or within the ADRs

Anticipated:

- **zod's major upgrade** for GHSA-mrhx-c7xc-m3w5, keeping zod 3's reading
  of every input. Discarded: accepting the advisory as Known CI debt
  (Manuel's, and a known memory-exhaustion path on the heartbeat's array).
- **The migration lock** (SPEC-023 Amendment 2026-10-25, items 1 and 2,
  delegated): five minutes; the revocation of the sixteen functions.
  Discarded: a timeout in the compose (it would kill a one-shot inside a
  migration).
- **The project name `cyberguard-enterprise`** (item 5, delegated).
  Discarded: the directory's name, which a newer bundle's directory would
  change.
- **The dashboard's standalone output off Windows** (item 4). Discarded:
  Developer Mode on Windows checkouts.
- **The proxy's uid 10001**, not 1000, which a host's first account
  usually holds, since the operator's TLS key is given to it.
- **The `app` network's fixed subnet and the dashboard's address**
  (second amendment, item 2, delegated). Discarded: a new variable for the
  trusted proxy (a deployment-contract surface for one address).
- **The proxy's scope** (second amendment, item 3, delegated): no HTTP/3;
  HSTS on the dashboard's hostname's responses. Discarded: a catch-all that
  answers another host with HSTS (it would pin a host the proxy does not
  serve).
- **The images' squash** before landing: the feature and its first review's
  fixes as one commit, one gate, one landing. Discarded: two pushes.
- **Running the bundle's tests in docker-in-docker** locally, as root.
- **Valkey with `--appendonly yes`**, as in the dev stack: sessions survive
  a restart.
- **The licence allowlist** (delegated, for Manuel's audit): the
  permissive licences, and five packages under their own licence
  (`caniuse-lite`, CC-BY-4.0; `thirty-two`, no declared licence, MIT text;
  sharp's libvips builds, LGPL-3.0-or-later; sharp's Windows build), which
  admits LGPL and CC-BY content into the published dashboard image until
  #129 removes sharp or ships the notices. Discarded: admitting LGPL in
  general; leaving the check red until Manuel decides.

Reactive:

- **The zod squash `aa56b49` removed SPEC-023's Amendment 2026-10-25 from
  `main`**: it was built on its branch's tree, cut before `9c06fd2`, with
  `9c06fd2` as its parent. The landing guard compares the branch with the
  landed tree, so it passed, and CI was green. Found while reading the
  amendment for #122's review; restored by `03b7976`, equal to `9c06fd2`.
  From now on, `git diff --stat main <branch>` before a landing lists only
  the change's files (auto-memory).
- Stale dev images migrated into `public`: rebuilt, `dev:up` builds.
- Next's standalone tree is symbolic links Windows refuses: the output by
  platform (amendment item 4).
- The proxy test's read-only `/data`: anonymous volumes in the test.
- The dashboard's address taken by the api: Docker's range moved to
  `10.254.254.8/29`.
- ClickHouse's `system.tables` readable by any user: the probe reads the
  log tables.
- The verifier could not read a root-owned `0700` directory as uid 1000.
- The heredoc and node `\\` escapes the Bash tool collapses: scripts
  written with the Write tool.
- The blocking status-file defect of `backup.sh` and `restore.sh`
  (*Self-reviews*).

### Deviations from the rules, recorded

- **`aa56b49` landed with a tree that removed a landed document**
  (*Reactive*); restored in the next commit, whose CI was green.
- **Three passes on the second amendment**, one more than the rules ask,
  since each found something.

## The threat model, for Manuel's audit

Changed this session (`7385f32`, `b794f52`):

- The cross-cutting line: the images non-root, what they run not writable
  by the process, the volumes' mount points its own; the bundle's
  containers read-only; the bundle's secrets generated by `setup.sh`.
- §8: a credential per service in each store, the stores on an internal
  network with the services and their one-shots alone, neither runtime
  ClickHouse user reading its system log tables. §9: the proxy in front.
- Closed: R2, R3, R8, R15, R16. Narrowed: R7 (remainder #124), R10
  (remainder #125), R11, R13.
- ADR-0022 §2's HSTS sentence, read by SPEC-023's second Amendment
  2026-10-25 as the responses to requests for the dashboard's hostname.
- The dependency runbook's line and R12 (`867ac68`): the production
  dependencies' and the agent's crates' licences checked against an
  allowlist; what an image ships beyond them, and an SBOM, not.

## Local environment operations

- `infra/dev/.env`: `CG_INGEST_ENROLL_PORT=18080`, with a comment.
- The dev stack's images rebuilt, its database volume recreated.
- Docker: the containers `cg-dind` (`docker:28-dind`, volume
  `cg-dind-data`, `apk add nodejs openssl gnupg gcompat libgcc`) for the
  bundle's tests, and `rust:1.93.0` with the volumes `cg-linux-target` and
  `cg-cargo-registry` for the Linux agent; `cg-dind` is stopped.

## Baselines

At the close's `main`:

- Rust: not run this session (no change to `agent/`); S45's stands, 66
  binaries, 139 passed, 7 ignored.
- `services/api`: 56 files, 387 tests passed.
- `services/ingest`, unelevated: 85 files, 80 passed, 4 failed, 1 skipped;
  495 tests passed, 4 failed (the capture marquees, the agent's exit code
  9), 3 skipped. Elevated (gate 1, `aa56b49`): 85 files, 499 passed, 2
  skipped.
- `dashboard`: 12 files, 141 tests passed.
- `deploy/docker/test/`: 74 (images 28, compose 14, setup 9, stack 19,
  licences 4), the stack's and setup's as root.
- `infra/dev/check-compose.test.mjs`: 9 of 9.

Catalogs: ADR 22, SPEC 22 (SPEC-023 amended twice). Known CI debt: none.

## Debts

Closed:

- **#122** (`2217725`): the lock wait bounded, the functions revoked.
- **#123** (`9c06fd2`, restored by `03b7976`): SPEC-023's TLS citation.
- **#92** (`7385f32`): the containers non-root.
- **#109** (`7385f32`): the rules read-only in the image.
- **#85** (`b794f52`): a Valkey user per service.
- **#86** (`b794f52`): each service connects as its Postgres role.
- **#94** (`b794f52`): the bundle's Postgres logs no parameter; R10's
  remainder is #125.
- **#103** (`b794f52`): the proxy in front of the dashboard sets
  `X-Forwarded-For`.
- **#104** (`b794f52`, `7385f32`): HSTS at the proxy.
- **#108** (`b794f52`): a ClickHouse user per service, the api's
  read-only; R7's remainder is #124.

Narrowed, destination changed:

- **#95**: the ClickHouse grants done (`b794f52`): the api reads class 1007
  alone. The data-protection document and the DPO's record remain.
- **#112**: the licence check done (`867ac68`); the SBOM to the
  release workflow.

New:

- **#124 — A compromised ingest can still rewrite the evidence the
  forensic key signs.** Ingest's ClickHouse user inserts the events, whose
  newest row for an id ClickHouse keeps, and it writes the alerts, so it
  can rewrite evidence before an export. Surfaced by the bundle's review
  (threat model R7). Destination: Manuel's.
- **#125 — A session that raises `log_parameter_max_length_on_error` for
  itself logs a passphrase.** Only a compromised service or the superuser
  would (threat model R10). Destination: Manuel's.
- **#126 — GHSA-mrhx-c7xc-m3w5 in zod below 4.6.6.** Opened and closed
  (`aa56b49`).
- **#127 — #122's remainder.** A one-shot's DDL waits without bound for a
  table lock a running service holds (an upgrade with the old services
  up); Kysely's shared lock is not bounded; an owner role can take the
  other service's key. Surfaced by #122's second review. Destination: F1's
  upgrade document (stop ingest and the api before the one-shots run).
- **#128 — The proxy image keeps Caddy's title, description, URL and
  vendor labels** beside CyberGuard's four. Surfaced by the images' second
  review. Destination: F1's release workflow, which sets the images'
  metadata.
- **#129 — The dashboard image ships sharp's libvips (LGPL-3.0-or-later)
  and `caniuse-lite` (CC-BY-4.0) without their notices.** The dashboard uses
  no `next/image`; Next installs sharp as an optional dependency, and its
  standalone output carries it. Surfaced by the licence check's review.
  Destination: F1's release workflow: sharp left out of the dashboard, and
  the images' third-party notices.

**Open at the close: 51.** Session 45 closed with 56; this session closed
ten, opened six and closed one of them.

## What waits on Manuel

- **The threat model**, this session's changes above and the earlier
  ones; with it ADR-0022 §2's HSTS reading and the remainders #124 and
  #125.
- **The delegated decisions above**, for his audit: the migration lock,
  the project name, the `app` network's addresses, the proxy's scope, the
  licence allowlist (LGPL and CC-BY content in the dashboard image until
  #129).
- **His dev stack's certificates volume**: the next `task dev:up` builds
  the non-root ingest, which cannot read the key a root-run ingest wrote;
  once, `task dev:down` then `docker volume rm
  cyberguard-dev_cg_ingest_certs` (`infra/dev/README.md`). The server
  certificate is reissued from the CA, which agents trust.
- **ADR-0022 Open question 1**, the Authenticode certificate, before the
  first public release; **#31's measurement** and **debt #7**, as before.

## State at the close

`main` = `origin/main` = the close commit, tree clean, no stash, no branch
but `main`, no worktree but the main one, no open pull request. CI on the
close SHA: ALL GREEN (*How Session 47 resumes* asks for the check again).

## How Session 47 resumes

1. *At the start* (CLAUDE.md *Session protocol*): this record, `main`, CI,
   catalogs ADR 22 / SPEC 22, Known CI debt none, `docs/debts.md` open 51;
   `C:`'s free space and the memory Docker leaves.
2. F1's remaining code (SPEC-023), on branches, one pull request per change,
   in this order:
   - the release workflow (`.github/workflows/release.yml`, §Operational
     §12) and its dry run in `deploy-ci.yml`: the images built with
     BuildKit's SBOM and provenance and pushed nowhere, the bundle step
     (the compose with each image as `<name>:<version>@sha256:<digest>`,
     `deploy/docker/` packed without `test/`, `proxy/` and the test
     override), deploy_ac_001's published-compose clause and
     deploy_ac_019's dry-run half; the images' metadata (#128); the SBOM
     (#112), which lists what each image ships; the licence check in the
     release and its report as a release asset; sharp left out of the
     dashboard and the images' third-party notices (#129);
   - `docs/operations/` (deploy_ac_021: each command `install.md` gives for
     the server one a test of `deploy/docker/test/` runs): install,
     configuration (the `app` network's addresses and their override, the
     Compose minimum version and Docker Engine 25 or later, gpg 2.2.7 or
     later, `.env` as plain `KEY=value`), backup and restore, upgrade (an
     agent's `server.url` `https://` with the CA as its anchor; browsers
     sign in again after the cookie rename; the first migration deletes
     what the retention no longer keeps; a database a release before
     SPEC-023 created is recreated; ingest and the api stopped before the
     one-shots run (#127); the project name; an override file carried to
     the newer bundle's directory; the dev certificates volume),
     data-protection (#95, the backups' keeping, the logon telemetry) and
     forensic verification;
   - the root compose stubs (#12).
3. Then F1 is done when SPEC-023's §Acceptance criteria say so; F2's SPEC
   (the agent's installer and service mode) follows.
4. The elevated gate before any change to a path it guards.
