# SPEC-003: mTLS 1.3 and signed envelope

- **ID:** SPEC-003
- **Title:** mTLS 1.3 and signed envelope
- **Status:** Accepted
- **Depends on:** ADR-0001, ADR-0002, ADR-0004 §Transport / §Message integrity / §Server validation order, ADR-0006, SPEC-001, SPEC-002
- **Authors:** Manuel (product owner), Claude (architecture advisor), Claude Code (implementation)
- **Created:** 2026-05-21
- **Last updated:** 2026-05-22

## Motivation

SPEC-002 gave the agent a cryptographically verifiable identity (an Ed25519 keypair and a server-issued X.509 client certificate). It did **not** make the heartbeat path *use* that identity: SPEC-001 still POSTs an unsigned envelope over plain HTTP, and SPEC-001 §Scope OUT and SPEC-002 §Scope OUT both defer the secure transport to "a future SPEC".

SPEC-003 is that SPEC. After it, the heartbeat path:

1. Connects over **TLS 1.3 only**, presenting the SPEC-002 client certificate (mutual TLS).
2. Validates the **server** certificate against a configured trust anchor (the internal CA root, per ADR-0004 §Transport).
3. Wraps each SPEC-001 heartbeat envelope as the inner `body` of a new **outer signed envelope** carrying a per-message nonce, a send timestamp, and an Ed25519 signature made with the SPEC-002 private key.

This closes the three-layer defense ADR-0004 mandates — transport (TLS), identity (client cert), message integrity (signature) — such that compromise of any single layer does not yield full impersonation.

The two primitives are **coupled** (the cert TLS presents is the cert SPEC-002 issued; the key that signs is the key SPEC-002 persisted) but **functionally distinct** (different threat properties, failure modes, and test surfaces). SPEC-003 treats them as one protocol with two halves.

## Scope

### In scope

- TLS 1.3 client authentication on the heartbeat path, presenting the SPEC-002 `cert.pem` and signing the handshake with the SPEC-002 private key.
- Server-certificate verification against a configured trust anchor PEM (`server.trust_anchor_path`).
- A versioned **outer signed envelope** that wraps the SPEC-001 inner envelope verbatim and adds `nonce`, `sent_at`, `agent_id`, `sequence_number`, and an Ed25519 `signature`.
- Per-envelope nonce generation (16 random bytes from `OsRng`).
- Canonical serialization for signing (JCS, RFC 8785) so the server can reproduce the signed bytes.
- Agent-side handling of TLS, certificate, signature, clock-skew, and server-rejection failure modes.
- Integration harness with one Rust test per acceptance criterion, against a TLS-enabled in-process mock.

### Out of scope (deferred)

- **Certificate rotation and renewal.** ADR-0004 §Rotation specifies day-75 auto-rotation of a 90-day cert. SPEC-003 *uses* the cert; rotation remains a dedicated future SPEC (re-enrollment is the recovery path until then, per SPEC-002).
- **Server-side nonce store and replay cache.** ADR-0004 §Server validation order step 5 keeps nonces in Redis. That is a server-SPEC concern. SPEC-003 specifies only what the **agent** emits and how it reacts to server rejections; the SPEC-003 test mock implements a minimal in-memory nonce check to exercise the agent's behavior, not a production cache.
- **Enrollment endpoint TLS.** `/v1/agents/enroll` stays **plain HTTP**, exactly as SPEC-002 defined it. This is the chicken-and-egg constraint: at enrollment time the agent has no client certificate yet, so it cannot perform mTLS client authentication. mTLS applies only to the heartbeat path, which runs *after* enrollment. Enrollment confidentiality remains a closed-network requirement until a future SPEC introduces server-auth-only TLS for enrollment.
- **Cipher-suite negotiation / TLS-version fallback.** TLS 1.3 only; no 1.2/1.1/1.0/SSL under any condition. Cipher suites are fixed to the ADR-0004 set (see §Functional requirements); they are not negotiable down.
- **TLS session resumption (0-RTT / tickets).** Not implemented in SPEC-003; each heartbeat interval may establish a fresh connection. A session-reuse NFR may follow if handshake cost proves material.
- **Revocation lists / OCSP / CRL.** Out. Per ADR-0004 the server validates a client cert against its own issuance record and a Redis revocation set; the agent does not consult revocation data for the server cert beyond trust-anchor chain validation.
- **Persisted cross-restart monotonic `sequence_number`.** ADR-0004 §Message integrity calls `sequence_number` "persisted on the agent". SPEC-001 deliberately scoped it as per-process (resets to 1 on restart). SPEC-003 keeps that: its anti-replay guarantee rests on `nonce` + `sent_at`, not on cross-restart sequence monotonicity. Persisted sequence pairs naturally with the deferred buffered-offline-events feature (ADR-0004 §Heartbeat) and lands with it. See §Drift from ADR-0004.
- **Buffered offline events.** Still deferred per ADR-0004 §Heartbeat, as in SPEC-001/002.
- **The server.** SPEC-003 tests use a TLS-enabled in-process mock. The production server (ingest) is a separate SPEC.

