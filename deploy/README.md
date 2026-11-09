# Deployment

Deployment manifests grouped by target.

| Subfolder | Purpose | MVP scope |
|---|---|---|
| [`docker/`](docker/) | The single-node production bundle (SPEC-023): the compose, its `.env.example`, its set-up, backup and restore scripts, the stores' initialisation, and the `tls/` and `relay/` directories an operator fills. Beside it, the repository keeps the proxy image's sources, the bundle's tests and their compose override, which the published bundle leaves out. The development stack is [`infra/dev/`](../infra/dev/) (ADR-0001 Amendment 2026-10-19). | In scope. |
| [`windows/`](windows/) | The sources of the agent's Windows MSI and setup (SPEC-024), and the pins of the tools that build them. | In scope (F2). |
| [`helm/`](helm/) | Helm charts for Kubernetes. | Post-MVP placeholder. |
| [`terraform/`](terraform/) | Terraform modules for cloud bootstrapping. | Post-MVP placeholder. |

The MVP server installation path is Docker Compose only. Helm and Terraform exist as structural placeholders.
