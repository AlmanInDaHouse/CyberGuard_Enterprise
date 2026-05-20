# Agent

The CyberGuard endpoint agent.

The agent is a self-deployable program that sends telemetry (processes, network, files, users, logs, configuration) and heartbeat to the CyberGuard server through an mTLS-terminated channel signed with the agent's Ed25519 key. SPEC-001 brings up the heartbeat path on a deliberate HTTP-only subset; mTLS + Ed25519 land in a future sub-SPEC per ADR-0004.

## Current layout

| Path | Purpose |
|---|---|
| [`cg-agent/`](cg-agent/) | First concrete crate. Member of the workspace at the repo root. Implements SPEC-001 (heartbeat). |
| [`crates/`](crates/) | Roadmap placeholders for the forward-looking multi-crate split (cg-agent-core / -windows / -linux / -cli). Each `README.md` describes the future crate; no code lives there yet. |

The workspace lives at the **repository root** ([`/Cargo.toml`](../Cargo.toml)) so that future Rust projects in this repo can join the same workspace without reorganising paths.

## Build

```sh
cargo build --release -p cg-agent
```

See [`cg-agent/README.md`](cg-agent/README.md) for run instructions, config schema and tests.