## Drift from ADR-0004 (declared, scoped)

SPEC-003 aligns tightly with ADR-0004 §Transport, §Message integrity, and §Server validation order. Three deliberate, justified drifts:

- **D1 — Signed region wraps the body inline instead of carrying a `batch_hash`.** ADR-0004's envelope carries `batch_hash = sha256(canonical(events[]))` and signs `canonical(envelope_minus_sig)`. SPEC-003 embeds the SPEC-001 heartbeat envelope directly as `body` inside the signed region and signs `JCS(outer_envelope_minus_signature)`. Because the body is *inside* the signed bytes, the signature commits to it directly — `batch_hash` is redundant and is dropped. `batch_hash` returns in the future events-batch SPEC, where embedding a large `events[]` array inside the signed region is impractical and a hash indirection is warranted. For the heartbeat (no `events[]`), inline embedding is simpler and strictly no weaker. **The signature still covers the whole envelope-minus-signature, exactly as ADR-0004 mandates** — this drift is about `batch_hash`, not about the signed region.
- **D2 — `nonce` is 16 random bytes (base64url-unpadded), not a UUIDv4.** ADR-0004 §Message integrity shows `"nonce": "uuid-v4"`. A UUIDv4 carries 122 bits of entropy; 16 raw `OsRng` bytes carry 128. SPEC-003 uses the raw-bytes form: stronger, simpler (no UUID formatting), and opaque to the server (which treats the nonce as a uniqueness token). The collision bound is in §Security considerations.
- **D3 — `sequence_number` is per-process, not persisted across restarts.** See §Scope OUT. Anti-replay in SPEC-003 is carried by `nonce` (uniqueness) + `sent_at` (freshness window); cross-restart sequence monotonicity is deferred with the buffered-offline-events feature.

No drift in: TLS 1.3-only enforcement, the cipher-suite set, server-certificate trust-anchor (CA) validation, the signature covering the canonical envelope-minus-signature, or the server validation order the agent's output is built to satisfy.

## Functional requirements

