# Configuration

This document is the reference for a CyberGuard server's configuration: the host it runs on, the variables of the bundle's `.env`, the service variables the compose derives from them, the checks that refuse a malformed value, the secrets, the project name and the networks, the TLS files, the logs and the proxy's answers (SPEC-023 §Data contracts §3). [install.md](install.md) applies it to a first installation; [upgrade.md](upgrade.md), [backup-and-restore.md](backup-and-restore.md) and [data-protection.md](data-protection.md) refer to it.

Commands run as root, in the bundle's directory, the one that holds `docker-compose.yml` and `.env`, unless a section says otherwise.

## The host

- One Linux x86-64 host. The images are built for `linux/amd64`.
- Rootful Docker Engine **29.1.2 or later**, with Compose plugin **2.40.3 or later** (`docker compose`), on a maintained, patched release. These are the support floors, not a recommendation to keep an old patch indefinitely. Docker Desktop and nested developer daemons are separate test environments, not production-host certification.
- `gpg` 2.2.7 or later, which `backup.sh` and `restore.sh` run.
- A POSIX `sh`, and root. `setup.sh` uses `cp`, `mv`, `od`, `tr`, `sed`, `grep`, `chmod` and `chown`; in `provided` mode it gives the dashboard's key to the proxy's user, which takes root. `.env` is root's, mode `0600`, so `docker compose` and the bundle's scripts run as root too.

### Ports to open

The compose publishes four ports, each on `CG_BIND_ADDRESS`, and no other: the api, the dashboard and the stores publish none.

| Port (default) | Variable | Protocol | Clients | In the container |
|---|---|---|---|---|
| 443 | `CG_HTTPS_PORT` | TCP, HTTPS | Browsers | the proxy's 8443 |
| 80 | `CG_HTTP_PORT` | TCP, HTTP, a redirect to HTTPS | Browsers | the proxy's 8080 |
| 8444 | `CG_ENROLL_PORT` | TCP, HTTPS | Agents enrolling | ingest's 8080 |
| 8443 | `CG_HEARTBEAT_PORT` | TCP, mutual TLS | Enrolled agents | ingest's 8443 |

Open the proxy's two ports to the browsers that use the dashboard, and ingest's two to the endpoints whose agents report to the server. The proxy serves no HTTP/3 and listens on no UDP port.

Agents connect to ingest's ports directly. Ingest terminates their TLS itself, and its heartbeat listener authenticates each agent by its client certificate: put no proxy that terminates TLS in front of these two ports.

When notification is on, ingest connects to the relay, `CG_SMTP_HOST` on `CG_SMTP_PORT`. The api, the dashboard and the stores sit on internal networks alone, with no route out of the host.

### Client addresses and the Docker daemon

The api limits sign-in attempts per client address and writes that address into the audit log. The address it takes is the one the proxy's connection came from: the proxy sets `X-Forwarded-For` to it and ignores the header a client sends (SPEC-023 §Operational §5). If a proxy or source NAT replaces several clients' addresses with one, they share that address's limiter.

