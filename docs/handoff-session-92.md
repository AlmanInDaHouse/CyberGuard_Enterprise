# Handoff — Session 92

Session 92, 2026-11-22, landed the runbooks' change that SPEC-025's *Out of scope* lists,
through PR #155, and began F3 (SPEC-026) with its smallest slice, the session route and the
login that ends its cookie's session (#102), through PR #156. With them landed SPEC-026's
Amendment 2026-11-22, which records where the revocation's runbook goes. No tag was pushed;
the release freeze stays.

## Start and resulting state

- Start: `main` = `origin/main` = `36d0ae8` (Session 91's close), clean, no stash, no other
  branch, no open PR. The tip's CI was ALL GREEN (`markdown-lint`). Counts: 22 ADRs / 25 SPECs,
  55 open debts, no Known CI debt. The repo agreed with Session 91's handoff. 24 GB free on `C:`.
- Landed on `main`, one commit per push, in this order:
  - `faae8e5`: the runbooks (`docs/operations/agent-identity.md`, new; `install.md`,
    `backup-and-restore.md`, `data-protection.md`, `README.md`; deploy-ci's pinned sentences
    and its documents' list);
  - `e5e207d`: the first review round's fixes;
  - `d259b31`: the session route and the login's end of the cookie's session;
  - `61f1339`: SPEC-026 Amendment 2026-11-22 and its catalog row (docs, straight to `main`,
    rebased onto `d259b31` with its patch-id unchanged).
- The runbooks' two commits and the F3 commit reached `main` by fast-forward to the SHAs their
  PRs' CI ran; `origin/main` was each branch's base before its first push.
- Resulting `main`: those commits and this close's. Branches: `main` alone (PRs #155 and #156
  closed, their branches deleted, local and on origin).
- Catalogs: 22 ADRs / 25 SPECs (SPEC-026's row gains its amendment).

## What landed

- **`docs/operations/agent-identity.md`**, a new runbook for the operator:
  - what the server holds of an agent;
  - the refusals ingest logs, and a command that prints them;
  - the revocation, which this release cannot make (below);
  - why nothing undoes a revocation: the endpoint is purged and enrolled anew;
  - a copied identity: its two signals, its flag and its remedy;
  - a restore's effect on the agents (SPEC-025 §Operational §7);
  - the compromise of ingest's CA, whose remedy is a new installation with every endpoint
    enrolled anew.
- **`install.md`**:
  - the token's identifier on standard error, and the token kept as its SHA-256 alone;
  - the enrollment limit, and its code 3 with status 429;
  - a copy and a purged agent accepted until revoked;
  - status 403.
- **`backup-and-restore.md`**: agents renewed after the backup, and revocations to be made again.
- **`data-protection.md`**:
  - the host name, platform and version on the agent's row (`:31`);
  - the agents' and tokens' row (`:77`): the token as its hash, the token that enrolled each
    agent, the revocation and its reason, the copy flag;
  - the enrollment limit's addresses in ingest's memory, held in its request lines too;
  - `cg_ingest` unable to write a revocation;
  - ingest's new log lines.
- **The operations README** lists the new document. **deploy-ci**: `install.test.mjs` and
  `upgrade.test.mjs` pin the two new sentences, which replace "until it expires or its row is
  deleted", and `operations.test.mjs` checks the new document's links.
- **SPEC-026 Amendment 2026-11-22**: its runbooks item also covers the revocation's steps and
  the copy flag in `agent-identity.md`.
- **F3's first slice** (`services/api/src/auth/routes.ts`, `services/api/src/auth/service.ts`):
  - `GET /v1/auth/session`, the session's user's id, email and role, `no-store`;
  - a successful login deletes the session its `cgsess` cookie names, whatever user it belonged
    to, once every check has passed and its `auth.login.ok` row is written, before it answers;
  - the login answers `no-store`.

## Decisions

### Manuel's

Both on 2026-11-22, in chat, by choosing the recommended options:

- **The revocation before SPEC-026's route: "Sin SQL; F3 lo escribe (Recomendado)".** The
  runbooks give no step in the database. `agent-identity.md` says that this release has no
  command or route that revokes an agent, and the revocation's steps land with SPEC-026's route
  (its Amendment 2026-11-22). Discarded: `psql` as the superuser with psql variables, and a CLI
  `revoke-agent.js` run as the owner role. Neither writes an audit row, and both add a temporary
  operator surface.
- **The CA's compromise: "Instalación nueva (Recomendado)".** New secrets and a new CA, every
  endpoint purged and enrolled anew, the old installation kept stopped for the investigation, its
  backups not restored. The product's missing rotation of its CA is recorded as #173. Discarded:
  only the risk, with the procedure deferred to #99.

### Anticipated (Claude Code)

- **A runbook of its own, `agent-identity.md`**, for the revocation, its undoing, a copy, a
  restore's effect and the CA's compromise, which the other runbooks link to. Discarded:
  spreading them over `install.md` and `backup-and-restore.md`, which would have scattered one
  subject over two documents.
- **The token's identifier recorded in the change record**, with the endpoint it is for:
  the agent's row names its token. That gives F3's agents page an identifier the server chose,
  not a name the endpoint sent. Discarded: identifying an agent by its host name, which the
  endpoint chooses (`feedback-runbook-no-pasted-names`).
- **The log command greps fixed strings the runbook gives**, and asks the operator to paste
  nothing.
- **F3 begun with the session route and #102**, the slice that needs no migration and no elevated
  gate. Discarded: the invitations, the reset and migration `0008`, which are authentication code
  and are not rushed at a session's end (`feedback-checkpoint-over-rushing`).

### Reactive corrections

- **deploy-ci's pinned sentences**: `install.test.mjs:255` and `upgrade.test.mjs:155` asserted the
  sentences that SPEC-025 makes false, and failed locally on the first draft. They now pin the new
  sentences. The purge's sentence was reordered so that `upgrade.test.mjs:157`'s pinned citation of
  R5 stays whole.
- **The first review round** found one List A item, confirmed by its verifier and by all three
  text reviewers: `data-protection.md` said no log holds the addresses the enrollment limit tracks,
  while ingest's request lines hold them. Fixed in `e5e207d`, with List B items: every refusal
  reason, the `agent_id` once the request is read, `enroll rejected` in the command, the owner
  role named, the restore's revocation linked, and the agent's message for a 429.
- **The F3 review** found dashapi_ac_003 weak: only two of its refusals were tested. A disabled
  user's refusal, a check after the password, and a session naming no user were added. The new case
  was probed against a login that revokes first, and failed.

## The reviews (CLAUDE.md compensating control 5)

### The runbooks (a text)

- Claude Code's pass: each anchor the new links name resolves (deploy-ci's `deploy_ac_021`); each
  log line, reason, status, bound and grant was read against `services/ingest/src` and migration
  `0010`.
- **Round 1** (`faae8e5`): three Sonnet reviewers with the fixed text prompt, one with the fixed
  code prompt for the tests, each in an isolated worktree, and one adversarial verifier per
  blocking item (7 agents). List A: the one item above, holding. Not changed, from List B:
  - "characters" for code points;
  - "held, at the longest, until ingest restarts", which covers the sweep;
  - "this release's agent does not yet renew", true until SPEC-025's agent half lands.
- **Round 2** (`8ec54e4`, before its amend): two Sonnet reviewers, the fixed text prompt. List A
  empty in both. One List B item fixed before the amend that gave `e5e207d`: a store ingest cannot
  reach is logged as `backend unavailable`, not as `heartbeat rejected`. The rest were not changed:
  - R5 of `docs/security/threat-model.md:320` still says "no revocation list", while the server
    half's revocation landed in Session 91. The threat model is Manuel's (below).
  - `agent-identity.md`'s revocation sentences change with F3, as SPEC-026's amendment says.

### SPEC-026 Amendment 2026-11-22 (a text)

One Sonnet reviewer, the fixed text prompt: List A empty. Its List B: the handoff it cites did
not yet exist (this one), and the replaced sentences can be checked only when F3's runbooks land.

### F3's slice (code)

| Criterion | Test | Verdict |
|---|---|---|
| dashapi_ac_002 | `services/api/test/dashapi-ac-002-session.test.ts` (the three roles' id, email and role, `no-store`; `401` without a session, with an unknown one, and with a session naming no user) | covered |
| dashapi_ac_003 | `services/api/test/dashapi-ac-003-login-ends-session.test.ts` (another user's session deleted and a new one issued, the old refused next; a wrong password, a malformed body and a disabled user's correct credentials leave it) | covered |

- Red before green: against the tree without the code, `GET /v1/auth/session` answered `404`, and
  the cookie's session outlived the login ("expected true to be false"); the failed-login half
  passed, as the old behaviour keeps the session.
- Two Sonnet reviewers with the fixed code prompt, and a verifier per item: both criteria covered;
  one weak, fixed (above). Not changed, from List B:
  - a Redis failure on the deletion, after `auth.login.ok`, answers an error with the new session
    written; SPEC-026 does not say what happens then;
  - `no-store` on the `401`s of `requireSession` is the rest of §Data contracts §3's sentence, for
    dashapi_ac_001's slice.
  - the re-read's own refusal is not raced in a test: the disabled user's refusal pins the
    ordering.

## Gates and CI

- Local, on each branch's tip:
  - markdownlint on each changed Markdown file, its exit read alone: `0`;
  - the *bundle and runbooks* bullet: `licences admitted: 431 packages`; 64 tests, 60 pass,
    0 fail, 4 skipped;
  - `cargo test -p cg-setup --test install_ac_002_setup_runbook`: 2 passed;
  - `services/api`: typecheck `0`, lint `0`, build `0`, `entrypoints ok`, "Test Files 58 passed
    (58)" / "Tests 395 passed (395)" before the review's added cases, which pass
    ("Tests 4 passed (4)" for the two files).
- No elevated gate: nothing changed on its paths.
- CI, each push, the REST fallback:

```text
SHA faae8e5 (PR #155)
  deploy-ci → success
  markdown-lint → success
  rust-ci → success
Verdict: ALL GREEN
SHA e5e207d (PR #155)
  markdown-lint → success
  deploy-ci → success
  rust-ci → success
Verdict: ALL GREEN
SHA d259b31 (PR #156)
  ts-ci → success
  deploy-ci → success
Verdict: ALL GREEN
SHA faae8e5 (push, main)
  markdown-lint → success
  rust-ci → success
Verdict: ALL GREEN
SHA e5e207d (push, main)
  rust-ci → success
  markdown-lint → success
Verdict: ALL GREEN
SHA d259b31 (push, main)
  ts-ci → success
Verdict: ALL GREEN
SHA 61f1339 (push, main)
  markdown-lint → success
Verdict: ALL GREEN
```

- This close's push: CI on its SHA (`event=push`), reported in the turn.

## Test baselines

- `services/api`: 395 tests passed before the review's added cases, in the same files.
- `services/ingest`: unchanged from Session 91 (unelevated 560: 553 passed, 4 failed, 3 skipped).

## Debts

- Closed: **#102** (the api's login deletes no prior session), by `d259b31`.
- Opened: **#173**, the product cannot replace ingest's CA. Ingest generates the CA at its first
  start, a restore brings back the backed-up one, and no command replaces it. A compromise of its
  key is recovered only by a new installation, with every endpoint enrolled anew and the history
  left in the old installation (`docs/operations/agent-identity.md#the-compromise-of-ingests-ca`,
  Manuel's choice of 2026-11-22). Destination: with #99's ADR separating the CA, for Manuel's "sí".
- Destinations changed: none.
- Open at close: **55**.

## Waiting on Manuel

- Unchanged since Session 87's close:
  - the Authenticode decision and its purchase;
  - #130's legal readings;
  - the commercial licence and terms;
  - `SECURITY.md`'s contact;
  - the isolated machines (M3);
  - a penetration test;
  - #99, #124 and #125.
- New: **R5's text** (`docs/security/threat-model.md:320`). It still says that agents have no
  revocation and that a stolen key is cut off only by deleting its row. Since Session 91 the server
  refuses a revoked agent, and the route that revokes lands with F3. The threat model is on
  Manuel's list. Session 93 can propose its amendment on an owner-review branch, with F3's
  revocation or before it.

## How Session 93 resumes

1. Run the start protocol:
   - `main` is this close's commit, with no other branch and no PR;
   - 55 open debts; 22 ADRs / 25 SPECs.
2. **F3, phase 1 (accounts)**, each slice with its acceptance tests seen red first, through a
   branch and a PR:
   - the api's migration `0008_dashboard` (SPEC-026 §Data contracts §2) with deploy_ac_004's rows
     for `invitations` and `audit_log`;
   - the invitations' routes, the users' list and creation by invitation, the reset
     (dashapi_ac_004 to 006);
   - dashapi_ac_001 growing with each route;
   - then the phase's pages (dash_ac_009 to 012's parts).
3. Then ingest's `0011_dashboard_rights` with the detection change (incident_ac_006 and 007, under
   the elevated gate), the tokens, agents and revocation routes, with the revocation's runbook
   (SPEC-026 Amendment 2026-11-22) and R5's amendment for Manuel, then the triage and the audit
   view, then **F4's agent half** after its SPEC-024 amendment.
