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
| 3 | OTP login + RBAC (3 roles) | **Delivered** in the api (SPEC-008, its Amendment 2026-10-15: strict input, org scope, account disable, audit). The dashboard supports login/logout; TOTP enrollment and user management remain F3 work (#83, #98). |
| 4 | Gmail/SMTP notification | **Delivered** (SPEC-014). |
| 5 | Incident PDF export | **Delivered** (SPEC-013). |
| 1 | 10 detection rules | **Delivered** (SPEC-016): ten rules, each with a wired fixture and a CI scenario, evaluated over the events a deployed agent captures (SPEC-017). |
| 2 | Windows agent: processes / network / logins | **Delivered** — processes (SPEC-005), TCP connections opened (SPEC-019) and logons succeeded and failed (SPEC-020), captured on the agent's normal run path (SPEC-017). |
| 6 | 1 SOAR playbook | **Delivered** (SPEC-022): `pb_high_severity_containment` runs on each new incident of severity High or more from a rule, records the agent's isolation in dry-run and emails the SOC, with a write-once, hash-chained execution record read at `GET /v1/incidents/:id/playbook-executions` (ADR-0021). |
| 7 | Installation docs (< 30 min) | **Pending** — F1's server bundle, release dry run and operations runbooks with executable server-command tests have landed (SPEC-023, S44–S48). F2–F4, endpoint validation, first-release artifact verification and a timed fresh-host installation remain. [Release tags are frozen](docs/security/release-protection.md) pending the release decision. |

On the server side, criteria **1 / 4** run in a standing stack (detection → incident → notify), driven by the in-process detection driver (ADR-0012 Amendment 2026-06-07), over events read in arrival order (SPEC-018); on the agent side, process, network and logon capture run on the agent's normal secure path (SPEC-017). The latest session handoff, [`docs/handoff-session-48.md`](docs/handoff-session-48.md), is the canonical current state.

The remaining MVP work — phases ordered by technical dependency, plus the owner-STOP decisions gating them — is the work order in [`docs/product/roadmap.md`](docs/product/roadmap.md).

## Layout

| Path | Purpose |
|---|---|
| [`docs/`](docs/) | Specifications (SPEC-XXX), architecture decisions (ADR-NNNN), threat model, runbook. |
| [`schemas/`](schemas/) | CyberGuard Common Event Schema (CGES) and OpenAPI contracts. |
| [`services/`](services/) | Server-side services. Built: `api`, `ingest` (TypeScript). Placeholders: `pipeline`, `ml`. Forensic ships inside `services/api` (SPEC-013) and SOAR inside `services/ingest` (SPEC-022); `services/soar/` holds only its README. |
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