- **FR-001.** The **secure heartbeat path** (TLS 1.3 mTLS + signed envelope) is active when `server.trust_anchor_path` is configured in `agent.toml`. When it is absent, the agent runs the SPEC-001 legacy path (plain HTTP + bare inner envelope) unchanged. This config-gating is the backward-compatibility mechanism (mirrors SPEC-002's optional `[enrollment]`); it keeps SPEC-001's closed-test deployments and test surface intact.
- **FR-002.** When the secure path is active, `[enrollment]` MUST be configured and a persisted identity (cert + key) MUST be loadable (SPEC-002). The agent uses that identity for both TLS client auth and envelope signing. If `server.trust_anchor_path` is set but no identity can be resolved, the agent exits with the SPEC-002 identity exit code (`5`) — there is nothing to authenticate with.
- **FR-003.** TLS 1.3 is enforced. The agent MUST refuse to negotiate any protocol version below TLS 1.3 (no 1.2/1.1/1.0/SSL). A server that cannot complete a TLS 1.3 handshake is treated as a handshake failure (FR-011).
- **FR-004.** The agent presents its SPEC-002 client certificate (`cert.pem`) and proves possession of the SPEC-002 private key on **every** TLS handshake on the secure path (mutual TLS).
- **FR-005.** The agent validates the server certificate chain against the trust anchor(s) in `server.trust_anchor_path` (PEM, one or more root certificates). Standard X.509 path validation applies: chain to a configured root, validity period (not expired / not yet valid), and basic constraints. Hostname verification follows RFC 6125 against `server.url`'s host.
- **FR-006.** Allowed TLS 1.3 cipher suites are exactly `TLS_AES_256_GCM_SHA384` and `TLS_CHACHA20_POLY1305_SHA256` (ADR-0004 §Transport). The agent does **not** offer `TLS_AES_128_GCM_SHA256` or any other suite.
- **FR-007.** Each heartbeat on the secure path is sent as the **outer signed envelope** defined in §Data contracts. `outer_envelope_version` is the constant `"0.1.0"`.
- **FR-008.** For every outer envelope the agent generates a fresh `nonce`: 16 bytes from `OsRng` (`getrandom`), base64url-encoded without padding. A new nonce is generated per envelope, never reused across retries or intervals.
- **FR-009.** `sent_at` is RFC 3339 / ISO 8601 UTC with milliseconds, taken from the agent's clock at envelope construction. It is the anti-replay timestamp the server checks against its ±5 min window (ADR-0004 §Server validation order step 3). It uses the same UTC-anchored model as SPEC-001 §FR-011.
- **FR-010.** The `signature` is a detached Ed25519 signature, made with the SPEC-002 private key, over the **canonical serialization (JCS, RFC 8785) of the outer envelope with the `signature` field removed** — i.e. over `{outer_envelope_version, agent_id, sequence_number, nonce, sent_at, body}`. It is base64url-encoded without padding. The on-the-wire envelope is standard JSON; only the *signed bytes* are canonical. The server recomputes the identical JCS canonicalization to verify (§Data contracts).
- **FR-011.** The inner `body` is the SPEC-001 heartbeat envelope **verbatim and unchanged** (`envelope_version` stays `"0.1.0"`, all SPEC-001 fields intact). SPEC-003 wraps; it does not modify the inner shape.
- **FR-012.** **Failure handling.** The agent reacts to secure-path failures as follows (codes in §Failure modes):
  - Server certificate untrusted / expired / hostname mismatch → terminal, exit `6` (fail closed; ratified Session 7).
  - TLS handshake rejected by the server because of the **client** certificate (server does not recognize / has revoked / sees an expired cert) → terminal, exit `7` (ratified Session 7).
  - TLS handshake transient failure (connection reset, timeout, DNS) → transport failure: retry with the SPEC-001 backoff policy; on exhaustion log `warn` and wait for the next interval (do not exit).
  - Local signature/crypto operation failure → terminal, exit `8`.
  - Server accepts the TLS connection but **rejects the signed envelope** (bad signature, replayed nonce, stale timestamp, unknown agent) → log `warn`, do not exit, proceed to the next interval. The agent cannot self-correct most of these mid-run; continuing lets transient conditions (e.g. clock drift corrected by NTP) recover.

## Non-functional requirements

- **NFR-001.** TLS handshake latency budget: a full 1.3 handshake (1-RTT) on the secure path should complete within **2 s** under nominal network latency; it shares the SPEC-001 `heartbeat.request_timeout_seconds` budget for the combined connect+send.
- **NFR-002.** Signature compute budget: Ed25519 signing of one envelope is sub-millisecond on commodity hardware and MUST NOT exceed **5 ms**; it is negligible against the 30 s interval.
- **NFR-003.** No key material in logs, reaffirming SPEC-002 NFR-002: the private key is never logged at any level; the signature bytes are not logged at `info` (debug only); the nonce may be logged.
- **NFR-004.** TLS library: **rustls** (see §Security considerations for rationale). The agent builds a rustls `ClientConfig` pinned to TLS 1.3, the ADR-0004 cipher suites, a root store loaded from `server.trust_anchor_path`, and a client-auth resolver backed by the SPEC-002 cert + key.
- **NFR-005.** Memory: TLS session state per connection is bounded and small (single connection at a time on the heartbeat path); steady-state resident memory stays within the SPEC-001 NFR-001 budget (< 30 MB).
- **NFR-006.** Source passes `cargo fmt --check` and `cargo clippy -- -D warnings`, including the new modules and dependencies. The secure path is fully exercised in CI (rustls is platform-agnostic; no Windows-only asymmetry as in SPEC-002).

## Data contracts

The outer signed envelope is a **transport-level meta-message**, not a CGES event — same reasoning as SPEC-001 and SPEC-002. No new CGES schema is introduced (B2 skipped).

### Outer signed envelope (wire JSON)

```json
{
  "outer_envelope_version": "0.1.0",
  "agent_id": "01934abc-def0-7000-89ab-000000000001",
  "sequence_number": 1,
  "nonce": "9f8c1b0e2d3a4f5061728394a5b6c7d8",
  "sent_at": "2026-05-21T10:23:11.482Z",
  "body": {
    "envelope_version": "0.1.0",
    "agent": {
      "agent_id": "01934abc-def0-7000-89ab-000000000001",
      "agent_version": "0.1.0",
      "agent_platform": "windows",
      "agent_hostname": "FIN-PC-014"
    },
    "sequence_number": 1,
    "sent_at": "2026-05-21T10:23:11.480Z",
    "status": "online",
    "uptime_seconds": 0
  },
  "signature": "<base64url-unpadded Ed25519 signature>"
}
```

- `outer_envelope_version` — string constant `"0.1.0"`. Independent of the inner `envelope_version`.
- `agent_id` — UUIDv7; the enrolled identity. MUST equal the `CN` of the presented client certificate (ADR-0004 §Server validation order step 2) and MUST equal `body.agent.agent_id`.
- `sequence_number` — mirrors `body.sequence_number` (the SPEC-001 per-process counter) at the envelope level, so the server reads it without parsing the body (ADR-0004 step 4). Per-process; see Drift D3.
- `nonce` — base64url-unpadded encoding of 16 `OsRng` bytes. Fresh per envelope (FR-008). Opaque uniqueness token; the example above shows hex for readability but the wire form is base64url.
- `sent_at` — RFC 3339 UTC with milliseconds; the anti-replay timestamp (FR-009).
- `body` — the SPEC-001 inner envelope **verbatim** (FR-011).
- `signature` — base64url-unpadded Ed25519 detached signature over the canonical signed region (below).

### Canonical signed region (what the signature covers)

The signature input is the **JCS (RFC 8785) canonicalization** of the outer envelope **with the `signature` field removed**:

```text
signed_bytes = JCS({
  "outer_envelope_version", "agent_id", "sequence_number",
  "nonce", "sent_at", "body"
})
signature   = base64url_unpadded( Ed25519_sign(agent_private_key, signed_bytes) )
```

JCS canonicalization (RFC 8785) recursively sorts object keys (including inside `body` and `body.agent`), uses minimal whitespace, and a fixed string/number form. The envelope contains only strings, unsigned integers (`sequence_number`, `uptime_seconds`), and nested objects — no floats, no sub-millisecond fractions — so the JCS number-formatting edge cases do not arise. Both agent and server MUST produce byte-identical `signed_bytes`; reproducibility is the whole point (§Security considerations). The wire serialization of the full envelope (including `signature`) is ordinary JSON and need not be canonical.

## Configuration

### New keys in `agent.toml`

```toml
[server]
url = "https://ingest.cyberguard.example:8443"
trust_anchor_path = "C:/ProgramData/CyberGuard/agent/server-ca.pem"

[tls]
minimum_version = "1.3"   # documented constant; not runtime-tunable below 1.3

[envelope]
canonical_form = "JCS"    # RFC 8785; the only supported value in SPEC-003
```

- `server.trust_anchor_path` — PEM file with one or more root certificates the agent trusts for **server** identity. Its presence activates the secure path (FR-001).
- `tls.minimum_version` — documented as `"1.3"`. The agent rejects any configured value below 1.3 at load time; it exists to make the floor explicit and auditable, not to allow downgrade.
- `envelope.canonical_form` — `"JCS"`. Documented for forward-compatibility; any other value is a config error in SPEC-003.

### Relationship to existing keys

- When `server.trust_anchor_path` is set, `server.url` SHOULD use the `https://` scheme; an `http://` URL with a trust anchor configured is a config error (the secure path requires TLS).
- `[enrollment]` (SPEC-002) is required on the secure path (FR-002).
- The SPEC-001 `[heartbeat]` block (interval, timeout, retry/backoff) is reused unchanged; the retry policy governs transient TLS/transport failures.

## Behavior

State machine — the SPEC-001/002 startup is unchanged through identity resolution; the heartbeat phase gains a connection-establishment step and a signed payload:

```text
Startup
  └─► LoadConfig ─[invalid]─► Exit(2)
        └─► InitLogger
              └─► CheckIdentity (SPEC-002)
                    ├─[secure path off]──► Heartbeating (SPEC-001 plain HTTP, inner envelope)
                    └─[secure path on]
                          └─► LoadIdentity (SPEC-002) ─[fail]─► Exit(5)
                                └─► SecureHeartbeating
                                       │
                                ┌──────┴───────────────────────────────┐
                                │ each interval (SPEC-001 schedule)     │
                                │   1. TLS 1.3 handshake (present       │
                                │      client cert; validate server     │
                                │      cert vs trust anchor)            │
                                │      ├─[server cert untrusted]─► Exit(6)
                                │      ├─[client cert rejected] ─► Exit(7)
                                │      └─[transient]─► retry/backoff;    │
                                │                       warn; next tick  │
                                │   2. Build inner envelope (SPEC-001)   │
                                │   3. Wrap: nonce + sent_at + sign      │
                                │      ├─[sign fails]─► Exit(8)           │
                                │   4. POST outer envelope over TLS      │
                                │      ├─[server rejects envelope]─► warn;│
                                │      │                            next  │
                                │      └─[2xx]─► info; next tick          │
                                └────────────────────────────────────────┘
                                       │ on SIGINT
                                       ▼
                                ShuttingDown (final signed envelope,
                                status="going_offline", single attempt) ─► Exit(0)
```

- **SecureHeartbeating** preserves SPEC-001 scheduling semantics (FR-011 absolute timeline, FR-005 sequence assignment, FR-008 graceful-shutdown final heartbeat). Only the transport (TLS) and payload (signed outer envelope) change.
- **Connection establishment** performs the TLS 1.3 handshake, presenting the client cert and validating the server cert. The validation order the agent's output is built to satisfy is ADR-0004 §Server validation order steps 1–6 (step 7 `batch_hash` is N/A under Drift D1).
- **Reconnection** on a transient TLS failure follows the SPEC-001 retry/backoff policy; a fresh handshake is attempted each interval (no session resumption in SPEC-003).
- The first heartbeat after entering SecureHeartbeating fires within the SPEC-001 §FR-004 5 s budget measured from entry into the state.

## Failure modes

| Failure | Detection | Exit code | Behavior / stderr |
|---|---|---|---|
| Config: `trust_anchor_path` set but `server.url` is `http://`, or `canonical_form` ≠ `JCS`, or `tls.minimum_version` < 1.3 | Config validation | `2` | `cg-agent: invalid config: <detail>` |
| Secure path on but no loadable identity (SPEC-002) | Identity load | `5` | SPEC-002 identity-load stderr line |
| Server certificate untrusted / expired / hostname mismatch | TLS handshake (server-cert validation) | `6` | `cg-agent: tls: server certificate verification failed: <detail>` |
| TLS handshake rejected by server due to client cert (unknown/revoked/expired) | TLS handshake (alert from server) | `7` | `cg-agent: tls: server rejected client certificate` |
| Transient TLS/transport failure (reset, timeout, DNS) | TLS/IO error | — | Retry per SPEC-001 backoff; on exhaustion log `warn`, next interval. No exit. |
| Local signature/crypto operation failure | signing error | `8` | `cg-agent: signing failed: <detail>` |
| Server rejects the signed envelope (bad signature / replayed nonce / stale timestamp / unknown agent) | non-2xx from server | — | Log `warn` (`signed envelope rejected by server`), next interval. No exit. |
| Local clock skew beyond ±5 min → server rejects timestamps | non-2xx (timestamp error) | — | Same as envelope rejection: `warn`, continue. NTP correction may recover. |

Exit codes retained: `0` success, `1` runtime, `2` config, `3` enrollment refused, `4` enrollment unreachable, `5` identity persist/load. Added by SPEC-003 (orthogonal range): `6` server-certificate verification failure, `7` server rejected client certificate, `8` local signature/crypto failure.

## Observability

All logs follow SPEC-001 JSON conventions. New events:

| Event | Level | Required fields |
|---|---|---|
| TLS handshake started | `info` | `server.url` |
| TLS handshake succeeded | `info` | `tls_version` (`"1.3"`), `cipher_suite` |
| Server certificate accepted (first connection) | `info` | `subject`, `issuer`, `not_after` |
| TLS handshake failed (server cert) | `error` | `reason` |
| TLS handshake failed (client cert rejected) | `error` | `reason` |
| TLS handshake failed (transient) | `warn` | `attempt`, `error` |
| Envelope signed | `debug` | `sequence_number`, `nonce` (signature bytes are NOT logged) |
| Signed heartbeat sent (accepted) | `info` | `sequence_number`, `status`, `sent_at`, `response_status` |
| Signed envelope rejected by server | `warn` | `sequence_number`, `response_status` |
| Signing failed | `error` | `sequence_number`, `reason` |

The private key and the raw signature bytes are never logged at `info`. The nonce may be logged (it is public and single-use).

## Acceptance criteria

Each AC maps 1:1 to a Rust integration test under `agent/cg-agent/tests/`, named `mtls_ac_NNN_*` to avoid collision with SPEC-001 `ac_NNN_*` and SPEC-002 `enroll_ac_NNN_*`. Tests run against a TLS-enabled in-process mock whose trust material (server cert + test CA, and an agent identity whose pubkey the mock knows) is generated at test setup and never committed.

- **AC-001.** Given a valid identity and a trust anchor matching the mock's server cert, the agent completes a TLS 1.3 mutual handshake and delivers its first signed heartbeat, which the mock accepts.
- **AC-002.** When the mock presents a server certificate that does **not** chain to the configured trust anchor, the agent refuses the connection and exits `6` with a stderr line containing `server certificate verification failed`.
- **AC-003.** A signed envelope the mock receives verifies: recomputing `JCS(outer minus signature)` and checking the Ed25519 signature against the agent's enrolled public key succeeds. Tampering with any signed field (e.g. flipping a `nonce` byte) makes verification fail.
- **AC-004.** Across N heartbeats the `nonce` values are all distinct and each decodes to exactly 16 bytes. A replayed envelope (identical `nonce` re-POSTed) is rejected by the mock, and the agent treats a server rejection as non-fatal (logs `warn`, continues to the next interval, does not exit).
- **AC-005.** An envelope whose `sent_at` is outside ±5 min of the mock's clock is rejected by the mock; the agent logs `warn` and continues (does not exit). *(Driven by injecting skew at the mock's validation, since the agent uses its real clock.)*
- **AC-006.** Against a mock that offers only TLS 1.2, the agent refuses to negotiate and never transmits a signed envelope; the failure surfaces as a handshake error (no cleartext or sub-1.3 transmission occurs).
- **AC-007.** On every handshake the mock (configured to require client auth) receives the agent's SPEC-002 client certificate; the certificate's `CN` equals the outer envelope `agent_id`.
- **AC-008.** When the mock rejects the client certificate at the TLS layer, the agent exits `7` with a stderr line containing `server rejected client certificate`.
- **AC-009.** The inner `body` of the outer envelope equals the SPEC-001 envelope verbatim: `envelope_version = "0.1.0"`, the four-field `agent` block, `sequence_number`, `sent_at`, `status`, `uptime_seconds` — byte-for-byte the shape SPEC-001 emits. (Backward-compat regression.)
- **AC-010.** With `server.trust_anchor_path` **absent**, the agent uses the SPEC-001 plain-HTTP path and the bare inner envelope — confirmed by the unchanged SPEC-001 integration tests continuing to pass. (Config-gating regression; no new test, asserted by the SPEC-001 suite.)

