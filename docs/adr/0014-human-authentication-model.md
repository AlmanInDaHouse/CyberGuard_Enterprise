# ADR-0014: Human authentication model — local self-hosted, password + TOTP

- Status: Proposed
- Date: 2026-06-01
- Last updated: 2026-06-01
- Deciders: Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

Phase 6 A (SPEC-007) closed the detection pipeline at the `alert → incident` link. Phase 6 C is the user-facing slice — the SOC dashboard and the API behind it (Blueprint §18 MVP: *"An administrator … creates a user with OTP. From the dashboard, generates an enrollment token … the SOC sees the alert …"*). The user-facing slice cannot be specified until one architectural fork is settled first: **how human operators authenticate.** This ADR settles that model and nothing below it, the same way ADR-0013 settled the incident-windowing *basis* before SPEC-007 specified the mechanics.

A read-only audit-first pass (Session 19, the C audit) surfaced the repo-grounded facts that force this decision:

- **No human authentication exists anywhere.** The codebase has agent identity only: X.509 + mTLS + DPAPI-protected Ed25519 keys (SPEC-002). The one HTTP surface, `services/ingest`, authenticates *agents* by client certificate at the transport layer (`services/ingest/src/app.ts:24-31`), not humans. There is no user store, no session, no password hashing, no TOTP, no login route, and no application-level auth middleware. Migrations `0001`–`0005` define `ca`, `agents`, `enrollment_tokens`, `alerts`, `detect_watermark`, `incidents` — no `users`, `sessions`, `roles`, or `audit_log`. Human auth is **greenfield in code**.
- **The storage homes are already decided.** ADR-0003 routes *"Relational state, RBAC, cases, audit log"* to PostgreSQL (`docs/adr/0003-polyglot-storage.md:28`, and §Context:9 names "users, agents, cases, RBAC" explicitly) and *"Cache, sessions, rate limit, SOAR locks"* to Redis (`:31`). Redis is already provisioned in dev (`infra/dev/docker-compose.dev.yml:55`; `INGEST_REDIS_URL` wired at `:134`). This ADR therefore does **not** re-decide storage; it decides the auth *model* that lands on those homes.
- **The security requirements are already prescribed.** The threat model's `cg-api` section is the authoritative requirement surface (assets at risk: session tokens, TOTP secrets, RBAC assignments, audit-log mutations): password hashing with **Argon2 or bcrypt**, **RFC 6238 TOTP enforced on every login**, **short-lived signed sessions with rotation**, login rate-limiting, object-level authorization on every read/write, and **RBAC tests that block CI** (`docs/security/threat-model.md:81-86`); the dashboard section mandates `HttpOnly` `Secure` `SameSite=Strict` cookies (`:186`). These are requirements this ADR's model must satisfy, not choices it makes.
- **A source contradiction must be resolved.** The Blueprint technology table's Auth row reads *"Own OAuth2/OIDC + TOTP (RFC 6238)"* (`docs/product/blueprint.md:130`), which could be read as federation. The threat model (local password + Argon2 + TOTP) and the MVP definition (*"creates a user with OTP"*, *"OTP login, RBAC with 3 roles"* — `:738`, `:746`) describe a **self-hosted local identity**. This ADR resolves the contradiction in §1.

This ADR decides the model across four coupled questions (identity source, session form, API home, RBAC shape). It deliberately does **not** decide the login flow, the table DDL, the role-capability matrix, session TTLs, rate-limit thresholds, or any harness — those are the C auth SPEC's, per §Out of scope.

## Decision

### 1. Identity is local and self-hosted — no federated OIDC / external IdP

Human identity is a **first-party local store**: a username/email + a password hashed with **Argon2 or bcrypt** (threat model `:81`), plus a per-user **RFC 6238 TOTP** factor enforced on **every** login. CyberGuard authenticates standalone; it does **not** depend on an external identity provider (Auth0, Okta, Keycloak, Entra) to authenticate an operator.

Rationale: the product is **self-deployable** for SMB / mid-market operators who *"cannot afford commercial XDR"* (Blueprint §2 self-deployable promise; `docs/product/blueprint.md:31`). Requiring an external IdP to log in would break that promise and add an operational dependency those operators cannot carry. This also resolves the source contradiction: *"Own OAuth2/OIDC"* (`blueprint.md:130`) is interpreted as **own / self-hosted** identity (CyberGuard issues and validates its own credentials and session tokens), **not** federation. Where the Blueprint row and the threat model differ, the threat model and the MVP definition (local password + TOTP) govern.

