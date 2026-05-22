# cg-ingest — server ingest (SPEC-004)

TypeScript + Fastify service (per [ADR-0007](../../docs/adr/0007-ingest-language-typescript-mvp.md))
that terminates the agent protocol and persists:

- `POST /v1/agents/enroll` (plain HTTP) — validates a single-use token,
  issues an Ed25519 client certificate, persists the agent. SPEC-002 contract.
- `POST /v1/agents/heartbeat` (mTLS) — validates the signed outer envelope
  (signature, nonce, timestamp) and persists the heartbeat. SPEC-003 contract.
- `GET /health` — liveness.

Persistence: agents/tokens/CA in **Postgres**, heartbeats in **ClickHouse**,
anti-replay nonces in **Redis**. Full design: [SPEC-004](../../docs/specs/SPEC-004-server-ingest-minimal.md).

> **Scaffold status (B3).** The two agent routes are `501` placeholders and
> the token CLI is a stub; the implementation lands in the B5 commit. The
> Fastify skeleton, config, DB schema/migrations, Dockerfile, and CI are in
> place.

## Toolchain

- Node.js 22 LTS, pnpm (via `corepack enable`).

## Local development

```sh
pnpm install
pnpm run typecheck      # tsc --noEmit (strict)
pnpm run lint           # biome
pnpm test               # vitest
pnpm run build          # tsc -> dist/
```

The service expects the `task dev:up` backends. Connection strings and the
two ports are configured via the `INGEST_*` environment variables documented
in SPEC-004 §Configuration. With Docker, `task dev:up` builds and runs this
service alongside the backends (see `infra/dev/docker-compose.dev.yml`).

## Migrations

```sh
pnpm run migrate        # apply Postgres migrations + bootstrap ClickHouse
```

Migrations are single-applier: a dedicated `migrate` run, or one instance
started with `INGEST_RUN_MIGRATIONS=true`. A Postgres advisory lock
serialises concurrent application (SPEC-004 MUST-2).

## Issuing an enrollment token

```sh
pnpm run issue-token -- --org default     # prints the opaque token once
```

(Implemented in B5; FR-015.)
