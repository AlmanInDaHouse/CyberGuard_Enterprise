# Dashboard

Next.js 15 (App Router) SOC dashboard.

Populated by SPEC-XXX-dashboard. Until then this folder is a placeholder.

Expected stack:

- Next.js 15 (App Router, RSC).
- Tailwind CSS.
- shadcn/ui component primitives.
- TanStack Query for data fetching.
- Recharts for visualization.

Expected views (per MVP scope):

- Alerts list with WebSocket updates.
- Incident detail with grouped alerts and MITRE mapping.
- Agents inventory and enrollment (token issuance).
- Cases (notes, evidence, assignment).
- Playbook executions (read-only in MVP).
- Forensic report viewer and PDF export.

## Configuration

- `API_BASE_URL`: the base URL of the read API (`services/api`), read on the server side. When unset it is `http://127.0.0.1:8081`, the api's own default listener (`API_BIND_HOST` and `API_PORT`). Against the dev stack, whose compose publishes the api on host port `CG_API_PORT` (18081 in `infra/dev/.env.example`), set `API_BASE_URL=http://127.0.0.1:18081`.
