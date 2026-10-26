# Handoff — Session 47

Session 47 opened and closed on 2026-10-26. Manuel pasted the prompt
Session 46 left, with its answer slots unfilled, and answered them when
asked (below). The prompt's order was the rest of F1's code (SPEC-023) on
branches, one pull request per change:

1. the release: `release.yml` and its dry run in `deploy-ci.yml`, with the
   SBOM, the provenance and the bundle step;
2. the proxy image's labels (#128);
3. sharp out of the dashboard, and the images' third-party notices (#129);
4. the SBOM half of #112;
5. `docs/operations/`: deploy_ac_021 (each command `install.md` gives for
   the server run by a test) and the upgrade document handoffs 45 and 46
   ask for (#127);
6. the root compose stubs (#12).

The delegation stood as in Sessions 38 to 46: what depends on Manuel is
decided for an enterprise buyer and recorded as delegated; an ADR still
needs his "sí", and the elevated gate is his to run. He also asked Claude
Code to review Session 46's delegated decisions and to change them where it
saw fit.

This session delivered items 1 to 4 and 6, with the revision of Session
46's decisions: two SPEC-023 amendments and seven pull requests. Item 5,
`docs/operations/`, has drafts on the branch `docs/s47-operations`, which
no review has read yet (*How Session 48 resumes*).

## Start

- `main` = `origin/main` = `edfe881`, tree clean, no stash, no branch but
  `main`, no pull request, no worktree. CI of `edfe881`: `markdown-lint →
  success`.
- Catalogs ADR 22 / SPEC 22, Known CI debt none, `docs/debts.md` open 51,
  as handoff-46 states.
- `C:` had 47 GB free. Docker's VM had 7.65 GB; the host had about 0.6 GB
  free with Manuel's other projects running.

Manuel's decisions this session, in chat (2026-10-26):

- The threat model: "Auditado, conforme". Debt #55 closes with it.
- Session 46's delegated decisions: "revisalo tu y en caso de modificacion
  hazlo bajo tu criterio". Claude Code revised all five (*Decisions*).
- His dev stack's certificates volume: "Bórralo tú (Recommended)".
- Memory: "Páralos tú (Recommended)": the containers of his other projects
  stopped, nothing deleted, Docker Desktop's Kubernetes untouched.
- Twice, after Claude Code's background commands were stopped for want of
  memory: "memoria liberada", and the work went on.

## Progress

Landed on `main`, in order, each commit with CI green on its pull request
and on `main`, by fast-forward, one commit per push:

| Commit | What | PR |
|---|---|---|
| `59c2bff` | SPEC-023 Amendments 2026-10-26, two: the images' labels and notices, the dashboard without an image library, the release's artifacts; the migration command's lock waits, the one-shots' order, the project name and the `app` addresses in `.env`, the proxy's answer to another host | — |
| `2f63baa`, `696329a` | #128: the nine OCI labels on the four images | #56 |
| `6fb5568`, `0386700`, `b70ed79` | #129: each image's `THIRD-PARTY-NOTICES`; sharp in no install | #57 |
| `c3e139d` | #127 narrowed: the one-shots bound every lock wait at 5 min; `api-migrate` after `ingest-migrate` | #58 |
| `e4be42f`, `74afd40` | The `app` network's addresses and the project name in `.env` | #59 |
| `2c52f0b` | The proxy answers another host `421` | #60 |
| `275288d` | #12: no compose file at the root | #61 |
| `e58ca2b` | #112's SBOM half: `release.yml`, and its dry run in `deploy-ci.yml` | #62 |

### The amendments (`59c2bff`)

The first records what items 1 to 4 of the prompt need:

- item 1, nine OCI labels on each image, each with its Dockerfile's
  value;
- item 2, `THIRD-PARTY-NOTICES` in each image;
- item 3, no sharp in any install, and `/_next/image` answering `404`;
- item 4, the release's artifacts.

The second revises Session 46's decisions:

- item 1, every lock wait of the migration command bounded at 5 min;
- item 2, `api-migrate` after `ingest-migrate`;
- item 3, the project name through `COMPOSE_PROJECT_NAME` in `.env` alone;
- item 4, the `app` network's three addresses as bundle variables;
- item 5, `421` to another host over HTTPS.

### The labels (#128)

The four Dockerfiles set title, description, URL, documentation and vendor
beside the four labels Session 46 set. `images.test.mjs` asserts the nine
labels and their values, and no other label of that prefix.

### The notices (#129)

- `.github/scripts/third-party-notices.mjs` writes each Node image's notices
  at build time. It names each package directory below `/app/node_modules`
  and, for the dashboard, its production dependencies too, read from the
  install its standalone tree was traced from. For each, it gives the
  licence, author and repository, and every licence or notice file.
- A package with no licence file gets the standard text of its declared
  licence, once.
- The build fails on a package it cannot attribute, or one whose files are
  not where `--sources` says.
- The proxy's notices are a static file.
- sharp is in no install (`ignoredOptionalDependencies`), and the
  dashboard's optimiser is off. The allowlist loses its three LGPL entries.

### The lock (#127)

- A waiting run sets `lock_timeout` in pg-pool's awaited `onConnect` hook,
  on every connection. That bounds the command's own lock, Kysely's
  migrator lock and every migration statement, and a connection the bound
  cannot be set on fails the run.
- A migration's lock that runs out prints the second line, and nothing
  commits.
- The services' own run is unchanged.

### The `app` addresses and the project name

`CG_APP_SUBNET`, `CG_APP_IP_RANGE` and `CG_DASHBOARD_APP_ADDRESS` are bundle
variables, and `API_TRUSTED_PROXIES` reads the same variable as the
dashboard's address. `COMPOSE_PROJECT_NAME` in `.env` renames the project,
and `setup.sh` keeps the line.

### The proxy's `421`

A catch-all HTTPS site answers any other host `421`, with no body and no
HSTS. It names no TLS policy of its own: the handshake stays the dashboard
site's, as before.

### The root stubs (#12)

Removed. `deploy/README.md`, `README.md` and the roadmap no longer describe
them. The roadmap's Part (b) §2 to §4 are marked as decided on 2026-10-23.

### The release (#112)

- `release-images.mjs` builds the images in a docker-container builder,
  BuildKit and the SBOM generator pinned by digest, with SBOM and
  provenance.
- `release-bundle.mjs` packs the bundle reproducibly.
- `release.yml` has five jobs, the licence check first.
- `deploy-ci.yml`'s second job is the dry run, which
  `deploy/docker/test/release.test.mjs` reads.

## Gates

No elevated gate this session. No change touched a path it guards
(*Developer-local SPEC-005 marquee validation* §5, *SPEC-006* §6): the
lock's `migrate.ts` and the Dockerfiles are outside them. The last elevated
run (handoff-46, gate 1, `aa56b49`) does not cover `main`'s tip: the next
change to a guarded path runs the gate.

Local gates, before each push, by the script
`full-gate.sh <logdir> [workspaces]` (scratchpad). It:

- builds the four images from the commit;
- runs `images.test.mjs` on the host;
- runs CLAUDE.md's TS gate in each workspace given;
- runs `deploy/docker/test/` in `cg-dind` as root, the images loaded and
  the tree by `git archive`, after clearing the stacks a stopped run left.

Literal lines:

```text
HEAD b378bd8b709a1a1afcc91c644fcd7aa8bd163d14 feat/s47-image-labels   (#128, landed as 2f63baa)
images.test (host): ℹ pass 28 ℹ fail 0
ingest: Tests  4 failed | 495 passed | 3 skipped (502)
api: Tests  387 passed (387)
dashboard: Tests  141 passed (141)
dind: # tests 74 # pass 74 # fail 0

HEAD 4fe22ec7b2de97390f2728f526de03bce011210e feat/s47-notices        (#129; amended to 6fb5568 before its dind step)
build ingest ok / build api ok / build dashboard ok / build proxy ok
images.test (host) exit 0 ℹ pass 33 ℹ fail 0
ingest: Tests  4 failed | 495 passed | 3 skipped (502)
api: Tests  387 passed (387)
dashboard: Tests  141 passed (141)
dind exit 0 # tests 85 # pass 85 # fail 0 # skipped 0                  (at 6fb5568)

HEAD 0386700551b0fdd9c2396c1bd05f7de8200dd2a4 feat/s47-notices
images.test (host) exit 0 ℹ pass 33 ℹ fail 0
dind (rerun) exit 0 # tests 89 # pass 89 # fail 0 # cancelled 0 # skipped 0
unit (host) exit 0 ℹ pass 14 ℹ fail 0
(b70ed79, a test of the lockfile alone: licences.test 6/6 on the host)

HEAD c70e6b57ded0b6d5a1992ff7b4414effe9edd205 feat/s47-migrate-locks
images.test (host) exit 0 ℹ pass 33 ℹ fail 0
ingest: Tests  4 failed | 498 passed | 3 skipped (505)
api: Tests  390 passed (390)
dind exit 0 # tests 91 # pass 91 # fail 0 # skipped 0
HEAD 1ebb3f18ef47282c587fc12a005091e6cff7cd5f feat/s47-migrate-locks   (landed squashed as c3e139d, same tree)
images.test (host) exit 0 ℹ pass 33 ℹ fail 0
ingest (rerun): Tests  4 failed | 498 passed | 3 skipped (505)
dind exit 0 # tests 91 # pass 91 # fail 0 # skipped 0

HEAD e22b8208260f9f68b3a5253832b153de32786cfc feat/s47-app-addresses   (the tree of e4be42f)
images.test (host) exit 0 ℹ pass 33 ℹ fail 0
dind exit 0 # tests 96 # pass 96 # fail 0 # skipped 0
compose.test (host) exit 0 ℹ pass 19 ℹ fail 0
HEAD 74afd407ed09a2ab0db14639c25260e5bb0f5448 feat/s47-app-addresses
images.test (host) exit 0 ℹ pass 33 ℹ fail 0
dind exit 0 # tests 96 # pass 96 # fail 0 # skipped 0
compose.test (host) exit 0 ℹ pass 19 ℹ fail 0

HEAD 2c52f0b8aff19c74b5dce9b758d92f1c7f5c7863 feat/s47-proxy-421
images.test (host) exit 0 ℹ pass 33 ℹ fail 0
dind exit 0 # tests 97 # pass 97 # fail 0 # skipped 0
provided: SNI cg.example.test: completed, certificate cg.example.test
provided: SNI other.example.test: refused (ERR_SSL_TLSV1_ALERT_INTERNAL_ERROR)
provided: SNI (none): refused (ERR_SSL_TLSV1_ALERT_INTERNAL_ERROR)
internal: SNI cg.example.test: completed, certificate ?
internal: SNI other.example.test: refused (ERR_SSL_TLSV1_ALERT_INTERNAL_ERROR)
internal: SNI (none): refused (ERR_SSL_TLSV1_ALERT_INTERNAL_ERROR)

275288d (#12): markdownlint-cli2 on README.md, deploy/README.md, docs/product/roadmap.md: Summary: 0 issues in 0 files

HEAD e58ca2be7ce0c0721a3a6266b2a034689389cdad feat/s47-release
Docker Engine 29.1.2
Docker Compose version v2.40.3-desktop.1
github.com/docker/buildx v0.30.1-desktop.1 792b8327a475a5d8c9d5f4ea6ce866e7da39ae8b
release-images exit 0
release-bundle exit 0
release.test exit 0 ℹ tests 11 ℹ pass 11 ℹ fail 0 ℹ skipped 0
```

The ingest failures in every run are the four capture marquees, which the
agent leaves with exit code 9 ("insufficient privilege to open
Microsoft-Windows-Kernel-Process ETW session"): the expected unelevated
result.

Twice the system stopped Claude Code's background commands for want of
memory while the session was idle:

1. the first gate of #129, at `4fe22ec`, during ingest's tests;
2. later, the CI watch of `c3e139d` and the release's first dry run.

Neither was restarted until Manuel freed memory and said so. Each time
their child processes, test containers and the BuildKit builder were
stopped by hand.

## Self-reviews

Each code change: Claude Code's matrix, then a reviewer with a fresh
context and CLAUDE.md's fixed prompt, then another pass after the fixes;
each SPEC text: Claude Code's pass, then the fixed text prompt. Every
*weak*, *missing* or *contradicted* verdict and every list-A entry was
fixed or is recorded here with its reason.

- **The amendments** (`59c2bff`): two passes.
  - First pass, list A: the label rule clashed with Caddy's `licenses`
    label, also Apache-2.0, so it was reworded to "every label of the
    prefix is one of the nine"; and "its shell scripts executable" was
    false, since `redis/entrypoint.sh` is `0644`, and was reworded.
    Fourteen list-B points were fixed.
  - Second pass: a classification sentence, fixed.
- **#128** (PR #56): two passes. The first found the version and the
  description asserted weakly; `696329a` asserts the version against
  `CG_TEST_VERSION` and a description that names its image. The second
  pass confirmed it.
- **#129** (PR #57):
  - The matrix: the generator's own behaviour had no test: a package
    without a licence file, the build's failure, the deduplication.
    `notices.test.mjs` was added, and each of seven mutations of the
    generator fails exactly one of its tests.
  - The first pass. List 2: the file-name pattern skipped
    `CopyrightNotice.txt` (reflect-metadata, tslib) and
    `ThirdPartyNoticeText.txt` (typescript); a manifest's homepage was
    printed as its repository; the no-file sentence named an author that
    was absent. List 1: the no-sharp test passed on an empty listing, and
    `--sources` and `--pnpm` were untested. List 3: the allowlist's
    comment claimed the agent's crates. All were fixed in `0386700`.
    - The wider pattern also found `MIT.LICENSE` and `ROME-LICENSE-MIT`.
    - The no-sharp test now searches the whole filesystem as root. As the
      image's user, `find` exited 1 on `/root`.
    - `licences.test.mjs` asserts the allowlist refuses LGPL.
  - The second pass.
    - The lockfile was not asserted: `b70ed79`.
    - Next's 112 vendored manifests in `next/dist/compiled` are named only
      through Next's licence files, and three `@edge-runtime/*` carry
      none. Not fixed under the natural reading of "package directory";
      recorded as #130.
    - `NOTICE`'s pointer is not asserted apart from the byte comparison of
      the file. Not fixed: the comparison covers it.
- **The lock** (PR #58):
  - The matrix. The suite assertion item 1 asks for was weak: the test
    locked the ledger with no migration pending. Replaced before the first
    push, over a database whose last three migrations are pending again:
    - a write in flight on a table the second of them changes;
    - the lock Kysely takes before the migrations.

    Both failed against `main`'s `migrate.ts`, by a 30 s timeout.
  - The first pass, fixed:
    - 1m, the api's wait for both one-shots, untested: added;
    - 3b, the `SET` fire-and-forget, so that a connection it failed on
      waited without bound: moved to `onConnect`; a run whose bound
      Postgres refuses (`NaN`) fails, while the intermediate code ended
      "successfully";
    - the roles' wording.

    Not fixed, with reason: the 300 s and the exit 1 by composition, since
    the constant is asserted and a five-minute test is out of proportion.
  - The second pass, list 2: c70e6b5 changed ingest's failure message for
    the services' own run too, which item 1 leaves unchanged. Restored
    (`1ebb3f1`).
  - The third pass: `c70e6b5` was untrue at its own SHA. The branch was
    squashed into `c3e139d`, with `1ebb3f1`'s tree, and CI ran again.
    - The `NaN` tests pass against `main`, whose code also failed there.
      Kept: they fail against the swallowed `SET`.
    - Not fixed, with reason: ClickHouse's waits, outside the contract's
      bound.
- **The `app` addresses** (PR #59):
  - The first pass:
    - `.env.example`'s values untested;
    - the networks under another project name untested;
    - the comment's second case.

    All fixed, folded into `e4be42f`.
  - The second pass: the comment did not say a second installation also
    needs its own project name and ports, so its `up` would replace the
    first's containers. Fixed in `74afd40`, its sentence checked by Claude
    Code.
  - Recorded:
    - a CIDR in `CG_DASHBOARD_APP_ADDRESS` is refused by Docker's
      container create; the reviewer probed it, no test;
    - a developer's local `deploy/docker/.env` could break the
      `--no-interpolate` read.
- **The `421`** (PR #60):
  - The first pass, list 3. The catch-all imported the TLS snippet, so in
    provided mode a handshake for any SNI, or none, completed and showed
    the operator's certificate. The import was removed; the probe above
    shows both modes refuse again.
  - The second pass: the comment overclaimed, since a certificate covering
    other names completes their handshake, which is then answered 421.
    Fixed.
  - Recorded:
    - the image half's "proxied nowhere" weak; the stack half asserts it;
    - a malformed request's answer not asserted;
    - the trailing-dot host and the absolute-form target, which the
      contract's text does not decide.
- **#12** (PR #61):
  - The first pass: `README.md:26` and `roadmap.md:377` still listed the
    stubs, Part (b) §2 still listed the compose's location as open, and
    §4's date: fixed.
  - The second pass: §2 overstated the ratification (ADR-0022 was
    ratified, SPEC-023 Accepted with it), §3 contradicted phase F, and
    `deploy/README.md` omitted the test override: fixed.
- **The release** (PR #62):
  - The matrix: `.github/workflows/README.md` said "the four OCI labels",
    and two places cited a runbook that does not exist yet. Fixed before
    the review.
  - The first pass, all fixed:
    - the bundle's listing tested by `LEFT_OUT` from the script under
      test, and its modes on three files: both now read
      `git ls-files -s`;
    - the SBOM sampled in the dashboard alone: each image now names an
      Alpine package and its Node packages;
    - #112 called closed in two documents, and R12 promising an SBOM of
      everything shipped: corrected to the index's attestation of the OS
      and Node packages;
    - the workflows README's "each workflow runs when its file changes".

    Recorded:
    - `mode=max` provenance publishes the build arguments, none secret;
    - the reproducible tarball, a delegated packaging choice;
    - the pre-release suffix and `--ref` options;
    - no technical guard on the first release: #135.
  - The dry run, reactive:
    - the test set `COMPOSE_FILE` to empty, which Compose reads as the
      directory: now unset;
    - Git Bash's tar marks a file starting with `#!` executable: the bundle
      step runs in a Linux container locally.
  - The second pass: no blocking finding. Recorded:
    - the tags not asserted;
    - the release-built images' non-root and capability properties
      asserted on deploy-ci's own builds only, as the contract allows;
    - a reused local builder not checked against the pin.

## Decisions

### Claude Code's, delegated or within the ADRs

Anticipated:

- **Session 46's five delegated decisions, revised** (second amendment;
  the first's allowlist):
  - the lock bound on the command's own lock alone, now every wait;
  - `-p`, now `.env` alone;
  - an override file for the `app` addresses, now three variables;
  - another host answered `200` with an empty body, now `421`;
  - LGPL admitted for sharp, now refused, with sharp left out.

  Each case is in the amendment's *What surfaced it*.
- **The nine labels** (item 1, delegated). Discarded: removing the base
  image's labels, which no Dockerfile instruction does; overriding them is
  the mechanism.
- **The notices** (item 2, delegated): a repository script run in each
  build, the standard text for a file-less package, and the build failing
  on what it cannot attribute. Discarded: a third-party licence tool (a new
  dependency); notices for the licence check's packages alone (the images
  ship more).
- **sharp out** (item 3, delegated): `ignoredOptionalDependencies` and
  `images.unoptimized`. Discarded: shipping libvips with its LGPL notices
  and a source offer, in the internet-facing image.
- **The release** (item 4, delegated):
  - a docker-container builder on BuildKit v0.34.0 and
    buildkit-syft-scanner 1.12.0, pinned by digest;
  - provenance `mode=max`;
  - `actions/attest-build-provenance` v4.2.2;
  - the dry run as OCI layouts;
  - a reproducible tarball owned by root.

  Discarded: the runner's default builder, whose BuildKit follows the
  runner.
- **The lock bound** (second amendment, item 1, delegated): `lock_timeout`
  on every pooled connection. Discarded: `statement_timeout`, which kills a
  long migration that waits for nothing; a compose timeout, which kills a
  one-shot inside a migration.
- **`api-migrate` after `ingest-migrate`** (item 2).
- **The project name and the addresses in `.env`** (items 3 and 4,
  delegated). Discarded: `-p`, which `backup.sh` and `restore.sh` do not
  follow; an override file, where a mismatch with `API_TRUSTED_PROXIES`
  fails silently.
- **`421` from a catch-all with no TLS policy** (item 5, delegated).
  Discarded: the empty `200`, which a monitor reads as up; the catch-all
  with the provided policy, which showed the certificate to scanners.
- **The order of work**:
  - the branches stacked in landing order, the release last, since its
    dry run is the heaviest gate and its listing test covers the final
    tree;
  - restacked with `git replay`;
  - a gate carried across a restack only where `git diff --quiet` showed
    the same tree.

  Discarded: the prompt's order with the release second, which would have
  held three small changes behind it.
- **The threat model's R12 and the dependency runbook**, changed with the
  release (delegated; SPEC-023 §Security considerations), for Manuel's
  audit (below).

Reactive, beyond the reviews' fixes above:

- `cg-dind` kept a stopped run's stack, whose `app` subnet made the next
  run's network fail ("Pool overlaps"): removed, and `full-gate.sh` now
  clears the stacks first.
- A testcontainers Postgres that never logged ready under memory pressure
  (the ingest suite reported "No test files found"): the suite rerun.
- The CI watcher keyed runs by name and event, after reading a PR's runs
  for a push.

### Deviations from the rules, recorded

- **The fixed prompt carried one sentence more** for the `app` change's
  first pass, in its Contract field: "The installation and configuration
  documents those items mention are not part of this change." The rest was
  verbatim.
- **`4fe22ec` was amended to `6fb5568` during its gate**, adding
  `deploy/docker/test/notices.test.mjs` alone. The TS gate and the images
  ran on `4fe22ec`, whose tree differs only by that file; the `cg-dind`
  run was on `6fb5568` and included it.
- **Gates carried by tree identity.** The api's suite ran at `c70e6b5`,
  and `services/api` did not change after. The app's gate ran at
  `e22b820`, the tree of `e4be42f`. The lock landed as `c3e139d`, with the
  tree of `1ebb3f1`.
- **#12's gate is markdownlint.** Its tree differs from `2c52f0b`'s,
  gated, only in the three documents and the two deleted stubs.
- **The fixes after a second pass** were text: the `app` comment, the
  `421` comment, and #12's lines. Claude Code checked each sentence
  against the code, without a third reviewer pass. The lock's fixes after
  its second pass did get a third pass.

## The threat model, for Manuel's audit

Changed this session (`e58ca2b`): R12, from "no SBOM is produced" to: each
released image's index carries an SBOM attestation that lists the
operating-system and Node packages installed in it, and each image carries
its third-party notices. Debt #112 closes with it. The dependency
runbook's matching sentence changed with it.

## Local environment operations

- Manuel's other projects: 16 containers stopped (`docker stop`), nothing
  deleted. `aura_cache` and `aura_db` restart with Docker, having
  `restart=always`.
- His dev stack:
  - `task dev:down`, the volume `cyberguard-dev_cg_ingest_certs` removed,
    `task dev:up`: ingest and the api run as uid 1000, healthy, and the
    server certificate was reissued from the CA;
  - its five containers stopped (`docker stop`) during the gates for
    memory, and started again at the close (`docker start`).
- `cg-dind`: the stacks a stopped run left removed; stopped.
- The buildx builder `cg-release` (docker-container, `moby/buildkit:v0.34.0`)
  that the dry run creates: removed at the close (`docker buildx rm`).
- The bundle step's local runs in `node:22.23.4-alpine3.24` containers
  (`--rm`, `apk add git tar gzip` inside).
- No installation.

## Baselines

At the close's `main`:

- Rust: not run this session (no change to `agent/`); S45's stands.
- `services/api`: 56 files, 390 tests passed.
- `services/ingest`, unelevated: 85 files, 80 passed, 4 failed, 1
  skipped; 498 tests passed, 4 failed (the capture marquees, the agent's
  exit code 9), 3 skipped. Elevated: not run this session (handoff-46,
  gate 1).
- `dashboard`: 12 files, 141 tests passed.
- `deploy/docker/test/` in `cg-dind`: 97 at `2c52f0b`. The release adds
  `release.test.mjs`: 5 tests without a dry run, and 11 with one.
- `images.test.mjs` on the host: 33.
- `infra/dev/check-compose.test.mjs`: not run (no change to `infra/dev/`).

Catalogs: ADR 22, SPEC 22 (SPEC-023 amended twice more). Known CI debt:
none.

## Debts

Closed:

- **#12** (`275288d`): no compose file at the root.
- **#55** (no commit): Manuel audited the threat model, "Auditado,
  conforme" (2026-10-26).
- **#112** (`e58ca2b`; the licence check `867ac68`): the SBOM in the
  release, the licence check in it and its report.
- **#128** (`2f63baa`, `696329a`): the nine labels.
- **#129** (`6fb5568`, `0386700`, `b70ed79`): the notices; sharp in no
  install.

Narrowed, destination unchanged:

- **#127** (`c3e139d`): every lock wait of the one-shots is bounded, and
  `api-migrate` runs after `ingest-migrate`. What remains:
  - a one-shot's statement still queues for a running service's table
    lock, up to the bound, and the service's writes on that table wait
    behind it;
  - an owner role can take the other service's key, and the other one-shot
    then fails after the bound.

  Destination: F1's upgrade document (ingest and the api stopped before
  the one-shots run).

New:

- **#130 — Third-party notices beyond the Node packages.**
  - The proxy's notices name Caddy and its Go modules without their
    licence texts (its SBOM lists 149 of them).
  - The agent's executable carries no notices of its crates.
  - The base images' GPL packages (BusyBox among them) are named only as
    their publisher distributes them, with no source offer.
  - Next's vendored packages in `next/dist/compiled` are named only
    through Next's licence files, and three `@edge-runtime/*` carry none.

  Surfaced by #129's work and its second review. Destination: before the
  first release (F4).
- **#131 — The agent reports Cargo's version, not the release's.**
  `agent_version` is `env!("CARGO_PKG_VERSION")`, `0.1.0`
  (`agent/cg-agent/src/lib.rs:116`), and `release.yml` builds without
  changing it. Destination: F2's SPEC, the installer.
- **#132 — A locally built image's `documentation` label points to
  `tree/unknown`**, since `CG_REVISION` defaults to `unknown`. Destination:
  a sweep.
- **#133 — The dashboard image ships `typescript`**, which Next's output
  tracing carries into the standalone tree. Surfaced by #129's first
  review. Destination: a sweep (`outputFileTracingExcludes`).
- **#134 — The proxy refuses a handshake for a name its certificate does
  not cover, or for none, and no SPEC states it.** That is Caddy's
  behaviour with the dashboard site's policy, and no test asserts it (a
  test would be stricter than SPEC-023). Surfaced by the `421` review.
  Destination: the next SPEC-023 amendment, with `docs/operations/`.
- **#135 — Nothing technical keeps a tag from publishing before F2 to F4
  and Manuel's OK.** Anyone who can push a `v*.*.*` tag publishes images,
  attestations and a GitHub Release. Surfaced by the release's review.
  Destination: Manuel's: a `release` environment with him as a required
  reviewer, or a tag ruleset.

**Open at the close: 52.** Session 46 closed with 51; this session closed
five and opened six.

## What waits on Manuel

- **#135**, before any tag: a protected `release` environment or a tag
  ruleset (repository settings).
- **The delegated decisions above**, for his audit:
  - the five revised;
  - the labels, the notices, sharp out, the release's builder, pins and
    provenance;
  - the lock bound, the project name and the addresses in `.env`, the
    `421`.
- **The threat model's R12** (above). **#124** and **#125** are his, as
  before.
- **ADR-0022 Open question 1**, the Authenticode certificate, before the
  first public release; **#31's measurement** and **debt #7**, as before.

## State at the close

`main` = `origin/main` = the close commit, tree clean, no stash, no worktree
but the main one, no open pull request. One branch beside `main`:
`docs/s47-operations` (`626b23d`, on `696329a`), the drafts of
`docs/operations/`, pushed so that they persist, with no pull request. CI
on the close SHA: ALL GREEN (*How Session 48 resumes* asks for the check
again).

## How Session 48 resumes

1. *At the start* (CLAUDE.md *Session protocol*):
   - this record, `main` and CI;
   - catalogs ADR 22 / SPEC 22, Known CI debt none, `docs/debts.md` open
     52;
   - `C:`'s free space and the memory Docker leaves.
2. `docs/operations/`, the rest of F1 (SPEC-023 §Scope 13, deploy_ac_021).
   Start from the drafts on `docs/s47-operations`, rebased onto `main`:
   README, install, configuration, backup-and-restore, upgrade,
   data-protection, forensic-verification.
   - First, a SPEC-023 amendment for the contract points the drafts left
     open:
     - users after the first: the drafts propose `create-user.js --role`,
       which writes a `user.create` audit row;
     - the Docker daemon settings that keep the client's address for the
       proxy and ingest;
     - the minimum Docker Engine and Compose versions: the dry run prints
       the runner's, and locally they were Engine 29.1.2 and Compose
       v2.40.3;
     - where an operator-provided ingest pair goes;
     - a logging-driver example;
     - #134's handshake.
   - Then deploy_ac_021's test: each server command of `install.md` run by
     a test of `deploy/docker/test/`, with a pty for `create-user`.
   - Then the upgrade document, with #127's remainder (ingest and the api
     stopped before the one-shots run) and the points handoff-46 lists.
3. Then F1 is done when SPEC-023's §Acceptance criteria say so; F2's SPEC
   (the agent's installer and service mode, with #131) follows.
4. The elevated gate before any change to a path it guards.