## Security considerations

### Why TLS 1.3 only, no 1.2 fallback

TLS 1.2 permits cipher suites and handshake constructions (RSA key exchange, CBC modes, renegotiation) with a long tail of downgrade and padding-oracle issues. TLS 1.3 removes them, mandates forward secrecy, and encrypts more of the handshake. A security product's agent has no legacy-server compatibility need — both endpoints are ours. Allowing 1.2 "just in case" only creates a downgrade target. ADR-0004 §Transport forbids 1.2; SPEC-003 enforces a hard 1.3 floor with no negotiated fallback (FR-003, FR-006).

### Why a per-message signature in addition to mTLS

Defense in depth (ADR-0004 §A2). TLS terminates at the reverse proxy / load balancer in any realistic deployment; from there to the ingest service the traffic is no longer protected by the agent's TLS session. A per-message Ed25519 signature, verified at ingest against the agent's enrolled key, means a compromise of the TLS-termination layer cannot inject events that appear authentic. It also gives an **auditable** record independent of the (ephemeral) TLS session: the signed envelope can be retained and re-verified. And the replay window for a signed envelope (bounded by the ±5 min timestamp + single-use nonce) is far narrower than a TLS session lifetime.

### Why the signature covers the whole envelope-minus-signature

The signed region is `JCS(outer minus signature)` — it binds `agent_id`, `sequence_number`, `nonce`, `sent_at`, and `body` together. Signing only the `body` would leave the anti-replay fields (`nonce`, `sent_at`) and the identity (`agent_id`) mutable by an on-path attacker or a compromised termination layer while keeping a valid body signature — which is precisely the threat the message signature exists to stop. Binding everything is what ADR-0004 §Message integrity mandates ("signature over the canonicalised envelope minus the signature field").

