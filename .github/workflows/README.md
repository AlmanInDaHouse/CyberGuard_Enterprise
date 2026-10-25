# CI/CD workflows

GitHub Actions workflows for CyberGuard. Each runs on a push to `main` and on a pull request, when the push or the PR changes a path its filter names; `deploy-ci.yml` runs on a pull request alone, weekly on `main` and on demand (`workflow_dispatch`). A branch with no PR open runs none.

## Active workflows

| Workflow | Paths that trigger it | What it runs |
|---|---|---|
| [`markdown-lint.yml`](markdown-lint.yml) | `**/*.md`, `.markdownlint.yaml` | markdownlint-cli2 (through `markdownlint-cli2-action@v24`) over every Markdown file, against `.markdownlint.yaml`. |
| [`schema-validation.yml`](schema-validation.yml) | `schemas/**`, `playbooks/**` | Every CGES example under `schemas/cges/v0.1/examples/` against the root schema `event.json`, which references the common, object and class schemas, and every SOAR playbook under `playbooks/` against `schemas/playbooks/v0.1/playbook.json` (SPEC-022 §Operational §8), with `ajv-cli` on Node 22. |
| [`rust-ci.yml`](rust-ci.yml) | `agent/**`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml` | `cargo fmt --check`, `cargo clippy -D warnings` and `cargo test --all`, in two jobs: `ubuntu-latest` and `windows-latest`. The tests that need real ETW or the Security log, elevated, are `#[ignore]`d in `cargo test --all`; the Windows job runs them in a step of their own (SPEC-017, the second Amendment 2026-10-19), and the elevated developer-local gate runs them too (`CLAUDE.md`). |
| [`ts-ci.yml`](ts-ci.yml) | `services/ingest/**`, `services/api/**`, `dashboard/**`, `rules/**`, `harness/**`, `agent/**`, the Cargo and pnpm root files | Three jobs. **ingest**: typecheck, lint, build, the entrypoint check, a debug build of `cg-agent`, then vitest; the rule fixtures and the `harness/scenarios/` run here (SPEC-016 `rules_ac_002`, `rules_ac_003`). **api**: typecheck, lint, build, the entrypoint check, vitest. **dashboard**: `next build`, typecheck, lint, vitest. |
| [`infra-ci.yml`](infra-ci.yml) | `infra/dev/**`, the two test backends (`services/*/test/helpers/backends.ts`) | `node --test` on `infra/dev/check-compose.test.mjs`; then `docker compose config` renders the dev compose with `infra/dev/.env.example` and every profile, and `infra/dev/check-compose.mjs` asserts that every published port binds `127.0.0.1`, that no service uses the host's network, that every image is named by a digest or by a tag other than `latest`, and that ClickHouse is pinned to a patch release that both test backends start too (debt #68). |
| [`dependency-audit.yml`](dependency-audit.yml) | `pnpm-lock.yaml`, the `package.json` files, `Cargo.lock`, the Cargo manifests; and every Monday at 06:17 UTC | `pnpm audit --prod --audit-level=high` over the lockfile: a high or critical advisory in a production dependency fails the run, a moderate or low one is counted and passes; and `pnpm audit --audit-level=critical` over every dependency, development ones included, where a critical advisory fails the run (debt #110). `cargo audit` (cargo-audit 0.22.2) over every crate `Cargo.lock` holds, development dependencies included: any vulnerability advisory fails the run, and an unmaintained, unsound or yanked crate is a warning that passes. The weekly run catches an advisory published after a release (debt #93); [`docs/security/dependency-review.md`](../../docs/security/dependency-review.md) says what to do when it is red. |
| [`deploy-ci.yml`](deploy-ci.yml) | On a pull request: `deploy/**`, `docs/operations/**`, `services/**`, `dashboard/**`, `agent/**`, `rules/**`, `playbooks/**`, `schemas/playbooks/**`, `LICENSE`, `NOTICE`, `.dockerignore`, the pnpm root files; every Monday at 06:41 UTC on `main`; on demand | Builds the four product images from the commit (ingest, api, dashboard, proxy) and runs `deploy/docker/test/` against them with `node --test`: each runs as its non-root uid and carries `LICENSE`, `NOTICE`, the four OCI labels and a health check, each Dockerfile pins its base images by digest, the rules and playbooks are read-only, the proxy's Caddy binary carries no file capability, and the dashboard and the proxy start read-only with every capability dropped (SPEC-023 deploy_ac_019's image half). It pushes nothing. Its actions are pinned by commit SHA. |

Each workflow also runs when its own file changes.

## Planned workflows

| Workflow | Activated by | Purpose |
|---|---|---|
| `release.yml` | SPEC-XXX-release (roadmap §F) | Build and publish release artifacts (agent installer, server images). |

The earlier plan named `ci.yml`, `harness.yml`, `schema-validate.yml` and `rules-test.yml`. Their work is done by `rust-ci.yml` and `ts-ci.yml` (build and tests; the rules and the scenarios in the ingest job) and by `schema-validation.yml`.

Workflows are introduced one at a time as the SPECs they support land. Empty workflow files are intentionally avoided.
