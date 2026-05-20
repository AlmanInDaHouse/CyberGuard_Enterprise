# infra/dev — local development stack

## Purpose

Single-machine Docker Compose stack that brings up the five storage backends specified by [ADR-0003](../../docs/adr/0003-polyglot-storage.md): PostgreSQL 16 (with pgvector), ClickHouse 24.8, Redis 7.4, NATS JetStream 2.10, and MinIO. **Dev only.** Not for production. Production infra will live under `infra/prod/` once it exists.

## Prerequisites

- Docker Desktop (Docker Engine ≥ 24, Compose v2). Verified on Docker 29 + Compose v2.40.
- [Task](https://taskfile.dev) — install via `winget install Task.Task` on Windows, `brew install go-task/tap/go-task` on macOS, or per platform instructions.

## First-time setup

```sh
cp infra/dev/.env.example infra/dev/.env
task dev:up
task dev:health
```

`task dev:up` brings up all five services in detached mode. `task dev:health` returns 0 once every container reports `healthy` (typically ~30 seconds after `up`).

## Port map

| Service | Container port | Host default | Env var | Purpose |
|---|---|---|---|---|
| PostgreSQL | 5432 | 15432 | `CG_PG_PORT` | SQL connections (host port shifted to avoid clashes with other local Postgres instances) |
| ClickHouse | 8123 | 8123 | `CG_CH_HTTP_PORT` | HTTP interface |
| ClickHouse | 9000 | 9000 | `CG_CH_TCP_PORT` | Native TCP |
| Redis | 6379 | 16379 | `CG_REDIS_PORT` | Client connections (host port shifted to avoid clashes with other local Redis instances) |
| NATS | 4222 | 4222 | `CG_NATS_CLIENT_PORT` | Client connections |
| NATS | 8222 | 8222 | `CG_NATS_MONITOR_PORT` | HTTP monitoring |
| MinIO | 9000 | 9001 | `CG_MINIO_API_PORT` | S3 API (host 9001 to avoid clash with ClickHouse 9000) |
| MinIO | 9001 | 9002 | `CG_MINIO_CONSOLE_PORT` | Web console |

## Connection strings (defaults from `.env.example`)

```sh
# PostgreSQL — psql client
PGPASSWORD=cyberguard_dev psql -h localhost -p 15432 -U cyberguard -d cyberguard

# ClickHouse — clickhouse-client
clickhouse-client --host localhost --port 9000 --user cyberguard --password cyberguard_dev --database cyberguard

# Redis — redis-cli
redis-cli -h localhost -p 16379 -a cyberguard_dev

# NATS — nats CLI (https://github.com/nats-io/natscli)
nats --server nats://localhost:4222 stream list

# MinIO — mc client (https://min.io/docs/minio/linux/reference/minio-mc.html)
mc alias set cgdev http://localhost:9001 cyberguard cyberguard_dev_minio_root
mc admin info cgdev
```

## Daily use

| Command | What it does |
|---|---|
| `task dev:up` | Bring all five services up (detached). |
| `task dev:ps` | Show container status. |
| `task dev:health` | Print healthcheck status per service; exit non-zero if any is not `healthy`. |
| `task dev:logs` | Follow logs for all services (last 100 lines tail). |
| `task dev:logs svc=postgres` | Follow logs for a single service. Use the service name from the compose file. |
| `task dev:down` | Stop and remove containers. **Volumes survive.** |
| `task dev:reset-data` | Destructive — stop containers AND remove named volumes. Use when you need a clean slate. |

## Troubleshooting

- **Port already in use** (`Bind for 0.0.0.0:5432 failed`). Edit `infra/dev/.env` and pick a free host port (e.g. `CG_PG_PORT=5433`). Re-run `task dev:up`.
- **Healthcheck stuck on `starting`**. Containers can take longer than the `start_period` on slow disks. Re-run `task dev:health` after a minute; if still not healthy, `task dev:logs svc=<name>` to inspect.
- **Permission denied on the named volumes**. Volumes are managed by Docker, not by the host. If a host file mounted under `./postgres/init/` is unreadable, check Windows / WSL2 file-sharing permissions in Docker Desktop.
- **Want a fully clean slate**. `task dev:reset-data` removes containers and **destroys the named volumes**, then `task dev:up` re-bootstraps Postgres extensions and MinIO root from scratch.
