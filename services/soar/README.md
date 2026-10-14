# services/soar

**Not built as a standalone service.** [ADR-0021](../../docs/adr/0021-soar-mvp-playbook-executor.md) places SOAR for the MVP as a TypeScript module inside `services/ingest`, run on the incident-create seam. A service here would come with the event bus, under its own ADR. This folder stays a placeholder.

What the module is to do, once ADR-0021's implementing SPEC lands (ADR-0021 §Decision and §Out of scope):

- Load declarative YAML playbooks from `INGEST_SOAR_PLAYBOOKS_DIR` (by default the repository's `playbooks/`, bundled into the image), validated against `schemas/playbooks/v0.1/` at boot and in CI.
- Run `dry_run` by default, and containment (`agent.isolate`) only in dry-run.
- Honour `trust_sources` per playbook (ADR-0005).
- Keep one record per playbook and incident, claimed before its first step, completed once, and hash-chained per step.

Human approval, rollback, and live semi-reversible or destructive actions wait for their own ADR.
