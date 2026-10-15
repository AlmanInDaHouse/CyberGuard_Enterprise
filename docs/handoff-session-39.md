# Handoff — Session 39

Session 39 opened and closed on 2026-10-15. Manuel asked for the debt sweep
and delegated every decision that waits on him. The session:

- Landed the compose check in CI (#68), one source for the ClickHouse DDL
  (#6) and per-run ids in the ingest tests (#3).
- Amended SPEC-008, SPEC-009, SPEC-010 and SPEC-022 after four reviews of
  their text, and implemented them: the SMTP send authenticates only over
  TLS (#74, delegated), nodemailer pinned (#75), SPEC-022's untested
  statements tested (#76); the api's auth surface brought to its contract:
  strict bodies (#72), Redis and Postgres bounded (#69, #71), org scope,
  account disable, the last admin, a revocation a racing login cannot
  outlive, constant work, a TOTP confirmation that no longer defeats the
  second factor (#81), a complete audit row.
- Brought the threat model in line with the system as built (#55), with
  fifteen open risks, each a debt with a destination.

This record was started early because the SPEC amendments cite the debts it
records. Its work order for Session 40 is *How Session 40 resumes* at the
end.

## State at close

- `main` = `origin/main` = the commit that completes this file (CI ALL
  GREEN, *CI monitoring*); tree clean; no stash.
- Catalogs: ADR 21 / SPEC 21. Known CI debt: zero.
- No branch or PR open. PRs #12, #13, #14, #15, #16 and #17 are closed and
  their branches deleted, local and on origin (*Progress*).
- Roadmap: no phase changed status; §F gains its items from this session
  (`docs/product/roadmap.md`).
- Code changed this session: `infra/dev/` and `.github/workflows/` (#68);
  `services/ingest/src/db/` and the api's test harness (#6); the ingest tests
  (#3); `services/ingest/src/notify/` and `src/soar/` (#74–#76);
  `services/api/src/` (the auth sweep). No gated path changed, and no
  elevated gate ran (*Gate outputs and baselines*).
- Outside the repository: Docker Desktop is running, with the dev stack and
  other projects' containers. `C:` has 15 GB free (23 GB at the start).
  No ETW session of the agent's name is running.

## Start

- `main` = `origin/main` = `0b00656`, tree clean, no stash; no branch, no
  open PR; CI of the tip `markdown-lint → success`, the only workflow
  triggered.
- Catalogs ADR 21 / SPEC 21; Known CI debt zero; debts up to #76, 52 open.
- `logman query -ets` listed no `CGAgent` session. Docker Desktop was
  running (server `29.1.2`). `C:` had 23 GB free.

Manuel's decisions this session, in chat:

- 2026-10-15, opening the session: "sigue con el barrido de deudas de la
  sesión 39, y todo lo que dependa de mi elige tu en beneficio del
  proyecto". Claude Code read it as the order to run the sweep and as a
  delegation of every decision that waits on him, decided for an
  enterprise buyer as on 2026-10-14. An ADR text still needs his explicit
  "sí", and the elevated gate is still his to run.

## Progress

### Sweep 3: #68

Branch `fix/s39-debt-68-compose-check` (PR #12). It was rebuilt once to fold
the first review's fixes into its two commits (*Self-reviews*), pushed with
`--force-with-lease`, one commit per push, CI green on each. `main` had not
moved, so no rebase. It landed by cherry-pick, each with a guard that stops:

```text
guard 0d9a481 -> 0   SHA 524cb5a: markdown-lint -> success | infra-ci -> success
guard 1ac2550 -> 0   SHA 5e06719: schema-validation -> success | markdown-lint -> success
```

| SHA | What |
|---|---|
| `524cb5a` | #68: `infra-ci` renders the dev compose, every profile included, and `infra/dev/check-compose.mjs` asserts loopback ports, no host network, images named by a digest or a tag other than `latest`, and ClickHouse pinned to the patch release both test backends start; `check-compose.test.mjs` tests the check. |
| `5e06719` | `schema-validation` on Node 22 (Node 20's end of life was 2026-04-30); its README row names the playbooks. |

PR #12 is closed and its branch deleted, local and on origin.

### Sweep 3: #6

Branch `fix/s39-debt-6-clickhouse-ddl` (PR #14), one commit, rebuilt once to
fold its review's fixes, then rebased onto `5e06719` by cherry-pick and
pushed again with CI green (`4b777e0`: `infra-ci` and `ts-ci → success`).
It landed by cherry-pick:

```text
guard 4b777e0 -> 0   SHA b10bc3d: infra-ci -> success | ts-ci -> success
```

`b10bc3d`: ingest's ClickHouse bootstrap moves, verbatim, into
`services/ingest/src/db/clickhouse-bootstrap.ts`, and the api test harness
runs it instead of its own copy of `cges_events`, which lacked the
`arrived_at` index and the 3002 time-to-live. `SHOW CREATE TABLE` of both
tables is identical after `main`'s six statements and after the module. A
tripwire fails if an api test defines either table again. SPEC-010's
Amendment 2026-10-15 records its Open question 1 as resolved. PR #14 is
closed and its branch deleted, local and on origin.

### Sweep 3: #3

Branch `fix/s39-debt-3-harness-ids` (PR #13), one commit. Its first version
renamed the three literals that repeated across files and added a tripwire;
the first review showed that this was not the allocator the debt names, so
the commit was rebuilt twice (*Self-reviews*), then rebased onto `b10bc3d`
and pushed with CI green (`61259c1`: `ts-ci → success`). It landed by
cherry-pick:

```text
guard 61259c1 -> 0   SHA ecd0fd4: ts-ci -> success
```

`ecd0fd4`: every agent the ingest tests write to the shared stores is drawn
at random per run, and each file that builds alert or event ids from a
sequence starts them with a prefix drawn per run. The ids still written by
hand are unique across files. A tripwire fails when a literal repeats
another file's or an id is built on a fixed prefix. PR #13 is closed and
its branch deleted, local and on origin.

### The SPEC amendments

Straight to `main` after four passes of the fixed SPEC prompt (*Self-reviews*),
in one push with this record's first form, so that every debt they cite was
recorded at the same SHA:

```text
SHA 34fe0ea: markdown-lint -> success
```

`34fe0ea`: SPEC-008 Amendment 2026-10-15 (the user routes, revocation, the TOTP
confirmation, the stores' bounds), SPEC-009 Amendment 2026-10-15 (the bounds
reach the read routes), SPEC-010 Amendment 2026-10-15 (Open question 1
resolved, #6), SPEC-022 Amendment 2026-10-15 (credentials only over TLS, #74),
the catalog, CLAUDE.md's local-gate step for `infra-ci`, and this file.

### Sweep 3: the send (#76, #75, #74)

Branch `fix/s39-send-tls`, three commits. #76 and #75 were written against
SPEC-022's second amendment, already on `main`; #74 was committed locally and
pushed only after its amendment landed. The branch was rebased twice onto a
moving `main`, each time pushed again one commit per push with CI green.
PR #15 was replaced by PR #16 (*Decisions*, reactive). The first review's
fixes were folded into the #74 commit. It landed by cherry-pick:

```text
guard dac1daa -> 0   SHA a8deda8: ts-ci -> success
guard 8fdfb32 -> 0   SHA b560665: ts-ci -> success
guard e8029fb -> 0   SHA 0e398e6: ts-ci -> success
```

| SHA | What |
|---|---|
| `a8deda8` | #76: the run reads an injectable clock (`SoarConfig.now`) and SPEC-014's send an injectable limit (`NotifyConfig.sendLimitMs`); a step the run's bound reaches before it starts, and `ESENDLIMIT` in SPEC-014's log, each with a test. |
| `b560665` | #75: nodemailer pinned to `8.0.10` exactly; soar_ac_006 asserts the installed release. |
| `0e398e6` | #74: the send authenticates only over TLS and fails `ENOTLS` otherwise; the test SMTP server offers STARTTLS with a loopback certificate; the AUTH cases run over it; an unverifiable certificate (`ESOCKET`), a refused STARTTLS (`ETLS`), CRAM-MD5 without TLS and a message over TLS with no mechanism are tested. |

PRs #15 and #16 are closed and the branch deleted, local and on origin.

### Sweep 3: the api (#72, #69, #71, #81)

Branch `fix/s39-api-auth` (PR #17), five commits written after `34fe0ea`, each
red-tested against the commit before it. Three fresh-context reviews; after
each, the fixes were folded into the five commits and the branch was pushed
again one commit per push with CI green (*Self-reviews*). It landed by
cherry-pick:

```text
guard 7830764 -> 0   SHA aa70bb4: ts-ci -> success
guard 3946f1c -> 0   SHA 0c3ca78: ts-ci -> success
guard 86472d1 -> 0   SHA 7900aee: ts-ci -> success
guard df29fe5 -> 0   SHA ee5235a: ts-ci -> success
guard 99eea90 -> 0   SHA bde96b1: ts-ci -> success
```

| SHA | What |
|---|---|
| `aa70bb4` | Items 14–15 (#69, #71): the Redis client and the Postgres pool bounded; the auth service reads both through a view whose every failure is a `503`; an error inside the rate limiter's EXEC is one too. apiauth_ac_014; read_ac_007's silent open connection. |
| `0c3ca78` | Items 1–2 (#72): strict bodies after the session, role, CSRF and `:id` checks, all `onRequest` hooks; a Fastify 4xx body `bad_request`; the logout's CSRF; request ids as UUIDs. apiauth_ac_010; auth_ac_003 over every admin route; auth_ac_007's accepted case. |
| `7900aee` | Items 3, 5–10: addresses normalized (migration `0004`), the org scope, `404`/`409`, the status route, the last admin, the authority version (migration `0006`) read twice by the login, audit rows with actor, org, target, request id, source IP and detail (migration `0005`); the sessions deleted after the commit, and after a COMMIT whose answer was lost. apiauth_ac_011 to 013; auth_ac_006. |
| `ee5235a` | Items 11–12 (#81): constant work at the login (Argon2 and the decrypt a found row costs); the TOTP confirmation only before enrollment, limited per user and on the login's per-IP counter, its code marked used. apiauth_ac_015, apiauth_ac_016. |
| `bde96b1` | Item 13: the bootstrap CLI through the API's schema and policy, with its audit row, a conflict for a taken address, and a test that runs it as a program. apiauth_ac_017. |

PR #17 is closed and its branch deleted, local and on origin.

### #55: the threat model

The threat model's Amendment 2026-10-15 (the system as built), after one pass
of the fixed SPEC prompt (*Self-reviews*), with debts #85–#101 in this file.
An audit by a subagent, read-only on `main`, gave the evidence table it rests
on.

## Decisions

Delegated by Manuel on 2026-10-15 ("todo lo que dependa de mi elige tu en
beneficio del proyecto"), decided by Claude Code for an enterprise buyer (a
security lead and a data-protection officer). Each is recorded in its SPEC
amendment, where CLAUDE.md's *Decision authority* would otherwise make it
his:

- **#74, credentials only over TLS** (SPEC-022 Amendment 2026-10-15): the
  send authenticates only on an encrypted connection and fails `ENOTLS`
  otherwise, always, with no setting. Alternatives: a setting defaulting to
  TLS, which lets an operator turn the rule off; TLS for every send, which
  refuses internal relays that take mail by address. It changes which relays
  an operator can use: a deployment-contract decision.
- **The password policy** (SPEC-008 Amendment 2026-10-15 item 4): 12 to 128
  code points with a letter and a digit, PCI DSS v4.0 8.3.6. Alternatives:
  NIST's 8 with a second factor; no composition rule (ASVS V2.1.9).
- **The auth surface's breaking changes** (SPEC-008 Amendment 2026-10-15):
  strict bodies, `404`/`409`/`429`/`503` where `200` or `500` answered, CSRF
  on the logout, the TOTP confirmation only before enrollment. A public API
  contract.
- **The last admin protected in the api; Redis failing closed with `503`**
  (the same amendment, items 10 and 14).
- **A Redis error reply in the read API answers `503`, not `500`**
  (SPEC-009 Amendment 2026-10-15).
- **The dashboard's login goes with roadmap §F** (#83): an installation that
  ends without a working login does not meet criterion 7, so F carries it;
  the README's criterion 3 now says the login is delivered in the api and
  still a stub in the dashboard. Alternative: a phase of its own before F,
  which F would wait on anyway. Where: `docs/product/roadmap.md` §F.
- **The threat model's Amendment 2026-10-15** (#55): the system as built,
  section by section, and fifteen risks it did not name, listed as open,
  each a debt with a destination. Risk treatment is the security posture,
  Manuel's. Alternative: rewrite the Phase-0 text in place, which would erase
  the plan the later phases still follow.

Anticipated:

- **The sweep's order**: the api's input and stores (#72, #69, #71), then
  the compose check (#68), the send (#76, #75, #74), #6, #3, as handoff-38's
  *How Session 39 resumes* ordered them; #68, #6 and #3 ran while the SPEC
  texts were in review, since their contracts were debt entries.
- **#72 widened to SPEC-008's surface.** Reading the routes for #72 found
  that no body was validated, the audit rows named the wrong principal, the
  routes ignored the org, accounts could not be disabled, and the TOTP
  confirmation defeated the second factor (#81). One amendment covers them.
  Alternative: fix #72 alone and record the rest as debts, which leaves an
  MFA bypass open.
- **#68 as a workflow of its own** (`infra-ci`), not a job in `ts-ci`, so a
  compose change does not run the TypeScript suites. Where: `524cb5a`.
- **#6: ingest's bootstrap exported and run by the api tests**, not a
  shared package, since the api tests already load ingest's migrations by
  URL. Where: `b10bc3d`.
- **#6 needs no elevated gate.** The statements moved verbatim, and `SHOW
  CREATE TABLE` of `heartbeats` and `cges_events` is identical after `main`'s
  six statements and after the module, so the `cges_events` schema did not
  change (CLAUDE.md *Developer-local SPEC-006 marquee validation* §6). The
  session's closing gate covers the tree anyway.
- **SPEC-010's amendment after its code.** `b10bc3d` landed before SPEC-010
  Amendment 2026-10-15. Open question 1 had named the shared module as the
  remedy and an uncaught drift as its trigger, so the code followed the SPEC
  as written, and the amendment records it. The fourth review of the texts
  found the amendment still in the future tense; it was put in the past.

Reactive:

- **Revocation by an authority version, not a stamp** (SPEC-008 Amendment
  2026-10-15 item 9). The first draft revoked before and after the commit,
  which a login spending its Argon2 verification across both defeats (first
  review). The second draft stamped a revocation time in Redis, read on every
  request, which replaced ADR-0014 §2's single delete and depended on a clock
  the VM steps back (second review). The version is read twice by the login,
  so the request path keeps one Redis read and no clock is involved.
- **The checks as `onRequest` hooks.** Fastify parses the body before
  preHandlers run (measured by the second review), so a caller without a
  session learned a body's form before its `401`.
- **SPEC-008's new criteria renamed `apiauth_ac_*`**: SPEC-020's logon
  criteria use `auth_ac_001` to `auth_ac_014`, and auth_ac_013 is an
  elevated-gate test (third and fourth reviews).
- **The trusted roots absent, not empty** (SPEC-022): an empty `ca` list
  replaces Node's roots with none (measured by the second review).
- **#3 rebuilt into an allocator.** Its first version renamed three literals
  and added a tripwire; the first review showed it was not what the debt
  names. Where: `ecd0fd4`.
- **#68's check widened**: profiles rendered, the host network refused, a
  digest accepted with `latest`, and a test of the check (first review).
- **A fix of the first api review introduced the most serious defect the
  second found**: deleting the sessions on any store failure let an admin of
  any org end any user's sessions during a Postgres outage, with no row.
  Hence a third pass before landing, where S38's practice would have landed
  after the second. Where: `7900aee`.
- **No elevated gate.** No change touched a gated path. Alternative: a gate
  run for confidence, which would delay the close on Manuel's terminal; the
  next gated change (#88's) runs it.
- **PR #15 replaced by PR #16.** After the send branch was rebased and
  force-pushed, GitHub kept PR #15's head at `d17eeea` and ran no workflow
  on `62d564b`; closed, it could not be reopened ("state cannot be changed.
  The fix/s39-send-tls branch was force-pushed or recreated."). PR #16 is
  the same branch.

## Self-reviews

### The SPEC texts: SPEC-008, SPEC-009, SPEC-010 and SPEC-022 Amendments 2026-10-15

Claude Code's own pass checked each cited path and each claim made in more
than one place. A reviewer with a fresh context, the fixed SPEC prompt, made
four passes on the working tree:

- **Pass 1, 4 items in List A**, all fixed: "before any store" against the
  session check, which reads Redis, and the two CSRF and RBAC tests it would
  have broken; the race between a login's Argon2 and the revocation; no
  CI-blocking RBAC test for the new admin route; ADR-0002 Rule 4 cited for
  strict schemas (debt #84). From List B, also fixed: the audit row's org,
  the order of revocation and the last-admin check, a no-op change,
  code-point counting, the legacy addresses, the CLI, the org scope, the
  Fastify parse errors, the untested statements, the constant work for an
  unknown address, and the TOTP confirmation (#81).
- **Pass 2, 7 items in List A**, all fixed: the stamp's lifetime against
  NFR-008-002; ordering by a clock that steps back; a no-op change that
  revoked sessions without a row; ADR-0014's single delete; Fastify's 413
  and 415; an empty list of trusted roots; #84's count. From List B: the
  checks moved to `onRequest`, the Redis timeouts, the migration's
  JavaScript functions, the not-backward-compatible list, the CLI's row,
  `ESOCKET` for a certificate failure.
- **Pass 3, 6 items in List A**, all fixed: the playbook executions said to
  read ClickHouse, the drill left out; `text/plain`, which Fastify parses;
  the logout's body; the attack's need for a user id; the TOTP
  confirmation's org; #84's section. From List B: the criteria renamed
  `apiauth_ac_*`; the user id lower-cased and the per-IP counter shared at
  the confirmation; the passages of the second SPEC-022 amendment replaced.
- **Pass 4, 4 items in List A**: SPEC-020's criteria range (fixed); SPEC-010
  in the future tense (fixed, *Decisions*); rows for refused changes between
  items 6 and 12 (fixed); and the delegation against CLAUDE.md's "explicit
  yes in chat to a specific proposal" and the deployment contract's owner
  STOP. The last is recorded for Manuel (*What waits on Manuel*), as SPEC-009
  Amendment 2026-10-14 was. From List B, also fixed: the clock's owner, the
  response body of a Fastify 4xx, the Postgres-side collision check, the
  password change's own session, the not-backward-compatible list, the CLI's
  taken address, the debts' record named.
- Pass 4's fixes had no fifth pass; each answers an item of pass 4.

### #68

Contract: debt #68 (`docs/handoff-session-38.md:1087-1093`); for the second
commit, the defect statement (Node 20's end of life, 2026-04-30).

| Criterion | Test | Verdict |
|---|---|---|
| A change to the dev compose runs a check | `infra-ci.yml` path filters; ran on every push | covered |
| The check renders the compose, every profile included | `infra-ci.yml`, `--profile '*'` | covered |
| Every published port binds `127.0.0.1`; no host network | `check-compose.test.mjs`: a port on `0.0.0.0`, one with no host address, `network_mode: host` | covered |
| Every image named by a digest or a tag other than `latest` | the same file: no tag, `latest`, `latest` with a digest | covered |
| ClickHouse on a patch release, the backends on the same | the same file: `24.8`, a backend on another release, a backend with none | covered |
| `schema-validation` on Node 22 | the CI log of `6ef570c`: `node: v22.23.3` | covered |

A reviewer with a fresh context, the fixed code prompt, one pass on
`0b00656..6ef570c`. List 1: the ports and the images weak, since a service
under a profile is not rendered (a mutated compose with `busybox:latest` on
`0.0.0.0` passed) and `network_mode: host` passed; no test of the check.
List 2: the READMEs claimed more than the code checked; `latest` with a
digest refused against the text. List 3: the render with the dev passwords
written into the working tree; CLAUDE.md's local gate without the step; the
`schema-validation` README row stale. Dispositions: all fixed and folded into
the two commits (`0d9a481`, `1ac2550`); the local-gate step in CLAUDE.md
with this record. Red run: the check's test against the first version, "pass
4 / fail 2" (the host network and the digest). The fixes had no second pass;
each answers an item of the first.

### #6

Contract: debt #6 (`docs/handoff-session-26.md`) and SPEC-010 §Open
questions 1.

| Criterion | Test | Verdict |
|---|---|---|
| One source for the ClickHouse DDL | `services/api/test/harness-clickhouse-ddl.test.ts` (no api test defines `cges_events` or `heartbeats`); red with `main`'s copy present | covered |
| Ingest's tables unchanged | `SHOW CREATE TABLE` compared after `main`'s statements and the module (identical); late_ac_007, net_ac_003, auth_ac_003 run the real bootstrap | covered |
| The api reads the table ingest writes | the api suite, 34 files / 116 tests, on the module | covered |

One pass of the fixed code prompt on `0b00656..3fb8cc8`. List 1: the single
source weak (no test kept the copy out). List 2: stale citations of
`migrate.ts` in SPEC-010, ADR-0015, ADR-0012, ADR-0020, SPEC-006, -018, -019,
-020 and two test comments; SPEC-010's Open question still open. List 3: the
api test ClickHouse also gets `heartbeats`, the index and the time-to-live;
a typed cast of the imported function. Dispositions: the tripwire, the test
comments and the helper's name fixed in the commit (`68e4a57`); SPEC-010's
amendment; the ADR and other SPEC anchors left to the citation sweep (#13);
the cast recorded (`check-api` runs on `services/ingest/**`). The fixes had no
second pass.

### #3

Contract: debt #3 (`docs/handoff-session-26.md`).

| Criterion | Test | Verdict |
|---|---|---|
| No hard-coded agent id collides across files | `services/ingest/test/harness-uuid-literals.test.ts`, three tests; red at `0b00656` with the three clashes and thirteen fixed-prefix templates | covered for the tree |
| An allocator | every agent the shared-store tests write drawn with `randomUUID()`; per-file `ID_PREFIX` for built ids | covered, as call sites rather than a helper |

Two passes of the fixed code prompt. Pass 1 (`1e25222`): the destination
missing, since three renames and a tripwire are not an allocator; templates
and per-file sharing unseen; the comment overstating the risk. Disposition:
the commit rebuilt (`d42fe83`) with random agents, per-run prefixes and a
fixed-prefix check. Pass 2 (`d42fe83`): seven hand-numbered agents left in
shared stores, a false comment, a style slip in late_ac_001. Disposition: all
fixed and folded (`61259c1`); recorded without change: test agents are now
UUIDv4 where production enrolls v7 (nothing reads the version), and ids
change per run. The fixes of pass 2 had no third pass.

### The send (#76, #75, #74)

Contract: SPEC-022's second Amendment 2026-10-14 and its Amendment 2026-10-15;
debts #74–#76 (`docs/handoff-session-38.md`).

| Criterion | Test (`services/ingest/test/soar-ac-006-bounds.test.ts` unless named) | Verdict |
|---|---|---|
| A step the run's bound reaches before it starts | the injected-clock case | covered; red without the clock |
| `ESENDLIMIT` at SPEC-014's limit | the 15 000 ms default asserted; a live send cut at an injected 500 ms | covered as a pair; red without the injected limit |
| nodemailer pinned | the installed-release case; `package.json`, the lockfile | covered |
| Credentials only over TLS, `ENOTLS` otherwise | the `PLAIN LOGIN` and `CRAM-MD5` cases without STARTTLS; a run and SPEC-014's log | covered; 6 red against the send before #74 |
| The AUTH cases over STARTTLS | the `PLAIN LOGIN` / `XOAUTH2 LOGIN` cases; `soar-ac-008-no-secret.test.ts` | covered |
| The certificate verified; a refused STARTTLS | the `ESOCKET` and `ETLS` cases | covered |
| Implicit TLS on 465 counts as TLS | none | missing: binding 465 needs privilege; read in `bounded-send.ts` |

One pass of the fixed code prompt, on `34fe0ea..14de4e2`. List 1: the
verification, 465, CRAM-MD5 without TLS, a refused STARTTLS and the message
over TLS with no mechanism missing or weak; the production builder passing
no roots true by reading. List 2: none. List 3: the test seams
(`SoarConfig.now`, `NotifyConfig.sendLimitMs`, `buildNotifyConfig`'s
`options.tls`) unstated; a stale comment ("Two fields private"). Dispositions:
the four tests and the comment folded into the #74 commit (`e8029fb`); 465
recorded above; the seams recorded here (debt #76 named an injectable bound or
clock). The fixes had no second pass.

### The api (#72, #69, #71, #81)

Contract: SPEC-008 Amendment 2026-10-15 and SPEC-009 Amendment 2026-10-15.

| Criterion | Test (`services/api/test/`) | Verdict |
|---|---|---|
| auth_ac_003 over every admin route | `auth-ac-003-rbac-enforcement.test.ts` | covered for the denial; the admin pass in this file only on create |
| auth_ac_006, the rows' values | `auth-ac-006-audit-append-only.test.ts` | covered, the correlation id against the request's own `reqId` |
| apiauth_ac_010 | `apiauth-ac-010-bodies.test.ts` | covered for the listed cases; not every case on every route |
| apiauth_ac_011 | `apiauth-ac-011-email.test.ts` | covered; the Postgres-side `lower()` check of 0004 untested |
| apiauth_ac_012 | `apiauth-ac-012-scope.test.ts` | covered |
| apiauth_ac_013 | `apiauth-ac-013-disable-revocation.test.ts` | covered; the race forced by a gate inside Argon2, red without the second read; two concurrent demotions not forced to interleave (weak) |
| apiauth_ac_014 | `apiauth-ac-014-stores.test.ts` | covered for Redis (three modes, two routes) and the cancellation |
| apiauth_ac_015 | `apiauth-ac-015-constant-work.test.ts` | covered: one Argon2id verification per branch, the dummy decrypt |
| apiauth_ac_016 | `apiauth-ac-016-totp-confirm.test.ts` | covered for sequential callers (#101 for bursts) |
| apiauth_ac_017 | `apiauth-ac-017-cli.test.ts` | covered, the program run included |
| read_ac_007's new case | `read-ac-007-store-failure.test.ts` | covered |

Three passes of the fixed code prompt:

- **Pass 1, `0e398e6..f38863e`.** Defects: the login measured the address
  before trimming it (an all-spaces address reached Postgres and counted a
  failure; a NUL produced unlimited `503`s); a lost COMMIT answer skipped the
  session deletion; the read routes still `preHandler`. Weak: the correlation
  id, several body cases, the 0004 refusal by spaces, `/status`'s `404`s,
  the slow-read test's comment, the per-IP confirmation limit. Dispositions:
  all fixed and folded (`4497245..7b3b01a`).
- **Pass 2, `0e398e6..7b3b01a`.** The serious one, introduced by pass 1's
  fix: on any Postgres failure, before BEGIN included, the deletion ran for
  any user id, cross-org, with no row. Also: no constant work at the
  confirmation; the lazy constant-work hash; a ROLLBACK queued behind a hung
  query (about 12 s); a `23505` discarding its connection; a raced login
  counted as a failure; the CLI's main untested as a program. Dispositions:
  the deletion only on a COMMIT without an answer, the rest fixed, each with
  a test red against the previous version (`4497245..36df5bb`). Recorded:
  `changePassword`'s limits (debt #100).
- **Pass 3, `0e398e6..36df5bb`.** A statement cancel returned its connection
  to the pool; a COMMIT the server refused still deleted sessions; no decrypt
  on the unknown-address path; an error inside the limiter's EXEC not a
  `503`; the address measured before lower-casing; the password change's row
  org from the user row; the CLI's path comparison canonical on one side.
  Dispositions: all fixed and folded (`7830764..99eea90`), the limiter's with
  a test red against the previous code (`401` for `503`). Recorded: the
  limiter's read-then-count (debt #101); `asStore` reporting a schema error as
  an outage; the runner's message for every migration.
- Pass 3's fixes had no fourth pass; each answers an item of pass 3.

### The threat model's amendment

Claude Code's own pass checked each cited path and the risk-to-debt map. Two
passes of the fixed SPEC prompt on the working tree:

- **Pass 1, 5 items in List A**, all fixed: "open, not accepted" credited to
  Manuel, though handoff-38 records it as Claude Code's decision; #84's
  threat-model line left uncorrected; the connections of class 4001 missing
  as an asset; the 2026-10-13 amendment's columns and time-to-live read as
  built; the Prometheus metric and the login's progressive back-off left as
  if built. From List B, also fixed: R1's destination against SPEC-002
  NFR-003, R2's instance per service against ADR-0003, the token's citation,
  #31's record, the risks untagged in the sections, the MinIO credentials,
  R4's signing before the check, R13's "over HTTP" as a default, #94's
  missing call site, the `image_file_name` asset, the reverse-proxy
  acceptance that does not apply, sessions not signed.
- **Pass 2, 3 items in List A**, all fixed: "a denied request writes no
  row", though the login and the TOTP confirmation write one; R9 with no
  destination, since #31 records none (now a SPEC-017 amendment, and in
  *How Session 40 resumes*); the References entry for ADR-0002 repeating the
  miscitation (named, and in #84). From List B, also fixed: the window from
  the last failure, the containers named, the CA's 10 years, the
  certificate with its key and the token's reference, the sequence-number
  defense dropped by ADR-0004, the agent's drops, Redis's expiries, R7's
  Postgres half, R6 conditional, the full path, `GET /v1/users` unpaged, the
  References. Pass 2's fixes had no third pass; each answers an item of
  pass 2.

## Gate outputs and baselines

No change of the session touched a gated path (CLAUDE.md *Developer-local
SPEC-005 marquee validation* §5, *SPEC-006* §6): no file under `agent/`,
`services/ingest/src/detect/`, `rules/windows/`, `routes/heartbeat.ts` or
`schemas.ts`, and the `cges_events` schema did not change (#6, *Decisions*).
No elevated gate ran. S38's gate 4 is not evidence for this tip, since
`services/` changed: the next gated change runs the gate again, #88's first.

Non-elevated, Claude Code's terminal, at the tip of `main` (`bde96b1`):

- `services/ingest`: `pnpm run typecheck`; `pnpm run lint` ("Checked 130
  files … No fixes applied."); `pnpm run build`; `pnpm test` → "Test Files  4
  failed | 67 passed | 1 skipped (72)", "Tests  4 failed | 269 passed | 1
  skipped (274)": the four capture marquees with the agent's exit 9
  ("insufficient privilege"), and `ac-001-marquee` skipped, the expected
  unelevated result.
- `services/api`, at `99eea90` (the tree of `bde96b1`): `pnpm run typecheck`;
  lint ("Checked 86 files … No fixes applied."); build; "entrypoints ok: [
  'dist/index.js', './dist/cli/create-user.js' ]"; `pnpm test` → "Test Files
  43 passed (43)", "Tests  216 passed (216)".
- `infra/dev`: `node --test infra/dev/check-compose.test.mjs` → "ℹ pass 6";
  the render and the check → "compose ok: 7 services, 11 published ports on
  127.0.0.1, clickhouse/clickhouse-server:24.8.14.39 in the compose and in 2
  test backends".
- The `cges_events` and `heartbeats` DDL compared on the dev ClickHouse:
  "main statements: 6", "heartbeats: identical", "cges_events: identical".

The red runs, against the code before each change:

- #68's check against its first version: "ℹ pass 4", "ℹ fail 2" (the host
  network, the digest with `latest`).
- #3's tripwire at `0b00656`: the three clashes, then 13 fixed-prefix
  templates.
- #76: "expected [ 'Pending first', 'Pending second' ] to deeply equal [
  'Pending first' ]"; the ESENDLIMIT case "expected true to be false".
- #74: six failures against the send before it ("expected undefined to be
  'ENOTLS'", "expected 'completed' to be 'failed'").
- api, commit 1: eleven failures against `main`'s bounds (timeouts at 15 s,
  `500`, no cancellation); the silent open connection "Test timed out in
  12000ms"; the limiter's EXEC "expected 401 to be 503".
- api, commit 2: 19 failures against commit 1's routes.
- api, commit 3: 18 failures against commit 2's code; the race "expected 200
  to be 401" without the second read; against pass 1's version, "expected []
  to deeply equal [ 'cgsess:tok-013-victim' ]" and "expected 12026 to be less
  than 8000".
- api, commit 4: six failures against commit 3's code.
- api, commit 5: "createUserFromCli is not a function" against the old CLI.

CI: every push green, on the branches and on `main` (*Progress*).

Baselines at the close:

- Cargo, non-elevated: 126 passed / 7 ignored (no agent source changed).
- Cargo, elevated `--ignored`: 7 passed (S38 gate 4; not re-run).
- vitest `services/ingest`, unelevated: 72 files / 274 tests (269 passed / 4
  failed with exit 9 / 1 skipped).
- `services/api`: 43 files / 216 tests.
- `dashboard`: 3 files / 8 tests (no dashboard code changed).
- `infra/dev`: 6 tests of the compose check.

## What waits on Manuel

1. Your audit of the decisions delegated on 2026-10-15 (*Decisions*). The
   fourth review of the SPEC texts read CLAUDE.md's "Manuel's OK is an
   explicit yes in chat to a specific proposal" and the deployment
   contract's owner STOP against them: #74 and the auth surface's breaking
   changes rest on your phrase of 2026-10-15, as SPEC-009 Amendment
   2026-10-14 rested on that of 2026-10-14.
2. The threat model's Amendment 2026-10-15 (#55): the threat model is yours,
   delegated. Its fifteen risks are open, not accepted: accepting one, or
   changing a destination, is your call.
3. Your explicit "sí" on each later ADR text: with F's SPEC, the ADR-0001
   amendment (Part (b) §4) and the ADR-0016 amendment (§3); ADR-0021's for
   #73; and, if the CA is to leave ingest (#99), a new ADR.
4. Optional: compacting the Docker WSL disk. `C:` has 15 GB free.
5. If you want §B2 back: a SPEC-021 text from outside Claude Code.

## Debts

Closed this session:

- **#68** — no workflow checked the dev compose: `infra-ci` (`524cb5a`).
- **#6** — the api's copy of the `cges_events` DDL (`b10bc3d`).
- **#3** — hand-numbered ids across the ingest tests (`ecd0fd4`).
- **#76** — SPEC-022's two untested statements (`a8deda8`).
- **#75** — nodemailer's private fields on an unpinned release (`b560665`).
- **#74** — SMTP credentials in clear text without TLS: decided under
  delegation, credentials only over TLS (`0e398e6`).
- **#69** and **#71** — Redis and Postgres unbounded in the api (`aa70bb4`).
- **#72** — an unknown role answered `500`; with it SPEC-008's bodies
  (`0c3ca78`).
- **#81** — the TOTP confirmation defeated the second factor (`ee5235a`),
  opened and closed this session.

Open debts at the close: 67. That is handoff-38's 52, less the nine closed
from before (#3, #6, #68, #69, #71, #72, #74, #75, #76), plus the twenty-five
opened (#77–#101), less #81, closed.

New this session:

- **#77 — The bootstrap CLI takes the password on its command line.**
  `services/api/src/cli/create-user.ts` reads `--password`, so the first
  admin's password is visible in the process list while the CLI runs and
  stays in the shell's history. Destination: roadmap §F's SPEC, which
  decides the installation and its bootstrap: the password read from
  standard input or a prompt.
- **#78 — No operator setting trusts a relay's private certificate
  authority.** The bounded send verifies the relay's certificate against
  Node's roots (SPEC-022 Amendment 2026-10-15). Only Node's
  `NODE_EXTRA_CA_CERTS` environment variable adds a private root, and a
  certificate the send cannot verify reads `send failed: ESOCKET`, the code
  of a reset too, which does not point the operator to the remedy.
  Destination: roadmap §F's SPEC, which decides the operator's
  configuration surface.
- **#79 — Behind a reverse proxy, the api reads the proxy's address.**
  Fastify's `trustProxy` is unset (`services/api/src/app.ts`), so `req.ip`
  is the proxy's. The per-IP login limit then counts every user together,
  and the audit rows' `source_ip` names the proxy. TLS termination is a
  deployment concern (SPEC-008 Amendment 2026-09-23), so a proxy is to be
  expected. Destination: roadmap §F's SPEC, which decides the proxy and the
  address it forwards.
- **#80 — A denied request writes no audit row.** A missing session
  (`401`) and an RBAC or CSRF denial (`403`) in
  `services/api/src/auth/prehandlers.ts` are not recorded, so an attempt
  to use an admin route without the role leaves no trace (threat model
  § cg-api, Repudiation). Destination: a later sweep, with SPEC-008.
- **#81 — `POST /v1/auth/totp/confirm` defeats the second factor.** It
  needs no session (SPEC-008 §Operational §8), and
  `confirmTotpEnrollment` (`services/api/src/auth/service.ts`) takes any
  user id, enrolled or not, with no rate limit, and marks no code as used.
  Someone who holds a user's password and user id can try codes there
  without limit, then log in with the code that matched within its window. Surfaced while
  writing SPEC-008 Amendment 2026-10-15 and widened by its second review.
  Destination: that amendment's item 12, implemented in this sweep.
- **#82 — A session lasts 8 h from its login, not 8 h idle within a
  24 h cap.** `services/api/src/auth/session.ts` sets a fixed time-to-live
  and never extends it, while NFR-008-002 asks for a sliding idle
  time-to-live with an absolute cap. The fixed 8 h is the stricter
  lifetime, and it logs an active user out after 8 h. Destination: a later
  sweep, SPEC-008's NFR or the code.
- **#83 — The dashboard cannot log a user in.** Its login page is a stub:
  `onSubmit` only prevents the form's default (`dashboard/app/login/page.tsx`,
  "GREEN gate: POST to the dashboard-origin auth proxy, then redirect to
  /incidents"), and no auth proxy route exists under `dashboard/app/`. The
  incident pages forward a `cgsess` cookie, which only a login through the
  api at the dashboard's host could set. So the product's UI is not usable
  without setting the cookie by hand, although SPEC-008's login works in the
  api. Nor does the dashboard offer the TOTP enrollment, a logout or user
  management. Surfaced by the review of SPEC-008 Amendment 2026-10-15.
  Destination: before or with roadmap §F, since an installation that ends
  without a working login does not meet criterion 7.
- **#84 — Six places cite ADR-0002 Rule 4 for Zod schemas.**
  SPEC-008's header, §Operational §1 and references
  (`docs/specs/SPEC-008-auth-core.md:6`, `:121` and the ADR-0002 reference),
  SPEC-009's data contracts (`docs/specs/SPEC-009-read-slice.md:43`, for
  the responses) and the threat
  model's § cg-api, Tampering (`docs/security/threat-model.md:82`) and its
  References entry for ADR-0002. ADR-0002's Rule 4 makes TypeScript
  the default for user-facing layers (`docs/adr/0002-language-per-component.md:59`),
  and the ADR locks no framework (`:134`). Surfaced by the reviews of
  SPEC-008 Amendment 2026-10-15, whose *Effect* names the right authority.
  It is a wrong authority, not a moved line. The threat model's line is
  corrected by its Amendment 2026-10-15. Destination: the SPEC texts, in a
  later SPEC pass.
- **#85 — A compromised ingest can forge an api session.** The api's
  sessions are Redis records holding the role
  (`services/api/src/auth/session.ts`), and ingest, the component exposed to
  agents, uses the same Redis credential (`infra/dev/docker-compose.dev.yml`).
  Threat model R2. Destination: roadmap §F, a Redis user per service with
  key-pattern ACLs.
- **#86 — Both services act as the Postgres superuser.** Either can read or
  rewrite `users`, `audit_log`, `ca` and `forensic_key`. Threat model R3.
  Destination: roadmap §F, a Postgres role per service with the grants it
  needs.
- **#87 — The agent's private key is readable by the enrolling user's
  unelevated processes.** DPAPI at machine scope, and an ACL that grants the
  enrolling user and SYSTEM (`agent/cg-agent/src/secure_storage.rs`,
  `src/identity.rs`); the files are written before their ACL is tightened.
  Threat model R1. Destination: a SPEC-002 amendment (its NFR-003 makes the
  key owner-only, and the owner is the enrolling user), with the next agent
  change under the elevated gate.
- **#88 — Agent certificates expire at 90 days, with no renewal and no
  revocation list.** `services/ingest/src/cert.ts` issues 90-day
  certificates and nothing renews them, so every agent stops being accepted
  90 days after its enrollment; a stolen certificate is cut off only by
  deleting its agent's row. The ingest server's own certificate is also
  issued for 90 days, and `ensureServerCert` reuses it from disk at every
  start without looking at its expiry, so once it expires every agent's
  mTLS connection fails at once. Threat model R5. Destination: the next
  sweep, first: the server certificate reissued at start before it expires
  (a SPEC-004 amendment), then the agents' renewal before roadmap §F's
  installation is called done.
- **#89 — Enrollment hardening.** The token travels in clear text over the
  plain-HTTP enrollment listener (accepted by SPEC-004) and is stored as
  typed; its `scope` and `org_id` are not checked; the route has no rate
  limit, and the CA signs a certificate for every request before the token
  is checked (`services/ingest/src/routes/enroll.ts`). Threat model R4.
  Destination: roadmap §F (TLS on the listener) and a SPEC-004 amendment.
- **#90 — The forensic export and report: any role, no audit row.** The
  export signs what ClickHouse returns at the request, which the shared
  ClickHouse credential can write (`services/api/src/forensic/export.ts`).
  Threat model R7. Destination: a later sweep, an audit row per export and
  report and a role check (SPEC-009's matrix).
- **#91 — Detection rules are re-read from disk on every cycle.**
  `services/ingest/src/detect/index.ts` loads `INGEST_DETECT_RULES_DIR` each
  cycle, so a write to the directory changes detection within one cycle, and
  a file that fails to load stops detection with a log line. Threat model R8.
  Destination: a later sweep: rules loaded at boot, or a failed load raised
  as an alert.
- **#92 — The containers run as root, and ingest writes its TLS key with the
  default file mode.** No `USER` in the ingest and api Dockerfiles;
  `services/ingest/src/cert.ts`. Threat model R15. Destination: roadmap §F.
- **#93 — No dependency audit in CI.** No `cargo audit` or `pnpm audit` step in
  `.github/workflows/`, and the dependency runbook the threat model names
  does not exist. Threat model R12. Destination: a later sweep.
- **#94 — Encryption passphrases travel as SQL parameters.** pgcrypto is
  called with the passphrase as a parameter (`services/ingest/src/ca.ts`,
  `services/api/src/auth/service.ts`, `services/api/src/cli/create-user.ts`,
  `services/api/src/forensic/key.ts`),
  so a Postgres that logs statements with their parameters records them.
  Threat model R10. Destination: roadmap §F's deployment of Postgres
  logging.
- **#95 — Logon telemetry is personal data with no asset entry.** Class 3002
  rows hold user names, domains, source addresses and host names for 365
  days, readable with the api's ClickHouse credential. Threat model R11.
  Destination: roadmap §F's per-service grants, and the DPO's record of
  processing.
- **#96 — The dev stack runs NATS without authentication and MinIO with the
  compose file's dev root credentials**, and nothing uses either
  (`infra/dev/docker-compose.dev.yml`). Destination: a later sweep, removing
  them until a SPEC uses them.
- **#97 — The dashboard forwards the browser's whole cookie header to the api
  over HTTP** (`dashboard/app/incidents/page.tsx`,
  `dashboard/src/lib/api/client.ts`). Threat model R13. Destination: with
  debt #83.
- **#98 — Admins set users' passwords and receive their TOTP secret.**
  `POST /v1/users` takes the password and returns the provisioning URI.
  Threat model R14. Destination: a later SPEC-008 change, an invitation the
  user completes.
- **#99 — One process holds the internet-facing listeners and the CA's
  private key.** A parser defect in ingest reaches the key that issues agent
  identities. Threat model R6. Destination: an ADR on separating the CA, a
  new component, for Manuel's "sí".
- **#100 — The password change counts no failure and leaves its TOTP code
  usable.** `POST /v1/auth/password` verifies the current password and a TOTP
  code with no rate limit, writes no row for a failure, and does not mark
  the code used, so with a session the code can still open another session
  within its window. Surfaced by the second review of the api's sweep.
  Destination: a later SPEC-008 change, with #80.
- **#101 — The login limiter reads its counters, then counts the failure.**
  `isAllowed` reads both counters and `recordFailure` increments them later
  (`services/api/src/auth/ratelimit.ts`), so a burst of parallel attempts
  passes the threshold before any count lands; the TOTP confirmation shares
  the pattern, and a confirmation costs no Argon2. Surfaced by the third
  review of the api's sweep. Destination: a later sweep, an atomic
  increment-then-check.

## How Session 40 resumes

1. *Session protocol* §At the start. Expected:
   - `main` = `origin/main` at this file's last commit; tree clean; no stash;
   - no branch or PR open;
   - catalogs ADR 21 / SPEC 21; Known CI debt zero;
   - `logman query -ets` lists no `CGAgent` session.
2. **#88's server certificate, first.** `ensureServerCert` reuses the ingest
   server's 90-day certificate from disk without looking at its expiry, so
   every agent's connection fails once it expires. A SPEC-004 amendment
   (reissue at start before it expires), the code on a branch, the
   elevated gate: the heartbeat's TLS is the capture path's.
3. The next sweep, in order of value: #83 (the dashboard's login, with
   #97), #90 (the forensic export and report audited and role-checked), #80
   and #100 (denials and the password change audited and limited), #101,
   #91, #93, #96, #82, #31 (an event's time bounded against its arrival,
   threat model R9), #13 (stale citations, swept by pattern, with #84's SPEC
   texts), then #30, #39, #40, #27, #8, #9, #11.
4. Roadmap §F, criterion 7: its SPEC decides Part (b) §2–§4 (delegated) and
   carries #85, #86, #89, #92, #94, #95, #77, #78 and #79 as the deployment's
   items. The ADR amendments go to Manuel for his "sí", with ADR-0021's for
   #73, and an ADR for #99 if the CA is to leave ingest.
5. #87 (the agent's key ACL) goes with the next agent change under the
   elevated gate.
6. §B2 and §B3 stay parked until a SPEC-021 text arrives (roadmap §B2).
