# Security documentation

Threat model, secure coding guidelines, privacy considerations and key-management procedures live here.

- [threat-model.md](threat-model.md) — the threat model, with its record of the system as built.
- [dependency-review.md](dependency-review.md) — the dependency runbook: the audit in CI, what to do when it is red, and how a dependency is added.

Populated by:

- SPEC-XXX-threat-model.
- SPEC-XXX-agent-server-protocol (mTLS, Ed25519 signing, key custody, replay defense, certificate rotation).
- SPEC-XXX-secure-coding.

For vulnerability reporting see the root [SECURITY.md](../../SECURITY.md).
