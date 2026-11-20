# SPEC-026: The dashboard — users and invitations, enrollment tokens and agents, triage, and the audit view

- **ID:** SPEC-026
- **Title:** The dashboard — users and invitations, enrollment tokens and agents, triage, and the audit view
- **Status:** Proposed
- **Depends on:**
  - ADR-0022 Amendment 2026-11-20 — the api's rights on ingest's tables, which this SPEC grants: tokens (item 1), agents (item 2), triage and its history (item 3), its own `audit_log` read (item 4), each change in one transaction with its `audit_log` row.
  - ADR-0014 and its Amendment 2026-11-20 — the human authentication model; an admin's reset of another user's credentials by an invitation shown on screen.
  - SPEC-025 — the server's side of tokens, agents and revocation (ingest's migration `0010_agent_certificates`), whose server half lands before this SPEC's code (`docs/product/roadmap.md`, step 4).
  - SPEC-007, SPEC-008, SPEC-009 — grouping, authentication and the read slice, which this SPEC extends.
  - This SPEC amends **by scope**: SPEC-002 (FR-014's "the token is already single-use"), SPEC-004 (FR-015's "no HTTP admin endpoint" and its §Ratification record's decision 2, CLI-only issuance, both superseded for the api's route, as SPEC-008 §Operational §5 asks; the record itself stands), SPEC-007 (§Data contracts §3's unique key, §Operational §1 and §4: a closed incident takes no new alert, and an incident's techniques accumulate), SPEC-008 (§Data contracts §1 and §2, §Operational §5, §6 and §8, its *Out of scope* on recovery, and its Amendment 2026-10-15's items 2, 5, 6 and 12), SPEC-009 (its *Out of scope*, §Operational §2's "all three roles read", NFR-009-004, and its second and third Amendments 2026-10-15 where they say the dashboard has three views and the reads record no address), SPEC-011 (`cg_mitre` among the preserved fields), SPEC-013 (the report's assignee), SPEC-022 (no dashboard view of executions), SPEC-023 (§Operational §4's grants and "the api names ingest's three tables", deploy_ac_004, its *Out of scope* and §8 on the pages the dashboard lacks, and its Amendment 2026-10-26's item 1, the later users created on the host until F3) and SPEC-024 (its two sentences that call the token single-use). Each one's acceptance criteria stand, read with the changes §Data contracts and §Operational state.
- **Authors:** Manuel (project owner), Claude Code (proposal and implementation)

## Context

The dashboard has four pages: the login, the incidents list, an incident's detail and a redirect (`dashboard/app/`). The api serves more than the dashboard shows: the alerts, an incident's events, its playbook records, the forensic export and report, the password change, the TOTP confirmation and the user routes (ADR-0022 §Context 9). There is no page for the agents or the enrollment tokens, though the MVP has the operator issue a token and watch the agent appear from the dashboard (`docs/product/blueprint.md`, the MVP paragraph); nothing writes the triage state that detection's tables carry (`incidents.status`, `incidents.assigned_to`, `alerts.status`); and no one can read `audit_log` but the database's owner role. Admins set users' passwords and receive their TOTP secrets (R14 of `docs/security/threat-model.md`, debt #98); the api's login keeps the session the browser already held (debt #102); the dashboard enrolls no TOTP and manages no user (debt #83). A closed incident absorbs the alerts of its key without reopening and without notice (`services/ingest/src/detect/incidents.ts`), and an incident keeps the techniques of its first alert alone (debt #26).

This SPEC is roadmap §F's F3. Manuel decided its scope and the role of each capability on 2026-11-20 (`docs/handoff-session-87.md`, questions 11 to 18), and its grants are ADR-0022's Amendment 2026-11-20.

## Scope

### In scope

1. **Ingest's migration `0011_dashboard_rights`**: the api's column grants on `enrollment_tokens`, `agents`, `incidents` and `alerts`; the triage history and its trigger; ingest's own `UPDATE` on `incidents` narrowed; the unique key of an open incident.
2. **The detection change**: a closed incident takes no new alert, and a new incident opens for its key; an incident accumulates its alerts' techniques (debt #26, its first half).
3. **The api's migration `0008_dashboard`**: the users' credentials that can be cleared, the invitations, `SELECT` on `audit_log` and its index.
4. **The api's routes**: the session; the login that ends the session its cookie names (#102); users, invitations and the reset (#98, #83); enrollment tokens, single-use and for several uses; agents and their revocation; the triage of incidents and alerts; the audit view and the triage history, each read recorded; the read routes' additions.
5. **The role of each capability** (§Operational §1), tested by a criterion that blocks CI.
6. **The audit vocabulary** of the new actions (§Data contracts §4).
7. **The dashboard's pages**, in four phases (§Operational §9): accounts, triage and the reads, agents and tokens, the audit view.

### Out of scope

- **The alerts' assignee, suppressing and escalating an alert, comments on incidents, and an export of the audit view**: after the MVP (Manuel's choice of 2026-11-20, question 16).
- **A notification beyond an incident's creation**: debt #26's second half, an email on a severity raise, goes to ADR-0017's later notification increment (question 18). A new incident opened after a close is a creation, and notifies (§Operational §6).
- **A reset the user starts alone**, proving who they are without an admin: it depends on the notifier (ADR-0014 Amendment 2026-11-20).
- **Email delivery of an invitation**: the api has no egress (ADR-0022 §3).
- **The retention of `audit_log` and of the triage history**: kept without a period, as the other tables are (`docs/operations/data-protection.md`); the position is roadmap step 9's (*Open questions*).
- **The runbooks**: the users' creation from the dashboard instead of the api's container (`docs/operations/install.md`), the audit view in `docs/operations/forensic-verification.md`, and the personal data of `docs/operations/data-protection.md` go through the runbooks' branch and pull request, which `deploy-ci` reads, before the release.
- **Pagination of the reads that have none today**: an incident's events (SPEC-010), its playbook records (SPEC-022) and an incident's alerts (SPEC-009). Their pages show what the routes answer.
- **An `activity_id` that follows the status**: an incident's `activity_id` stays `1`, its creation; its triage is its `status` (Decision record 7).

## Data contracts

### 1. Ingest's migration `0011_dashboard_rights`

One migration, run as `cg_ingest_owner` in the transaction of every pending migration, after `0010_agent_certificates` (SPEC-025). Its grants act only when the role they name exists, as `0009_runtime_grants` does, so they do nothing in the developer's stack and in tests that run without the bundle's roles.

**The api's grants**, to `cg_api` directly; `cg_api` reaches the schema through `cg_incident_reader`, granted to it by `deploy/docker/postgres/10-roles.sh`, whose `USAGE` on it `0009_runtime_grants` grants:

| Table | Grant |
|---|---|
| `enrollment_tokens` | `INSERT (token_id, token_sha256, org_id, scope, expires_at, max_uses)`; `SELECT` of every column but `token_sha256`; `UPDATE (withdrawn_at)`. |
| `agents` | `SELECT (agent_id, org_id, enrolled_at, expires_at, last_seen, enrolled_by_token_id, hostname, platform, agent_version, revoked_at, revocation_reason, duplicate_signal, duplicate_signal_at)`; `UPDATE (revoked_at, revocation_reason)`. |
| `incidents` | `UPDATE (status, assigned_to, updated_at)`, besides the `SELECT` it has through `cg_incident_reader`. |
| `alerts` | `UPDATE (status, updated_at)`, besides the `SELECT` it has through `cg_incident_reader`. |
| `triage_history` | `SELECT`, through `cg_incident_reader`, as SPEC-023 §Operational §4 grants a table the api reads. |

No grant reaches `ca`, a token's hash, an agent's key or certificate, or a column detection derives (`alert_ids`, `source_events`, `cg_mitre`, `severity_id`, `title`, `final_score` among them); none is an `INSERT` or a `DELETE` on `incidents`, `alerts` or `agents`. The writes of the api's role keep the triggers of SPEC-025: an issued token's `issued_at`, `uses` and `state` are the database's, a withdrawal and a revocation are write-once and take the database's time.

**Ingest's own grant on `incidents`**: its table-level `UPDATE` is revoked and replaced by `UPDATE (alert_ids, severity_id, cg_mitre, updated_at)`, the columns grouping writes, so that neither runtime role but the api changes a status or an assignee.

**`triage_history`**, owned by `cg_ingest_owner`:

| Column | Type | Meaning |
|---|---|---|
| `history_id` | `bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY` | Its order. |
| `org_id` | `text NOT NULL` | The row's org. |
| `object_kind` | `text NOT NULL`, `CHECK (object_kind IN ('incident', 'alert'))` | |
| `object_id` | `uuid NOT NULL` | The incident or the alert. |
| `field` | `text NOT NULL`, `CHECK (field IN ('status', 'assigned_to'))` | |
| `old_value`, `new_value` | `text` | The values before and after; `NULL` for no assignee. |
| `changed_at` | `timestamptz NOT NULL` | The transaction's `now()`, which is the `occurred_at` of the `audit_log` row the api writes in the same transaction. |
| `changed_by_role` | `text NOT NULL` | `session_user`, the role that made the change. |

An index on `(org_id, object_kind, object_id, history_id)`.

**The triggers**, each with a function `cg_ingest_owner` owns:

- `triage_history_record`, `AFTER UPDATE OF status, assigned_to` on `incidents` and `AFTER UPDATE OF status` on `alerts`, `FOR EACH ROW`: one row in `triage_history` for each of those fields whose value changed (`IS DISTINCT FROM`). Its function is `SECURITY DEFINER`, so that it inserts as the owner while no runtime role holds `INSERT` on the history, and sets `search_path` to `ingest, pg_temp`, so that a runtime role cannot place an object the function would resolve before ingest's.
- `triage_history_write_once`, `BEFORE UPDATE OR DELETE` `FOR EACH ROW`, and `BEFORE TRUNCATE` `FOR EACH STATEMENT`, on `triage_history`: refused with `SQLSTATE` `P0001`, on the model of `playbook_executions_write_once` (`services/ingest/src/db/migrations/0008_playbook_executions.ts`). It holds against a writer that goes through a runtime role, not against the owner, who can drop it.

**The open incident's unique key.** The constraint `incidents_grouping_key_unique` is dropped and replaced by a unique index on `grouping_key` where `status NOT IN ('resolved', 'false_positive')`: at most one open incident per key, any number of closed ones.

### 2. The api's migration `0008_dashboard`

Run as `cg_api_owner`, its grants acting only when `cg_api` exists, as `0007_runtime_grants` does.

**`users`.** `password_hash` and `totp_secret` become nullable, under `CHECK ((password_hash IS NULL) = (totp_secret IS NULL))` and `CHECK (totp_secret IS NOT NULL OR NOT totp_enrolled)`. A user with neither has an invitation to complete and cannot sign in.

**`invitations`**:

| Column | Type | Meaning |
|---|---|---|
| `invitation_id` | `uuid PRIMARY KEY` | |
| `user_id` | `uuid NOT NULL REFERENCES users (user_id)` | Whom it is for. |
| `org_id` | `text NOT NULL` | The user's org. |
| `purpose` | `text NOT NULL`, `CHECK (purpose IN ('invite', 'reset'))` | A new user's, or a reset's. |
| `token_sha256` | `bytea NOT NULL UNIQUE` | SHA-256 of the invitation's text, its base64url characters as ASCII. The text is kept nowhere. |
| `issued_at` | `timestamptz NOT NULL DEFAULT now()` | |
| `expires_at` | `timestamptz NOT NULL`, `CHECK (expires_at <= issued_at + interval '72 hours')` | 72 hours after its issue. |
| `pending_totp_secret` | `bytea` | The TOTP secret shown to the holder, encrypted as `users.totp_secret` is (`pgp_sym_encrypt` under `API_DB_ENC_PASSPHRASE`), until the invitation is completed. |
| `completed_at`, `withdrawn_at` | `timestamptz` | At most one set. |

A unique index on `user_id` where `completed_at IS NULL AND withdrawn_at IS NULL`: a user has at most one open invitation. `cg_api` gets `SELECT`, `INSERT` and `UPDATE` on `invitations`, no `DELETE`.

**`audit_log`.** `cg_api` gets `SELECT`, besides its `INSERT`; still no `UPDATE` or `DELETE`. An index on `(org_id, id)`.

### 3. The routes

Every route below answers JSON, sets `Cache-Control: no-store`, and follows SPEC-008's order of checks: the session (`401`), the role (`403`, with its `access.denied` row), the CSRF token on a mutation (`403`), then the `:id`, canonical or `404` (SPEC-009 Amendment 2026-10-14), then the body, `.strict()` (`400`). A row of another org answers `404`. A list takes `limit` (default 50, at most 200) and an opaque `cursor`, keyset over the order it names, and answers `{ items, next_cursor }` (SPEC-009 NFR-009-001). The roles are §Operational §1's.

#### Session and credentials

| Route | Body or query | Answer |
|---|---|---|
| `GET /v1/auth/session` | | `{ user_id, email, role }` of the session's user. |
| `POST /v1/auth/login` | as today | As today; with a `cgsess` cookie, it ends the session the cookie names once the login succeeds (§Operational §2). |
| `POST /v1/auth/password` | as today | As today; it ends every session of the user, the current one among them, and issues none (SPEC-008 Amendment 2026-10-15, item 9). |
| `POST /v1/invitations/totp` | `{ token }` | `{ email, provisioning_uri }`. No session. |
| `POST /v1/invitations/complete` | `{ token, password, totp_code }` | `204`. No session. |

#### Users and invitations

| Route | Body or query | Answer |
|---|---|---|
| `GET /v1/users` | `limit`, `cursor`; order `(created_at, user_id)` | Each user's `user_id`, `email`, `role`, `status`, `totp_enrolled`, and `invitation`: `null`, or `{ purpose, expires_at, expired }` of its open invitation. |
| `GET /v1/users/assignable` | `limit`, `cursor`; order `(email, user_id)` | The org's active admins and analysts with a confirmed factor: `user_id`, `email`. |
| `POST /v1/users` | `{ email, role }` | `201` `{ user_id, invitation: { token, expires_at } }`. |
| `POST /v1/users/:id/role`, `POST /v1/users/:id/status` | as today | As today. |
| `POST /v1/users/:id/invitation` | `{}` | `201` `{ token, expires_at }`: a new invitation for a user who has not completed one and has no confirmed factor; any open one is withdrawn. |
| `POST /v1/users/:id/invitation/withdraw` | `{}` | `204`; `409 no_open_invitation`. |
| `POST /v1/users/:id/reset` | `{}` | `201` `{ token, expires_at }` (§Operational §4); `409 cannot_reset_self`, `409 user_disabled`. |

#### Enrollment tokens

| Route | Body or query | Answer |
|---|---|---|
| `POST /v1/enrollment-tokens` | `{ "uses": 1 }`, or `{ "uses": n, "hours": h }` with `n` from 2 to 1000 and `h` from 1 to 168 | `201` `{ token_id, token, max_uses, expires_at }` (§Operational §5). |
| `GET /v1/enrollment-tokens` | `limit`, `cursor`; order `(issued_at, token_id)` descending | Each token's `token_id`, `max_uses`, `uses`, `issued_at`, `expires_at`, `withdrawn_at`, and `state`: `active`, `used_up`, `expired` or `withdrawn`. Never the token or its hash. |
| `POST /v1/enrollment-tokens/:id/withdraw` | `{}` | `204`; `409 already_withdrawn`. |

#### Agents

| Route | Body or query | Answer |
|---|---|---|
| `GET /v1/agents` | `state` (`active`, `revoked`), `token_id`, `limit`, `cursor`; order `(enrolled_at, agent_id)` descending | Each agent's columns of §1's `SELECT` grant, `org_id` aside. |
| `GET /v1/agents/:id` | | The same, for one agent. |
| `POST /v1/agents/:id/revoke` | `{ reason }`, 1 to 500 code points | `204`; `409 already_revoked`. |

#### Triage

| Route | Body | Answer |
|---|---|---|
| `POST /v1/incidents/:id/status` | `{ status, revision }` | `200` with the incident's detail; `409 incident_changed`, `409 transition_refused`, `409 incident_superseded`. |
| `POST /v1/incidents/:id/assignee` | `{ user_id, revision }`, `user_id` a user's or `null` | `200` with the incident's detail; `409 incident_changed`; `400 invalid_assignee`. |
| `POST /v1/alerts/:id/status` | `{ status, revision }` | `200` with the alert; `409 alert_changed`, `409 transition_refused`. |

#### The audit view

| Route | Query | Answer |
|---|---|---|
| `GET /v1/audit-log` | `action`, `user_id`, `outcome`, `since`, `until`, `limit`, `cursor`; order `id` descending | Each row's `id`, `occurred_at`, `user_id`, `user_email`, `action`, `target_id`, `outcome`, `source_ip`, `correlation_id`, `detail`. |
| `GET /v1/triage-history` | `object_kind`, `object_id`, `since`, `until`, `limit`, `cursor`; order `history_id` descending | Each row of §1's table, `org_id` aside, with `old_email` and `new_email` for an assignee's ids. |

**The read routes' additions**, additive: an incident's list item and detail, and an alert, carry `revision`, the decimal count of microseconds since 1970 of the row's `updated_at`, which a triage write sends back; an incident's detail and the report's header carry `assignee_email`, the email of the user `assigned_to` names in the org, or `null`. The incidents list takes `status` as today, and the alerts list as today.

### 4. The audit vocabulary

Each new row has SPEC-008's columns (Amendment 2026-10-15, item 6): the acting user, the org, `target_id`, the outcome, the source address and the correlation id. Its `detail` holds strings, and never an address, a password, a code, a token or a reason's text.

| Action | Written by | `target_id` | `detail` |
|---|---|---|---|
| `user.create` | `POST /v1/users` | the user | `{ role }`, as today |
| `invitation.issue` | `POST /v1/users`, `/invitation`, `/reset` | the user | `{ purpose, expires_at }` |
| `invitation.withdraw` | `/invitation/withdraw`, and an issue that replaces an open one | the user | `{ purpose }` |
| `invitation.complete` | `/v1/invitations/complete` | the user, or `NULL` for an unknown token | `{ purpose }`; outcome `ok` or `denied` |
| `user.reset` | `/reset` | the user | none |
| `token.issue` | `POST /v1/enrollment-tokens` | the token's id | `{ max_uses, expires_at }` |
| `token.withdraw` | `/withdraw` | the token's id | none |
| `agent.revoke` | `/revoke` | the agent | none |
| `incident.status.change` | `/v1/incidents/:id/status` | the incident | `{ from, to }` |
| `incident.assign` | `/v1/incidents/:id/assignee` | the incident | `{ from, to }`, user ids, `""` for none |
| `alert.status.change` | `/v1/alerts/:id/status` | the alert | `{ from, to }` |
| `audit.read` | `GET /v1/audit-log`, `GET /v1/triage-history` | none | `{ view }` and each filter the request gave, its value as given |

`invitation.complete` is an authentication event (SPEC-008 Amendment 2026-10-19, item 2); the others record a change or a read. `invitation.totp`, the request that shows the provisioning URI, writes no row: it changes nothing a session can use.

## Operational

### 1. The role of each capability

Manuel's choice of 2026-11-20 (question 12), with SPEC-008 §Operational §5's rows, which stand:

| Capability | admin | analyst | viewer |
|---|---|---|---|
| One's own session (`GET /v1/auth/session`) | ✅ | ✅ | ✅ |
| Read incidents, alerts, an incident's events and playbook records (SPEC-009, SPEC-010, SPEC-022) | ✅ | ✅ | ✅ |
| See an incident's assignee (`assignee_email`) | ✅ | ✅ | ✅ |
| An incident's status: acknowledge, investigate, contain, close, reopen | ✅ | ✅ | ❌ |
| Assign an incident; list the assignable users (their emails) | ✅ | ✅ | ❌ |
| An alert's status: acknowledge, close | ✅ | ✅ | ❌ |
| The forensic export and report (SPEC-009 third Amendment 2026-10-15) | ✅ | ✅ | ❌ |
| The agents' inventory | ✅ | ✅ | ❌ |
| Revoke an agent | ✅ | ❌ | ❌ |
| Enrollment tokens: list, issue, withdraw | ✅ | ❌ | ❌ |
| Users: list, create, invite, withdraw an invitation, reset, role, status | ✅ | ❌ | ❌ |
| The audit view and the triage history | ✅ | ❌ | ❌ |

An unauthenticated holder of an invitation reaches its two routes alone (§3). A route outside these rows answers as SPEC-008 and SPEC-009 say.

### 2. The login and the cookie's session (debt #102)

When `POST /v1/auth/login` carries a `cgsess` cookie that names a session, a successful login deletes that session before it issues the new one, whatever user it belonged to; a failed login leaves it. The dashboard's revocation before a login (SPEC-009 second Amendment 2026-10-15, item 3) stays. The session the login deleted writes no row of its own: the login's `auth.login.ok` is the record.

### 3. Invitations (debt #98)

An invitation is 32 bytes from a cryptographic random source, written in base64url; the api keeps its SHA-256 alone and shows the text once, to the admin who issued it, who hands it to the user outside CyberGuard. It lives 72 hours, is completed once, and a new one for the same user withdraws the open one.

- `POST /v1/users` creates the user with no password and no TOTP secret, `totp_enrolled` false, and issues an `invite` invitation, in one transaction with `user.create` and `invitation.issue`.
- `POST /v1/invitations/totp`, unauthenticated, looks the invitation up by its hash: an unknown, expired, withdrawn or completed one, or one whose user is disabled, answers `401 invitation_rejected`, one answer for all. Otherwise, on its first call it generates the TOTP secret and stores it, encrypted, on the invitation; every call answers the user's email and the provisioning URI of that secret. The secret reaches the holder of the invitation alone, never the admin.
- `POST /v1/invitations/complete`, unauthenticated, checks the invitation as `/totp` does, the new password as SPEC-008 Amendment 2026-10-15 item 4 does, and the code against the pending secret, marked used as a login's is. In one transaction it sets the user's `password_hash` (Argon2id), `totp_secret` and `totp_enrolled`, raises their `authority_version`, and sets `completed_at`, with an `invitation.complete` row; it issues no session. The user then signs in.
- Both routes are limited per invitation (five failures in 15 minutes, on a counter of their own under `rl:`) and per source address (the login's counter), with the reservation of SPEC-008 Amendment 2026-10-16 item 3. A failure of `complete` writes `invitation.complete` with outcome `denied`.
- The host's command (`services/api/src/cli/create-user.ts`) still creates a user with a password and a secret it prints, for the first admin and for the last admin's way back (SPEC-008 §Operational §6); the users after the first are created with an invitation, and the command's header says so (debt #136). `POST /v1/auth/totp/confirm` stays for a user that command created; a user with no secret is refused as an unknown one is.

The dashboard builds the link from its own origin, `/invitation#<token>`: the fragment reaches neither the dashboard's server nor the api's log (§Operational §9).

### 4. The reset (ADR-0014 Amendment 2026-11-20)

`POST /v1/users/:id/reset`, admin, for another active user of the org: in one transaction, with `user.reset` and `invitation.issue`, it clears `password_hash` and `totp_secret`, sets `totp_enrolled` false, raises `authority_version`, withdraws any open invitation and issues a `reset` one; after the commit it deletes the user's sessions, as a disable does. An admin's own reset answers `409 cannot_reset_self`, a disabled user's `409 user_disabled`. The org's last active admin who has lost a credential is recovered by the host's command (SPEC-008 §Operational §6), which creates another admin.

### 5. Enrollment tokens

`POST /v1/enrollment-tokens` draws 32 bytes from a cryptographic random source, writes them in base64url, and inserts the token's identifier, its SHA-256 (SPEC-025 §Data contracts §1), the session's org, the scope `enroll`, `max_uses`, and an `expires_at` the database computes: `now() + interval '15 minutes'` for one use, `now()` plus the hours asked for several. It answers the token once and writes `token.issue` in the same transaction. The api shares no code with ingest's `issue-token`; the hash's form is SPEC-025's.

A token's `state` in the list is `withdrawn` when `withdrawn_at` is set, else `used_up` when `uses = max_uses`, else `expired` when `expires_at` has passed, else `active`. The agents a token enrolled are `GET /v1/agents?token_id=`.

### 6. Triage

- **An incident's status.** From `open`, `acknowledged`, `investigating` or `contained`, to any other of the six; from `resolved` or `false_positive`, to `open` alone, a reopening. Any other change, the same status included, is `409 transition_refused`.
- **An alert's status.** From `new` or `acknowledged`, to `acknowledged`, `resolved` or `false_positive`; any other change is `409 transition_refused`.
- **The assignee.** A user id of an active admin or analyst of the org with a confirmed factor, checked under `FOR SHARE` in the write's transaction, or `null`; anything else is `400 invalid_assignee`. An incident in any status can be assigned.
- **One transaction.** The write updates the row `WHERE` its id, its org and its `revision` match, setting `updated_at` to `now()`, and appends its `audit_log` row; if the row is not updated, the incident or alert is read again: absent, `404`; else `409 incident_changed` or `409 alert_changed`. A failure of the audit row rolls the change back and answers `503`. The history's trigger writes in the same transaction.
- **A reopening that meets an open incident of the same key** (§7) answers `409 incident_superseded`: the alerts since the close are in that one.

### 7. A closed incident and the techniques (detection)

Grouping's upsert (`services/ingest/src/detect/incidents.ts`) conflicts on the open incident's unique key of §Data contracts §1: an alert whose key has an open incident joins it, as SPEC-007 §Operational §4 says; an alert whose key has only closed incidents, `resolved` or `false_positive`, opens a new incident, which is a creation: it is notified (ADR-0017) and runs its playbooks (ADR-0021), as any other. A close that commits while an alert of its key is grouped either refuses the close as changed (§6) or leaves the alert to a new incident; no alert joins an incident after its close.

On the update, `cg_mitre.techniques` becomes the incident's techniques followed by the alert's that it lacks, in the alert's order; `cg_mitre.tactics`, the status, the assignee, `activity_id` and the title are kept, as SPEC-007's invariant says of a human's triage.

### 8. The audit view

`GET /v1/audit-log` and `GET /v1/triage-history` write their `audit.read` row, and commit it, before they read the page; if the row cannot be written, they answer `503` and send nothing (Manuel's choice of 2026-11-20, question 15). A page can include its own row. `user_email` and the history's emails are read from `users` when the page is built.

### 9. The dashboard

**Its access to the api.** The browser reaches the dashboard alone (ADR-0022 §2). A page's server reads forward the session cookie alone, as SPEC-009's second Amendment 2026-10-15 item 6 says, except the reads that record a source address, the export, the report and the audit view, which forward `X-Forwarded-For` as the login does (item 5). A mutation is a route handler under `/api/` on the dashboard's origin, with the same-origin check of item 2, the CSRF cookie sent as `x-csrf-token`, `X-Forwarded-For` passed through, a 16 KiB body limit and `Cache-Control: no-store`, as the login and the logout are. The invitation's two routes are handlers of the same kind with no session and no CSRF token. The role read from `GET /v1/auth/session` shows or hides a page's controls; the api's checks decide.

**The pages, in four phases**, each landing with its api routes:

1. **Accounts**: a navigation shared by every page, its entries by role; `/invitation`, which reads the token from the fragment, removes it from the address bar, shows the provisioning URI as a QR code and as text, and completes the invitation; `/account`, the password change, which then clears both cookies and returns to `/login`; `/users`, for admins: the list, a new user with the invitation's link shown once, a new invitation, its withdrawal, a reset, the role and the status.
2. **Triage and the reads**: `/incidents` with its status filter, pages, and a refresh every 30 seconds while open; an incident's detail with its assignee, its status and assignee controls, its alerts with theirs, its events (SPEC-010), its playbook records (SPEC-022), and the export's and the report's downloads, through handlers that stream the api's answer; `/alerts`, with its filters, pages, the 30-second refresh and the status controls.
3. **Agents and tokens**: `/agents`, its state and token filters, each agent's host name, platform, version, last heartbeat, certificate's end, revocation and the copy flag of SPEC-025; an agent's revocation with its reason, for admins; `/tokens`, for admins: the list, a new token for one use or several shown once with a warning that it will not be shown again, its withdrawal, and the agents it enrolled.
4. **The audit view**: `/audit`, for admins, `audit_log` and the triage history, with their filters and pages.

A `409` on a triage control reloads the incident or the alert and says that it changed.

### 10. No new deployment configuration

No environment variable, setting or port is added. The invitation's 72 hours, the tokens' bounds, the lists' caps, the rate limits and the 30-second refresh are constants of the code. The Valkey keys are under the api's existing prefixes (`rl:`, `totp:replay:`), so its ACL is unchanged (SPEC-023 §Operational §4).

## Security considerations

- **What a compromised api can now do** is ADR-0022's Amendment 2026-11-20 list, which Manuel accepts by ratifying it: enroll agents of its own within the token caps, revoke every agent (each revocation undone by the host's administrator), withdraw every token, close or reassign incidents and alerts (each change kept in the owner's history), and read the operators' activity and addresses. It cannot read `ca`, a token's hash or an agent's key, write a column detection derives, or delete a row.
- **The history does not depend on the api.** Its trigger writes as the owner, so a change the api makes without its `audit_log` row is still recorded, with the role that made it.
- **An invitation is a credential until it is completed or expires**: whoever holds it chooses the password and enrolls the factor (ADR-0014 Amendment 2026-11-20). It is shown once, kept as a hash, travels in a URL fragment that no server logs, and the two routes that take it are limited.
- **The admin sees neither a password nor a TOTP secret** (R14 closed): the secret is generated for the invitation's holder and shown to them alone.
- **Emails reach analysts** through the assignee picker, and every role through an incident's assignee (Manuel's choice of 2026-11-20, question 13).
- **The audit view's readers are audited**, before the rows are sent.
- **The dashboard-to-api leg** is plain HTTP on the host's internal network (ADR-0022 §3, debt #97); it now carries invitations, new passwords, TOTP codes and enrollment tokens besides a login's.

## Acceptance criteria

The api's criteria are vitest tests of `services/api` (testcontainers), named `dashapi_ac_NNN` in files `dashapi-ac-NNN-<slug>.test.ts`; the grants' are the api's deploy_ac_004 test (SPEC-023); detection's are vitest tests of `services/ingest`, numbered after SPEC-007's `incident_ac_005`; the dashboard's are its vitest tests, numbered after SPEC-009's `dash_ac_008`.

- **dashapi_ac_001 (the roles; blocks CI).** For each route of §Data contracts §3 and each of the three roles, a request with a valid session and CSRF token answers as §Operational §1 says: a capability the role lacks answers `403` with an `access.denied` row and changes nothing; a mutation without the CSRF token answers `403`; every route but the invitation's two and `/health` answers `401` without a session. Every read route of SPEC-009, SPEC-010 and SPEC-022 answers `200` to each of the three roles.
- **dashapi_ac_002 (the session).** `GET /v1/auth/session` answers the session's user's id, email and role, and `401` without one.
- **dashapi_ac_003 (#102).** A login carrying the cookie of a live session deletes that session and issues another; a failed login carrying it leaves it live.
- **dashapi_ac_004 (an invitation).** `POST /v1/users` answers a token once; the database holds its SHA-256 and no column holds its text; `/v1/invitations/totp` answers the email and a provisioning URI whose secret is the same on a second call; `complete` with a wrong code answers `401` and writes a `denied` row, with the right one `204`, after which the user signs in with the chosen password and a code of that secret, and the invitation answers `401`. An expired, withdrawn or completed invitation, and a disabled user's, answer `401`; the sixth failure for one invitation within 15 minutes answers `429`. No route answers the admin the user's TOTP secret.
- **dashapi_ac_005 (the reset).** A reset clears the user's credentials, raises their `authority_version`, deletes their sessions, withdraws their open invitation and answers a new one, with `user.reset` and `invitation.issue`; a login with the old password fails; the user completes the new invitation and signs in. An admin's own reset answers `409 cannot_reset_self`, a disabled user's `409 user_disabled`.
- **dashapi_ac_006 (the users' lists).** `GET /v1/users` pages through 205 users, 200 and 5; `/assignable` lists the active admins and analysts with a confirmed factor, not a viewer, a disabled user or one with an open invitation.
- **dashapi_ac_007 (tokens).** A token for one use expires 15 minutes after its issue by the database's clock, and one for 3 uses and 24 hours, 24 hours after; `uses` 1001, `hours` 169, and `hours` with `uses` 1 answer `400`. The answer's token hashes to the row's `token_sha256`, and the list shows no token and no hash. A withdrawal answers `204`, a second `409 already_withdrawn`; `token.issue` and `token.withdraw` are written. A token the api issued enrolls an agent through ingest (SPEC-025 §Operational §1), which the list by `token_id` then shows.
- **dashapi_ac_008 (agents).** The list and the detail show §1's columns and no key or certificate; the filters by state and token hold. A revocation with a reason answers `204`, stores the database's time, and a second answers `409 already_revoked`; `agent.revoke` is written; the agent's next heartbeat answers `403 agent_revoked` (SPEC-025).
- **dashapi_ac_009 (an incident's status).** Each allowed transition answers `200`, with one `incident.status.change` row and one history row of the same time; a refused one, `409 transition_refused`; a stale `revision`, `409 incident_changed`, and the incident unchanged; a reopening while an open incident holds its key, `409 incident_superseded`. When the audit row cannot be written, the answer is `503`, and the status and the history are unchanged.
- **dashapi_ac_010 (an assignee).** An analyst assigns an incident to an admin, then to themself, then to no one: three `incident.assign` rows with their ids, three history rows, and the detail's `assignee_email` follows; a viewer's id, a disabled user's, an unknown one and another org's answer `400 invalid_assignee`.
- **dashapi_ac_011 (an alert's status).** `new` to `acknowledged` to `resolved` answers `200` twice, with the rows; `resolved` to `new` answers `409 transition_refused`.
- **dashapi_ac_012 (the audit view).** The filters by action, user, outcome and time hold; 205 rows page as 200 and 5 and more; each request writes `audit.read` with its filters before it answers; when that row cannot be written, the answer is `503` with no rows. The triage history pages, filters by object, and names the assignees' emails.
- **dashapi_ac_013 (the reads' additions).** An incident's detail and list item and an alert carry a `revision` that a triage write accepts; an incident's detail and its report carry the assignee's email.
- **deploy_ac_004, its rows for this SPEC.** As `cg_api`: `INSERT` into `enrollment_tokens` of §1's columns allowed, `SELECT token_sha256` refused, `SELECT` of its other columns allowed, `UPDATE` of `withdrawn_at` allowed and of `max_uses` refused; on `agents`, `SELECT` of §1's columns allowed, of `pubkey` and `cert_pem` refused, `UPDATE` of `revoked_at` and `revocation_reason` allowed and of `last_seen` refused, `INSERT` and `DELETE` refused; on `incidents`, `UPDATE` of `status`, `assigned_to` and `updated_at` allowed, and of `alert_ids`, `cg_mitre` and `severity_id` refused; on `alerts`, `UPDATE` of `status` and `updated_at` allowed, and of `source_events` refused; `INSERT` and `DELETE` on both refused; `SELECT` on `triage_history` allowed, and `INSERT`, `UPDATE` and `DELETE` on it refused; `SELECT` on `audit_log` allowed, `UPDATE` and `DELETE` refused; `SELECT`, `INSERT` and `UPDATE` on `invitations` allowed, `DELETE` refused; `SELECT` on `ca` refused. As `cg_ingest`: `UPDATE` of `incidents.status` refused, of `alert_ids` allowed. A status change made as `cg_api` writes a history row whose `changed_by_role` is `cg_api`. Each ingest table the api reads or writes is owned by `cg_ingest_owner`.
- **incident_ac_006 (a closed incident).** An incident closed as `resolved`, then a new alert of its key: a second incident with that key and the alert, the first unchanged; the notifier and the playbooks are called for the second. The same with `false_positive`. With the first `contained`, the alert joins it.
- **incident_ac_007 (the techniques).** Alerts with techniques `[T1]`, then `[T2, T1]`, then `[T3]`, grouped into one incident: its techniques are `[T1, T2, T3]`, and its tactics, status, assignee and title are unchanged.
- **dash_ac_009 (the mutations' path).** Each mutation handler refuses a cross-site request (the same-origin check), forwards the CSRF cookie as `x-csrf-token` and `X-Forwarded-For`, and answers `no-store`; the invitation's handlers send neither session nor CSRF token.
- **dash_ac_010 (the controls by role).** With a viewer's session, an incident's page shows the assignee and no triage control, no download and no entry for agents, tokens, users or the audit view; with an analyst's, the triage controls, the downloads and the agents' page, without revocation; with an admin's, all of them.
- **dash_ac_011 (the invitation page).** The page reads the token from the fragment, removes it from the address, shows the QR code and the URI, and completes the invitation; no request carries the token in a URL.
- **dash_ac_012 (shown once).** A new token and a new invitation are shown on the page that issued them and nowhere else, and a reload does not show them.

The api's criteria, the grants' and the dashboard's run in `ts-ci`. incident_ac_006 and incident_ac_007 change `services/ingest/src/detect/`, so the change that lands them, with migration `0011`, passes the elevated gate (CLAUDE.md, compensating control 4).

## Risks

- **An invitation intercepted before its user completes it** gives its holder the account, until an admin resets it; the reset's row and the user's lost access reveal it.
- **A notification and a playbook run for each new incident after a close**: an alert of a closed incident's key opens a new one, which notifies, as any creation does. An attack that continues after a close is notified again, which is the intent.
- **`audit.read` rows grow with each read of the view**, and `audit_log` has no retention (*Open questions*).
- **The triage history keeps users' ids and the time of each change** without a period, as `audit_log` does.
- **A reopening can meet an open incident of its key** and is refused; the alerts since the close are in the newer one.
- **The 30-second refresh** costs a read of the api every 30 seconds for each open list page.
- **Emails of admins and analysts are shown to analysts**, and an assignee's to viewers (Manuel's choice).
- **The dashboard-to-api leg is plain HTTP** on the host (debt #97).

## Open questions

1. **The retention of `audit_log` and of the triage history.** Kept without a period, as `docs/operations/data-protection.md` states for `audit_log`, against ADR-0003's one year in Postgres and seven in MinIO. A period would be a setting of the deployment, which is Manuel's. Reopened by roadmap step 9's retention and erasure position, or by a buyer's requirement.

## Decision record

Proposed by Claude Code on 2026-11-20, on Manuel's choices of that day (ADR-0022 and ADR-0014 Amendments 2026-11-20; `docs/handoff-session-87.md`, questions 11 to 18). Beyond them, each decision with what it discards:

1. **Grants to `cg_api` directly, not to a new group role.** A role is created by the database's initialisation, which reaches no installed stack (`deploy/docker/postgres/README.md`), and the owner roles cannot create one.
2. **A closed incident's key opens a new incident, by a unique index of the open incident.** Against reopening the closed one, which would undo a human's close and hide the alert's time; against a generation counter in the key, which needs a read before the write.
3. **Optimistic concurrency by `revision`, the row's `updated_at`.** Against a lock held across the analyst's reading: grouping bumps `updated_at`, so a close of an incident that took an alert since it was read is refused, and the analyst sees the alert.
4. **The history's actor is the database role; the human is the `audit_log` row of the same time.** Against a setting the api would place in the transaction, which a compromised api could forge, and the history is for that case.
5. **Invitations in an api table, not in Valkey.** Valkey is neither backed up nor restored, and a key under a new prefix would change its ACL (SPEC-023 deploy_ac_006).
6. **The TOTP secret generated for the invitation's holder, on their first request.** Against generating it with the user, which would show it to the admin (R14).
7. **`activity_id` stays `1`.** ADR-0022's Amendment grants no `UPDATE` of it, and the status is the triage state the pages read.
8. **The invitation lives 72 hours, a constant.** It spans a weekend; a setting would be deployment configuration.
9. **The token's text from the api's own code**, 32 random bytes in base64url hashed as SHA-256, the form SPEC-025 fixes. Against a package shared with ingest, a new component.
10. **Assignee validation in the api's transaction.** No foreign key crosses from ingest's schema to the api's, which `cg_ingest_owner` cannot reach.
11. **The phases of the pages follow their routes**, accounts first, since an installation needs its users before its triage.
12. **The dashboard's revocation before a login stays**, though the api now ends the cookie's session: it costs one request, and holds against a deployment whose api predates this SPEC.

## References

- [ADR-0014](../adr/0014-human-authentication-model.md), [ADR-0022](../adr/0022-single-node-production-deployment.md) — their Amendments 2026-11-20.
- [SPEC-007](SPEC-007-incident-grouping-mvp.md), [SPEC-008](SPEC-008-auth-core.md), [SPEC-009](SPEC-009-read-slice.md), [SPEC-023](SPEC-023-production-deployment.md), [SPEC-025](SPEC-025-agent-certificate-renewal-revocation.md).
- [Threat model](../security/threat-model.md) — R3, R7, R13 and R14; its Amendment 2026-11-20 (SPEC-026).
