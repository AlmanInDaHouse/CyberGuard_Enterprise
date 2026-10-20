# Handoff — Session 43

Session 43 opened on 2026-10-19. Manuel set its order: first the SPEC of
roadmap §F (criterion 7: the production compose, the operator's
configuration and secrets surface, TLS and CA, published images, the
dashboard in the compose), with the debts §F carries; what needs a new ADR
or an amendment, proposed on an owner-review branch; while the SPEC waits
on him, the sweep: #117, the remaining half of #110, and #111's
rustls-pemfile. He delegated again every decision that waits on him, an
ADR text and the elevated gate excepted.

## Start

- `main` = `origin/main` = `b71855e`, tree clean, no stash, no branch but
  `main`, no open pull request. CI of `main`'s tip: `markdown-lint →
  success`, and the weekly `dependency-audit` of 2026-10-19 (13:25 UTC) →
  success.
- Catalogs ADR 21 / SPEC 21; Known CI debt none; `docs/debts.md` open 63,
  which matches its rows. All as handoff-42 states.
- No `CGAgent` ETW session. Docker Desktop was stopped, and was started
  (not restarted). `C:` had 30 GB free.

Manuel's decisions this session, in chat:

- 2026-10-19, opening the session: the order above, and the delegation, as
  in Sessions 38 to 42 ("lo que dependa de mí, decídelo tú en beneficio del
  proyecto, para un comprador empresarial, registrando cada decisión como
  delegada. Un ADR sigue necesitando mi «sí» explícito, y el gate elevado lo
  sigo corriendo yo").
- The same message kept a slot for each of handoff-42's three questions,
  and each reached the session holding the template's placeholder. Put to
  him as one question with three parts, each with a recommendation, he
  chose on 2026-10-19:
  - #115 and #117's ADR amendments: "Redáctalas (Recommended)";
  - #31's measurement on this machine: "Todavía no (Recommended)";
  - the NATS and MinIO volumes and variables: "Bórralos tú (Recommended)".

## Progress

### Roadmap §F's contract, on an owner-review branch

`adr/s43-f-deployment`, two commits, pushed, no pull request (an
owner-review branch runs no workflow):

1. `368faa6`, NOT YET RATIFIED: **ADR-0022** (Proposed), *Single-node
   production deployment — Docker Compose behind a TLS proxy, a credential
   per service, published images*, with five co-located amendments:
   ADR-0001 (no compose file at the root, debt #12; the production compose
   in `deploy/docker/`; `infra/` for the developer environment; one release
   tag), ADR-0003 (an event's row stays in ClickHouse for its class's whole
   period while MinIO is not built), ADR-0004 (enrollment over TLS 1.3, the
   agent validating the server against its configured trust anchor),
   ADR-0016 (the forensic public key's trust anchor: its SHA-256
   fingerprint, recorded outside CyberGuard at installation, and a shipped
   verifier) and ADR-0019 (retention as the operator's setting per class,
   365 days by default). The catalog names each.
2. `6a0bb06`, NOT YET RATIFIED: **SPEC-023** (Proposed), *Production
   deployment — the bundle, the operator's configuration and secrets, TLS,
   published images*: the bundle (compose, environment template, set-up
   script, the stores' initialisation, backup and restore), four product
   images (ingest, the api, the dashboard and a proxy built from Caddy's),
   the operator's configuration reference for every service, a scoped
   credential per service in Postgres, Redis and ClickHouse (two roles per
  service in Postgres, the migrations in one-shot containers), the server
   certificate's names, enrollment over TLS on both ends, the release with
   SBOMs, provenance attestations and a licence check, the forensic key's
   commands and verifier, the retention per class, the relay's private CA,
   the bootstrap command's password from standard input. Twenty-one
   acceptance criteria, deploy_ac_001 to deploy_ac_021.

Its compare URL:
<https://github.com/AlmanInDaHouse/CyberGuard_Enterprise/compare/main...adr/s43-f-deployment>.
Nothing lands until Manuel's "sí" to ADR-0022 and its amendments; SPEC-023,
which rests on them, lands with them, its status then `Accepted`.

What §F's phase becomes (SPEC-023 §Context and §Out of scope, ADR-0022
§Out of scope): four SPECs, then a timed installation.

- **F1, SPEC-023**: the server's installation, as above.
- **F2, the agent's installation**: its installer and service mode
  (ADR-0010 §Decision part 2's supersession), its key custody (debt #87),
  its log file, and ADR-0019's and ADR-0020's open questions on the
  endpoint.
- **F3, the dashboard's missing pages**: the TOTP enrollment and the user
  management (#83, #98), the api's login deleting the prior session
  (#102), the agents and the enrollment tokens, the alerts, the forensic
  export and report, the playbook records, the password change. Today the
  dashboard calls four api routes, and with the api on an internal network
  the rest reach no browser.
- **F4, the agents' certificate renewal** (#88), with the rest of #89.
- **Criterion 7**: an installation from the runbook, timed below 30
  minutes, after F1 to F4 (deploy_ac_020). The first public release waits
  for F2 to F4 (ADR-0010 asks the packaging SPEC before any external
  deployment; F3 makes the product usable; F4 keeps a released agent
  accepted past 90 days, R5), and for Manuel's OK.

Reviews, the fixed SPEC prompt, six passes; their List A points and what
was done, by pass:

- **First pass, on the first draft (15 points), all fixed:**
  - the export and the report, and every api route the dashboard does not
    call, reach no browser once the api is on an internal network (named
    in both texts, given to F3);
  - deploy_ac_020 asked for an agent "seen by the dashboard", which no
    SPEC delivered (the agents view given to F3);
  - the rule that every secret is required made SMTP mandatory;
  - the superuser and ClickHouse's administrator are used by backup and
    restore too;
  - the 180-day default cut ADR-0003's 365-day total without amending it
    (default 365, and ADR-0003's in-place amendment);
  - the enrollment listener now serves the server certificate, which
    SPEC-004 Amendment 2026-10-15's swap and expiry rules must cover;
  - SPEC-020's auth_ac_003 asserted a 1007 row kept past 365 days
    (restated by deploy_ac_011);
  - debt #87 and R1 moved without saying so;
  - ADR-0004 §Compliance misquoted, and no ADR-0004 amendment (added);
  - R5 missing from the §F list;
  - SBOMs required of images the product does not build;
  - two definitions of the CA fingerprint;
  - `issue-token` "unchanged" yet validating less;
  - backups unencrypted, unstated;
  - the threat model's year of log retention.
- **Second pass (8 points), all fixed:**
  - deploy_ac_010 still fetched the export through the proxy (now from
    the api's network with an admin's session);
  - the verifier was to name the event that changed, which an export
    without per-event hashes cannot show (now: the chain-head line);
  - the new "self-issued" rule rewrote a CA-signed pair servercert_ac_004
    keeps (now recognised by `O=CyberGuard self-issued` or SPEC-004's old
    profile);
  - the SMTP failure code changed without amending SPEC-022 (§Operational
    §14, soar_ac_006 restated);
  - an *Out of scope* item without a destination;
  - ADR-0004's amendment called itself the superseding ADR;
  - "pinned internal CA" against an operator's CA;
  - Redis "the commands it runs" against whole categories (now the list
    of each client's commands and scripts).
- **Third pass (5 points and one arguable), all fixed:**
  - the api barred from the tables F3's pages need (now: a later route
    gets a scoped grant in its SPEC, and no api grant ever reaches `ca`);
  - ADR-0004 §Transport declared unchanged while the anchor may be the
    operator's (restated);
  - ADR-0003's *Effect* left out §Retention model, part of §Decision;
  - `setup.sh`'s second run contradicted itself (three runs now: copy,
    fill, idempotent);
  - the proxy's two volumes, one listed, both root-owned under a non-root
    proxy, and Caddy's binary needing `NET_BIND_SERVICE` through its file
    capability (now a product image, `cyberguard-proxy`, which removes the
    capability and owns its volumes);
  - publishing the agent before its packaging SPEC, against ADR-0010:95
    (now the first release waits for F2).
- **Fourth pass (5 points), all fixed:**
  - the proxy called both the product's image and a third party's (now:
    the bundle's compose pins every image by digest; only the stores'
    carry their publishers' attestations);
  - ADR-0001's amendment said the bundle is the whole directory, with the
    proxy's configuration (now: the proxy's sources and the tests aside);
  - ADR-0004's amendment added the anchor to step 2 while calling the
    other steps unchanged;
  - the backups: the threat model names encrypted backups as the
    mitigation of a backup's leak (`docs/security/threat-model.md:169`),
    and SPEC-023 is the operational SPEC it defers to (now: `backup.sh`
    encrypts with `gpg`'s symmetric mode under a generated
    `CG_BACKUP_PASSPHRASE`);
  - ADR-0003's MinIO tier routed only in part (its three uses named).

  From its List B, taken: a schema per service in Postgres, so that
  neither role can plant a table or a function the other resolves
  (CVE-2018-1058's shared-schema case; ADR-0022 §4 and SPEC-023 §4); a
  changed retention materialised (`MATERIALIZE TTL`), so that older rows
  are measured against the new period; the Redis health check's password
  through `REDISCLI_AUTH`; ADR-0021's open question given a reopen
  condition; an operator-provided agent-facing pair's place; the Linux
  agent build in `deploy-ci.yml`; when SPEC-023's code is done, against
  the release-time criteria; HSTS without the key pinning the threat
  model's §9 names, which browsers dropped.
- **Fifth pass (3 points), all fixed:**
  - the schema per service did not stop a compromised ingest from running
    SQL as the api: owning the tables the api reads, it could attach a
    row-level policy or swap in a view whose function runs with the
    reader's rights, so "R3 closes" was false (now: two roles per service,
    an owner that runs the migrations in a one-shot container,
    `ingest-migrate` or `api-migrate`, and a runtime role that owns
    nothing and creates nothing; deploy_ac_004 asserts it);
  - where backups are encrypted contradicted itself (now: on the host, as
    each stream leaves its container; ClickHouse's archive is clear only
    on its backup disk until deleted);
  - a Risks row still described backups in clear.

  From its List B, taken: each service's search path set on its
  connection, so the dev stack resolves names as production does; the
  row policy widened by a rule that cites another class too; SPEC-002's
  enrollment tests and FR-013 under the new rule; the proxy's uid 10001,
  not the host's first account's 1000, since it is given a host file; the
  old dev key's reason (a `0600` key the non-root ingest cannot read); F1
  to F4 named in SPEC-023's *Context*; a playbook record's triggers out of
  the running ingest's reach (ADR-0021's open point, narrowed).
- **Sixth pass (3 points), all fixed, its fixes not re-reviewed:**
  - the one-shots' `restart: "no"` against "every container with
    `restart: unless-stopped` and a health check" (now: the long-running
    ones);
  - the first release waiting for F2 in one place and for F2 to F4 in
    another (now F2 to F4 everywhere);
  - deploy_ac_021 asking a Linux test to run the runbook's Windows agent
    steps (now: the server's commands; the endpoint's are the elevated
    gate's).

  From its List B, taken: `audit_log` append-only by grant (`UPDATE` and
  `DELETE` revoked from the api's role) and `TEMPORARY` revoked from
  `PUBLIC`; the one-shots' command (`node dist/db/migrate.js`); no
  service setting `INGEST_ENROLL_TLS=false`; an installed agent with
  `http://` beside `[enrollment]` named in the upgrade document;
  ADR-0004's anchor sentence true at its SHA; the duplicated SPEC-004 in
  the header. Its other List B points (the ingest health check during the
  certificate's expiry window, `docker compose up --wait` with one-shots,
  Caddy's catch-all redirect for the health check, `pg_restore` over the
  initialised schemas, `gh` for attestation checks, the dev host's
  `chown`) are implementation's, for F1's code.

