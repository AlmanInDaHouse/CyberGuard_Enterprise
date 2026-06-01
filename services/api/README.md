# services/api

CyberGuard human-facing API / BFF — TypeScript + Fastify + Zod. The user-facing
trust boundary (ADR-0014 §3), distinct from the agent-mTLS `services/ingest`.

Realises **SPEC-008 (auth-core)**:

- **Local self-hosted identity** — password (Argon2id) + RFC 6238 TOTP on every
  login. No federated OIDC / external IdP (ADR-0014 §1).
- **Opaque server-side sessions in Redis**, immediately revocable (the product
  invariant, ADR-0014 §2); `HttpOnly` `Secure` `SameSite=Strict` cookies.
- **Server-side RBAC** — three roles `admin` / `analyst` / `viewer` (ADR-0014 §4).
- **Login rate-limiting** (per account + per source IP), **CSRF** tokens on every
  mutation, and an append-only **audit_log**.
- Its **own** migrations under `src/db/migrations/` with its **own** runner
  (own Kysely ledger table + advisory-lock key), against the shared Postgres
  (ADR-0003); Redis for sessions + rate-limit.

The incident/alert read surface and the dashboard are **SPEC-009** (the read-slice).

**Status:** scaffolded at the SPEC-008 harness-first RED gate — the structure
(migrations + Fastify scaffold) is green; the auth logic is stubbed and the nine
`auth_ac_*` acceptance tests fail by design until the GREEN gate lands the
implementation.
