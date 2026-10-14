# Services

Server-side runtime components, one per subdirectory. A built service has its own build, tests and Dockerfile; the others are placeholders. TypeScript is the server-side default; the event firehose keeps Go, which its ADR confirms or changes ([ADR-0002](../docs/adr/0002-language-per-component.md), Amendment 2026-10-14).

| Service | Language | Purpose |
|---|---|---|
| [`api/`](api/) | TypeScript (Fastify + Zod) | The human-facing API: authentication and RBAC, the read API, and the forensic export and report (the `forensic/` role, SPEC-013). |
| [`ingest/`](ingest/) | TypeScript (Fastify) | The agents' enroll and mTLS heartbeat endpoints, event storage in ClickHouse, the detection driver (the `pipeline/` slice, ADR-0012), and incident notification. SOAR is to run here too ([ADR-0021](../docs/adr/0021-soar-mvp-playbook-executor.md), with its implementing SPEC). |
| [`pipeline/`](pipeline/) | Go, for the firehose | Not built: the high-throughput event firehose, under its own ADR. The detection slice runs inside `ingest/` (ADR-0012). |
| [`soar/`](soar/) | — | Not built as a service: ADR-0021 places SOAR inside `ingest/`. |
| [`ml/`](ml/) | Python (FastAPI) | Not built: the detection ML service, the only Python surface in the project. |
| [`forensic/`](forensic/) | — | Not built as a service: the report render runs inside `api/` (SPEC-013). |

Each built service owns its `Dockerfile`, its tests, and its harness scenario contributions.
