# SOAR playbooks

Declarative YAML playbooks executed by [`services/soar/`](../services/soar/).

## Required fields per playbook

| Field | Purpose |
|---|---|
| `id` | Stable identifier. |
| `name`, `description` | Human-readable metadata. |
| `trust_sources` | Subset of `["rule", "ml", "hybrid"]` whose alerts can trigger this playbook. |
| `trigger` | Condition that selects an alert or incident. |
| `actions` | Ordered list of declarative steps. |
| `approval` | Required for any destructive step. |
| `rollback` | Compensating step for each reversible action. |

## Action classes

| Class | Examples | Source policy |
|---|---|---|
| Reversible | tag, notify, suppress, escalate severity, ack | rule / ml / hybrid |
| Semi-reversible | isolate endpoint, kill process | rule (or hybrid with rule confirmation) |
| Destructive | delete data, disable AD user, modify policies | always requires human approval |

The action policy follows the detection philosophy described in the onboarding (forthcoming ADR-0005).

MVP target: one operational playbook (dry-run + notification, no destructive actions).

Populated by SPEC-XXX-playbooks.
