# Handoff — Session 93

Session 93, 2026-11-22, built the api's half of F3's phase 1, the accounts (SPEC-026): migration
`0008_dashboard`, the invitations, the users' creation by invitation, the reset and the users'
lists, with dashapi_ac_001 begun and dashapi_ac_004 to 006 landed. It drafted, on an owner-review
branch, the threat model's amendment for R5 that Session 92's handoff left waiting. The phase's
pages, migration `0011_dashboard_rights` and the detection change did not start: the pages need
a design pass of their own (a QR code library, the mutation handlers), and they move to Session 94
(*How Session 94 resumes*). No tag was pushed; the release freeze stays.

## Start and resulting state

- Start: `main` = `origin/main` = `96a2b46` (Session 92's close), clean, no stash, no other
  branch, no open PR. The tip's CI was ALL GREEN (`markdown-lint`). Counts: 22 ADRs / 25 SPECs,
  55 open debts, no Known CI debt. The repo agreed with Session 92's handoff. 23 GB free on `C:`.
- Landed on `main`, by fast-forward to the SHA each PR's CI ran, one commit per push:
  - PR #157: `5c5f404` (migration `0008_dashboard`), `1bb0aae` (its review fixes);
  - PR #158: `60af68d` (the invitations), `9144889` (the reset), `105f848` (the second review's
    fixes), `5b5e002` (the users' lists), `5fd0cf2` (the threat model's lines the code made true),
    `28ae7e5` (the third review's fixes), `cbf259e` (the final review's one item).
- Resulting `main`: those commits and this close's.
- Branches: `main`, and `docs/tm-r5-revocation-built` (`57d7d75`, owner review, pushed, no PR),
  which waits for Manuel (*Waiting on Manuel*). PRs #157 and #158 closed, their branches deleted.
- Catalogs: 22 ADRs / 25 SPECs, unchanged.

## What landed

- **Migration `0008_dashboard`** (`services/api/src/db/migrations/0008_dashboard.ts`, SPEC-026
  §Data contracts §2): `users.password_hash` and `totp_secret` nullable, both or neither, a
  confirmed factor needing a secret; `invitations`, with its 72 hours, one end, a unique hash and
  one open invitation per user; an index on `audit_log (org_id, id)`; `cg_api`'s `SELECT` on
  `audit_log` and `SELECT`, the `INSERT` of its columns and the `UPDATE` of the secret and the end
  on `invitations`, no `DELETE`. Run again over its own schema it changes nothing.
- **A user with no credentials** signs in nowhere: the login does the unknown address's work, the
  decrypt and the fixed hash; the TOTP confirmation refuses them as an unknown user. The last
  active admin counts only an admin with a password and a confirmed factor.
- **The invitations** (`services/api/src/auth/service.ts`, `routes.ts`):
  - `POST /v1/users` takes `{ email, role }` and answers `201 { user_id, invitation: { token,
    expires_at } }`, the token 32 random bytes in base64url, kept as its SHA-256 alone;
  - `POST /v1/users/:id/invitation` (a user with no credentials; `409 user_has_credentials`,
    `409 user_disabled`) and `/invitation/withdraw` (`409 no_open_invitation`); a disable
    withdraws the open one; a new one clears every earlier pending secret;
  - `POST /v1/invitations/totp` and `/complete`, no session: a new secret at each call, kept
    encrypted on the invitation; the completion sets the password and the factor from the last
    one, marks its code used and raises the authority version; both limited per invitation
    (`rl:inv:<sha256>`) and per source address, one answer `401 invitation_rejected`, a text that
    is not base64url refused as unknown;
  - the rows `user.create`, `invitation.issue`, `invitation.withdraw`, `invitation.totp`,
    `invitation.complete` (with `denied`).
- **The reset**, `POST /v1/users/:id/reset`: the credentials cleared, the authority raised, the
  open invitation withdrawn, a `reset` one issued, `user.reset`, the sessions deleted after the
  commit; `409 cannot_reset_self`, `409 user_disabled`.