### 2. Session form: opaque server-side token in Redis (stateful, immediately revocable)

A successful password+TOTP login establishes a session represented by an **opaque, high-entropy token** held in a server-side **Redis** session record, delivered to the browser in an `HttpOnly` `Secure` `SameSite=Strict` cookie. The token carries **no client-trusted claims**; all authority (user, role, expiry) lives server-side and is read per request.

This is the real fork, decided with its trade-off:

- **Chosen — opaque token in Redis (stateful).** *Pro:* **immediate revocation** — logout, a forced password reset, a role change, or a suspected compromise invalidates a session immediately (a single server-side delete); a security tool must be able to kill an operator session *now*. Short TTL and rotation are native to Redis TTL semantics. It is consistent with ADR-0003's session home (`0003-polyglot-storage.md:31`) and needs no new infrastructure (Redis is already a hard stack dependency). *Con:* stateful — Redis must be available on the auth path, and each authenticated request does one O(1) Redis read. Accepted: Redis is already required by ADR-0003 and the BFF already needs it for rate-limiting.
- **Rejected — stateless JWT (see A2).** Revocation needs a denylist that reintroduces the very server-side state JWT exists to avoid, and a stateless model contradicts ADR-0003 §sessions→Redis (it would require an amendment). The threat model's *"signed sessions with rotation"* requirement is satisfied by the opaque-token model (integrity comes from opacity + server-side validation; rotation from re-issue); the exact cookie-signing mechanism is a SPEC detail.

ADR-0003's placement of sessions in Redis is weighed as a **strong signal**, not the sole reason; the deciding factor is revocation immediacy for a security product.

### 3. The human-facing API is a new `services/api`, distinct from the agent ingest

The human auth + read API is the **`services/api`** component — TypeScript + Fastify + Zod, as ADR-0002 already assigns (`docs/adr/0002-language-per-component.md:13`, `:39`). It is **not** folded into `services/ingest`.

Rationale: `services/ingest` is the **agent** trust boundary (mTLS client-cert, machine identity); the human API is a **different** trust boundary — the threat model calls `cg-api` *"the only path through which users mutate state"* with its own asset set (`threat-model.md:75`). Conflating human password/TOTP/session handling into the agent-mTLS service would merge two trust boundaries that the threat model keeps separate. The existing Fastify+Zod scaffold (`services/ingest/src/app.ts`) is reused as a **pattern**, not as a host. *Context, not corrected here:* SPEC-002 §Out of scope (`docs/specs/SPEC-002-agent-enrollment.md:40`) forward-referenced the enrollment endpoint as living "under `services/api/`"; SPEC-004 landed it in `services/ingest/src/routes/enroll.ts` instead. That stale forward-reference is noted as context; this ADR does not amend SPEC-002.

### 4. RBAC: three fixed roles (admin / analyst / viewer), enforced server-side

The MVP authorization model is **exactly three roles — `admin`, `analyst`, `viewer`** (`blueprint.md:746`), each user carrying one role on the user record (Postgres, per ADR-0003 RBAC→PG). Enforcement is **server-side in the BFF**, on every read and write; the client is never trusted for role. The richer `analyst_l2` / `soc_manager` set seen in the SOAR playbook example (`blueprint.md:439`) is a **later phase**, not MVP.

This ADR fixes the role *set*, the *home* of the role (server-side, on the user record), and *where* enforcement runs (the BFF, not the UI). The detailed **role × capability matrix** — which role may do what on which endpoint — is the C auth SPEC's, and per the threat model that matrix must be covered by **CI-blocking tests** (`threat-model.md:84-86`).

## Alternatives considered

### A1 — Federated identity (external OIDC / IdP)

*Pros:* offloads password storage, MFA, and account lifecycle to a hardened provider. *Cons:* an external runtime dependency that breaks the self-deployable promise (Blueprint §2) for the SMB target; operational burden of running or contracting an IdP; contradicts the threat model's local password+TOTP requirements and the MVP's "creates a user with OTP". **Rejected** — closed decision (Manuel, Session 19 gate input).

### A2 — Stateless JWT sessions