### Nonce uniqueness

The nonce is 16 bytes (128 bits) from `OsRng` (`getrandom` → `BCryptGenRandom` on Windows, `/dev/urandom` on Unix). By the birthday bound, the probability of any collision stays below 2⁻³² until roughly 2⁴⁸ (~2.8×10¹⁴) envelopes — unreachable at one heartbeat per 30 s (≈10⁶/year/agent). Combined with the server's single-use nonce cache (server SPEC) and the ±5 min timestamp window, replay of a captured envelope is rejected: either the nonce is already cached, or the timestamp has aged out.

### Canonical form requirement

The signature is over bytes. If the agent and server serialize the signed region differently (key order, whitespace, number form, Unicode normalization), the server computes a different byte string and verification fails on legitimate traffic. JCS (RFC 8785) gives a single deterministic byte form for a given JSON value, reproducible across languages (the Rust agent and the Go ingest service can both implement it). Without a canonical form the signature is unverifiable in practice. SPEC-003 fixes JCS; the envelope avoids floats and sub-millisecond fractions so the JCS number-formatting corners never arise.

### Clock-skew threat

The ±5 min `sent_at` window (ADR-0004) bounds how long a captured envelope remains replayable on timestamp grounds and how far a misconfigured agent clock can drift before ingest rejects it. It does **not** defend against an attacker who controls the agent's clock (endpoint compromise is an accepted threat) nor against replay *within* the 5 min window (the nonce cache covers that). NTP on the endpoint is an operational requirement (ADR-0004 §Consequences); the agent does not itself correct or detect skew beyond reacting to server rejections (FR-012).

