# cg-agent-core

Cross-platform runtime for the CyberGuard agent.

Populated by SPEC-XXX-cg-agent-core. Until then this folder is a placeholder.

Expected responsibilities:

- Async runtime (tokio).
- mTLS transport (rustls) with CA pinning.
- Ed25519 message signing with monotonic `sequence_number` and `nonce`.
- Local encrypted buffer for degraded mode (key derived from platform-native key custody).
- Heartbeat scheduler (30s, offline after 3 missed).
- Configuration management and hot-reload of policy.
- Client certificate auto-rotation from day 75 of a 90-day TTL.
