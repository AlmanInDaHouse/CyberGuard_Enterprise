# services/ingest

Go service that terminates mTLS connections from agents and writes raw event batches to NATS JetStream.

Populated by SPEC-XXX-ingest. Until then this folder is a placeholder.

Expected responsibilities:

- Validate mTLS client certificates against the internal CA (CA pinning).
- Verify Ed25519 message signatures and the `sequence_number` / `nonce` envelope (replay defense via Redis).
- Publish batches to `events.raw.{org}.{agent}`.
- Reject agents below `min_supported_version` via the `X-CG-Agent-Version` header.
- Honor the per-`agent_id` revocation list.
