# CyberGuard

Self-deployable enterprise SOC/XDR platform.

CyberGuard is composed of two main components:

- **CyberGuard Server** — central platform providing SIEM, SOAR, XDR/EDR, UEBA, forensic engine, SOC dashboard, agent management, secure login with OTP/MFA, RBAC, audit and observability.
- **CyberGuard Agent** — endpoint program that sends telemetry (processes, network, files, users, logs, configuration) and heartbeat to the server through an encrypted channel.

The product promise is to deploy a functional, self-hosted, auditable SOC in under 30 minutes, with real detections from day one and an exportable forensic report at the first incident.

## Status

Active development, past the bootstrap phase. Built and in the tree: the Rust endpoint agent (`agent/`), the TypeScript `ingest` and `api` services (`services/`), the Next.js SOC dashboard (`dashboard/`), the detection engine, incident grouping, the forensic hash-chain and PDF report, and email notification. See [`docs/adr/`](docs/adr/) for the accepted architecture decisions and [`docs/specs/`](docs/specs/) for the accepted specifications.

### MVP scorecard (Blueprint §18)

| # | Criterion | State |
|---|---|---|
| 3 | OTP login + RBAC (3 roles) | **Delivered** (SPEC-008). |
| 4 | Gmail/SMTP notification | **Delivered** (SPEC-014). |
| 5 | Incident PDF export | **Delivered** (SPEC-013). |
| 1 | 10 detection rules | **Partial 1/10** (SPEC-006). |
| 2 | Windows agent: processes / network / logins | **Partial 1/3** — processes only (SPEC-005). |
| 6 | 1 SOAR playbook | **Pending** — unblocked by the prod-driver seam. |
| 7 | Installation docs (< 30 min) | **Pending** — owner-STOP deployment contract. |

Criteria **1 / 2 / 4** now run in a standing stack (detection → incident → notify), driven by the in-process detection driver (ADR-0012 Amendment 2026-06-07). The latest session handoff, [`docs/handoff-session-29.md`](docs/handoff-session-29.md), is the canonical current state.

The remaining MVP work — phases ordered by technical dependency, plus the owner-STOP decisions gating them — is the work order in [`docs/product/roadmap.md`](docs/product/roadmap.md).

## Layout

| Path | Purpose |
|---|---|
| [`docs/`](docs/) | Specifications (SPEC-XXX), architecture decisions (ADR-NNNN), threat model, runbook. |
| [`schemas/`](schemas/) | CyberGuard Common Event Schema (CGES) and OpenAPI contracts. |
| [`services/`](services/) | Server-side services. Built: `api`, `ingest` (TypeScript). Placeholders: `pipeline`, `soar`, `ml`. Forensic ships inside `services/api` (SPEC-013). |
| [`agent/`](agent/) | Rust workspace for the endpoint agent. |
| [`dashboard/`](dashboard/) | Next.js 15 SOC dashboard. |
| [`rules/`](rules/) | Sigma-compatible detection rules and per-rule tests. |
| [`playbooks/`](playbooks/) | SOAR playbooks (YAML) and tests. |
| [`harness/`](harness/) | Scenario-based harness that validates detections, pipelines and reports end-to-end. |
| [`deploy/`](deploy/) | Deployment manifests (docker, helm, terraform). |

## Tooling

The project uses [Task](https://taskfile.dev) as a cross-platform task runner. Install:

- **Windows (winget):** `winget install Task.Task`
- **Windows (scoop):** `scoop install task`
- **macOS / Linux:** see <https://taskfile.dev/installation/>

The top-level lifecycle targets below are still stubs pending the infrastructure SPEC. The working developer stack runs via `task dev:*` (see [`infra/dev/`](infra/dev/)) and per-workspace `cargo` / `pnpm` commands:

| Target | Purpose |
|---|---|
| `task bootstrap` | Initialize a developer environment. |
| `task up` | Bring up local stack via docker compose. |
| `task down` | Tear down local stack. |
| `task test` | Run unit and integration tests across services. |
| `task harness` | Run end-to-end harness scenarios. |
| `task lint` | Lint code, schemas, rules and docs. |
| `task reset-data` | Wipe local data volumes. |

## License

Apache License 2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).

## Security

To report a vulnerability, see [SECURITY.md](SECURITY.md).
