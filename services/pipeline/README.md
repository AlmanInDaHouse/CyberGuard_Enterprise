# services/pipeline

Go service implementing the SIEM/XDR processing chain: normalize → enrich → correlate → score.

Populated by SPEC-XXX-pipeline. Until then this folder is a placeholder.

Expected responsibilities:

- Consume `events.raw.*`, produce `events.normalized.*` in CGES.
- Enrich with GeoIP, threat intel, asset criticality and user role; Redis-backed cache.
- Run the rule engine (Sigma-compatible) and consume ML detector alerts in parallel.
- Emit alerts with `detection_source: "rule" | "ml" | "hybrid"`.
- Combined score: `w1 · heuristic + w2 · UEBA + w3 · ML` (default weights `0.6 / 0.25 / 0.15`, configurable per org).
- Deduplicate alerts by `dedup_key` in a 5-minute window.
- Group alerts into incidents by host / user / MITRE tactic within a time window.
