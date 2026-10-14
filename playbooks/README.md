# SOAR playbooks

Declarative YAML playbooks, to be run by a SOAR module inside `services/ingest` ([ADR-0021](../docs/adr/0021-soar-mvp-playbook-executor.md)). Their schema is to be versioned in `schemas/playbooks/v0.1/` and validated at boot and in CI ([ADR-0002](../docs/adr/0002-language-per-component.md) Rule 6). The module, the schema, the loader and the first playbook land with ADR-0021's implementing SPEC; until then this folder holds no playbook.

## Fields of a playbook (ADR-0021 §2)

| Field | Purpose |
|---|---|
| `id`, `version`, `name` | Identity; the execution record carries `id` and `version`. |
| `trust_sources` | The detection sources whose incidents may run it: `rule`, `ml`, `hybrid_only` ([ADR-0005](../docs/adr/0005-detection-rules-and-ml-in-parallel.md) §Playbook governance). Every MVP incident comes from a rule. |
| `trigger` | `on: incident.created`, and a `when` on the incident's `severity_id`. |
| `steps` | At most 10 ordered steps, each an action from the closed list below, run `live` or `dry_run` (`dry_run` by default). |

## Actions in the MVP (ADR-0021 §3)

| Action | ADR-0005 class | Live |
|---|---|---|
| `agent.isolate` | semi-reversible | no: `dry_run` only |
| `notify.email` | reversible | yes |

The loader is to refuse any other action, a `live` `agent.isolate`, and any destructive action, so no playbook needs an approval step in the MVP. Human approval, rollback, and live semi-reversible or destructive actions wait for their own ADR (ADR-0021 §Out of scope).

MVP target: one operational playbook, dry-run and notification, with no destructive action (Blueprint §18).
