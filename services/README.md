# Services

Server-side runtime components. Each subdirectory hosts a single service with its own build, test and Dockerfile.

| Service | Language | Purpose |
|---|---|---|
| [`api/`](api/) | TypeScript (Fastify + Zod) | BFF for the dashboard and external clients. |
| [`ingest/`](ingest/) | Go | mTLS-terminating ingest endpoint; writes raw events to NATS JetStream. |
| [`pipeline/`](pipeline/) | Go | normalize → enrich → correlate → score. |
| [`soar/`](soar/) | Go | Playbook executor with dry-run, approval, rollback and audit. |
| [`ml/`](ml/) | Python (FastAPI) | Detection ML service. The only Python surface in the project. |
| [`forensic/`](forensic/) | Go | Generates incident reports (PDF / HTML / JSON). |

Each service must own its `Dockerfile`, its tests, and its harness scenario contributions.
