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

## Dependencies

- ADR-0002 → ADR-0001 (defines the top-level components this ADR assigns languages to)
