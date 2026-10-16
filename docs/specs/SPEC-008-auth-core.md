# SPEC-008: Auth-core — local human authentication (password + TOTP), opaque Redis sessions, server-side RBAC

- **ID:** SPEC-008
- **Title:** Auth-core
- **Status:** Accepted
- **Depends on:** ADR-0014 (the human-auth MODEL this SPEC realises — local self-hosted identity, opaque-Redis sessions, new `services/api`, 3-role RBAC; not re-decided here), ADR-0003 (storage homes: users / RBAC / audit_log → Postgres, sessions + rate-limit → Redis), ADR-0001 (locates `services/api`), ADR-0002 (`services/api` = TypeScript + Fastify + Zod, its language table row; strict schemas are the threat model's § cg-api, Tampering), SPEC-004 (the ingest scaffold reused as a pattern — `app.ts`, `config.ts`, `migrate.ts`; the enrollment-token issuance capability this SPEC's RBAC authorizes), the [threat model](../security/threat-model.md) § `cg-api` (the binding auth-requirement surface)
- **Authors:** Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)
- **Created:** 2026-06-01
- **Last updated:** 2026-06-01

## Motivation

ADR-0014 fixed the human-authentication *model*; this SPEC realises its *mechanics*. It is the first half of Phase 6 C (the user-facing slice), split from the read-slice (SPEC-009) by risk: **auth-core must exist before any read endpoint can enforce RBAC.** SPEC-009 (incident/alert list + detail + drill) cannot authorize a `viewer`-vs-`analyst` read without the identity, session, and role-enforcement this SPEC provides — hence 008 precedes 009.

SPEC-008 re-decides nothing from ADR-0014. It inherits and realises: local identity (password hashed with Argon2, TOTP RFC 6238 every login — ADR-0014 §1); the opaque server-side session token in Redis whose **immediate revocability is a product invariant** (ADR-0014 §2); the new `services/api` component, distinct from the agent-mTLS `services/ingest` trust boundary (ADR-0014 §3); and the three fixed roles `admin`/`analyst`/`viewer` enforced server-side (ADR-0014 §4). It derives the verifiable acceptance criteria, the `users`/`audit_log` schemas, the Redis session shape, the login/logout/rotation/revocation contracts, the rate-limit and CSRF controls ADR-0014 §Compliance bound, and the RBAC capability matrix for the auth-core surface.

