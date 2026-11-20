# ADR-0004: Agent-Server secure protocol

- Status: Accepted
- Date: 2026-05-20
- Last updated: 2026-11-20
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

- `sequence_number` is monotonic per agent process lifetime; not persisted across restarts. See SPEC-003 §Drift D3 for the rationale.
- `timestamp` is ISO 8601 UTC with milliseconds.
- `batch_hash` is the SHA-256 of the canonicalised events array.
- `signature` is Ed25519 over the canonicalised envelope minus the signature field itself.

The server validates in this order, rejecting on the first failure:

1. mTLS handshake completed with a valid client certificate.
2. `agent_id` in the envelope matches the `CN` of the client certificate.
3. `timestamp` falls within ±5 min of server clock.
4. (Informational only since SPEC-003 §Drift D3.) `sequence_number` is logged for ordering within a session but is not used for anti-replay enforcement.
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
- Anti-replay defense (`nonce` plus `timestamp` window) closes the classic attack of replaying captured agent traffic.
- Local key storage uses OS-native protected stores. An attacker with user-level access on the endpoint still has to escalate to extract the key.
- The version header lets the server reject incompatible agents cleanly, preventing silent contract drift across releases.

### Negative

- The internal CA is a critical asset. Compromise is catastrophic and requires HSM or equivalent protection in enterprise deployments. MVP runs the CA in software with operational compensating controls.
- mTLS termination in load balancers is non-trivial. The reverse-proxy configuration must forward client-certificate details to `cg-ingest` in trusted headers. A future ADR may revisit this if Envoy or HAProxy specifics surface.
- Clock skew between agent and server in excess of ±5 min breaks ingest. NTP requirements must be documented in the agent installation runbook.
- An encrypted local buffer means a compromised endpoint can read its own buffered events but cannot tamper with what has already been sent. This is acceptable; the threat model assumes endpoint compromise as a possibility.

### Neutral

- Ed25519 is chosen over Ed448, ECDSA, or RSA: smaller signatures, faster verification, no curve-parameter footguns. It is standard in modern crypto stacks (`libsodium`, `ring`, `rustls`).
- The `/v1/` in the enrollment URL implies versioned API endpoints. API-versioning policy is a future concern.

## Compliance

Subsequent ADRs and SPECs that touch the agent-server boundary must reference this ADR. Any change to enrollment, transport, message integrity, rotation, revocation, heartbeat, or version compatibility opens a superseding ADR; configuration tuning (timeouts, buffer caps, cipher-suite ordering) lives in operations documentation rather than ADRs.

The wire format of events themselves is defined in a dedicated ADR ([ADR-0006](0006-cges-ocsf-alignment.md) — CGES alignment with OCSF), which depends on this one for the envelope semantics in which CGES events travel.

## Amendment 2026-05-23: persistent disk buffer deferred; sequence_number persistence retired; stale ADR-0008 cross-reference corrected

**Status.** This amendment supersedes parts of §Message integrity per batch, §Heartbeat and degraded mode, and §Consequences. ADR-0004 remains `Accepted`; the amendment convention is in-place per [docs/engineering-notes.md](../engineering-notes.md).

**Context.** This amendment carries three corrections to ADR-0004, all surfaced by Session 10's work and closed here in one atomic move. **(a)** ADR-0009 (Event delivery semantics + agent buffer model) adopts an in-memory ring buffer, ephemeral, FIFO drop on overflow — no disk persistence in SPEC-005. The persistent disk-backed encrypted buffer originally specified in §Heartbeat and degraded mode is deferred to a future SPEC. **(b)** That decision has a downstream consequence in this document: the cross-restart `sequence_number` persistence claim and its anti-replay role — already drifted by SPEC-003 §Drift D3 in Session 7 but never landed in this ADR's prose — loses its remaining justification once the persistent buffer is also gone. **(c)** ADR-0008 was reserved for "contract generation tooling" at the time of ADR-0004's original writing; it has since been ratified earlier in this same session as "ETW crate selection," and a §Consequences > Neutral cross-reference written under the old assumption is now misleading. Three causes, three effects, closed in one amendment per the established convention.

