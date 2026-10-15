# SPEC-004: Server ingest minimal

- **ID:** SPEC-004
- **Title:** Server ingest minimal
- **Status:** Accepted
- **Depends on:** ADR-0002, ADR-0003, ADR-0004, ADR-0007, SPEC-002, SPEC-003
- **Authors:** Manuel (product owner), Claude (architecture advisor), Claude Code (implementation)
- **Created:** 2026-05-22
- **Last updated:** 2026-05-22

## Motivation

Phase 1 built a real agent: it enrolls (SPEC-002), gets an X.509 identity, and heartbeats over mTLS with a signed envelope (SPEC-003). But it has had nothing real to talk to — every test to date used an in-process mock server.

SPEC-004 builds the first real server: a `services/ingest/` TypeScript + Fastify service (per ADR-0007) that terminates the agent protocol and **persists**. After SPEC-004, `task dev:up` brings the whole local stack up, and a real `cg-agent` connecting to it lands actual rows in actual Postgres and actual ClickHouse. The end-to-end loop — real agent → real server → real persistence — closes for the first time and becomes locally reproducible. That is the product's "wow moment," and the marquee acceptance criterion (AC-001) exists to prove it.

Scope is deliberately minimal: ingest + persist + reject. No read/query API, no dashboard, no alerts, no SOAR — only the two agent endpoints and exactly what they need.

## Scope

### In scope

- A Fastify service at `services/ingest/` (TypeScript, per ADR-0007).
- `POST /v1/agents/enroll` over plain HTTP — the SPEC-002 contract: validate a single-use token, issue an Ed25519 client certificate, persist the agent, return the SPEC-002 response.
- `POST /v1/agents/heartbeat` over **mTLS 1.3** — the SPEC-003 contract: validate the client cert, the outer signed envelope, the signature, the nonce (anti-replay), and the timestamp window, then persist the heartbeat.
- Persistence: agent identities + tokens + CA in **PostgreSQL**; heartbeats in **ClickHouse**; anti-replay nonces in **Redis** (ADR-0003).
- Server-local CA: a self-signed Ed25519 root generated at first run, used to issue client certs (ADR-0004 §Enrollment).
- Enrollment-token issuance via a **CLI** subcommand (no admin HTTP endpoint in SPEC-004).
- Structured JSON logging (pino), graceful shutdown, a `/health` endpoint.
- An integration harness with one test per acceptance criterion, including the end-to-end marquee test with a real `cg-agent`.

### Out of scope (deferred)

- **Query/read API** for reading heartbeats or agents back out — that is the API SPEC (`services/api/`), future.
- **Dashboard, alerting, SOAR, correlation pipeline, CGES events.** SPEC-004 carries only transport-level meta-messages (enroll + heartbeat), not CGES events. The high-throughput event firehose is deferred per ADR-0007.
- **Multi-tenancy / `org_id` semantics.** A single physical `org_id` column exists (default `"default"`) but is not used for isolation, consistent with ADR-0003 Rule 2 and Blueprint §17.1.
- **RBAC / admin user model.** Token issuance runs as whoever has shell access to the server host (a CLI). Real RBAC is a future SPEC. See §Ratification record.
- **Certificate rotation / renewal / revocation enforcement.** SPEC-002/003 deferred rotation; the server issues 90-day certs and does not yet rotate or check a revocation list (the `agents` row has the data a future revocation SPEC needs).
- **Log forwarding, metrics export, OpenTelemetry.** Deferred to an ops SPEC.
- **Horizontal-scale orchestration** (k8s, autoscaling). The service is *designed* stateless (all state in PG/CH/Redis) so it can scale by adding instances, but standing up that orchestration is out of scope.

## Functional requirements

### Application structure

- **FR-001.** The service is a TypeScript ESM project at `services/ingest/`, Node.js **22 LTS**, built with Fastify 5 and pino logging. Untrusted input (both request bodies and the CLI) is validated with Zod schemas before use.
- **FR-002.** Because the two endpoints have incompatible TLS requirements — enroll must accept clients that have **no** certificate yet, heartbeat must **require** a client certificate — the service runs **two listeners**: a plain-HTTP server for `/v1/agents/enroll` (default port `8080`) and an mTLS server for `/v1/agents/heartbeat` (default port `8443`). Both are built from a shared services layer (DB, CA, crypto, Redis, ClickHouse) so the persistence and validation logic is written once. A single `/health` endpoint is served on the plain-HTTP listener.

### Enrollment — `POST /v1/agents/enroll` (plain HTTP)

- **FR-003.** The request body is the SPEC-002 enrollment request: `{ envelope_version, enrollment_token, agent_pubkey (base64url, 32 bytes), agent_hostname, agent_platform, agent_version }`. The body is Zod-validated; a malformed body is `400`.
- **FR-004.** Token validation is single-use and server-side, backed by the Postgres `enrollment_tokens` table. The token state machine is `issued → consumed | expired`. A token is accepted only if it exists, is in state `issued`, and `expires_at > now()` (TTL 15 min per ADR-0004). Consumption is **atomic**: a single SQL `UPDATE … SET state='consumed', consumed_at=now(), consumed_by_agent_id=$id WHERE token=$t AND state='issued' AND expires_at > now() RETURNING …`. If it returns no row, the token is rejected (`401`) or, if it exists in state `consumed`, conflict (`409`). This makes a token race-safe: two concurrent requests cannot both consume it.
- **FR-005.** On a valid token, the server assigns a fresh `agent_id` (UUIDv7), issues an Ed25519 X.509 client certificate (FR-006), persists the agent row (FR-007), and returns the SPEC-002 response: `{ envelope_version, agent_id, client_certificate (PEM), issued_at, expires_at }` with HTTP `200`.
- **FR-006. Certificate issuance.** The server's CA is a self-signed Ed25519 root, generated at first run and stored in the `ca` table (single row). Client certs are issued with `CN = agent_id`, the agent's submitted 32-byte Ed25519 public key as the subject key, signed by the CA's Ed25519 private key, 90-day TTL (ADR-0004). Issuance uses `@peculiar/x509` over Node's native WebCrypto, **verified viable** during SPEC-004 sanity reads (ADR-0007 §Context). The verified path the implementation (B5) follows:

  ```ts
  import "reflect-metadata";              // required by @peculiar/x509 v2 (tsyringe DI)
  import * as x509 from "@peculiar/x509";
  const crypto = globalThis.crypto;       // Node 20+ native WebCrypto
  x509.cryptoProvider.set(crypto);
  const ALG = { name: "Ed25519" };

  // agent_pubkey arrives as 32 raw bytes (base64url-decoded from the request)
  const subjectKey = await crypto.subtle.importKey("raw", rawPub, ALG, true, ["verify"]);
  const cert = await x509.X509CertificateGenerator.create({
    serialNumber, subject: `CN=${agentId}`, issuer: caCert.subject,
    notBefore, notAfter,                  // 90-day TTL
    signingAlgorithm: ALG,
    publicKey: subjectKey,                // raw 32-byte key, no DER/PEM pre-wrap
    signingKey: caPrivateKey,             // CA Ed25519 private key
    extensions: [ new x509.KeyUsagesExtension(x509.KeyUsageFlags.digitalSignature, true) ],
  }, crypto);
  const pem = cert.toString("pem");
  ```

  `@peculiar/x509` 2.0.0 on Node 24.12.0 was confirmed to issue such a cert, verify it against the CA, and round-trip the subject key back to the identical 32 bytes.

