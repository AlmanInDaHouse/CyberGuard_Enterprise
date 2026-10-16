# Handoff — Session 41

Session 41 opened on 2026-10-16. Manuel set its order: PR #23 (debt #93)
first, with its review and elevated gate 3; then the sweep in handoff-40's
order; then roadmap §F's SPEC when the sweep allowed. He delegated again
every decision that waits on him, an ADR text and the elevated gate
excepted.

This record was started early because the register's rows for the debts
this session opens point to it. Later commits of the session complete it.

## Start

- `main` = `origin/main` = `328416b`, tree clean, no stash. One branch and
  one PR open, as handoff-40 names them: `fix/s40-debt-93-dependencies`,
  PR #23, at `4fbce0c`. CI of `main`'s tip: `markdown-lint → success`, the
  only workflow triggered. CI of the PR's tip: `rust-ci`, `ts-ci`,
  `markdown-lint` and `dependency-audit → success`.
- Catalogs ADR 21 / SPEC 21; Known CI debt zero; `docs/debts.md` open 69.
- `logman query -ets` listed no `CGAgent` session. Docker Desktop was
  stopped; Claude Code started it, an operation CLAUDE.md allows. `C:` had
  46 GB free.

Manuel's decisions this session, in chat:

- 2026-10-16, opening the session, the order: PR #23 first, its review with
  the fixed prompt and its fixes, then elevated gate 3 (`C:\tmp\s40-gate3.ps1`),
  which he runs, then the landing commit by commit, #93 closed and
  `dependency-audit` recorded in CLAUDE.md's *Environment facts*; then the
  sweep (#96, #82, #31 with the elevated gate, #13 with #84, then #30, #39,
  #40, #27, #8, #9, #11 and #110); then roadmap §F's SPEC (criterion 7).
- The same message, the delegation, as in Sessions 38 to 40: "lo que
  dependa de mí, decídelo tú en beneficio del proyecto, para un comprador
  empresarial, registrando cada decisión como delegada. Un ADR sigue
  necesitando mi "sí" explícito". The rest of that line reached the session
  cut off. The slot the prompt kept for his reading of ADR-0014:98 (debt
  #80's `401` half) held the template's placeholder, not an answer, so that
  half stays open, waiting for him.
- 2026-10-16, after gate 3: "termino lee logs".

## Progress

### #93: PR #23, its review

One pass of the fixed code prompt on `93127fc..4fbce0c`. Contract: debt #93
(`docs/handoff-session-39.md:714-716`) and the audit statement
(`docs/handoff-session-40.md:957-1018`).

Claude Code's own pass came first:

| Claim of the contract | Evidence | Verdict |
|---|---|---|
| No `pnpm audit` step in CI | `.github/workflows/dependency-audit.yml:45-64`, job `pnpm-audit`; green at the tip | covered |
| No `cargo audit` step in CI | `dependency-audit.yml:66-82`, job `cargo-audit`; red at `c91de2d`, green at `4fbce0c` | covered |
| The dependency runbook does not exist | nothing in the diff | missing |
| `pnpm audit --prod` reports none | a local run at the tip, every severity: "No known vulnerabilities found" | covered |
| nodemailer 10.1.1 keeps the three fields the send reads | `services/ingest/test/soar-ac-006-bounds.test.ts:267` (the version pin), `:276`, `:297`, `:327`, `:343`, `:359` (TLS), `:401` (HELO), `:413` (mechanisms) | covered |
| The crates move within their ranges | `Cargo.toml` unchanged; `Cargo.lock` also gains `rand_pcg` and reselects `windows-sys` | covered; more moved than stated |
| "Development dependencies are not audited" | true of pnpm only: `cargo audit` reads the whole lock | weak (the text) |

Then the reviewer, with the fixed prompt. Its list 1: C1 and C2 (the two audit steps) covered; C3 (the runbook) missing; C4 (`pnpm audit --prod` reports none) covered for today, weak as a standing gate for moderate advisories; C5 (the exact pin) covered; C6 (the three fields) weak; C7 (the crates within their ranges) covered, with more moved than stated; C8 (the policy) covered for pnpm, contradicted for cargo. Its list 2: the cargo job's policy, pnpm's moderate advisories, the landing order, the threat model's two sentences, the gate script's header. Its list 3: what moved beyond the contract, and the security-relevant changes (rustls, rustls-webpki).

What was done with each point, Claude Code's and the reviewer's:

- **The runbook** (missing). Written: `docs/security/dependency-review.md`,
  in the docs commit that closes #93.
- **nodemailer** (weak). 10.1.1 reads the AUTH line's mechanisms as whole
  tokens, where 8.0.10 matched a prefix, and two comments still described
  the prefix match. Fixed in the nodemailer commit. The `PLAIN-CLIENTTOKEN`
  case stays, as a guard against a release that reads prefixes again; the
  `XOAUTH2` case still reaches the send's own reading of the AUTH line.
- **The cargo job** (contradicted). It fails on any vulnerability advisory,
  whatever its severity, across the whole `Cargo.lock`, while the workflow's
  comment and its README row said "high or critical" and "development
  dependencies are not audited". Fixed in the text; the job stays as strict
  as it is.
- **pnpm's moderate advisories** are counted in the summary line, not
  listed. Fixed in the text.
- **The landing order.** `c91de2d` was red at its own SHA, and cherry-picked
  in order it would have turned `main` red. Fixed: the agent's crates now
  come before the workflow.
- **The threat model's `:311` and `:327`.** Fixed in the docs commit that
  closes #93 (below).
- **The gate script's header** named h2 and quinn-proto, which the agent
  does not compile. Fixed: rustls and rustls-webpki, and the `windows-sys`
  reselection.
- **Recorded, from list 3:**
  - rustls-webpki 0.103.15, certificate path validation in the agent, and
    rustls 0.23.46's stricter handshake;
  - `rand_pcg` 0.10.2, a new crate in the lock, never compiled;
  - `windows-sys` reselected among the versions the lock held; socket2, the
    anstyle crates and nu-ansi-term are in the agent's Windows build;
  - the overrides beyond postcss and browserslist (nanoid, source-map-js,
    baseline-browser-mapping), each needed by the base audit, and the
    `browserslist` override being global;
  - fastify's fast-json-stringify 7 and set-cookie-parser 3, and next's
    sharp 0.35.5, whose install script `onlyBuiltDependencies` blocks, as
    before.
- **Recorded, minor:** `@types/nodemailer` 8 against nodemailer 10.1.1, which
  ships its own types the send does not use; SPEC-022:398 names nodemailer
  8.0.10, a citation for #13's sweep.
- **cargo audit's three warnings**, which pass:
  - anyhow 1.0.102, unsound (RUSTSEC-2026-0190, `Error::downcast_mut`,
    patched in 1.0.103; the agent does not call it): moved to 1.0.104 in the
    agent commit, before the gate;
  - rustls-pemfile 2.2.0, unmaintained (RUSTSEC-2025-0134), a direct
    dependency the agent reads its certificates with
    (`agent/cg-agent/src/tls.rs`), and chacha20 0.10.0, yanked, in the
    development dependencies: debt #111.

The reviewer ran no `git checkout`; the branch and the status were checked
after it.

### #93: the rebuild and the landing

The branch was rebuilt from `aab07dd`: the nodemailer commit with its
comments fixed, the overrides, the agent's crates with anyhow, then the
workflow with its text fixed. It was rebased onto `328416b` (docs only) and
pushed one commit per push, the first with `--force-with-lease`:

```text
f876753  next 15.5.28                        ts-ci -> success
fcbf6ce  fastify 5.12.7, fast-uri 3.1.8      ts-ci -> success
833444e  kysely 0.28.17                      ts-ci -> success
fb74bca  nodemailer 10.1.1                   ts-ci -> success
acbdae2  pnpm.overrides                      ts-ci -> success
028ae86  rustls 0.23.46, h2 0.4.20,
         quinn-proto 0.11.19, anyhow 1.0.104 ts-ci, rust-ci -> success
6d1f393  dependency-audit.yml                rust-ci, markdown-lint,
                                             dependency-audit, ts-ci -> success
```

Local gates at the tip:

- Rust: `cargo fmt --check` OK, `cargo clippy -D warnings` OK, `cargo test
  --all` 66 binaries, 126 passed, 0 failed, 7 ignored (`-j 1`, the host's
  memory being short).
- `services/ingest`: typecheck, lint, build and the entrypoint check OK;
  vitest 74 files, of which 69 passed, 4 failed (the capture marquees, the
  agent's exit code 9, unelevated) and 1 skipped; 290 tests passed, 2
  skipped.
- `services/api` and `dashboard`: their files are the ones S40's gates ran
  on at `1105c07`, `aab07dd` and `5a4209c`; the rebase onto `328416b` changed
  none of them.

Elevated gate 3, run by Manuel on `6d1f393` (`C:\tmp\s40-gate3.ps1`,
2026-10-16, 13:45 to 13:51). Its logs, as pnpm and cargo printed them
(see B14 under *#93: the close*):

```text
01-elev    net session: No hay entradas en la lista.
02-head    6d1f393 / fix/s40-debt-93-dependencies
03-status  status lines: 0
05-build   Finished `release` profile [optimized] target(s) in 1m 38s
06-cargo   test auth_ac_012_logons_on_the_real_log ... ok
           test capture_ac_006_elevated_shutdown_leaves_no_session ... ok
           test net_ac_008_a_real_connection_is_reported_with_its_probe ... ok
           test net_ac_009_an_accepted_connection_and_the_filter ... ok
           test ac_004_cache_hit_terminate_matches_launch_byte_for_byte ... ok
           test ac_007_parent_pid_only_when_parent_dead_at_child_launch ... ok
           test ac_009_events_lost_under_deliberate_etw_buffer_pressure ... ok
08-vitest  ✓ test/servercert-ac-renewal.test.ts (19 tests | 1 skipped) 7798ms
           ✓ test/soar-ac-006-bounds.test.ts (23 tests) 22759ms
           ✓ test/detect-ac-001-marquee.test.ts (1 test) 46811ms
           ✓ test/spec-005-marquee.test.ts (1 test) 40467ms
           ✓ test/net-ac-010-marquee.test.ts (1 test) 25554ms
           ✓ test/auth-ac-013-marquee.test.ts (1 test) 25494ms
           ✓ test/rules-ac-007-loaded-at-start.test.ts (3 tests) 10513ms
           ✓ test/ac-001-marquee.test.ts (1 test) 4449ms
           detect_ac_001 captured image_file_name — probe: C:\Users\manul\AppData\Local\Temp\cg-detect-probe-tVvu4b\winword.exe; child: C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe
           spec_005 captured image_file_name — launch: C:\Windows\System32\cmd.exe; terminate: cmd.exe
           Test Files  74 passed (74)
                Tests  295 passed | 1 skipped (296)
10-agent   logman query CGAgent-KernelProcess -ets: No se encontró el Conjunto de recopiladores de datos.
```

The skipped test is `servercert_ac_008`, which runs only off Windows.

Landed on `main` commit by commit with `git cherry-pick --ff`: the branch
sat on `main`'s tip, so each commit landed under the SHA its CI had run on.
Before each push, `git diff --quiet <branch-sha> HEAD` stopped the loop on a
mismatch; none occurred. One push per commit, each waiting for its push
runs:

```text
SHA f876753: ts-ci -> success
SHA fcbf6ce: ts-ci -> success
SHA 833444e: ts-ci -> success
SHA fb74bca: ts-ci -> success
SHA acbdae2: ts-ci -> success
SHA 028ae86: ts-ci, rust-ci -> success
SHA 6d1f393: markdown-lint, dependency-audit -> success
```

GitHub closed PR #23 as merged, since `main` holds its head; the branch was
deleted, local and on origin.

Decisions (Claude Code's, inside the accepted ADRs):

- Reactive:
  - **The commits reordered**, the agent's crates before the workflow, so
    that no landed SHA is red. Alternative: squash the two.
  - **The cargo job kept strict**, its text corrected. Alternative: a
    severity threshold, which `cargo audit` does not offer without parsing
    its JSON output, and which would let a moderate advisory in the agent's
    TLS stack pass.
  - **anyhow 1.0.104 in the agent commit**, before the gate. Alternative:
    leave the unsound warning to a later change.
  - **`cherry-pick --ff`.** Alternative: new SHAs for the same trees,
    whose push runs would cover commits that never ran on a PR.

### #93: the close

The docs commit that closes #93:

- `docs/security/dependency-review.md`: the runbook the threat model names.
- The threat model: `:204` names the runbook; the record of the system as
  built (*Cross-cutting*) and R12 say what is audited and what is not
  (#110 to #113).
- CLAUDE.md:
  - *Environment facts*: the weekly run on `main`'s tip;
  - *Local pre-commit gate*: a *Dependencies* step, `pnpm audit --prod
    --audit-level=high` when a lockfile or a `package.json` changes, and the
    statement that `cargo audit` has no local mirror;
  - the elevated gate's expected outcome: `net_ac_008`'s and
    `auth_ac_012`'s lines print only under `--nocapture`, as gates 1 to 3
    showed.
- `docs/security/README.md` and `.github/workflows/README.md` link the
  runbook.

Decisions (delegated: the security posture, recorded for Manuel's audit):

- **What the runbook binds.** No advisory is ignored without Manuel
  accepting it as debt; a change to a crate the agent links, or to a package
  ingest's capture or detection path runs on, lands after the elevated
  gate; a crate that `cargo update` adds and the agent compiles is a new
  dependency of `cg-agent`, his. Alternative: leave each to the session's
  judgement.
- **Licences compatible with Apache-2.0**, the workspace's licence, checked
  when a dependency is added. Alternative: a list of allowed licences, which
  #112's tooling would enforce.
- **#112 to roadmap §F's packaging.** An SBOM describes a release, and
  §F makes the first one. Alternative: a sweep now, with nothing released
  to describe.

Self-review: one pass of the fixed SPEC prompt on the working tree, before
the commit. List A, seven points, all fixed:

- A1: the runbook said pnpm's moderate advisories are "printed"; they are
  counted in the summary line.
- A2: the runbook said development dependencies are not audited; `cargo
  audit` reads the agent's.
- A3: #111 said three warnings; the job at `6d1f393` prints two.
- A4: this record stated no open count for the register to match.
- A5: the threat model's cadence sentence (`:204`) named npm, Go and
  Python, beside the runbook that names neither.
- A6: the threat model's *New risks* line cited no handoff for R12's new
  debts.
- A7: CLAUDE.md said a green local gate predicts a green CI run, and
  `dependency-audit` had no local step.

From list B, taken: the as-built record's date (B1); a malicious release
with no advisory, under *What is not covered* (B3); a crate's `build.rs`
(B4); where `cargo audit` runs for a new dependency (B5); a crate `cargo
update` adds (B6); the detection path (B7); pnpm's ignore options (B9);
SPEC-022's other citations of nodemailer 8.0.10 (B10, in #13 below); the
two decisions above (B11); the matrix's test lines and the reviewer's
lists apart (B12); the expected outcome of the elevated gate (B15).
Recorded: `pnpm-workspace.yaml` and `rust-toolchain.toml` do not trigger
the audit (B8; neither holds an override today); the security README's
"Populated by" list (B16). On B14: gate 3's vitest log holds `Ô£ô` and
`ÔÇö` where pnpm printed `✓` and `—` (its UTF-8 read in the console's code
page), and its agent-session log an `Error:` line above the message; the
lines above show what pnpm printed, as handoff-40 did.

### #96: PR #24, waiting on ADR-0003

Branch `fix/s41-debt-96-dev-stack` (PR #24), one commit: NATS and MinIO out
of the dev compose, `.env.example`, the README and the Taskfile, and
`check-compose.mjs` refusing them until a SPEC uses them. Its review (the
fixed code prompt) found that ADR-0003 §Consequences says "The local
development environment requires all five services to be up", which the
change makes false. An ADR amendment is Manuel's: it is on the
owner-review branch `adr/s41-0003-dev-stack` (`b0b1bbf`, NOT YET
RATIFIED), and #96 lands after it. This section is completed when it lands.

### #82: the contract

SPEC-008's second Amendment 2026-10-16, *the session's idle lifetime and
its cap*, lands first as a document. Every request that passes the session
check rewrites the entry: `last_seen` now, and a time-to-live of 8 h or the
time left until 24 h after `issued_at`, whichever is shorter. A session
past its cap is deleted and answers `401`; the rewrite is `SET ... XX`, so
it never brings back a revoked session. The values are NFR-008-002's
defaults; its per-org setting stays an open question, with debt #20.

Decisions (delegated: the deployment contract and the security posture,
recorded in the amendment's *Decision* with their alternatives): what counts
as a use (every request that passes the session check, refused ones
included); the per-org setting deferred to roadmap §F's settings; `SET ...
XX`; a session past its cap deleted; an unreadable `issued_at` ending the
session.

For Manuel's audit, when the code lands: the threat model's §3 sentence
"Sessions are opaque random tokens in Redis, not signed, and last a fixed
8 h (debt #82)" becomes the sliding 8 h within a 24 h cap; and a token in
use works for up to 24 h after its login, where it worked for 8 h.

Two passes of the fixed SPEC prompt on the working tree:

- First pass, List A, four points, all fixed: what counts as a use (item 1
  and the *Decision* disagreed on a refused request); the first Amendment
  2026-10-15 item 9's "one Redis read", not listed in *Effect*; item 1
  citing §Operational §5 for the session check; an api `cgcsrf` cookie that
  does not exist. From List B, taken: the earlier references to "Amendment
  2026-10-16" meaning the first; SPEC-009's fixed 8 h clause; the security
  cost; a store failure in the rewrite; milliseconds; the handoff named; the
  password change among the deletions; the race test's seam; the README's
  "whichever is shorter". Recorded: the test harness's fixed `issued_at`
  (`services/api/test/helpers/db.ts`), which the code commit changes.
- Second pass, List A, two points, both fixed: this record did not yet list
  the threat model's change; the new text for SPEC-009 left out the cap
  and the deletions. From List B, taken: criteria for an unchanged
  `issued_at`, a refused request as a use, and a failed rewrite or delete;
  `503` over `401` for a failed delete; the per-org setting kept as an open
  question; the cookies' wording; the cost of a Redis that refuses writes;
  "writes once"; "not a time"; the catalog's delegation and edge; the race
  case outside "the real api"; the alternatives of three more decisions; no
  audit row for an expiry. Recorded: ADR-0014's "one O(1) Redis read"
  (`:35`), the cost of the option it chose, which leaves the lifetime to
  SPEC-008 (`:104`): not amended. No third pass ran on these fixes.

### #82: the code

Branch `fix/s41-debt-82-session-lifetime` (PR #25), one commit, rebuilt
once to fold its first review's points, CI on each push:

```text
03e0f35: ts-ci, markdown-lint -> success
33a56ba: ts-ci, markdown-lint -> success     the first review's points
```

Landed on `main` with `git cherry-pick --ff` behind `git diff --quiet
<branch-sha> HEAD`, under the SHA its CI ran on; GitHub closed PR #25 as
merged, and the branch was deleted.

```text
SHA 33a56ba: markdown-lint, ts-ci -> success
```

`resolveSession` (`services/api/src/auth/session.ts`) rewrites the entry
at each use with `SET ... PX ... XX`, deletes an entry past its cap or that
it cannot read, and lets a Redis error through as the session check's
`503`; the logout reads its session for the audit row without a second
use (`peekSession`). The test harness's `seedSession` writes the current
time as `issued_at`, which a 24 h cap needs. SPEC-009's logout clause
and the threat model's §3 sentence say the sliding lifetime.

apiauth_ac_022 ran red first against `main`'s code: 6 of 9 failed for the
missing behavior. The 3 that passed assert behavior kept: the login's 8 h,
no revoked session brought back, the logout. One case first passed by
accident (the seed's 1 h time-to-live equalled the hour left to the cap);
it was made to lower the time-to-live first, and failed. Local gates at
the tip: `services/api` 47 files, 294 tests (it was 46, 282); `dashboard`
10 files, 117 tests.

Self-review matrix (`services/api/test/apiauth-ac-022-session-lifetime.test.ts`):

| Criterion | Test | Verdict |
|---|---|---|
| A login's entry lives 8 h | "a login's entry lives 8 h" | covered |
| An idle session used gets 8 h again; `last_seen` in the request | "a use of a session idle for 7 h 59 min ..." | covered (the other fields unchanged, after the review) |
| 23 h after its login: the hour left; `issued_at` kept | "a use of a session issued 23 h ago ..." | covered |
| A refused request is a use | "a request the role check refuses ...", "a request the CSRF check refuses ..." | covered |
| Past the cap, or `issued_at` unreadable: `401`, deleted | "a session issued 24 h and one second ago ...", "... not a time ...", "... no issued_at ...", "... not a JSON object ..." | covered |
| A revoked session not brought back | "a use does not bring back a session deleted ..." | covered (the session module, a racing client) |
| The logout ends it | "a logout still ends the session" | covered |
| A failed rewrite or delete: `503` | "a rewrite Redis fails answers 503 ..." | covered |

Claude Code's own matrix had one *missing*, an entry with no `issued_at`,
added before the first review.

Two passes of the fixed code prompt (security code: a pass after the
fixes):

- First pass, nothing blocking. Fixed: the logout re-resolved its session,
  a second use (now `peekSession`); an entry that is not a JSON object
  answered `500` and stayed (now `401` and deleted); tests for a CSRF
  refusal as a use and for the fields a rewrite keeps; the threat model's
  sentence names the per-org setting's absence. Recorded: a Redis that
  refuses writes but allows `DEL` makes a logout answer `503` before its
  delete; an `issued_at` in the future is accepted.
- Second pass, nothing blocking; it checked that dropping `XX`, the cap
  or the rewrite, or rewriting `issued_at`, each fails a test. Recorded:
  the `503` case asserts the status only; `Date.parse` reads `"2026"` as a
  time; an `issued_at` far in the future is refreshed with no cap;
  `revokeAllForUser` still parses each entry unguarded, so an entry that is
  not JSON, which only a Redis writer can put there, makes a role, status or
  password change answer `503`. Each needs a writer to the shared Redis
  (R2, #85).

Decisions (anticipated, Claude Code's inside SPEC-008): `peekSession` for
the logout, where the alternative was passing the request's session down;
an unreadable entry deleted, like an unreadable `issued_at`, where the
alternative was a `500`.

### #31: the contract, its shape half

SPEC-017's Amendment 2026-10-16, *an event's time, id, pids and exit code
at ingest*, lands first as a document. An element's `time`, and a non-null
`created_time`, are 1 to 19 decimal digits below 2^63, what the
`DateTime64(9)` columns hold; a 1007 `event_id` is a UUID, its pids fit
`UInt32` and its `exit_code` `Int32`. Any other value makes the POST `400`
as a whole, as SPEC-019 refuses an invalid element. Today such values are
stored dated 1970 (`BigInt` reads `"0x10"` and `""`), stored wrapped, or
answered `500`, which the agent resends without bound (#32).

The first draft also bounded an element's `time` against its arrival: 5
minutes ahead and 7 days behind, the elements outside dropped with a `warn`
line and the POST answered `200`. The fixed SPEC prompt's first pass found
that half outside Claude Code's authority, and it was taken out:

- a bound behind arrival is a lateness horizon, which ADR-0012 (`:300`) and
  SPEC-018's owner decision 2 rule out;
- dropping elements under a `200` changes what ADR-0004 and ADR-0009 §1 call
  acceptance;
- the agent's times come from an ETW session clocked by the performance
  counter, so an honest agent's times might drift from the server's after
  its system clock is corrected, and the window would drop them unseen.

Debt #31 narrows to that half when the code lands; *What waits on Manuel* carries
it. The pass's other List A points, fixed: *What surfaced it* said a bad
`time` is always a `500` (`BigInt` stores `"0x10"` and `""` as 1970); 2^64
let through values the columns cannot hold (2^63 now); the partition count
of the window, gone with it. From List B, taken: criteria that fail before
the fix (`parent_pid` -1 was already refused; `"0x10"`, `" 1"` and 2^63
added); `exit_code`, unbounded against `Int32`; the catalog's placement and
its SPEC-005 edge; SPEC-017's header.

For Manuel's audit, when the code lands: the threat model's §2 record of
the system as built gains these checks; R9 stands.

The second pass of the fixed SPEC prompt, on the narrowed text, found five
List A points, all fixed: the register and R9 still named the bound
against arrival as #31's next sweep step (both now say it is Manuel's);
this record pointed at a *What waits on Manuel* section it did not yet
have (now below); the ETW clock claim held for processes and connections,
not logons, whose time is the Security log's `TimeCreated`; "stored
wrapped" was false from 1e21 up, which fails the insert; and a `time`
whose conversion error reads like a lost connection is answered `503`, not
`500`. From List B, taken: the pattern without a leading 0, as CGES writes
`created_time`; SPEC-005 AC-004's signed 64-bit bound; 2^63 for 4001 and
3002, `"0"` and `"0123"` among the refused times; the upper edges in
capture_ac_015, which already holds before the code and guards the edges;
the out-of-scope pointers of SPEC-018, SPEC-019 and SPEC-020; SPEC-017's
header and its status line; the delegation naming the API contract.
Recorded as a new debt, #114: a heartbeat whose own `body.sent_at` cannot be
converted is answered `503` and resent without bound. No third pass ran on
these fixes.

### #31: the code, its shape half

Branch `fix/s41-debt-31-event-shape` (PR #26), one commit, rebuilt once to
fold its review's points, CI on each push:

```text
59edb61: ts-ci, markdown-lint -> success
90ee7e7: ts-ci, markdown-lint -> success     the review's points
```

`services/ingest/src/schemas.ts` checks an element's `time` and a non-null
`created_time` in one refinement (`/^[1-9][0-9]{0,18}$/`, then `BigInt`
below 2^63), so that `BigInt` only reads digits; a 1007 `event_id` as a
UUID, its pids within `UInt32` and its `exit_code` within `Int32`. The
first implementation put the regex and the bound in two checks: zod runs a
refinement after a failed regex, and `BigInt("1.5e18")` threw (a `500`),
which the green run caught.

capture_ac_014 ran red first against `main`'s code: all 18 cases failed
for the missing behavior, 9 stored with `200` and 9 answered `500`.
capture_ac_015 passed before the code, as the amendment says: it guards the
edges. Local gate at the tip: typecheck, lint, build and entrypoints OK;
vitest 75 files, 308 passed, 2 skipped, 4 failed (the capture marquees,
exit code 9, unelevated). `rules_ac_007` failed once in that run on a
ClickHouse query timeout under memory pressure, and passed alone (3/3) and
in the elevated gate.

Elevated gate 1, run by Manuel on `90ee7e7` (`C:\tmp\s41-gate1.ps1`,
2026-10-16, 16:15 to 16:20). Its logs, as pnpm and cargo printed them:

```text
01-elev    net session: No hay entradas en la lista.
02-head    90ee7e7 / fix/s41-debt-31-event-shape
03-status  status lines: 0
05-build   Finished `release` profile [optimized] target(s) in 9.94s
06-cargo   test auth_ac_012_logons_on_the_real_log ... ok
           test capture_ac_006_elevated_shutdown_leaves_no_session ... ok
           test net_ac_008_a_real_connection_is_reported_with_its_probe ... ok
           test net_ac_009_an_accepted_connection_and_the_filter ... ok
           test ac_004_cache_hit_terminate_matches_launch_byte_for_byte ... ok
           test ac_007_parent_pid_only_when_parent_dead_at_child_launch ... ok
           test ac_009_events_lost_under_deliberate_etw_buffer_pressure ... ok
08-vitest  ✓ test/servercert-ac-renewal.test.ts (19 tests | 1 skipped) 8431ms
           ✓ test/detect-ac-001-marquee.test.ts (1 test) 46856ms
           ✓ test/spec-005-marquee.test.ts (1 test) 40520ms
           ✓ test/net-ac-010-marquee.test.ts (1 test) 25582ms
           ✓ test/capture-ac-014-015-event-shape.test.ts (19 tests) 1734ms
           ✓ test/auth-ac-013-marquee.test.ts (1 test) 25586ms
           ✓ test/rules-ac-007-loaded-at-start.test.ts (3 tests) 9956ms
           ✓ test/ac-001-marquee.test.ts (1 test) 4543ms
           detect_ac_001 captured image_file_name — probe: C:\Users\manul\AppData\Local\Temp\cg-detect-probe-Wat0Sj\winword.exe; child: C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe
           spec_005 captured image_file_name — launch: C:\Windows\System32\cmd.exe; terminate: cmd.exe
           Test Files  75 passed (75)
                Tests  314 passed | 1 skipped (315)
10-agent   logman query CGAgent-KernelProcess -ets: No se encontró el Conjunto de recopiladores de datos.
```

The skipped test is `servercert_ac_008`, which runs only off Windows. The
release build was a no-op: the agent's sources and `Cargo.lock` are gate
3's.

Landed on `main` with `git cherry-pick --ff` behind `git diff --quiet
<branch-sha> HEAD`, under the SHA its CI and the gate ran on; GitHub closed
PR #26 as merged, and the branch was deleted.

```text
SHA 90ee7e7: markdown-lint, ts-ci -> success
```

Self-review matrix (`services/ingest/test/capture-ac-014-015-event-shape.test.ts`):
every case of capture_ac_014 and capture_ac_015 covered, each asserting the
status, the body, no `cges_events` row and no heartbeat (014), or each value
as stored (015).

One pass of the fixed code prompt, nothing contradicted in the criteria.
Fixed: the amendment's *Effect* said a malformed element no longer makes a
`500`, but a lone surrogate in a member checked only as a string still does
(ClickHouse refuses it); the text now says so, in the same commit, with the
heartbeat's `sequence_number` and `uptime_seconds` of 1e21 or more joining
debt #114; the threat model's §2 said "unbounded" twice; a doc comment cut off
from its schema. Recorded: no positive control for the 4001 and 3002
baselines in the file (the reviewer's probe showed them valid);
capture_ac_015 leaves a row dated 2262 in the suite's shared ClickHouse. No
second pass ran on those text and comment fixes.

Decision (reactive): **one refinement for the nanoseconds**, where two
checks let `BigInt` throw on a value the regex had refused.

## Debts

New this session:

- **#114 — A heartbeat whose own `body.sent_at` cannot be converted, or
  whose `sequence_number` or `uptime_seconds` is 1e21 or more, is answered
  `503`, which the agent resends without bound.** The inner
  `sent_at` is checked only as non-empty (`services/ingest/src/schemas.ts`);
  `toChDateTime` throws inside the heartbeats insert's `try`, which answers
  `503 unavailable` (`services/ingest/src/routes/heartbeat.ts`). Only an
  authenticated agent can send one, and it blocks its own delivery.
  Surfaced by the review of SPEC-017's Amendment 2026-10-16. Destination:
  with #32.
- **#111 — `cargo audit` lets two warnings pass, one in the agent's
  runtime.** rustls-pemfile 2.2.0 is unmaintained (RUSTSEC-2025-0134); it
  is a direct dependency of `cg-agent`, which reads its certificates with it
  (`agent/cg-agent/src/tls.rs`). chacha20 0.10.0 is yanked, in the
  development dependencies only, through testcontainers. The cargo-audit job
  at `6d1f393` prints "2 allowed warnings found"; a third, anyhow 1.0.102,
  went with `028ae86`. Surfaced by the review of PR #23. Destination: rustls-pemfile replaced by rustls-pki-types' PEM reader
  with the next agent change under the elevated gate, as #87 is; chacha20
  with the test tooling's sweep (#110).
- **#112 — No tool checks the dependencies' licences, and no SBOM is
  produced.** A licence is checked by hand when a dependency is added
  (`docs/security/dependency-review.md`); the transitive ones are not.
  Destination: roadmap §F's packaging, with the release artifacts.
- **#113 — The workflows pin their actions by major tag, not by commit.**
  `actions/checkout@v5`, `actions/setup-node@v5`,
  `actions-rust-lang/setup-rust-toolchain@v1` and the rest run whatever
  their tag names when they run. Destination: a later sweep, each action
  pinned by commit SHA.

Changed this session:

- **#32**: narrowed by `90ee7e7`: a malformed `time`, `created_time`,
  `event_id`, pid or exit code no longer makes a `500`. A POST whose times
  spread over more than 100 days, or whose text member holds a lone
  surrogate, still does (SPEC-017 Amendment 2026-10-16, *Effect*).
- **#31**: SPEC-017's Amendment 2026-10-16 takes its shape half (`90ee7e7`,
  after elevated gate 1); #31 narrows to the bound against arrival, whose destination
  is now Manuel's decision: a lateness horizon, which ADR-0012 and
  SPEC-018 decided against for the MVP, with evidence on the agent's ETW
  clock first. The threat model's R9 says so.
- **#20**: SPEC-008's per-org session lifetime joins it, with Open
  question 2 (SPEC-008's second Amendment 2026-10-16, item 4).
- **#13**: SPEC-022's third Amendment (`:398`) names nodemailer 8.0.10 for
  the `ESOCKET` code of a certificate it cannot verify; since `fb74bca` the
  release is 10.1.1, which reports it the same way. A citation for the
  pattern sweep. The same SPEC's `:347`, `:348` and `:376` cite nodemailer
  8.0.10's file paths too, but they sit in an amendment's *What surfaced
  it* and *Decisions*: records of that moment (Class H-inline), not edited.

Closed this session:

- **#82**, by `33a56ba`: a session ends after 8 h without use, and 24 h
  after its login at the latest (SPEC-008's second Amendment 2026-10-16).
- **#93**, by `f876753` to `6d1f393`: the production dependencies past their
  advisories, `dependency-audit.yml`, and the runbook.

**Open at the commit that records #114: 71** (`docs/debts.md`,
which matches): 69 at the start, #93 and #82 closed, #111 to #114 new.

## What waits on Manuel

- **ADR-0003's Amendment 2026-10-16, for his "sí".** On the owner-review
  branch `adr/s41-0003-dev-stack` (`b0b1bbf`, NOT YET RATIFIED):
  §Consequences' "The local development environment requires all five
  services to be up" becomes "the dev stack runs the backends the code
  uses; NATS JetStream and MinIO join it with the first SPEC that uses
  them". Debt #96's change (PR #24, green and reviewed) lands after it.
- **The dev stack's containers on this machine.** The `cyberguard-dev-*`
  containers were created before the compose bound its ports to
  `127.0.0.1` (debt #18), and still publish them on every interface, with
  the dev passwords. Recreating them from the compose keeps their volumes;
  he chooses whether Claude Code does it.
- **#31's other half: an event's time bounded against its arrival.** A
  bound behind arrival is a lateness horizon, which ADR-0012 and SPEC-018
  decided against for the MVP; a bound ahead needs evidence that an honest
  agent's ETW times track the server's clock. Recommendation: no backward
  bound in the MVP; a forward bound once a gate measures the agent's clock
  after a system clock correction.
- **The `401` half of #80: his reading of ADR-0014 §Compliance (`:98`).** The
  opening prompt kept a slot for it, which reached the session unfilled.
  Recommendation, as handoff-40's: a request with no session is not an
  authentication event, and #80 closes.
- **cargo-audit on the *Approved local toolchain*.** Without it, a
  `Cargo.lock` change is checked by its pull request's CI alone (CLAUDE.md,
  *Local pre-commit gate*). Installing it is his to approve.
- **The threat model, for his audit.** This session changed it under his
  delegation: *Dependency review* (`:204`) and R12 (#93); the record of the
  system as built, *Cross-cutting* (#93) and §3's session lifetime (#82); R9's
  destination (#31).
