# Handoff — Session 88

Session 88, 2026-11-20 to 2026-11-21, ran roadmap step 3's second half on the two owner-review
branches. It ran six more fresh-context review rounds over the F3/F4 ADR branch (rounds 4 to 9,
round 10 stopped unfinished), wrote SPEC-026 (F3) with its threat-model amendment on a branch
stacked on it, and ran four review rounds over SPEC-026 (round 5 stopped unfinished). Neither
branch reached a round with no List A: the session closed on the hours limit at Manuel's word
("95% de limite de horas, no da tiempo a seguir con los revisores"), so the compare was not given
for ratification and nothing landed from the branches. No code changed and no tag was pushed; the
release freeze stays.

## Start and resulting state

- Start: `main` = `origin/main` = `2491a23`, clean, no stash; one other branch,
  `docs/f3-f4-adr-amendments` at `3c38cf2`, no PR; the tip's CI ALL GREEN; 22 ADRs / 23 SPECs,
  56 open debts, no Known CI debt. The repo agreed with Session 87's handoff. 20 GB free on `C:`.
- Resulting `main`: this close's commit alone.
- Branches, both pushed with no PR (owner-review branches: a push with no PR runs no workflow),
  every commit marked NOT YET RATIFIED:
  - `docs/f3-f4-adr-amendments` at `4cb678a`: Session 87's six commits, rebased onto `2491a23` (`3316900` to `4d98f8f`),
    then the fixes of rounds 4 to 9 (`9e7cc4b`, `9b5d225`, `1c5e78c`, `92a4c4d`, `c26b5b5`,
    `4cb678a`). Compare:
    `https://github.com/AlmanInDaHouse/CyberGuard_Enterprise/compare/main...docs/f3-f4-adr-amendments`.
  - `docs/f3-spec-026` at `b61f5b6`, stacked on `4cb678a`: SPEC-026 with the threat model's
    Amendment 2026-11-20 (SPEC-026) and its catalog row (`84fe772`), then the fixes of its rounds
    1 to 4 (`41af070`, `51e2647`, `fb3aff0`, `b61f5b6`). Compare:
    `https://github.com/AlmanInDaHouse/CyberGuard_Enterprise/compare/docs/f3-f4-adr-amendments...docs/f3-spec-026`.
- Catalogs on `main`: 22 ADRs / 23 SPECs, unchanged; on the branches, SPEC-025 and SPEC-026
  `Proposed`.

## Decisions

### Manuel's

