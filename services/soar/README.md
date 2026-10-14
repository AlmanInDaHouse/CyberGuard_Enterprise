# services/soar

**Not built as a standalone service.** For the MVP, SOAR is a TypeScript module inside `services/ingest` (`src/soar/`), run on the incident-create seam ([ADR-0021](../../docs/adr/0021-soar-mvp-playbook-executor.md), [SPEC-022](../../docs/specs/SPEC-022-soar-mvp-playbooks.md)). A service here would come with the event bus, under its own ADR. This folder stays a placeholder.

What the module does, and what waits (ADR-0021 §Decision and §Out of scope):

- It loads declarative YAML playbooks from `INGEST_SOAR_PLAYBOOKS_DIR` (by default the repository's `playbooks/`, bundled into the image), validated against `schemas/playbooks/v0.1/` at boot and in CI.
- It runs `dry_run` by default, and containment (`agent.isolate`) only in dry-run.
- It honours `trust_sources` per playbook (ADR-0005).
- It keeps one record per playbook and incident, claimed before its first step, completed once, and hash-chained per step.

Human approval, rollback, and live semi-reversible or destructive actions wait for their own ADR.
