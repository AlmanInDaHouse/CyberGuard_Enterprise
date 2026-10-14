# Playbook schema v0.1

[`playbook.json`](playbook.json) (JSON Schema draft 2020-12) is the playbook document of [SPEC-022](../../../docs/specs/SPEC-022-soar-mvp-playbooks.md) §Data contracts §1. The loader in `services/ingest/src/soar/loader.ts` validates each playbook against it at boot, then applies what the schema cannot state: unique step ids, the closed placeholder set, and no live `agent.isolate`. CI validates `playbooks/` against it twice: through the loader in `ts-ci`, and with `ajv-cli` in `schema-validation`.
