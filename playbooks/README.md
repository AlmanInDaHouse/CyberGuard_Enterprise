# SOAR playbooks

Declarative YAML playbooks, run by the SOAR module inside `services/ingest` (`src/soar/`) when the detection cycle creates an incident ([ADR-0021](../docs/adr/0021-soar-mvp-playbook-executor.md), [SPEC-022](../docs/specs/SPEC-022-soar-mvp-playbooks.md)). Ingest reads this folder when `INGEST_SOAR_PLAYBOOKS_DIR` is unset; the variable names another directory. The image bundles a copy of this folder at build time. Their schema is [`schemas/playbooks/v0.1/`](../schemas/playbooks/v0.1/), validated at boot and in CI ([ADR-0002](../docs/adr/0002-language-per-component.md) Rule 6).

| Playbook | What it does |
|---|---|
| [`pb_high_severity_containment.yml`](pb_high_severity_containment.yml) | On a new incident of severity High or more from a rule: records, in dry-run, the isolation of its agent, then emails the SOC that containment awaits an analyst. |

## Fields of a playbook (SPEC-022 §Data contracts §1)

| Field | Purpose |
|---|---|
| `id`, `version`, `name` | Identity; the execution record carries `id` and `version`. |
| `trust_sources` | The detection sources whose incidents may run it: `rule`, `ml`, `hybrid_only` ([ADR-0005](../docs/adr/0005-detection-rules-and-ml-in-parallel.md) §Playbook governance). In the MVP only `rule` matches. |
| `trigger` | `on: incident.created`, and `when: { severity_id_gte }` on the incident's severity at creation. |
| `steps` | At most 10 ordered steps, each an action from the closed list below, run `live` or `dry_run` (`dry_run` by default). |

## Actions in the MVP (ADR-0021 §3)

| Action | ADR-0005 class | Live |
|---|---|---|
| `agent.isolate` | semi-reversible | no: `dry_run` only |
| `notify.email` | reversible | yes |

The loader refuses any other action, a `live` `agent.isolate`, and any destructive action, so no playbook needs an approval step in the MVP. A `notify.email` template may hold only the placeholders SPEC-022 lists. Every run leaves one execution record per playbook and incident, readable at `GET /v1/incidents/:id/playbook-executions`. Human approval, rollback, and live semi-reversible or destructive actions wait for their own ADR (ADR-0021 §Out of scope).

Ingest loads the playbooks at boot. Run from the repository, a playbook added here runs from the next boot. In the image, it needs a rebuild, or a directory mounted and named by `INGEST_SOAR_PLAYBOOKS_DIR`. A directory with no playbook leaves SOAR idle.
