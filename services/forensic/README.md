# services/forensic

Go service that builds forensic reports per incident.

Populated by SPEC-XXX-forensic. Until then this folder is a placeholder.

Expected responsibilities:

- Reconstruct a timeline from `events.normalized.*` for the incident window.
- Hash-chain evidence references for tamper-evident reporting.
- Render PDF / HTML / JSON output.
- Embed MITRE ATT&CK mapping and an AI-generated summary (labeled as AI-generated).
- Store generated reports in MinIO.
