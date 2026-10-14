# Playbook schemas

Versioned JSON Schemas of the SOAR playbook document ([ADR-0002](../../docs/adr/0002-language-per-component.md) Rule 6; [ADR-0021](../../docs/adr/0021-soar-mvp-playbook-executor.md); [SPEC-022](../../docs/specs/SPEC-022-soar-mvp-playbooks.md) §Data contracts §1).

| Version | Purpose |
|---|---|
| [`v0.1/`](v0.1/) | The MVP's playbook: `trust_sources`, an `incident.created` trigger with a severity floor, up to 10 steps of `agent.isolate` (dry-run only) and `notify.email`. |