**Amendment, part (a) — buffer model deferred.** The third and fourth bullets of §Heartbeat and degraded mode (the disk-backed encrypted buffer with the 200 MB / 24 h cap, and the `sequence_number`-ordered drain on reconnect) are superseded by ADR-0009's in-memory ring buffer contract. The §Consequences > Negative bullet beginning *"An encrypted local buffer means…"* is similarly superseded — no persistent buffer exists in MVP, so the consequence is moot. The original bullets remain at their positions for historical context per the amendment convention; the binding contract is ADR-0009. Persistent disk-backed encrypted buffering for offline events is reserved for a future SPEC that will address at-rest encryption, file rotation, crash recovery, and the renewed cross-restart `sequence_number` persistence question.

**Amendment, part (b) — `sequence_number` persistence consequence retired.** Three bullets have been rewritten in-place to close the Session 7 SPEC-003 §Drift D3 declaration that never landed in ADR-0004's prose:

- §Message integrity per batch, first bullet — `sequence_number` is no longer claimed as "persisted on the agent"; the new text declares per-process monotonicity and cross-references SPEC-003 §Drift D3.
- §Message integrity per batch, server validation order step 4 — the strictly-greater-than check is rewritten as informational only; `sequence_number` is logged for ordering, not used for anti-replay enforcement.
- §Consequences > Positive, anti-replay bullet — `sequence_number` is removed from the anti-replay defense enumeration; only `nonce` plus `timestamp` window remain.

Cross-reference convention applied: each rewritten bullet cites SPEC-003 §Drift D3 (origin of the drift), not this amendment (closure of the drift). Provenance points to the cause, not the effect.

**Amendment, part (c) — stale ADR-0008 cross-reference corrected.** ADR-0008 was reserved at the time of ADR-0004's original writing for "contract generation tooling"; it has since been ratified earlier in this session as ADR-0008: ETW crate selection for Windows event capture, unrelated to API-versioning or contract generation. The §Consequences > Neutral bullet that referenced "ADR-0008 (contract generation tooling)" is rewritten in-place to drop the now-misleading attribution. API-versioning policy remains a future concern; if and when a future ADR addresses it, that ADR can cross-reference ADR-0004 — the reverse pointer is removed.