- **FR-007. Agent persistence.** On enrollment the server inserts a row into the Postgres `agents` table: `agent_id`, `org_id` (default `"default"`), `pubkey` (32 bytes), `cert_pem`, `enrolled_at`, `expires_at`, `last_seen` (null until the first heartbeat). The insert and the token consumption (FR-004) occur in the **same transaction**: either the agent is enrolled and the token consumed together, or neither happens.

### Heartbeat — `POST /v1/agents/heartbeat` (mTLS)

- **FR-008.** The listener requires a client certificate (`requestCert: true, rejectUnauthorized: true`) verified against the server CA at the TLS layer. A client with no certificate, an expired/altered certificate, or a certificate not chaining to the CA fails the TLS handshake — which is exactly what SPEC-003 AC-008 expects (the agent exits `7`). No application code runs for a failed handshake.
- **FR-009.** The request body is the SPEC-003 outer signed envelope: `{ outer_envelope_version, agent_id, sequence_number, nonce (base64url, 16 bytes), sent_at, body (the SPEC-001 inner envelope), signature (base64url Ed25519) }`. Zod-validated; malformed ⇒ `400`.
- **FR-010. Server validation order** (ADR-0004 §Server validation order), rejecting on the first failure:
  1. **mTLS handshake** completed with a cert chaining to the CA (FR-008, TLS layer).
  2. The envelope `agent_id` equals the **CN** of the presented client certificate, and that `agent_id` exists in the `agents` table. Mismatch / unknown ⇒ `401`.
  3. `sent_at` is within **±5 min** of server time. Outside ⇒ `422`. "Server time" is the Node process wall clock (`Date.now()`) at the moment validation runs, under the assumption that the host is NTP-synced. The service does **not** validate its own clock against an external source; clock skew on the server host becomes server skew. Running NTP on the server host is an **operational prerequisite, documented not enforced** (the agent host has the same NTP requirement per ADR-0004 §Consequences).
  4. `nonce` has **not** been seen — checked against Redis (FR-011). Seen ⇒ `409`.
  5. `signature` verifies as Ed25519 over the **canonical body** (RFC 8785 JCS of the outer envelope minus the `signature` field — SPEC-003 §Data contracts) under the agent's public key (from the client cert / `agents` row). Invalid ⇒ `401`.
  6. On success: persist the nonce in Redis (FR-011), persist the heartbeat in ClickHouse (FR-012), update `agents.last_seen`, return `200`.

  `sequence_number` is recorded but not used for rejection in SPEC-004 (cross-restart monotonic sequence is deferred per SPEC-003 drift D3). The server canonicalizes with the `canonicalize` npm package (RFC 8785); cross-language byte-identity with the agent's `serde_jcs` holds for the envelope's value space (ASCII strings, unsigned integers, nested objects — no floats), and the marquee AC proves the interop end-to-end.
- **FR-011. Nonce store.** Nonces are stored in Redis with `SET nonce:<b64> 1 NX EX 600` (10 min TTL — comfortably above the ±5 min skew window so a replay cannot outlive its timestamp validity). `NX` makes the check-and-set atomic: if the key already exists, it is a replay (`409`).
- **FR-012. Heartbeat persistence.** Each accepted heartbeat is one row in the ClickHouse `heartbeats` table: the inner-envelope fields (`agent_id`, `sequence_number`, `inner_sent_at`, `status`, `uptime_seconds`), the outer fields (`outer_sent_at`, `nonce`), and the server `arrived_at`. Ordered by `(agent_id, arrived_at)`, partitioned by `toYYYYMMDD(arrived_at)`. The insert is **synchronous**: the server `await`s the ClickHouse acknowledgement before returning `200` and does **not** use `async_insert`. So a `200` means the row is durable and immediately queryable — this is what makes the cross-store ordering (§Behavior) well-defined and lets the marquee AC read the row back with no buffer-flush race.

### Errors, logging

