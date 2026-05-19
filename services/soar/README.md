# services/soar

Go service that executes SOAR playbooks declaratively.

Populated by SPEC-XXX-soar. Until then this folder is a placeholder.

Expected responsibilities:

- Parse YAML playbooks under `playbooks/`.
- Enforce dry-run mode by default.
- Honor `trust_sources` per playbook (`["rule"]`, `["rule", "ml"]`, or `["hybrid_only"]`).
- Require explicit human approval for destructive actions.
- Provide rollback steps for reversible and semi-reversible actions.
- Maintain a tamper-evident audit log of every step (hash-chained).
