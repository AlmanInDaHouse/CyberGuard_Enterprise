# CI/CD workflows

GitHub Actions workflows for CyberGuard. Each runs on a push to `main` and on a pull request, when the push or the PR changes a path its filter names. A branch with no PR open runs none.

## Active workflows

| Workflow | Paths that trigger it | What it runs |
|---|---|---|
| [`markdown-lint.yml`](markdown-lint.yml) | `**/*.md`, `.markdownlint.yaml` | markdownlint-cli2 (through `markdownlint-cli2-action@v23`) over every Markdown file, against `.markdownlint.yaml`. |
| [`schema-validation.yml`](schema-validation.yml) | `schemas/**` | Every CGES example under `schemas/cges/v0.1/examples/` against the root schema `event.json`, which references the common, object and class schemas, with `ajv`. |
| [`rust-ci.yml`](rust-ci.yml) | `agent/**`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml` | `cargo fmt --check`, `cargo clippy -D warnings` and `cargo test --all`, in two jobs: `ubuntu-latest` and `windows-latest`. The tests that need real ETW or the Security log, elevated, are `#[ignore]`d here and run in the elevated developer-local gate (`CLAUDE.md`). |
| [`ts-ci.yml`](ts-ci.yml) | `services/ingest/**`, `services/api/**`, `dashboard/**`, `rules/**`, `harness/**`, `agent/**`, the Cargo and pnpm root files | Three jobs. **ingest**: typecheck, lint, build, the entrypoint check, a debug build of `cg-agent`, then vitest; the rule fixtures and the `harness/scenarios/` run here (SPEC-016 `rules_ac_002`, `rules_ac_003`). **api**: typecheck, lint, build, the entrypoint check, vitest. **dashboard**: `next build`, typecheck, lint, vitest. |
| [`infra-ci.yml`](infra-ci.yml) | `infra/dev/**`, the two test backends (`services/*/test/helpers/backends.ts`) | `node --test` on `infra/dev/check-compose.test.mjs`; then `docker compose config` renders the dev compose with `infra/dev/.env.example` and every profile, and `infra/dev/check-compose.mjs` asserts that every published port binds `127.0.0.1`, that no service uses the host's network, that every image is named by a digest or by a tag other than `latest`, and that ClickHouse is pinned to a patch release that both test backends start too (debt #68). |

Each workflow also runs when its own file changes.

## Planned workflows

| Workflow | Activated by | Purpose |
|---|---|---|
| `release.yml` | SPEC-XXX-release (roadmap §F) | Build and publish release artifacts (agent installer, server images). |

The earlier plan named `ci.yml`, `harness.yml`, `schema-validate.yml` and `rules-test.yml`. Their work is done by `rust-ci.yml` and `ts-ci.yml` (build and tests; the rules and the scenarios in the ingest job) and by `schema-validation.yml`.

Workflows are introduced one at a time as the SPECs they support land. Empty workflow files are intentionally avoided.
