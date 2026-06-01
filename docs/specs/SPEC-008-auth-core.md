# SPEC-008: Auth-core — local human authentication (password + TOTP), opaque Redis sessions, server-side RBAC

- **ID:** SPEC-008
- **Title:** Auth-core
- **Status:** Accepted
- **Depends on:** ADR-0014 (the human-auth MODEL this SPEC realises — local self-hosted identity, opaque-Redis sessions, new `services/api`, 3-role RBAC; not re-decided here), ADR-0003 (storage homes: users / RBAC / audit_log → Postgres, sessions + rate-limit → Redis), ADR-0001 (locates `services/api`), ADR-0002 (`services/api` = TypeScript + Fastify + Zod; Rule 4 strict schemas), SPEC-004 (the ingest scaffold reused as a pattern — `app.ts`, `config.ts`, `migrate.ts`; the enrollment-token issuance capability this SPEC's RBAC authorizes), the [threat model](../security/threat-model.md) § `cg-api` (the binding auth-requirement surface)
- **Authors:** Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)
- **Created:** 2026-06-01
- **Last updated:** 2026-06-01

## Motivation

ADR-0014 fixed the human-authentication *model*; this SPEC realises its *mechanics*. It is the first half of Phase 6 C (the user-facing slice), split from the read-slice (SPEC-009) by risk: **auth-core must exist before any read endpoint can enforce RBAC.** SPEC-009 (incident/alert list + detail + drill) cannot authorize a `viewer`-vs-`analyst` read without the identity, session, and role-enforcement this SPEC provides — hence 008 precedes 009.

SPEC-008 re-decides nothing from ADR-0014. It inherits and realises: local identity (password hashed with Argon2, TOTP RFC 6238 every login — ADR-0014 §1); the opaque server-side session token in Redis whose **immediate revocability is a product invariant** (ADR-0014 §2); the new `services/api` component, distinct from the agent-mTLS `services/ingest` trust boundary (ADR-0014 §3); and the three fixed roles `admin`/`analyst`/`viewer` enforced server-side (ADR-0014 §4). It derives the verifiable acceptance criteria, the `users`/`audit_log` schemas, the Redis session shape, the login/logout/rotation/revocation contracts, the rate-limit and CSRF controls ADR-0014 §Compliance bound, and the RBAC capability matrix for the auth-core surface.

