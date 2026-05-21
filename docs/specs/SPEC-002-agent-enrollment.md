# SPEC-002: Agent enrollment

- **ID:** SPEC-002
- **Title:** Agent enrollment
- **Status:** Accepted
- **Depends on:** ADR-0001, ADR-0002, ADR-0004 §Enrollment, ADR-0006, SPEC-001
- **Authors:** Manuel (product owner), Claude (architecture advisor), Claude Code (implementation)
- **Created:** 2026-05-21
- **Last updated:** 2026-05-21

## Motivation

SPEC-001 §Scope OUT explicitly declared: *"The SPEC-001 endpoint trusts whatever agent.id is asserted in the envelope. ... A future sub-SPEC (per ADR-0004 §Enrollment) replaces this with X.509-bound identity. SPEC-001 deployments are therefore limited to closed test environments."*

SPEC-002 closes that gap. After SPEC-002, the agent **has** cryptographically verifiable identity: an Ed25519 private key it generates locally on first run, and an X.509 client certificate issued by the server, bound to that key, identifying the agent by a server-assigned `agent_id` (UUIDv7).

After SPEC-002, the agent has identity. SPEC-003 will make the heartbeat path **use** that identity (mTLS 1.3 + signed envelope per ADR-0004). The two are intentionally separated so that the cryptographic-material lifecycle (this SPEC) can be reviewed and harnessed independently of the transport rewrite (next SPEC).

## Scope

### In scope

