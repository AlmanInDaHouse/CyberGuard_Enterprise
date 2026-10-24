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

- `API_BASE_URL`: the base URL of the read API (`services/api`), read on the server side. When unset it is `http://127.0.0.1:8081`, the api's own default listener (`API_BIND_HOST` and `API_PORT`). Against the dev stack, whose compose publishes the api on host port `CG_API_PORT` (18081 in `infra/dev/.env.example`), set `API_BASE_URL=http://127.0.0.1:18081`. Set, it must be an `http:` or `https:` URL, not the empty string: the server refuses to start otherwise, with `cg-dashboard: fatal: invalid configuration: API_BASE_URL: <reason>` (`instrumentation.ts`, SPEC-023 §Operational §2).

## Security headers, cookies and health

- `middleware.ts` sets on every response a Content Security Policy with a nonce new per response, `X-Content-Type-Options: nosniff`, `Referrer-Policy: same-origin` and a `Permissions-Policy`; `next dev` sets no CSP. Every page renders at request time (`app/layout.tsx`), so Next puts the nonce on its scripts (SPEC-023 §Operational §7).
- The browser's cookies are `__Host-cgsess` and `__Host-cgcsrf`; the api's own session cookie keeps the name `cgsess`, which the dashboard sends it.
- `GET /api/health` answers `200 { "status": "ok" }`, calling nothing.