*Pros:* no session-store read per request; horizontally trivial. *Cons:* revocation requires a denylist (re-introducing server-side state, defeating the premise); cannot immediately kill a compromised operator session without extra infrastructure; contradicts ADR-0003 §sessions→Redis and would force an amendment. **Rejected** in favour of opaque-Redis (§2).

### A3 — Session state in a Postgres table

*Pros:* a single store; transactional with the user and audit rows. *Cons:* ADR-0003 explicitly homes sessions in Redis (TTL-native, ephemeral by design); a Postgres session table contradicts that routing and adds churn-heavy write/expiry load to the relational system of record. **Rejected** — Redis is the decided, fit-for-purpose home.

### A4 — Host the human API by extending `services/ingest`

*Pros:* reuse the running service and scaffold; one fewer process. *Cons:* merges the agent-mTLS trust boundary with the human-auth trust boundary the threat model keeps distinct; `services/ingest` is agent-facing by construction (ADR-0007). **Rejected** — reuse the Fastify+Zod *pattern*, not the service (§3).

## Consequences

### Positive

- **Self-deployable auth:** no external IdP is needed to log in; the product authenticates standalone, preserving the SMB promise (Blueprint §2).
- **Immediate session revocation** (opaque-Redis) — a first-class property for a security tool: compromised or role-changed sessions die on a single delete.
- **No storage re-decision and no ADR amendment** — the model lands on ADR-0003's existing homes (users/RBAC/audit → Postgres, sessions → Redis).
- **Clean trust-boundary separation** — agent mTLS (`services/ingest`) and human password+TOTP (`services/api`) stay distinct, matching the threat model.

### Negative

- **New greenfield surface** the C SPEC must design: `users`, `roles` / role assignment, and `audit_log` tables (Postgres), plus the Redis session keyspace. This ADR *names* them as consequences; it does not design them.
- **A second always-on service** (`services/api`) and **Redis on the auth path**. Mitigated: both are already in the stack (ADR-0001 places `services/api`; ADR-0003 makes Redis a hard dependency).
- **A new at-rest secret class** — password hashes and TOTP secrets (a threat-model asset). The `pgcrypto` precedent (`services/ingest/src/db/migrations/0001_initial.ts:7`, encrypting the CA key) and SPEC-002's secret-handling template (`SPEC-002-agent-enrollment.md:250`) inform the SPEC; this ADR only flags the requirement.
- **RBAC enforcement becomes a CI gate** — the role-capability matrix must be test-covered or CI fails (threat model `:84-86`). A consequence for the C harness, not a cost of this ADR.

### Neutral

- This ADR **amends no Accepted ADR.** The model *confirms* ADR-0003 (users/RBAC/audit → Postgres; sessions → Redis) and ADR-0001/0002 (`services/api` = TypeScript/Fastify). Following the dep-graph convention (constraint-bearing edges only; an explained absence beats an invented edge), the README edges added at Accept are **dependencies, not amendments** — in contrast to ADR-0013, which amended ADR-0012 §8. *Flagged for the gate:* this no-amendment property is **contingent on §2** — had the session form been stateless JWT (A2), it would have contradicted ADR-0003 §sessions→Redis and required an ask-first amendment.
- Multi-tenant / org-scoped RBAC stays out of scope (single `org_id`, Blueprint §17.1); the three roles are global within the single MVP org.

## Compliance