- First-run detection: agent starts with no persisted identity.
- Reading a single-use `enrollment_token` from `agent.toml`.
- Generating an Ed25519 keypair locally via the OS RNG.
- Building and POSTing an enrollment request (token + raw pubkey + agent metadata) to `/v1/agents/enroll` over plain HTTP.
- Receiving an enrollment response (server-assigned `agent_id` + X.509 client certificate PEM + issued / expires timestamps).
- Persisting the cert and private key on disk under `%ProgramData%\CyberGuard\agent\`, with the private key **encrypted at rest** via the mechanism chosen during ratification (see §Security considerations).
- Idempotent subsequent runs: if persisted identity is present, load it and skip enrollment.
- Failure handling: token rejected, token already used, server 5xx, network unreachable, persistence failure — each mapped to a documented exit code and stderr line.
- Integration harness with one Rust integration test per acceptance criterion.

### Out of scope (deferred)

- **mTLS 1.3 transport.** The heartbeat path remains plain HTTP through SPEC-002. Replaced in SPEC-003 per ADR-0004 §Transport.
- **Signed envelopes.** Heartbeats and enrollment requests are still unsigned bodies. Per-message Ed25519 signatures land in SPEC-003 per ADR-0004 §Message integrity.
- **Certificate rotation and renewal.** ADR-0004 §Rotation specifies auto-rotation from day 75 of a 90-day cert. SPEC-002 issues the cert; rotation is a dedicated future SPEC and is acknowledged as a non-trivial workstream (mid-flight key swap, dual-cert acceptance windows, server-side coordination).
- **Linux and macOS targets.** Windows-first per ADR-0002 Rule 2.
- **Buffered offline events.** Per ADR-0004 §Heartbeat and degraded mode; a separate future SPEC.
- **The server.** SPEC-002 tests use an in-process axum mock at `/v1/agents/enroll`. The production server endpoint is part of the API SPEC under `services/api/`.
- **Confidentiality of the enrollment request itself.** Enrollment runs over plain HTTP in SPEC-002, so the request body (including the enrollment token) is visible to any on-path observer. This is resolved by mTLS in SPEC-003 per ADR-0004 §Transport. Until then, SPEC-002 deployments are limited to closed test environments, consistent with the SPEC-001 trust posture.

## Functional requirements

- **FR-001.** On startup, after `LoadConfig` and `InitLogger` succeed, the agent inspects the configured `enrollment.cert_path` and `enrollment.key_path`. If **both** files exist and are readable, the agent is considered already enrolled and proceeds to `LoadIdentity`. If either is missing, the agent enters the `Enrolling` state.
- **FR-002.** The `Enrolling` state requires `enrollment.token` to be present in `agent.toml` and non-empty. If missing, the agent exits with code `2` and a stderr line containing `missing key 'enrollment.token'`. The token is consumed in memory only; it is not persisted.
- **FR-003.** During `Enrolling`, the agent generates a fresh Ed25519 keypair using `OsRng` (operating-system entropy source — `BCryptGenRandom` on Windows, `/dev/urandom` on Unix). The private key material lives in a `zeroize`-protected wrapper and is overwritten on drop.
- **FR-004.** The agent constructs an enrollment request envelope containing: the raw 32-byte public key (base64url-encoded, unpadded), the agent metadata (`agent.hostname`, `agent.platform`, `agent.version`), and the `enrollment_token`. The envelope shape is defined in §Data contracts.
- **FR-005.** The agent POSTs the enrollment request as JSON to `{server.url}/v1/agents/enroll` with a `Content-Type: application/json` header. The request timeout is `enrollment.timeout_seconds` (default 30 s).
- **FR-006.** On a `200 OK` response, the agent parses the body into an `EnrollmentResponse` containing the server-assigned `agent_id` (UUIDv7), the X.509 client certificate in PEM format, and the `issued_at` / `expires_at` ISO 8601 UTC timestamps. The agent validates that `agent_id` matches the UUIDv7 pattern and that the certificate parses as DER after base64-decoding the PEM body. Any mismatch is a fatal enrollment error (exit code `3`).
- **FR-007.** On enrollment success, the agent persists three artifacts under `%ProgramData%\CyberGuard\agent\` (or the configured directory): `cert.pem` (the certificate), `key.dat` (the private key encrypted with Windows DPAPI at **`CRYPTPROTECT_LOCAL_MACHINE`** scope — see §Security considerations), and `identity.json` (the server-assigned `agent_id`, `issued_at`, `expires_at`, and the public-key fingerprint for cross-check on load). All three files are written with owner-only ACLs on Windows. The `CRYPTPROTECT_LOCAL_MACHINE` scope ties the encrypted blob to the **machine**, not to a specific Windows user account — so a service-account change does not invalidate `key.dat`, but copying it to another host does.
- **FR-008.** On subsequent startups when persisted identity is present, the agent loads the cert (PEM → in-memory X.509), the encrypted private key (`key.dat` → decrypt → in-memory Ed25519 secret key, in a `zeroize`-protected wrapper), and `identity.json`. The pubkey derived from the loaded private key must equal the fingerprint recorded in `identity.json`; a mismatch is a fatal load error (exit code `5`).
- **FR-009.** Token rejection by the server (`401 Unauthorized` or `403 Forbidden`) results in: a `warn` log line `enrollment failed: token rejected`, exit code `3`, and a stderr line `cg-agent: enrollment failed: token rejected by server`. The agent does not retry. The enrollment token is single-use; retrying with the same token is meaningless.
- **FR-010.** Token already consumed (`409 Conflict`) results in the same exit code `3` and a distinct stderr line `cg-agent: enrollment failed: token already used`. Equivalent operator action (re-issue a fresh token).
- **FR-011.** Server 5xx and network unreachable conditions are retried with exponential backoff. Defaults: `enrollment.max_retries = 3`, `enrollment.backoff_initial_ms = 1000`, `enrollment.backoff_factor = 2.0`. After `max_retries` exhausted, the agent exits with code `4` and a stderr line `cg-agent: enrollment failed: server unreachable after N attempts`.
- **FR-012.** Persistence failure (any IO error writing `cert.pem`, `key.dat`, or `identity.json`, or failing to set ACLs) results in exit code `5` and a stderr line `cg-agent: enrollment failed: cannot persist identity: <error>`. The cert and key are **not** kept in memory only as a fallback: persistence is part of the success condition.
- **FR-013.** After successful enrollment **and** successful persistence, the agent transitions directly into the heartbeat loop (SPEC-001 `Heartbeating` state), using the persisted identity. The first heartbeat is sent within the 5 s budget from SPEC-001 §FR-004 measured from the entry into `Heartbeating`, **not** from process start.
- **FR-014.** After successful enrollment and persistence (and before entering `Heartbeating`), the agent hygienizes the on-disk token: it atomically rewrites `agent.toml` with the `enrollment.token` key removed (write to a temp file in the same directory, then rename over the original). This is **best-effort** — failure to rewrite is logged at `warn` (`could not hygienize enrollment token from config`) but does not abort the run, because the token is already single-use and consumed server-side. Defensively, on any startup where persisted identity is already present, a still-present `enrollment.token` in `agent.toml` is logged at `warn` (`stale enrollment token present in config, ignoring`) and ignored — the agent never re-enrolls while it holds a valid identity.

## Non-functional requirements

- **NFR-001.** Total enrollment latency (from request POST to identity persisted) must complete within `enrollment.timeout_seconds` (default 30 s) under nominal server latency.
- **NFR-002.** Private-key bytes in memory live exclusively inside `zeroize::Zeroizing<[u8; 32]>` or equivalent. The bytes must be overwritten with zeroes on drop, on enrollment failure, and on graceful shutdown. `cargo deny` or equivalent dependency review must flag any crate that copies key material out of a `Zeroizing` wrapper.
- **NFR-003.** Persisted artifacts (`cert.pem`, `key.dat`, `identity.json`) must have owner-only filesystem ACLs on Windows (read / write for the agent service account; no read for any other principal). Verification is part of AC-010.
- **NFR-004.** The agent crate compiles cleanly with `cargo fmt --check` and `cargo clippy -- -D warnings`, including the new modules and new dependencies.
- **NFR-005.** Enrollment must not silently fall back to running without persisted identity. Any failure to persist is a fatal exit (FR-012). The product invariant is: a running agent always has on-disk identity.

## Data contracts

The enrollment request and response envelopes are **transport-level meta-messages**, not CGES events. They reuse `schemas/cges/v0.1/common/cg_agent.json` only conceptually (the `agent_hostname`, `agent_platform`, `agent_version` fields match its shape); they do not introduce a new CGES schema, following the same reasoning as SPEC-001 §Data contracts.

**Drift from ADR-0004 §Enrollment step 4 (declared, scoped):** ADR-0004 prescribes a *"Certificate Signing Request"* (PKCS#10 CSR). SPEC-002 instead sends the raw 32-byte Ed25519 public key (base64url-encoded) plus the agent metadata. The X.509 certificate the server issues is identical in either case; the difference is operational. A PKCS#10 CSR carries: subject info, optional extensions, and a self-signature proving knowledge of the corresponding private key. In SPEC-002 the enrollment token already proves authorisation (single-use, scope-bound, server-signed JWT), so the CSR self-signature is redundant. Choosing raw pubkey trims the agent dependency surface (no `rcgen` or PKCS#10 crate needed) and keeps the harness easier to read. When SPEC-003 (rotation) lands, the rotation request *does* sign over the new pubkey using the **existing** private key (which is the security property the CSR self-signature would otherwise provide) — so this drift does not weaken any downstream invariant.

### Enrollment request envelope

```json
{
  "envelope_version": "0.1.0",
  "enrollment_token": "<opaque base64url string, server-issued>",
  "agent_pubkey": "<base64url-unpadded 32-byte Ed25519 pubkey>",
  "agent_hostname": "FIN-PC-014",
  "agent_platform": "windows",
  "agent_version": "0.1.0"
}
```

- `envelope_version` — string constant `"0.1.0"`. Independent of the SPEC-001 heartbeat envelope version.
- `enrollment_token` — opaque string. The agent does not parse it. Internally the server may use a signed JWT (per ADR-0004 §Enrollment), but that structure is server-side concern.
- `agent_pubkey` — base64url-encoded, no padding, exactly 32 bytes when decoded.
- `agent_hostname`, `agent_platform`, `agent_version` — same shape as `common/cg_agent.json`.

### Enrollment response envelope (success, HTTP 200)

```json
{
  "envelope_version": "0.1.0",
  "agent_id": "01934abc-def0-7000-89ab-000000000001",
  "client_certificate": "-----BEGIN CERTIFICATE-----\nMIIB...\n-----END CERTIFICATE-----\n",
  "issued_at": "2026-05-21T10:23:11.482Z",
  "expires_at": "2026-08-19T10:23:11.482Z"
}
```

- `agent_id` — server-assigned UUIDv7. From this point the canonical identity of the agent.
- `client_certificate` — PEM-encoded X.509. The cert's subject `CN = agent_id` and (if multi-tenancy is enabled) `SAN = org_id`, per ADR-0004 §Enrollment step 5.
- `issued_at`, `expires_at` — ISO 8601 UTC with milliseconds. Default cert TTL is 90 days per ADR-0004.

### Persisted identity file (`identity.json`)

```json
{
  "agent_id": "01934abc-def0-7000-89ab-000000000001",
  "agent_pubkey_fingerprint": "<sha256 hex of the 32-byte pubkey>",
  "issued_at": "2026-05-21T10:23:11.482Z",
  "expires_at": "2026-08-19T10:23:11.482Z"
}
```

The fingerprint is a tamper-evidence cross-check on identity load: if `key.dat` decrypts to a key whose pubkey does not match this fingerprint, the load fails (FR-008). It is **not** a cryptographic defense — `identity.json` is not signed, so an attacker who can write to it can also recompute a matching fingerprint. The real defenses on the identity artifacts are DPAPI at-rest encryption of `key.dat` (machine scope) and the owner-only filesystem ACLs on all three files (NFR-003). The fingerprint exists to catch accidental corruption and key/identity desynchronisation (e.g. a half-completed re-enrollment), not to resist a privileged local attacker.

## Configuration

### New keys in `agent.toml`

```toml
[enrollment]
token = "<single-use server-issued token>"   # required on first run only
cert_path = "C:/ProgramData/CyberGuard/agent/cert.pem"
key_path = "C:/ProgramData/CyberGuard/agent/key.dat"
identity_path = "C:/ProgramData/CyberGuard/agent/identity.json"
timeout_seconds = 30
max_retries = 3
backoff_initial_ms = 1000
backoff_factor = 2.0
```

### Changes to existing keys

- `agent.id` (SPEC-001) becomes **optional**. Its semantics change: if persisted identity is present, `agent.id` is ignored. If persisted identity is absent (first run), `agent.id` is **not consulted** either — the server assigns the canonical id during enrollment. The field remains in the schema for backward-compat with SPEC-001 test configs but is now informational only.
- `agent.hostname` and `agent.platform` (the latter detected by the agent at compile time, but configurable for portability) are still required because they appear in the enrollment request.

### Default paths

`%ProgramData%\CyberGuard\agent\` is the Windows default. Rationale:

- `%ProgramData%` is per-machine, not per-user. The agent typically runs as a Windows service under a dedicated service account; the directory must be readable / writable by that account regardless of the interactive user.
- The path is ACL-protectable: `icacls` can grant exclusive access to the service principal.
- It survives user-profile resets.

The path is overridable in `agent.toml` for tests and for non-default deployments.

## Behavior

State machine — two startup paths:

```text
Startup
  └─► LoadConfig
        ├─[invalid]──► ExitConfigError (code 2)
        └─[valid]
              └─► InitLogger
                    └─► CheckIdentity (FR-001)
                          ├─[identity files present]──► LoadIdentity
                          │                                  │
                          │                                  ▼
                          │                              Heartbeating (SPEC-001 path)
                          │
                          └─[identity files absent]──► Enrolling
                                                          │
                                                  ┌───────┼───────┐
                                                  │       │       │
                                          token rejected  │  server 5xx /
                                          (FR-009/010)    │  network (FR-011)
                                                  │       │       │
                                                  ▼       ▼       ▼
                                              Exit(3)   PersistIdentity   Exit(4)
                                                          │
                                                          ├─[persist fails]──► Exit(5)
                                                          └─[persist OK]
                                                                │
                                                                ▼
                                                          Heartbeating (SPEC-001 path)
