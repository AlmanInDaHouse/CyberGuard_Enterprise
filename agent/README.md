# Agent

The CyberGuard endpoint agent.

The agent is a self-deployable program that sends telemetry (processes, network, files, users, logs, configuration) and heartbeat to the CyberGuard server through an mTLS-terminated channel signed with the agent's Ed25519 key. Today it enrolls, heartbeats over mTLS 1.3 with signed envelopes (SPEC-001/002/003), and captures processes on Windows (SPEC-005, SPEC-017).

## Current layout

| Path | Purpose |
|---|---|
| [`cg-agent/`](cg-agent/) | First concrete crate. Member of the workspace at the repo root. Implements SPEC-001/002/003 and Windows process capture (SPEC-005, SPEC-017). |
| [`crates/`](crates/) | Roadmap placeholders for the forward-looking multi-crate split (cg-agent-core / -windows / -linux / -cli). Each `README.md` describes the future crate; no code lives there yet. |

The workspace lives at the **repository root** ([`/Cargo.toml`](../Cargo.toml)) so that future Rust projects in this repo can join the same workspace without reorganising paths.

## Build

```sh
cargo build --release -p cg-agent
```

See [`cg-agent/README.md`](cg-agent/README.md) for run instructions, config schema and tests.
