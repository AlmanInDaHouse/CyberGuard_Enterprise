# services/forensic

**Superseded as a standalone service by [SPEC-013](../../docs/specs/SPEC-013-forensic-report-render.md).** The per-incident forensic report is rendered as a **module in `services/api`** (TypeScript, `@react-pdf/renderer`), **on-demand**, **PDF only** — not a standalone Go service (ADR-0007 language precedent; the api already owns the read-layer the report composes). This folder stays a placeholder; no service is built here in the MVP.

Status of the responsibilities this placeholder once listed:

- Reconstruct a timeline for the incident window — **delivered** by SPEC-010 (the drill, `services/api/src/read/`), not here.
- Hash-chain evidence references for tamper-evident reporting — **delivered** by SPEC-012 (`services/api/src/forensic/`), not here.
- Render PDF / HTML / JSON output — **PDF delivered** by SPEC-013 (in `services/api`); **HTML / JSON dropped** (no pure-JS HTML→PDF without Chromium; JSON is already the SPEC-012 forensic-export response).
- Embed MITRE ATT&CK mapping and an AI-generated summary — **MITRE delivered** by SPEC-013 (from `IncidentDetail.cg_mitre`); the **AI summary is deferred** (not part of the MVP PDF promise; a later increment).
- Store generated reports in MinIO — **deferred**: physical at-rest (recorte (b)) stays zero-consumer (ADR-0016 / SPEC-012 §Out of scope); a later forensic-persistence increment.