- Human authentication MUST be local: password (Argon2 or bcrypt) + RFC 6238 TOTP on every login; no external IdP is required to authenticate (§1).
- Sessions MUST be opaque server-side tokens in Redis — short-TTL, rotated, immediately revocable (the threat model's signed-session-with-rotation intent, `:81`, realized as an opaque server-side token per §2) — delivered in `HttpOnly` `Secure` `SameSite=Strict` cookies (`:186`).
- Login MUST be rate-limited with progressive back-off per account and per source IP (threat model `:81`); the concrete thresholds are SPEC-level (§Out of scope).
- The human-facing API MUST live in `services/api` (TypeScript / Fastify / Zod), separate from the agent ingest trust boundary (§3).
- RBAC MUST enforce exactly `admin` / `analyst` / `viewer` server-side on every read and write; the role × capability matrix MUST be covered by tests that block CI (§4; threat model `:84-86`).
- All authentication events and role assignments MUST be audit-logged (threat model `:86`, `:196`).

## Out of scope

Each deferred item names its destination:

- **Login flow, session TTL / rotation cadence, rate-limit thresholds, Argon2 parameters, cookie-signing mechanism.** The C auth SPEC (the mechanics).
- **CSRF defense** (Fastify-issued tokens on every mutation; threat model `:82`, `:184`). The C auth SPEC — a session/transport mechanic, not one of the four model forks settled here, but named so it is not silently dropped.
- **Table DDL** (`users`, `roles` / role assignment, `audit_log`) and the **Redis session keyspace** shape. The C auth SPEC's migrations.
- **The detailed role × capability matrix** and its CI-blocking tests. The C auth SPEC.
- **First-admin bootstrap** (how the initial `admin` is provisioned on a fresh install). The C auth SPEC.
- **Password reset / account recovery flows.** Depend on the notifier/SMTP slice (`services/soar` is README-only today); a later phase. TOTP enrollment that needs no delivery (on-screen QR at first login) stays available without it.
- **WebAuthn / passkeys.** Enhancement target, not MVP (threat model `:88`).
- **Multi-tenant / org-scoped RBAC.** A future multi-tenancy ADR (Blueprint §17.1).
- **Service-account / external-API tokens** (machine-to-API, distinct from human login). A later phase.
- **The alert/incident read API and WebSocket push surface.** The read slice of C (its own SPEC); this ADR is the auth model only.

## Landing checklist (atomic on flip to Accepted)

When this ADR is ratified Proposed→Accepted, the same commit:

1. Flips the status header to `Accepted`.
2. Adds the catalog row to `docs/adr/README.md`.
3. Adds these dependency edges to `docs/adr/README.md` §Dependencies (all **dependencies**, none an amendment):
   - `ADR-0014 → ADR-0001` (locates the human-facing `services/api` component in the monorepo).
   - `ADR-0014 → ADR-0002` (`services/api` = TypeScript + Fastify + Zod, per the language table row).
   - `ADR-0014 → ADR-0003` (consumes the storage homes: users / RBAC / audit_log → Postgres, sessions → Redis; does **not** amend).
4. Adds **no** amendment block — this ADR amends no Accepted ADR (see §Consequences > Neutral; the no-amendment property is contingent on §2's session-form choice).

## References

- [ADR-0003](0003-polyglot-storage.md) — Polyglot storage. The storage homes this ADR consumes: §Decision table "Relational state, RBAC, cases, audit log" → Postgres (`:28`) and "Cache, sessions, …" → Redis (`:31`). Consumed, not amended.
- [ADR-0001](0001-monorepo-layout.md) — Monorepo layout. Places `services/api/` as a top-level component.
- [ADR-0002](0002-language-per-component.md) — Language per component. Assigns `services/api/` to TypeScript / Fastify + Zod (`:13`, `:39`); its §A1/§A2 cons (a Rust or Python BFF loses dashboard type-sharing and weakens untrusted-input validation) reinforce TypeScript for the human API.
- [ADR-0004](0004-agent-server-protocol.md) — Agent-server protocol. The *agent* trust boundary (mTLS + Redis revocation list) this ADR contrasts with; not the human boundary.
- [ADR-0013](0013-incident-correlation-windowing.md) — The precedent: an ADR that decides one architectural basis before the SPEC specifies mechanics. This ADR follows the same single-layer discipline.
- [SPEC-002](../specs/SPEC-002-agent-enrollment.md) — Agent enrollment. §Out of scope (`:40`) forward-referenced the enroll endpoint to `services/api` (stale; SPEC-004 landed it in `services/ingest` — noted as context, not corrected here); §Security considerations (`:250`) is the secret-handling template the C SPEC inherits.
- [Threat model](../security/threat-model.md) — § `cg-api` (`:75-86`) is the authoritative auth-requirement surface (Argon2/bcrypt, TOTP every login, signed/rotating sessions, server-side object-level RBAC, CI-blocking RBAC tests, audit) and § Dashboard (`:186`) the cookie posture this model must satisfy.
- [Blueprint](../product/blueprint.md) — §2 (self-deployable promise; `:31` SMB target), §18 MVP (`:738`, `:746` — "creates a user with OTP", OTP login + three roles), and the technology table Auth row (`:130`, "Own OAuth2/OIDC + TOTP" — read as own/self-hosted identity, not federation).