From the passes' List B, taken among others: a Redis ACL test for the
forge path (`SET cgsess:x`); ClickHouse `readonly = 2`; the Redis
`CLIENT` subcommands narrowed; the redirect's `Location` naming the public
port; `setup.sh`'s tools POSIX and its secrets in hexadecimal; restore with
the services stopped; the dev volume `cg_ingest_certs` a root-run ingest
wrote; `deploy-ci.yml` on `services/**`, `dashboard/**` and `agent/**`;
the report's authenticity caveat; the open questions whose reopen
condition was §F, each routed; the Redis image's licence, RSALv2 and
SSPLv1 since 7.4, raised as ADR-0022 Open question 2, for Manuel. Not
taken, recorded: a vault, store TLS and high availability have no
destination in the MVP; the `@v24`-style moving tags are #113's.

My own pass, before the reviewers: each claim made in two places agrees,
and the `path:line` citations exist (cert.ts:31-36, Dockerfile:41-42,
SPEC-009:258, threat model :200 and :316-332, blueprint :722, :740, :752,
roadmap :452-461), read again after each round. Caddy's documented
`reverse_proxy` defaults were checked: it passes `Host`, sets
`X-Forwarded-For`, `-Proto` and `-Host`, and ignores a client's values;
deploy_ac_008 asserts it.