The MVP narrative this enables (Blueprint §18, [blueprint.md:740](../product/blueprint.md#L740)): *"An administrator … creates a user with OTP. From the dashboard, generates an enrollment token."* — the human identity + the authorization gate around the existing agent-enrollment-token issuance.

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

Per ADR-0003 §Decision (users/RBAC → Postgres) and ADR-0014 §1/§4. The auth migrations are **owned by `services/api`** under its **own** runner and numbering, starting at `0001_users` (`services/api` is README-only today, so `0001` is free in its tree) — **not** appended to the `services/ingest` chain. The api runner mirrors the ingest applier pattern (`services/ingest/src/db/migrate.ts:50-85`) but with its **own** Kysely migration-ledger table and its **own** advisory-lock key, against the same Postgres instance (ADR-0003, one instance). The ownership decision and the runner contract are in §Operational §7 (Ratification §1).

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
- **`totp_secret`** — the base32 TOTP secret, **encrypted at rest** with pgcrypto `pgp_sym_encrypt` under a service passphrase, mirroring the CA private key precedent **in code** (`services/ingest/src/ca.ts:85-88` encrypt, `:99-103` decrypt). Because `services/api` owns its migrations independently (§Operational §7), its `0001_users` ensures the extension itself with `CREATE EXTENSION IF NOT EXISTS pgcrypto` (the same idempotent statement ingest's `0001_initial.ts:7` uses), so api does not depend on ingest having run first. The passphrase is a new `services/api` config var following the `INGEST_CA_PASSPHRASE` pattern (`services/ingest/src/config.ts:61`). It is a `TOTP secret` asset the threat model lists at risk ([threat-model.md:77](../security/threat-model.md#L77)).
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

Sessions live **only** in Redis (ADR-0003 §Decision; never a Postgres table). `services/api` is the first code consumer of Redis — the URL is already a required, validated config var (`services/ingest/src/config.ts:13`, `INGEST_REDIS_URL`) and Redis is provisioned (`infra/dev/docker-compose.dev.yml:55`), but no code reads it yet; the api config adds its own `*_REDIS_URL` following the same Zod pattern (`config.ts:44-108`).

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

`POST /v1/auth/login` body `{ email, password, totp_code }` (Zod-validated, strict — no extra fields, as the threat model's § cg-api, Tampering asks):

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

The `users`/`audit_log` migrations are **owned by `services/api`**: they live under `services/api/src/db/migrations/` numbered from `0001` (`0001_users`, `0002_audit_log`), applied by **`services/api`'s own** migration runner — a mirror of the ingest applier pattern (`migrate.ts:50-85`) but with its **own** Kysely migration-ledger table and its **own** advisory-lock key, so the two services' migration histories never collide on the shared Postgres instance (ADR-0003, one instance). The api runner is invoked at api startup behind an `API_RUN_MIGRATIONS` flag (mirroring `INGEST_RUN_MIGRATIONS`, `config.ts:62`) and is **self-contained** — its `0001` ensures `pgcrypto` itself (`CREATE EXTENSION IF NOT EXISTS`, idempotent), so api does not depend on ingest having run first. **Rationale (Manuel, Session 19 gate — Option A):** `services/api` is a component in its own right (ADR-0014 §3, ADR-0001); its schema — and especially the human-secret DDL (`password_hash`, `totp_secret`) — must not live in the agent-facing `services/ingest` tree. This **extends ADR-0014 §3's component separation from a runtime-only boundary to deploy-time (DDL) as well** — it realises §3 more fully and contradicts no Accepted ADR: ADR-0003's single-instance rule is preserved (two runners, one database), and ADR-0007 makes ingest run *its* migrations, not *all* migrations. **Rejected alternative:** appending to the shared `services/ingest` chain (`0006`/`0007`) under the existing runner — fewer moving parts by one runner, but it puts human-secret DDL in the agent-facing service's tree and couples api's schema readiness to an ingest-owned step.

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

## Amendment 2026-09-23: listener bind address

**What surfaced it.** Diagnosing the api container packaging (roadmap A'', Session 29) showed the listener bound to a hard-coded `127.0.0.1` (`services/api/src/server.ts`, since `fc33274`). That suits the host-process topology the test harness and `pnpm dev` use, but inside a container it binds only the container's own loopback, so neither the published port nor the container network can reach the api. This SPEC never fixed a bind address; the loopback bind was an implementation choice.

**Amendment.** The listener binds `API_BIND_HOST`, a new optional environment variable; its default, `127.0.0.1`, keeps the prior behavior. A containerised deployment needs a non-loopback bind such as `0.0.0.0`; exposure stays governed by the port mapping, not by the bind. The listener stays plain HTTP; TLS termination remains a deployment concern.

**Effect.** Additive and backward-compatible: with the variable unset the bind is unchanged, and the test harness sets loopback explicitly. No endpoint, port (`API_PORT`) or data contract changes. No SPEC consolidates the api's configuration surface today; that table is deferred to the operator configuration surface of roadmap Phase F. Status stays **Accepted**.

## Amendment 2026-10-15: the user routes' input, scope and audit; account disable; revocation; the TOTP confirmation; the stores' bounds

**What surfaced it.** Debts #72, #69 and #71 (`docs/handoff-session-38.md`), and a reading of this SPEC against `services/api/src/` while fixing #72 (the new debts it cites are recorded in `docs/handoff-session-39.md`, #81 among them for the TOTP confirmation):

- No route validates its body: `auth/routes.ts` casts `req.body`. §Operational §1 asks for a validated body with no extra fields, as the threat model's § cg-api, Tampering, asks for strict schemas with no extra fields. An unknown role reaches the `users` CHECK and answers `500` (#72), and a field of the wrong type reaches Postgres or Argon2.
- The session, role and CSRF checks are preHandlers, which Fastify runs after it has parsed the body, so a caller with no session learns whether a body parses before it gets its `401`.
- The `user.create` and `role.change` rows record the user acted on as `user_id`, which §Data contracts §2 defines as the acting principal, so no row names the admin who acted. No row carries a `correlation_id`; auth_ac_006 checks only that a login row exists. A role change records no role, so the trail cannot say which role was given. Every row's `org_id` is `default`, whatever the session's org.
- Email uniqueness is case-sensitive (`users_email_org_unique UNIQUE (org_id, email)`), against §Data contracts §1's "case-insensitive uniqueness per org". The login looks the address up as typed.
- The user routes ignore the session's org: `GET /v1/users` lists every org's users, `POST /v1/users` takes `org_id` from the body, and a role change reaches any user id. That is the object-level authorisation the threat model asks for on every write (§ cg-api, Information Disclosure).
- A role change for a user that does not exist answers `200` and writes an `ok` row. An `:id` that is not a UUID, and an address already taken, answer `500`.
- §5's matrix gives admins "disable users", and §2 lists "account disable" among the revocation triggers, but no route disables a user. An admin cannot remove a user's access.
- A role change deletes the user's sessions after the update commits. A login that read the user's row before the commit, and spent its Argon2 verification across the deletion, writes its session after it, so that session keeps the old role.
- For an unknown address or a disabled user, the login answers before any Argon2 work, against NFR-008-003's "constant work even for unknown users": the response time tells whether an active account holds the address.
- `POST /v1/auth/totp/confirm` takes any user id, enrolled or not, has no rate limit, and does not mark the accepted code as used. Someone who holds a user's password and user id can try codes there without limit and then log in with the code that matched, so the second factor does not hold. User ids are random, and only the admin routes return them, which narrows the attack without closing it.
- `POST /v1/auth/logout` changes state without the CSRF token that §4 requires of every mutating request.
- A user can be created with any password, a one-character one included. The bootstrap CLI (§6) passes any role to its INSERT, where the `users` CHECK refuses an unknown one with a raw error, and writes no audit row.
- #69: with Redis unreachable, a session-guarded request waits until Redis returns (`maxRetriesPerRequest: null`, `services/api/src/services.ts`). A Redis error reply answers `500`.
- #71: a Postgres connection already open whose peer goes silent holds a request until the operating system's TCP timeout.

**The amendment.**

1. **The order of the checks.** On a session-guarded route the session check (`401`), the role check (`403`) and the CSRF check (`403`) run first, in that order, as Fastify `onRequest` hooks, before Fastify reads the body. Then, still in an `onRequest` hook, the route checks its `:id`. Only then does Fastify read the body, which the route checks before it reads Postgres. The session check reads Redis. The login and the TOTP confirmation take no session; they check their body before any store is read, the rate limiter included. So a caller without a session, or without the role, learns nothing about a body.
2. **Bodies.** A route's body is parsed by a strict schema: an extra field, a missing one or one of the wrong type is refused → `400 { error: "bad_request" }`, with nothing written: no row, no audit row and no rate-limit count. A body Fastify cannot read keeps the status Fastify gives it (SPEC-009 Amendment 2026-10-14 item 4): `400` for JSON that does not parse, `413` for a body over its size limit, `415` for a media type Fastify has no parser for. Fastify also reads `text/plain`, and such a body fails the schema with `400`. The response body of such a status is then `{ error: "bad_request" }`, on every route of the api, the read routes included. The schemas, with lengths in JavaScript string length (UTF-16 code units) unless item 4 says otherwise:
   - `POST /v1/auth/login`: `email`, 1 to 254; `password`, 1 to 1024; `totp_code`, six ASCII digits. The `400` depends only on the body's form, never on an account, so it is no oracle (NFR-008-003).
   - `POST /v1/auth/totp/confirm`: `userId`, a UUID in the canonical text form of SPEC-009 Amendment 2026-10-14 item 1; `totp_code`, six ASCII digits.
   - `POST /v1/auth/password`: `currentPassword`, 1 to 1024; `newPassword`, a new password (item 4); `totp_code`, six ASCII digits.
   - `POST /v1/users`: `email`, which after item 3's trimming and lower-casing is at most 254 long and is accepted by zod 3's `string().email()`; `password`, a new password (item 4); `role`, one of `admin`, `analyst` and `viewer`. There is no `org_id`: a user is created in the session's org.
   - `POST /v1/users/:id/role`: `role`, one of the three.
   - `POST /v1/users/:id/status` (item 8): `status`, `active` or `disabled`.
   - `POST /v1/auth/logout` takes no body. Fastify still reads one that is sent, after the session and CSRF checks, so a body that does not parse answers as the rule above says; one that parses is ignored. The logout requires the CSRF token, as §4 requires of every mutating request. `GET /v1/users` has no body.
3. **Email.** An address is trimmed and lower-cased with JavaScript's `String.prototype.trim` and `toLowerCase` before it is checked, stored, looked up or used as the rate limiter's account key, so the case variants of one address are one account for the per-account limit. Migration `0004_users_email_ci` computes that form for every stored address with the same two functions. If two addresses of one org share it, the migration fails, naming the org and the number of such addresses but never an address. Otherwise it rewrites each address to that form. It then checks the same with Postgres's `lower()`, which can differ from JavaScript's on some non-ASCII text, and fails the same way, naming no address, before it adds a unique index on `(org_id, lower(email))`. The `(org_id, email)` constraint stays.
4. **New passwords.** A password set by `POST /v1/users`, by `POST /v1/auth/password` or by the bootstrap CLI has 12 to 128 Unicode code points, with at least one letter (`\p{L}`) and one decimal digit (`\p{Nd}`). The length follows OWASP ASVS 4.0 V2.1.1 and V2.1.2, and PCI DSS v4.0 requirement 8.3.6. The letter and the digit follow PCI DSS 8.3.6 and depart from ASVS V2.1.9, which advises against composition rules (*Decisions*). The login accepts any password of 1 to 1024 code units, so most users whose passwords predate this rule still log in (*Effect*).
5. **Scope and statuses.** The user routes act in the session's org, as SPEC-009's reads do: this is object-level authorisation inside the single-org MVP, and multi-org RBAC stays out of scope (§Out of scope). The login and the bootstrap CLI work in the org `default`, as before, so outside a test that writes a session directly the session's org is `default`. The TOTP confirmation finds its user by id, as before, in whichever org.
   - `GET /v1/users` lists the session org's users, each with its id, address, role and status.
   - A role or status change finds its user in the session's org, so another org's user id is a `404`, as an unknown id is. An `:id` that is not a UUID in canonical text form is a `404` before its body is read. An upper-case `:id` is read in lower case.
   - An address already taken in the org → `409 { error: "conflict" }`.
   - A store that cannot answer → `503 { error: "unavailable" }`, logged as SPEC-009 Amendment 2026-10-14 item 3 says.
6. **Audit rows.** Every row written while serving a request carries:
   - `user_id`, the acting principal. On `user.create`, `role.change`, `user.disable`, `user.enable` and `auth.logout` it is the session's user. On the login and self-service rows it is the user concerned, or `NULL` for a failed login of an unknown address, as before.
   - `org_id`, the session's org; for the login and the TOTP confirmation, the org of the user concerned, or `default` when no user is found. A failed confirmation whose id matches no user has `user_id` `NULL`.
   - `target_id`, the user acted on, on the rows of the user routes.
   - `correlation_id`, the request's id. The api generates a UUID for each request and writes it on the request's log lines as `reqId`. A request id the client sends is ignored.
   - `source_ip`, the request's address as the api sees it (debt #79 for a reverse proxy, `docs/handoff-session-39.md`).
   - `detail`, a new nullable `jsonb` column (migration `0005_audit_log_detail`), holding only role and status values: `{ "role": … }` on `user.create`, and `{ "from": …, "to": … }` on `role.change`, `user.disable` and `user.enable`. It never holds an address, a password, a code or a token.

   A change of the user routes and its row are written in one transaction, so no change is made without its row, and no row is written for a change those routes refused. The login and the TOTP confirmation write a row for a failure too (items 2 and 12). The bootstrap CLI's `user.create` row has `user_id` `NULL`, since no principal is in session; `org_id` `default`; `target_id` the new user; no `correlation_id` or `source_ip`; and its `detail`.
7. **Every change is recorded.** A role or status change that the checks accept writes its row and deletes the user's sessions (item 9), also when it sets the value the user already holds; its `detail` then has `from` equal to `to`. So no request deletes a user's sessions without a row.
8. **Account disable.** `POST /v1/users/:id/status`, admin only, with CSRF, takes `{ "status": "disabled" }` or `{ "status": "active" }` and writes `user.disable` or `user.enable`. Disabling deletes the user's sessions (item 9), and its login fails with the generic `401`, as before. Enabling restores the login and issues no session.
9. **Revocation and a login that races it.** Each user's row carries an authority version, a counter (migration `0006_users_authority_version`). A role change, a status change and a password change raise it in their transaction, and after the commit delete every session of the user, as §2 says. The deletion scans every session and finds each one written before the scan starts. The login reads the version with the user's row. After it writes its session, it reads the row again. If the version moved or the user is no longer active, it deletes the session it wrote and fails with the generic `401`, writing `auth.login.fail`. A login that read the row before a change committed therefore either wrote its session before the deletion's scan, which removes it, or after it, and then reads the new version. Revocation stays the server-side deletion of the session entries, and the session check one Redis read (ADR-0014 §2). If the deletion after the commit fails, the change stands, the answer is a `503`, and the failure is logged at `error` with the user's id. The user's sessions then keep their old authority until a deletion succeeds: the admin's repeated request deletes them (item 7). After a password change the user cannot repeat the same request, since its current password has changed; a further password change, or an admin's status change, deletes the sessions. A password change deletes the caller's own session too, and issues none: the caller logs in again with the new password, as §8's rotation.
10. **The last admin.** A role change or a disable that would leave the org without an active admin → `409 { error: "conflict" }`, and nothing changes. The check and the change run in one transaction that locks the org's active admins and the target, in user-id order, so two admins demoting each other at once cannot both succeed. The bootstrap CLI (§6) remains the way back for the org `default`.
11. **Constant work at the login.** For an unknown address or a disabled user, the login verifies the password against a fixed Argon2id hash of the service's parameters before it fails, so its work does not depend on whether an active account holds the address (NFR-008-003).
12. **The TOTP confirmation** (§8) confirms only a user whose TOTP is not yet confirmed. For any other user id, and for a wrong code, it answers the same generic `401` after the same work. It is rate-limited like the login (§3): per user id, with the login's per-account threshold, and per source IP, on the login's own per-IP counter, so failures from one address at either route count against that address at both. The per-user counter is its own. A `userId` in upper case is read in lower case, so its case variants are one id. A failed confirmation writes `user.totp.enroll` with outcome `denied`, as a failed login writes `auth.login.fail`. A code it accepts is marked used, as the login marks it (§1.3), so the same code cannot open a session within its window.
13. **The bootstrap CLI** takes its address, password and role through the schemas of item 2 and the policy of item 4, refuses any other role, and writes the `user.create` row of item 6. For an address already taken it fails with a conflict and writes nothing. Its password on the command line is debt #77 (`docs/handoff-session-39.md`).
14. **Redis is bounded and fails closed.** The api's Redis client fails a command it has not completed within 5 s, whether the connection could not be made within its 5 s connection timeout or the server does not answer, and it holds no command beyond that bound. A Redis failure → `503 { error: "unavailable" }`, logged as item 5 says: an unreachable or silent server, or an error reply, in the session check, the rate limiter, the TOTP replay marker or a deletion. No session is resolved and no login proceeds without Redis. This supersedes SPEC-009 Amendment 2026-10-14 item 6.
15. **Postgres is bounded.** The server cancels a statement of the api's pool that runs 5 s (`statement_timeout`), and the client fails a query that has had no answer for 6 s (`query_timeout`), so a slow statement ends at the server first and a silent peer at the client. A lock wait inside item 10's transaction counts. Either way the answer is a `503`. A connection whose query failed this way leaves the pool rather than being reused. The bound covers every route of the api. The drill, the forensic export and the report read Postgres by key and their events from ClickHouse, whose own timeout applies. The playbook executions read Postgres only. The migration runner and the bootstrap CLI use their own pools and are not bounded by this item.

**Decisions.** Delegated by Manuel on 2026-10-15 ("todo lo que dependa de mi elige tu en beneficio del proyecto") and decided by Claude Code, for an enterprise buyer (a security lead and a data-protection officer):

- **The password floor is 12 code points, with a letter and a digit**: PCI DSS v4.0 requirement 8.3.6, which applies to the user accounts of a security system connected to a cardholder data environment. Alternatives: NIST SP 800-63B's 8 characters for a password used with a second factor, which a PCI assessment would flag; and no composition rule, as ASVS V2.1.9 advises, which PCI does not allow.
- **The last admin is protected in the api.** Alternative: rely on the CLI to recover, which needs shell access to the host and leaves the org with nobody who can disable a leaver until then.
- **A login that races a change is caught by an authority version the login reads twice.** Alternatives: a revocation stamp the session check reads on every request, which adds a second Redis read per request and replaces ADR-0014 §2's single delete, an ADR change; ordering by Postgres's clock, the clock of a VM where ClickHouse's was measured stepping back (`docs/handoff-session-38.md`, debt #41); and reading the role from Postgres on every request, which moves the session's authority out of the Redis record.
- **Redis fails closed**, with a `503`. Alternative: resolve sessions from a cache while Redis is down, which breaks the immediate revocation of §2.
- **The role trail records the roles in a `detail` column.** Alternative: encode the role in `action`, which mixes the event's kind with its value.
- **The TOTP confirmation is limited like the login.** Alternative: require a session for it, which the bootstrap flow cannot offer, since a user cannot log in before confirming (§8).

**Effect.**

- §Data contracts §1 gains migrations `0004` (the stored addresses normalized, and a case-insensitive unique index) and `0006` (the authority version), and §2 the `detail` column (migration `0005`). §Data contracts §3's session entry is unchanged.
- §Operational §1's body is now enforced, and its login reads the user's row twice. §2's revocation is still a delete, after a change raises the version. §4's CSRF covers the logout. §5's matrix row "Create / disable / list users" gains its disable route, which is also §2's admin revoke. §6's CLI validates its input and writes its audit row. §8's confirmation is limited to a user not yet confirmed, rate-limited, and marks its code used.
- The header, §Operational §1 and the ADR-0002 reference cite ADR-0002 Rule 4 for strict schemas. The rule makes TypeScript the default for user-facing layers, and ADR-0002 locks no framework. The authority is the threat model's § cg-api, Tampering (debt #84).
- auth_ac_003 covers every admin route: a viewer and an analyst get `403` on `POST /v1/users`, `GET /v1/users`, `POST /v1/users/:id/role` and `POST /v1/users/:id/status`, and an admin gets past the check. It still blocks CI. Its admin case creates a user whose password the policy accepts; the check is unchanged (Convention #12).
- auth_ac_006 asserts the values of three rows. A role change's row holds the admin as `user_id`, the session's org, the target, a UUID `correlation_id` equal to the request's `reqId`, the `source_ip` and the `detail`. A `user.create` row holds its `detail`. A login's row holds its `correlation_id` and `source_ip`.
- New acceptance criteria, each CI-able under `services/api/test/`. They are named `apiauth_ac_NNN`, files `apiauth-ac-NNN-<slug>.test.ts`, numbered after auth_ac_009: SPEC-020's logon criteria use `auth_ac_001` to `auth_ac_014`, auth_ac_013 in the elevated gate, so this SPEC's own auth_ac_001 to 009 already share their names, and the new ones take a prefix of their own.
  - **apiauth_ac_010 (bodies and the order).** On each route, a body that fails its schema → `400`, with nothing written. The cases: an unknown role on create and on a role change (#72); an extra field; a field of the wrong type; a malformed `totp_code`; JSON that does not parse; a new password of 11 or 129 code points, or of 12 with no digit or no letter. On the login, a malformed body writes no audit row and counts against no limit. Without a session the answer is `401`, and for a viewer `403`, whatever the body, an unparseable one included. A logout without the CSRF token → `403`.
  - **apiauth_ac_011 (email).** A user created as `Ann@Example.COM`, with two leading spaces, logs in as `ann@example.com`. A second user `ANN@example.com` in the same org → `409`. Failed logins under case variants of one address count as one account. Migration `0004` refuses an org that holds two addresses differing only in case or surrounding spaces, and rewrites a single such address.
  - **apiauth_ac_012 (scope and statuses).** `GET /v1/users` lists only the session org's users. A role or status change aimed at another org's user, at an unknown id or at a non-canonical `:id` → `404`, and nothing changes.
  - **apiauth_ac_013 (disable, revocation and the last admin).** Disabling a user makes the next request with each of its sessions a `401`, and its login a `401`; its row's `detail` reads `from` `active`, `to` `disabled`. Enabling restores the login and writes no session. A login whose row was read before a role change committed, and whose Argon2 verification ends after the change deleted the sessions, gets `401` and leaves no session. A change to the value already held writes its row. The status route needs CSRF. Demoting or disabling the org's last active admin → `409`, and nothing changes. Two concurrent demotions of the last two admins leave one admin.
  - **apiauth_ac_014 (the stores).** With Redis refusing connections, accepting a connection and never answering, or answering with an error, a session-guarded request and a login each answer `503` within 8 s, logged at `error`. The bound of the other Redis calls of item 14 is the same client's and is not tested apart. A statement that runs 5 s on the api's pool fails with Postgres's cancellation, and a read whose statement runs past the bound answers `503`.
  - **apiauth_ac_015 (constant work).** A login for an unknown address, and one for a disabled user, run the Argon2id verification, as a login with a wrong password does.
  - **apiauth_ac_016 (the TOTP confirmation).** A confirmation for a user whose TOTP is confirmed answers `401` and changes nothing. Failed confirmations are throttled per user id and per source IP, as failed logins are. A code a confirmation accepted cannot open a session.
  - **apiauth_ac_017 (the bootstrap CLI).** The CLI refuses an unknown role and a password the policy refuses, and writes a `user.create` row with its `detail`.
- read_ac_007, under SPEC-009, gains a case: with an open Postgres connection whose peer goes silent, a read answers `503` within 8 s.
- Not backward-compatible for:
  - a client that sends a body the schemas refuse, extra fields included, or that read a `500` as "invalid role", or a `200` as the answer to a role change for a user that does not exist (now `404`);
  - a logout without the CSRF token;
  - a user whose password is empty or longer than 1024 code units, who can no longer log in;
  - a TOTP confirmation for a user already confirmed, which now fails, and a confirmation past the limit, now `429`;
  - a client of `GET /v1/users` that saw other orgs' users, or changed the role of another org's user (now `404`);
  - a client that read a store failure as a `500`, or waited out a Redis outage, now answered `503`;
  - a user whose stored address is longer than 254 code units once trimmed and lower-cased, whose login now answers `400`;
  - a deployment whose org holds two addresses that differ only in case or surrounding spaces, where migration `0004` fails the start when `API_RUN_MIGRATIONS` is set, until the operator resolves them.

  No product client of the repository calls these routes, only the api's own tests: the dashboard's login form is still a stub (`dashboard/app/login/page.tsx`, debt #83, `docs/handoff-session-39.md`). When it is wired, it sends the bodies of item 2.
- SPEC-009 Amendment 2026-10-14 item 3's "a connection already open whose peer goes silent is not bounded", and its item 6, are superseded by items 14 and 15, as SPEC-009 Amendment 2026-10-15 records.
- Status stays **Accepted**.

## Amendment 2026-10-15: the source address behind a proxy

This is SPEC-008's second amendment of 2026-10-15. A reference to "SPEC-008 Amendment 2026-10-15" written before it, in this SPEC, other documents or code comments, means the first, *Amendment 2026-10-15: the user routes' input, scope and audit; account disable; revocation; the TOTP confirmation; the stores' bounds*. This amendment is a document change: until the code that implements it lands, the api behaves as *What surfaced it* says.

**What surfaced it.** Debt #79 (`docs/handoff-session-39.md:616`). Fastify's `trustProxy` is unset (`services/api/src/app.ts`), so behind a reverse proxy the api reads the proxy's address as every request's source address. The per-IP login counter (§Operational §3) then counts every user together, and the audit rows' `source_ip` names the proxy. SPEC-009's *Amendment 2026-10-15: the dashboard's login and logout* makes the dashboard such a proxy for every login and logout: without this amendment, ten failed logins through the dashboard from anywhere would refuse every dashboard login for fifteen minutes (`services/api/src/auth/ratelimit.ts`, `IP_MAX` and `WINDOW_SECONDS`).

**Amendment.**

1. **`API_TRUSTED_PROXIES`**, a new optional environment variable that the api reads with the others at start (`services/api/src/config.ts`, which refuses to start on an invalid one): a comma-separated list of IPv4 and IPv6 addresses and CIDR ranges. Spaces around each entry and empty entries are ignored. Any other entry refuses the start: a name such as the ranges `@fastify/proxy-addr` names (`linklocal`, `loopback`, `uniquelocal`), netmask notation, a zone index, or a prefix length past its family's (32, 128). So does a CIDR range that holds every IPv4 or every IPv6 address, which would trust every peer of that family: a prefix length of 0, or an IPv6 range that holds the whole IPv4-mapped space (`::ffff:0:0/96`, and any shorter prefix that contains it, such as `::/80`), since a mapped range matches IPv4 peers. Each entry is checked alone: entries that together cover a family, such as `0.0.0.0/1, 128.0.0.0/1`, are not refused.
2. **The source address.** An *IP address* here is a string Node's `net.isIP` accepts, with no zone index. An IPv6 address is read in its canonical form (RFC 5952), and one that maps an IPv4 address (`::ffff:a.b.c.d`, in either notation) as that IPv4 address, in the list as in a request, so that one address has one form. A range is matched by its prefix; host bits set in it are ignored. The entries of `X-Forwarded-For` are its comma-separated values, each trimmed. For a request whose peer address is in the list, the api reads those entries from the right: it passes the entries that are in the list and takes the first that is not. If every entry is in the list, the leftmost is taken. If it meets an entry that is not an IP address, or the request carries no `X-Forwarded-For`, the source address is the peer's. For any other request, the source address is its peer's and its `X-Forwarded-For` is ignored. Unset or empty: every request's source address is its peer's as the socket gives it, the prior behavior. The api computes this itself, with Node's `net.BlockList`; Fastify's `trustProxy` stays unset, so `req.ip` and the request log keep the peer's address.
3. **One source address.** The address of item 2 is the one the per-IP counter that the login and the TOTP confirmation share uses (§Operational §3; the first Amendment 2026-10-15, item 12), and the audit rows' `source_ip` (the first Amendment 2026-10-15, item 6).
4. **Who is listed.** Listing a peer trusts the `X-Forwarded-For` entries it passes on. A deployment lists the hops that reach the api as a peer or appear in the header: the reverse proxies in front of the api, and the dashboard. Item 2's walk takes the rightmost entry that is not listed, unless, reading from the right, it meets an entry that is not an IP address before it; when no entry qualifies, item 2 says what it takes. When the outermost listed hop appends its peer's address to the header or sets it, and every listed hop behind it appends or passes the header on unchanged, that entry is the address of the hop in front of the outermost listed one. A listed hop that sets the header behind another listed hop discards what that hop wrote, and the api then counts its requests under a listed address; so the listed hops behind the outermost one append, or pass the header on unchanged, and none sets it. The dashboard does not append, and sets the header only when none reached it: it passes on the header that reached it, or, when none did, fills it with its own peer's address (SPEC-009, *the dashboard's login and logout*, item 5). So, with the listed hops between it and the api appending or passing the header on unchanged, the entry the walk takes for a request through the dashboard is what reached the dashboard: what the hop in front of it appended or set, the header the client sent, or, when none arrived, the dashboard's own peer's address. So the dashboard is listed only when a proxy that appends or sets the header stands in front of it; that proxy need not be listed, since its address does not reach the header. Without one, a client that reaches the dashboard chooses the address the api counts (debt #103). A listed range must hold no other host, since every address in it is trusted: a client in it chooses the address it is counted under. A hop that is not listed counts as one client. The proxies write bare addresses; an entry with a port is not an IP address and ends the walk at the peer. Roadmap §F's SPEC sets the list for the production compose. On the dev stack the api reaches host processes through one gateway address, which no list can single out, so it keeps the lockout of *What surfaced it*.

**Decision.** Delegated by Manuel on 2026-10-15 ("Lo que dependa de mí, decídelo tú en beneficio del proyecto", the opening of Session 40, recorded in `docs/handoff-session-40.md`): a variable a deployment reads is the deployment contract (CLAUDE.md *Decision authority*). Decided for an enterprise buyer: an explicit list of addresses, empty by default, so nothing is trusted unless an operator names it. Alternatives: trusting the loopback by default, which covers the host-process topology but not a container network; trusting one hop from any peer, which lets any client that reaches the api choose its counted address.

**Acceptance criteria.**

- **apiauth_ac_018 (the source address behind a proxy).** With `API_TRUSTED_PROXIES` listing the test's peer address, a failed login carrying `X-Forwarded-For: 203.0.113.7` is counted under `203.0.113.7`, and its `auth.login.fail` row's `source_ip` is `203.0.113.7`; an IPv6 entry is read the same way, and `::ffff:203.0.113.9` as `203.0.113.9`. Failures under two forwarded addresses are counted apart, so the per-IP threshold reached under one does not refuse the other. With a second listed address, `X-Forwarded-For: 198.51.100.4, <listed>` is counted under `198.51.100.4`; with every entry listed, under the leftmost. A listed peer with no header, or whose walk meets an entry that is not an IP address, is counted under its own address. A failed TOTP confirmation through the listed peer is counted under the forwarded address too. With the variable set but not listing the peer, and with it unset, the same requests are counted and recorded under the peer's address. A value with a name (`loopback`), netmask notation, a zone index, a prefix past its family's length, a `/0` range, or a range that holds the IPv4-mapped space (`::ffff:0:0/96`, `::/80`) refuses the configuration; spaces and an empty entry do not.

**Effect.** The api's variables gain `API_TRUSTED_PROXIES` (`services/api/src/config.ts`). SPEC-008 has no table of them: the Amendment 2026-09-23 says no SPEC consolidates the api's configuration surface, which stays roadmap §F's (debt #20). §Operational §3: its two dimensions, thresholds and windows are unchanged; the per-IP dimension reads the source address of item 2. The audit rows' `source_ip` follows item 3. Additive and backward-compatible: unset, the behavior is the prior one, and the lockout *What surfaced it* describes remains until a deployment lists the dashboard behind a proxy as item 4 says; a dashboard listed without one lets a client choose its counted address (debt #103). Debt #79 closes with the code that implements this amendment; roadmap §F's SPEC lists the addresses for its compose.

## Amendment 2026-10-16: refused requests, the password change, the limiter's reservation

This amendment is a document change: until the code that implements it lands, the api behaves as *What surfaced it* says.

**What surfaced it.** Three debts of the api's sweep (`docs/handoff-session-39.md`):

- #80 (`:623`): a request refused for want of a session (`401`), and one the role check or the CSRF check refuses (`403`, `services/api/src/auth/prehandlers.ts`), write no audit row, so an attempt to use an admin route without the role leaves no trace (threat model §3 cg-api, Repudiation).
- #100 (`:745`): `POST /v1/auth/password` verifies the current password and a TOTP code with no rate limit, writes no row for a failure, and does not mark the code used, so with a session the code can still open another session within its window.
- #101 (`:751`): the limiter reads its counters, then counts a failure later (`services/api/src/auth/ratelimit.ts`, `isAllowed` and `recordFailure`), so a burst of parallel attempts passes the threshold before any count lands; the TOTP confirmation shares the pattern.

**Amendment.**

1. **A refusal of a known principal writes a row** (#80, its `403` half). A request that the role check (§Operational §5) or the CSRF check (§Operational §4) refuses with `403` writes one `audit_log` row:
   - `action` `access.denied`; `outcome` `denied`;
   - `user_id` and `org_id`, the session's; `target_id` null;
   - `source_ip` and `correlation_id`, as the first Amendment 2026-10-15 item 6 and the second Amendment 2026-10-15 item 3 say;
   - `detail`, `{ "reason": "role" | "csrf", "route": "<METHOD> <route>" }`: the request's method and the route's pattern as registered (`/v1/users/:id/role`), never the request's path, query or body.

   This holds on every route those checks guard, SPEC-009's forensic routes included. The request is refused either way: when the row cannot be written it answers `503 { error: "unavailable" }`. A request refused for want of a session (`401`) is not settled here. It has no principal to record, and a row for each would let anyone who reaches the api fill the table. But ADR-0014 §Compliance (`docs/adr/0014-human-authentication-model.md:98`) asks that "all authentication events" be audit-logged, and whether a rejected or missing session token is one is a reading of that ADR, which is Manuel's. Until he reads it, such a request writes no row, as now, and debt #80 stays open for that half. At the api's default log level (`info`), the request log records the request, with its method, URL, status and request id.
2. **The password change is limited, and its code used** (#100). `POST /v1/auth/password` is rate-limited per user id, on a counter of its own, with the login's per-account threshold. It needs a session, so it shares no counter that a request with no session fills: neither the TOTP confirmation's per-user counter nor the login's per-IP counter. A failure, a wrong current password or a wrong code, answers `401`, writes `user.password.change` with outcome `denied`, and counts on that counter. A code it accepts is marked used, as the login marks it (§Operational §1.3), before the password is changed: it cannot open a session, or change the password again, within its window, and a code already used is a failure.
3. **The limiter reserves before it verifies** (#101). Each limited attempt (a login, a TOTP confirmation, a password change) first reserves one slot on each of its counters, atomically. One Redis script checks every counter against its threshold and, only if none has reached it, increments each, starting the window of a counter it creates. A burst of parallel attempts gets at most the threshold's number of slots, and the rest answer `429` before any credential work. An attempt refused this way reserves nothing and changes no window.
   - **A failed attempt** keeps its slots, which count it, and restarts their windows, as a failure did before.
   - **An attempt that ends in a store failure before it is a failure or a success** gives its slots back, as a success does, so a store's outage counts no attempt it interrupted. A failure stays a failure when a store then fails to record it: its slots stay counted. A counter the give-back cannot reach, Redis being down, keeps its slots until its window ends.
   - **A successful attempt** gives its slots back:
     - the login clears the account's counter, as before, and gives back its per-IP slot;
     - the TOTP confirmation and the password change give back each of theirs.
   - **A login that item 9 of the first Amendment 2026-10-15 refuses after its credentials matched** gives its slots back too: it counts no failure, as built (`services/api/src/auth/service.ts`, `reject(false)`).
   - A give-back never takes a counter below 0, and a counter that reaches 0 is removed. A successful login still clears the account's counter, as before, slots of attempts in flight included. The thresholds (5 per account or user, 10 per address) and the window (15 minutes from the last failure) are unchanged; attempts in flight now count toward the thresholds too.
4. **The audit row's `detail`.** The first Amendment 2026-10-15 item 6 says `detail` holds only role and status values; SPEC-009's third Amendment 2026-10-15 added the forensic rows' counts and hashes. It now also holds item 1's reason and route. It still never holds an address, a password, a code or a token.

**Decision.** Delegated by Manuel on 2026-10-15 ("Lo que dependa de mí, decídelo tú en beneficio del proyecto", the opening of Session 40, recorded in `docs/handoff-session-40.md`): what the api records of a refused request, and how it limits the routes that check a second factor, are the security posture, and a `429` on the password change breaks a public API contract (CLAUDE.md *Decision authority*). The `401` half of #80 waits for Manuel's reading of ADR-0014 (item 1). Decided for an enterprise buyer:

- every refusal of a known principal is recorded with its route; a principal's own refusals are attributable to it, so their rows are its trace, and a principal that floods them is identified by them;
- the password change is limited per user as the login is per account, and no unauthenticated route can lock it;
- no burst of parallel requests passes a threshold.

Alternatives:

- for item 1, the request's path instead of the route's pattern, which would copy ids and query strings into the row;
- for item 2, the login's per-IP counter, which requests with no session fill;
- for item 3, one lock per counter held across each attempt, which would make a user's or an address's attempts wait for one another, where the script makes one atomic round trip.

**Acceptance criteria.** CI-able, against the real api and its stores.

- **apiauth_ac_019 (refused requests).**
  - An analyst's `POST /v1/users` answers `403` and writes one `access.denied` row: the session's user and org, outcome `denied`, `detail` `{ "reason": "role", "route": "POST /v1/users" }`, the request's source address and id.
  - An admin's `POST /v1/users` without `X-CSRF-Token` writes one with reason `csrf`.
  - A viewer's forensic export writes one, with route `GET /v1/incidents/:id/forensic-export`, and a viewer's report one with route `GET /v1/incidents/:id/report`; neither writes a forensic row.
  - A refused request with a query string records the route's pattern, without the query.
  - A request with no session writes no row, as now (item 1).
  - When the store cannot write the row, the refused request answers `503`.
- **apiauth_ac_020 (the password change).**
  - A wrong current password, and a wrong code, each answer `401` and write a denied `user.password.change` row.
  - After five failures for one user, from five addresses, the sixth answers `429`; failed TOTP confirmations of that user, and failed logins from its address, do not count toward it.
  - A code that changed the password neither opens a session nor changes the password again.
- **apiauth_ac_021 (the reservation).**
  - Twenty parallel failed logins from one address, each for its own account, answer ten `401` and ten `429`.
  - Six parallel failed logins of one account, from six addresses, answer five `401` and one `429`.
  - Eight parallel failed TOTP confirmations of one user id, each from an address of its own, answer five `401` and three `429`; so do eight parallel failed password changes of one user.
  - A successful login takes its slot back from the address's counter, and does not restart the window.
  - An attempt answered `429` does not restart the window.
  - A login that ends in a store failure gives its slots back: the account's and the address's counters hold what they held before it.

**Effect.**

- §Data contracts §2: the rows that MUST be written gain `access.denied` and a denied `user.password.change`.
- §Operational §1: the slots are reserved before the credentials are checked (before steps 1 to 3), replacing step 5's increment on a failure, and a success gives the address's slot back.
- §Operational §3: the counters reserve; their dimensions, thresholds and window are unchanged. auth_ac_004 still holds.
- §Operational §4 and §5: a refusal writes item 1's row.
- §Operational §8: the password change is limited, recorded on failure, and marks its code used.
- The first Amendment 2026-10-15:
  - Item 6's "no row is written for a change those routes refused" now means no row of the change's own action; a role or CSRF refusal writes item 1's row.
  - Item 6's `detail` is restated by item 4 above.
  - Item 12's per-user counter stays the confirmation's own.
- SPEC-009's third Amendment 2026-10-15, *Effect* on NFR-009-004: "the two forensic routes write one audit row each" holds for the documents they produce; a refused request writes item 1's `access.denied` row instead.
- SPEC-009's third Amendment 2026-10-15, item 3, says a request refused at the role writes no row "as on every route (debt #80)", and its forensic_ac_001 says a viewer's export and report "write no row". A viewer's refused export or report now writes item 1's `access.denied` row, and still no forensic row: forensic_ac_001 reads "write no forensic row", and the code that implements this amendment changes its test to count forensic rows only. The `access.denied` row is apiauth_ac_019's.
- The threat model, when the code lands, changes in four §3 sentences:
  - "a request the session, role or CSRF check denies writes no row (#80)" becomes the role and CSRF refusals' row, the session's refusal still writing none;
  - "rate limits exist only on the login and the TOTP confirmation" gains the password change;
  - "the login and the TOTP confirmation do write one for a failure" gains the password change;
  - "A read writes none" gains the refused read's `access.denied` row.

  The threat model is Manuel's, and `docs/handoff-session-40.md` lists the change for his audit.
- Not backward-compatible, for a client that relied on the prior behavior:
  - the password change can answer `429`, and a code the login has just used fails it;
  - a refused request answers `503` when its row cannot be written;
  - attempts in flight count toward the thresholds, so an eleventh concurrent login from one address answers `429` even when none has failed;
  - a refused request of a known principal writes a row.
- Debts #100 and #101 close with the code that implements this amendment. Debt #80 narrows with it to its `401` half, which waits for Manuel's reading of ADR-0014 (item 1).

## Amendment 2026-10-16: the session's idle lifetime and its cap

This is SPEC-008's second amendment of 2026-10-16. A reference to "SPEC-008 Amendment 2026-10-16" written before it, in this SPEC, other documents or code comments, means the first, *Amendment 2026-10-16: refused requests, the password change, the limiter's reservation*. This amendment is a document change: until the code that implements it lands, the api behaves as *What surfaced it* says.

**What surfaced it.** Debt #82 (`docs/handoff-session-39.md:636`). §Data contracts §3 gives the session entry a "TTL = idle timeout (sliding), with an absolute lifetime cap", and NFR-008-002 recommends "idle 8 h / absolute 24 h, per-org configurable". The api sets the entry's time-to-live to 8 h at login and never changes it (`services/api/src/auth/session.ts`, `createSession`; `resolveSession` only reads the entry). A session therefore ends 8 h after its login, whether it was used or not: an analyst working a shift is signed out at its eighth hour, and nothing caps a lifetime because nothing extends one. `last_seen` is written at login and never again.

**Amendment.**

1. **A session ends after 8 h without use, and 24 h after its login at the latest.** Every request that passes the session check (the first Amendment 2026-10-15, item 1) is a use, whatever the route then answers: a read, a change, the logout, and a request the role or CSRF check then refuses. At a use, the api rewrites the entry with `last_seen` set to the request's time and its time-to-live set, in milliseconds, to 8 h or to the time left until 24 h after `issued_at`, whichever is shorter. A session with no time left, or whose `issued_at` is missing or not a time the api can read, is no session: the request answers `401`, as for a missing one, and the entry is deleted. `issued_at` never changes: a use rewrites `last_seen` and the time-to-live only.
2. **A use never brings back a revoked session.** The rewrite replaces the entry only if it still exists (Redis `SET ... XX`). A logout, a role change, a password change or an account disable that deletes the entry between the read and the rewrite stays done: the request that read it completes, as it did before this amendment, and the next one finds no session (auth_ac_005's invariant is unchanged).
3. **A store failure is the session check's.** A rewrite or a delete that Redis fails is a failure of the session check, which the first Amendment 2026-10-15, item 14, answers `503`: a session past its cap whose delete fails answers `503`, not `401`.
4. **The values are fixed, 8 h and 24 h**, NFR-008-002's recommended defaults. Its "per-org configurable" is not built: the api has no per-org settings at all, and which settings an org or the operator of a deployment sets is roadmap §F's SPEC to decide, with the api's configuration surface (debt #20). Open question 2 stays open on that point, deferred, and reopens when that SPEC lists the api's settings. The login's `issued_at` is the api's clock; instances that share Redis and disagree on the time move the cap by their difference.
5. **The cookies are unchanged.** The cookies that carry the tokens, the api's `cgsess` and the dashboard's `cgsess` and `cgcsrf`, are set without `Max-Age` or `Expires`: they are the browser session's, and the server's entry decides how long they work.

**Decision.** The lifetime is the one SPEC-008 already states; this amendment makes the code meet it rather than restating the NFR as the fixed 8 h the code had, which would sign out a user in the middle of a shift and leave `last_seen` meaningless. What it costs:

- A token that keeps being used now works for up to 24 h after its login, where it worked for 8 h. The cap bounds a stolen cookie's window, and revocation (§Operational §2) ends a session at once.
- Each request a session passes now writes to Redis, and the logout and the password change write twice. A Redis that serves reads and refuses writes (full under `noeviction`, or a read-only replica) now answers `503` on every session-guarded request, where it served them.
- Concurrent uses of one session may write `last_seen` out of order: it is the time of a recent use.
- A session that ends at its idle time or its cap writes no audit row, as an idle session never did. §Data contracts §2's "session revoke" is a deletion someone asks for.

The points decided here:

- **What counts as a use**: every request that passes the session check, reads and refused requests included. Alternative: only a request the route then serves, which needs a second hook after the handler, and would end the session of an analyst whose last requests were refused.
- **Per-org configuration deferred** (item 4): which settings the operator of a deployment sets is the deployment contract, Manuel's, delegated by him on 2026-10-15 ("Lo que dependa de mí, decídelo tú en beneficio del proyecto", the opening of Session 40) and again at the opening of Session 41 (`docs/handoff-session-41.md`). Decided for an enterprise buyer: the defaults now, the setting with the operator's surface. Alternative: an environment variable now, one value for every org, before §F decides which settings an org has. Reopen when roadmap §F's SPEC lists the api's settings.
- **The rewrite is `SET ... XX`**, one command. Alternatives: `WATCH` and `MULTI`, which retry under contention, or a Lua script, which would move the cap's arithmetic into Redis.
- **A session past its cap is deleted** when a request presents it. Alternative: let its time-to-live end it, which already falls at the cap, and which leaves nothing to delete but the case of an entry written before this amendment.
- **An `issued_at` the api cannot read ends the session.** Alternative: serve it as issued now, which would give it a fresh 24 h.

**Acceptance criteria.** CI-able, against the real api, its Postgres and its Redis, but for the race case, which runs the session module itself. The tests move a session in time by editing its entry's `issued_at` and time-to-live, not by waiting.

- **apiauth_ac_022 (the session's lifetime).**
  - A login's entry has a time-to-live of 8 h, within five seconds.
  - A session whose time-to-live was lowered to 60 s, as after 7 h 59 min idle, is used by an authorized request: it answers `200`, its time-to-live is 8 h again, within five seconds, and its `last_seen` falls between the request's start and its end.
  - A session issued 23 h ago is used: the time-to-live becomes the hour left to its cap, within five seconds, not 8 h. Its `issued_at` is unchanged, and a second use gives it no more than the hour left.
  - A request the role check refuses, a viewer's `POST /v1/users` (`403`), is a use: its session's time-to-live is 8 h again.
  - A session issued 24 h and one second ago answers `401` on an authorized route, and its entry is gone; so does an entry whose `issued_at` is not a time.
  - A session deleted after the api read its entry, and before the use's rewrite, is not recreated (item 2's `XX`). The test runs the session module against a Redis client that deletes the entry once its read returns.
  - The logout of a session still ends it: the next request answers `401`.
  - A rewrite that Redis fails answers `503`; so does the delete of a session past its cap.

**Effect.**

- §Data contracts §3: the TTL line holds as written; the values are item 4's, and `last_seen` is the time of the last use.
- NFR-008-002: met, but for "per-org configurable", which Open question 2 keeps open (item 4).
- §Operational §2 and auth_ac_005: unchanged (item 2).
- The first Amendment 2026-10-15, item 9: "the session check one Redis read" becomes a read and a rewrite, or a delete past the cap; revocation stays the deletion of the entries. ADR-0014's "one O(1) Redis read" (`docs/adr/0014-human-authentication-model.md:35`) is the cost it weighed for the option it chose, and ADR-0014 leaves the session's lifetime to this SPEC (`:104`): it is not amended.
- SPEC-009's second Amendment 2026-10-15, item 4, says a session the dashboard could not end lives "a fixed 8 hours as built (debt #82)"; when the code lands, it lives until 8 h without use or 24 h after its login end it, unless a role, status or password change deletes it first. The code that implements this amendment updates that clause.
- The threat model's §3 sentence "Sessions are opaque random tokens in Redis, not signed, and last a fixed 8 h (debt #82)" becomes the sliding 8 h within a 24 h cap, when the code lands. The threat model is Manuel's; `docs/handoff-session-41.md` lists the change for his audit.
- Backward-compatible for a client: a session ends no earlier than before (8 h after its last use is never earlier than 8 h after its login), but for item 4's clock difference.
- Debt #82 closes with the code that implements this amendment; the per-org setting joins debt #20.

## References

- [ADR-0014](../adr/0014-human-authentication-model.md) — the human-auth model this SPEC realises (identity §1, session §2, `services/api` §3, RBAC §4); the §Compliance MUSTs (rate-limit, cookies, CI-blocking RBAC, audit) and the §Out of scope CSRF deferral this SPEC closes.
- [ADR-0003](../adr/0003-polyglot-storage.md) — storage homes: users/RBAC/audit → Postgres (§Decision / §Retention), sessions + rate-limit → Redis (§Decision). No amendment (already routed).
- [ADR-0001](../adr/0001-monorepo-layout.md) / [ADR-0002](../adr/0002-language-per-component.md) — `services/api` location + TypeScript/Fastify/Zod (`0002` §Context + §Decision row).
- [SPEC-004](SPEC-004-server-ingest-minimal.md) — the ingest scaffold reused as a pattern (`app.ts`, `config.ts`, `migrate.ts`); `cli/issue-token.ts` (the opaque-token CSPRNG pattern and the enrollment-token issuance this SPEC's RBAC authorizes — CLI-only until auth existed).
- [SPEC-002](SPEC-002-agent-enrollment.md) — agent identity (mTLS/DPAPI); the secret-handling template; the *agent* trust boundary distinct from this human one.
- [SPEC-009 (forthcoming)] — the read-slice (incident/alert list + detail + drill) gated behind this SPEC's auth; owns the incident/alert RBAC matrix rows.
- [Threat model](../security/threat-model.md) — § `cg-api` (`:81` Argon2/TOTP/sessions/rate-limit, `:82` CSRF, `:83` audit fields, `:84-86` RBAC CI tests + audit/role trail) and § Dashboard (`:184` CSRF, `:186` cookies, `:88` WebAuthn deferred) — the binding requirement surface.
- `services/ingest/src/ca.ts` — the pgcrypto `pgp_sym_encrypt`/`decrypt` at-rest precedent (`:85-103`) the TOTP-secret encryption mirrors.
- `services/ingest/src/config.ts` — the Zod env-validation pattern (`:44-108`); `INGEST_REDIS_URL` already required (`:51`); `INGEST_CA_PASSPHRASE` the secret-passphrase precedent (`:61`).
- `services/ingest/src/db/migrate.ts` — the advisory-lock migration applier (`:19`, `:50-85`) **whose pattern `services/api`'s own migration runner mirrors** (its own ledger table + advisory-lock key; §Operational §7). The auth migrations do not extend the ingest chain.
- [Blueprint](../product/blueprint.md) §18 (the MVP "creates a user with OTP" + admin "generates an enrollment token", `:740`/`:748`), §10 (SOAR; the richer role set deferred), §17 (single-org MVP).
