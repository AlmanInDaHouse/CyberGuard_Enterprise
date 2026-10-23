# ADR-0001: Monorepo layout for CyberGuard

- Status: Accepted
- Date: 2026-05-20
- Deciders: Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

CyberGuard is a self-deployable SOC/XDR platform composed of several runtime artifacts written in different languages:

- A Rust endpoint agent (Windows and Linux targets).
- A constellation of Go services (ingest, pipeline, soar, forensic).
- A TypeScript / Fastify API and BFF.
- A Python FastAPI ML service (the only Python surface in the project).
- A Next.js 15 SOC dashboard.

These components share contracts (the CyberGuard Common Event Schema, OpenAPI definitions), detection assets (Sigma-compatible rules, SOAR playbooks), a scenario-based harness, and deployment manifests. They will evolve at different cadences but must remain consistent at the contract boundaries.

The project also commits to a Spec-Driven workflow: every module is preceded by a SPEC and one or more ADRs, and a harness scenario blocks the merge of any new detection or pipeline transformation. These workflow artifacts must live alongside the code they govern.

We need a repository layout that:

1. Keeps contracts (`schemas/`), detection assets (`rules/`, `playbooks/`) and harness scenarios (`harness/scenarios/`) close to the services that consume them, so changes are atomic.
2. Allows independent build and test cycles per language without forcing a monolithic toolchain.
3. Makes the boundary between "central server" and "endpoint agent" explicit.
4. Reserves a clear home for documentation that is mandatory by policy (SPECs, ADRs, threat model, runbook).
5. Leaves placeholders for deployment targets we do not implement in the MVP (Helm, Terraform) without obscuring what is currently in scope.

## Decision

We adopt a single polyglot monorepo with the following top-level structure:

```text
docs/        — specifications, ADRs, architecture, security, product, operations
schemas/     — versioned data contracts (CGES, OpenAPI)
services/    — server-side services, one subdirectory per service
agent/       — Rust workspace for the endpoint agent
dashboard/   — Next.js 15 SOC dashboard
rules/       — Sigma-compatible detection rules and per-rule tests
playbooks/   — SOAR playbooks (YAML) and tests
harness/     — scenario-based end-to-end harness
deploy/      — deployment manifests (docker / helm / terraform)
.github/     — CI/CD workflows
```

Root-level meta-files only:

- `README.md`, `LICENSE`, `NOTICE`, `SECURITY.md`.
- `.editorconfig`, `.gitignore`, `.gitattributes`, `.markdownlint.yaml`.
- `Taskfile.yml` (cross-platform task runner; no Makefile).
- `docker-compose.yml`, `docker-compose.dev.yml`.

### Criteria for adding a new top-level directory

A new top-level directory is justified only when **all** of the following hold:

1. It hosts a class of artifact that does not naturally fit into any existing top-level (services, agent, dashboard, schemas, rules, playbooks, harness, deploy, docs).
2. It will accumulate at least three distinct subdirectories or files within the current MVP cycle.
3. Its addition is documented in a new ADR that supersedes or amends this one.

Cosmetic groupings ("`tools/`", "`scripts/`", "`misc/`") are disallowed. One-off helpers live next to the service that uses them.

### Criteria for splitting the repository

If at any point a component requires an independent release cadence, an independent legal regime, or imposes a build cost on the rest of the repo that exceeds 30% of the average local build time, we open an ADR to evaluate extraction. Until then the monorepo holds.

## Alternatives considered

### A1 — Multi-repo (one repo per component)

Pros: independent release pipelines, smaller blast radius for accidental changes, simpler per-component CI.

Cons: contract drift between schemas and consumers becomes invisible until integration; coordinated changes across agent and server require synchronized PRs across repos; harness scenarios end up far from the rules they validate; onboarding requires cloning N repos.

Rejected because contract consistency between agent, ingest, pipeline and dashboard is the single highest correctness risk in the MVP. A monorepo aligns incentives toward keeping contracts and consumers in sync.

### A2 — Monorepo grouped by layer (`backend/`, `frontend/`, `infra/`)

Pros: familiar to developers coming from web stacks.

Cons: collapses the agent and server into a generic "backend", obscuring the fact that they have very different deployment models, threat models and lifecycles; pushes schema and rule changes to a single "backend" folder that becomes a hotspot.

Rejected because the agent is not a backend service; it is an artifact distributed to endpoints. The architectural boundary deserves a top-level.

### A3 — Monorepo grouped by domain (`detection/`, `response/`, `ingestion/`, ...)