### Trust anchor distribution

`server.trust_anchor_path` must contain the correct internal-CA root. Getting the right PEM onto the endpoint is an **operator** concern (installer / config management), not an agent concern — but the consequence is the agent's: a wrong or attacker-supplied trust anchor means the agent will trust a wrong (or attacker-run) server. The agent fails closed on a chain that does not validate (FR-005, exit 6); it cannot detect a *maliciously correct-looking* anchor. This is the same trust-bootstrapping assumption as any PKI client.

### Downgrade attacks

TLS 1.3 only, enforced at the client config (FR-003). No 1.2/1.1/1.0/SSL is offered or accepted under any condition, including in response to a server that requests downgrade. The cipher-suite set is fixed to two AEAD suites (FR-006); `TLS_AES_128_GCM_SHA256` is not offered. There is no negotiated path to a weaker primitive.

## Ratification record

The three decisions surfaced for Manuel's call were ratified in the Session 7 review, each at the recommended default:

1. **Persistent server-certificate verification failure → exit `6` (fail closed).** A failing chain is either misconfiguration (wrong trust anchor) or an active MITM; neither self-heals by retrying, and silently retrying against an unvalidated server is dangerous. Reflected in FR-012, the §Failure modes table, and AC-002. (Alternative considered and rejected: retry as a transient transport failure to ride out a legitimate server-cert rotation window — rejected in favor of fail-closed security.)
2. **TLS client-certificate rejection by the server → exit `7`.** A rejected client cert means expired / revoked / unknown — operator action (re-enroll) is required, and retrying the same rejected cert is futile and noisy. Reflected in FR-012, the §Failure modes table, and AC-008. (Alternative considered and rejected: retry in case the rejection is transient server-side state — rejected to give a fast, clear operator signal.)
3. **No server-leaf SPKI pinning — CA trust anchor only.** ADR-0004 §Transport requires CA pinning (the trust anchor), not leaf SPKI pinning. Leaf pinning is stronger against a compromised-CA MITM but operationally fragile (breaks on every legitimate server-cert rotation, needs coordinated pin updates). Reflected in §Scope OUT and §Security considerations > Trust anchor distribution. (Alternative considered and rejected: pin the server SPKI hash, accepting the rotation-coordination burden.)

