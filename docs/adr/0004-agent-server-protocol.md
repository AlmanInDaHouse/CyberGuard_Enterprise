# ADR-0004: Agent-Server secure protocol

- Status: Accepted
- Date: 2026-05-20
- Deciders: Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

The CyberGuard agent runs on customer endpoints and sends telemetry to the CyberGuard server over an untrusted network. The protocol between agent and server is the highest-impact security boundary in the entire system. A flaw here either compromises customer telemetry confidentiality, allows attacker-controlled events to poison detection, or lets a compromised endpoint impersonate other agents and produce arbitrary alerts.

ADR-0001 locked where the agent lives in the repository. ADR-0002 locked the languages on both sides of the boundary — Rust for the agent, Go for the ingest service. ADR-0003 locked Redis as the backend for ephemeral state, including the anti-replay nonces this protocol relies on. This ADR locks the protocol's authentication model, transport, message integrity, anti-replay defense, key custody, certificate rotation, and degraded-mode semantics.

The decision recorded here must:

1. Provide three independent layers of defense (transport, identity, message integrity) such that the compromise of any single layer does not yield full impersonation.
2. Defend against replay of captured traffic, including from a compromised endpoint.
3. Keep agent private key material out of attacker reach as long as the attacker remains at user-level on the endpoint.
4. Define what happens when the server is unreachable — buffering, ordering, retransmission.
5. Allow operators to revoke a misbehaving or stolen agent instantly.
6. Allow agent and server to evolve independently within a versioned compatibility contract.

## Decision

The Agent-Server protocol is mTLS 1.3 with Ed25519-signed message batches, anti-replay defense via a server-side nonce cache, per-agent X.509 client certificates issued by an internal CA on enrollment, and OS-backed local key storage on the endpoint.

### Enrollment

1. An operator generates an enrollment token from the dashboard. The token is a signed JWT: single-use, TTL 15 min, scope `enroll`, bound to `org_id`, signed by the server.
2. The agent installer receives the token plus the server URL.
3. On first run the agent generates an Ed25519 keypair locally. The private key is stored in OS-protected storage — Windows DPAPI in MVP; Linux keyring and macOS Keychain are deferred per ADR-0002 Rule 2.
4. The agent sends a Certificate Signing Request together with the enrollment token to `/v1/agents/enroll`.
5. The server validates the token (signature, scope, TTL, single-use), then issues an X.509 client certificate signed by the internal CA with `CN = agent_id` (UUIDv7) and `SAN = org_id`, with a TTL of 90 days.
6. The agent stores the client certificate alongside the private key.

### Transport in operation

- **mTLS 1.3 is mandatory.** TLS 1.2 is explicitly forbidden.
- The client validates the server certificate against the pinned internal CA.
- The server validates the client certificate against the internal CA *and* a Redis-backed revocation list (`agent_id` set, no TTL), checked on every connection.
- Allowed cipher suites are `TLS_AES_256_GCM_SHA384` and `TLS_CHACHA20_POLY1305_SHA256`.

### Message integrity per batch

Every event batch the agent sends carries a signed envelope:

```json
{
  "agent_id": "ag_01J...",
  "sequence_number": 482931,
  "timestamp": "2026-05-20T11:30:00.000Z",
  "nonce": "uuid-v4",
  "batch_hash": "sha256(canonical(events[]))",
  "signature": "ed25519(agent_priv_key, canonical(envelope_minus_sig))"
}
```

- `sequence_number` is monotonic and persisted on the agent.
- `timestamp` is ISO 8601 UTC with milliseconds.
- `batch_hash` is the SHA-256 of the canonicalised events array.
- `signature` is Ed25519 over the canonicalised envelope minus the signature field itself.

The server validates in this order, rejecting on the first failure:

1. mTLS handshake completed with a valid client certificate.
2. `agent_id` in the envelope matches the `CN` of the client certificate.
3. `timestamp` falls within ±5 min of server clock.
4. `sequence_number` is strictly greater than `last_seen_for_agent`.
5. `nonce` is not present in the Redis nonce cache.
6. `signature` validates under `agent_id`'s known public key.
7. `batch_hash` matches `sha256(canonical(events[]))` recomputed server-side.

On success the server stores `nonce` in Redis (TTL 10 min), updates `last_seen_for_agent`, and accepts the events.

### Rotation and revocation

- Client certificates have a TTL of 90 days.
- Automatic rotation begins on day 75: the agent generates a new keypair, submits a CSR over the existing mTLS connection, the server issues a new certificate, the agent atomically swaps key and certificate, and the server invalidates the old certificate.
- Revocation by `agent_id` is instantaneous: the operator marks the agent revoked in the dashboard, the revocation list entry is pushed to Redis with no TTL, and every subsequent connection is checked against the list.
- The internal CA is rotated annually with a 90-day overlap during which both old and new CAs are accepted.

### Heartbeat and degraded mode