- **2026-11-20, round 5, the certificate a renewal records** ("Registrar al firmar
  (Recomendado)"): the renewal records the certificate it signs as the agent's `pending_cert_pem`,
  and only a request under that certificate, verified and accepted, promotes it; any other
  certificate is `403 agent_superseded`. The CA's key alone takes over no enrolled agent; a restore
  of the database taken before a renewal refuses that agent until it is enrolled again. Discarded:
  promoting any later certificate the CA signed, which recovered a restore but let the CA's key take
  over an agent. Lives in ADR-0004 Amendment 2026-11-20 item 4 and SPEC-025 §Operational §3.
- **2026-11-21, ADR-0019's Open question 1** ("Diferir (Recomendado)"): F3's agents page shows no
  audit coverage and the agent reads none; SPEC-026 *Open questions* 2, reopened by a buyer's
  requirement. Discarded: an ADR-0019 amendment making the agent read and report the host's audit
  policy.
- **2026-11-21, the close** (quoted above): close without further review rounds.

### Anticipated (Claude Code)

- **SPEC-026 on a branch stacked on the ADR branch**, restacked after each ADR commit with
  `git rebase --onto`. Discarded: commits on the ADR branch, which would mix a SPEC that is Claude
  Code's to land with the texts Manuel ratifies.
- **SPEC-026's design** (its *Decision record*): column grants direct to `cg_api` through ingest's
  migration `0011_dashboard_rights`; the triage history written by an owner-held
  `SECURITY DEFINER` trigger; invitations in an api table (`0008_dashboard`), 72 hours, a TOTP
  secret generated anew at each request of the holder and recorded (`invitation.totp`); optimistic
  concurrency by `revision`; `audit.read` written before the rows or `503`; a closed incident takes
  no new alert and techniques accumulate; four page phases.
- **SPEC-025's in-process rate limits**, kept, with the departure from ADR-0003's row stated under
  ADR-0003's §Compliance (round 7). Discarded: a Valkey prefix for ingest, which changes ADR-0022 §4
  and the Valkey ACL. Named to Manuel for the ratification.

### Reactive corrections

Each round's List A, below, was fixed in the commit after it. Two changed a design, not only a text:

- Round 5 (ADR branch): the promotion of any later certificate let the CA's key take over an
  agent; Manuel's choice above replaced it.
- Round 8 (ADR branch): the agent's fallback to its old pair on any refused handshake could strand
  it after a promotion, since TLS 1.3 cannot tell a refusal from a lost answer
  (`agent/cg-agent/src/tls.rs:215-220`); the agent now falls back only on a TLS alert that refuses
  the certificate, and retries any other failure under the new pair (SPEC-025 §Operational §10,
  renew_ac_103, its Risks).

## The reviews (CLAUDE.md compensating control 5)

The fixed prompt for a SPEC or ADR text, three reviewers per round in isolated worktrees, each
List A item then given to one adversarial verifier; only verified items counted.

### Claude Code's pass

- ADR branch: the citations of each fix were opened before its commit (among them
  `services/ingest/src/ca.ts:21`, `heartbeat.ts:288-294`, `ADR-0003:31` and `:148`,
  `SPEC-004:202` and `:214`, `signing.rs:14`); ADR-0004's date of Manuel's round-5 choice was
  checked against the session's transcript (2026-11-20, 22:11 UTC).
- SPEC-026: each cited path exists; the audit vocabulary's rows match the routes that write them;
  the test ids continue SPEC-007's `incident_ac_005` and SPEC-009's `dash_ac_008`; the limits
  (72 hours, 15 minutes, 1000 uses, 168 hours) agree wherever stated. Two of its own fixes before
  round 1: `cg_incident_reader`'s `USAGE` is granted by `0009_runtime_grants`, not by
  `10-roles.sh`; "the browser reaches the dashboard alone" is ADR-0022 §2.

### The ADR branch, rounds 4 to 10

- **Round 4, at `3c38cf2` rebased.** List A fixed in `9e7cc4b`: deploy_ac_004's refusals in
  ADR-0022's Effect, ADR-0014's sessions deleted after the commit, the threat model's single-use
  sentences, SPEC-025's narrowed `cg_ingest` grants and several criteria.
- **Round 5.** The promotion design (above), fixed in `9b5d225` with `pending_cert_pem`.
- **Round 6.** Fixed in `1c5e78c`: the old pair removed only after a `2xx` under the new one,
  NFR-002 by scope, the enrollment `429` as exit `3`.
- **Round 7, at `1c5e78c`, four items.** The CA's accepted threat without "alone" and without the
  R6 path through ingest's database role or the server-certificate impersonation; the remedy's
  condition on the anchor; SPEC-004 §Behavior's `200` on a failed Postgres update and its
  "no persistence" on a refusal, amended by scope; ADR-0003's rate-limit row. Fixed in `92a4c4d`
  with two items queued from SPEC-026's round 1 (ADR-0014's reset is audit-logged, its invitation's
  completion the authentication event; SPEC-025:33's list of SPEC-026's ingest changes) and
  ADR-0022's "reassign alerts".
- **Round 8, at `92a4c4d`, two items.** "No certificate outlives the CA" (ingest's server
  certificate is not capped), narrowed to the agents'; the fallback (above). Fixed in `c26b5b5`.
- **Round 9, at `c26b5b5`, three items** (one reviewer found none). The example's
  `outer_envelope_version` (`"0.1.0"`, SPEC-003 FR-007); ADR-0004's source for item 4 naming
  Manuel's second choice; the restore runbook's text as the change that comes with SPEC-025. Fixed
  in `4cb678a`.
- **Round 10, at `4cb678a`:** stopped unfinished at the close; no result.
- List B of each round: the cheap ones fixed with the List A; not fixed, with the reason: the
  catalog's dependency line names the load-bearing edges only, as for SPEC-025; duplicate
  `heartbeats` rows after a `503` (events deduplicate by `event_id`), the copy flag set before a key
  is proved and a holder of the CA's key raising it on any agent (disclosed in SPEC-025's Risks as a
  flag that cannot be cleared); `data-protection.md:31` until the runbooks' PR (SPEC-025 *Out of
  scope*, before the release); implementation details for the code's own review.

### SPEC-026, rounds 1 to 5

- **Round 1, at `9cea3dc`, sixteen items** (each round ran at the SHA before the next restack; the commits named are their SHAs now). Fixed in `fc2db39` (`41af070` after the restacks): among them
  dashapi_ac_001's exclusions, the TOTP secret per request, the threat model's rate-limit line, a
  compromised api causing or suppressing notifications, the breaking `/v1/users` changes declared,
  the triggers split in two, a *Test scenarios* section.