Follow [install.md's daemon settings and external-client check](install.md#start-the-stack): merge `userland-proxy: false`, `ip-forward: true` and `iptables: true` into the rootful daemon's existing configuration and plan its restart. Keep Docker's forwarding and firewall rules. Use a direct IPv4 route to an IPv4 published address, with no intermediary that replaces the source. Do not infer address preservation for IPv6-to-IPv4 conversion, hairpin traffic or Docker Desktop from that setting. Check the actual production route from a separate client before accepting traffic; the host's own loopback is not that check.

Before the server takes users, sign in from a client machine and check, in the proxy's access log, that the request carries that machine's address. The log holds one JSON object per request, and `request.remote_ip` is the address the request came from:

```sh
docker compose logs --no-color --no-log-prefix proxy
```

## The environment file

`.env` holds every bundle variable. The compose reads it, and so do `setup.sh`, `backup.sh` and `restore.sh`. The first run of `setup.sh` writes it from `.env.example`, with mode `0600`, and stops ([install.md](install.md)). Each later run:

- adds each variable `.env.example` has and `.env` lacks, with its default, as a newer bundle brings at an upgrade;
- fills each empty secret ([Secrets](#secrets));
- keeps every value already set, and every other line, a `COMPOSE_PROJECT_NAME` line among them;
- sets the file's mode to `0600`;
- prints the names of the variables it added or filled, never a value.

It refuses to run, changing nothing, while `CG_DASHBOARD_HOSTNAME` or `CG_AGENT_SERVER_NAMES` is empty, or while `CG_TLS_MODE` is `provided` or empty and `tls/dashboard.crt` or `tls/dashboard.key` is missing; it then prints `setup.sh: missing:` followed by what is missing. Once the secrets are filled, a further run changes nothing.

### Its format

The scripts read a variable from the last line that starts with its name and `=`, take everything after the `=` to the end of that line, and remove one pair of surrounding double or single quotes. Write `.env` as plain `KEY=value` lines:

- one line per variable, at the start of the line, with no `export` and no space around `=`;
- no comment after a value; a comment is a line of its own that starts with `#`;
- no quotes: no value the template holds needs them.

```dotenv
CG_DASHBOARD_HOSTNAME=<the dashboard's DNS name>
CG_TLS_MODE=provided
CG_AGENT_SERVER_NAMES=<a DNS name agents use>,<an IP address agents use>
```

### Applying a change

A change to `.env` takes effect at the next `docker compose up`, which runs the two migration one-shots again and starts the services with the new values:

```sh
docker compose up -d --wait
```

The exceptions:

- A generated secret is not changed after the first `docker compose up` ([Secrets](#secrets)).
- `COMPOSE_PROJECT_NAME` is set before the first `docker compose up` ([The project name](#the-project-name)).
- The `app` network's three variables take `docker compose down`, then `docker compose up -d --wait` ([The `app` network's addresses](#the-app-networks-addresses)).
- `CG_BACKUP_DIR` is read by `backup.sh`, and `CG_BACKUP_PASSPHRASE` by `backup.sh` and `restore.sh`, at each run; no container reads either.

## Bundle variables

Left empty, a variable of the tables below takes its default, except the two hostnames, which are required, and the secrets, which `setup.sh` fills. The arrow names the service variable the compose sets from a bundle variable ([Service variables](#service-variables)).

### Names and TLS

| Variable | Default | Meaning |
|---|---|---|
| `CG_DASHBOARD_HOSTNAME` | none, required | The DNS name users reach the dashboard by: the proxy's site address. In `provided` mode the certificate in `tls/` names it. |
| `CG_TLS_MODE` | `provided` | `provided`: the proxy serves `tls/dashboard.crt` and `tls/dashboard.key`. `internal`: the proxy issues a certificate from its own CA, for an evaluation ([The dashboard's certificate](#the-dashboards-certificate)). |
| `CG_AGENT_SERVER_NAMES` | none, required | Comma-separated DNS names and IP addresses agents reach the server by, from 1 to 20: the names of the server certificate ingest issues itself (→ `INGEST_SERVER_NAMES`). |

### Published ports

| Variable | Default | Meaning |
|---|---|---|
| `CG_BIND_ADDRESS` | `0.0.0.0` | The host address the four published ports bind. |
| `CG_HTTPS_PORT` | `443` | The proxy's HTTPS port. The HTTP port's redirect names it. |
| `CG_HTTP_PORT` | `80` | The proxy's HTTP port, which redirects every request to HTTPS. |
| `CG_ENROLL_PORT` | `8444` | Ingest's enrollment port. |
| `CG_HEARTBEAT_PORT` | `8443` | Ingest's heartbeat port. |

### The `app` network

| Variable | Default | Meaning |
|---|---|---|
| `CG_APP_SUBNET` | `10.254.254.0/28` | The `app` network's subnet. |
| `CG_APP_IP_RANGE` | `10.254.254.8/29` | The addresses Docker hands out on `app`, inside the subnet. |
| `CG_DASHBOARD_APP_ADDRESS` | `10.254.254.2` | The dashboard's address on `app`, inside the subnet and outside the range (→ `API_TRUSTED_PROXIES`). |

### Notification

| Variable | Default | Meaning |
|---|---|---|
| `CG_SMTP_HOST` | empty | The SMTP relay's host. |
| `CG_SMTP_PORT` | empty | The relay's port. On 465 the connection is TLS from its start; on another port the send uses STARTTLS when the relay offers it. |
| `CG_SMTP_USER`, `CG_SMTP_PASS` | empty | The account the send authenticates as, used only when the relay asks for PLAIN, LOGIN or CRAM-MD5, and only over TLS. |
| `CG_SMTP_FROM` | empty | The sender address the email is sent as. |
| `CG_NOTIFY_RECIPIENT` | empty | The mailbox an email goes to when an incident is created, and the recipient of the playbooks' `notify.email` steps. |
| `CG_SMTP_CA_FILE` | empty | The name of a PEM file in the bundle's `relay/` directory holding the relay's private CA (→ `INGEST_SMTP_CA_FILE`; [The relay's CA](#the-relays-ca)). |

Set all six of `CG_SMTP_HOST`, `CG_SMTP_PORT`, `CG_SMTP_USER`, `CG_SMTP_PASS`, `CG_SMTP_FROM` and `CG_NOTIFY_RECIPIENT`, or none: with none, notification is off, and detection runs. An empty value counts as unset.

The send authenticates only when the relay's EHLO reply names PLAIN, LOGIN or CRAM-MD5. It gives its credentials only over TLS: a relay that asks for them on a connection in clear gets none, and the send fails with the code `ENOTLS`. A relay whose reply names none of the three receives the email without credentials; on a port other than 465, from a relay that offers no STARTTLS, the email travels in clear. A failed send is logged at `warn` by ingest, `incident notification send failed`, with its code and the incident's id, and does not stop detection.

### Backups

| Variable | Default | Meaning |
|---|---|---|
| `CG_BACKUP_DIR` | `backups` | The host directory `backup.sh` writes its encrypted files to, relative to the bundle's directory unless absolute; the script sets its mode to `0700` ([backup-and-restore.md](backup-and-restore.md)). |

### Retention

| Variable | Default | Meaning |
|---|---|---|
| `CG_RETENTION_PROCESS_DAYS` | `365` | Days a process event (class 1007) is kept after its arrival. |
| `CG_RETENTION_NETWORK_DAYS` | `365` | Days a network connection event (class 4001) is kept after its arrival. |
| `CG_RETENTION_LOGON_DAYS` | `365` | Days a logon event (class 3002) is kept after its arrival. |
| `CG_RETENTION_HEARTBEAT_DAYS` | `30` | Days an agent's heartbeat is kept after its arrival. |

Each is an integer from 7 to 3650. An event of another class keeps the longest of the three event periods. The `ingest-migrate` one-shot sets the periods as the tables' time-to-live at each `docker compose up`, and ClickHouse deletes a row at a merge, so a row may outlive its period by the time a merge takes.

A change takes effect when `ingest-migrate` next runs, and it is materialised: rows already stored are measured against the new period, so a shorter period deletes, at the next merges, the stored rows it no longer keeps. Materialising rewrites the parts of the events table, which on a large table takes time and I/O; the one-shot does it only when the computed TTL rules differ from the table's current rules, including a release that changes their form. Use [upgrade.md's migration sequence](upgrade.md#run-the-migrations-with-the-writers-stopped) when applying such a change. [data-protection.md](data-protection.md) gives what each class holds.

### Sessions

| Variable | Default | Meaning |
|---|---|---|
| `CG_SESSION_IDLE_MINUTES` | `480` | A session's idle lifetime, from 5 to 480 minutes (→ `API_SESSION_IDLE_MINUTES`). |
| `CG_SESSION_MAX_HOURS` | `24` | A session's cap, from 1 to 24 hours (→ `API_SESSION_MAX_HOURS`). |

The idle lifetime is not above the cap. The api reads both when it starts. From then on, every session takes the new values at its next request, sessions created before the change included: a session older than a shorter cap ends at its next request, and the new idle lifetime runs from that request.

### Detection and response

| Variable | Default | Meaning |
|---|---|---|
| `CG_DETECT_INTERVAL_MS` | `5000` | The detection driver's poll interval in milliseconds, from 0 to 2147483647 (→ `INGEST_DETECT_INTERVAL_MS`). `0` is the kill switch: ingest loads no rule and runs no detection, and its listeners open as usual. |
| `CG_SOAR_PLAYBOOKS_DIR` | empty | The directory, inside ingest's container, that SOAR reads its playbooks from (→ `INGEST_SOAR_PLAYBOOKS_DIR`; [Playbooks and rules](#playbooks-and-rules)). |

### Log level

| Variable | Default | Meaning |
|---|---|---|
| `CG_LOG_LEVEL` | `info` | Ingest's and the api's log level: `fatal`, `error`, `warn`, `info`, `debug`, `trace` or `silent` (→ `INGEST_LOG_LEVEL`, `API_LOG_LEVEL`). |

### Generated secrets

| Variable | Default | What it is |
|---|---|---|
| `CG_PG_SUPERUSER_PASSWORD` | generated | Postgres's superuser, `postgres`: the database's initialisation, `backup.sh` and `restore.sh`. |
| `CG_PG_INGEST_OWNER_PASSWORD` | generated | Postgres's `cg_ingest_owner`, which `ingest-migrate` runs ingest's migrations as. |
| `CG_PG_INGEST_PASSWORD` | generated | Postgres's `cg_ingest`, which ingest connects as. |
| `CG_PG_API_OWNER_PASSWORD` | generated | Postgres's `cg_api_owner`, which `api-migrate` runs the api's migrations as. |
| `CG_PG_API_PASSWORD` | generated | Postgres's `cg_api`, which the api connects as. |
| `CG_CH_ADMIN_PASSWORD` | generated | ClickHouse's administrator, `cg_admin`: the initialisation, `ingest-migrate`, `backup.sh` and `restore.sh`. |
| `CG_CH_INGEST_PASSWORD` | generated | ClickHouse's `cg_ingest`, which ingest connects as. |
| `CG_CH_API_PASSWORD` | generated | ClickHouse's `cg_api`, which the api connects as. |
| `CG_REDIS_INGEST_PASSWORD` | generated | The key-value store's `cg_ingest`, which ingest and the store's health check connect as. |
| `CG_REDIS_API_PASSWORD` | generated | The key-value store's `cg_api`, which the api connects as. |
| `CG_INGEST_CA_PASSPHRASE` | generated | Encrypts, in Postgres, the private key of the CA that signs the agents' certificates and ingest's own. |
| `CG_API_DB_ENC_PASSPHRASE` | generated | Encrypts, in Postgres, the users' TOTP secrets. |
| `CG_API_FORENSIC_PASSPHRASE` | generated | Encrypts, in Postgres, the forensic signing key. |
| `CG_BACKUP_PASSPHRASE` | generated | Encrypts the backup files. `backup.sh` and `restore.sh` read it; no container receives it. |

## The integers

Every integer ingest and the api check is written in ASCII decimal digits alone: a port, the poll interval, the session lifetimes and the retention periods, and so, in `.env`, `CG_SMTP_PORT`, `CG_DETECT_INTERVAL_MS`, `CG_SESSION_IDLE_MINUTES`, `CG_SESSION_MAX_HOURS` and the four `CG_RETENTION_*`. A sign, whitespace, an exponent, a fraction or a prefix such as `0x` is refused, as a value out of bounds is; leading zeros are digits (SPEC-023 Amendment 2026-10-23).

## Secrets

`setup.sh` fills each empty secret with 32 bytes from `/dev/urandom` in lowercase hexadecimal, 64 characters, and keeps a value already set. A secret an operator sets is kept too; the services' Postgres and key-value store passwords go into connection URLs without escaping, so such a value holds only characters that are safe in a URL, as hexadecimal is.

Do not change a generated secret after the first `docker compose up`. The bundle has no procedure that rotates one:

- the stores' initialisation sets the Postgres roles' and the ClickHouse users' passwords once, on an empty volume, and does not run again;
- `CG_INGEST_CA_PASSPHRASE`, `CG_API_DB_ENC_PASSPHRASE` and `CG_API_FORENSIC_PASSPHRASE` encrypt keys stored in Postgres, which no command encrypts again under another passphrase;
- a backup file decrypts only with the `CG_BACKUP_PASSPHRASE` it was written under, which `restore.sh` reads from `.env`.

`.env` is the deployment's master secret: it holds the keys to the CA, the TOTP secrets and the forensic key, which the database holds encrypted. Keep it owned by root with mode `0600`, and keep a copy of it apart from the backups ([backup-and-restore.md](backup-and-restore.md)): losing it with no copy loses all three.

## The project name

The compose names its project `cyberguard-enterprise`. Compose names the volumes and the networks after the project, `<project>_<name>`:

- the volumes: `pg_data`, `ch_data`, `ch_backups`, `redis_data`, `ingest_certs`, `proxy_data` and `proxy_config`;
- the networks: `public`, `edge`, `app`, `data` and `agents`.

The default names are therefore `cyberguard-enterprise_pg_data`, `cyberguard-enterprise_app` and so on, wherever the bundle is unpacked. A bundle unpacked in another directory under the same project name is the same installation: its `docker compose up` replaces the running containers.

An installation that needs another project name sets it in `.env`, before its first `docker compose up`, and keeps it there:

```dotenv
COMPOSE_PROJECT_NAME=<project name>
```

`setup.sh` keeps the line. Every `docker compose` command run in the bundle's directory then acts on that project, those `backup.sh` and `restore.sh` run among them, and the line moves with `.env` to a newer bundle's directory ([upgrade.md](upgrade.md)) and to a restore. Set the name in `.env`, not with `-p`: the `docker compose` commands of `backup.sh` and `restore.sh` take no `-p`, so under a name given with `-p` they act on another project.

A second installation on the same host sets, before its first `docker compose up`, its own project name, its own `app` network's addresses ([The `app` network's addresses](#the-app-networks-addresses)) and other published ports.

## The `app` network's addresses

The `app` network joins the dashboard and the api. Its subnet is `CG_APP_SUBNET` (`10.254.254.0/28`), Docker hands out the addresses of `CG_APP_IP_RANGE` (`10.254.254.8/29`) alone, and the dashboard has the fixed address `CG_DASHBOARD_APP_ADDRESS` (`10.254.254.2`). The api trusts the `X-Forwarded-For` of that address alone: the compose sets both the dashboard's address and `API_TRUSTED_PROXIES` from `CG_DASHBOARD_APP_ADDRESS`, so the two cannot disagree.

Set the three variables otherwise when the host routes a network that overlaps `CG_APP_SUBNET`, or for a second installation on the host. Keep the range inside the subnet, and the dashboard's address inside the subnet and outside the range, so that the api, which joins the network first, never takes it.

On an installed stack, a change to them takes the network down and up again. `docker compose down` removes the containers and the networks and keeps the volumes; do not add `-v`, which deletes the volumes and their data.

```sh
docker compose down
docker compose up -d --wait
```

## Service variables

The compose sets the service variables from the bundle variables. A service variable the compose leaves unset keeps the default its code gives it. The tables quote the compose's values as it writes them:

- `${CG_X:-<default>}` takes the default when `CG_X` is unset or empty;
- `${CG_X:-}` and `${CG_X}` are empty when `CG_X` is;
- `${CG_X:?run setup.sh}` makes `docker compose` refuse to start, naming `CG_X`, while `CG_X` is unset or empty;
- `${CG_SMTP_CA_FILE:+…}` is empty while `CG_SMTP_CA_FILE` is.

### Ingest

| Variable | In the production compose |
|---|---|
| `INGEST_PG_URL` | `postgres://cg_ingest:${CG_PG_INGEST_PASSWORD:?run setup.sh}@postgres:5432/cyberguard?options=-c%20search_path%3Dingest%2Cpublic` |
| `INGEST_CH_URL` | `http://clickhouse:8123` |
| `INGEST_CH_USER` | `cg_ingest` |
| `INGEST_CH_PASSWORD` | `${CG_CH_INGEST_PASSWORD:?run setup.sh}` |
| `INGEST_CH_DB` | `cyberguard` |
| `INGEST_REDIS_URL` | `redis://cg_ingest:${CG_REDIS_INGEST_PASSWORD:?run setup.sh}@redis:6379` |
| `INGEST_BIND_HOST` | `0.0.0.0` |
| `INGEST_SERVER_NAMES` | `${CG_AGENT_SERVER_NAMES:?run setup.sh}` |
| `INGEST_CA_PASSPHRASE` | `${CG_INGEST_CA_PASSPHRASE:?run setup.sh}` |
| `INGEST_RUN_MIGRATIONS` | `"false"`: the `ingest-migrate` one-shot migrates |
| `INGEST_LOG_LEVEL` | `${CG_LOG_LEVEL:-info}` |
| `INGEST_DETECT_INTERVAL_MS` | `${CG_DETECT_INTERVAL_MS:-5000}` |
| `INGEST_SOAR_PLAYBOOKS_DIR` | `${CG_SOAR_PLAYBOOKS_DIR:-}`; ingest takes an empty value as unset |
| `INGEST_RETENTION_PROCESS_DAYS` | `${CG_RETENTION_PROCESS_DAYS:-365}` |
| `INGEST_RETENTION_NETWORK_DAYS` | `${CG_RETENTION_NETWORK_DAYS:-365}` |
| `INGEST_RETENTION_LOGON_DAYS` | `${CG_RETENTION_LOGON_DAYS:-365}` |
| `INGEST_RETENTION_HEARTBEAT_DAYS` | `${CG_RETENTION_HEARTBEAT_DAYS:-30}` |
| `INGEST_SMTP_HOST` | `${CG_SMTP_HOST:-}` |
| `INGEST_SMTP_PORT` | `${CG_SMTP_PORT:-}` |
| `INGEST_SMTP_USER` | `${CG_SMTP_USER:-}` |
| `INGEST_SMTP_PASS` | `${CG_SMTP_PASS}` |
| `INGEST_SMTP_FROM` | `${CG_SMTP_FROM:-}` |
| `INGEST_NOTIFY_RECIPIENT` | `${CG_NOTIFY_RECIPIENT:-}` |
| `INGEST_SMTP_CA_FILE` | `${CG_SMTP_CA_FILE:+/run/cyberguard/relay/${CG_SMTP_CA_FILE}}` |

The compose leaves these unset, so they keep their defaults:

| Variable | Default |
|---|---|
| `INGEST_ENROLL_PORT`, `INGEST_HEARTBEAT_PORT` | `8080`, `8443` |
| `INGEST_SERVER_CERT_PATH`, `INGEST_SERVER_KEY_PATH` | `/certs/server.pem`, `/certs/server-key.pem`, in the `ingest_certs` volume |
| `INGEST_ENROLL_TLS` | `true`: enrollment over TLS |
| `INGEST_DETECT_RULES_DIR` | the image's `/app/rules/windows` |

The `ingest-migrate` one-shot has the same ClickHouse URL and database, the four retention periods and the log level. It connects to Postgres as `cg_ingest_owner`, with `${CG_PG_INGEST_OWNER_PASSWORD:?run setup.sh}`, and to ClickHouse as `cg_admin`, with `${CG_CH_ADMIN_PASSWORD:?run setup.sh}`.

### The api

| Variable | In the production compose |
|---|---|
| `API_PG_URL` | `postgres://cg_api:${CG_PG_API_PASSWORD:?run setup.sh}@postgres:5432/cyberguard?options=-c%20search_path%3Dapi%2Cpublic` |
| `API_REDIS_URL` | `redis://cg_api:${CG_REDIS_API_PASSWORD:?run setup.sh}@redis:6379` |
| `API_CH_URL` | `http://clickhouse:8123` |
| `API_CH_USER` | `cg_api` |
| `API_CH_PASSWORD` | `${CG_CH_API_PASSWORD:?run setup.sh}` |
| `API_CH_DB` | `cyberguard` |
| `API_DB_ENC_PASSPHRASE` | `${CG_API_DB_ENC_PASSPHRASE:?run setup.sh}` |
| `API_FORENSIC_PASSPHRASE` | `${CG_API_FORENSIC_PASSPHRASE:?run setup.sh}` |
| `API_BIND_HOST` | `0.0.0.0` |
| `API_RUN_MIGRATIONS` | `"false"`: the `api-migrate` one-shot migrates |
| `API_LOG_LEVEL` | `${CG_LOG_LEVEL:-info}` |
| `API_TRUSTED_PROXIES` | `${CG_DASHBOARD_APP_ADDRESS:-10.254.254.2}` |
| `API_SESSION_IDLE_MINUTES` | `${CG_SESSION_IDLE_MINUTES:-480}` |
| `API_SESSION_MAX_HOURS` | `${CG_SESSION_MAX_HOURS:-24}` |

The compose leaves `API_PORT` unset: `8081`.

The `api-migrate` one-shot has `API_PG_URL` alone, as `cg_api_owner`, with `${CG_PG_API_OWNER_PASSWORD:?run setup.sh}`. It starts when `ingest-migrate` has completed successfully, and the api when both have.

### The dashboard, the proxy and the stores

- The dashboard: `API_BASE_URL` is `http://api:8081`. `PORT` and `HOSTNAME`, Next's, keep the image's `3000` and `0.0.0.0`.
- The proxy: `CG_DASHBOARD_HOSTNAME`, `CG_TLS_MODE` and `CG_HTTPS_PORT`, which its configuration reads.
- Postgres: the superuser `postgres` with `CG_PG_SUPERUSER_PASSWORD`, the database `cyberguard`, and the four `CG_PG_*` passwords of the services' roles, which its initialisation reads.
- ClickHouse: the administrator `cg_admin` with `CG_CH_ADMIN_PASSWORD`, the database `cyberguard`, and `CG_CH_INGEST_PASSWORD` and `CG_CH_API_PASSWORD`, which its initialisation reads.
- The key-value store: `CG_REDIS_INGEST_PASSWORD` and `CG_REDIS_API_PASSWORD`, from which its entrypoint writes its users at each start.

## Start-up checks

Ingest and the api check their variables when they start. On a value they refuse, they print one line on standard error and exit 1:

```text
cg-ingest: fatal: invalid configuration: <VAR>: <reason>
cg-api: fatal: invalid configuration: <VAR>: <reason>
```

The line names the service variable and never quotes a value, except a refused `API_TRUSTED_PROXIES` entry, an address, which its reason quotes. It names the first variable refused: correct it and start again, and the next one, if any, follows. The two one-shots check the variables they use before they change anything, with the same line after their own prefix:

- `ingest-migrate` checks its Postgres and ClickHouse variables and the four retention periods: `cg-ingest migrate: fatal: invalid configuration: <VAR>: <reason>`;
- `api-migrate` checks `API_PG_URL`: `cg-api migrate: fatal: invalid configuration: API_PG_URL: <reason>`.

Read a service's line with:

```sh
docker compose logs --no-color --tail 40 <service>
```

What a bundle variable can make them refuse:

| Bundle variable | Service variable | Refused | Reason |
|---|---|---|---|
| `CG_AGENT_SERVER_NAMES` | `INGEST_SERVER_NAMES` | No entry; more than 20; an entry that is neither a DNS name of at most 253 characters nor an IPv4 or IPv6 address. An IPv6 address with a zone (`%`) and a name whose last label is all digits are refused. | `must name at least one host`, `must name at most 20 hosts`, `each entry must be a DNS name or an IP address` |
| `CG_SMTP_*`, `CG_NOTIFY_RECIPIENT` | `INGEST_SMTP_*`, `INGEST_NOTIFY_RECIPIENT` | Some of the six set and others not; the line names the first one missing | `incident notification (SMTP) is all-or-nothing: set all six INGEST_SMTP_* / INGEST_NOTIFY_RECIPIENT vars, or none` |
| `CG_SMTP_PORT` | `INGEST_SMTP_PORT` | Not an integer from 1 to 65535 | `must be an integer from 1 to 65535` |
| `CG_SMTP_CA_FILE` | `INGEST_SMTP_CA_FILE` | A file ingest cannot read; one with no PEM certificate block, or whose first one does not parse | `must name a readable file`, `must hold at least one PEM certificate` |
| `CG_RETENTION_*` | `INGEST_RETENTION_*` | Not an integer from 7 to 3650; refused by `ingest-migrate` before it sets a time-to-live, and by ingest | `must be an integer from 7 to 3650` |
| `CG_SESSION_IDLE_MINUTES` | `API_SESSION_IDLE_MINUTES` | Not an integer from 5 to 480; above the cap | `must be an integer from 5 to 480`, `must not be above API_SESSION_MAX_HOURS` |
| `CG_SESSION_MAX_HOURS` | `API_SESSION_MAX_HOURS` | Not an integer from 1 to 24 | `must be an integer from 1 to 24` |
| `CG_DASHBOARD_APP_ADDRESS` | `API_TRUSTED_PROXIES` | Not an IP address; refused by the api | `not an IP address or a CIDR range: <entry>` |
| `CG_DETECT_INTERVAL_MS` | `INGEST_DETECT_INTERVAL_MS` | Not an integer from 0 to 2147483647 | `must be an integer from 0 to 2147483647` |
| `CG_LOG_LEVEL` | `INGEST_LOG_LEVEL`, `API_LOG_LEVEL` | Not one of pino's levels | `must be one of fatal, error, warn, info, debug, trace, silent` |

Other checks at the start:

- Before `setup.sh` has filled the secrets, or while a hostname is empty, `docker compose` refuses to start, naming the variable, with `run setup.sh`.
- The proxy does not start when `CG_TLS_MODE` is neither `provided` nor `internal`.
- Ingest loads its playbooks and its detection rules before it opens a listener: a playbook or a rule that fails to load stops it with no listener open.
- The dashboard checks `API_BASE_URL` when its server starts, and on a value it refuses, empty or not an `http:` or `https:` URL, prints `cg-dashboard: fatal: invalid configuration: API_BASE_URL: <reason>` and exits 1. The production compose sets it to `http://api:8081`.

## TLS

### The dashboard's certificate

In `provided` mode, the proxy serves `tls/dashboard.crt`, PEM, the leaf first and its chain after it, and `tls/dashboard.key`. The certificate names `CG_DASHBOARD_HOSTNAME`. `setup.sh` gives the key to the proxy's user, uid 10001, with mode `0400`; the compose mounts `tls/` read-only into the proxy. To renew the pair, replace both files, give the new key to the proxy's user, and restart the proxy, which reads its configuration when it starts:

```sh
sh setup.sh
docker compose restart proxy
```

In `internal` mode, the proxy issues the dashboard's certificate from its own CA, which it keeps in the `proxy_data` volume. Browsers warn until that CA's root is installed in them. The mode is for an evaluation; production uses `provided`.

### Ingest's server certificate

By default, ingest issues its server certificate itself, from the CA that signs the agents' certificates. The certificate names each entry of `CG_AGENT_SERVER_NAMES`, its subject is `CN=` the first entry (its first 64 characters) and `O=CyberGuard self-issued`, and both of ingest's listeners serve it. At its start, ingest issues a new one when the list or its first entry has changed, when fewer than 30 days are left or the certificate has expired, or when its key does not match it. While it runs, it checks the certificate once a day and issues a new one when fewer than 30 days are left; a reissued certificate is served without a restart. Agents trust the CA, which `export-ca.js` prints ([install.md](install.md)), so a reissued certificate needs nothing on the endpoints. Keep in the list every name an agent's `server.url` and `heartbeat_url` use.

An operator may provide the pair instead:

- Place it in the `ingest_certs` volume (`<project>_ingest_certs`), which ingest mounts at `/certs`, as `server.pem` and `server-key.pem`, both owned by uid 1000, with the private key mode `0600`, before ingest's first start. Use the procedure below.
- Ingest serves any pair that is not one it issued itself as it is, and never rewrites it. It does not compare the certificate's names with `CG_AGENT_SERVER_NAMES`: the certificate names each name the agents use.
- The agents' trust anchor is then the CA that issued the pair, which the operator distributes to the endpoints, not the output of `export-ca.js`.
- Renewal is the operator's. At its start and once a day while it runs, ingest logs `server_cert_expiring`, at `warn`, when fewer than 30 days are left, and `server_cert_expired`, at `error`, from ten minutes before the expiry on. From ten minutes before the expiry, neither listener serves the certificate: each connection is closed before its handshake, and agents retry.
- Ingest reads the pair when it starts. Renewal replaces both files while ingest is stopped, using the same procedure below. If the issuing CA changes, distribute and verify the new server trust anchor on the endpoints as part of the change.

#### Placing or replacing the operator's pair

Run `setup.sh` first. Obtain an unencrypted PEM private key and the matching PEM certificate chain (leaf first) from the operator's PKI. Verify the pair, chain, validity and all DNS/IP names the agents will use before installing them. Keep a protected recovery copy of both files and their issuing CA outside the host. `backup.sh` does not save this volume.

The following commands stop ingest, create its container without starting its dependencies, and copy both files into the existing volume. Substitute the two absolute source paths. The project comes from `.env`; there is no hard-coded volume name or additional helper image. As the installation's root operator, prepare private temporary copies on the host with the required ownership and modes; `cp --archive` preserves them in the volume. The source files stay unchanged and the temporary copies are removed when the shell exits. No container runs as root or with added capabilities.

<!-- configuration: ingest-pair -->

```sh
set -e
docker compose stop ingest
docker compose up --no-start --no-deps ingest
pair_stage=$(mktemp -d)
trap 'rm -f "$pair_stage/server.pem" "$pair_stage/server-key.pem"; rmdir "$pair_stage"' 0
install -o 1000 -g 1000 -m 0644 '<server-cert.pem>' "$pair_stage/server.pem"
install -o 1000 -g 1000 -m 0600 '<server-key.pem>' "$pair_stage/server-key.pem"
docker compose cp --archive "$pair_stage/server.pem" ingest:/certs/server.pem
docker compose cp --archive "$pair_stage/server-key.pem" ingest:/certs/server-key.pem
```

Do not start ingest if either copy or permission change fails: it must never run with a half-replaced pair. For a fresh installation, continue with [install.md](install.md); for a restore, run `restore.sh` next, before any whole-stack `up`. For renewal on a stack whose stores are already healthy, start the stopped service and wait for health:

<!-- configuration: ingest-renew -->

```sh
docker compose up -d --wait --no-deps ingest
```

Check an agent connection using the operator's issuing CA. The internal CA exported by `export-ca.js` still issues agent client certificates; it is not the server trust anchor for this externally issued pair.

### The relay's CA

When the SMTP relay's certificate is issued by a private CA, put that CA's PEM file in the bundle's `relay/` directory, readable by uid 1000, and set `CG_SMTP_CA_FILE` to its name. The compose mounts `relay/` read-only into ingest at `/run/cyberguard/relay`, whether or not the variable is set. With the variable set, the send trusts that file's certificates for the relay, and not Node's built-in roots.

A send whose relay certificate fails verification records the verification's code as its failure code, in ingest's log line and in a SOAR step's error (`send failed: <code>`). The code points to the remedy:

- `UNABLE_TO_GET_ISSUER_CERT_LOCALLY`, `UNABLE_TO_VERIFY_LEAF_SIGNATURE`, `SELF_SIGNED_CERT_IN_CHAIN` or `DEPTH_ZERO_SELF_SIGNED_CERT`: the send does not trust the CA that issued the relay's certificate. Set `CG_SMTP_CA_FILE`, or correct the file it names.
- `ERR_TLS_CERT_ALTNAME_INVALID`: `CG_SMTP_HOST` is not a name the relay's certificate carries. Set it to one that is.
- `CERT_HAS_EXPIRED` or `CERT_NOT_YET_VALID`: a certificate of the relay's chain is outside its validity period.

## Playbooks and rules

`CG_SOAR_PLAYBOOKS_DIR` names the directory, inside ingest's container, that SOAR reads its playbooks from when ingest starts:

- Empty: the image's `/app/playbooks`.
- `/app/playbooks-none`, an empty directory the image carries: SOAR stays idle, and ingest logs `soar_idle`.
- Another directory: a compose override file mounts it into ingest's container ([Compose override files](#compose-override-files)), readable by uid 1000, and `CG_SOAR_PLAYBOOKS_DIR` names it as the container sees it. A directory that does not exist leaves SOAR idle, and ingest logs `soar_idle` at `warn`.

```yaml
services:
  ingest:
    volumes:
      - <directory on the host>:<directory in the container>:ro
```

```dotenv
CG_SOAR_PLAYBOOKS_DIR=<directory in the container>
```

A playbook that fails to load stops ingest at its start, naming the file. The image's rules, playbooks and their schema are owned by root and not writable by ingest's process. Another rule set takes another image, or an override file that mounts another directory and sets `INGEST_DETECT_RULES_DIR`, in ingest's `environment`, to it.

Ingest reads both directories when it starts. After a change to the override file or to `CG_SOAR_PLAYBOOKS_DIR`, apply it with:

```sh
docker compose up -d --wait
```

After a change to the files of a directory the override already mounts, restart ingest:

```sh
docker compose restart ingest
```

## Logs

The compose keeps each container's output, its standard output and standard error, in Docker's `json-file` log on the host, rotated at 10 MB, five files per container. Ingest and the api write JSON lines there at `CG_LOG_LEVEL`, and the proxy its access log. `docker compose logs <service>` reads them.

The threat model asks for a year of retention for service logs ([threat-model.md](../security/threat-model.md)); the bundle runs no log store. The following complete override uses Docker's TLS-verified syslog driver for all nine services, including the migration one-shots. Replace `logs.example.net:6514` with the collector's DNS name and port, and place its issuing CA on the **Docker daemon's host** at the absolute path shown. The collector's certificate must cover that DNS name. No container mount supplies this CA to the daemon.

Compose's `!override` replaces the options map: the inherited `json-file` options `max-size` and `max-file` must not leak into the syslog driver. The `cache-*` options bound Docker's local dual-logging cache, which keeps `docker compose logs` usable; the collector owns the one-year retention. This changes logging only when the operator applies the override; the bundle's default remains `json-file`.

<!-- configuration: logging -->

```yaml
x-shipped-logging: &shipped-logging
  driver: syslog
  options: !override
    syslog-address: "tcp+tls://logs.example.net:6514"
    syslog-tls-ca-cert: "/etc/docker/cyberguard-logging/collector-ca.pem"
    syslog-format: "rfc5424micro"
    tag: "{{.Name}}"
    mode: "blocking"
    cache-disabled: "false"
    cache-max-size: "10m"
    cache-max-file: "5"
    cache-compress: "true"

services:
  proxy:
    logging: *shipped-logging
  dashboard:
    logging: *shipped-logging
  api:
    logging: *shipped-logging
  ingest:
    logging: *shipped-logging
  ingest-migrate:
    logging: *shipped-logging
  api-migrate:
    logging: *shipped-logging
  postgres:
    logging: *shipped-logging
  clickhouse:
    logging: *shipped-logging
  redis:
    logging: *shipped-logging
```

Use the supported Compose version above. Review the merged configuration, recreate the containers in a maintenance window and confirm receipt from each service and one-shot at the collector. Test a collector outage before accepting the installation. Failure to connect or verify TLS can prevent container creation; blocking delivery can stall writes to stdout/stderr. The local cache supports local reads and is not a durable delivery queue or an acknowledgment from the collector. Neither TLS nor this driver guarantees lossless or exactly-once delivery. Monitor receiver availability, delivery errors and retention at the collector, and protect access to the personal data it holds.

The option meanings are defined in Docker's [syslog driver](https://docs.docker.com/engine/logging/drivers/syslog/) and [dual logging](https://docs.docker.com/engine/logging/dual-logging/) documentation. [data-protection.md](data-protection.md) names what the logs hold.

### Compose override files

Compose reads `docker-compose.override.yml`, beside `docker-compose.yml`, together with it, when no `-f` and no `COMPOSE_FILE` names the files (Docker Compose's documentation on merging Compose files). `backup.sh` and `restore.sh` run `docker compose` in the bundle's directory with no `-f`, so the file applies to their commands too. Put each override of this document in that one file, and carry it with `.env` to a newer bundle's directory ([upgrade.md](upgrade.md)). To read the compose as Compose merges it, without the values of `.env`:

```sh
docker compose config --no-interpolate
```

## The proxy's answers

- In both TLS modes, a connection with no DNS SNI or a DNS SNI outside the served certificate's names is refused during the handshake, before HTTP routing. The dashboard's SNI succeeds. In `provided` mode an additional DNS name covered by the supplied certificate can also complete TLS; HTTP `Host` routing still follows the rules below. A TCP connection alone is not a dashboard health check.
- On `CG_HTTP_PORT`, every request is answered `308`, to `https://<CG_DASHBOARD_HOSTNAME>:<CG_HTTPS_PORT>` followed by the request's path and query.
- On `CG_HTTPS_PORT`, a request whose `Host` names `CG_DASHBOARD_HOSTNAME`, with or without a port, in any letter case, goes to the dashboard, and its response carries `Strict-Transport-Security: max-age=31536000`.
- On `CG_HTTPS_PORT`, a request whose `Host` names another host is answered `421 Misdirected Request`, with no body and no `Strict-Transport-Security`, and goes nowhere. The answer comes once the TLS handshake completes, which it does when the certificate the proxy serves covers the name the client asked for.

A monitor of the proxy addresses it by the dashboard's hostname, both in its TLS connection and in its `Host`: `https://<CG_DASHBOARD_HOSTNAME>:<CG_HTTPS_PORT>/api/health` answers `200` with `{"status":"ok"}`, from the dashboard. A monitor that sends another `Host` is answered `421`, and one that connects by an address may not complete the handshake. A monitor may also probe the HTTP port, 80 by default, whose `308` comes from the proxy alone, as the proxy's own health check does.

Ingest answers `GET /health` on `CG_ENROLL_PORT`, over TLS with its server certificate, `200` with `{"status":"ok"}`.

## What the bundle assumes

- One host. The leg between the dashboard and the api is plain HTTP, on the internal `app` network, which holds those two containers alone; the stores are reached without TLS on the internal `data` network. A deployment that splits the stack across hosts is not supported: the session token, a login's password and its TOTP code would cross the network in clear (the threat model's R13, debt #97 in the [debt register](../debts.md)).
- Browsers reach the dashboard through the proxy alone. The api reads `X-Forwarded-For` from the dashboard's address alone, and the proxy sets that header from the connection it received: publishing the dashboard any other way lets a client choose the address the api counts and records, and reopens the threat model's R16 ([threat-model.md](../security/threat-model.md)).

## The agent's tuning

The agent reads its configuration from `agent.toml`, which [install.md](install.md) writes. These keys tune it. Each is optional, with the default shown.

| Key | Default | Meaning |
|---|---|---|
| `[heartbeat] interval_seconds` | `30` | Seconds between heartbeats. |
| `[heartbeat] request_timeout_seconds` | `10` | A request's timeout, in seconds. |
| `[heartbeat] max_retries` | `3` | Attempts for a request. A request with events that fails transiently (a connection error, a timeout or a 5xx answer) is retried until it is delivered or rejected; one without events, up to this number. A rejected request, one answered with any other status that is not a success, is retried up to this number of attempts in all, then dropped. |
| `[heartbeat] backoff_initial_ms` | `1000` | The first wait before a retry, in milliseconds. |
| `[heartbeat] backoff_factor` | `2.0` | The factor each further wait is multiplied by. |
| `[heartbeat] backoff_max_ms` | `60000` | The longest wait, in milliseconds. |
| `[enrollment] timeout_seconds` | `30` | The enrollment request's timeout, in seconds. |
| `[enrollment] max_retries` | `3` | Enrollment attempts in all while the server answers with a server error or cannot be reached; then the agent exits 4. |
| `[enrollment] backoff_initial_ms` | `1000` | The first wait before a retry, in milliseconds. |
| `[enrollment] backoff_factor` | `2.0` | The factor each further wait is multiplied by. |
| `[log] level` | `info` | The agent's log level. |

```toml
[heartbeat]
interval_seconds = 30
request_timeout_seconds = 10
max_retries = 3
backoff_initial_ms = 1000
backoff_factor = 2.0
backoff_max_ms = 60000
```

`interval_seconds`, `request_timeout_seconds`, `backoff_initial_ms` and `backoff_max_ms` are above 0, and `backoff_factor` is 1.0 or more. The agent refuses a configuration that breaks a rule: it prints a line that starts `cg-agent: invalid config:` on standard error and exits 2.
