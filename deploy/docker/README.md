# CyberGuard bundle

The single-node production deployment (SPEC-023, ADR-0022): this directory is what a release
publishes as `cyberguard-deploy-<version>.tar.gz`, its images pinned by digest. The full
procedure is `docs/operations/install.md` at the release's tag
(`https://github.com/AlmanInDaHouse/CyberGuard_Enterprise/blob/v<version>/docs/operations/install.md`).

In brief, as root, in this directory, on a host with Docker Engine and its Compose plugin:

1. `./setup.sh` writes `.env` from `.env.example` and stops.
2. Set `CG_DASHBOARD_HOSTNAME`, `CG_AGENT_SERVER_NAMES` and `CG_TLS_MODE` in `.env`; in `provided` mode, put the dashboard's certificate and key in `tls/`.
3. `./setup.sh` again fills the secrets. Keep a copy of `.env` apart from the backups: it holds the keys to the CA, the TOTP secrets and the forensic key.
4. `docker compose up -d --wait`.
5. `docker compose exec api node dist/cli/create-user.js --email <admin's address>`, the password typed at its prompt, the TOTP factor confirmed with a code.

| Path | What it is |
|---|---|
| `docker-compose.yml` | The production compose: the proxy, the dashboard, the api, ingest, the three stores and the two migration one-shots. |
| `.env.example` | The bundle variables with their defaults, the secrets empty. |
| `setup.sh` | Fills `.env` (SPEC-023 §Operational §3). |
| `postgres/` | The database's roles, schemas and logging settings, run once on an empty volume. |
| `clickhouse/` | The server's settings (its own logs' period, the backup disk) and its users, grants and row policies, run once on an empty volume. |
| `redis/` | The key-value store's entrypoint, which writes its ACL file from `.env`. |
| `relay/` | The SMTP relay's private CA, when it has one. |
| `tls/` | The dashboard's certificate and key, in `provided` mode. |

In the repository only: `proxy/` (the proxy image's sources), `test/` (the bundle's tests, `node --test`, which `deploy-ci.yml` runs against images built from the commit) and `docker-compose.test.yml` (the override that names those images).
