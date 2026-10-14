# services/pipeline

**Not built.** The planned home of the high-throughput event firehose and the SIEM/XDR processing chain: normalize → enrich → correlate → score. ADR-0002 assigns it Go, and the event-firehose ADR confirms or changes that language and its component boundary ([ADR-0007](../../docs/adr/0007-ingest-language-typescript-mvp.md) §Compliance; [ADR-0002](../../docs/adr/0002-language-per-component.md), Amendment 2026-10-14). For the MVP, the detection slice runs inside `services/ingest` ([ADR-0012](../../docs/adr/0012-normalize-before-correlate-pipeline.md)). This folder stays a placeholder.

Planned responsibilities:

- Consume `events.raw.*`, produce `events.normalized.*` in CGES.
- Enrich with GeoIP, threat intel, asset criticality and user role; Redis-backed cache.
- Run the rule engine (Sigma-compatible) and consume ML detector alerts in parallel.
- Emit alerts with `detection_source: "rule" | "ml" | "hybrid"`.
- Combined score: `w1 · heuristic + w2 · UEBA + w3 · ML` (default weights `0.6 / 0.25 / 0.15`, configurable per org).
- Deduplicate alerts by `dedup_key` in a 5-minute window.
- Group alerts into incidents by host / user / MITRE tactic within a time window.
