# CyberGuard

Self-deployable enterprise SOC/XDR platform.

CyberGuard is composed of two main components:

- **CyberGuard Server** — central platform providing SIEM, SOAR, XDR/EDR, UEBA, forensic engine, SOC dashboard, agent management, secure login with OTP/MFA, RBAC, audit and observability.
- **CyberGuard Agent** — endpoint program that sends telemetry (processes, network, files, users, logs, configuration) and heartbeat to the server through an encrypted channel.

The product promise is to deploy a functional, self-hosted, auditable SOC in under 30 minutes, with real detections from day one and an exportable forensic report at the first incident.

## Repository status

Bootstrap phase. Only scaffolding and meta-files. See [`docs/specs/`](docs/specs/) for incoming specifications and [`docs/adr/`](docs/adr/) for architectural decisions.

## Layout

| Path | Purpose |
|---|---|
| [`docs/`](docs/) | Specifications (SPEC-XXX), architecture decisions (ADR-NNNN), threat model, runbook. |
| [`schemas/`](schemas/) | CyberGuard Common Event Schema (CGES) and OpenAPI contracts. |
| [`services/`](services/) | Server-side services (api, ingest, pipeline, soar, ml, forensic). |
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

Available targets (currently empty stubs, populated by incoming SPECs):

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
