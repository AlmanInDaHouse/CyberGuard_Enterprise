# CI/CD workflows

GitHub Actions workflows for CyberGuard.

## Active workflows

| Workflow | Trigger | Purpose |
|---|---|---|
| [`markdown-lint.yml`](markdown-lint.yml) | PR + push to `main` | Validate Markdown syntax in `**/*.md` against `.markdownlint.yaml`. |

## Planned workflows

| Workflow | Activated by | Purpose |
|---|---|---|
| `ci.yml` | SPEC-XXX-ci | Build and unit test all services, the agent, and the dashboard. |
| `harness.yml` | SPEC-XXX-harness | Run all scenarios under `harness/scenarios/`. Block merge on regression. |
| `schema-validate.yml` | SPEC-XXX-cges-v0.1 | Validate `schemas/` with `ajv` and check examples against schemas. |
| `rules-test.yml` | SPEC-XXX-detection-rules | Validate every rule against its `.test.json` fixture. |
| `release.yml` | SPEC-XXX-release | Build and publish release artifacts (agent installer, server images). |

Workflows are introduced one at a time as the SPECs they support land. Empty workflow files are intentionally avoided.