```

- **CheckIdentity** — inspects both `cert_path` and `key_path`. Both present and readable ⇒ `LoadIdentity`. Either missing ⇒ `Enrolling`. The check does not validate the cert; that happens in `LoadIdentity`.
- **LoadIdentity** — reads `cert.pem`, `key.dat`, and `identity.json`. Decrypts `key.dat` via the chosen mechanism (see §Security considerations). Cross-checks the derived public key against `agent_pubkey_fingerprint`. Failure ⇒ exit `5`. Success ⇒ `Heartbeating` with the loaded identity.
- **Enrolling** — generates keypair, builds request, POSTs to `/v1/agents/enroll`, applies retry policy on 5xx and network errors only. 4xx errors (`401`, `403`, `409`) are terminal.
- **PersistIdentity** — writes `cert.pem`, `key.dat` (encrypted), `identity.json`, and applies owner-only ACLs. Any IO failure ⇒ exit `5`.
- **Heartbeating** — SPEC-001 path, unchanged. The heartbeat envelope continues to carry the `agent_id` field, which now comes from the persisted identity instead of `agent.toml`. The first heartbeat after enrollment fires within the 5 s budget from SPEC-001 §FR-004 measured from entry into `Heartbeating`.

## Failure modes

| Failure | Detection | Exit code | Stderr line |
|---|---|---|---|
| Config missing required key (incl. `enrollment.token` on first run) | Config validation at startup | `2` | `cg-agent: invalid config: missing key '<path>'` |
| Server rejects token (`401` / `403`) | HTTP response in `Enrolling` | `3` | `cg-agent: enrollment failed: token rejected by server` |
| Token already used (`409`) | HTTP response in `Enrolling` | `3` | `cg-agent: enrollment failed: token already used` |
| Malformed enrollment response (bad JSON, bad PEM, bad UUIDv7) | Parse in `Enrolling` | `3` | `cg-agent: enrollment failed: malformed server response` |
| Server 5xx after `max_retries` | HTTP response in `Enrolling` | `4` | `cg-agent: enrollment failed: server unreachable after N attempts` |
| Network unreachable after `max_retries` | reqwest error in `Enrolling` | `4` | `cg-agent: enrollment failed: server unreachable after N attempts` |
| Cannot persist `cert.pem` / `key.dat` / `identity.json` | IO error in `PersistIdentity` | `5` | `cg-agent: enrollment failed: cannot persist identity: <error>` |
| ACL application fails on Windows | Win32 error in `PersistIdentity` | `5` | Same as above |
| Loaded private key does not match stored fingerprint | Cross-check in `LoadIdentity` | `5` | `cg-agent: identity load failed: pubkey fingerprint mismatch` |
| Encrypted `key.dat` cannot be decrypted (DPAPI rejection — `key.dat` was copied from a different machine) | Crypto error in `LoadIdentity` | `5` | `cg-agent: identity load failed: cannot decrypt private key` |
| `identity.json` missing while `cert.pem` and `key.dat` are present | `LoadIdentity` | `5` | `cg-agent: identity load failed: identity.json missing` |
| `identity.json` present but malformed (bad JSON, missing fields) | Parse in `LoadIdentity` | `5` | `cg-agent: identity load failed: identity.json corrupted` |
| `enrollment.token` present but no `[enrollment]` table | Config validation at startup | `2` | (same as missing key 2) |

Exit codes from SPEC-001 retained: `0` success, `1` runtime error, `2` config error. Added by SPEC-002: `3` enrollment refused by server (terminal), `4` enrollment exhausted retries against the server, `5` identity persistence or load failure.

## Observability

All logs use the SPEC-001 conventions (one JSON line per stdout, tracing-subscriber JSON formatter). New lifecycle events:

| Event | Level | Required fields |
|---|---|---|
| Enrollment starting | `info` | `agent.hostname`, `server.url`, `enrollment.timeout_seconds` |
| Keypair generated | `info` | `agent_pubkey_fingerprint` (sha256 of pubkey, hex) |
| Enrollment request sent | `info` | `attempt`, `server.url` |
| Enrollment retry | `warn` | `attempt`, `backoff_ms`, `error` |
| Enrollment response received | `info` | `agent_id`, `expires_at` |
| Identity persisted | `info` | `cert_path`, `key_path`, `identity_path` |
| Enrollment failed (token rejected) | `warn` | `response_status` |
| Enrollment failed (token already used) | `warn` | `response_status` |
| Enrollment failed (server unreachable) | `warn` | `attempts`, `last_error` |
| Identity loaded from disk | `info` | `agent_id`, `expires_at` |
| Identity load failed | `error` | `reason` |

The pubkey fingerprint is the hex-encoded sha256 of the 32-byte raw public key. The private key value is never logged at any level. The enrollment token is never logged at any level (it is treated as a credential).

## Acceptance criteria

Each AC maps 1:1 to one Rust integration test under `agent/cg-agent/tests/`. Test names mirror AC IDs.

- **AC-001.** Given an `agent.toml` with a valid `enrollment.token` and no persisted identity at the configured paths, the agent enrolls successfully against a mock at `/v1/agents/enroll` and transitions into the heartbeat loop within the timeout budget.
- **AC-002.** The keypair survives the full gen → send → persist → load chain. The 32-byte public key generated on first run equals all of: (a) the `agent_pubkey` decoded from the enrollment request the mock received; (b) the public key re-derived from the private key after it is persisted to `key.dat` and reloaded on a second run; and (c) the `agent_pubkey_fingerprint` recorded in `identity.json`. The property under test is end-to-end identity continuity, not just key validity at generation time.
- **AC-003.** The enrollment request received by the mock contains: a non-empty `enrollment_token` matching the configured value, a base64url-decoded `agent_pubkey` of exactly 32 bytes, the configured `agent_hostname`, the runtime `agent_platform`, and the crate `agent_version`.
- **AC-004.** On a `200 OK` response carrying a fresh `agent_id`, a valid PEM cert, and ISO 8601 `issued_at` / `expires_at`, the agent writes three artifacts: `cert.pem`, `key.dat`, `identity.json`. All three files exist on disk after enrollment.
- **AC-005.** On a second run with the three identity files present, the agent does **not** POST to `/v1/agents/enroll`, loads the existing identity, and proceeds directly to the heartbeat loop.
- **AC-006.** When the mock returns `401`, the agent exits with code `3` and stderr contains the substring `token rejected by server`. No identity files are written.
- **AC-007.** When the mock returns `409`, the agent exits with code `3` and stderr contains `token already used`. No identity files are written.
- **AC-008.** When the mock returns `500` for every retry (within `max_retries`), the agent exits with code `4` and stderr contains the substring `server unreachable after`. No identity files are written.
- **AC-009.** When the mock is unreachable at the network layer for every retry, the agent exits with code `4` and the same `server unreachable after` substring. No identity files are written.
- **AC-010.** *(Windows)* After a successful enrollment, the persisted `cert.pem`, `key.dat`, and `identity.json` have filesystem ACLs that exclude all principals other than the owner and `SYSTEM`. Verified via the Windows ACL APIs. Test is `#[cfg(windows)]`.
- **AC-011.** After a successful first run, `agent.toml` no longer contains the `enrollment.token` key (FR-014 hygiene). A second run started with the hygienized config and the persisted identity present proceeds without enrollment and without error.
- **AC-012.** *(POSIX, parked)* On non-Windows platforms the persisted artifacts have mode `0600`. The test is `#[cfg(unix)]` and is parked for the SPEC-003 Linux work; it asserts the mode when run on a POSIX host but is not part of the Windows-first MVP gate.