- The agent sends a heartbeat every 30 seconds. The heartbeat is a signed envelope with an empty events array.
- An agent is considered offline after three consecutive missed heartbeats (a window of approximately 90 seconds).
- If the server is unreachable, the agent buffers events locally to disk, encrypted with a key derived from DPAPI (or the platform keyring once Linux and macOS land). The buffer cap defaults to 200 MB and 24 hours; both are configurable.
- On reconnect, the agent drains the buffer in `sequence_number` order with exponential backoff. There is no event reordering across the buffered and live streams.

### Version compatibility

- Every request from the agent carries the header `X-CG-Agent-Version`.
- The server holds a configured `min_supported_agent_version`. Agents below the minimum receive `426 Upgrade Required`. The agent installer surfaces the upgrade requirement clearly.

### Out of scope

- The wire format of events themselves. CGES is the subject of a future ADR.
- Agent-to-agent communication. Agents are sink-only in MVP.
- Agent calls to external threat-intelligence feeds. The MVP agent does not initiate outbound traffic beyond the server.

## Alternatives considered

### A1 — Bearer token over TLS, no mTLS

Pros: simpler implementation, no client-certificate lifecycle to manage.

Cons: token compromise yields full impersonation of the agent until revocation propagates; there is no cryptographic binding between agent identity and key material; the audit trail is weaker.

Rejected. The threat model of a security product cannot accept token-only agent identity.

### A2 — mTLS only, no message-level signature

Pros: simpler envelope, less CPU per batch.

Cons: TLS terminates at the reverse proxy or load balancer in any realistic production deployment. Without a per-message signature, a compromise of the TLS termination layer allows event injection that is invisible to the server. Message-level signature provides defense in depth.

Rejected. Defense in depth is non-negotiable on this boundary.

### A3 — Symmetric key (HMAC) instead of Ed25519

Pros: faster signature and verification, smaller signature size.

Cons: symmetric keys require secure key exchange and rotation; compromise of the agent key allows server impersonation in addition to agent impersonation; Ed25519 with public-key infrastructure aligns naturally with the X.509 client certificates already required for mTLS.

Rejected. Asymmetric cryptography is the right tool here.

### A4 — gRPC with built-in auth interceptors instead of HTTP/2 + custom envelope

Pros: well-tooled, generated client and server stubs.

Cons: locks transport to the gRPC ecosystem; the Rust gRPC client landscape has multiple options each with their own tradeoffs; a custom envelope over HTTP/2 gives identical multiplexing benefits with no framework lock-in; debugging tooling for raw HTTP is universal.

Rejected as a pragmatic preference for HTTP/2 with JSON envelopes.

## Consequences

### Positive

- Three independent layers of defense — TLS for transport, X.509 client certificate for identity, Ed25519 signature for message integrity. Compromise of any single layer does not yield full impersonation.
- Anti-replay defense (`sequence_number` plus `nonce` plus `timestamp` window) closes the classic attack of replaying captured agent traffic.
- Local key storage uses OS-native protected stores. An attacker with user-level access on the endpoint still has to escalate to extract the key.
- The version header lets the server reject incompatible agents cleanly, preventing silent contract drift across releases.

### Negative

- The internal CA is a critical asset. Compromise is catastrophic and requires HSM or equivalent protection in enterprise deployments. MVP runs the CA in software with operational compensating controls.
- mTLS termination in load balancers is non-trivial. The reverse-proxy configuration must forward client-certificate details to `cg-ingest` in trusted headers. A future ADR may revisit this if Envoy or HAProxy specifics surface.
- Clock skew between agent and server in excess of ±5 min breaks ingest. NTP requirements must be documented in the agent installation runbook.
- An encrypted local buffer means a compromised endpoint can read its own buffered events but cannot tamper with what has already been sent. This is acceptable; the threat model assumes endpoint compromise as a possibility.

### Neutral

- Ed25519 is chosen over Ed448, ECDSA, or RSA: smaller signatures, faster verification, no curve-parameter footguns. It is standard in modern crypto stacks (`libsodium`, `ring`, `rustls`).
- The `/v1/` in the enrollment URL implies versioned API endpoints. API-versioning policy is a future concern, likely tied to ADR-0008 (contract generation tooling).

## Compliance

Subsequent ADRs and SPECs that touch the agent-server boundary must reference this ADR. Any change to enrollment, transport, message integrity, rotation, revocation, heartbeat, or version compatibility opens a superseding ADR; configuration tuning (timeouts, buffer caps, cipher-suite ordering) lives in operations documentation rather than ADRs.

The wire format of events themselves is reserved for a dedicated future ADR (`ADR-0006` — CGES alignment with OCSF), which depends on this one for the envelope semantics in which CGES events travel.

## References

- [ADR-0001](0001-monorepo-layout.md) — Monorepo layout
- [ADR-0002](0002-language-per-component.md) — Language per component (Rust for agent, Go for ingest)
- [ADR-0003](0003-polyglot-storage.md) — Polyglot storage (Redis hosts nonces and revocation list)
- Blueprint §7 — Agent-Server Secure Protocol
- Blueprint §8 — CGES (event payload schema, separate concern reserved for ADR-0006)
