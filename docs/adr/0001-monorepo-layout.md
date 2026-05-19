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

```
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
