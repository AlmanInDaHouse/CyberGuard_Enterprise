# SPEC-001: Agent heartbeat

- **ID:** SPEC-001
- **Title:** Agent heartbeat
- **Status:** Accepted
- **Depends on:** ADR-0001, ADR-0002, ADR-0004 (partial; see §Scope), ADR-0006
- **Authors:** Manuel (product owner), Claude (architecture advisor), Claude Code (implementation)
- **Created:** 2026-05-20
- **Last updated:** 2026-05-20

## Motivation

The CyberGuard Agent must signal liveness to the server so the operator can see which endpoints are online and react to silent failures. This SPEC establishes the first heartbeat path end-to-end: configuration, transport, structured logging, graceful shutdown, and harness-validated acceptance criteria.

It is also the **pattern-setting SPEC** for the project. Future Rust modules, future test harnesses, and future SPECs inherit the conventions established here:

- SPEC structure (header, scope, FR/NFR, data contracts, configuration, behavior, failure modes, observability, ACs).
- Harness-first development (acceptance criteria become tests before code).
- Toolchain pinning and dependency justification.
- Module split inside the Rust crate.

## Scope

### In scope

- A `cg-agent` Rust binary on Windows.
- Configuration via a TOML file (`agent.toml`).
- Plain **HTTP** transport against a single configured server URL.
- A heartbeat envelope sent on a configurable interval (default 30 s).
- Monotonic `sequence_number` increment across heartbeats within a single agent lifetime.
- Structured JSON logs to stdout.
- Graceful shutdown on `SIGINT` / `Ctrl+C` with a final heartbeat carrying `status = "going_offline"`.
- Retry with exponential backoff on transient transport failures.
- A harness with one Rust integration test per acceptance criterion, run by `cargo test`.

### Out of scope (deferred)

- **mTLS 1.3 + Ed25519 signed envelope per ADR-0004 §Transport / §Message integrity.** This SPEC implements a deliberate HTTP-only subset of the agent-server boundary. A future sub-SPEC will replace the SPEC-001 envelope with the full ADR-0004 envelope carrying `events = []`. SPEC-001 envelope is transitional.
- Enrollment flow (JWT enrollment token, CSR, X.509 client cert issuance) — per ADR-0004 §Enrollment, future SPEC.
- Linux and macOS targets — per ADR-0002 Rule 2, Windows-first.
- Buffered offline mode with disk-encrypted local store — per ADR-0004 §Heartbeat and degraded mode, future SPEC.
- Telemetry events of any kind (process, network, file, auth). This SPEC carries no `events[]` payload; the heartbeat is liveness-only.
- Server implementation. SPEC-001 tests use a tiny in-process mock; the production server (TypeScript + Fastify per ADR-0002) is a separate SPEC.
- Authentication of any kind. The SPEC-001 endpoint trusts whatever `agent.id` is asserted in the envelope. The `agent.id` in `agent.toml` is operator-provisioned for the lifetime of SPEC-001 and is not cryptographically verifiable. A future sub-SPEC (per ADR-0004 §Enrollment) replaces this with X.509-bound identity. SPEC-001 deployments are therefore limited to closed test environments.

## Functional requirements

