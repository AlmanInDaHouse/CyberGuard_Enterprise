# Architecture Decision Records

This directory holds Architecture Decision Records following the [MADR](https://adr.github.io/madr/) format.

## Naming

`NNNN-short-kebab-title.md`, where `NNNN` is a zero-padded sequential id starting at `0001`.

## Status values

- `Proposed` — under discussion.
- `Accepted` — current binding decision.
- `Deprecated` — superseded by a newer ADR; kept for historical context.
- `Superseded by ADR-NNNN` — explicit replacement pointer.

## Catalog

| ADR | Title | Status |
|---|---|---|
| [0001](0001-monorepo-layout.md) | Monorepo layout for CyberGuard | Accepted |
| [0002](0002-language-per-component.md) | Language per component | Accepted |
| [0003](0003-polyglot-storage.md) | Polyglot storage | Accepted |
| [0004](0004-agent-server-protocol.md) | Agent-Server secure protocol | Accepted |

## Dependencies

- ADR-0002 → ADR-0001 (defines the top-level components this ADR assigns languages to)
- ADR-0003 → ADR-0001 (defines the components whose data this ADR routes)
- ADR-0003 → ADR-0002 (language choices cite NATS and ClickHouse Go client maturity)
- ADR-0004 → ADR-0001 (locates `agent/` and `services/ingest/`)
- ADR-0004 → ADR-0002 (Rust for agent and Go for ingest are inputs to the protocol)
- ADR-0004 → ADR-0003 (Redis hosts nonce cache and revocation list)
