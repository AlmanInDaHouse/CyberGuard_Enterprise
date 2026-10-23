# infra/dev — local development stack

## Purpose

Single-machine Docker Compose stack that brings up the three storage backends of [ADR-0003](../../docs/adr/0003-polyglot-storage.md) the code uses, PostgreSQL 16 (with pgvector), ClickHouse 24.8 and the key-value store, Valkey 9.1 (a BSD-3-Clause fork of Redis 7.2.4 that speaks its protocol; ADR-0003 Amendment 2026-10-23), which the compose names `redis`, and the ingest and api services built from the repository. ADR-0003's other two, NATS JetStream and MinIO, are left out until a SPEC uses them (debt #96): nothing reads or writes them, and the stack ran NATS without authentication and MinIO with the compose file's dev root credentials. **Dev only.** Not for production. Production infra will live under `infra/prod/` once it exists.

## Prerequisites

- Docker Desktop (Docker Engine ≥ 24, Compose v2). Verified on Docker 29 + Compose v2.40.
- [Task](https://taskfile.dev) — install via `winget install Task.Task` on Windows, `brew install go-task/tap/go-task` on macOS, or per platform instructions.

## First-time setup

```sh
cp infra/dev/.env.example infra/dev/.env
task dev:up
task dev:health
```

`task dev:up` brings up the stores, ingest and the api in detached mode. `task dev:health` returns 0 once the three stores report `healthy` (typically ~30 seconds after `up`).

## Port map

| Service | Container port | Host default | Env var | Purpose |
|---|---|---|---|---|
| PostgreSQL | 5432 | 15432 | `CG_PG_PORT` | SQL connections (host port shifted to avoid clashes with other local Postgres instances) |
| ClickHouse | 8123 | 8123 | `CG_CH_HTTP_PORT` | HTTP interface |
| ClickHouse | 9000 | 9000 | `CG_CH_TCP_PORT` | Native TCP |
| Valkey (`redis`) | 6379 | 16379 | `CG_REDIS_PORT` | Client connections (host port shifted to avoid clashes with other local Redis or Valkey instances) |
| ingest | 8080 | 8080 | `CG_INGEST_ENROLL_PORT` | Agent enrollment, plain HTTP (SPEC-004) |
| ingest | 8443 | 8443 | `CG_INGEST_HEARTBEAT_PORT` | The agents' mTLS listener |
| api | 8081 | 18081 | `CG_API_PORT` | The human-facing API (SPEC-008, SPEC-009) |

## Connection strings (defaults from `.env.example`)

```sh
# PostgreSQL — psql client
PGPASSWORD=cyberguard_dev psql -h 127.0.0.1 -p 15432 -U cyberguard -d cyberguard

# ClickHouse — clickhouse-client
clickhouse-client --host 127.0.0.1 --port 9000 --user cyberguard --password cyberguard_dev --database cyberguard

# Valkey — valkey-cli (redis-cli speaks the same protocol); the password from the
# environment, off the command line
VALKEYCLI_AUTH=cyberguard_dev valkey-cli -h 127.0.0.1 -p 16379
```

## Daily use

| Command | What it does |
|---|---|
| `task dev:up` | Bring the stack up (detached). |
| `task dev:ps` | Show container status. |
| `task dev:health` | Print the three stores' healthcheck status; exit non-zero if any is not `healthy`. |
| `task dev:logs` | Follow logs for all services (last 100 lines tail). |
| `task dev:logs svc=postgres` | Follow logs for a single service. Use the service name from the compose file. |
| `task dev:down` | Stop and remove containers. **Volumes survive.** |
| `task dev:reset-data` | Destructive — stop containers AND remove named volumes. Use when you need a clean slate. |

## Troubleshooting

- **Port already in use** (`Bind for 127.0.0.1:15432 failed`). Edit `infra/dev/.env` and pick a free host port (e.g. `CG_PG_PORT=5433`). Re-run `task dev:up`.
- **Only this machine reaches the stack.** Every port is published on `127.0.0.1` alone, so the stores and their dev passwords are not on the network. Connect with `127.0.0.1`, not `localhost`: on Windows `localhost` tries `::1` first, which the stack does not serve, and a refused connection costs about two seconds. An agent on another machine would need the mappings edited and a heartbeat certificate for the host name it dials: the self-issued one covers `localhost` and `127.0.0.1` only (SPEC-004 Amendment 2026-09-21).
- **CI checks the compose.** A change to it runs `infra-ci` (`.github/workflows/infra-ci.yml`), which renders it with `.env.example` and every profile, and runs `check-compose.mjs`: every port on `127.0.0.1`, no service on the host's network, every image named by a digest or by a tag other than `latest`, ClickHouse on the same patch release as the test backends, and the key-value store on Valkey's image, pinned to a patch release and the same in the test backends, with no image of the redis repository (SPEC-023 deploy_ac_022). `check-compose.test.mjs` tests the check itself. To run it locally, write the render outside the repository, since it holds the dev passwords: `docker compose -f infra/dev/docker-compose.dev.yml --env-file infra/dev/.env.example --profile '*' config --format json > "${TMPDIR:-/tmp}/compose.json"`, then `node infra/dev/check-compose.mjs "${TMPDIR:-/tmp}/compose.json" services/ingest/test/helpers/backends.ts services/api/test/helpers/backends.ts`.
- **Healthcheck stuck on `starting`**. Containers can take longer than the `start_period` on slow disks. Re-run `task dev:health` after a minute; if still not healthy, `task dev:logs svc=<name>` to inspect.
- **Permission denied on the named volumes**. Volumes are managed by Docker, not by the host. If a host file mounted under `./postgres/init/` is unreadable, check Windows / WSL2 file-sharing permissions in Docker Desktop.
- **Want a fully clean slate**. `task dev:reset-data` removes containers and **destroys the named volumes**, then `task dev:up` re-bootstraps the Postgres extensions from scratch.
- **A Redis volume left from an older stack.** The stack ran Redis 7.4 until SPEC-023; Valkey may not load the data files Redis 7.4 wrote. Remove the volume once: `task dev:down`, `docker volume rm cyberguard-dev_cg_redis_data`, then `task dev:up`. It holds sessions, rate-limit counters, TOTP replay keys and nonces alone (ADR-0003 Rule 5): users sign in again.
- **NATS and MinIO containers left from an older stack.** A stack brought up before debt #96 keeps their containers as orphans of the project; `task dev:up` and `task dev:down` pass `--remove-orphans`, which removes them. Their volumes (`cyberguard-dev_cg_nats_data`, `cyberguard-dev_cg_minio_data`) stay until `docker volume rm` removes them. A container created before debt #18 publishes its ports on every interface; `task dev:up` recreates it from the compose, on `127.0.0.1`, keeping its volume.
