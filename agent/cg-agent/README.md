# cg-agent

The CyberGuard endpoint agent crate: a single binary implementing [SPEC-001](../../docs/specs/SPEC-001-agent-heartbeat.md) (heartbeat), [SPEC-002](../../docs/specs/SPEC-002-agent-enrollment.md) (enrollment), [SPEC-003](../../docs/specs/SPEC-003-mtls-signed-envelope.md) (mTLS 1.3 + signed envelope) and, on Windows, process capture ([SPEC-005](../../docs/specs/SPEC-005-agent-process-telemetry-windows-etw.md), [SPEC-017](../../docs/specs/SPEC-017-agent-capture-normal-run-path.md)).

## What it does

- Loads `agent.toml` (`--config <path>`, default `./agent.toml`), enrolls on first run and persists its identity (DPAPI-sealed key on Windows).
- With `server.trust_anchor_path` set, runs the secure path: TLS 1.3 mutual authentication and Ed25519-signed envelopes to `/v1/agents/heartbeat`.
- On Windows the secure path opens an ETW Kernel-Process session and delivers process Launch / Terminate events inside the signed envelope, at least once, in batches of up to 1024 (SPEC-017). The image path is translated to Win32 form.
- On Windows the agent must run **elevated**: an unelevated agent exits with code 9 and says so on stderr (SPEC-005 AC-002, ADR-0010). Other platforms have no capture backend and send heartbeats only.
- Without a trust anchor, the SPEC-001 plain-HTTP heartbeat runs instead (no capture).

## Build

From the repository root:

```sh
cargo build --release -p cg-agent
```

The release binary lives at `target/release/cg-agent.exe` (Windows) or `target/release/cg-agent` (Linux / macOS). Non-Windows release builds are a compile error: there is no production secure-storage or capture backend off Windows.

## Run

```sh
cargo run -p cg-agent -- --config path/to/agent.toml
```

See [SPEC-001 §Configuration](../../docs/specs/SPEC-001-agent-heartbeat.md#configuration), [SPEC-002](../../docs/specs/SPEC-002-agent-enrollment.md) and [SPEC-003 §Configuration](../../docs/specs/SPEC-003-mtls-signed-envelope.md#configuration) for the `agent.toml` schema and defaults.

## Test

```sh
cargo test --all
```

Integration tests under `tests/` map to the acceptance criteria of each SPEC (`ac_*` SPEC-001, `enroll_ac_*` SPEC-002, `mtls_ac_*` SPEC-003, `process_ac_*` SPEC-005, `capture_ac_*` SPEC-017). The delivery tests feed synthetic events to a TLS mock and run on every platform. The tests that need real ETW are `#[ignore]`d; on Windows, in an elevated terminal:

```sh
cargo test -p cg-agent -- --ignored --test-threads=1
```

## Lint

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

CI runs both, and the tests, on Linux and Windows (`.github/workflows/rust-ci.yml`).

## Layout

| Module | Responsibility |
|---|---|
| `main.rs` | Entry point, CLI parsing, logger init, path selection, exit codes. |
| `lib.rs` | `run` (SPEC-001 plain path) and `run_secure` (secure path with capture). |
| `delivery.rs` | The secure path's batching, retry and shutdown loop (SPEC-017). |
| `etw/` | ETW session, dispatch, ring buffer, created-time cache, hygiene (Windows). |
| `cges/` | Rendering captured events to the CGES wire shape. |
| `paths.rs` | Kernel device path → Win32 path translation. |
| `config.rs` | TOML schema, validation, defaults. |
| `envelope.rs`, `signing.rs`, `canonical.rs` | Heartbeat envelope, signed outer envelope, JCS. |
| `tls.rs`, `transport.rs` | mTLS client; plain-HTTP client with retry. |
| `enrollment.rs`, `identity.rs`, `crypto.rs`, `secure_storage.rs` | Enrollment and identity. |
| `errors.rs`, `startup.rs`, `shutdown.rs` | Error types and exit codes, startup checks, shutdown signal. |
