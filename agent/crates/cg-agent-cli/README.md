# cg-agent-cli

CLI and installer for the CyberGuard agent.

Populated by SPEC-XXX-cg-agent-cli. Until then this folder is a placeholder.

Expected responsibilities:

- Enrollment flow against the server (consume the JWT enrollment token, generate Ed25519 keypair, submit CSR, install issued client certificate).
- Status command (heartbeat health, last successful send, buffer state).
- Configuration inspection.
- Clean uninstall.
- Single-binary distribution per target platform.
