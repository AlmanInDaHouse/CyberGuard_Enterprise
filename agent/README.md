# Agent

Rust workspace for the CyberGuard endpoint agent.

The agent is a self-deployable program that sends telemetry (processes, network, files, users, logs, configuration) and heartbeat to the CyberGuard server through an mTLS-terminated channel signed with the agent's Ed25519 key.

## Workspace layout

| Crate | Purpose |
|---|---|
| [`crates/cg-agent-core/`](crates/cg-agent-core/) | Cross-platform runtime: scheduler, transport, signing, local buffer. |
| [`crates/cg-agent-windows/`](crates/cg-agent-windows/) | Windows-specific telemetry sources and DPAPI key custody. |
| [`crates/cg-agent-linux/`](crates/cg-agent-linux/) | Linux-specific telemetry sources and keyring custody. |
| [`crates/cg-agent-cli/`](crates/cg-agent-cli/) | Installer and CLI for enrollment, status and uninstall. |

## Build

Populated by SPEC-XXX-cg-agent. Until then [Cargo.toml](Cargo.toml) declares an empty workspace.