## References

- [ADR-0001](../adr/0001-monorepo-layout.md) — Monorepo layout (`agent/`, `docs/specs/`).
- [ADR-0002](../adr/0002-language-per-component.md) — Rust for the agent.
- [ADR-0004](../adr/0004-agent-server-protocol.md) — Agent-Server secure protocol. SPEC-003 implements §Transport (mTLS 1.3) and §Message integrity (signed envelope) for the heartbeat path, aligned to §Server validation order; rotation/revocation and the server-side nonce cache remain deferred.
- [ADR-0006](../adr/0006-cges-ocsf-alignment.md) — CGES alignment. The outer envelope is a transport meta-message, not a CGES event (no new schema).
- [SPEC-001](SPEC-001-agent-heartbeat.md) — Agent heartbeat. SPEC-003 wraps its envelope verbatim as `body` and reuses its scheduling/retry semantics.
- [SPEC-002](SPEC-002-agent-enrollment.md) — Agent enrollment. SPEC-003 uses the cert and key SPEC-002 issues and persists.
- [RFC 8446](https://www.rfc-editor.org/rfc/rfc8446) — TLS 1.3.
- [RFC 8032](https://www.rfc-editor.org/rfc/rfc8032) — Ed25519 (EdDSA).
- [RFC 8785](https://www.rfc-editor.org/rfc/rfc8785) — JSON Canonicalization Scheme (JCS).
- [Foundational Blueprint](../product/blueprint.md) — §7 (Agent-Server Secure Protocol).
