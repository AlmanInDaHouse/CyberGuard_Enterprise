# Handoff — Session 45

Session 45 opened on 2026-10-23 and closed on 2026-10-24. Manuel pasted the
prompt Session 44 left. Two of its answer slots arrived unfilled, and he
answered them when asked (below). The prompt's order was F1's code
(SPEC-023) on branches, one pull request per change:

1. the services' configuration checks and the commands;
2. the Postgres schemas and roles, the retention's time-to-live, and
   enrollment over TLS with #118, under one elevated gate;
3. then the dashboard, the relay's CA (#78), the images, the bundle with
   `deploy-ci.yml`, the release and the licence check, `docs/operations/`,
   and the root compose stubs (#12).

\#121 was to ride the dev-stack change. The delegation stood as in Sessions
38 to 44: an ADR still needs Manuel's "sí", and the elevated gate is his to
run.

This session delivered items 1 and 2, the dashboard and the relay's CA.
The images, the bundle, the release, `docs/operations/` and the compose
stubs remain, in that order (*How Session 46 resumes*).

## Start

- `main` = `origin/main` = `19be4c5`, tree clean, no stash, no branch but
  `main`, no pull request, no worktree. CI of `19be4c5`: `markdown-lint →
  success`.
- Catalogs ADR 22 / SPEC 22; Known CI debt none; `docs/debts.md` open 63,
  equal to its rows. All as handoff-44 states.
- `C:` had 58 GB free; `target/` held 518 MB.

Manuel's decisions this session, in chat:

- 2026-10-23, the two unfilled slots, each answered to a question that put
  it to him with its options:
  - his dev stack's Redis volume: "Bórralo tú (Recommended)";
  - the threat model (R1's and R14's destinations and the earlier
    changes): "Todavía no (Recommended)".
- 2026-10-24, after running the elevated gate: "s45-gate1.ps1 terminado".

## Progress

Each change: a branch cut from `main`, a pull request, the acceptance tests
seen failing for the missing behaviour before the code, the local gate in
each workspace it touches, CI on every push, a fresh-context review with
CLAUDE.md's fixed prompt and another pass after the fixes, then landing.

### The start-up checks, `/health` and the session lifetimes (PR #41)

`0cfa4df`, `09b2f87`, `f1301f8`, `df969fa`, `8d7969a`, `fc8d703`, landed by
fast-forward:

- Ingest and the api refuse a malformed variable at start, with
  `cg-<service>: fatal: invalid configuration: <VAR>: <reason>` on one line,
  exit 1. Ingest loads its rules before it opens a listener.
- The api's `GET /health`. The session lifetimes are settings
  (`API_SESSION_IDLE_MINUTES`, `API_SESSION_MAX_HOURS`).
- A lost store no longer ends either process: a connection lost idle or
  under a checked-out client is logged (`pg_connection_lost`,
  `redis_unreachable`) and fails the query in flight (`watchPool`,
  `watchRedis`). The forwarder test that surfaced it cuts a real store.
- Debt #105: a certificate and key path that name one file are refused,
  through links, case and hard links.

Its review asked two bounds of SPEC-023 back: SPEC-023's **Amendment
2026-10-23** (`6dd3156`), the integers of the start-up checks (decimal
digits alone; `INGEST_DETECT_INTERVAL_MS` up to 2147483647), item 2
**delegated** (an operator surface), after four review rounds.

### The commands (PRs #42 and #43)

- api (`7ad2396`, `8bc9c02`, `63d5c08`): `create-user.js` reads the password
  from standard input, refuses `--password` in any form, confirms the TOTP
  factor (three codes) and prints how to confirm it later (debt #77);
  `forensic-key.js`; `verify-export.js`; the export records
  `forensic_pubkey_sha256`; `migrate.js` waits for its advisory lock.
- ingest (`83f65bd`, `116d0be`): `export-ca.js` (the PEM and the SHA-256 of
  its DER), `issue-token.js` on `INGEST_PG_URL` alone, `migrate.js` waiting.

### Postgres schemas and roles (PR #44)

`5d6a564`, `b82247e`, `74cb51c`:

- `deploy/docker/postgres/`: five roles, none a superuser; the schemas
  `ingest` and `api`; the extensions in `public`; `PUBLIC`'s `CREATE` on
  `public`, its `CONNECT` on the databases and the large-object functions
  revoked; no statement's parameters in the log. Passwords reach `psql`
  through `\getenv`, not its command line.
- Each service migrates into its schema and keeps its ledger there; ingest's
  `0009` and the api's `0007` grant each runtime role what its code uses,
  table by table. Every connection carries its search path.
- \#121: the dev compose starts `valkey-server` with no shell, so the
  entrypoint drops to its user.
- `5d6a564` is the branch's first two commits as one: `28dc041` was red on
  its PR run (an unhandled `57P01` from a throwaway database's drop) and
  `4c780cc` fixed the harness, so `main` got no red SHA.

### The retention's time-to-live (PR #45)

`047eaf5`, `56b0bb4`: `INGEST_RETENTION_PROCESS_DAYS`, `_NETWORK_DAYS`,
`_LOGON_DAYS`, `_HEARTBEAT_DAYS` (7 to 3650; 365, 365, 365, 30); the
migration step sets a TTL per class of `cges_events` (another class at the
longest of the three) and on `heartbeats`, from the row's arrival,
materialised only when it changes, under the migrations' advisory lock
(debt #47). The branch was squashed to one commit after its second review:
the code and the threat model had been in two commits, and the first was
false at its own SHA.

### Enrollment over TLS (PR #46)

`63ea972`, `e77a772`, `7432a0f`:

- ingest: `INGEST_SERVER_NAMES` (the self-issued certificate's SANs, its
  CN the first, `O=CyberGuard self-issued`; reissued when the list or its
  first name changes; IPv6 held in hexadecimal form, a zone and a name
  ending in digits refused); a key file it issued set to `0600` at start
  (debt #92's key half); `INGEST_ENROLL_TLS` (default on: the enrollment
  listener and its `/health` over TLS 1.3 with ADR-0004's two suites; the
  mTLS listener held to the same two; the reissue swap and the expiry guard
  on both).
- agent: with `[enrollment]`, `server.url` must be `https://` and the trust
  anchor is required, an empty anchor path refused (exit 2); enrollment
  trusts the anchor's roots alone; an untrusted server certificate ends it
  with exit 6 at once; SPEC-002's FR-013 path is removed.
- \#118: the SPEC-005 marquee measures from the probe's spawn and logs
  `marquee_elapsed_seconds` and `verdict`.
- The dev compose probes ingest's `/health` over HTTPS and the api's
  `/health`.

### The dashboard (PR #47)

`84a4aad`, `3ee6142`, `98216b9`: the browser's cookies `__Host-cgsess` and
`__Host-cgcsrf` (the api's keeps `cgsess`; debt #106); `middleware.ts`'s CSP
with a nonce new per response and `'strict-dynamic'`, `nosniff`,
`Referrer-Policy: same-origin`, `Permissions-Policy`, every page rendered at
request time; `GET /api/health`; `API_BASE_URL` checked in
`instrumentation.ts` at the server's start.

### The relay's CA (PR #48)

`f397129`: `INGEST_SMTP_CA_FILE` (checked at start: its first certificate
block must parse, as Node loads a CA file up to its first bad block); the
send then trusts its certificates instead of Node's roots; a failed
verification fails the send with its code (`UNABLE_TO_VERIFY_LEAF_SIGNATURE`,
`ERR_TLS_CERT_ALTNAME_INVALID`, `UNSPECIFIED`, …) instead of `ESOCKET`, in
SPEC-014's log line and a SOAR step's error (debt #78). The branch's three
commits landed as one, rebased on the dashboard's.

## Gates

`C:\tmp\s45-gate1.ps1` (Manuel, elevated, 2026-10-24 01:08–01:17), on
`e6db6a4`, PR #46's tip, which held #44's and #45's commits:

```text
No hay entradas en la lista.
net session exit: 0
e6db6a40bef5089dc760793f0ee08edbe070e308
status lines: 0
    Finished `release` profile [optimized] target(s) in 1.08s
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
{"event":"spec_005_marquee_complete","marquee_elapsed_seconds":35.287,"budget_seconds":45,"verdict":"pass"}
spec_005 captured image_file_name — launch: C:\Windows\System32\cmd.exe; terminate: cmd.exe
 Test Files  82 passed (82)
      Tests  421 passed | 2 skipped (423)
pnpm test exit: 0
logman exit: 0
```

The release binary (23:48:26) was newer than every agent source; the two
skipped tests are the POSIX-mode ones (`servercert_ac_008`, deploy_ac_014's
`0600`), which run only off Windows; the marquees ran (detect_ac_001 47.4 s,
auth_ac_013 26.0 s, net_ac_010 25.8 s, SPEC-005 40.7 s, ac-001 5.5 s); no
`CGAgent` session was left. The gate covers the code of #44, #45 and #46,
whose commits landed by cherry-pick with each tree checked equal to the
branch commit CI ran on. #47 and #48 touch no path the gate guards.

## Self-reviews

For each change, Claude Code's matrix (criterion → test → covered / weak /
missing), a fresh-context reviewer with the fixed prompt, and another pass
after the fixes. The prompts carried one added line of environment
constraints (no Rust compile, no build or suite in a worktree, no push,
commit or branch change), about the environment, not the change. The full
lists are in the reviewers' reports; their dispositions:

- **PR #41** (four rounds): the alias checks; the process-ending store loss
  (fixed, tested by a forwarder cut); one-line reasons; a `last_seen` idle
  check added and then reverted as an authentication change no contract
  states; the amendment's bounds (`6dd3156`). Recorded: the ClickHouse
  client has no connection to listen to; the store-cut test's timing; the
  shorter idle lifetime applies to each session at its next use.
- **PR #42 / #43** (two rounds each): `--password=` refused; arguments
  before configuration; the report's note on the fingerprint; the command
  pools listened to. Recorded: the TTY path untested (no TTY in CI); the
  confirmation's three attempts are the command's own bound.
- **PR #44** (two rounds): passwords off `psql`'s command line; no large
  objects; no connection to `postgres` or `template1`; ts-ci on the
  initialisation. Recorded: the routes not all run as `cg_api`; #86 and #94
  effective once the bundle's compose sets the roles' URLs.
- **PR #45** (three rounds): the retention required of every caller; the
  values checked before any DDL; the ClickHouse step under the lock; the
  "longest of the three" and "no ALTER on the same values" tests made
  discriminating (each seen failing on a mutation); the squash.
- **PR #46** (two rounds): IPv6 encoding (`@peculiar/x509` wrote
  `::ffff:10.0.0.5` as `::ffff:10`) and zones; mistyped addresses; the CN
  on a reordered list; the empty anchor path; stale comments. Recorded:
  SPEC-023:230's anchor (`tls.rs:83`) drifted (#123); every
  `InvalidCertificate` is terminal, as SPEC-003's exit 6 is.
- **PR #47** (two rounds, the first cut short by an API error and rerun):
  `https:` and the headers on a redirect, a static asset and the 404;
  a credential refusal added and then removed as a narrowing the contract
  does not state. Recorded: browsers holding the old cookie names are
  signed out and their api sessions live out their TTL (the upgrade
  document); the start-up test will need its entry point when the image
  sets `output: "standalone"`.
- **PR #48** (two rounds): `UNSPECIFIED`; the CA file's first block;
  comments. Recorded: the file is read twice; the codes besides the three
  seen end to end are tested on the mapping alone.

## Decisions

Manuel's: above, with his words.

### Claude Code's, delegated or within the ADRs

Anticipated:

- **Each new variable's check lands with the code that reads it**
  (`INGEST_SERVER_NAMES`, the retention, `API_BASE_URL`,
  `INGEST_SMTP_CA_FILE`), against all in PR #41: a check without a reader.
- **The start-up line names the first issue only, and no reason quotes the
  value**; the api's other fatal errors take the same `cg-api: fatal:`
  prefix; pino's `silent` is a level.
- **`create-user.js` exits 0 after three invalid codes** (the user exists;
  a non-zero exit would invite a rerun into a conflict), writes a denied
  audit row per invalid code as the route does, and bounds the attempts
  itself instead of using the route's limiter.
- **`REVOKE ALL ON DATABASE` takes `CONNECT` from `PUBLIC`**, the login
  roles granted it by name; deploy_ac_004 and deploy_ac_005 run in the api
  suite against a Postgres started with the bundle's initialisation, until
  the bundle's own tests exist.
- **D3's server and agent halves in one commit**: an enrollment listener on
  TLS with an agent that cannot speak it is not true at its own SHA.
- **SPEC-023's Amendment 2026-10-23, item 2, delegated**: the detection
  interval bounded by Node's longest timer, against SPEC-023's unbounded
  "≥ 0", an operator surface (Manuel's list), decided for him and listed.
- **#44, #45 and #46 as three stacked pull requests under one elevated
  gate**, against one branch: one change each to review, one gate.
- **Enrollment's TLS on reqwest with a preconfigured rustls config**,
  against a raw tokio-rustls client like the heartbeat's: the retry code
  stays as it was.
- **An unreadable or bad trust anchor at enrollment exits 6**, as on the
  heartbeat path, against 5.
- **The dashboard's headers in middleware** (the CSP needs a nonce per
  request) and every page dynamic, against `next.config` headers.
- **The relay's verification code replaces `ESOCKET` in the mailer's error**,
  against mapping it only where it is reported: one place.

Reactive:

- **D1's first two commits landed as one** (`5d6a564`): the first was red.
- **D2 squashed** after its review: the threat model in its code's commit.
- **The classifier of an untrusted server certificate descends nested
  `io::Error`s**: reqwest's connector wraps tokio-rustls's.
- **IPv6 names canonicalised** before issuance: a library encodes the
  dotted form wrongly.
- **A CA file's first block must parse**, against "any block" (Node would
  load nothing) and "every block" (a narrowing past the contract).
- **The dashboard's credential refusal removed**: not in the contract.
- **#48 squashed on the dashboard's tip**: one CI cycle to land.

### Deviations from the rules, recorded

- `5ad7257` (#48) was pushed after typecheck, lint and the four affected
  suites, not the full ingest gate; the full gate ran right after on the
  same tree (424 passed, the four marquees at exit 9).
- PR #41's two fix commits were pushed, after a rebase, without a full gate
  on their own tip; their code had passed in the gates of the stacked PRs,
  and a full gate ran on the branch's tip before landing.
- PR #41 landed with the landing script polling the pull-request runs of
  each SHA, already complete, instead of the push runs; the push runs were
  read after the six pushes, all `success`, and the script was fixed to
  poll the push event before the next landing.

## The threat model, for Manuel's audit

Changed with the code, as SPEC-023 §Security considerations asks:

- §1 Agent: `image_file_name` has no column time-to-live; the token travels
  over TLS 1.3, the agent trusting its anchor alone.
- System as built, `cg-ingest`: the enrollment listener over TLS.
- §5 cg-soar: what the send trusts for the relay, and the recorded code.
- §8 Storage: the retention per class and on heartbeats.
- §9 Dashboard: the `__Host-` cookies, the CSP with a nonce and its reading
  of "inline scripts … forbidden", no key pinning, HSTS the proxy's.
- Cross-cutting: the key file set to `0600` at start.
- R4: the wire half done; the rest of #89 to the renewal SPEC (F4).
- R11: the connections' and logons' retention.
- R15: the containers as root; the key file's mode fixed.

The earlier sessions' changes listed for his audit still wait.

## Local environment operations

- His dev stack's Redis volume, as he answered: `task dev:down`, `docker
  volume rm cyberguard-dev_cg_redis_data`, `task dev:up`; the stores came up
  healthy on `valkey/valkey:9.1.2-alpine`. `dev:up` also started the api and
  ingest, and ingest failed to bind host port 8080, which his
  `jenkins-supplychain` container holds; `docker compose rm -s -f api
  ingest` left the stack as it ran before, the three stores.
- The reviewers ran in isolated worktrees; each was removed after checking
  it held no change and no junction.

## Baselines

At the close's `main`:

- Rust: `cargo test --all` 66 binaries, 139 passed, 7 ignored.
- `services/api`: 55 files, 377 tests passed.
- `services/ingest`, unelevated: 83 files, 78 passed, 4 failed, 1 skipped;
  424 tests passed, 4 failed (the capture marquees, the agent's exit code
  9), 3 skipped. Elevated (gate 1, before the relay's CA): 82 files, 421
  passed, 2 skipped.
- `dashboard`: 12 files, 141 tests passed.
- `infra/dev/check-compose.test.mjs`: 9 of 9.

Catalogs: ADR 22, SPEC 22. Known CI debt: none.

## Debts

Closed:

- **#17** and **#20** (`0cfa4df`): the configuration surfaces SPEC-023
  consolidates, checked at start; the api's `/health`.
- **#105** (`0cfa4df`, `09b2f87`): one file for both paths refused.
- **#77** (`7ad2396`): the bootstrap command reads the password from
  standard input.
- **#121** (`5d6a564`): the dev store's entrypoint drops to its user.
- **#47** (`047eaf5`): the retention of 1007 and 4001.
- **#118** (`63ea972`): the marquee's budget log.
- **#106** (`84a4aad`): the `__Host-` cookies.
- **#78** (`f397129`): the relay's private CA and its verification code.

Narrowed, destination changed:

- **#89**: the TLS half done (`63ea972`); the rest to F4's renewal SPEC.
- **#92**: the key file's mode done (`63ea972`); the containers as root to
  F1's images.
- **#104**: the dashboard's headers done (`84a4aad`); HSTS to the bundle's
  proxy.
- **#95**: the retention done (`047eaf5`); the ClickHouse grants to the
  bundle, and the DPO's record.
- **#86**, **#94**: the roles and the logging settings exist (`5d6a564`);
  they take effect when the bundle's compose connects as the roles.

New:

- **#122 — The migrate one-shot waits for its advisory lock without
  bound, and any role can take another service's key.** A holder that
  hangs blocks `docker compose up --wait`. Surfaced by PR #43's and #44's
  reviews. Destination: F1's bundle (a timeout on the one-shots), or a
  SPEC-023 amendment.
- **#123 — SPEC-023 §Operational §6 cites `agent/cg-agent/src/tls.rs:83`**,
  now `build_client_config`'s line; the call is at `:106`, enrollment's at
  `:73`. Surfaced by PR #46's second review. Destination: the next SPEC-023
  amendment.

**Open at the close: 56.** Session 44 closed with 63; this session closed
nine and opened two.

## What waits on Manuel

- **The threat model**, this session's changes above and the earlier ones.
- **His dev database**, to recreate once (the schemas `ingest` and `api`,
  `infra/dev/README.md`); its next migration deletes heartbeats older than
  30 days and events older than 365.
- **Any agent of his** with an `http://` `server.url` beside `[enrollment]`
  stops with exit 2: `https://`, and the CA `export-ca.js` prints as its
  trust anchor.
- **Host port 8080**, held by `jenkins-supplychain`, which the dev stack's
  ingest needs.
- **ADR-0022 Open question 1**, the Authenticode certificate, before the
  first public release; **#31's measurement** and **debt #7**, as before.

## State at the close

`main` = `origin/main` = the close commit, tree clean, no stash, no branch
but `main`, no worktree but the main one, no open pull request. CI on the
close SHA: ALL GREEN (*How Session 46 resumes* asks for the check again).

## How Session 46 resumes

1. *At the start* (CLAUDE.md *Session protocol*): this record, `main`, CI,
   catalogs ADR 22 / SPEC 22, Known CI debt none, `docs/debts.md` open 56;
   `C:`'s free space.
2. F1's remaining code (SPEC-023), on branches, one pull request per change,
   in this order:
   - the images: the dashboard's (`output: "standalone"`; its start-up test
     then runs the standalone server), all four non-root with a health
     check, the rules and playbooks read-only (#92's containers, #109);
   - the bundle and its tests with `deploy-ci.yml`: the production compose,
     `setup.sh`, the proxy with HSTS (#103, #104's half), the key-value
     store's ACL, the ClickHouse users (#108), backup and restore, the
     one-shots' lock wait (#122);
   - the release workflow and the licence check (#112);
   - `docs/operations/`, the upgrade document saying: an agent's
     `server.url` must be `https://` with the CA as its anchor; browsers
     sign in again after the cookie rename; the first migration deletes
     what the retention no longer keeps; a database a release before
     SPEC-023 created is recreated;
   - the root compose stubs (#12).
3. The elevated gate for any change to the capture path, the detection path
   or the schema of `alerts` or `cges_events`.
