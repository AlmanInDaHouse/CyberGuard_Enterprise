# Handoff — Session 39

Session 39 opened on 2026-10-15. This record was started early because the
SPEC amendments of its first sweep cite the debts it records. Its work order
for Session 40 is *How Session 40 resumes* at the end, written at the close.

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

## What waits on Manuel

1. Your audit of the decisions delegated on 2026-10-15 (*Decisions*). The
   fourth review of the SPEC texts read CLAUDE.md's "Manuel's OK is an
   explicit yes in chat to a specific proposal" and the deployment
   contract's owner STOP against them: #74 and the auth surface's breaking
   changes rest on your phrase of 2026-10-15, as SPEC-009 Amendment
   2026-10-14 rested on that of 2026-10-14.

## Debts

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
- **#84 — Five places cite ADR-0002 Rule 4 for Zod schemas.**
  SPEC-008's header, §Operational §1 and references
  (`docs/specs/SPEC-008-auth-core.md:6`, `:121` and the ADR-0002 reference),
  SPEC-009's data contracts (`docs/specs/SPEC-009-read-slice.md:43`, for
  the responses) and the threat
  model's § cg-api, Tampering (`docs/security/threat-model.md:82`). ADR-0002's Rule 4 makes TypeScript
  the default for user-facing layers (`docs/adr/0002-language-per-component.md:59`),
  and the ADR locks no framework (`:134`). Surfaced by the reviews of
  SPEC-008 Amendment 2026-10-15, whose *Effect* names the right authority.
  It is a wrong authority, not a moved line. Destination: the SPEC texts in
  a later SPEC pass; the threat model's line with #55.