**Backward compatibility.** Strictly tightening. The wire envelope is unchanged: agent emits the same fields, server reads the same fields. The semantics of `sequence_number` no longer claim cross-restart persistence (the agent never honoured that claim since SPEC-003 D3 anyway) and no longer participate in anti-replay enforcement (the server's secure path was already `nonce` + `sent_at` per D3). No SPEC needs revision; SPEC-003 §Drift D3 stands and is now matched by ADR-0004's own prose.

**Effect on other sections.** §Context point 4 (must define server-unreachable behavior) — unchanged; the new model still defines it. §Enrollment, §Transport in operation, §Rotation and revocation, §Version compatibility, §Out of scope — all unchanged. §Consequences > Neutral — second bullet rewritten by part (c); first bullet (Ed25519 choice) unchanged. §Compliance — unchanged; the term "buffer caps" remains valid for the ephemeral ring's size cap.

## Amendment 2026-10-04: events travel inside the signed body; no `batch_hash` (SPEC-017)

**Status.** This amendment supersedes step 7 of the server validation order in §Message integrity per batch, and the "empty events array" wording of §Heartbeat and degraded mode. ADR-0004 remains `Accepted`.

**Context.** The 2026-05-23 cascade (this ADR, ADR-0009, SPEC-003) specified `events` and `batch_hash` as top-level fields of the outer envelope, with the server recomputing the hash. The implementation that shipped with SPEC-005 placed the events inside `body` instead (`body.events`), on the agent and on the server, and never computed `batch_hash`. The S32 audit surfaced the difference.

**Amendment.** The realized shape is the contract (SPEC-017 §Data contracts). `body` carries an optional `events` array; the signature covers `body`, so the events are signed directly and a separate hash adds nothing. Step 7 (verify `batch_hash`) is retired. A heartbeat without events omits the member.

**Effect on other sections.** None on steps 1–6, rotation, revocation or version compatibility. The integrity argument of §Message integrity per batch is unchanged: every event is under the envelope signature.

## Amendment 2026-10-19 (ADR-0022): enrollment over TLS

**What surfaced it.** Debt #89 (`docs/handoff-session-39.md:694`) and roadmap §F. §Enrollment does not name the transport of step 4's request. SPEC-004 FR-002 serves it over plain HTTP, so its token can be read on the wire, and the agent's enrollment client trusts the public web roots, not the internal CA (`agent/cg-agent/src/enrollment.rs`). §Compliance says that a change to enrollment or transport "opens a superseding ADR"; as the Amendments 2026-05-23 and 2026-10-04 did, this one changes the protocol in place, ADR-0022 being the decision.

**The amendment.** §Enrollment, step 4: the agent sends its request over TLS 1.3 only, and validates the server's certificate against its configured trust anchor, as §Transport in operation's client does against the pinned internal CA; the operator gives the agent that anchor with the token and the server URL (step 2). The anchor is the internal CA, or the CA that issued the server certificate when the operator provides that certificate (SPEC-004). The enrollment listener asks for no client certificate, since the agent has none yet.

**Effect.** §Transport in operation: its "pinned internal CA" is the agent's configured trust anchor, which is the internal CA unless the operator provides the server's certificate (SPEC-004), since one anchor will serve both of the agent's clients (today the enrollment client trusts the public web roots, `agent/cg-agent/src/enrollment.rs`, and the heartbeat client the anchor, `agent/cg-agent/src/tls.rs`); otherwise unchanged. §Rotation and revocation and §Version compatibility: unchanged. §Enrollment, step 2: the installer receives the anchor with the token and the server URL. Its other steps: unchanged; their signed JWT (debt #119) and their scope and org (debt #89), which are not built, stay for roadmap §F's renewal SPEC. §Compliance asks that such a change open a superseding ADR: this one is made in place, as the two earlier ones were, and decided by ADR-0022. Proposed by Claude Code on 2026-10-19 under Manuel's delegation (ADR-0022 §Decision record), and ratified by Manuel on 2026-10-23 with ADR-0022, in chat: "Sí, ratifico". Status stays `Accepted`.

## Amendment 2026-11-20 (SPEC-025): the token, renewal, revocation and the CA as roadmap §F builds them

**What surfaced it.** Roadmap §F's F4, the agents' certificate renewal (debt #88) with the rest of debt #89, debt #119 and the server's part of debt #170, and the contracts of F3 and F4, which roadmap step 3 writes together (`docs/product/roadmap.md`). The protocol as built departs from §Enrollment, §Transport in operation and §Rotation and revocation in six places: the token is an opaque value stored as typed, not a signed JWT (`services/ingest/src/cli/issue-token.ts`); the agent sends its raw public key, not a Certificate Signing Request (`services/ingest/src/routes/enroll.ts`); the CA signs a certificate before the token is checked (`enroll.ts`, debt #89); the certificate carries no `org_id`; there is no revocation list and no renewal (debt #88, R5 of `docs/security/threat-model.md`); and the CA is a root valid for ten years with no rotation (`services/ingest/src/ca.ts:14-15`). Manuel decided each point below on 2026-11-20, in chat, taking the recommendation of the message that put the options to him: "acepto cambios y sigo tus recomendaciones". §Compliance says that such a change "opens a superseding ADR"; this one changes the protocol in place, as the Amendments 2026-05-23, 2026-10-04 and 2026-10-19 did. SPEC-025 implements it.

**The amendment.**

1. **The enrollment token** (§Enrollment, steps 1 and 5). The token is an opaque value of 256 bits from a cryptographic random source, shown once to whoever issues it, an admin in the dashboard or an operator with the host's command, and never again. The server keeps its SHA-256 alone, under an identifier of its own, with its scope `enroll`, its org, its expiry and its remaining uses. Before it signs anything, the server checks the token's hash, scope, org, remaining uses, withdrawal and expiry, and locks its row; one transaction then signs the certificate, records the agent with the org the token names and the token that enrolled it, and consumes one use. A token is single-use and lives 15 minutes, unless an admin issues it in the dashboard for several uses: then it has at most 1000 uses and lives at most 7 days, the admin can withdraw it, and its page lists the agents it enrolled. The host's command issues single-use tokens alone. The database refuses a token beyond those bounds, and a withdrawal, once made, cannot be undone, by constraints and a trigger that ingest's owner role holds, so that neither running service can issue past them. There is no JWT: the token's row holds what its claims would, and a signing key would be one more key to keep.
2. **The key at enrollment and at renewal** (§Enrollment, step 4; §Rotation and revocation, second bullet). The agent sends its Ed25519 public key, not a PKCS#10 request. To renew, it generates a new key pair and sends the new public key over its mTLS connection, in a request signed by its current key as its envelopes are, carrying a signature by the new key over that request, so the server knows the requester holds both keys. The private key never leaves the endpoint (`docs/product/blueprint.md:222`), as with a Certificate Signing Request, and the agent needs no new dependency.
3. **The certificate** (§Enrollment, step 5). Its `CN` is the `agent_id`; it carries no `SAN` with the `org_id`. The server reads an agent's org from the agent's row, which its token set at enrollment, on every request. A certificate lasts 90 days, and its `notAfter` is never later than the CA's.
4. **Renewal** (§Rotation and revocation, second bullet). The agent asks for a new certificate once its current one has 45 days or fewer left, and asks again until it has one; the server refuses with `409`, not a `403`, a request from a certificate with more than 45 days left by the server's own clock. A request under a certificate issued later, by its `notBefore`, than the one the server holds for the agent, signed by the CA, for an agent not revoked, and whose key verifies the request's envelope, makes that certificate and its key the agent's; from then on a request under the older certificate is refused with `403` and the reason `agent_superseded`. So the old certificate stops being accepted at the agent's first request under the new one, and a lost answer to a renewal does not strand the agent, which still holds its old pair until its swap is complete. The same rule recovers a restore of the server's database taken before a renewal: the agent's next request, under its newer certificate, makes that certificate the agent's again. What a restore leaves: a revocation made after the backup is undone, and must be made again; a copy of the endpoint holding the older certificate is accepted until the agent's next request under the newer one. The same rule cuts the other way when a copy of the endpoint, or a thief of its key, renews first: the original's requests are then refused as superseded, and it cannot renew, until the operator revokes the agent and enrolls the endpoint anew (item 8); so is an endpoint restored from its own backup taken before a renewal.
5. **Revocation** (§Transport in operation, third bullet; §Rotation and revocation, third bullet). A revocation is a time and a reason on the agent's row in PostgreSQL, set once, its time the database's own, taken in the transaction that records it: a trigger that ingest's owner role holds refuses any change to them once set, so neither running service can undo a revocation, and undoing one is an act of the server host's administrator, through the owner role, by the runbook's procedure. The server checks it on every request of the agent, heartbeat and renewal alike, in the query that reads the agent's key (`services/ingest/src/routes/heartbeat.ts:176-177`), and refuses a revoked agent's request with `403` and the reason `agent_revoked`. It takes effect at the commit that records it. There is no list in Redis or Valkey, which is neither backed up nor restored (`docs/operations/backup-and-restore.md:13`) and holds each service's keys under its own prefixes (ADR-0022 §4), and no certificate revocation list, a signed artefact whose lapsed `nextUpdate` would refuse every agent. The TLS handshake still validates the client's certificate chain against the CA.
6. **What a refused agent does.** An agent refused with `403`, revoked or superseded, keeps running: it logs the refusal and does not exit, as for any refusal of its envelope (SPEC-003 FR-012, `docs/specs/SPEC-003-mtls-signed-envelope.md:77`), and retries the batch up to its limit and then drops it (`agent/cg-agent/src/delivery.rs`). The refusal is the handler's, after the TLS handshake: SPEC-003 FR-012's terminal exit `7` for a client certificate the handshake refuses (`:74`) does not apply to it. It does not exit, because the service control manager does not restart a service that stops with an error (`agent/cg-setup/src/scm.rs:482-483`): an exit would leave the endpoint without its agent until someone acts on it, and a mass revocation could not be undone from the server. A revoked agent cannot renew, so it stops at its certificate's `notAfter` as today.
7. **The CA** (§Rotation and revocation, fourth bullet). The internal CA is the root that ingest generates at its first start, valid for 3650 days, about ten years (`services/ingest/src/ca.ts:14-15`), with no scheduled rotation and no intermediate. A client certificate's `notAfter` never passes the CA's. A compromise of the CA's key is handled by the runbook's procedure: a new CA, and every agent enrolled again under it; where the agents' trust anchor is ingest's CA, not the CA of an operator-provided server certificate (Amendment 2026-10-19), the new CA's certificate is distributed as their anchor. The decision is reopened by debt #99, if the CA leaves ingest, or in the CA's ninth year, whichever comes first. Manuel accepts the risk of a single long-lived root by ratifying this amendment.
8. **A copied identity** (debt #170). A copy of an enrolled endpoint holds the agent's key, and the protocol cannot tell it from the original. The server detects it, and does not refuse it: it flags the agent whose requests present an older certificate after a newer one, or whose heartbeats come from two processes in turn, writes no alert and revokes nothing; the operator reads the flag and decides. It compares no source address, which is personal data.

**Effect on other sections.** §Context's requirements stand: point 5's instant revocation is met by item 5. §Enrollment: steps 1, 4 and 5 read as items 1 to 3 say; steps 2, 3 and 6 are unchanged. §Transport in operation: its third bullet reads as item 5; the handshake, TLS 1.3 alone and the cipher suites are unchanged. §Message integrity per batch: its validation order gains, after step 2, item 5's revocation check and item 4's comparison of certificates, and step 6's "known public key" is, for a request under a later certificate that item 4 makes the agent's, the key of that certificate; its other steps are unchanged, and the renewal request is signed as an envelope is. §Rotation and revocation: its four bullets read as items 4, 5 and 7, and the 90 days stand. §Heartbeat and degraded mode and §Version compatibility: unchanged. §Consequences > Negative: the CA stays the critical asset its first bullet names, now with no rotation (item 7). §Compliance asks that such a change open a superseding ADR: this one is made in place, as the three earlier ones were. *References*: ADR-0003 hosts the nonces, never a revocation list, which its entry, the catalog's dependency edge and ADR-0014's reference to this ADR had said. The Blueprint's §7 repeats the original text, and the last bullet of its *Rotation and revocation* points here. Proposed by Claude Code on 2026-11-20, on Manuel's choices of that day; awaiting his ratification. Status stays `Accepted`.

## References

- [ADR-0001](0001-monorepo-layout.md) — Monorepo layout
- [ADR-0002](0002-language-per-component.md) — Language per component (Rust for agent, Go for ingest)
- [ADR-0003](0003-polyglot-storage.md) — Polyglot storage (Redis hosts the anti-replay nonces; revocations live in PostgreSQL, by the Amendment 2026-11-20)
- Blueprint §7 — Agent-Server Secure Protocol
- Blueprint §8 — CGES (event payload schema, defined in [ADR-0006](0006-cges-ocsf-alignment.md))
