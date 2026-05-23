# Specifications

Functional and technical specifications. Every module of CyberGuard is preceded by a SPEC document landed in this directory before any code is written.

## Naming

`SPEC-XXX-short-kebab-title.md`, where `XXX` is a zero-padded sequential id starting at `001`.

## Required sections per SPEC

1. **Context** — why the module exists and what problem it solves.
2. **Scope** — what is in and what is explicitly out.
3. **Data contracts** — schemas referenced, request/response shapes, event shapes.
4. **Acceptance criteria** — observable conditions for "done".
5. **Test scenarios** — harness scenarios mapped to this SPEC, with expected inputs and outputs.
6. **Risks** — known failure modes and mitigations.
7. **Open questions** — unresolved decisions, tracked until closure.
8. **References** — related SPECs, ADRs, external standards.

## Catalog

| SPEC | Title | Status |
|---|---|---|
| [SPEC-001](SPEC-001-agent-heartbeat.md) | Agent heartbeat | Accepted |
| [SPEC-002](SPEC-002-agent-enrollment.md) | Agent enrollment | Accepted |
| [SPEC-003](SPEC-003-mtls-signed-envelope.md) | mTLS 1.3 and signed envelope | Accepted |
| [SPEC-004](SPEC-004-server-ingest-minimal.md) | Server ingest minimal | Accepted |
| [SPEC-005](SPEC-005-agent-process-telemetry-windows-etw.md) | Agent process telemetry — Windows ETW Kernel-Process | Accepted |
