# SPEC-025: Agent certificates — the enrollment token at rest, renewal, revocation and a copied identity

- **ID:** SPEC-025
- **Title:** Agent certificates — the enrollment token at rest, renewal, revocation and a copied identity
- **Status:** Proposed
- **Depends on:**
  - ADR-0004 Amendment 2026-11-20 — the decisions this SPEC implements: the token opaque and stored as its hash, checked before the certificate is signed (item 1); the raw public key, and the new key proved by both keys at renewal (item 2); no `SAN`, the org from the agent's row (item 3); renewal with 45 days left, and the certificate recorded at the renewal, which replaces the older one at its first use (item 4); revocation on the agent's row in PostgreSQL, write-once, refused with `403` (item 5); a refused agent keeps running (item 6); the ten-year CA (item 7); a copied identity detected, not refused (item 8).
  - ADR-0022 §4 and its Amendment 2026-11-20 — the runtime roles' grants; this SPEC narrows ingest's and leaves the api's to SPEC-026.
  - ADR-0004 (the protocol), ADR-0009 (the agent's ring), ADR-0010 and SPEC-024 (the service and its custody of the identity).
  - This SPEC amends **by scope**: SPEC-002 (the identity's `expires_at` informational; the renewal; the certificate's `SAN = org_id` (`:105`), which the certificate never carries, the org being read from the agent's row, ADR-0004 Amendment 2026-11-20 item 3; and the token's signed JWT (`:72`, `:88`, `:290`), which is an opaque value stored as its hash, item 1), SPEC-003 (the `403` reasons; the renewal request; its "Redis revocation set" (`:44`) read as the agent's row; FR-012 lists a certificate the server "has revoked" among the handshake refusals that end in exit `7`, and a revocation is instead the handler's `403`, which ends in none; and FR-012's exit `7` for a client certificate the handshake refuses does not apply while the agent keeps an older pair: a renewed certificate the server refuses with a certificate alert, or the newer of two pairs at a start so refused, falls back to the older pair with no exit, and any other failure under it is retried under it, §Operational §10; a renewal request closed or reset with no HTTP response is a failed attempt, tried again after an hour with no exit, §Operational §9; and AC-008 stands for an agent with one pair, whose heartbeat the handshake refuses), SPEC-004 (FR-002, the mTLS listener's renewal route; FR-004; FR-006, the certificate's `notAfter`; FR-007, FR-010, FR-013 and FR-015; NFR-002, whose "all state lives in Postgres, ClickHouse, and Redis" gains one exception, the rate-limit counters of §Operational §5, held in each process's memory, so that behind several instances each limit holds per instance; §Behavior, whose "no persistence occurs" on a validation failure gains the copy signal's write on a `403 agent_superseded` (§Operational §6), and whose cross-store fork that answers `200` when the Postgres update fails answers `503` instead for a request under the pending certificate, whose promotion is then not recorded (§Operational §3); and its Postgres schema), SPEC-023 (deploy_ac_004 gains rows for `cg_ingest` on `agents`) and SPEC-024 (the renewal's write of the identity, by an amendment of SPEC-024 that lands before the agent half's code; this SPEC's §Operational §10). Each one's acceptance criteria stand, read with the changes §Data contracts and §Operational state.
- **Authors:** Manuel (project owner), Claude Code (proposal and implementation)

## Context

An agent's certificate lasts 90 days, and nothing renews it: 90 days after its enrollment the server refuses its handshake, and the agent exits with code `7` (R5 of `docs/security/threat-model.md`; debt #88). Nothing revokes one either: a stolen key with its certificate is cut off only by deleting the agent's row, which the enrollment token's reference to it blocks. The enrollment token is stored as typed, its org and scope are not checked, the CA signs a certificate before the token is checked, and enrollment has no rate limit (R4; debt #89). The CA signs every certificate with `org_id` absent and the server writes the org `default` wherever it writes one (debt #119's reading of `services/ingest/src/routes/enroll.ts` and `services/ingest/src/routes/heartbeat.ts`). A copy of an enrolled endpoint's disk presents the same agent, which the server cannot tell from the original (R18; debt #170).

ADR-0004's Amendment 2026-11-20 decides how each is closed. This SPEC is roadmap §F's F4. Roadmap step 4 builds it in two halves, in this order: the server half, before F3's code, and the agent half, after it (`docs/product/roadmap.md`). Each criterion below is labelled by its half.

## Scope

### In scope

1. **The token at rest** (server): its SHA-256 alone, under an identifier; existing tokens hashed by the migration; uses and a withdrawal; the agent's row naming the token that enrolled it.
2. **Enrollment checks the token before it signs** (server): scope, org, state, expiry, withdrawal and remaining uses, under the token row's lock; the agent's org from the token; a rate limit on failed enrollments.
3. **Every agent request** (server): the revocation check, the certificate the request presents compared with the agent's and its pending one, the pending one made the agent's, any other refused; the org from the agent's row; the agent's host name, platform and version kept.
4. **The renewal route** (server) on the mTLS listener, and its rate limit.
5. **Revocation's storage** (server): the columns, the write-once trigger, ingest's runtime role unable to write them. The route that revokes is SPEC-026's (F3); until it lands, nothing but the tests and the owner role writes a revocation.
6. **A copied identity** (server): two signals, a flag on the agent's row, no alert and no automatic revocation.
7. **The agent's renewal** (agent): the decision from its certificate, the request, the checked answer, the swap of key, certificate and signing key, and its refusals.

