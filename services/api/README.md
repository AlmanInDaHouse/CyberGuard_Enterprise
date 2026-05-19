# services/api

TypeScript BFF built on Fastify with Zod schema validation.

Populated by SPEC-XXX-api-surface. Until then this folder is a placeholder.

Expected responsibilities:

- HTTP surface for the dashboard.
- Authentication (OAuth2/OIDC + TOTP).
- Authorization via RBAC (admin, analyst, viewer).
- Translation between dashboard requests and internal services (pipeline, soar, forensic).
- WebSocket push of new alerts to the dashboard.