Pros: maps to the SIEM/XDR pipeline stages described in the onboarding (collect → normalize → enrich → correlate → score → alert → incident → case → playbook → report).

Cons: a single service typically straddles multiple domains; detection rules and the rule engine end up in different top-levels; the layout encourages duplication of cross-cutting concerns.

Rejected because domain decomposition is better expressed inside `services/` (where each service maps to a stage) than at the top level.

## Consequences

### Positive

- Atomic changes that touch a contract, its consumer service and a harness scenario can land in a single PR.
- Onboarding is a single `git clone` and a single `task bootstrap`.
- The boundary between agent and server is visible from the root.
- The documentation policy (SPEC → ADR → schemas → harness → implementation) has a stable home (`docs/`) and a single index.
- CI configuration lives in one place; cross-language linting and harness validation are easier to orchestrate.

### Negative

- A polyglot toolchain is required locally (Rust, Go, Node, Python). We mitigate this by gating environment setup behind `task bootstrap`.
- Path-based access control on a single repo is coarser than per-repo permissions; to be addressed in a future ADR (CODEOWNERS) once the contributor base requires it.
- Repo size grows monotonically with all build artifacts ignored; `.gitignore` and `.gitattributes` are first-class concerns and must be reviewed when adding a new language.

### Neutral

- Release tagging will need a convention (e.g. `agent/vX.Y.Z`, `server/vX.Y.Z`) since components ship independently. To be addressed when the first release is cut.

## Compliance

Subsequent ADRs and SPECs that introduce new top-level directories must reference this ADR and explicitly justify the addition against the "Criteria for adding a new top-level directory" section above.

## Amendment 2026-10-19: where the compose files live, `infra/`, and the release tag

**What surfaced it.** Debt #12 (`docs/handoff-session-27.md:140`) and roadmap Part (b) §2 and §4. §Decision lists `docker-compose.yml` and `docker-compose.dev.yml` among the root's meta-files, and `deploy/` as the home of deployment manifests. The two root files are `services: {}` stubs, and the dev stack that runs lives in `infra/dev/docker-compose.dev.yml`, which shares the stub's basename. `infra/` is a top-level directory that no ADR added: it arrived with the dev stack (`565b666`, 2026-05-20), and §Criteria asks an ADR for one. ADR-0022 places the production compose; §Consequences, *Neutral*, left the release tag to the first release, which ADR-0022 §7 provides for.

**The amendment.**

1. **No compose file at the root.** The root's meta-files lose `docker-compose.yml` and `docker-compose.dev.yml`, and the two stubs are removed, with the basename collision of debt #12.
2. **The production compose lives in `deploy/docker/`**, with the files it reads (the stores' initialisation) and the scripts that go with it (the set-up script, the backup scripts) and the sources of the proxy's image, as §Decision's `deploy/` already provides for deployment manifests. Each release publishes that directory, its tests and the proxy's sources aside, as its bundle (ADR-0022 §1).
3. **`infra/` holds the developer environment**, today `infra/dev/`: the dev stack's compose, its initialisation and its checks. It is not a deployment manifest: it builds the services from source and serves the test suites, so it stays out of `deploy/`. This records its addition against §Criteria: it holds the developer environment, which `deploy/`'s deployment manifests are not, and more than three files already sit within it, under `infra/dev/`. A2 rejected grouping the whole repository by layer, not a directory for this environment.
4. **One release tag for the product, `vX.Y.Z`**, covering the server's images, the bundle and the agent's executable, while the agent and the server are released together; ADR-0004's `min_supported_agent_version` lets them part later. Separate tags per component, as §Consequences, *Neutral*, suggested, wait until the two ship on different cadences.

**Effect.** §Decision's tree gains `infra/`; its root list loses the two compose files. §Alternatives considered: none (A2 rejected grouping by layer, not a directory for the developer environment). §Compliance: unchanged. §Consequences, *Neutral*: its separate tags per component wait, as item 4 says. `deploy/docker/README.md`, `infra/dev/README.md` and `infra/dev/.env.example`, which name the root files or an `infra/prod/` that will not exist, and `deploy/README.md`, whose "local" deployment item 3 gives to `infra/`, are corrected with the code that implements SPEC-023. Proposed by Claude Code on 2026-10-19 under Manuel's delegation (ADR-0022 §Decision record), and ratified by Manuel on 2026-10-23 with ADR-0022, in chat: "Sí, ratifico". Status stays `Accepted`.