## Security considerations

This is the first SPEC in the project that handles secret material. The template established here applies to future secret-handling SPECs.

### Key generation entropy

The Ed25519 keypair is generated with `OsRng` from the `rand_core` ecosystem, which delegates to `getrandom`, which in turn calls `BCryptGenRandom` on Windows (the kernel's CSPRNG) and `/dev/urandom` on Unix. The agent never uses thread-local non-cryptographic RNGs for any key-relevant code path.

### Key storage at rest

The private key is encrypted with Windows DPAPI at **`CRYPTPROTECT_LOCAL_MACHINE`** scope (machine scope, not per-user) and persisted in `key.dat`. The encrypted blob can be decrypted only on the machine that performed the encryption, by any account on that machine with read access to `key.dat` — which is why the owner-only ACL on `key.dat` (NFR-003) is the access-control half of the defense, and DPAPI is the at-rest-confidentiality half.

Trade-off explicitly: machine scope (rather than machine + user scope) was chosen so that a change of the agent's service account does **not** invalidate the stored key — operationally important because services are routinely re-homed to managed service accounts after install. A copy of `key.dat` exfiltrated to another host is useless to the attacker (the DPAPI master key never leaves the origin machine). The remaining downside is that a machine reimage breaks the binding — the agent has to re-enroll. ADR-0004 §Rotation already anticipates re-enrollment as a recovery path, and SPEC-002 §FR-008 surfaces a clear failure (exit code `5`) when `key.dat` cannot be decrypted, prompting operator re-enrollment with a fresh token.

Because the scope is machine-wide, the access-control boundary against other local accounts is the ACL on `key.dat`, not DPAPI. An attacker who is already an administrator on the same machine can both read `key.dat` and invoke DPAPI unprotect — that is an accepted limitation (an administrator on the endpoint is outside the SPEC-002 threat boundary, consistent with the threat model's "root-level local compromise" accepted threat).

### Enrollment token confidentiality and lifetime

The enrollment token is single-use, server-side enforced. ADR-0004 §Enrollment specifies a TTL of 15 minutes. The agent does not parse the token; it is treated as opaque.

**Pending Manuel's ratification.** The proposal is plain TOML in `agent.toml` for SPEC-002. The token's lifetime on disk is bounded by the time between the installer writing `agent.toml` and the agent's first successful enrollment (typically seconds). Once enrollment succeeds, the agent **does not** wipe the token from `agent.toml` automatically — that is operator responsibility documented in the installation runbook. An installer that hardens the deployment can wipe the token after the agent process exits with code `0` on first run.

Trade-off: plain TOML means a privileged-at-rest snapshot of the endpoint during the window between install and first run leaks a one-shot enrollment capability. Alternative: store the token in an environment variable read by the installer, never on disk. This is a one-line change in SPEC-002 and zero change in the implementation, but it complicates the operator UX.

### Replay protection on enrollment requests

The enrollment request itself is not signed (the agent does not yet have an identity to sign with — that is the chicken-and-egg this SPEC resolves). Replay protection therefore lives entirely server-side: the token is single-use, the server consumes it on first use, and replays of the same request body get `409 Conflict`. Beyond that, an attacker who has captured a token-carrying enrollment request can only enrol *a different* pubkey under that token before the legitimate agent does — they cannot impersonate a legitimate enrollment that has already succeeded.

**Race-and-replay in transit (plain HTTP, SPEC-002).** Because SPEC-002 enrollment runs over plain HTTP (mTLS deferred to SPEC-003), an on-path attacker can observe an enrollment request and attempt to **win the race**: replay the captured body with the attacker's own `agent_pubkey` substituted, reaching the server before the legitimate agent's request lands. The server consumes the single-use token for whichever request arrives first; if the attacker wins, the legitimate agent gets `409` and the attacker holds a valid certificate. The token's confidentiality on the wire is therefore a real gap in SPEC-002 — explicitly listed in §Scope OUT ("Confidentiality of the enrollment request itself") and resolved by mTLS 1.3 in SPEC-003, which makes the request body unobservable and unmodifiable in transit. Until SPEC-003, enrollment must occur in a closed/trusted network segment.

### What a compromised enrollment token grants

If an attacker obtains a still-valid enrollment token (e.g. by reading the installer's intermediate state on the endpoint), they can:

- Enroll a fake agent under the bound `org_id` with their own pubkey. The fake agent receives a real certificate and can pose as a CyberGuard agent for the cert's lifetime (default 90 days).

They **cannot**:

- Impersonate a specific existing agent (the cert's `CN` is a new server-assigned `agent_id`, not the attacker's choice).
- Decrypt or forge events from other agents (each agent has its own keypair).
- Sign retroactively for events that occurred before their enrollment (subsequent SPECs add per-message Ed25519 signatures bound to the agent's private key; the attacker's fake agent signs with its own key).

Mitigations (already part of ADR-0004): TTL 15 min, single-use, scope-bound JWT. SPEC-002 honours all three on the agent side by treating the token as opaque and not retrying on 4xx responses.

## Ratification record

The three decisions originally surfaced for Manuel's call were ratified in the Session 6 review:

1. **Key-at-rest mechanism — DPAPI at `CRYPTPROTECT_LOCAL_MACHINE` scope.** Machine scope, not per-user, so a service-account change does not invalidate the stored key. Detailed in §Security considerations > Key storage at rest; reflected in FR-007 and the §Failure modes decryption row.
2. **Key rotation — deferred entirely** to a dedicated future SPEC. SPEC-002 issues 90-day certs and does not implement renewal; re-enrollment is the recovery path until the rotation SPEC lands. Listed in §Scope OUT.
3. **`enrollment_token` storage — plain TOML in `agent.toml`**, with mandatory post-enrollment hygiene: the agent atomically rewrites `agent.toml` to drop the token after a successful first run (FR-014), and ignores a stale token when an identity is already present. The on-disk exposure window is bounded to the time between install and first successful enrollment.

## References

- [ADR-0001](../adr/0001-monorepo-layout.md) — Monorepo layout (`agent/` location, `docs/specs/` path).
- [ADR-0002](../adr/0002-language-per-component.md) — Language per component (Rust for agent; `services/ml/` Python boundary, irrelevant here).
- [ADR-0004](../adr/0004-agent-server-protocol.md) — Agent-Server secure protocol. SPEC-002 implements the §Enrollment subset only; §Transport (mTLS) and §Message integrity (signed envelopes) are deferred to SPEC-003.
- [ADR-0006](../adr/0006-cges-ocsf-alignment.md) — CGES alignment. SPEC-002 reuses `common/cg_agent.json` conceptually but introduces no CGES class (enrollment envelopes are transport-level meta-messages, same as SPEC-001 heartbeat).
- [SPEC-001](SPEC-001-agent-heartbeat.md) — Agent heartbeat. SPEC-002 augments the startup path with a `CheckIdentity` branch and reuses the `Heartbeating` state unchanged.
- [Foundational Blueprint](../product/blueprint.md) — §7 (Agent-Server Secure Protocol).