- **Round 2, at `fc2db39`, thirteen items, several shared.** The TOTP confirmation missing from
  dashapi_ac_001; the invitation cited to ADR-0022 (it is ADR-0014's); the reset as an
  authentication event (the ADR branch's fix); R14's closure "to the admin's session"; a reload
  that cannot show a secret; SPEC-025:33; NFR-007-003 by scope. Fixed in `51e2647` with List B's
  pending secret cleared, a disable withdrawing the invitation, R7, R13 and the employee-data entry.
- **Round 3, at `9d5007f`, five items and ADR-0019.** The token's hash inserted but never read;
  SPEC-009's three-views sentences and SPEC-004's single-use section by scope; a disable's
  `invitation.withdraw` row; `triage_history`'s grant to `cg_incident_reader`; ADR-0019's reopened
  question (Manuel's choice). Fixed in `fb3aff0`, with the constant work of an invited user's login
  and a reset admin out of the last-admin count.
- **Round 4, at `7c0a972`, two items.** The login's deletion of the cookie's session against
  SPEC-008's re-read order; "dry-run" for playbooks whose `notify.email` runs live. Fixed in
  `b61f5b6`, with SPEC-008's changed items listed by scope and the invitation route's precondition.
- **Round 5, at `b61f5b6`:** stopped unfinished at the close; no result.
- List B not fixed, with the reason: the QR encoder of `/invitation` is a dashboard dependency the
  phase that builds it records (name, version, licence); the `revision`'s exact SQL comparison, the
  order of `incident_changed` and `transition_refused`, the cross-service tests of dashapi_ac_007
  and 008, which phase lands `0011`: the code's own contract review; the host's `create-user`
  command, which still creates users with a password and prints a secret, is guidance for the first
  admin and the last admin's way back (R14's remaining note).

## Gates and CI

- `markdownlint-cli2` on each changed Markdown file before each commit, its exit read alone: `0`
  each time.
- The branches: a push with no PR open runs no workflow.
- This close: CI on its SHA (reported in the turn).

## Debts

- Opened: none. Closed: none. Re-routed: none on `main`; #26's second half (the email on a severity
  raise) goes to ADR-0017's later notification increment when SPEC-026 lands (its *Out of scope*),
  and #98, #83, #102 and #136 close with SPEC-026's code.
- Open at close: **56**.

## Waiting on Manuel

- The ratification of the ADR branch after its last round, read on the compare, with three separate
  yeses: the ADR amendments (ADR-0004, ADR-0022, ADR-0014) and the threat model's Amendment
  2026-11-20 (SPEC-025); the ten-year CA; the api's new rights and what a compromised api can then
  do. With them: SPEC-025's departure from ADR-0003's rate-limit row, and the residuals each
  round left (the copy flag that cannot be cleared; a restore before a renewal; a server refusing a
  certificate without an alert).
- On the SPEC-026 branch: the threat model's Amendment 2026-11-20 (SPEC-026), for his yes; and the
  breaking change of `GET` and `POST /v1/users` (a public API contract, his list), declared in
  SPEC-026 §Data contracts §3.
- As at Session 87's close: the Authenticode decision and its purchase; #130's legal readings; the
  commercial licence and terms; `SECURITY.md`'s contact; the isolated machines (M3); a penetration
  test; #99, #124 and #125.

## How Session 89 resumes

1. Run the start protocol. `main` is this close's commit; the other branches are
   `docs/f3-f4-adr-amendments` (`4cb678a`, no PR) and `docs/f3-spec-026` (`b61f5b6`, stacked on it,
   no PR); 56 open debts; 22 ADRs / 23 SPECs.
2. A fresh-context review round over the ADR branch at `4cb678a` (the fixed prompt for a SPEC or ADR
   text, its eight documents), its fixes committed with NOT YET RATIFIED and SPEC-026 restacked
   after each, until a round finds no List A; rounds 7 to 9 found four, two and three items, so one
   or two more are expected. Then give Manuel the compare and ask the three yeses above, apart.
3. In parallel, the same over `docs/f3-spec-026` at `b61f5b6` (rounds 1 to 4 found sixteen,
   thirteen, six and two); then ask Manuel's yes on its threat-model amendment and on the breaking
   `/v1/users` change.
4. After his yeses: land the ADR branch on `main` in order (ADRs, threat model, SPEC-025), the
   markers stripped, CI green on each push; then SPEC-026 `Accepted` with its catalog row and its
   debts' destinations; then the runbooks' change through a branch and a PR; then F4's server half
   (roadmap step 4), contract before code.
