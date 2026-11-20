# Handoff — Session 87

Session 87, 2026-11-20, did roadmap step 3's first half. It settled the S86 investigation's nine
contradictions against the repo, put Manuel's decisions to him in one message, and drafted their
records on the owner-review branch `docs/f3-f4-adr-amendments` (`806051a` to `3c38cf2`, pushed, no
PR, every commit marked NOT YET RATIFIED). The branch holds amendments of ADR-0004, ADR-0022 and
ADR-0014, a pointer in the Blueprint's §7, the threat model's amendment with SPEC-025, and SPEC-025
(F4), `Proposed`. SPEC-026 (F3) did not start: it did not fit with quality, and it is Session 88's
work (*How Session 88 resumes*). No code changed and no tag was pushed; the release freeze stays.

## Start and resulting state

- Start: `main` = `origin/main` = `bc3e643`, clean, no stash, no branch but `main`, no PR open, the
  tip's CI ALL GREEN; 22 ADRs / 23 SPECs, 56 open debts, no Known CI debt. The repo agreed with
  Session 86's handoff. 20 GB free on `C:` and 5.3 GB of memory free at the close; no build ran.
- Resulting `main`: this close's commit alone.
- Branches: `docs/f3-f4-adr-amendments`, pushed with no PR (an owner-review branch: a push with no
  PR runs no workflow), six commits: `806051a` (the three ADR amendments, the Blueprint's pointer,
  the ADR catalog), `0ed8b5c` (the first review's fixes), `885d8c9` (the threat model's
  Amendment 2026-11-20 (SPEC-025)), `e8b790d` (SPEC-025 and its catalog row), `a03445d` and
  `3c38cf2` (the second and third reviews' fixes). Compare:
  `https://github.com/AlmanInDaHouse/CyberGuard_Enterprise/compare/main...docs/f3-f4-adr-amendments`.
- Catalogs on `main`: 22 ADRs / 23 SPECs, unchanged; on the branch, SPEC-025 `Proposed`.

## The investigation's contradictions, settled against the repo

The critic of `C:\tmp\s86-research\` (`critic.md`) was read with the four reports and its claims
checked. Its C9 was wrong: deploy_ac_004's red evidence is local, in the api's vitest suite
(`services/api/test/deploy-ac-004-005-postgres.test.ts`, testcontainers), with no probe pull
request. The others became the message's questions or Claude Code's decisions below. Three facts
the reports missed shaped the questions: ADR-0022 §4 (`:48`) is made false by F3's grants, so it
needs an amendment of its own; a restore of the database after a renewal would refuse every agent
that renewed since the backup; and the service control manager never restarts a service that stops
with an error (`agent/cg-setup/src/scm.rs:482-483`), so a terminal exit of a revoked agent would be
permanent.

## Decisions

### Manuel's

On 2026-11-20, in chat, to the message that put nineteen questions to him, each with options and a
recommendation: "acepto cambios y sigo tus recomendaciones". Read, and said back to him, as: the
recommendation on each of the nineteen, and his OK to draft the amendments of ADR-0004, ADR-0022 §4
and ADR-0014 on an owner-review branch. Not read as a ratification of any ADR, nor as the separate
acceptances of the ten-year CA (question 8) and of the api's new rights (question 11), which are
asked with the ratification. The recommendations he took:

1. ADR-0004 amended in place, its References and the catalog's edge corrected (no revocation list
   in ADR-0003).
2. The token opaque, stored as its SHA-256, scope and org checked before the CA signs; no JWT.
3. The new key at renewal proved by the current key and itself; no PKCS#10, no new dependency.
4. Revocation in Postgres on the agent's row, write-once, checked on every request; not Valkey, not
   a CRL.
5. A revoked or superseded agent is refused with a non-terminal `403`.
6. Renewal from 45 days left.
7. After a restore, the server learns a later certificate again; the runbook says what a restore
   undoes.
8. The ten-year CA, no scheduled rotation, reopened by #99 or its ninth year.
9. No `SAN = org_id`.
10. A pointer in the Blueprint's §7.
11. The api's four rights: issue and withdraw tokens, revoke agents, triage, read `audit_log`.
12. The role × capability rows of the message's table (triage and assignment admin and analyst;
    the assignee seen by all three; the audit view, tokens, revocation and invitations admin; the
    agents' inventory admin and analyst; one's own session all three).
13. Host name, platform and version kept on the agent's row; user ids, not emails, in ingest;
    emails shown as the message said, the forensic PDF's assignee included; admins read operators'
    activity and addresses; the copy of an identity detected with two signals, no address compared.
14. Tokens for several uses, bounded, admin-only.
15. Reading the audit view writes an `audit.read` row before the rows are sent, `503` if it cannot.
16. Out of F3: the alerts' assignee, suppress and escalate, comments on incidents, the audit
    export.
17. An admin resets a user's credentials with an invitation shown on screen (ADR-0014 amendment).
18. No notification beyond an incident's creation; #26's email half goes to ADR-0017's later
    increment.
19. #42 (connection reuse) leaves F4 for roadmap step 6.

### Anticipated (Claude Code)

- **SPEC numbers**: SPEC-025 is F4, both halves, each criterion labelled; SPEC-026 is F3. Against
  one SPEC per half: Manuel asked for "el SPEC de F4".
- **Renewal writes no row; the first request under the new certificate promotes it** (SPEC-025
  Decision record 1). Against storing it at signing: a lost answer would strand the agent, and a
  restore would need a second mechanism.
- **A withdrawal is `withdrawn_at`, not an enum value**: Kysely runs every pending migration in one
  transaction (`services/ingest/src/db/migrate.ts:163-164`).
- **The agent keeps both pairs until a handshake under the new one succeeds, and checks the issuer,
  not the chain**: an operator-provided server certificate makes the agent's anchor another CA
  (SPEC-023 §Operational §6). Against checking the chain: every renewal would be discarded there.
- **Token caps, the withdrawal and the revocation's time enforced by triggers of ingest's owner
  role**, `issued_at` forced at insert: against relying on the api, which a compromise controls.
- **A history of triage changes by a `SECURITY DEFINER` trigger of ingest's owner role** (ADR-0022
  amendment, item 3), beyond question 11's text: without it, the only trace of a hidden closure is
  the `audit_log` row the api itself writes. Reported to Manuel in the same turn.
- **Rate limits in process, constants**; the renewal on the heartbeat target's base URL with its own
  path; no new environment variable, port or `agent.toml` key.

### Reactive corrections

- The message to Manuel promised "una fila de auditoría por enrolamiento" for multi-use tokens.
  Ingest cannot write the api's `audit_log` (ADR-0022 §4); the record is the agent's row naming its
  token, which the token's page lists. Reported to Manuel in the same turn.
- The three reviews' findings, below.

## The reviews (CLAUDE.md compensating control 5)

### Claude Code's pass

Each `path:line` the texts cite was opened (among them `heartbeat.ts:176-177`, `ca.ts:14-15`,
`scm.rs:482-483`, `backup-and-restore.md:13`, `SPEC-003:74` and `:77`, `blueprint.md:222`,
`delivery.rs:54`, `cert.ts:79-80`), and the claims made in more than one document were compared.
Two of its own fixes before the first commit: the SPEC-003 path, and ADR-0014's "a disabled
account is not recreated", which the repo does not say (no route deletes a user).

### A reviewer with a fresh context, three rounds (the fixed prompt for a SPEC or ADR text)

- **Round 1, at `806051a`.** List A: the Blueprint's note added two lines and moved about 25
  `blueprint.md:NNN` citations; the note called unbuilt parts built; step 6's "known public key"
  under a later certificate; the api's token grants against the claimed bound; ADR-0014's reference
  to a Redis revocation list. All fixed in `0ed8b5c` (the note folded into the bullet, no line
  added). List B: fixed but the ADR-0014 §3 cross-reference (the api stays in `services/api`;
  ADR-0022's amendment states the boundary's move).
- **Round 2, at `e8b790d`.** List A, eleven: the renewal URL (the heartbeat target is a base URL);
  the swap's chain check against an operator's anchor; `issued_at` chosen by the inserter; ADR-0022
  claiming a threat-model change it does not make; the two "updated in place" lines unchanged;
  equal `notBefore` after a restart; SPEC-004 FR-002 and FR-006 unlisted; the used-up token's
  status; renewal "writes no row" against promotion; #42 unrecorded; commit time against
  transaction time. All fixed in `a03445d` but #42, recorded by this close (`docs/debts.md`, the
  roadmap's step 6).
- **Round 3, at `a03445d`.** List A, three: #42 (this close); the triggers' kinds; R7's text without
  the alert's time. Fixed in `3c38cf2` with most of List B (uses that only grow, a failed
  enrollment defined, the promotion's lost race, the renewal serialised with other requests, the
  history's `SECURITY DEFINER`, host names as Manuel's choice, old tokens capped). Not fixed, with
  the reason: SPEC-026 does not exist yet (its references stand until it does); the copy flag
  cannot be cleared by a running service (disclosed in SPEC-025's Risks).
- **Not yet done:** a fresh-context round over `3c38cf2`. Session 88 runs it before Manuel reads
  the compare (*How Session 88 resumes*).

## Gates and CI

- `markdownlint-cli2` on each changed Markdown file, its exit read alone: `0` each time, the last
  over the branch's eight files.
- The branch: a push with no PR open runs no workflow.
- This close: `markdown-lint` on its SHA (reported in the turn).

## Debts

- Opened: none. Closed: none.
- Re-routed: **#42** to roadmap step 6 (Manuel's choice of 2026-11-20, question 19).
- Open at close: **56**.

## Waiting on Manuel

- The ratification of the branch, read on the compare, with three separate yeses: the ADR
  amendments (ADR-0004, ADR-0022, ADR-0014) and the threat model's amendment; the acceptance of the
  ten-year CA; and of the api's new rights and what a compromised api can then do. SPEC-025 lands
  `Accepted` after them and after its last review round.
- As at Session 86's close: the Authenticode decision and its purchase; #130's legal readings; the
  commercial licence and terms; `SECURITY.md`'s contact; the isolated machines (M3); a penetration
  test; #99, #124 and #125.

## How Session 88 resumes

1. Run the start protocol. `main` is this close's commit; the one other branch is
   `docs/f3-f4-adr-amendments` (`3c38cf2`, no PR); 56 open debts; 22 ADRs / 23 SPECs.
2. A fresh-context review round over the branch at `3c38cf2` (the fixed prompt for a SPEC or ADR
   text, the eight documents), its fixes committed with NOT YET RATIFIED, until a round finds no
   List A; then give Manuel the compare and the three yeses above.
3. SPEC-026 (F3) on the same branch or a branch stacked on it: the api's routes (tokens, multi-use
   ones included; agents and revocation; triage; the audit view and its `audit.read` row;
   invitations, #98; the reset of ADR-0014's amendment; #102; TOTP enrollment, #83; users; the
   password change), the grants and the triage history through ingest's migrations, deploy_ac_004
   and SPEC-023 by scope, the matrix rows of question 12 (SPEC-008 §5, SPEC-009), the audit
   vocabulary, the pages with their phasing, and the gated detection change (a closed incident's
   new alert opens a new incident, SPEC-007 by scope; #26's technique accumulation). With it the
   threat model's amendment for SPEC-026 (R3, R7, §3 `:302`, R14, §9). Its two-half review.
4. After Manuel's ratification: land the branch on `main` in order (ADRs, threat model, SPEC-025,
   SPEC-026), the markers stripped, CI green on each push; then the runbooks' change (revocation,
   the CA's compromise, a restore after renewal, the token's wording, data protection) through a
   branch and a PR; then F4's server half (roadmap step 4), contract before code.
