# cg-agent

The CyberGuard endpoint agent crate. Single binary that implements [SPEC-001 — Agent heartbeat](../../docs/specs/SPEC-001-agent-heartbeat.md).

## Status

Scaffold. The binary builds, prints a placeholder line, and exits. The full SPEC-001 behaviour (TOML config load, HTTP heartbeat loop with retry / backoff, structured JSON logs, graceful shutdown with a final `"going_offline"` heartbeat) lands in a subsequent commit alongside the integration harness.

## Build

From the repository root:

```sh
cargo build --release -p cg-agent
```

The release binary lives at `target/release/cg-agent.exe` (Windows) or `target/release/cg-agent` (Linux / macOS).

## Run (dev)

```sh
cargo run -p cg-agent
```

A configuration file is required by SPEC-001 §FR-001 but is not consumed by the scaffold. The full command will be:

```sh
cargo run -p cg-agent -- --config path/to/agent.toml
```

See [SPEC-001 §Configuration](../../docs/specs/SPEC-001-agent-heartbeat.md#configuration) for the `agent.toml` schema and defaults.

## Test

```sh
cargo test -p cg-agent
```

Integration tests under `tests/` map 1:1 to SPEC-001 acceptance criteria (AC-001 to AC-010). The scaffold ships with no tests; the harness lands in a follow-up commit explicitly RED-by-design (no implementation yet), and the implementation commit turns it green.

## Lint

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

CI runs both as part of `.github/workflows/rust-ci.yml`.

## Layout (post-implementation)

The module split inside `src/` will be (per SPEC-001 §Behavior and Session 5 briefing):

| Module | Responsibility |
|---|---|
| `main.rs` | Entry point, CLI parsing, logger init, run loop. |
| `config.rs` | TOML schema, validation, defaults. |
| `envelope.rs` | Heartbeat envelope type, serde derives. |
| `transport.rs` | HTTP client wrapper, retry policy, backoff. |
| `shutdown.rs` | Signal handler, graceful shutdown. |
| `errors.rs` | `thiserror` enum for the crate. |
| `lib.rs` | Re-exports for the integration harness. |