- **FR-001.** The agent reads its configuration from a TOML file whose path is passed via `--config <path>` on the command line. If `--config` is absent, the agent looks for `agent.toml` in the current working directory.
- **FR-002.** Required configuration keys are: `server.url` (string, base URL of the heartbeat endpoint), `agent.id` (UUIDv7 string), `agent.hostname` (non-empty string). Missing required keys cause the agent to exit with a non-zero code and a clear error message before any network activity.
- **FR-003.** Optional configuration keys with defaults are: `heartbeat.interval_seconds` (default `30`), `heartbeat.request_timeout_seconds` (default `10`), `heartbeat.max_retries` (default `3`), `heartbeat.backoff_initial_ms` (default `1000`), `heartbeat.backoff_factor` (default `2.0`), `heartbeat.backoff_max_ms` (default `60000`), `log.level` (default `"info"`).
- **FR-004.** The agent sends an initial heartbeat **within 5 seconds of process start** after configuration is validated. This first heartbeat carries `sequence_number = 1`.
- **FR-005.** After the initial heartbeat, the agent sends a heartbeat every `heartbeat.interval_seconds` seconds. Each subsequent heartbeat increments `sequence_number` by exactly 1. A new `sequence_number` is assigned at each scheduling tick, regardless of whether the previous heartbeat was successfully delivered. Sequence gaps observed at the server therefore indicate undelivered heartbeats, not retries.
- **FR-006.** The heartbeat envelope is POSTed as JSON to `{server.url}/v1/agents/heartbeat`. The endpoint is fixed in SPEC-001; routing is not configurable.
- **FR-007.** On transport failure (network error, non-2xx HTTP response, request timeout), the agent retries with exponential backoff: `backoff_initial_ms` × `backoff_factor^n`, capped at `backoff_max_ms`, up to `max_retries` attempts. The `sequence_number` does not increment across retries of the same heartbeat — only one logical heartbeat per interval.
- **FR-008.** On `SIGINT` / `Ctrl+C`, the agent ceases scheduling new heartbeats, sends a final heartbeat with `status = "going_offline"`, and exits with code `0`. The final heartbeat is best-effort: a single attempt with the normal request timeout, no retries.
- **FR-009.** All log output goes to stdout as one JSON object per line. No log lines on stderr (stderr is reserved for fatal pre-logging-init errors only).
- **FR-010.** The agent emits a log entry at level `info` for each heartbeat sent (regardless of success) and at level `warn` for each transport failure, with the failing endpoint, attempt number, and error reason included as structured fields.
- **FR-011.** Heartbeats are scheduled on an absolute timeline anchored at `start_time` (the agent's UTC clock at the moment the configuration validates). The N-th heartbeat (1-indexed) is scheduled at `start_time + (N − 1) × interval_seconds`. Failures or retries of an earlier heartbeat do not shift the schedule of subsequent heartbeats.

## Non-functional requirements

- **NFR-001.** The compiled binary on Windows must run with under **30 MB resident memory** at steady state (well below the 50 MB threshold cited in Blueprint §5).
- **NFR-002.** Heartbeat scheduling jitter must not exceed **±500 ms** relative to the configured interval under normal load.
- **NFR-003.** The agent emits one log line per heartbeat (success or failure) plus one line per retry attempt; this is a hard cap to keep stdout volume predictable for the future log shipper.
- **NFR-004.** The binary is single-file, no external runtime dependency beyond the OS. (Rust statically links libstd; reqwest's rustls backend embeds the TLS stack.)
- **NFR-005.** Source code passes `cargo fmt --check` and `cargo clippy -- -D warnings` in CI.

## Data contracts

The heartbeat envelope is a **transport-level meta-message**, not a CGES event. The deliberate decision (recorded here, not in CGES schemas) is:

- The envelope **reuses `schemas/cges/v0.1/common/cg_agent.json`** as a sub-object to carry agent identity. The schema fits exactly: `agent_id`, `agent_version`, `agent_platform`, `agent_hostname`.
- The envelope does **not** introduce a new CGES class, because heartbeats are not OCSF events; they are agent → server liveness signals. ADR-0006 §Compliance governs CGES extensions for events, not transport meta-messages.
- When ADR-0004 (mTLS + Ed25519 signed envelope) is materialised in a future SPEC, this SPEC-001 envelope is replaced by the ADR-0004 envelope carrying `events = []`. The SPEC-001 envelope is **transitional** by design.

### Heartbeat envelope shape (JSON)

```json
{
  "envelope_version": "0.1.0",
  "agent": {
    "agent_id": "01934abc-def0-7000-89ab-000000000001",
    "agent_version": "0.1.0",
    "agent_platform": "windows",
    "agent_hostname": "FIN-PC-014"
  },
  "sequence_number": 1,
  "sent_at": "2026-05-20T10:23:10.901Z",
  "status": "online",
  "uptime_seconds": 0
}
```

- `envelope_version` — string constant `"0.1.0"` for SPEC-001. Bumps when the envelope shape changes.
- `agent` — matches `schemas/cges/v0.1/common/cg_agent.json` (additionalProperties: false on that schema; the four fields are mandatory).
- `sequence_number` — integer, ≥ 1, monotonically increasing within one agent lifetime. Resets to `1` on agent restart.
- `sent_at` — RFC 3339 / ISO 8601 UTC with milliseconds.
- `status` — enum: `"online"` for regular heartbeats, `"going_offline"` for the final heartbeat at graceful shutdown.
- `uptime_seconds` — integer, seconds since `start` (process startup), measured by the agent's monotonic clock.

A standalone JSON Schema for this envelope is **not** added to `schemas/cges/v0.1/` because the envelope is not a CGES event. A pure-language type (Rust `struct` with serde derives) is the canonical shape. Cross-language reproducibility comes from the table above plus the test fixtures.

## Configuration

### File format

`agent.toml` (TOML, edition v1):

```toml
[server]
url = "http://localhost:8080"

[agent]
id = "01934abc-def0-7000-89ab-000000000001"
hostname = "FIN-PC-014"

[heartbeat]
interval_seconds = 30
request_timeout_seconds = 10
max_retries = 3
backoff_initial_ms = 1000
backoff_factor = 2.0
backoff_max_ms = 60000

[log]
level = "info"
```

### Validation

- Required keys absent → exit with code `2` and a stderr line `cg-agent: invalid config: missing key '<path>'`.
- `server.url` not parseable as a URL → exit with code `2` and a stderr line `cg-agent: invalid config: server.url not a valid URL`.
- `agent.id` not a UUIDv7 (pattern `^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$`) → exit with code `2` and a stderr line `cg-agent: invalid config: agent.id not a UUIDv7`.
- All numeric heartbeat fields validate `> 0`; `backoff_factor` ≥ `1.0`.

## Behavior

State machine:

```text
Startup
  └─► LoadConfig
        ├─[invalid]──► ExitConfigError (code 2)
        └─[valid]
              └─► InitLogger
                    └─► Heartbeating ──┐
                              ▲        │ every interval
                              └────────┘ (tick N: send heartbeat
                                          sequence_number = N, schedule
                                          tick N+1 at start_time + N · interval)
                                  │
                                  │ on SIGINT
                                  ▼
                          ShuttingDown
                          (final heartbeat, status =
                           "going_offline", single attempt)
                                  │
                                  ▼
                              Exit (code 0)
```

- **LoadConfig** — read TOML, validate, populate the in-memory config struct. Record `start_time` (UTC) immediately after validation succeeds; this is the schedule anchor of FR-011.
- **InitLogger** — initialize tracing-subscriber with JSON formatter at the configured level. After this point, no logs go to stderr except the final panic message (if any).
- **Heartbeating** — single state covering the entire heartbeat lifecycle. Tick `N` (1-indexed, starting at 1) fires at `start_time + (N − 1) × interval_seconds`. Each tick assigns `sequence_number = N`, attempts the heartbeat with the retry policy (FR-007), and unconditionally schedules tick `N + 1`. Retry failures of tick `N` do not delay tick `N + 1`.
- **ShuttingDown** — triggered by `tokio::signal::ctrl_c()`. Cancel the scheduler, send the final heartbeat (`status = "going_offline"`, one attempt only, normal timeout), exit `0` regardless of the final heartbeat's success.

## Failure modes

| Failure | Detection | Behavior |
|---|---|---|
| Server unreachable (DNS, connection refused, timeout) | reqwest error / status code | Retry per FR-007. Log each attempt at `warn`. On final failure of an interval's heartbeat, log at `warn` and wait for the next interval. Do not exit. |
| Malformed config (missing key, bad type, bad URL, bad UUIDv7) | Config validation at startup | Print error to stderr, exit code `2`. No log line on stdout (logger not initialised yet). |
| Clock skew (client clock vastly off from server) | Out of scope for SPEC-001 | Not detected by the agent. ADR-0004 §Server validation order will reject envelopes with `timestamp` outside ±5 min; SPEC-001 does not implement that check (no signed envelope yet). |
| Network blip mid-request | reqwest timeout / IO error | Same as "server unreachable". |
| OS shutdown / `SIGTERM` | `tokio::signal` (on Windows, `Ctrl+C` and `Ctrl+Break` map to `ctrl_c()`) | Same as `SIGINT`: ShuttingDown path. |
| Agent process killed (`kill -9`, task manager force-end) | N/A (no graceful shutdown opportunity) | No final heartbeat. Server-side will mark the agent offline after 3 missed heartbeats per ADR-0004 §Heartbeat. |
| Configuration reload at runtime | Out of scope for SPEC-001 | Restart the agent to apply config changes. |
| Disk full during log emission | tracing-subscriber drops the line | Acceptable in SPEC-001. Log rotation / forwarding is a deployment concern, not a SPEC-001 concern. |

## Observability

- **Format:** one JSON object per line on stdout, produced by `tracing-subscriber` with the JSON formatter.
- **Mandatory fields per line:** `timestamp` (RFC 3339 UTC with ms), `level`, `target` (Rust module path), `message`, plus event-specific structured fields.
- **Levels in use:** `info` (normal lifecycle: startup, heartbeat sent, graceful shutdown), `warn` (transport failures, retries), `error` (config errors, panics before they abort the process). `debug` and `trace` available but not emitted under the default `log.level = "info"`.

### Lifecycle log events

| Event | Level | Required fields |
|---|---|---|
| Agent starting | `info` | `agent.id`, `agent.version`, `agent.platform`, `agent.hostname`, `server.url`, `heartbeat.interval_seconds` |
| Heartbeat sent (success) | `info` | `sequence_number`, `status`, `sent_at`, `response_status`, `duration_ms` |
| Heartbeat retry | `warn` | `sequence_number`, `attempt`, `backoff_ms`, `error` |
| Heartbeat failed after retries | `warn` | `sequence_number`, `attempts`, `error` |
| Shutdown signal received | `info` | `signal` |
| Final heartbeat sent | `info` | `sequence_number`, `status`, `response_status`, `duration_ms` |
| Final heartbeat failed | `warn` | `sequence_number`, `error` |
| Agent stopping | `info` | `uptime_seconds` |

## Acceptance criteria

Each AC maps 1:1 to one Rust integration test under `agent/cg-agent/tests/`. Test names mirror the AC ID (e.g. `ac_001_reads_valid_config`).

- **AC-001.** Given a valid `agent.toml`, the agent starts, validates config, and reaches the Heartbeating state without error.
- **AC-002.** Given a valid config and a reachable mock server, the agent sends its first heartbeat within 5 seconds of process start.
- **AC-003.** Given `heartbeat.interval_seconds = 1` (test override) and `start_time` recorded at process startup, the agent sends 3 heartbeats whose `sent_at` timestamps each fall within ±500 ms of `start_time + (N − 1) × 1 s` for N ∈ {1, 2, 3}. (Anchor-relative drift bound from NFR-002, applied to the test scenario.)
- **AC-004.** Heartbeat envelopes received by the mock server have `sequence_number` values that are monotonically increasing by exactly 1, starting at 1.
- **AC-005.** When the mock server returns HTTP 503 on the first 2 attempts of a heartbeat and HTTP 200 on the 3rd, the agent eventually delivers that heartbeat and the next interval still ticks. The retry attempts back off by `backoff_initial_ms × backoff_factor^n`.
- **AC-006.** When the mock server is unreachable for the entire `max_retries` window of a heartbeat, the agent emits a `warn` log line `heartbeat failed after retries` and continues to the next interval (does NOT exit).
- **AC-007.** When the agent receives `SIGINT` / `Ctrl+C` while in Heartbeating, it sends one final heartbeat with `status = "going_offline"` and exits with code `0` within 2 seconds of the signal.
- **AC-008.** Given a config file missing the `server.url` key, the agent exits with code `2` and writes a stderr line containing the substring `missing key 'server.url'`.
- **AC-009.** All stdout lines emitted by the agent during a normal run are valid JSON, each containing `timestamp`, `level`, and `message` fields.
- **AC-010.** When the mock server rejects every retry of heartbeat N (so heartbeat N is never accepted), the next scheduling tick still arrives and the next received heartbeat carries `sequence_number = N + 1`. Sequence gaps observed at the server indicate undelivered heartbeats, not retries.

## References

- [ADR-0001](../adr/0001-monorepo-layout.md) — Monorepo layout (`agent/` location, `docs/specs/` path).
- [ADR-0002](../adr/0002-language-per-component.md) — Rust for the agent (Rule 1, justification table).
- [ADR-0004](../adr/0004-agent-server-protocol.md) — Full agent-server protocol. SPEC-001 implements only the HTTP transport subset; mTLS + Ed25519 are deferred to a sub-SPEC.
- [ADR-0006](../adr/0006-cges-ocsf-alignment.md) — CGES alignment. SPEC-001 reuses `common/cg_agent.json` and explains why the envelope itself is not a CGES event.
- [Foundational Blueprint](../product/blueprint.md) — §5 (Rust footprint goals), §7 (heartbeat 30 s, offline after 3 missed).