### Out of scope

- **The api's routes and grants**: issuing, listing and withdrawing tokens, a multi-use token among them, the agents' pages and their flag, the revocation's route, the audit rows: SPEC-026 (F3), as roadmap step 4 orders. This SPEC builds the server's side of each, so SPEC-026 adds to ingest no route and no check of enrollment, renewal or revocation: its ingest changes are its migration `0011_dashboard_rights` (the api's grants, the triage history, ingest's own `UPDATE` on `incidents` narrowed and the open incident's unique key) and grouping's handling of a closed incident and of the techniques.
- **The runbooks**: revoking an agent and undoing a revocation, the CA's compromise, a restore's effect on renewed agents, a copy's remedy, the token's wording in `docs/operations/install.md`, and the personal data of `docs/operations/data-protection.md`. They go through a branch and a pull request of their own, since `deploy-ci` reads `docs/operations/`, before the release.
- **Separating the CA from ingest** (debt #99), which reopens ADR-0004's item 7.
- **Reusing the agent's connection** (debt #42): roadmap step 6, after renewal (Manuel's choice of 2026-11-20).
- **The evidence-integrity and database-logging proposals** (debts #124 and #125): F4's pre-release proposals, apart from this SPEC, their destination unchanged.
- **Comparing the source addresses of an agent's requests, or reading the local addresses of its connections (class 4001), to find a copy**: both are personal data, and neither is used.

## Data contracts

### 1. Ingest's migration `0010_agent_certificates`

One migration, run as `cg_ingest_owner`, in the transaction Kysely runs every pending migration in (`services/ingest/src/db/migrate.ts`). It references no new enum value, which that transaction could not use.

`enrollment_tokens`:

| Column | Type | Meaning |
|---|---|---|
| `token_id` | `uuid` `PRIMARY KEY` | The token's identifier, which the pages and the agent's row name. Filled for each existing row by `gen_random_uuid()`. |
| `token_sha256` | `bytea` `NOT NULL` `UNIQUE`, 32 bytes | SHA-256 of the token's text, its base64url characters as ASCII. Filled for each existing row from its `token`, by `public.digest`. |
| `org_id`, `scope`, `state`, `issued_at`, `expires_at`, `consumed_at` | as today | `issued_at` is the database's `now()` at the insert, whatever the insert gives (the trigger below). `state` is `consumed` once `uses = max_uses`. `consumed_at` is the time of the last use. |
| `max_uses` | `integer` `NOT NULL` `DEFAULT 1`, `CHECK (max_uses BETWEEN 1 AND 1000)` | The token's cap. |
| `uses` | `integer` `NOT NULL` `DEFAULT 0`, `CHECK (uses BETWEEN 0 AND max_uses)` | Enrollments it made. Filled with 1 for a row in state `consumed`. |
| `withdrawn_at` | `timestamptz` | Set once, by the withdrawal, at the database's `now()`; a withdrawn token enrolls nothing. |

The columns `token` and `consumed_by_agent_id` are dropped, after the latter is copied to `agents.enrolled_by_token_id`. A `CHECK` holds `expires_at <= issued_at + interval '7 days'`, and, when `max_uses = 1`, `expires_at <= issued_at + interval '15 minutes'`; before adding them the migration caps each existing row's `expires_at` at its bound, which shortens by the skew between a client's clock and the database's a token issued with the expiry the client computed.

`agents`:

| Column | Type | Meaning |
|---|---|---|
| `enrolled_by_token_id` | `uuid` `REFERENCES enrollment_tokens (token_id)` | The token that enrolled the agent; `NULL` for an agent whose token row was deleted before this migration. |
| `hostname`, `platform`, `agent_version` | `text` | What the agent last sent, at enrollment and in each accepted heartbeat's agent block, each cut to 255 code points. `NULL` for an agent enrolled before this migration until its next heartbeat. |
| `revoked_at` | `timestamptz` | The revocation's time. |
| `revocation_reason` | `text`, `CHECK (char_length(revocation_reason) BETWEEN 1 AND 500)` | Required with `revoked_at` (`CHECK ((revoked_at IS NULL) = (revocation_reason IS NULL))`). |
| `process_started_at`, `previous_process_started_at` | `timestamptz` | The start of the agent's process, as its last two distinct heartbeats computed it (§Operational §6). |
| `duplicate_signal` | `text`, `CHECK (duplicate_signal IN ('superseded_certificate', 'interleaved_process'))` | The first signal of a copy (§Operational §6). |
| `duplicate_signal_at` | `timestamptz` | Its time; both set once. |
| `pending_cert_pem` | `text` | The certificate the agent's last renewal issued (§Operational §4), until a request under it makes it the agent's (§Operational §3); `NULL` otherwise. |

**The triggers**, each with a function `cg_ingest_owner` owns; those that refuse do so with `SQLSTATE` `P0001`:

- `agents_write_once`, `BEFORE UPDATE` on `agents`: an update that changes `revoked_at` or `revocation_reason` of a row whose `revoked_at` is set (`revocation is write-once`), or `duplicate_signal` or `duplicate_signal_at` once set. An update that sets `revoked_at` on a row without one has it replaced by the transaction's `now()`, whatever value it gave, so a revocation is never dated in the future or the past.
- `enrollment_tokens_issue`, `BEFORE INSERT` on `enrollment_tokens`, which refuses nothing: it sets `issued_at` to `now()`, `uses` to 0, `state` to `issued`, and `consumed_at` and `withdrawn_at` to `NULL`, whatever the insert gives, so that the `CHECK`s on `expires_at` bound a token's life from the time it is really issued, against either running service.
- `enrollment_tokens_write_once`, `BEFORE UPDATE` on `enrollment_tokens`: an update that changes `withdrawn_at` once set (`withdrawal is write-once`); one that changes `token_sha256`, `org_id`, `scope`, `issued_at`, `expires_at` or `max_uses` (`token terms are fixed`); and one that lowers `uses` or sets `state` back to `issued` (`token uses only grow`). An update that sets `withdrawn_at` has it replaced by `now()`.

The owner role, which holds the triggers, can disable them; neither runtime role can.

**The grants**, when `cg_ingest` exists: its table-level `INSERT` and `UPDATE` on `agents` are revoked and replaced by `INSERT (agent_id, org_id, pubkey, cert_pem, enrolled_at, expires_at, enrolled_by_token_id, hostname, platform, agent_version)` and `UPDATE (pubkey, cert_pem, expires_at, last_seen, hostname, platform, agent_version, process_started_at, previous_process_started_at, duplicate_signal, duplicate_signal_at, pending_cert_pem)`; on `enrollment_tokens` it keeps `SELECT`, `INSERT` and `UPDATE`. It has no right to write `revoked_at` or `revocation_reason`. The api's grants are SPEC-026's.

### 2. The renewal request and its answer

`POST /v1/agents/renew` on the mTLS listener. The agent sends it to its heartbeat target (`server.heartbeat_url`, or `server.url` without it; `heartbeat_target()`, `agent/cg-agent/src/config.rs`), of whose URL the agent uses the host and port alone (`SecureSender::new`, `agent/cg-agent/src/tls.rs`), and sends the request with the path `/v1/agents/renew` where the heartbeat sends `HEARTBEAT_PATH` (`agent/cg-agent/src/delivery.rs`); a path in that URL is ignored, as it is for the heartbeat. No configuration key is added.

The request is SPEC-003's outer envelope with a body of its own, and one more member:

```json
{
  "outer_envelope_version": "0.1.0",
  "agent_id": "…",
  "sequence_number": 1,
  "nonce": "base64url, 16 bytes",
  "sent_at": "RFC 3339 UTC",
  "body": { "renew_version": "1", "new_agent_pubkey": "base64url, 32 bytes" },
  "signature": "base64url Ed25519, by the current key",
  "new_key_signature": "base64url Ed25519, by the new key"
}
```

Both signatures sign the same bytes: the JCS canonical form of the envelope without `signature` and without `new_key_signature` (SPEC-003's canonicalisation). `sequence_number` is the agent's next one, informational as in SPEC-003.

The answer to a renewal is SPEC-002's enrollment answer: `{ envelope_version, agent_id, client_certificate, issued_at, expires_at }`, with `200`.

### 3. The refusals this SPEC adds

| Status | `error` | Where | When |
|---|---|---|---|
| `403` | `agent_revoked` | heartbeat, renewal | The agent's row has `revoked_at`. |
| `403` | `agent_superseded` | heartbeat, renewal | The request presents a certificate that is neither the agent's nor its pending one (§Operational §3). |
| `401` | `bad_new_key_signature` | renewal | `new_key_signature` does not verify under `new_agent_pubkey`. |
| `409` | `renewal_not_due` | renewal | The certificate presented has more than 45 days left, or a new one would end no later than it (§Operational §4). |
| `429` | `rate_limited` | enrollment, renewal | §Operational §5. |
| `401` | `token_rejected` | enrollment | As today, and also for a withdrawn token, one of another scope, or one whose org is not `default`. |
| `409` | `token_already_used` | enrollment | As today: the token exists and has no use left. |

A refusal's body names its `error` alone. Its log line carries the `agent_id` when one is known, the reason and the status, and never the token, a key or a signature.

## Operational

### 1. Enrollment (server)

In one transaction: the server computes the token's SHA-256, selects its row `FOR UPDATE`, and checks, refusing on the first failure, each refusal a failed enrollment for §5: the row exists, `scope = 'enroll'` and `org_id = 'default'` (the MVP's one org), else `401 token_rejected`; `uses < max_uses`, else `409 token_already_used`, as a consumed token answers today whether or not it has expired; `withdrawn_at IS NULL` and `expires_at > now()`, else `401 token_rejected`. Then it assigns the `agent_id`, has the CA sign the certificate, inserts the agent with the token's org, its `enrolled_by_token_id`, and the request's host name, platform and version, and increments `uses`, setting `consumed_at`, and `state = 'consumed'` when no use is left. A refused request signs nothing. The lock serialises enrollments with one token, so two requests cannot spend its last use twice.

The certificate's `notAfter` is the earlier of 90 days after its `notBefore` and the CA's `notAfter`.

`issue-token` (`services/ingest/src/cli/issue-token.ts`) inserts a single-use token whose `expires_at` the database computes as `now() + interval '15 minutes'`, with its SHA-256 and identifier, prints the token once to standard output and its identifier and expiry to standard error, and refuses any `--org` but `default` with exit `2`. It writes no audit row: `audit_log` is the api's, and ingest's runtime role holds no right on it (`services/api/src/db/migrations/0007_runtime_grants.ts`); the audit rows of a token's issue are those of the api's routes (SPEC-026). A token's expiry is computed by the database wherever one is issued, so that a client's clock never meets the `CHECK`s.

### 2. Every request of an agent (server)

On the heartbeat and the renewal, after the envelope's structure (`400`) and the client certificate's `CN` equal to `agent_id` (`401 unknown_agent`), the server reads the agent's row once: `pubkey`, `cert_pem`, `pending_cert_pem`, `org_id`, `revoked_at`, and the copy columns. In this order, before the nonce is spent:

1. No row → `401 unknown_agent`, as today.
2. `revoked_at` set → `403 agent_revoked`.
3. The certificate check of §3: `403 agent_superseded`, or the key the envelope is then verified under.

The timestamp (`422`), the nonce (`409`) and the signature (`401`) follow, as SPEC-004 FR-010 orders them. Every row the request writes that has an org, its heartbeat's events in `cges_events` among them, carries the row's `org_id`, never a constant.

An accepted heartbeat updates `last_seen`, `hostname`, `platform` and `agent_version` from its agent block, and the process columns of §6, in one statement.

### 3. The certificate a request presents (server)

The server compares the DER of the certificate the TLS socket presents with the DER of `cert_pem` and of `pending_cert_pem`:

- **Equal to `cert_pem`**: the envelope is verified under `pubkey`.
- **Equal to `pending_cert_pem`**: the envelope is verified under that certificate's key. If it verifies, and the request passes every later check and its writes succeed (a heartbeat whose events cannot be stored is refused, as `services/ingest/src/routes/heartbeat.ts` refuses it today, and promotes nothing), the server sets `pubkey`, `cert_pem` and `expires_at` to the pending certificate's and clears `pending_cert_pem`, with a compare-and-set on the old `cert_pem` and the pending one; a compare-and-set that finds another certificate there answers `503` and leaves the agent's row as it is, and the agent's next request is compared afresh; so does a request whose promotion the database does not record for any other reason, a failed write among them, and never `200`, so that a `2xx` under the pending certificate always means it is the agent's (§10). It logs `agent certificate promoted` with the agent and both certificates' serials. This is the renewal's completion (ADR-0004 Amendment 2026-11-20, item 4). A renewal request under the pending certificate that §4 accepts promotes it too; one §4 refuses promotes nothing.
- **Neither**: `403 agent_superseded`, and the copy signal `superseded_certificate` (§6). A certificate the CA signed that the server did not record at a renewal, an older one of the agent's among them, is never made the agent's.

The handshake has already proved that the presented certificate chains to the CA, is within its validity and is held by its key's owner (SPEC-004 FR-008).

### 4. Renewal (server)

After §2 and §3, the timestamp, the nonce and the current key's signature: `new_key_signature` under `new_agent_pubkey` (`401 bad_new_key_signature`); the presented certificate's `notAfter` 45 days or fewer after the server's `now()`, and the new certificate's `notAfter` (§1) later than it, which the CA's own end can prevent (`409 renewal_not_due`); the rate limit of §5 (`429`). Then the CA signs a certificate with `CN = agent_id`, the new key, and the `notAfter` of §1; the server stores it as the agent's `pending_cert_pem`, replacing any earlier one, and answers it; if it cannot store it, it answers `503` and sends no certificate. The agent's certificate changes at its first accepted request under the new one (§3), so an answer the agent does not receive changes nothing the agent uses, and the agent renews again, which replaces the pending certificate.

### 5. Rate limits (server)

In the process's memory, constants, no setting, reset at a restart. ADR-0003's table puts rate limits in Redis (`docs/adr/0003-polyglot-storage.md:31`), and its §Compliance asks a SPEC that moves data from the backend the table names to say so and why: ingest's Valkey user is limited to its anti-replay nonces (ADR-0022 §4), and a counter lost at a restart weakens a limit for 15 minutes at most and corrupts no state; behind several instances each limit holds per instance (§Risks). The limits:

- **Enrollment**: a failed enrollment is one §1 refuses for its token (`401` or `409`); a malformed request (`400`) does not count. Once ten enrollments from one source address have failed within 15 minutes, every enrollment from that address is refused with `429 rate_limited`, without its token being read, until 15 minutes after the tenth failure. A successful enrollment counts for nothing.
- **Renewal**: one certificate per agent per 10 minutes; a renewal that would sign a second is `429 rate_limited`.

### 6. A copied identity (server)

Two signals, detect-only:

- **`superseded_certificate`**: a request presents a certificate that is neither the agent's nor its pending one (§3).
- **`interleaved_process`**: each accepted heartbeat computes its process's start as `body.sent_at − body.uptime_seconds`. If `process_started_at` is `NULL`, it takes that start. If it is within 120 seconds of `process_started_at`, nothing changes. Otherwise, if it is within 120 seconds of `previous_process_started_at`, two processes are taking turns as the agent: the signal is raised. In both of the last cases the columns shift: `previous_process_started_at` takes `process_started_at`, which takes the new start.

The first signal sets `duplicate_signal` and `duplicate_signal_at`, and logs `agent duplicate suspected` with the agent and the signal; a later one changes neither. No alert is written, no request is refused for it, and nothing revokes the agent: the operator reads the flag on the agent's page (SPEC-026) and decides. A restart, a sleep or a single step of the host's clock moves the start once, so it raises nothing (a step and its correction can, §Risks); a virtual machine reverted to a snapshot can resume the process of the snapshot, and raise `interleaved_process`.

### 7. A restore (server)

A database restored from a backup taken before an agent renewed holds the older certificate and no pending one: the agent's next request, under its newer certificate, is refused with `403 agent_superseded` and raises the copy signal, until the operator revokes the agent and enrolls the endpoint anew, as the restore runbook's change that comes with this SPEC states (§Out of scope). A revocation made after the backup is not in the restored database, and must be made again. A copy of the endpoint holding the older certificate is accepted until the agent is revoked.

### 8. The renewal decision (agent)

The agent reads its certificate's `notAfter` from the certificate it presents, not from `identity.json`, whose `expires_at` becomes informational (SPEC-002, by scope). At its start, and before each heartbeat, with no other request of its own in flight, so that no request under the old certificate reaches the server after one under the new, it asks for a new certificate when `notAfter − now` is 45 days or less and its last attempt was more than an hour ago or there was none. The time is injected into the decision, so a test can move it. A certificate already past its `notAfter` cannot renew: the handshake refuses it, and the agent exits `7`, as today.

### 9. The renewal request (agent)

The agent generates a new Ed25519 key pair in memory, builds the request of §Data contracts §2, signs it with its current key and with the new one, and sends it through the same TLS client configuration its heartbeats use, with its current certificate (`build_client_config`, `agent/cg-agent/src/tls.rs`), not the enrollment client's. A failed attempt keeps the current pair, discards the new key, and is tried again after an hour; it never exits the process, whatever its status, the handshake's refusals of SPEC-003 FR-012 aside. A connection closed or reset with no HTTP response after the request is sent, which may follow the server's storing of the pending certificate (§4), is such a failed attempt, not one of those refusals: a certificate the server refuses that way is refused again at the next heartbeat, which exits `7` as today.

### 10. The swap (agent)

Before it keeps the answer, the agent checks that the certificate's `CN` is its `agent_id`, that its key is the new public key, that its issuer's name is its current certificate's, and that its `notAfter` is later than its current one; otherwise it discards both and logs why. It does not check the chain: the CA that signs agents' certificates is ingest's, while its trust anchor may be the CA of an operator-provided server certificate (SPEC-023 §Operational §6), and the server's handshake checks the chain. Then it keeps both pairs, with the custody SPEC-024 gives the identity in each layout, and presents the new one, through a client configuration of its own that resumes no TLS session made under the old pair, so that the server is shown the new certificate; once a request under the new pair is answered with a `2xx`, which by §3 means the server has made it the agent's, it removes the old one; if the server answers the new certificate with a TLS alert that refuses a certificate, it removes the new pair and goes on with the old, logging why, and does not exit; any other failure of a request under the new pair, a connection closed or reset with no HTTP response among them, which may follow a promotion the server has made (§3), leaves both pairs and is tried again under the new one, as a `503` is, since a request under the old pair after a promotion is refused as superseded and flagged as a copy. A start that finds both pairs presents the newer, and falls back to the older only on such an alert. At no point does it hold a key and a certificate that do not match as its only pair, nor remove the old pair before the server has accepted a request under the new one (a later restore of the server's database is §7's case). SPEC-024's amendment for these writes lands before the agent half's code.

### 11. A refusal on the agent

`403` with `agent_revoked` or `agent_superseded` is, as every refusal of the envelope, a `warn` line, retries, a dropped batch logged at `error`, and no exit (ADR-0004 Amendment 2026-11-20, item 6; SPEC-003 FR-012). The agent adds the reason the body names to its line. A revoked agent's renewal is refused too, so it stops at its certificate's `notAfter`.

### 12. No new deployment configuration

No environment variable, port, `agent.toml` key or setting is added. The window, the lifetimes, the caps and the rate limits are constants of the code.

## Security considerations

- **The token at rest.** A read of `enrollment_tokens` yields hashes of 256-bit values, which enroll nothing.
- **Nothing is signed for a refused token.** The check and the lock precede the CA's signature.
- **Proof of both keys.** A renewal needs the current key, through the handshake and the envelope, and the new key, through its signature: a thief of the certificate alone renews nothing, and the private key never leaves the endpoint. A holder of the CA's key alone takes over no enrolled agent: a certificate it signs is not the server's record of a renewal, so it passes the handshake and is refused as superseded (§Operational §3).
- **Revocation cannot be undone by a running service.** The owner role's trigger refuses it, and `cg_ingest` cannot write the columns.
- **A copy is detected, not prevented.** Both copies hold the key. The signals flag the agent; the operator revokes it, which refuses every endpoint that holds it, the original included, and purges and enrolls anew each of them.
- **The ten-year CA** is accepted with ADR-0004's amendment (item 7); an agent's certificate never outlives it.

## Acceptance criteria

Where each criterion's test runs: *Test scenarios*.

- **renew_ac_001 (the token at rest) [server].** After the migration over rows inserted with plain tokens, `information_schema.columns` lists no `token` column; each row's `token_sha256` is the SHA-256 of its former token; an issued row's `uses` is 0 and a consumed one's 1, a single-use row whose `expires_at` was 15 minutes and 2 seconds after its `issued_at` now ends 15 minutes after it, and the agent it enrolled names it in `enrolled_by_token_id`. `issue-token` prints a token whose SHA-256 is the only form of it in the database, and refuses `--org other` with exit `2`.
- **renew_ac_002 (checked before signing) [server].** Enrollments with an unknown token, an expired one, a withdrawn one, one of scope `other`, and one of org `other` answer `401 token_rejected`, and one with no use left `409 token_already_used`, expired or not; in each the CA signs nothing (a spy on the signing function counts 0) and no agent row is written. Two concurrent enrollments with one single-use token: one `200`, one `409`.
- **renew_ac_003 (several uses) [server].** A token of `max_uses` 3 enrolls three agents, each naming it; the fourth answers `409`; after `withdrawn_at` is set on another such token, its next enrollment answers `401`. The migration's checks refuse a single-use token of more than 15 minutes and any token of more than 7 days.
- **renew_ac_004 (the org) [server].** An agent enrolled with a token of org `default` has that org; its heartbeat's `cges_events` rows carry its row's org, read from the row (a test that sets another org on the row sees that org on the rows).
- **renew_ac_005 (the write-once columns) [server].** Setting `revoked_at` with a reason succeeds once, and stores the transaction's time, not a time given a day ahead; a second change of either, clearing them, and `revoked_at` without a reason fail; `duplicate_signal` once set cannot change; a token's `withdrawn_at` once set cannot change or be cleared, and its `expires_at` and `max_uses` cannot change.
- **renew_ac_006 (a revoked agent) [server].** A heartbeat and a renewal of a revoked agent answer `403 agent_revoked`, write no heartbeat row and no event, and spend no nonce (Redis holds no `nonce:` key for the request).
- **renew_ac_007 (the pending certificate, another one) [server].** An agent enrolled, renewed, and then heartbeating under the new certificate: the row holds the new key and certificate and no pending one, and the log has `agent certificate promoted`; a heartbeat under the old one answers `403 agent_superseded` and sets `duplicate_signal = 'superseded_certificate'`. A certificate for the agent with a later `notBefore` and a key of the test's, signed with the CA's key but by no renewal, answers `403 agent_superseded` and changes neither `pubkey` nor `cert_pem`. A pending certificate whose key does not verify the envelope answers `401` and changes nothing. A database restored to the row before the renewal: the next heartbeat under the new certificate answers `403 agent_superseded`.
- **renew_ac_008 (renewal) [server].** With 45 days left on the presented certificate, the answer is `200` with a certificate of `CN = agent_id`, the new key, a chain to the CA and a `notAfter` 90 days after its `notBefore`, the row's `pending_cert_pem` is that certificate, and its `pubkey` and `cert_pem` are unchanged; with the pending certificate presented and due, the row is promoted and the answer is `200`; a heartbeat under the pending certificate whose write the test makes fail answers `503` and leaves `cert_pem` and `pending_cert_pem` as they were; with 46 days, `409 renewal_not_due`; a bad `new_key_signature`, `401 bad_new_key_signature`; a replayed nonce, `409`; a stale `sent_at`, `422`; a second issuance within 10 minutes, `429`.
- **renew_ac_009 (the CA's end) [server].** With a CA whose `notAfter` is 60 days away, an enrollment's certificate ends at the CA's `notAfter`; a renewal presenting a certificate with 40 days left, which ends before the CA does (one the test signs with the CA's key and writes to the agent's row as its certificate), answers `200` with a certificate that ends at the CA's `notAfter`; and a renewal under a certificate that already ends there answers `409 renewal_not_due`. An insert into `enrollment_tokens` with an `issued_at` a year ahead stores the database's time, and its `expires_at` past 7 days from it is refused.
- **renew_ac_010 (the enrollment limit) [server].** Ten failed enrollments from one address, then an eleventh with a valid token: `429`, and the token keeps its use; another address enrolls.
- **renew_ac_011 (what the agent sends) [server].** An enrollment stores the host name, platform and version it carries; a heartbeat with another host name updates it; a host name of 300 characters is stored cut to 255.
- **renew_ac_012 (an interleaved process) [server].** Heartbeats whose starts are A, B, A raise `interleaved_process` once; A, B, B and A, A + 60 s do not.
- **renew_ac_013 (the grants) [server; the api's deploy_ac_004 test].** As `cg_ingest`: `UPDATE agents SET last_seen` allowed, `UPDATE agents SET revoked_at` refused, and an `INSERT` into `agents` that names `revoked_at`, `revocation_reason` or `duplicate_signal` refused.
- **renew_ac_101 (the decision) [agent].** With the time injected, a certificate with 46 days left asks for nothing; with 45, it asks once, and again only after an hour; the decision reads the certificate's `notAfter`, not `identity.json`'s.
- **renew_ac_102 (the request) [agent].** The request goes to `/v1/agents/renew` on the heartbeat target, through the heartbeat's client configuration; its `signature` verifies under the current key and its `new_key_signature` under `new_agent_pubkey`, over the same canonical bytes.
- **renew_ac_103 (the swap) [agent].** After a `200`, the next heartbeat presents the new certificate, on a connection that resumes no session of the old pair, and is signed by the new key, the old pair is kept until a request under the new one is answered `2xx`, and is then gone; a `503` under the new pair leaves both pairs; an answer whose certificate names another `CN`, another key, another issuer or an earlier `notAfter` is discarded, and the next heartbeat uses the old pair; a new certificate the server refuses with a certificate alert leaves the agent on the old pair, running; a connection closed without an HTTP response after the server has promoted the new certificate leaves both pairs, and the next request under the new pair is accepted, with no `agent_superseded` and no copy signal.
- **renew_ac_104 (an interrupted swap) [agent].** The swap of §Operational §10 interrupted after each of its steps: the next start presents a matching pair the server accepts, the newer when it holds both.
- **renew_ac_105 (refusals) [agent].** A `403 agent_revoked` and a `403 agent_superseded` each give a `warn` line naming the reason, and the process keeps running; a `409` or `429` to a renewal is tried again after an hour; a renewal whose connection the server closes with no HTTP response after storing the pending certificate leaves the agent running on its old pair, and its next renewal, an hour later, replaces the pending certificate.

The server half is done when renew_ac_001 to 013 pass, with the elevated gate (it changes `services/ingest/src/routes/heartbeat.ts`); the agent half when renew_ac_101 to 105 pass, with the elevated gate (it changes the secure path of `agent/cg-agent/src/lib.rs` and `agent/cg-agent/src/delivery.rs`), and after F3's code.

## Test scenarios

- **SC-F4-001, a token for a rollout.** A token of three uses, as SPEC-026's route issues one (the host's command issues single-use tokens alone), enrolls three endpoints, each naming it, and a fourth is refused. Realised by renew_ac_001 and renew_ac_003.
- **SC-F4-002, a routine renewal.** An agent with 45 days left asks for a certificate, heartbeats under it, and the server makes it the agent's; the agent then removes its old pair. Realised by renew_ac_007, renew_ac_008 and renew_ac_101 to 103.
- **SC-F4-003, a lost answer.** The connection closes after the server has promoted the new certificate; the agent retries under the new pair and is accepted. Realised by renew_ac_103.
- **SC-F4-004, a revoked agent.** The agent is revoked with a reason; its next heartbeat and renewal are refused, and the agent logs the reason and keeps running. Realised by renew_ac_005, renew_ac_006 and renew_ac_105.
- **SC-F4-005, a copy and a restore.** A copy heartbeats under the certificate the original replaced, and the agent is flagged; a database restored to before a renewal refuses the agent. Realised by renew_ac_007 and renew_ac_012.
- The server half's tests are vitest tests of `services/ingest` against testcontainers, but renew_ac_013, which is the api's deploy_ac_004 test; the agent half's are Rust tests of `cg-agent`, against its test servers (`agent/cg-agent/tests/common/`) where one is needed, with the time injected where a criterion moves it.

## Risks

- **A copy of the endpoint keeps working until it is noticed.** Both hold the key (R18). The signals flag it; nothing refuses it beyond the refusal of a superseded certificate (§Operational §3), which, once either holder renews, refuses the other.
- **A virtual machine reverted to a snapshot can raise `interleaved_process`**, which the operator reads as a copy (§Operational §6).
- **A restore of the server's database** refuses the agents that renewed after its backup, and flags them as copies, until each is revoked and its endpoint enrolled anew (§Operational §7).
- **The rate limits are per process.** A restart resets them, and a deployment of one ingest has one. The enrollment limit counts by source address, so ten failures behind one address translation refuse every enrollment behind it for 15 minutes; and an agent refused with `429` at its enrollment exits `3`, as SPEC-002 makes any other `4xx` of enrollment, its service stopped and not restarted, until the operator runs the setup again after those 15 minutes, with the same token while it is valid.
- **An endpoint restored from its own backup or snapshot taken before a renewal** presents the older certificate: it is refused as superseded, cannot renew, and is flagged as a copy, until the operator revokes the agent and enrolls the endpoint anew.
- **A step of a virtual machine's clock and its correction**, more than 120 seconds each, give the starts A, B, A, and raise `interleaved_process`.
- **The copy flag cannot be cleared** by either running service or the api: a false one stays until the agent is revoked and its endpoint enrolled anew, or the owner role clears it.
- **The enrollment limit counts by the address ingest sees.** Where Docker hides clients' addresses behind one (`docs/operations/configuration.md`), ten failures from anyone refuse every enrollment for 15 minutes, and an endpoint refused then stays stopped until its setup is run again.
- **A server that refuses a new certificate by closing the connection without a TLS alert** leaves the agent trying it, with no fallback to the old pair, logging each failure, until the operator acts: telling that close from a lost answer after a promotion is not possible, and a fallback in the second case would strand the agent (§Operational §10).
- **A token for several uses** is readable, wherever it is distributed, until it expires, is used up or is withdrawn (SPEC-026 issues it).

## Open questions

None. ADR-0004's amendment answers the protocol's; the runbooks are listed under *Out of scope*.

## Decision record

Proposed by Claude Code on 2026-11-20, on Manuel's choices of that day (ADR-0004 Amendment 2026-11-20). Beyond that amendment, each decision with what it discards:

1. **Renewal records the certificate it signs as the agent's pending one; the first request under it makes it the agent's.** Against making it the agent's when it is signed: a lost answer would strand the agent with a certificate the server refuses. Against making any later certificate the CA signed the agent's, which would recover a restore by itself: the CA's key alone would take over an enrolled agent (ADR-0004 §Context, point 1). Manuel's choice of 2026-11-20 ("Registrar al firmar").
2. **A certificate is recognised by its DER, equal to the agent's or to its pending one.** Against ordering certificates by their `notBefore` and serial, which the pending record makes unnecessary and which would admit a certificate the server never issued.
3. **A withdrawal is `withdrawn_at`, not a `withdrawn` state.** Against a new enum value, which the migrations' one transaction could not use.
4. **`consumed_by_agent_id` gives way to `agents.enrolled_by_token_id`.** Against keeping the column: a token for several uses enrolls several agents.
5. **At most 1000 uses and 7 days.** The cap bounds what one leaked token enrolls; the operator issues another past it.
6. **The interleaved process from the heartbeat's own fields.** Against the sequence number, which restarts with the process, and the source address, which is personal data.
7. **The renewal's URL on the heartbeat's target.** Against a key of `agent.toml`, which would be deployment configuration.
8. **`409` for a renewal that is not due, `403` only for a revoked or superseded agent.** So that `403` keeps one meaning: the server refuses this agent.
9. **The agent's host name, platform and version kept on its row**: Manuel's choice of 2026-11-20 (data about people), recorded with the threat model's amendment; the data-protection document's change names it. **Cut, not refused, past 255 code points.** Against refusing the request, which would be a check narrower than SPEC-004's.

## References

- [ADR-0004](../adr/0004-agent-server-protocol.md) — the protocol; its Amendment 2026-11-20.
- [ADR-0003](../adr/0003-polyglot-storage.md) — the rate limits' backend, from which §Operational §5 departs under its §Compliance.
- [ADR-0022](../adr/0022-single-node-production-deployment.md) — the runtime roles' grants; its Amendment 2026-11-20.
- [SPEC-002](SPEC-002-agent-enrollment.md), [SPEC-003](SPEC-003-mtls-signed-envelope.md), [SPEC-004](SPEC-004-server-ingest-minimal.md) — enrollment, the envelope, the server.
- [SPEC-023](SPEC-023-production-deployment.md) — the grants' test, deploy_ac_004.
- [SPEC-024](SPEC-024-agent-installer-service.md) — the service's custody of the identity.
- [Threat model](../security/threat-model.md) — R4, R5 and R18.