### #115, on an owner-review branch

`adr/s43-115-runner-privilege`, pushed, no pull request:

- `d796bab`, NOT YET RATIFIED: ADR-0010 Amendment 2026-10-19 (the CI
  privilege assumption validated by the agent's own real-ETW and
  Security-log tests in `rust-ci`'s Windows job; AC-001 out of CI on the
  hosted Windows runner for want of a container runtime alone, as measured
  on 2026-05-27; *Validation point* and the deferral after the fallbacks
  superseded) and ADR-0008 Amendment 2026-10-19 (the Phase 0 CI gate's
  question answered the same way). The catalog rows name both.
- `32c8693`, NOT YET RATIFIED: SPEC-005 Amendment 2026-10-19 (AC-001 is
  not the CI privilege validation; its elapsed time is printed in the
  elevated gate's output; AC-001 is a vitest test, amending by scope the
  SPEC's "one Rust integration test per AC"). Claude Code's, but it rests
  on ADR-0010's and lands with it.

Compare URL:
<https://github.com/AlmanInDaHouse/CyberGuard_Enterprise/compare/main...adr/s43-115-runner-privilege>.

Reviews, the fixed SPEC prompt, five passes. List A, all fixed: the date
AC-001 left CI (2026-05-27, `72432e4`, not 2026-05-29); the catalog naming
SPEC-017 as the record that made the amendment (now the debt); the
References' line on SPEC-005, AC-001's "visible in the CI run output" and
the SPEC catalog's SPEC-017 line, each left saying the opposite; AC-001
said to run only elevated, while an unelevated Windows run executes it too;
SPEC-005's "one Rust test per AC". The fifth pass's three fixes were not
re-reviewed: each is a clause the pass itself gave. To do at landing: the
roadmap's `docs/adr/0010-agent-privilege-model-mvp.md:120` anchor moves to
`:132`, and SPEC-006:171 keeps "runneradmin ETW privilege untested in CI"
as an analogy, a live sentence #115's landing corrects.

### #117, classified

Debt #117's destination is a pass that classifies each sentence #13's sweep
left, a record (Class H-inline) or a live claim (Class B):

- **Records, not edited:** ADR-0014:14, ADR-0015:13 and :16, ADR-0016:22
  and :24 sit in a *Context* that opens "A read-only audit (this session)
  established…": what an audit verified at its session. SPEC-011:12 is the
  *Context* of the change it made. SPEC-008:54 gives the reason `0001` was
  free when the api's migrations began. SPEC-007:160 and SPEC-014:45 state
  what the code was when the SPEC was written ("today it returns only a
  boolean"; "today returns `Promise<void>`") and what the code gate was to
  do: what the author verified then, which CLAUDE.md's *Class H-inline*
  names ("record what was verified … at a specific moment").
- **Superseded by amendments, not edited:** SPEC-010:119 and :121 are Open
  questions its Amendments 2026-10-15 and 2026-10-14 resolve; the SPEC
  amendment workflow keeps the original text.
- **So no ADR amendment follows from #117.** Manuel's "Redáctalas" is
  answered by #115's amendments.

A first version took SPEC-007:160 and SPEC-014:45 as live and corrected
them (a local commit, `a15e7f6`, never pushed). Its fixed SPEC prompt's
reviewer found an anchor the edit broke (`alerts.ts:75` for `:74`), a Risks
row (SPEC-007:208) pointing to the flag the edit removed, and the other
"today" sentences of SPEC-007 (:25, :26, :50, :62, :111, :183, :205), the
same kind and uncorrected. Correcting two of a family was arbitrary; the
commit was dropped and the family classified as records.

### #110, the remaining half, landed

PR #38, two commits, each pushed alone with CI green, then landed on
`main` by fast-forward pushes of the same SHAs, each with its push runs
green; GitHub closed PR #38 as merged and the branch was deleted.

1. `e7a23c8`: `markdownlint-cli2-action` v23 → v24, whose engine is
   markdownlint-cli2 0.23.2 (markdownlint 0.41.1); the root devDependency
   follows it, and CLAUDE.md's local lint runs that devDependency. The
   repository's Markdown lints clean under it. A major step on its own,
   as the runbook's step 4 asks.
2. `0e26dd7`: a third `dependency-audit` job, `pnpm audit
   --audit-level=critical` over every dependency, and the local gate's
   mirror of it; three overrides of children markdownlint-cli2 pins
   exactly, each the smallest fixed release within its major and at least
   two weeks old (js-yaml 5.4.1, markdown-it 14.3.1, smol-toml 1.9.0); the
   runbook's account of what is left; the threat model's record of the
   system as built and R12.

`pnpm audit` over every dependency: from 12 advisories (6 high) to 3:
braces (high, no fixed release, through the micromatch of
markdownlint-cli2 and of tailwindcss 3), katex (low, loaded by markdownlint
and never rendering; its fix is outside micromark's range) and tailwindcss
3's postcss-selector-parser (moderate). The production audit: no known
vulnerabilities.

Reviews, the fixed code prompt, two passes:

- the first, on the one-commit version (`e52a411`): the overrides clear
  `pnpm audit` but not the linter CI's action or a fresh `npx` runs, which
  bundle their own children (taken: the local lint runs the overridden
  devDependency; the runbook says CI's copies are unpatched and parse only
  this repository's files); "two jobs" in the runbook (taken); the threat
  model's two sentences on the development dependencies (taken); braces
  also through tailwindcss 3 (taken); katex's reason (taken); the action
  and the engine bumped with the overrides in one commit, against the
  runbook's step 4 (taken: split); markdown-it 14.3.2 where 14.3.1 is the
  smallest fix (taken); Node ≥ 22 for markdownlint-cli2 0.23 (recorded in
  the commit);
- the second, on `e7a23c8..0e26dd7`: no list-A point. Recorded: `@v24` is a
  moving tag (debt #113); braces and katex have no reachable fix (the
  register's row says so); CLAUDE.md:37's "the engine of CI's" holds for
  the version, not for the three overridden children (corrected in this
  close's CLAUDE.md line).

Matrix, against #110's destination: markdownlint-cli2 with CI's action →
`package.json:8` 0.23.2 = the action v24.2.0's `package.json`, and CI's
log printing `markdownlint-cli2 v0.23.2`, covered (a check, not a test);
tailwindcss's next major → not attempted, the debt keeps it; an audit of
the development dependencies in CI → `dependency-audit.yml`'s third job,
green on the PR and on `main`, covered. Local gate: `pnpm install`,
`pnpm audit --prod --audit-level=high` ("No known vulnerabilities found"),
`pnpm audit --audit-level=critical` (exit 0), markdownlint over the
repository's 131 files (0 issues), on the final lock, in the session's
docs worktree.

### #111, rustls-pemfile out of the agent: PR #37, waiting on gate 1

`16d5c33` on `fix/s43-debt-111-pemfile`, one commit on `main` `0e26dd7`,
CI green (`markdown-lint`, `dependency-audit`, `ts-ci`, `rust-ci`, the
Windows job's ignored tests included). The agent reads its trust anchor and
its client chain with `rustls::pki_types::pem` (`CertificateDer::
pem_slice_iter`), no crate added; rustls-pemfile leaves `Cargo.toml` and
`Cargo.lock`, and `cargo audit` prints no warning. `pem_error_text` words
the marker and base64 errors as rustls-pemfile did. The dependency runbook
and the threat model's R12 say so.

- The tests: three unit tests in `agent/cg-agent/src/tls.rs`, written
  first and passing against rustls-pemfile, then against the new reader;
  the exact text of a missing section end fails against the pki-types
  `Display` (a mutation run).
- Reviews, the fixed code prompt, two passes. The first: no defect;
  rustls-pemfile 2.2.0 was already a wrapper over the same pki-types
  reader, so accept and refuse are unchanged; the error text printed
  markers as bytes (taken: `pem_error_text`); R12 false once the crate
  was gone (taken). The second: no list-A point; `pem_error_text`'s comment
  claimed more than it does (taken); the first commit was not true at its
  SHA, R12 still naming the crate (taken: the branch squashed to one
  commit); #87 does not travel with this gate (its row re-routed).
  Recorded, not taken: an `IllegalSectionStart` test; R12's clause on
  `cargo audit`'s warnings has no debt once #111 closes (the runbook makes
  a new warning a debt; nothing enforces it).
- Matrix, against #111's destination: rustls-pemfile replaced →
  `Cargo.lock`, `Cargo.toml` and `cargo audit`'s run, covered;
  behaviour kept → `tls.rs` tests and `mtls_ac_001`–`009`, covered (a
  multi-root anchor through `build_client_config` weak); under the
  elevated gate → not run at the close (below).
- Local gate on the first version (`640a5d3`): `cargo fmt --check`,
  `cargo clippy --all-targets --all-features -D warnings`, `cargo test
  --all` (66 binaries, 129 passed, 7 ignored), `cargo audit` (no warning).
  The squash changed a doc comment and documents only.

### Gate 1

`C:\tmp\s43-gate1.ps1`, on `16d5c33`. It was written for `ab228e8` and
moved to `16d5c33` when the branch was rebased and squashed; the agent's
code is the same but for a doc comment. Manuel had not run it at the
close. The script switches the clean main tree to
`fix/s43-debt-111-pemfile` itself, and logs the commit it ran on (it must
be `16d5c33`). PR #37 lands after it, by fast-forward behind
`git diff --quiet 16d5c33 HEAD`, if `main` has not moved; if it has, a
rebase, CI again, and the gate holds as long as the commits under it touch
no runtime input (CLAUDE.md, *Developer-local SPEC-006 marquee
validation*).

## Decisions

Manuel's, in chat: the three answers of the session's opening, above.

### Claude Code's decisions, delegated or within the ADRs

Anticipated:

- **§F as four SPECs, in the order F1 to F4,** and the scope of each
  (ADR-0022 *Out of scope*, SPEC-023 *Out of scope*). Delegated: what
  enters each later SPEC is product scope. The alternative, one SPEC for
  the whole phase, was discarded: the agent's service mode and the
  dashboard's pages are their own contracts.
- **The deployment-contract points Manuel delegated on 2026-10-14**
  (roadmap Part (b) §2–§4), decided for an enterprise buyer and recorded in
  ADR-0022 and its amendments: the compose in `deploy/docker/` (against the
  root; against `infra/prod/`), the forensic key's fingerprint recorded out
  of band with a verifier (against a public endpoint; against an HSM or a
  timestamping service, which cost money), secrets generated into the
  environment file (against Docker secrets files), a shipped Caddy proxy
  (against nginx; against the operator's own), internal networks with the
  dashboard–api leg in clear (against a second internal CA), a scoped
  credential per service, retention 365 days per class by default.
- **Debt destinations moved, proposed in ADR-0022 and SPEC-023, each
  delegated and listed for Manuel's audit:** #87 and R1 to F2; #98 and
  #102 to F3; #24 out of §F, to the SPEC that adds a second org.
- **#117 classified as records**, not corrected (above).
- **#110:** the overrides as exact pins of the smallest fixed releases;
  the critical-level audit of every dependency, not `high` (braces, high,
  has no fix and would hold `main` red).
- **#111:** the error text kept as rustls-pemfile wrote it.

Reactive:

- **#117's first version corrected two "today" sentences**; the reviewer
  showed a family of them, and the commit was dropped (above).
- **§F's documents changed through six review passes**, each finding the
  neighbours of what the last one fixed; the largest changes: the
  dashboard's pages given to F3, retention 365 days with an ADR-0003
  amendment, an ADR-0004 amendment, a product proxy image, the first
  release after F2, backups encrypted by `backup.sh` (against leaving them
  in clear for the operator's storage), two Postgres roles per service (an owner for the migrations, in a one-shot container, and a runtime role that owns nothing), against one role per service owning its tables.
- **#110's one commit split in two** after its review, and its overrides
  corrected to the smallest fixes.
- **#111's branch rebased onto `main` after #110 landed** (a conflict on
  R12's line, both clauses kept), then squashed to one commit after its
  second review; the agent's code is the same as at `ab228e8` but for a
  doc comment.
- **A docs worktree** (`C:\Users\manul\cg-s43-docs`) held the document
  work while the main tree stayed on #111's branch for the gate; nothing
  was compiled there.

## The threat model, for Manuel's audit

This session changed it as follows:

- R12 and the record of the system as built: the development dependencies
  fail CI only on a critical advisory and carry three lesser ones (#110,
  `0e26dd7`, on `main`); `cargo audit` prints no warning since
  rustls-pemfile left the agent (#111, PR #37, not landed).
- Proposed, with ADR-0022 (not landed): R2, R3, R8, R10, R15 and R16 close,
  R13 narrows to the host, R4, R7, R11 and R12 narrow, once SPEC-023's code
  lands; R1's destination moves with #87 to F2.

The earlier sessions' changes listed for his audit (handoff-41's #93, #82
and #31; handoff-42's) still wait.

## Local environment operations

- **Docker Desktop started** (it was stopped):
  `"/c/Program Files/Docker/Docker/Docker Desktop.exe"`. Verified: `docker
  info` answered, server 29.1.2. Not restarted.
- **The NATS and MinIO volumes removed**, as Manuel chose, after checking
  that no container used them:
  `docker volume rm cyberguard-dev_cg_nats_data cyberguard-dev_cg_minio_data`.
  Verified: `docker volume ls` no longer lists them. Three volumes outside
  the dev stack's project (`cyberguard_backup_data`, `cyberguard_pgdata`,
  `cyberguard_postgres_data`) were left as they are.
- **Manuel's `infra/dev/.env`**: the NATS block (`CG_NATS_CLIENT_PORT`,
  `CG_NATS_MONITOR_PORT`) and the MinIO block (`CG_MINIO_API_PORT`,
  `CG_MINIO_CONSOLE_PORT`, `CG_MINIO_ROOT_USER`, `CG_MINIO_ROOT_PASSWORD`),
  with their comments, removed; a copy of the file before the edit was kept
  in the session's scratchpad. The file is not tracked.
- **`cargo clean --profile dev`**, when `C:` fell to 15 GB free: "Removed
  8923 files, 30.2GiB total"; 45 GB free after.
- **A git worktree**, `C:\Users\manul\cg-s43-docs`, for document work;
  removed at the close.

## Baselines

At the close's `main`:

- Rust: `cargo test --all` 66 binaries, 126 passed, 7 ignored (handoff-42's;
  `main` gained no agent change). On PR #37: 129 passed (three new).
- `services/api`, `services/ingest` and `dashboard`: as handoff-42 states;
  `main` gained no service change (CI's `ts-ci` green on `0e26dd7`).
- `cargo audit` on `main`: one allowed warning (rustls-pemfile, until PR
  #37 lands). `pnpm audit --prod --audit-level=high`: no known
  vulnerabilities. `pnpm audit` over every dependency: 3 (1 high, 1
  moderate, 1 low).

Catalogs on `main`: ADR 21, SPEC 21 (ADR-0022 and SPEC-023 wait on the
owner-review branch). Known CI debt: none.

## Debts

Closed:

- **#117**, by its classification, with no commit (above).

Narrowed:

- **#110** (`e7a23c8`, `0e26dd7`): to braces (high, no fixed release;
  through markdownlint-cli2's and tailwindcss 3's micromatch), katex (low,
  its fix outside micromark's range) and tailwindcss 3's
  postcss-selector-parser (moderate, fixed in its next major).

Re-routed:

- **#87**: its destination was the next agent change under the elevated
  gate, which #111's is; it does not travel with it, since the key's owner
  depends on the account F2's service mode runs as. Destination: F2,
  proposed in ADR-0022.

Pending a landing: #111 (PR #37, after gate 1), #115 (after Manuel's "sí"
to `adr/s43-115-runner-privilege`), and, with ADR-0022 and SPEC-023, the
debts SPEC-023 implements once its code lands.

New:

- **#118 — The SPEC-005 marquee's budget log differs from SPEC-005.** It
  logs `within_budget` where SPEC-005:239 asks a `verdict` field, and its
  clock starts before the agent's enrollment, not at the probe's spawn
  that NFR-005-004 names (`services/ingest/test/spec-005-marquee.test.ts`).
  Surfaced by the fifth review of #115's branch. Destination: the next
  change to the marquee.
- **#119 — The enrollment contract's texts differ from the code in places
  SPEC-023 does not touch.** ADR-0004 §Enrollment's signed JWT, `SAN =
  org_id`, the revocation list and the rotations (none built); SPEC-004
  §Observability's "CA generated" log and the CLI's structured "Token
  issued" event (neither emitted); SPEC-002's stderr wording, its
  `expires_at` log at identity load and its pointer to `services/api` for
  the production endpoint; SPEC-003 §Observability's handshake logs.
  Surfaced by the session's TLS inventory. Destination: F4, the renewal
  SPEC, which rewrites the enrollment contract.

**Open at the close: 64.** Session 42 closed with 63; this session closed
one, #117, and opened #118 and #119.

## What waits on Manuel

- **Gate 1** (`powershell -NoProfile -ExecutionPolicy Bypass -File
  C:\tmp\s43-gate1.ps1`), on `16d5c33`, the main tree on that branch: then
  PR #37 lands and #111 closes.
- **His "sí" to ADR-0022 and its five co-located amendments**
  (`adr/s43-f-deployment`), and with it SPEC-023 lands; then F1's code
  starts.
- **His "sí" to ADR-0010's and ADR-0008's amendments**
  (`adr/s43-115-runner-privilege`); SPEC-005's lands with them, #115
  closes.
- **ADR-0022 Open question 1**, code-signing the agent (money), and
  **Open question 2**, the Redis image's licence (RSALv2/SSPLv1 since 7.4;
  Valkey would replace it), a data-store choice.
- **#31's measurement**, which he answered "todavía no" this session.
- **The threat model**, this session's changes above, and the earlier ones.
- **Debt #7**, the shared package.

## State at the close

`main` = `origin/main` = the close commit, the tree clean, no stash. The
branches besides `main`, each with its reason:

- `fix/s43-debt-111-pemfile`: PR #37, open, CI green on `16d5c33`; it
  waits on gate 1.
- `adr/s43-115-runner-privilege` and `adr/s43-f-deployment`: owner-review
  branches, no pull request; they wait on Manuel's "sí".

The main tree is on `main`; the docs worktree was removed. CI on the close
SHA: ALL GREEN (*How Session 44 resumes* asks for the check again).

## How Session 44 resumes

1. *At the start* (CLAUDE.md *Session protocol*): this record, `main`, CI,
   catalogs ADR 21 / SPEC 21 (or 22 / 22 once ADR-0022 and SPEC-023
   land), Known CI debt none, `docs/debts.md` open 64.
2. Ask Manuel for what waits on him above that the work needs.
3. If gate 1 is green: land PR #37 (fast-forward behind the guard), close
   #111.
4. With his "sí": land `adr/s43-115-runner-privilege` (the marker
   stripped, a ratification line, #115 closed, the roadmap's ADR-0010
   anchor and SPEC-006:171 corrected), and `adr/s43-f-deployment`
   (ADR-0022 `Accepted`, SPEC-023 `Accepted`, the register's re-routed
   destinations), then start F1's code on a branch, in the order SPEC-023
   allows: the services' configuration checks and commands, the images,
   the bundle and its tests, the release workflow; the elevated gate for
   the retention change and the enrollment change.
5. Without it: the sweep, #118 and the debts without an owner decision.