The MVP narrative this enables (Blueprint §18, [blueprint.md:738](../product/blueprint.md#L738)): *"An administrator … creates a user with OTP. From the dashboard, generates an enrollment token."* — the human identity + the authorization gate around the existing agent-enrollment-token issuance.

## Scope

### In scope

In dependency order (each builds on the prior):

1. **`users` table + the role-on-record model** (migration `0001_users`, owned by `services/api` — §Operational §7). Local identity: `password_hash` (Argon2), `totp_secret` encrypted at rest (pgcrypto, the `ca` precedent), `role` as a `CHECK`-constrained column (ADR-0014 §4: "each user carrying one role on the user record" — no separate role table for three fixed global roles). §Data contracts §1.
2. **`audit_log` table** (migration `0002_audit_log`, owned by `services/api`), append-only, with the threat-model's fields ([threat-model.md:83](../security/threat-model.md#L83)). Every auth event + every role change writes a row (the role-assignment trail, [threat-model.md:86](../security/threat-model.md#L86)). §Data contracts §2.
3. **The Redis session contract** (opaque token, TTL, payload, rotation, immediate revocation). Sessions are **Redis-only**, never a Postgres table (ADR-0003 §Decision; ADR-0014 §2). §Data contracts §3.
4. **The login flow**: password (Argon2 verify) → TOTP (RFC 6238 verify, replay-rejected) → opaque session issued + `HttpOnly`/`Secure`/`SameSite=Strict` cookie + CSRF token. §Operational §1.
5. **Session rotation + immediate revocation** (the product invariant): rotate on login and on role change; revoke = Redis `DEL`, effective on the next request with no grace. §Operational §2.
6. **Rate-limiting** per account **and** per source IP, progressive back-off, Redis counters (ADR-0014 §Compliance MUST; [threat-model.md:81](../security/threat-model.md#L81)). §Operational §3.
7. **CSRF defense** — Fastify-issued token on every mutation (ADR-0014 deferred CSRF here explicitly; this SPEC is its named destination). §Operational §4. **(Decision: included, not re-deferred — see §Operational §4 and Ratification §6.)**
8. **Server-side RBAC enforcement** + the **auth-core capability matrix** (user management, role assignment, enrollment-token issuance authorization). §Operational §5.
9. **First-admin bootstrap** via a CLI, mirroring `cli/issue-token.ts` (no chicken-and-egg HTTP path for the first user). §Operational §6.
10. **Acceptance criteria** (harness-first RED at the next gate): the nine ACs in §Acceptance criteria, the RBAC one CI-blocking per [threat-model.md:84-86](../security/threat-model.md#L84-L86).

### Out of scope

Each with a named destination:

- **All incident/alert reads** — the list, the incident detail with grouped alerts + MITRE, and the alert→source-event drill (with its `event_id` v4/v7 reconciliation, ADR-0009/0011 domain). **SPEC-009** (the read-slice). The RBAC *matrix rows* for those read capabilities are SPEC-009's; this SPEC defines only the auth-core capability rows (§Operational §5).
- **The dashboard UI** (Next.js views, WebSocket push of alerts). **SPEC-009** + the dashboard SPEC; Blueprint §13.
- **Notification / email**, and therefore **password-reset / account-recovery by email** and **email-delivered invites**. The notifier slice (Blueprint §16, SMTP/Gmail; needs credentials, ask-first). TOTP enrollment that needs no delivery (on-screen QR at first login) stays in scope.
- **Federated OIDC / external IdP.** Closed by ADR-0014 §1 (local self-hosted identity). Not revisited.
- **WebAuthn / passkeys.** Enhancement target, not MVP ([threat-model.md:88](../security/threat-model.md#L88)).
- **Multi-tenant / org-scoped RBAC.** Single `org_id` for the MVP (ADR-0003 Rule 2; Blueprint §17 item 1); the three roles are global within the single org.
- **Service-account / machine-to-API tokens** (distinct from human login). A later phase.
- **The HTTP wrapper that issues enrollment tokens.** The *capability* (admin-only) is defined in the RBAC matrix here so enforcement is designed; the *endpoint* touches `enrollment_tokens` (an ingest-domain table) and lands with the agents-management view (SPEC-009 era). §Operational §5 names the boundary.
- **Agent identity** (mTLS, X.509, DPAPI). SPEC-002/003; a different trust boundary (ADR-0014 §3).

## Data contracts

### 1. `users` table (migration `0001_users`, owned by `services/api`)

Per ADR-0003 §Decision (users/RBAC → Postgres) and ADR-0014 §1/§4. The auth migrations are **owned by `services/api`** under its **own** runner and numbering, starting at `0001_users` (`services/api` is README-only today, so `0001` is free in its tree) — **not** appended to the `services/ingest` chain. The api runner mirrors the ingest applier pattern (`services/ingest/src/db/migrate.ts:49-84`) but with its **own** Kysely migration-ledger table and its **own** advisory-lock key, against the same Postgres instance (ADR-0003, one instance). The ownership decision and the runner contract are in §Operational §7 (Ratification §1).

```sql
CREATE TABLE IF NOT EXISTS users (
  user_id        uuid        PRIMARY KEY,                 -- UUIDv7, slice-generated (like alert_id/incident_id)
  org_id         text        NOT NULL DEFAULT 'default',
  email          text        NOT NULL,                    -- login identifier (citext-or-lower-normalized; see note)
  password_hash  text        NOT NULL,                    -- Argon2id encoded string (algorithm + params + salt + hash)
  totp_secret    bytea       NOT NULL,                    -- pgcrypto pgp_sym_encrypt(<base32 secret>) — NEVER plaintext
  totp_enrolled  boolean     NOT NULL DEFAULT false,      -- false until first successful TOTP confirmation
  role           text        NOT NULL
                   CHECK (role IN ('admin','analyst','viewer')),   -- role-on-record (ADR-0014 §4); mirrors alerts.status CHECK
  status         text        NOT NULL DEFAULT 'active'
                   CHECK (status IN ('active','disabled')),
  created_at     timestamptz NOT NULL DEFAULT now(),
  updated_at     timestamptz NOT NULL DEFAULT now(),
  last_login_at  timestamptz,
  CONSTRAINT users_email_org_unique UNIQUE (org_id, email)
);
```

- **`password_hash`** — an Argon2id encoded string (the threat model says "Argon2 or bcrypt", [threat-model.md:81](../security/threat-model.md#L81); ADR-0014 §1 picks Argon2). The KDF parameters (memory/iterations/parallelism) are NFR-008-001, not the column.
- **`totp_secret`** — the base32 TOTP secret, **encrypted at rest** with pgcrypto `pgp_sym_encrypt` under a service passphrase, mirroring the CA private key precedent **in code** (`services/ingest/src/ca.ts:85-88` encrypt, `:99-103` decrypt). Because `services/api` owns its migrations independently (§Operational §7), its `0001_users` ensures the extension itself with `CREATE EXTENSION IF NOT EXISTS pgcrypto` (the same idempotent statement ingest's `0001_initial.ts:7` uses), so api does not depend on ingest having run first. The passphrase is a new `services/api` config var following the `INGEST_CA_PASSPHRASE` pattern (`services/ingest/src/config.ts:18`). It is a `TOTP secret` asset the threat model lists at risk ([threat-model.md:77](../security/threat-model.md#L77)).
- **`role`** — a `CHECK`-constrained column, not a join table: ADR-0014 §4 binds one role per user on the record, and three fixed global roles do not warrant a roles/role_assignments pair (mirrors the `alerts.status` CHECK pattern, `0002_alerts.ts:41-42`). The role-*change* history lives in `audit_log` (§2), satisfying the role-assignment trail.
- **`email`** normalization (lower-cased / `citext`) is an implementation choice for the RED→GREEN gate; the contract is case-insensitive uniqueness per org.

### 2. `audit_log` table (migration `0002_audit_log`, owned by `services/api`, append-only)

Per ADR-0003 §Retention (audit log → Postgres append-only) and the threat model's required fields verbatim ([threat-model.md:83](../security/threat-model.md#L83): "an audit log with `user_id`, `org_id`, `action`, `target_id`, `timestamp`, and the request correlation id, written append-only to Postgres").

```sql
CREATE TABLE IF NOT EXISTS audit_log (
  id             bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  occurred_at    timestamptz NOT NULL DEFAULT now(),       -- the "timestamp"
  user_id        uuid,                                     -- the acting principal (NULL for a failed login w/ unknown user)
  org_id         text        NOT NULL DEFAULT 'default',
  action         text        NOT NULL,                     -- e.g. auth.login.ok / auth.login.fail / auth.logout / user.create / role.change / token.issue / session.revoke
  target_id      text,                                     -- the object acted on (user_id, token id, …)
  outcome        text        NOT NULL CHECK (outcome IN ('ok','denied','error')),
  source_ip      inet,
  correlation_id uuid                                      -- the request correlation id
);
```

- **Append-only is a contract, not just a column shape:** the service path performs INSERT only; no UPDATE/DELETE on `audit_log`. (Hard DB-level immutability — a revoke-UPDATE/DELETE grant or a rule — is an implementation hardening for the gate; the contract here is write-only.)
- **What MUST be logged** (every auth-relevant event, [threat-model.md:84/:86](../security/threat-model.md#L84)): login success + failure, logout, session revoke, user create/update/disable, role change (the assignment trail), enrollment-token issuance authorization.

### 3. Redis session entry (opaque, Redis-only — ADR-0014 §2)

Sessions live **only** in Redis (ADR-0003 §Decision; never a Postgres table). `services/api` is the first code consumer of Redis — the URL is already a required, validated config var (`services/ingest/src/config.ts:13`, `INGEST_REDIS_URL`) and Redis is provisioned (`infra/dev/docker-compose.dev.yml:55`), but no code reads it yet; the api config adds its own `*_REDIS_URL` following the same Zod pattern (`config.ts:7-24`).

```text
key    = "cgsess:<token>"
token  = base64url(32 random bytes from a CSPRNG)   -- mirrors cli/issue-token.ts:23-25 and ca.ts randomSerial
value  = JSON { user_id, org_id, role, csrf_token, issued_at, last_seen }
TTL    = idle timeout (sliding), with an absolute lifetime cap   -- values = NFR-008-002
```

- **Opacity (and the "signed session" reconciliation):** the token carries no client-trusted claims; all authority (`role`, `org_id`) is read server-side from this entry per request (ADR-0014 §2). The threat model's *"signed sessions"* ([threat-model.md:81](../security/threat-model.md#L81)) is realised by this opacity + server-side validation rather than a signed-cookie wrapper — an opaque high-entropy token's unforgeability comes from CSPRNG entropy plus the server-side lookup, so **no separate cookie signature/MAC is used**. This discharges the cookie-signing mechanism ADR-0014 §2 explicitly handed to this SPEC, and is stronger on the revocability axis the threat model cares about (a signed-stateless cookie cannot be revoked immediately).
- **Rotation:** a new `token` (new key) is issued on login and on role change; the old key is `DEL`-eted (§Operational §2).
- **Revocation (the product invariant):** logout and admin-initiated revoke `DEL` the key; the next request bearing that cookie finds no entry → `401`, with **no grace window** (auth_ac_005).
- **`csrf_token`** is bound to the session entry (§Operational §4).

## Operational

### 1. Login flow (contract, not code)

`POST /v1/auth/login` body `{ email, password, totp_code }` (Zod-validated, ADR-0002 Rule 4 — no extra fields):

1. Look up the active user by `(org_id, email)`. **Generic failure for every negative branch** (unknown user, wrong password, wrong/replayed TOTP, disabled user) → `401` with a single opaque message — no user-enumeration, no "which factor failed" oracle (NFR-008-003).
2. Verify `password` against `password_hash` (Argon2 verify).
3. Verify `totp_code` against the decrypted `totp_secret` with an RFC 6238 TOTP verifier (the library is a gate choice), **rejecting replay**: a code already accepted within its time-step is refused (last-accepted step tracked per user in Redis).
4. On success: create the Redis session (§Data contracts §3), set cookie `Set-Cookie: cgsess=<token>; HttpOnly; Secure; SameSite=Strict; Path=/` ([threat-model.md:186](../security/threat-model.md#L186)), issue the CSRF token (§4), write `audit_log` `auth.login.ok`, bump `users.last_login_at`.
5. On failure: increment rate-limit counters (§3), write `audit_log` `auth.login.fail`.

`POST /v1/auth/logout` → `DEL` the session key (immediate revocation, §2), clear the cookie, audit `auth.logout`.

### 2. Rotation and revocation (product invariant)

- **Rotation:** issue a fresh session token (new Redis key) and `DEL` the prior on every login and on any **role change** for that user (a privilege change must not ride an old session). Mitigates session fixation.
- **Revocation = immediate:** revocation is a Redis `DEL`; because every request resolves authority by reading the session entry, a deleted entry fails the **very next** request with `401`. This immediacy is the deciding property behind ADR-0014 §2's opaque-Redis choice; it is an **invariant, not a tunable** (auth_ac_005). Triggers: logout, admin revoke, role change (old sessions revoked), account disable.

### 3. Rate-limiting (MUST — ADR-0014 §Compliance)

Login is rate-limited on **two independent dimensions** ([threat-model.md:81](../security/threat-model.md#L81): "per account and per source IP"):

- **Per account:** progressive back-off after consecutive failures for one `(org_id, email)`, escalating to a temporary lockout.
- **Per source IP:** throttle across accounts from one IP (credential-stuffing defense).
- **Home:** Redis counters (`INCR` + TTL windows), per ADR-0003 §Decision ("rate limit → Redis"). The concrete thresholds and back-off curve are NFR-008-004, per-org configurable (the surface ADR-0014 §Out of scope deferred to this SPEC). Both dimensions are asserted by auth_ac_004.

### 4. CSRF (DECISION: included here)

ADR-0014 left CSRF in §Out of scope with destination "the C auth SPEC" — **this SPEC is that destination**, and it introduces the first state-mutating human endpoints, so CSRF is realised here rather than re-deferred. Contract: on login the server issues a CSRF token bound to the session entry (`csrf_token`, §Data contracts §3); **every mutating request** (`POST`/`PUT`/`PATCH`/`DELETE`) MUST present it (Fastify-issued token, [threat-model.md:82](../security/threat-model.md#L82) / [:184](../security/threat-model.md#L184)) in a request header, validated server-side against the session's `csrf_token`. `SameSite=Strict` on the session cookie is **partial** mitigation (named, not relied on alone). The token-delivery mechanism (readable cookie double-submit vs body/header synchronizer) is an implementation choice for the gate; the contract is "session-bound CSRF token required on every mutation" (auth_ac_007).

### 5. RBAC — server-side enforcement + auth-core capability matrix

Enforcement is server-side on every auth-core operation — and, by the same mechanism SPEC-009 inherits for its read surface, on every read and write (ADR-0014 §4; [threat-model.md:84](../security/threat-model.md#L84) object-level authz). The only read SPEC-008 itself owns is *list users*; the read-authz surface for incidents/alerts is SPEC-009's. The client is never trusted for role: the role is read from the session entry, never from the request body. The matrix below covers **only the auth-core surface**; the incident/alert read capabilities are **SPEC-009's matrix rows** (named, not defined here).

| Capability | admin | analyst | viewer |
| --- | --- | --- | --- |
| Log in / log out / own session | ✅ | ✅ | ✅ |
| Manage own credentials (password change, TOTP setup) | ✅ | ✅ | ✅ |
| Create / disable / list users | ✅ | ❌ | ❌ |
| Assign / change a user's role | ✅ | ❌ | ❌ |
| Authorize enrollment-token issuance | ✅ | ❌ | ❌ |

- **Enrollment-token issuance** is authorized to `admin` (Blueprint §18: "An administrator … generates an enrollment token"). The *capability + its RBAC* are defined here; the *endpoint* wraps the existing `cli/issue-token.ts` logic (`:23-34`), which SPEC-004 deliberately left CLI-only ("No HTTP admin endpoint in SPEC-004", `cli/issue-token.ts:1-4`) **because no auth existed** — SPEC-008 supplies the auth that makes an authed HTTP wrapper safe. Because that wrapper writes `enrollment_tokens` (an ingest-domain table), the endpoint itself lands with the agents view (SPEC-009 era); this SPEC fixes only that the capability is admin-gated. The SPEC that lands the endpoint records a SPEC-004 §Ratification-decision-2 amendment ("CLI-only in SPEC-004" superseded once an authed HTTP wrapper ships) so the supersession is not lost.
- **Server-side-enforcement test is CI-blocking** ([threat-model.md:84-86](../security/threat-model.md#L84)): auth_ac_003.

### 6. First-admin bootstrap

The MVP starts with no users, but user creation is admin-only — a chicken-and-egg. Resolved the same way agent enrollment-token issuance is: a **CLI** (`services/api/src/cli/create-user.ts`, mirroring `services/ingest/src/cli/issue-token.ts`) seeds the first `admin` directly in Postgres (Argon2 hash + encrypted TOTP secret + printed TOTP provisioning URI once to stdout, never logged). After the first admin exists, all further user creation is via the authed admin API. No unauthenticated HTTP bootstrap path ever exists.

### 7. Migration ownership — `services/api` owns its auth migrations (own runner)

The `users`/`audit_log` migrations are **owned by `services/api`**: they live under `services/api/src/db/migrations/` numbered from `0001` (`0001_users`, `0002_audit_log`), applied by **`services/api`'s own** migration runner — a mirror of the ingest applier pattern (`migrate.ts:49-84`) but with its **own** Kysely migration-ledger table and its **own** advisory-lock key, so the two services' migration histories never collide on the shared Postgres instance (ADR-0003, one instance). The api runner is invoked at api startup behind an `API_RUN_MIGRATIONS` flag (mirroring `INGEST_RUN_MIGRATIONS`, `config.ts:19`) and is **self-contained** — its `0001` ensures `pgcrypto` itself (`CREATE EXTENSION IF NOT EXISTS`, idempotent), so api does not depend on ingest having run first. **Rationale (Manuel, Session 19 gate — Option A):** `services/api` is a component in its own right (ADR-0014 §3, ADR-0001); its schema — and especially the human-secret DDL (`password_hash`, `totp_secret`) — must not live in the agent-facing `services/ingest` tree. This **extends ADR-0014 §3's component separation from a runtime-only boundary to deploy-time (DDL) as well** — it realises §3 more fully and contradicts no Accepted ADR: ADR-0003's single-instance rule is preserved (two runners, one database), and ADR-0007 makes ingest run *its* migrations, not *all* migrations. **Rejected alternative:** appending to the shared `services/ingest` chain (`0006`/`0007`) under the existing runner — fewer moving parts by one runner, but it puts human-secret DDL in the agent-facing service's tree and couples api's schema readiness to an ingest-owned step.

### 8. Credential self-management (TOTP first-enrollment + password change)

Two self-service operations drive the `users.totp_enrolled` state and the "manage own credentials" matrix row (§5); both keep the no-delivery posture ADR-0014 §Out of scope retains for TOTP enrollment (on-screen, never emailed):

- **TOTP first-enrollment confirmation.** A user created by the bootstrap CLI (§6) or by an admin has `totp_secret` set but `totp_enrolled = false`. The provisioning URI / QR for that secret is shown **on screen** (returned to the admin who created the user, or at first sign-in) — never emailed. The user confirms with one valid current code; on success the server flips `totp_enrolled` → `true` and audits `user.totp.enroll`. Login MUST require a confirmed factor: an account with `totp_enrolled = false` completes enrollment before any session is issued. Asserted by auth_ac_009.
- **Authenticated password change.** An authenticated user changes their own password via `POST /v1/auth/password` presenting the current password + a current TOTP code + the new password; the server re-hashes (Argon2id), **rotates the session** (§2), and audits `user.password.change`. Distinct from **password reset / recovery**, which is email-dependent and stays out of scope (the notifier slice, §Out of scope).

## Non-functional requirements

NFR identifiers scoped to this SPEC (`NFR-008-NNN`).

- **NFR-008-001 (password KDF).** Argon2id with parameters at or above current OWASP guidance; per-user random salt; parameters revisable. (Algorithm fixed by ADR-0014 §1; parameter values are the tunable.)
- **NFR-008-002 (session lifetime).** Sliding idle TTL with an absolute cap; recommended default idle 8 h / absolute 24 h, per-org configurable. The cap bounds a stolen-cookie window; revocation (§2) handles the immediate case.
- **NFR-008-003 (no auth oracle).** Every login-failure branch returns one indistinguishable response (timing-safe password compare; constant work even for unknown users) — no user-enumeration, no per-factor oracle.
- **NFR-008-004 (rate-limit policy).** Per-account and per-IP thresholds with progressive back-off; recommended default e.g. account lockout after 5 consecutive failures with exponential back-off, IP throttle on sustained failures; per-org configurable.
- **NFR-008-005 (revocation immediacy).** A revoked session MUST fail the next request; no positive cache of session validity may outlive the `DEL` (read-through per request).

## Security considerations

This SPEC handles the project's first *human* secret material (the agent secret-handling template is SPEC-002 §Security). Inherited posture:

- **Secrets at rest:** password as Argon2id hash (never reversible); TOTP secret pgcrypto-encrypted (`ca.ts:85-103` precedent), never plaintext in a column, never logged. The pgcrypto passphrase is config, not in the DB.
- **Secrets in transit / browser:** session token only in an `HttpOnly` `Secure` `SameSite=Strict` cookie ([threat-model.md:186](../security/threat-model.md#L186)); never in URL or body; `Cache-Control: no-store` on authed responses.
- **Session fixation / hijack:** rotation on login + privilege change (§2); opacity + server-side authority (§Data contracts §3).
- **Brute force / stuffing:** dual-dimension rate-limit (§3).
- **CSRF:** session-bound token on every mutation (§4).
- **Audit:** every auth event + role change is an append-only `audit_log` row (§Data contracts §2).
- **Accepted / deferred:** WebAuthn step-up ([threat-model.md:88](../security/threat-model.md#L88)); email-based recovery (notifier slice). A root-level local DB compromise reading the pgcrypto passphrase is outside this boundary (consistent with the threat model's accepted local-compromise posture).

## Acceptance criteria

Each AC maps 1:1 to a test under `services/api/test/`: file `auth-ac-NNN-<slug>.test.ts`, logical id `auth_ac_NNN` (mirroring `incident_ac_NNN`/`detect_ac_NNN`). TypeScript/vitest; **all CI-able on Linux `ts-ci`** (synthetic users/sessions via testcontainers Postgres + Redis — auth-core needs **no ETW and no agent**, so SPEC-008 has no developer-local marquee). `ts-ci`'s scope extends to `services/api/` (CLAUDE.md §Local pre-commit gate names "the future `services/api/`"). The harness-first RED phase (next gate) turns `ts-ci` red with the Known CI debt co-located in that SHA (Convention #13), green when the impl lands.

| AC | Gate | Why |
| --- | --- | --- |
| auth_ac_001 | **CI-able** | valid login (password + current TOTP) → session issued, cookie + CSRF set |
| auth_ac_002 | **CI-able** | invalid login (bad password / bad TOTP / replayed TOTP) → rejected, generic, no session |
| auth_ac_003 | **CI-able (blocking)** | RBAC: non-admin → 403 server-side; admin → allowed (threat-model:84-86) |
| auth_ac_004 | **CI-able** | rate-limit per account AND per IP |
| auth_ac_005 | **CI-able** | revocation immediacy: revoke → next request 401, no grace |
| auth_ac_006 | **CI-able** | audit_log append-only row on an auth event |
| auth_ac_007 | **CI-able** | CSRF: mutation without valid token → 403; with token → allowed |
| auth_ac_008 | **CI-able** | secrets at rest: password is Argon2id, totp_secret is ciphertext |
| auth_ac_009 | **CI-able** | TOTP first-enrollment: confirm a code → `totp_enrolled` flips false→true |

- **auth_ac_001 (valid login).** Given an active user with a known password and an enrolled TOTP secret, when `POST /v1/auth/login` receives the correct password and the current TOTP code, then the response is `200`, sets a `cgsess` cookie with `HttpOnly`+`Secure`+`SameSite=Strict`, a Redis `cgsess:<token>` entry exists with the user's `role`/`org_id`, and a CSRF token is issued. **CI-able.**
- **auth_ac_002 (invalid login rejected — three sub-cases).** (a) wrong password, (b) wrong TOTP, (c) a TOTP code already accepted within its time-step (replay) → each returns the **same** generic `401`, creates **no** session, and writes an `auth.login.fail` audit row. No response distinguishes the sub-cases (NFR-008-003). **CI-able.**
- **auth_ac_003 (RBAC server-side — CI-blocking).** Given an authenticated `viewer` (and an `analyst`), when they call an admin-only capability (create user / change role), then the response is `403` enforced server-side (role read from the session, not the request); given an `admin`, the same call succeeds. **Production-faithful (Convention #12):** the passing case is achieved by giving the principal the `admin` role, **never** by weakening the check — the analogue of SPEC-007's `enrollTestAgent`-not-drop-the-FK. This test **blocks CI** ([threat-model.md:84-86](../security/threat-model.md#L84)). **CI-able.**
- **auth_ac_004 (rate-limit, two dimensions).** (a) N consecutive failed logins for one account → that account is throttled/locked while a different account from the same context still proceeds; (b) sustained failures from one source IP across multiple accounts → that IP is throttled. Both dimensions asserted. **CI-able.**
- **auth_ac_005 (revocation immediacy — product invariant).** Given an active session, when it is revoked (logout or admin revoke = Redis `DEL`), then the **next** request bearing that cookie returns `401` with no grace. **CI-able.**
- **auth_ac_006 (audit append-only).** Given an auth event (a login, a role change), then an `audit_log` row exists with `user_id`/`org_id`/`action`/`target_id`/`occurred_at`/`correlation_id`; the service exposes no update/delete path for `audit_log`. **CI-able.**
- **auth_ac_007 (CSRF).** Given an authenticated session, when a mutating request omits or presents a wrong CSRF token → `403`; when it presents the session-bound token → allowed. **CI-able.**
- **auth_ac_008 (secrets at rest).** Given a created user, when its row is read directly, then `password_hash` is an Argon2id encoding (not the plaintext password) and `totp_secret` is pgcrypto ciphertext (not the plaintext base32 secret). **CI-able.**
- **auth_ac_009 (TOTP first-enrollment confirmation).** Given a user with `totp_secret` set and `totp_enrolled = false`, when the user submits a valid current TOTP code to the enrollment-confirmation operation, then `totp_enrolled` flips to `true` and a `user.totp.enroll` audit row is written; a login attempt before confirmation yields no session. **CI-able.** (Drives the `totp_enrolled` state §Data contracts §1 introduces — no unreachable state field.)

**Migration coverage note.** A `0001_users` migration test verifies the table, the `role` CHECK, the `(org_id,email)` UNIQUE, and `NOT NULL`s; a `0002_audit_log` test verifies the append-only shape and the `outcome` CHECK. Both CI-able, against the `services/api` runner.

## Test scenarios

Per ADR-0005 §Harness obligation.

- **SC-AUTH-001 — happy-path login.** Input: correct password + current TOTP. Expected: session + cookie + CSRF. Realized by auth_ac_001.
- **SC-AUTH-002 — credential attacks.** Input: wrong password / wrong TOTP / replayed TOTP / unknown user. Expected: indistinguishable `401`, counters incremented. Realized by auth_ac_002 + auth_ac_004.
- **SC-AUTH-003 — privilege boundary.** Input: viewer/analyst attempt an admin capability. Expected: `403` server-side. Realized by auth_ac_003.
- **SC-AUTH-004 — kill switch.** Input: revoke an active session. Expected: immediate `401` on next request. Realized by auth_ac_005.

## Risks

| Risk | Mitigation |
| --- | --- |
| Two migration runners (`services/ingest` + `services/api`) against one Postgres instance | Each uses its **own** Kysely ledger table + **own** advisory-lock key, so the two histories never collide (§Operational §7); ratified Option A — ownership separation |
| Redis unavailable ⇒ no auth (sessions + rate-limit are Redis-only) | Redis is already a hard stack dependency (ADR-0003); acceptable — the same posture as the rest of the platform; surfaced as an operational dependency |
| TOTP replay within a time-step window | Per-user last-accepted-step tracked in Redis (§Operational §1.3); auth_ac_002(c) |
| Enrollment-token-issuance endpoint writes an ingest-domain table (`enrollment_tokens`) across the boundary | Capability + RBAC defined here; endpoint deferred to the agents view; cross-boundary write flagged (§Operational §5) |
| `services/api` is the first Redis consumer in code | URL already a required config var + provisioned (`config.ts:13`, compose:55); only the client wiring is new |
| Argon2 cost vs login latency | NFR-008-001 parameters tunable; cost is per-login only |

## Open questions

1. **Migration ownership — RESOLVED (Manuel, Session 19 gate): Option A.** `services/api` owns its auth migrations (`0001_users`/`0002_audit_log`) under its own runner — not the shared ingest chain. Rationale: `services/api` is a component in its own right (ADR-0014 §3 / ADR-0001), so its DDL — especially the human-secret tables (`password_hash`, `totp_secret`) — must not live in the agent-facing service's tree. See §Operational §7.
2. **Session lifetime defaults.** Idle 8 h / absolute 24 h (NFR-008-002). **Recommendation: those defaults, per-org configurable.**
3. **Rate-limit thresholds.** Account lockout after 5 failures + IP throttle (NFR-008-004). **Recommendation: those defaults, per-org configurable.**
4. **CSRF token delivery mechanism.** Double-submit cookie vs synchronizer header (§Operational §4). **Recommendation: defer the mechanism to the RED→GREEN gate; the contract (session-bound token on every mutation) is fixed.**

## Ratification record

Load-bearing decisions for Manuel's gate (recommended-default-and-rationale pattern per SPEC-005/006/007).

1. **Migration ownership = `services/api` owns its auth migrations (`0001_users`/`0002_audit_log`) under its own runner** (own ledger table + advisory-lock key, same Postgres instance). Ratified Option A (Manuel, Session 19 gate): ownership separation — human-secret DDL lives in the api component's tree, not the agent-facing ingest tree (ADR-0014 §3 / ADR-0001). The shared-ingest-chain alternative is rejected.
2. **Sessions = opaque token in Redis, revocation = immediate `DEL` (invariant).** Realises ADR-0014 §2; the next request after `DEL` fails (auth_ac_005).
3. **Password = Argon2id; TOTP secret pgcrypto-encrypted at rest.** Realises ADR-0014 §1; reuses the `ca.ts` pgcrypto precedent.
4. **Role = CHECK-constrained column on `users`, not a role table.** Realises ADR-0014 §4 ("role on the user record"); three fixed global roles; role-change trail in `audit_log`.
5. **RBAC enforced server-side; the enforcement test blocks CI** ([threat-model.md:84-86](../security/threat-model.md#L84)); production-faithful (Convention #12).
6. **CSRF included in SPEC-008** (its ADR-0014-named destination), not re-deferred; session-bound token on every mutation.
7. **Rate-limit is a MUST on two dimensions** (account + IP), Redis-homed; thresholds per-org configurable.
8. **First-admin via CLI** (`create-user.ts`, mirroring `issue-token.ts`); no unauthenticated HTTP bootstrap.

## References

- [ADR-0014](../adr/0014-human-authentication-model.md) — the human-auth model this SPEC realises (identity §1, session §2, `services/api` §3, RBAC §4); the §Compliance MUSTs (rate-limit, cookies, CI-blocking RBAC, audit) and the §Out of scope CSRF deferral this SPEC closes.
- [ADR-0003](../adr/0003-polyglot-storage.md) — storage homes: users/RBAC/audit → Postgres (§Decision / §Retention), sessions + rate-limit → Redis (§Decision). No amendment (already routed).
- [ADR-0001](../adr/0001-monorepo-layout.md) / [ADR-0002](../adr/0002-language-per-component.md) — `services/api` location + TypeScript/Fastify/Zod (`0002` §Context + §Decision row, Rule 4 strict schemas).
- [SPEC-004](SPEC-004-server-ingest-minimal.md) — the ingest scaffold reused as a pattern (`app.ts`, `config.ts`, `migrate.ts`); `cli/issue-token.ts` (the opaque-token CSPRNG pattern and the enrollment-token issuance this SPEC's RBAC authorizes — CLI-only until auth existed).
- [SPEC-002](SPEC-002-agent-enrollment.md) — agent identity (mTLS/DPAPI); the secret-handling template; the *agent* trust boundary distinct from this human one.
- [SPEC-009 (forthcoming)] — the read-slice (incident/alert list + detail + drill) gated behind this SPEC's auth; owns the incident/alert RBAC matrix rows.
- [Threat model](../security/threat-model.md) — § `cg-api` (`:81` Argon2/TOTP/sessions/rate-limit, `:82` CSRF, `:83` audit fields, `:84-86` RBAC CI tests + audit/role trail) and § Dashboard (`:184` CSRF, `:186` cookies, `:88` WebAuthn deferred) — the binding requirement surface.
- `services/ingest/src/ca.ts` — the pgcrypto `pgp_sym_encrypt`/`decrypt` at-rest precedent (`:85-103`) the TOTP-secret encryption mirrors.
- `services/ingest/src/config.ts` — the Zod env-validation pattern (`:7-24`); `INGEST_REDIS_URL` already required (`:13`); `INGEST_CA_PASSPHRASE` the secret-passphrase precedent (`:18`).
- `services/ingest/src/db/migrate.ts` — the advisory-lock migration applier (`:18`, `:49-84`) **whose pattern `services/api`'s own migration runner mirrors** (its own ledger table + advisory-lock key; §Operational §7). The auth migrations do not extend the ingest chain.
- [Blueprint](../product/blueprint.md) §18 (the MVP "creates a user with OTP" + admin "generates an enrollment token", `:738`/`:746`), §10 (SOAR; the richer role set deferred), §17 (single-org MVP).