- **FR-013.** Error responses match SPEC-002/003 expectations. Enroll: `400` malformed, `401` token invalid/expired, `409` token already consumed. Heartbeat: `400` malformed, `401` unknown/mismatched agent or bad signature, `409` replayed nonce, `422` timestamp skew. The SPEC-003 agent treats any non-2xx heartbeat response as a non-fatal rejection (warn, next interval), so these codes do not destabilise it.
- **FR-014.** All logs are JSON lines via pino (Fastify's default), one object per line, the same convention as the agent. Secrets are never logged: not the CA private key, not enrollment tokens, not signatures at info level. Per-request: method, path, status, duration, and `agent_id` once known.

### Token issuance (CLI)

- **FR-015.** Enrollment tokens are issued by a CLI subcommand, `task ingest:issue-token --org <id>` (no HTTP admin endpoint in SPEC-004). It inserts an `enrollment_tokens` row in state `issued` with a 15-min `expires_at`, prints the opaque token once to stdout, and exits. The token is a 32-byte `OsRng` value, base64url-encoded.

## Non-functional requirements

- **NFR-001.** Heartbeat endpoint p99 latency `< 100 ms` under nominal local load (the hot path: TLS already established, one Redis `SET NX`, one Ed25519 verify, one ClickHouse async insert, one Postgres `last_seen` update). Enroll endpoint p99 `< 1 s` (cert issuance + a transaction; rare).
- **NFR-002.** The service is **stateless**: all state lives in Postgres, ClickHouse, and Redis. Scaling is by adding instances behind a load balancer; no instance holds authoritative state. (Standing up that orchestration is out of scope; the design constraint is in scope.)
- **NFR-003.** Graceful shutdown: on `SIGINT`/`SIGTERM` the service stops accepting new connections, drains in-flight requests (Fastify `close()`), flushes the ClickHouse insert buffer, and closes PG/Redis pools before exit.
- **NFR-004.** Source passes `tsc --noEmit` (strict), the linter/formatter (biome), and the test suite (vitest) in CI (`ts-ci`).
- **NFR-005.** No secret material in logs (reaffirms SPEC-002 NFR-002 / SPEC-003 NFR-003 on the server side): CA private key, tokens, and raw signatures are never logged.

## Data contracts

### Reused envelopes (no new wire schema)

The enrollment request/response (SPEC-002 §Data contracts) and the outer signed envelope (SPEC-003 §Data contracts) are the wire contracts; SPEC-004 does not change them. They remain transport-level meta-messages, not CGES events, so no `schemas/cges/` schema is introduced (consistent with SPEC-001/002/003).

### PostgreSQL schema

```sql
-- single-row CA (server identity for issuing client certs)
CREATE TABLE ca (
  id           smallint PRIMARY KEY DEFAULT 1 CHECK (id = 1),
  cert_pem     text        NOT NULL,
  private_key  bytea       NOT NULL,   -- protection per §Ratification (CA key storage)
  created_at   timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE agents (
  agent_id    uuid        PRIMARY KEY,
  org_id      text        NOT NULL DEFAULT 'default',
  pubkey      bytea       NOT NULL,    -- 32-byte Ed25519 public key
  cert_pem    text        NOT NULL,
  enrolled_at timestamptz NOT NULL DEFAULT now(),
  expires_at  timestamptz NOT NULL,
  last_seen   timestamptz
);

CREATE TYPE token_state AS ENUM ('issued', 'consumed', 'expired');
CREATE TABLE enrollment_tokens (
  token               text        PRIMARY KEY,   -- opaque base64url
  org_id              text        NOT NULL DEFAULT 'default',
  scope               text        NOT NULL DEFAULT 'enroll',
  state               token_state NOT NULL DEFAULT 'issued',
  issued_at           timestamptz NOT NULL DEFAULT now(),
  expires_at          timestamptz NOT NULL,
  consumed_at         timestamptz,
  consumed_by_agent_id uuid REFERENCES agents(agent_id)
);
```

### ClickHouse schema

```sql
CREATE TABLE heartbeats (
  agent_id        UUID,
  sequence_number UInt64,
  inner_sent_at   DateTime64(3, 'UTC'),
  outer_sent_at   DateTime64(3, 'UTC'),
  status          LowCardinality(String),
  uptime_seconds  UInt64,
  nonce           String,
  arrived_at      DateTime64(3, 'UTC') DEFAULT now64(3)
) ENGINE = MergeTree
PARTITION BY toYYYYMMDD(arrived_at)
ORDER BY (agent_id, arrived_at);
```

Heartbeats are transport meta-messages, so the ordering key is heartbeat-appropriate (`agent_id, arrived_at`) rather than the CGES `(org_id, occurred_at, event_id)` of ADR-0003 Rule 2 — which governs CGES events, a future concern.

### Migrations

Schema is created by the kysely migrator (timestamped migration files under `services/ingest/src/db/migrations/`). One initial migration creates the Postgres tables; the ClickHouse table is created by a small bootstrap step (kysely targets PG; ClickHouse DDL runs via `@clickhouse/client` at migrate time).

**Migration runtime ownership (single applier).** Migrations are applied by a dedicated `task ingest:migrate` step, or by a single designated service instance gated on `INGEST_RUN_MIGRATIONS=true` (**default `false`**). Plain container start does **not** run migrations unless that flag is set — so scaling to N instances (NFR-002) cannot have N instances race to migrate. Migrations are **idempotent** (`CREATE TABLE IF NOT EXISTS`, `CREATE TYPE` guarded by existence checks, ClickHouse `CREATE TABLE IF NOT EXISTS`) and sequenced by kysely's migration version table. Concurrent application of the same migration is made safe by wrapping the Postgres migration run in a session-level **advisory lock** (`pg_try_advisory_lock(<fixed key>)`; skip if not acquired), so even two instances that both have the flag set cannot double-apply. This matters for SPEC-004 specifically because the marquee AC-001 may stand the service up multiple times in one test run.

## Configuration

Environment variables (validated by Zod at startup; the service refuses to start on a missing/invalid one):

```text
INGEST_PG_URL            postgres://…            # Postgres connection
INGEST_CH_URL            http://…:8123           # ClickHouse HTTP
INGEST_REDIS_URL         redis://…:6379          # Redis
INGEST_ENROLL_PORT       8080                    # plain-HTTP enroll listener
INGEST_HEARTBEAT_PORT    8443                    # mTLS heartbeat listener
INGEST_BIND_HOST         127.0.0.1               # bind address, both listeners (see Amendment 2026-09-21)
INGEST_SERVER_CERT_PATH  /…/server.pem           # server TLS identity (see §Ratification)
INGEST_SERVER_KEY_PATH   /…/server-key.pem
INGEST_CA_PASSPHRASE     <secret>                # CA private-key protection (see §Ratification)
INGEST_RUN_MIGRATIONS    false                   # if true, this instance applies migrations on start (single applier)
INGEST_LOG_LEVEL         info
```

Defaults target `task dev:up`'s backends. The two ports are separate (rather than one port with SNI) for implementation simplicity and because the two listeners have different TLS requirements anyway.

## Behavior

**Enroll lifecycle:** receive → Zod-validate body → begin PG transaction → atomically consume token (FR-004) → assign `agent_id` → issue cert (FR-006) → insert `agents` row → commit → return SPEC-002 response. Any failure rolls the transaction back (no half-enrolled agent, no consumed-but-unissued token).

**Heartbeat lifecycle:** TLS handshake (mTLS, client cert validated by Node TLS against the CA) → receive → Zod-validate → run the §FR-010 validation order → on success `SET nonce NX EX 600`, insert ClickHouse row, update `last_seen`, return `200`. On any validation failure, return the mapped status (§FR-013) and log at `warn`; no persistence occurs.

**First run:** if the `ca` table is empty, generate the Ed25519 CA root and insert the single row before serving. The server TLS identity is materialised per §Ratification: the server **generates a self-signed/self-issued cert+key only if `INGEST_SERVER_CERT_PATH`/`INGEST_SERVER_KEY_PATH` are absent**; if a cert already exists at those paths it is used as-is and **never overwritten** (an operator-provided cert is thus honoured, and a regenerated CA cannot silently clobber a deployed identity). Generation creating only one of the two files, or an unreadable existing pair, is a fatal start-up error rather than a silent regenerate.

### Cross-store ordering and partial-failure semantics

A successful heartbeat touches three stores and there is **no cross-store transaction**. The order is fixed and each partial-failure fork has a committed behavior, so B5 implements a specified contract rather than discovering it under load:

**Order:** Redis nonce `SET NX` → ClickHouse insert → Postgres `last_seen` UPDATE.

- **Redis fails** (cannot `SET NX`): respond `503`, no ClickHouse row, no `last_seen` update. The nonce was never persisted, so the agent's next-interval retry carries the **same** nonce and is accepted normally. No loss.
- **ClickHouse fails after a successful Redis `SET NX`:** respond `503`. The nonce is now **burned** (it is in Redis for its TTL and cannot be replayed), the heartbeat is **lost** from ClickHouse, and `last_seen` is not updated. The agent retries next interval with a **new** nonce, which is accepted and produces a new ClickHouse row + `last_seen`. The single lost heartbeat is **accepted data loss** for SPEC-004 — there is no retry queue or WAL; durability of the event stream is a concern for the future firehose SPEC (ADR-0007), not this control-plane minimal.
- **Postgres `last_seen` UPDATE fails after successful Redis + ClickHouse:** respond **`200`**. The heartbeat **is** persisted — ClickHouse is the source of truth; `last_seen` is a denormalized convenience cache on the `agents` row. Log `warn` (`last_seen update failed`). The agent does **not** retry (it got `200`); `last_seen` converges on the next successful heartbeat. This is the one fork where a downstream-store failure does not surface as a non-2xx.

## Failure modes

| Failure | HTTP | Body | Log | State cleanup |
|---|---|---|---|---|
| Malformed enroll body | `400` | `{error:"invalid_request"}` | warn | none |
| Token unknown / expired | `401` | `{error:"token_rejected"}` | warn | none |
| Token already consumed | `409` | `{error:"token_already_used"}` | warn | none |
| Cert issuance failure | `500` | `{error:"internal"}` | error | tx rollback |
| Malformed heartbeat body | `400` | `{error:"invalid_request"}` | warn | none |
| Unknown / CN-mismatched agent | `401` | `{error:"unknown_agent"}` | warn | none |
| Signature invalid | `401` | `{error:"bad_signature"}` | warn | none |
| Replayed nonce | `409` | `{error:"replay"}` | warn | none |
| Timestamp skew > ±5 min | `422` | `{error:"stale_timestamp"}` | warn | none |
| Bad/absent client cert (mTLS) | — | (connection refused at TLS) | warn | none |
| Postgres unavailable during enroll | `503` | `{error:"unavailable"}` | error | transaction rolls back atomically — token **not** consumed and agent **not** created; no half-state (FR-007) |
| Postgres unavailable during heartbeat agent lookup (step 2) | `503` | `{error:"unavailable"}` | error | none |
| Postgres `last_seen` UPDATE fails (after Redis + ClickHouse succeed) | `200` | (success) | warn | heartbeat persisted in ClickHouse (source of truth); `last_seen` converges next beat (§Cross-store) |
| ClickHouse unavailable (after Redis `SET NX`) | `503` | `{error:"unavailable"}` | error | nonce burned, heartbeat lost (accepted data loss); agent retries with a **new** nonce (§Cross-store) |
| Redis unavailable | `503` | `{error:"unavailable"}` | error | none; agent retries with the **same** nonce, accepted (§Cross-store) |

A `503` on a heartbeat is, to the SPEC-003 agent, just another non-2xx → it logs a warning and tries the next interval; no agent crash. The one exception is a post-persist `last_seen` failure, which returns `200` (the heartbeat is already durable in ClickHouse) — see §Behavior > Cross-store ordering.

## Observability

pino JSON logs. Mandatory per-event fields:

| Event | Level | Fields |
|---|---|---|
| Server listening | info | `enroll_port`, `heartbeat_port` |
| CA generated (first run) | info | `ca_subject`, `not_after` |
| Enroll succeeded | info | `agent_id`, `org_id`, `expires_at` |
| Enroll rejected | warn | `reason`, `response_status` |
| Heartbeat accepted | info | `agent_id`, `sequence_number`, `response_status` |
| Heartbeat rejected | warn | `agent_id?`, `reason`, `response_status` |
| Token issued (CLI) | info | `org_id`, `expires_at` (never the token value) |
| Backend unavailable | error | `backend`, `error` |

## Acceptance criteria

Each AC maps 1:1 to one integration test under `services/ingest/test/` (or `harness/` for the polyglot marquee test). Tests run the Fastify app against real Postgres, ClickHouse, and Redis via **testcontainers** (fallback to local `task dev:up` backends or in-process fakes only if CI cannot grant a Docker socket — decided in B4 and documented).

- **AC-001 — MARQUEE (end-to-end loop).** Spin up the ingest service (both listeners) backed by real Postgres + ClickHouse + Redis. Issue a token via the CLI. Run the **real compiled `cg-agent`** (built from the Rust workspace) pointed at the service: it enrolls, persists its identity, then sends one heartbeat over mTLS. Assert: (a) a row exists in Postgres `agents` whose `agent_id` equals the one the agent received and whose `pubkey` matches; (b) a row exists in ClickHouse `heartbeats` for that `agent_id` with `sequence_number = 1` and `status = 'online'`; (c) `agents.last_seen` is non-null. **This test must not mock the agent, the TLS, the signature, or the databases** — it is the proof that a real agent talks to a real server and lands real rows, and it transitively proves Rust↔Node JCS/Ed25519/TLS interop. If driving the Rust binary from a TypeScript test proves too convoluted, the marquee test lives at the repo top level under `harness/` as a polyglot integration test rather than under `services/ingest/test/`; it does **not** get downgraded to a mocked test.
- **AC-002 — replay rejection.** A heartbeat that is accepted once, replayed verbatim (same nonce), is rejected `409`; no second ClickHouse row appears.
- **AC-003 — timestamp skew rejection.** A heartbeat whose `sent_at` is 6 min off server time is rejected `422`; no ClickHouse row.
- **AC-004 — signature mismatch rejection.** A heartbeat whose body is altered after signing (or signed by a different key) is rejected `401`; no ClickHouse row.
- **AC-005 — unknown/mismatched cert rejection.** A heartbeat presenting a client cert whose CN is not a known `agent_id` (or whose envelope `agent_id` ≠ cert CN) is rejected `401`.
- **AC-006 — token reuse rejection.** Enrolling twice with the same token: the first succeeds `200`, the second is `409`; exactly one `agents` row results.
- **AC-007 — token expiry rejection.** Enrolling with a token whose `expires_at` is in the past is rejected `401`; no `agents` row.

## Security considerations

### CA private-key protection

The CA private key is the highest-value secret in SPEC-004: whoever holds it can mint client certs for arbitrary `agent_id`s. The proposal (see §Ratification) is to store it in the `ca` table encrypted with Postgres `pgcrypto` (`pgp_sym_encrypt`) under a passphrase supplied via `INGEST_CA_PASSPHRASE`, so a database dump alone does not yield the key. This is a real but bounded protection: an attacker with both the DB contents **and** the environment passphrase has the key, and an attacker with code execution on the server process can read the decrypted key in memory. It is materially better than plaintext-in-DB and materially weaker than an HSM/KMS — which is the named hardening path, out of SPEC-004 scope.

**Passphrase rotation is deferred work, named so it is not invisible.** SPEC-004 does not implement `INGEST_CA_PASSPHRASE` rotation. An operator who must rotate the passphrase has to re-encrypt the stored CA private key (decrypt under the old passphrase, re-encrypt under the new) via a manual procedure; specifying and tooling that procedure is a future ops SPEC concern, not SPEC-004. Until that SPEC lands, treat the passphrase as long-lived and protect it accordingly.

### Nonce-store collision bound

Nonces are 16 bytes (128 bits) from the agent's `OsRng`, stored 10 min. With the birthday bound, the probability of any collision among `n` nonces in the window is `≈ n² / 2¹²⁹`. Even at an implausible 10⁶ heartbeats in 10 min (≈1,667/s sustained), that is `≈ 10¹² / 2¹²⁹ ≈ 1.5 × 10⁻²⁷` — negligible. A nonce collision would at worst cause one legitimate heartbeat to be falsely rejected as a replay (then retried next interval), not a security breach.

### Enrollment-token single-use

The single-use guarantee is enforced server-side and atomically (FR-004): the `UPDATE … WHERE state='issued' … RETURNING` is the consume, so two concurrent requests cannot both win. This closes the race the SPEC-002 §Security "race-and-replay in transit" note flagged on the agent side — the server is the authority. The token's plaintext-on-the-wire exposure (enroll is plain HTTP) remains the SPEC-002-documented gap, accepted for closed-network MVP.

### mTLS-only heartbeat; plain-HTTP enroll is the attack surface

Heartbeats require mTLS, so an attacker without a CA-issued cert cannot submit one. Enrollment, by the SPEC-002 chicken-and-egg, stays plain HTTP — it is therefore the service's exposed attack surface, with the same threat profile as SPEC-002 §Security (an on-path observer can read a token in flight and race the agent). The mitigation is unchanged: short token TTL, single-use, closed network until a future SPEC adds enrollment-channel confidentiality.

## Ratification record

Four decisions are surfaced for Manuel's call. Each is presented with its trade-off so the decision can be made on the merits.

### 1. CA private-key storage

The CA private key must persist across restarts (the same CA must keep signing). The **proposal** is to store it in the Postgres `ca` table, encrypted with `pgcrypto` (`pgp_sym_encrypt`) under a passphrase from `INGEST_CA_PASSPHRASE`. The appeal is operational simplicity: the CA travels with the database (one backup, one restore, one place), and a plain DB dump does not leak the key. The cost is that the protection is only as strong as the environment passphrase — an attacker who captures both the DB and the env has the key — and the decrypted key necessarily lives in the service's memory while signing. The **alternative** is a key file on disk with OS-level permissions (owner-only), which moves the trust boundary to the filesystem and decouples the key from DB backups (a DB dump never contains it), at the cost of a second artifact to provision and protect, and a split backup story. The **deferred hardening path**, out of SPEC-004 scope but worth signalling now, is an HSM or cloud KMS that never exposes the private key to the process at all; that is the right enterprise answer and a future SPEC. Recommendation: pgcrypto-in-DB for SPEC-004 (simplest coherent story for a local-deploy MVP), explicitly flagged as interim.

### 2. Enrollment-token issuance UX

Tokens have to be minted somehow. The **proposal** is a CLI subcommand, `task ingest:issue-token --org <id>`, run by an operator with shell access to the server host; it writes the `enrollment_tokens` row and prints the token once. The appeal is that it needs no authentication model — shell access *is* the authorization — which keeps SPEC-004 free of an admin-auth subsystem it would otherwise have to build and secure. The cost is that there is no remote/programmatic issuance and no audit trail beyond the row itself, so it does not scale to a real operations team. The **alternative** is an authenticated admin HTTP endpoint (`POST /v1/admin/tokens`), which is the right long-term answer but drags in an admin auth model (API keys or RBAC) that is itself a future SPEC. Recommendation: CLI for SPEC-004, defer the admin HTTP API.

### 3. Server's own TLS identity

The mTLS heartbeat listener needs a server certificate that the agent's `trust_anchor_path` trusts (SPEC-003 FR-005). The **proposal** is that the server generates a self-signed TLS identity at first run (or reuses the CA to issue itself a server cert) and writes it to `INGEST_SERVER_CERT_PATH`/`INGEST_SERVER_KEY_PATH`; the operator copies the CA/server cert into the agent's trust anchor. The appeal is zero external dependency — `task dev:up` just works, and the local "wow moment" needs no manual cert plumbing. The cost is that a self-signed/self-issued server identity is not anchored in any external trust, so it is only as trustworthy as the host that generated it; in a real deployment an operator would want to supply a cert from their own PKI. The **alternative** is operator-provided server cert + key (the service only consumes them), which is the production-correct posture but makes the local first-run more involved. Recommendation: self-generate at first run for SPEC-004 (optimise the local reproducible loop), with the operator-provided path documented as the production option.

### 4. Admin user model

SPEC-004 could stand up a minimal user/RBAC model so token issuance and (future) admin actions are authenticated. The **recommendation is NOT to** — defer all of RBAC to a dedicated future SPEC. For SPEC-004, the only privileged action is token issuance, and it runs as a CLI under whoever has shell access to the server box. For a local-deploy MVP that is an accepted threat (shell access to the server is already total compromise); building a user model now would be premature and would expand the SPEC's security surface for no MVP benefit. The cost of deferring is that there is no per-operator attribution for token issuance until the RBAC SPEC lands. Recommendation: defer; no user model in SPEC-004.

## Amendment 2026-09-21: listener bind address

**What surfaced it.** Running the ingest container image for the first time, during the roadmap A' packaging work (Session 28), showed both listeners bound to a hard-coded `127.0.0.1` (`services/ingest/src/server.ts`, since `f407c05`). That suits the host-process topology the test harness and `pnpm dev` use (agent and server on the same loopback), but inside a container it binds only the container's own loopback, so neither the published ports nor the container network can reach the server. This SPEC never fixed a bind address; the loopback bind was an implementation choice.

**Amendment.** Both listeners bind `INGEST_BIND_HOST`, a new optional environment variable; its default, `127.0.0.1`, keeps the prior behavior. A containerised deployment needs a non-loopback bind such as `0.0.0.0`; exposure stays governed by the port mapping, not by the bind. The bind is independent of the server certificate: SAN verification is against the host the agent dials (SPEC-003 FR-005), not the bound interface. The self-issued cert (§Behavior > First run) covers only `localhost` / `127.0.0.1`, so an agent dialing any other host needs an operator-provided cert at `INGEST_SERVER_CERT_PATH` / `INGEST_SERVER_KEY_PATH`, which the server honours and never overwrites.

**Effect.** Additive and backward-compatible: with the variable unset the bind is unchanged, and the test harness sets loopback explicitly. FR-002 is unaffected: both listeners and their ports are intact; only the bind interface became configurable. §Configuration gains the `INGEST_BIND_HOST` row. Status stays **Accepted**.

## Amendment 2026-10-15: the server certificate's renewal

**What surfaced it.** Debt #88 (`docs/handoff-session-39.md:683`) and the threat model's R5 (`docs/security/threat-model.md:320`). The self-issued server certificate lasts 90 days (`services/ingest/src/cert.ts`), and §Behavior > First run has the server use an existing pair as-is at every start, never overwritten, so nothing looks at its expiry. Once it expires, every agent's next full handshake fails, since the agent checks the server certificate's validity period against its own clock (SPEC-003 FR-005), and the agent exits with code 6, a terminal failure (SPEC-003 §Failure modes; `agent/cg-agent/src/tls.rs:258`). Renewing the certificate afterwards does not bring those agents back. A connection the server closes before its handshake is, by contrast, a transient failure the agent retries (`agent/cg-agent/src/tls.rs:159-161`, `:248-254`). Manuel ordered this amendment on 2026-10-15: the certificate reissued at start before it expires.

**Amendment.**

1. **Two kinds of pair.** When both `INGEST_SERVER_CERT_PATH` and `INGEST_SERVER_KEY_PATH` exist, the server reads the pair and classifies it by its certificate. The certificate is *self-issued* when its signature verifies under the public key of the CA in the `ca` table and it carries the profile the server issues: subject `CN=localhost`, and a SAN of exactly `DNS:localhost` and `IP:127.0.0.1`. Any other certificate is *operator-provided*, including one whose signature cannot be checked under the CA's key (an issuer that does not sign with Ed25519, say), one the CA signed with another subject or SAN, and one a previous CA signed. The key file beside a self-issued certificate is the server's too. This narrows the Amendment 2026-09-21's "operator-provided", which meant whoever supplied the files. Both files absent: the server issues a pair, as before. Exactly one present, a file the server cannot read, or a certificate it cannot parse: a fatal start-up error, as before, with no reissue logged.
2. **A self-issued certificate is reissued before it expires.** The server issues a new pair with the profile of item 1, key usage `digitalSignature` and extended key usage `serverAuth` as at the first issuance, a new Ed25519 key, a `notAfter` 90 days ahead and a `notBefore` one hour in the past. The hour covers an agent whose clock is behind the server's by up to the ±5 minutes this SPEC's FR-010 step 3 tolerates, which would otherwise see a certificate not yet valid and exit 6. The first issuance uses the same validity. The server reissues when the current certificate has fewer than 30 days of validity left or has expired; and, at start, when the key file does not hold the certificate's private key (another key, or none it can parse). A mismatched pair was fatal before this amendment, since Node refuses the secure context; it is now repaired. The server checks at start, before the mTLS listener binds, and while running, every 24 hours, against the certificate the listener serves. The check stops when the server closes. A certificate whose `notBefore` is in the future is not covered.
3. **How a pair is written.** For each file the server removes a leftover `<path>.tmp`, writes the new content there and renames it over `<path>`, the key file first. The key file is created with mode `0600` (owner read and write); on Windows Node applies only the write bit, and the key file inherits its directory's ACL. For a self-issued pair, the directories of both paths must be writable by the server, as they already were for the first issuance; a reissue that cannot write fails by item 6. A failure between the two renames of a reissue, a crash or a rename that fails, leaves a new key beside the old certificate: a running server keeps the pair it holds, and its next check, finding that certificate still on disk, reissues; the next start repairs it by item 2, or fails if that reissue fails too (item 6). At the first issuance such a failure leaves a key without a certificate, fatal by item 1, as before. A deployment that placed a self-issued pair on a read-only mount never needed to write it; its reissues now fail by item 6, and it reaches item 7 unless the pair is replaced by an operator-provided one.
4. **While running.** When the check finds its own certificate due by item 2, it reads the pair on disk again before writing. If that certificate is still the one the listener serves, the server reissues. Otherwise the files changed under it:
   - a self-issued pair on disk whose key matches and which has 30 days or more left is another instance's renewal (item 9): the server adopts it, replacing the listener's secure context with it, and writes nothing;
   - a self-issued certificate on disk that cannot be adopted (its key does not match, or it is due itself) is the servers' own files: the server reissues;
   - anything else, an operator-provided certificate, a certificate the server cannot parse or a file it cannot read, is left as it is: the server writes nothing and the next start classifies the files. Each such check logs `server_cert_changed_on_disk` at `warn`, or `server_cert_expired` at `error` once the certificate it serves is in item 7's window. An operator's pair placed or renewed while the server runs takes effect at the next start.

   After a reissue, the listener's secure context is replaced with the new pair and the same options (the CA, TLS 1.3 only). A connection already established keeps the certificate it opened with; each full handshake after the swap gets the new one.
5. **The agents need nothing for a reissue.** The server sends the certificate alone, with no intermediate, so the agent's path validation (SPEC-003 FR-005) succeeds only when the certificate's issuer, the internal CA, is the configured anchor; SPEC-003 asks for the internal CA root there (`docs/specs/SPEC-003-mtls-signed-envelope.md:299`). The agent opens a connection per send. So a reissue under the same CA, valid from an hour before it (item 2), changes nothing an agent holds.
6. **A reissue that fails**, at start or while running, is logged at `error` and the server keeps its current pair; the next check retries. A leftover `<path>.tmp` that cannot be removed, such as a directory, is such a failure. At start, a failure with no pair to keep is fatal: none existed, or its key does not match its certificate, which cannot serve TLS.
7. **An expired certificate is not served.** While the certificate the server holds expires within ten minutes or has expired, by the clock the checks read, the mTLS listener closes each connection before its handshake. An agent then retries instead of exiting (*What surfaced it*); the ten minutes cover an agent whose clock is ahead of the server's by up to the ±5 minutes FR-010 step 3 tolerates. A self-issued certificate reaches that window only after 30 days of checks that could not replace it: its reissues failed (item 6), or its files were replaced with ones the server leaves alone (item 4) and it was not restarted. An operator-provided one reaches it when it is not renewed. Each of those checks logged at `error` or `warn`, and in the window they log at `error` (items 4, 6 and 8). Connections the listener closes are not logged one by one. Connections are served again once the server holds a certificate with more than ten minutes left: after a reissue, an adoption, or a restart on a renewed pair.
8. **An operator-provided pair is never overwritten**, as §Behavior > First run states. At start and at each check, the server logs it at `warn` (`server_cert_expiring`) while it has fewer than 30 days left, and at `error` (`server_cert_expired`) once it is in item 7's window: ten minutes or less left, or expired. The server still starts: the enrollment listener, the detection driver and SOAR do not depend on the mTLS listener's certificate, and stopping them would widen the outage. Its renewal is the operator's.
9. **Several instances** on the same paths each check on their own; the files are state on disk they share, as the first issuance's already were. The first to reissue writes the files, and each other instance adopts that pair at its own next check that finds its certificate due (item 4). If two reissue at once, their temporary files and renames can interleave: a failed write is retried at the next check, and a key and a certificate of different instances on disk are reissued by the next check or start that reads them.
10. **Constants, not settings.** The 30-day threshold, the 24-hour check, the 90-day validity, the hour's backdate and item 7's ten minutes are constants of the service; there is no new environment variable. A test injects the clock the checks and item 7 read, and the running check's interval, through `startIngest`'s options (`StartOptions.serverCert`). Issuance always reads the system clock, since the agents verify the certificate against theirs.

**Observability and failure modes.** §Observability gains six rows, logged by the boot logger (`startIngest`'s `StartOptions.log`, pino by default), which gains the `error` level and serves the running check as well as the start:

| Event | Level | Fields |
|---|---|---|
| Server certificate issued | info | `event: "server_cert_issued"`, `reason` (`absent`, `expiring`, `expired` or `key_mismatch`), `not_after`, `cert_path` |
| Server certificate adopted from disk | info | `event: "server_cert_adopted"`, `not_after`, `cert_path` |
| A pair that is not self-issued, fewer than 30 days left | warn | `event: "server_cert_expiring"`, `not_after`, `cert_path` |
| The certificate held, in item 7's window, that the server cannot replace: a pair that is not self-issued, or a self-issued one whose files changed (item 4) | error | `event: "server_cert_expired"`, `not_after`, `cert_path` |
| Certificate files changed while running | warn | `event: "server_cert_changed_on_disk"`, `cert_path` |
| Server certificate reissue failed | error | `event: "server_cert_reissue_failed"`, `error`, `cert_path`, and `not_after` of the certificate kept, absent when none is |

§Failure modes gains a row: *Server certificate expired, or expiring within ten minutes* — no HTTP; the connection is closed before the TLS handshake; logged at `error` by the checks (`server_cert_expired`, `server_cert_reissue_failed`), not per connection; no state cleanup; the agent retries (item 7).

**Acceptance criteria.** Each maps to a test in `services/ingest/test/servercert-ac-renewal.test.ts`. "Verifies" means a TLS client that presents a client certificate from the server's CA, trusts only that CA and checks validity with the system clock completes a TLS 1.3 handshake on the heartbeat listener.

- **servercert_ac_001 — expired, reissued at start.** An expired self-issued pair at the configured paths: the server starts; the files hold a new pair, with a new serial, signed by the CA, valid for 90 days from the start within a minute and from an hour before it within a minute, its key matching; the listener serves it and verifies; `server_cert_issued` is logged with reason `expired`.
- **servercert_ac_002 — expiring, reissued at start.** The same with 10 days left, reason `expiring`.
- **servercert_ac_003 — kept while valid.** A self-issued pair with 60 days left is kept byte for byte, the listener serves it, and no `server_cert_*` event is logged.
- **servercert_ac_004 — the operator's pair is never overwritten.** Two starts, each on a pair signed by another CA, one with 10 days left and one expired, and a third on a pair the server's CA signed for another host with 10 days left. The files are byte-identical after each start, `server_cert_expiring` (10 days) or `server_cert_expired` is logged, and `/health` answers `200`. The listener serves the 10-day pairs.
- **servercert_ac_005 — reissued while running.** A server started on a self-issued pair with 40 days left, its check's clock then moved 11 days ahead and its interval shortened: without a restart, a new handshake gets a new certificate, which verifies; a client limited to TLS 1.2 is refused after the swap; the files hold the new pair; it is reissued once.
- **servercert_ac_006 — a key that does not match.** A self-issued certificate beside a key file that holds another key, and beside one that holds no key: reissued at start with reason `key_mismatch`; the listener verifies.
- **servercert_ac_007 — a failed reissue.** While running, a reissue that cannot write its files logs `server_cert_reissue_failed`, and the listener keeps serving the current certificate. At start, the same failure on a self-issued pair with 10 days left logs it, and the server starts on that pair. At start, with no files or with a key that does not match, the same failure logs it without `not_after` and the start fails. A certificate file that cannot be parsed fails the start too, with no reissue logged.
- **servercert_ac_008 — the key file's mode** (not on Windows). A key file the server writes, at the first issuance and at a reissue, has mode `0600`.
- **servercert_ac_009 — files changed while running.** A server serving a self-issued pair with 40 days left, whose files an operator's pair then replaces, with its check's clock moved 11 days ahead: the files keep the operator's pair byte for byte, the listener keeps serving the self-issued certificate, and `server_cert_changed_on_disk` is logged at `warn`. With the clock then moved past that certificate's expiry, the check logs `server_cert_expired` at `error` and the listener closes a connection before its handshake.
- **servercert_ac_010 — another instance's files.** The same server, whose files a fresh self-issued pair then replaces, as another instance's reissue would: the listener serves that pair without a restart, which verifies, the files are unchanged, and `server_cert_adopted` is logged. When the files instead hold a self-issued certificate beside another key, the server reissues: the files hold a new pair, which the listener serves.
- **servercert_ac_011 — an expired certificate is not served.** On an expired operator-provided pair, on one with five minutes left, and on an expired self-issued pair whose reissue fails, the server starts and the heartbeat listener closes a connection before its handshake: the client's error is a closed connection, not a certificate error. Each logs at `error`: `server_cert_expired` for the operator-provided pairs, `server_cert_reissue_failed` for the self-issued one. After the expired operator-provided case, a pair with 60 days left placed at the paths and a restart serve again. A running server on an operator-provided pair with 60 days left, its check's clock moved 40 days ahead and then past its expiry, logs `server_cert_expiring` at `warn`, then `server_cert_expired` at `error`, and from then closes a connection before its handshake.

**Effect.** §Out of scope's *Certificate rotation / renewal / revocation enforcement* no longer covers the server's own certificate; the agents' certificates and revocation stay deferred (debt #88: the agents' renewal before roadmap §F's installation is called done, by ADR-0004 §Rotation and revocation). §Behavior > First run: "never overwritten" holds for an operator-provided pair; a self-issued pair is reissued by items 2 to 6, a mismatched one included, which was fatal; the other fatal start-up errors stay fatal; the first issuance's `notBefore` moves from a minute to an hour in the past. §Configuration: no new variable; for a self-issued pair the directories of the two paths must be writable (item 3). §Observability: the six rows above. §Failure modes: the row above. §Acceptance criteria: servercert_ac_001 to 011. FR-008 holds while the server holds a certificate with more than ten minutes left; otherwise item 7 closes a connection before any handshake. The Amendment 2026-09-21: its operator-provided pair, at another host's name, is kept and never overwritten, as it states; item 1 narrows the term to a certificate that is not self-issued. §Ratification record 3 records the choice made then and is unchanged. SPEC-003's Ratification 1 rejected retrying a server certificate that does not validate; item 7 does not reopen it, since the agent never accepts an invalid certificate: the server presents none, and the signal moves from the agents' exit to the server's `error` log. ADR-0004 is unchanged. The agent validates the server against the pinned internal CA (`docs/adr/0004-agent-server-protocol.md:39`). Its §Rotation and revocation governs the client certificates and the CA, not the server's own certificate, and its §Compliance's "rotation" is read as that section's; debt #88 and Manuel's order name this SPEC as the place for the server's renewal. Backward-compatible: an operator-provided pair is untouched, and a self-issued one changes only near its expiry or when its key does not match, under the same CA the agents trust. Debt #92 narrows to the containers' user, and to key files written before this amendment, which keep their mode until their next reissue. Status stays **Accepted**.

## References

- [ADR-0002](../adr/0002-language-per-component.md) — Language per component (TypeScript for API/BFF-style untrusted-input validation; §A3).
- [ADR-0003](../adr/0003-polyglot-storage.md) — Polyglot storage (Postgres relational, ClickHouse events, Redis nonces).
- [ADR-0004](../adr/0004-agent-server-protocol.md) — Agent-server protocol; §Enrollment and §Server validation order are implemented here.
- [ADR-0007](../adr/0007-ingest-language-typescript-mvp.md) — `services/ingest/` is TypeScript for the MVP control plane; the decision this SPEC is the first consumer of.
- [SPEC-002](SPEC-002-agent-enrollment.md) — Enrollment request/response contract and token semantics.
- [SPEC-003](SPEC-003-mtls-signed-envelope.md) — Outer signed envelope, canonical form (JCS), signature, nonce, timestamp.
- [RFC 8785](https://www.rfc-editor.org/rfc/rfc8785) — JSON Canonicalization Scheme (server canonicalizes with the `canonicalize` npm package; agent with `serde_jcs`).
- [Foundational Blueprint](../product/blueprint.md) — §6 (storage), §7 (agent-server protocol), §9 (pipeline, for the deferred firehose).