- **The users' lists**: `GET /v1/users` paged `{ items, next_cursor }` in `(created_at, user_id)`
  order with `totp_enrolled` and the open invitation; `GET /v1/users/assignable`, admin or analyst,
  in `(email, user_id)` order (`services/api/src/auth/users-list.ts`).
- **`no-store`** on every route of SPEC-026 §Data contracts §3, its refusals included (an `onSend`
  hook and a route's `config.noStore`), and on the TOTP confirmation, which §3 names.
- **The host's command** keeps a body of its own with a password; its header says the users after
  the first are created with an invitation (#136).
- **The threat model's lines** of the Amendment 2026-10-15 that the Amendment 2026-11-20 (SPEC-026)
  updates in place by the change that lands the code (`5fd0cf2`): the rate limits, `audit_log`'s
  grants and failures, the users' list, the four secrets under pgcrypto, and the login's line,
  which Session 92's `d259b31` made true and did not update.

## Decisions

### Manuel's

None this session.

### Anticipated (Claude Code)

- **Four slices, two PRs.** Migration `0008` alone first (PR #157), then the invitations, the reset
  and the lists on one branch (PR #158), one commit per push. Discarded: a PR per slice, which
  serialises a landing and a `main` CI run per slice for no extra coverage.
- **The invitation code inside the auth service**, sharing its transaction and authority-change
  helpers. Discarded: a module of its own, which would have needed those helpers exported.
- **Error codes on the error types** (`AuthError` and `ConflictError` carry the body's `error`).
  Discarded: one error class per code.
- **`no-store` by route configuration and an `onSend` hook**, so that a refusal answered by a hook
  or the error handler carries it. Discarded: a header set in each handler, which misses them.
- **Lock order users, then invitations**, in every writer of both. Discarded: locking through a
  join, whose order Postgres does not fix.
- **The invitation's counter keyed by the hex of the token's SHA-256** under `rl:`, never the
  token.
- **A refused request for a secret writes no row**: SPEC-026 §Data contracts §4 lists
  `invitation.totp` for a secret shown; only the completion writes `denied`.
- **A new invitation for a user with no credentials is an `invite`**; the reset's is a `reset`.
- **The completion copies the pending ciphertext into `users.totp_secret`.** Discarded: a decrypt
  and a new encryption in the api's process.
- **The users' list's cursor holds the time as microseconds**, exact over Postgres's time.
  Discarded: an ISO time, which JavaScript holds to the millisecond.
- **The last active admin**: an admin with a password and `totp_enrolled`; an unconfirmed admin
  with a password does not count either.
- **The TOTP confirmation answers `no-store`**, the logout does not: §3 names the first in its
  opening sentence and the logout in no table.
- **The threat model's in-place lines** executed with the code (`5fd0cf2`), the login's line
  included. The lines that wait for the pages, the audit view and the triage history stand.
- **R5's amendment drafted by a fork in an isolated worktree**, reviewed by two text rounds.
- **The pages moved to Session 94** (`feedback-checkpoint-over-rushing`): they need a third-party
  QR library chosen and recorded, mutation handlers generalised from the login's, and three pages
  with dash_ac_009 to 012, which this session could not give a full review.

### Reactive corrections

- **The first baseline run was not a baseline**: it ran while `0008` was being written, and
  counted 3 failures that the new migration caused. The suite was run again on the finished tree.
- **`migrate-lock-wait.test.ts`** assumed the last three migrations were 0005 to 0007, runnable
  again; with `0008` its ledger check failed. `0008` drops its constraints if they exist before
  adding them, and the test's pending set gained it.
- **dashapi_ac_001's password route** answers 401 to the test's wrong current password: its
  allowed roles are shown past the role check by their `user.password.change` denied row.
- **The tests of the old `POST /v1/users`** (apiauth_ac_010, 011, auth_ac_003, 006, 008) moved to
  the invitation: the password policy is tested at the completion, auth_ac_008 on the completed
  user's row.
- **A seeded alert needs a source event** (`alerts_source_events_nonempty`).
- The review rounds' fixes, below.

## The reviews (CLAUDE.md compensating control 5)

Every reviewer and verifier ran on Sonnet, the reviewers in isolated worktrees, with the fixed
prompts; one verifier per item.

### Migration `0008` (PR #157)

| Criterion | Test | Verdict |
|---|---|---|
| §2 users' credentials, both or neither | `services/api/test/migration-0008-dashboard.test.ts` | covered |
| §2 invitations' constraints and open-invitation index | same file | covered |
| §2 `audit_log` index | same file | covered |
| deploy_ac_004, invitations and `audit_log` rows | `services/api/test/deploy-ac-004-005-postgres.test.ts` | covered |
| §2 a user with no credentials signs in nowhere | `services/api/test/dashapi-credentialless-user.test.ts` | covered |
| §3 the TOTP confirmation refuses them as unknown | same file | weak, fixed (`1bb0aae`) |
| §2 the last active admin | `migration-0008-dashboard.test.ts` | covered |

- Red first: without the migration the structural tests failed on their inserts; with it and the
  old service, the login verified a `null` hash ("expected false to be true"), made no decrypt
  ("expected +0 to be 1"), and a demotion answered 200, not 409.
- Two code reviewers: holding after verification, the confirmation's test (weak) and the grant
  branch of `0008` never run twice; both fixed in `1bb0aae`, the first probed against the old
  service and red.

### The invitations, the reset and the lists (PR #158)

| Criterion | Test | Verdict |
|---|---|---|
| dashapi_ac_004 | `services/api/test/dashapi-ac-004-invitation.test.ts` | covered after fixes |
| dashapi_ac_005 | `services/api/test/dashapi-ac-005-reset.test.ts` | covered |
| dashapi_ac_006 | `services/api/test/dashapi-ac-006-users-lists.test.ts` | covered |
| dashapi_ac_001 (the routes that exist) | `services/api/test/dashapi-ac-001-roles.test.ts` | covered after fixes |

- Red first: the new routes answered 404, the §3 routes carried no `no-store`, the list answered
  the old array; the reset's own 404 case passed without its route, by the route's absence.
- Round 2 (`60af68d`, two reviewers): no code defect; test gaps fixed in `105f848` — every refusal
  on both routes with its denied row, the completion's code refused at a login, a user of
  another org, the per-address limit, the exact answer of each allowed role, the org's users in
  the refusal's snapshot, `no-store` on the routes that take no session (probed red by removing
  it from the TOTP confirmation), which also gained it.
- Round 3 (`1bb0aae..5b5e002`, two reviewers): a text that is not base64url could fold onto an
  invitation under the hash's ASCII encoding (no limit bypassed, the token's entropy unchanged),
  fixed and probed red; the password hashed before the code is marked used; the read routes with
  an `:id` answering 200 on a seeded incident; two more routes checked for a secret (`28ae7e5`).
- Round 4, the fresh pass after the security fixes (`1bb0aae..28ae7e5`, two reviewers): one clean,
  one item, the order of checks untested on the three new routes, fixed in `cbf259e`.
- Not changed, recorded: the completion holds the user's and the invitation's row locks during the
  Argon2 hash (per invitation and per user, behind its limit); the regex check for a secret in the
  admin's answers matches the word, not every encoding of one.

### R5's amendment (a text, branch `docs/tm-r5-revocation-built`)

- Round 1 (`738c229`): List A, one item, holding: R5's "neither running service has the right to
  write" lacked "in the bundle" (the dev stack keeps the superuser, R3). Fixed in `57d7d75` with
  two List B items: the grant's lines `:178-185`, and Manuel's choice cited to SPEC-026's
  Amendment 2026-11-22.
- Round 2 (`57d7d75`): List A empty.

## Gates and CI

- Local, on each branch's tip (`services/api`, then the *bundle and runbooks* bullet):
  - typecheck `0`, lint `0`, build `0`, `entrypoints ok`;
  - `5c5f404`: "Test Files 60 passed (60)" / "Tests 406 passed (406)";
  - `cbf259e`: "Test Files 64 passed (64)" / "Tests 512 passed (512)";
  - `licences admitted: 431 packages`; deploy tests 64, 60 pass, 0 fail, 4 skipped;
  - markdownlint on `docs/security/threat-model.md`, its exit read alone: `0`.
- No elevated gate: nothing changed on its paths.
- CI, each push, the REST fallback:

```text
SHA 5c5f404 (PR #157)
  ts-ci -> success
  deploy-ci -> success
Verdict: ALL GREEN
SHA 1bb0aae (PR #157)
  ts-ci -> success
  deploy-ci -> success
Verdict: ALL GREEN
SHA 1bb0aae (push, main)
  ts-ci -> success
Verdict: ALL GREEN
SHA 60af68d (PR #158)
  ts-ci -> success
  deploy-ci -> success
Verdict: ALL GREEN
SHA 9144889 (PR #158)
  deploy-ci -> success
  ts-ci -> success
Verdict: ALL GREEN
SHA 105f848 (PR #158)
  ts-ci -> success
  deploy-ci -> success
Verdict: ALL GREEN
SHA 5b5e002 (PR #158)
  ts-ci -> success
  deploy-ci -> success
Verdict: ALL GREEN
SHA 5fd0cf2 (PR #158)
  markdown-lint -> success
  ts-ci -> success
  deploy-ci -> success
Verdict: ALL GREEN
SHA 28ae7e5 (PR #158)
  markdown-lint -> success
  ts-ci -> success
  deploy-ci -> success
Verdict: ALL GREEN
SHA cbf259e (PR #158)
  markdown-lint -> success
  ts-ci -> success
  deploy-ci -> success
Verdict: ALL GREEN
SHA 60af68d (push, main)
  ts-ci -> success
Verdict: ALL GREEN
SHA 9144889 (push, main)
  ts-ci -> success
Verdict: ALL GREEN
SHA 105f848 (push, main)
  ts-ci -> success
Verdict: ALL GREEN
SHA 5b5e002 (push, main)
  ts-ci -> success
Verdict: ALL GREEN
SHA 5fd0cf2 (push, main)
  markdown-lint -> success
Verdict: ALL GREEN
SHA 28ae7e5 (push, main)
  ts-ci -> success
Verdict: ALL GREEN
SHA cbf259e (push, main)
  ts-ci -> success
Verdict: ALL GREEN
```

- `docs/tm-r5-revocation-built` was pushed with no PR open: no workflow ran.
- This close's push: CI on its SHA (`event=push`), reported in the turn.

## Test baselines

- `services/api`: 512 tests, 64 files (395 at Session 92's close).
- `services/ingest`: unchanged from Session 91 (unelevated 560: 553 passed, 4 failed, 3 skipped).

## Debts

- Closed: **#136**, by `60af68d`: the command's header says the users after the first are created
  with an invitation.
- Destinations changed: **#98**, its api half landed (`60af68d`, `9144889`); the user pages remain.
- Opened: none.
- Open at close: **54**.

## Waiting on Manuel

- Unchanged since Session 87's close: the Authenticode decision and its purchase; #130's legal
  readings; the commercial licence and terms; `SECURITY.md`'s contact; the isolated machines
  (M3); a penetration test; #99, #124 and #125.
- **R5's amendment**, on `docs/tm-r5-revocation-built` (`57d7d75`), to read at
  <https://github.com/AlmanInDaHouse/CyberGuard_Enterprise/compare/main...docs/tm-r5-revocation-built>:
  R5 updated in place to what is built (the revocation enforced by the server, nothing that writes
  one until F3's agents route, no renewal until SPEC-025's agent half), R5 kept open until both.
  It also carries two lines the ratified Amendment 2026-11-20 (SPEC-025) updates in place with the
  server half, which Session 91 did not update: the token at `:293` and R4's closing. Those two
  execute ratified text; if Manuel prefers, they land apart from R5's change.

## How Session 94 resumes

1. Run the start protocol:
   - `main` is this close's commit; the branches are `main` and `docs/tm-r5-revocation-built`
     (owner review, unless Manuel has answered); no PR;
   - 54 open debts; 22 ADRs / 25 SPECs.
2. If Manuel has given his OK to R5's amendment, land it as *Integration path* §3 and §4 say: the
   branch rebased onto `main` (`threat-model.md` changed on `main` in `5fd0cf2`, other lines),
   the `NOT YET RATIFIED` markers stripped from the subjects, its Status recording his words and
   date, markdownlint, one commit per push. If he wants `:293` and R4 apart, they land first as an
   execution of the Amendment 2026-11-20 (SPEC-025).
3. **F3, phase 1's pages** (SPEC-026 §Operational §9, item 1), through a branch and a PR, each part
   with its tests seen red first (`dashboard/test/`, vitest):
   1. **The QR code**: choose a library that writes an SVG with no dependency of its own and no
      install script; read its `package.json` and its licence; record its name, version, licence
      and reason in the handoff (CLAUDE.md *Decision authority*: not heavyweight). The lockfile
      changes, so the gate adds `pnpm install --frozen-lockfile`, `pnpm audit --prod
      --audit-level=high`, `pnpm audit --audit-level=critical` and the licence check.
   2. **The mutation handlers** (dash_ac_009), generalised from `dashboard/src/lib/auth/handlers.ts`:
      the same-origin check, a 16 KiB body, the `__Host-cgcsrf` cookie sent as `x-csrf-token`, the
      session cookie as the api's `cgsess`, `X-Forwarded-For` passed through, `no-store`; the
      invitation's two handlers send neither session nor CSRF token. Routes under `app/api/`:
      `account/password`, `users`, `users/[id]/role`, `/status`, `/invitation`,
      `/invitation/withdraw`, `/reset`, `invitations/totp`, `invitations/complete`.
   3. **The navigation**, shared by every page, its entries by the role `GET /v1/auth/session`
      answers (dash_ac_010's part: no users entry but for an admin).
   4. **`/invitation`** (dash_ac_011): the token read from the fragment and removed from the
      address bar, the provisioning URI as a QR code and as text, the completion; no request
      carries the token in a URL.
   5. **`/account`**: the password change, then both cookies cleared and `/login`.
   6. **`/users`**, for admins: the list with its pages, a new user with the invitation's link
      (`<origin>/invitation#<token>`) shown once (dash_ac_012's part), a new invitation, its
      withdrawal, a reset, the role and the status.
   7. With the landing commit, the threat model's lines that wait for the pages (§9, `:310`, the
      Amendment 2026-11-20 (SPEC-026)) and R14's closing; #83 and #98 close.
   - Self-review: the fixed code prompt on Sonnet, two reviewers, a verifier per item, and a fresh
     pass after any security fix.
4. Then ingest's `0011_dashboard_rights` with the detection change (incident_ac_006 and 007, under
   the elevated gate, CLAUDE.md compensating control 4), with deploy_ac_004's remaining rows
   (`enrollment_tokens`, `agents`, `incidents`, `alerts`, `triage_history`, `ca`, and
   `cg_ingest`'s narrowed `UPDATE`); then the tokens, agents and revocation routes with the
   revocation's runbook (SPEC-026 Amendment 2026-11-22); then the triage and the audit view; then
   **F4's agent half** after its SPEC-024 amendment.
