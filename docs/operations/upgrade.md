# Upgrade

This document covers two cases: an installation of the bundle upgraded to a newer release, and a deployment built from the repository before SPEC-023, which meets the changes SPEC-023 makes when it runs its code. The installation is in [install.md](install.md), the variables in [configuration.md](configuration.md), and the backups in [backup-and-restore.md](backup-and-restore.md).

It covers the server. The agent is installed and run by hand until its installer ships (SPEC-023 §Scope); each release publishes its executable as `cg-agent-<version>-x86_64-pc-windows-msvc.exe`.

## Upgrading to a newer release

An installation keeps its data in Docker volumes named `<project>_<name>`, where `<project>` is `cyberguard-enterprise` unless `.env` sets `COMPOSE_PROJECT_NAME`. A bundle unpacked in another directory under the same project name is the same installation: its `docker compose up` replaces the running containers and keeps the volumes. An upgrade therefore unpacks the newer bundle in a directory of its own, carries the installation's files into it, and runs the stack from there.

Below, `<older>` is the directory of the installed bundle and `<newer>` the newer bundle's. Every command runs as root on the server, since `.env` is owned by root with mode `0600`.

Read the newer release's notes before scheduling the outage. In particular, check store-version compatibility and any explicit migration step; this procedure is not permission to downgrade a store or reuse a newer schema with older services. Keep the older bundle, its image digests, a protected copy of its `.env`, and a tested restore available until the upgrade is accepted. Never use `down -v` on the installed project.

The steps, in order; stop on any nonzero command result:

1. Verify and unpack the newer bundle.
2. Carry the installation's files into the newer directory.
3. Run `setup.sh` there and review the added settings.
4. Pull the newer images.
5. Stop the dashboard, the api and ingest; take the pre-upgrade backup.
6. Start the stores and run both migration one-shots, with the writers still stopped.
7. Start the services and check the installation.

### Unpack the newer bundle

Download the newer release's bundle, `cyberguard-deploy-<version>.tar.gz`, with the release's `SHA256SUMS`, and check it as [install.md](install.md#verify-the-files) says. In the directory it was downloaded to, unpack it into `<parent>`, the directory that is to hold the newer bundle's, for instance the one that holds `<older>`:

```sh
tar -xzf cyberguard-deploy-<version>.tar.gz -C <parent>
```

It unpacks into one directory, `<parent>/cyberguard-deploy-<version>/`, which is `<newer>`.

### Carry the installation's files

The newer directory holds the bundle's files alone. Copy into it what the installation added to the older one:

| What | When | Why |
|---|---|---|
| `.env` | Always. | It holds the secrets the stores and the keys were set up with, and the installation's settings, `COMPOSE_PROJECT_NAME` among them when it is set. |
| `tls/dashboard.crt` and `tls/dashboard.key` | In `provided` mode. | The dashboard's certificate and key. |
| The CA file in `relay/` | When `CG_SMTP_CA_FILE` is set. | The relay's private CA, which `CG_SMTP_CA_FILE` names. |
| `docker-compose.override.yml` | When the installation has one ([configuration.md](configuration.md#compose-override-files)). | The newer directory's `docker compose` reads it only when it is there. |
| The backup directory | When `CG_BACKUP_DIR` names a directory inside the bundle's directory, as its default `backups` does. | `backup.sh` resolves a relative `CG_BACKUP_DIR` against its working directory. Re-evaluate every relative path from the newer directory, even a path starting with `..`; an absolute path stays the same. |

The first command always applies; each other one when its row does. The last one is for the default `CG_BACKUP_DIR`:

```sh
cp -p <older>/.env <newer>/.env
cp -p <older>/tls/dashboard.crt <older>/tls/dashboard.key <newer>/tls/
cp -p <older>/relay/<CA file> <newer>/relay/
cp -p <older>/docker-compose.override.yml <newer>/
cp -pR <older>/backups <newer>/backups
```

The project name moves with `.env`. Keep `COMPOSE_PROJECT_NAME` there, and never pass `-p`: `backup.sh` and `restore.sh` run `docker compose` in the bundle's directory and follow only what Compose reads there, so under `-p` they act on another project. Under another project name, `docker compose up` acts on another installation, whose volumes are new and empty.

The named volumes are reused. The CA is in the database and the ingest server pair in `ingest_certs`; retaining them and the same hostnames preserves the agents' trust. Keep a separate protected copy of an operator-provided ingest pair and its issuing CA for disaster recovery. In `internal` mode the proxy's CA is in `proxy_data`, so retaining that volume preserves the browsers' trust. Review each override against the newer compose; do not blindly carry an image override that would keep an older product image running.

### Run setup.sh in the newer directory

```sh
cd <newer>
./setup.sh
```

With `.env` in place, `setup.sh`:

- adds each variable the newer `.env.example` has and `.env` lacks, with its default, and prints `setup.sh: added <name>` for each;
- keeps every value `.env` already sets, the `COMPOSE_PROJECT_NAME` line included;
- fills each generated secret that is empty, in an installed `.env` those the newer bundle adds, and prints `setup.sh: filled: <names>`, never a value; when it fills none it prints `setup.sh: nothing to fill`;
- sets the mode of `.env` to `0600` and, in `provided` mode, gives the TLS key to the proxy's user (uid 10001) with mode `0400`.

It refuses, changing nothing, when a hostname is empty or, in `provided` mode, a TLS file is missing, naming what is missing, for instance `setup.sh: missing: tls/dashboard.crt tls/dashboard.key`. Read each added variable in [configuration.md](configuration.md), and set it in `.env` now when its default does not suit the installation.

### Pull the newer images

```sh
docker compose pull
```

The images are then on the host before anything stops, so the services stop below for the migrations and the start alone.

### Stop the writers and take the pre-upgrade backup

Coordinate the maintenance window with administrators: no other migration, restore, direct database write or deployment command may run during it. Use the newer directory throughout the commands below. It names the same project and volumes as the older one.

<!-- upgrade: stop -->

```sh
docker compose stop dashboard api ingest
```

A migration that changes a table a running service holds a lock on waits for that lock, up to five minutes, and the service's own statements on that table wait behind it meanwhile; past five minutes the migration fails (*When a lock wait runs out*, below). Stopping these services removes their active database sessions. Other operator or maintenance sessions can still hold locks; stopping the writers does not grant exclusive database access. The proxy and the stores stay up.

While the three are stopped, users cannot use the dashboard and agents cannot reach ingest. A running agent does not stop: it retries its delivery and keeps its events in its ring, which drops the oldest when it fills and logs a warning with the dropped total.

Take the backup before any store replacement or migration, with the stores running and the writers stopped:

<!-- upgrade: backup -->

```sh
./backup.sh
```

Copy both encrypted files off-host and keep `.env` separately, as [backup-and-restore.md](backup-and-restore.md) says. Record their common timestamp and the older version. The script dumps the stores sequentially, not in a transaction spanning both; the outage removes application writes but does not make this an atomic cross-store snapshot. If the backup fails, investigate before migrating. Starting the old services is only a fallback while no newer store or migration has run.

### Run the migrations with the writers stopped

Start or replace only the stores, then run each one-shot in the foreground. `set -e` stops this block if a command fails. `--no-deps` prevents starting a writer as a dependency; `--force-recreate` runs the newer migration container even when an older one exited successfully.

<!-- upgrade: migrate -->

```sh
set -e
docker compose up -d --wait postgres clickhouse redis
docker compose up --no-deps --force-recreate --exit-code-from ingest-migrate ingest-migrate
docker compose up --no-deps --force-recreate --exit-code-from api-migrate api-migrate
```

Proceed only when both one-shots exited 0. The first applies ingest's Postgres migrations, then its ClickHouse tables and retention; the second applies the api's Postgres migrations. A normal whole-stack `up` can start ingest as soon as `ingest-migrate` succeeds, while `api-migrate` is still running. This upgrade procedure deliberately runs both before restarting either writer (debt #127).

### Start and check the newer release

<!-- upgrade: start -->

```sh
set -e
docker compose up -d --wait --no-deps ingest api
docker compose up -d --wait --no-deps dashboard proxy
docker compose ps -a
```

`--no-deps` here avoids rerunning the completed migrations. Wait for healthy ingest and api before the dashboard and proxy. Confirm a dashboard sign-in, an existing agent's new heartbeat and the expected records; inspect the service logs on a failure. Record the deployed release and the results in the maintenance record.

From now on, run every `docker compose` command, `backup.sh` and `restore.sh` in the newer directory. A `docker compose up` in the older directory would replace the containers with the older release's.

The upgrade keeps the database, which holds the CA and the forensic key, so the `sha256:` line each of these commands prints is the one it printed before the upgrade:

```sh
docker compose exec -T ingest node dist/cli/export-ca.js
docker compose exec -T api node dist/cli/forensic-key.js
```

The older directory is no longer used. It still holds a copy of `.env` and, in `provided` mode, the TLS key: remove it, or keep it as you keep `.env`.

### When a lock wait runs out

The one-shots' command, `node dist/db/migrate.js`, waits at most five minutes for each lock it needs: its own migration lock, and every lock its migrations wait for (SPEC-023 §Data contracts §4, as its second Amendment 2026-10-26 amends it). When a wait runs out, it prints one of these two lines and exits 1; `api-migrate` prints them with `cg-api` in place of `cg-ingest`:

```text
cg-ingest migrate: fatal: the migration lock is held by another session: waited 300 s
cg-ingest migrate: fatal: a lock a migration waits for is held by another session: waited 300 s
```

The first line names the one-shot's own lock, which another session holds: another run of the same command, for instance one started with `docker compose run`, or any session of an owner role or of the superuser that took it. The second names a lock one of its migrations waits for, such as a lock that an ingest or an api left running holds on a table the migration changes.

A run that fails on a Postgres lock timeout has committed none of that run's Postgres migrations and, for ingest, changed nothing in ClickHouse. This does not undo a preceding successful one-shot. A later ClickHouse failure leaves ingest's Postgres migrations committed. Keep the writers stopped and read the output:

```sh
docker compose logs ingest-migrate api-migrate
docker compose stop dashboard api ingest
```

Find and resolve the competing session with its owner, then repeat the migration block above. Do not kill an unidentified database session or raise the timeout to hide contention. Owner roles can still take either service's advisory key; the two distinct keys are not an isolation boundary. Schedule one upgrade at a time and restrict owner credentials to maintenance. The bound is per lock wait, not a five-minute limit for the whole upgrade.

### If the upgrade cannot be completed

Keep the failed installation and its logs for diagnosis. There is no automated down-migration or in-place rollback command. After migrations or a store replacement, do not run the older bundle against those volumes. Recover the pre-upgrade backup into empty stores using the older release and its saved `.env`, following [backup-and-restore.md](backup-and-restore.md); preserve the failed volumes and isolate the recovery until it is checked. Restore the saved operator-provided certificates too, when used. Confirm the CA and forensic fingerprints, the records and an agent heartbeat before switching users and agents to the recovered installation. Writes accepted after the backup are outside that recovery point.

### A change of retention

The `ingest-migrate` one-shot sets the retention at each run ([configuration.md](configuration.md#retention)). It alters a table's time-to-live, and materialises it, only when the table's time-to-live differs from the one this release's rules give it with the values of `.env`: rows already stored are then measured against the new period. That happens when a `CG_RETENTION_*` value changes, and under a release whose rules for a table differ in form from the installed release's, even with the same values. ClickHouse deletes the rows past their period at its merges, so a row may outlive its period by the time a merge takes. Materialising rewrites the parts of `cges_events`, which on a large table takes time and I/O.

`setup.sh` keeps the values `.env` sets, so under the same rules an upgrade changes no period unless you change a `CG_RETENTION_*` value. A value changed in the newer directory's `.env` is applied by the migration block above. A value outside 7 to 3650 days stops `ingest-migrate` before it sets a time-to-live, with `cg-ingest migrate: fatal: invalid configuration: <variable>: <reason>`, which names the `INGEST_RETENTION_*` variable the compose sets from it.

### Steps a release adds

The stores' initialisation, `postgres/` and `clickhouse/initdb/` in the bundle, runs once, on an empty data directory. It sets up the roles and the users, the schemas and the extensions, the rights, those on the advisory-lock functions among them, and Postgres's logging settings, which it writes into the data directory. A change a newer bundle makes to any file there does not reach an installed stack, so each such change is a step of the upgrade to the release that brings it, listed in this section.

Two kinds of change need no step: the rights on each Postgres table, which the services' migrations grant and the one-shots run at every `docker compose up`, and the key-value store's ACL file, which its container writes from `.env` at each start. ClickHouse's rights are granted in its initialisation alone, which names their tables, so a ClickHouse right a release adds is a step.

No release brings such a step yet.

## A deployment built before SPEC-023

The bundle comes with SPEC-023, and no release of it predates SPEC-023's code. Moving a deployment built from the repository before SPEC-023 to the bundle is a new installation, from [install.md](install.md), whose stores start empty and whose CA is new; its agents enroll again (*Agents*, below). A stack that keeps running the services built from the repository, the development stack of `infra/dev/` among them, meets the changes below when it runs SPEC-023's code. For such a stack, in order:

1. Before SPEC-023's code first runs, set the retention periods (*Retention*) and write the integer variables in digits (*Integer values*).
2. Stop the stack and preserve its databases, configuration and certificates before planning the schema transition (*The database*). The development stack's disposable database has its own recreation procedure.
3. Start SPEC-023's code, and create an admin, as [install.md](install.md#create-the-first-admin) does.
4. Enroll each agent again (*Agents*).

### Retention

SPEC-023's migration step sets a retention on `cges_events`, per class, and on `heartbeats` ([configuration.md](configuration.md#retention)), whether ingest runs it (`INGEST_RUN_MIGRATIONS=true`) or `node dist/db/migrate.js` does. A store that code from before SPEC-023 created had one period at most, 365 days on the logons (class 3002), measured from arrival. The first run of SPEC-023's migration step over such a store sets the new periods and materialises them: with the defaults, it deletes the heartbeats that arrived more than 30 days before and the events that arrived more than 365 days before, at ClickHouse's merges.

To keep more, set ingest's `INGEST_RETENTION_PROCESS_DAYS`, `INGEST_RETENTION_NETWORK_DAYS`, `INGEST_RETENTION_LOGON_DAYS` and `INGEST_RETENTION_HEARTBEAT_DAYS` before that run, each from 7 to 3650 days. The development compose passes no retention variable to ingest, so its ingest runs the defaults: keeping more there takes setting those variables in ingest's `environment` in `infra/dev/docker-compose.dev.yml`.

### Integer values

Ingest and the api read each integer variable they check, the ports, `INGEST_SMTP_PORT`, the detection interval, the session lifetimes and the retention periods, in ASCII decimal digits alone; leading zeros are digits. A value written otherwise, with a sign, whitespace, an exponent, a fraction or a prefix such as `0x`, is refused at start, as a value out of bounds is, with `cg-ingest: fatal: invalid configuration: <variable>: <reason>` or `cg-api: fatal: invalid configuration: <variable>: <reason>`. Code from before SPEC-023 accepted such a form for the ports and the interval when it denotes an integer, for instance `1e3`, `+8080`, `8080.0`, `0x1F90` or a value with a leading space. It also read an empty `INGEST_DETECT_INTERVAL_MS` as 0, the kill switch, which is now refused: write `0`. The interval is at most 2147483647.

### The database

Code from before SPEC-023 created its Postgres tables in the schema `public`. The services now keep theirs in the schemas `ingest` and `api`, each connection's search path naming its own (SPEC-023 §Operational §4). On such a database, the first run of SPEC-023's migrations creates the services' tables again in those schemas, empty, beside the old ones, which are no longer read. What the old tables hold is then lost to the services, whether or not the database is recreated afterwards; among it:

- the agents and their enrollment tokens, the alerts, the incidents, the playbook records, the users and the audit log are no longer read;
- the CA and the forensic key are no longer read either: ingest generates a new CA at its next start, and the api a new forensic key, whose fingerprint is recorded as [forensic-verification.md](forensic-verification.md#recording-the-fingerprint-at-installation) says; the agents enroll again (*Agents*, below);
- the detection cursor starts again: once an agent of an org has enrolled again, the rules read that org's process events (class 1007) again from the first one ClickHouse keeps and write their alerts and incidents again, and for each incident created again ingest sends its notification, when notification is set up, and runs its playbooks, when playbooks are loaded.

There is no automatic import from `public`. Preserve the old database and keys for access to the old records; starting new tables is not a migration of that evidence. For a non-development deployment, plan and validate the data transition separately before running the newer services. Do not delete its database on the strength of this runbook. The development stack's recreation step below is for its disposable data only.

If the transition creates a new internal CA, ingest also needs a server certificate that CA issued. Unless its pair is operator-provided, retain a protected copy of the old pair, then remove the active server certificate and key, the files `INGEST_SERVER_CERT_PATH` and `INGEST_SERVER_KEY_PATH` name (`server.pem` and `server-key.pem` in ingest's certificates volume), while ingest is stopped: at its next start ingest issues a pair from its CA. A pair the old CA signed is not one its current CA signed, so ingest never rewrites it (SPEC-023 §Operational §6): it goes on serving it, and an agent anchored on the new CA ends with exit code 6.

For the development stack, [infra/dev/README.md](../../infra/dev/README.md) gives the commands that remove its database volume and its certificates volume.

### Agents

Ingest's enrollment listener serves HTTPS, TLS 1.3 only, by default, and the agent enrolls over TLS, trusting the roots of its trust anchor alone (SPEC-023 §Operational §6). An agent keeps its `[enrollment]` section after it enrolls, with the token alone removed. With `[enrollment]`, `server.url` must be `https://` and `server.trust_anchor_path` must be set:

- an agent whose `agent.toml` keeps an `http://` `server.url` beside `[enrollment]` exits with code 2 at its next start, and its line on standard error ends with `server.url must use https when [enrollment] is set`;
- an agent without `server.trust_anchor_path` exits with code 2, its line ending with `missing key 'server.trust_anchor_path'`.

Its `[server]` section then reads:

```toml
[server]
url = "https://<server name>:<enrollment port>"
heartbeat_url = "https://<server name>:<heartbeat port>"
trust_anchor_path = "C:/ProgramData/CyberGuard/agent/server-ca.pem"
```

`<server name>` is one of the names the server certificate carries: `CG_AGENT_SERVER_NAMES` in the bundle, `INGEST_SERVER_NAMES` for ingest, `localhost` and `127.0.0.1` by default. The bundle's ports are `CG_ENROLL_PORT` (8444) and `CG_HEARTBEAT_PORT` (8443); the development stack publishes 8080 and 8443 on `127.0.0.1` (`CG_INGEST_ENROLL_PORT` and `CG_INGEST_HEARTBEAT_PORT`). The anchor file holds the CA certificate that `export-ca.js` prints, as [install.md](install.md#on-the-server-the-trust-anchor-and-a-token) writes it, or, with an operator-provided server certificate, the CA that issued it.

The internal CA changes too: a new installation from the bundle has its own, and a stack that starts with the new, empty schemas generates a new one when ingest starts (*The database*, above). An agent's client certificate came from the old internal CA, so each agent enrolls again. Its server trust anchor changes to the new internal CA only when that CA also issues the server pair. For an operator-provided server pair, keep trusting its actual issuer instead. With an internally issued server pair, until reenrollment:

- while ingest serves a certificate the new CA issued, the agent refuses it and exits with code 6, its line beginning `cg-agent: tls: server certificate verification failed:`;
- while ingest still serves the pair the old CA signed, the heartbeat listener refuses the agent's certificate, which the new CA did not issue, and the agent exits with code 7, its line `cg-agent: tls: server rejected client certificate`.

Enrolling again needs ingest to serve a certificate its new CA issued (*The database*, above), unless its pair is operator-provided, whose CA stays the anchor. Then, for each agent:

1. On the server, prepare `server-ca.pem`: for the internally issued server pair, export the new internal CA as [install.md](install.md#on-the-server-the-trust-anchor-and-a-token) does; for an operator-provided pair, use its verified issuing CA, never substitute `export-ca.js` output. Issue a new enrollment token in either case.
2. On the endpoint, stop the agent. From an elevated PowerShell, install that server trust anchor and remove the certificate and key it enrolled with, the files its `enrollment.cert_path` and `enrollment.key_path` name. For an unchanged operator-provided pair its existing verified anchor can be retained. With the paths of [install.md](install.md#on-the-endpoint-install-and-run-the-agent), and the correct issuer file selected in step 1:

   ```powershell
   Copy-Item '<server-ca.pem>' 'C:\ProgramData\CyberGuard\agent\server-ca.pem'
   Remove-Item 'C:\ProgramData\CyberGuard\agent\cert.pem', 'C:\ProgramData\CyberGuard\agent\key.dat'
   ```

3. Set its `[server]` section as above, and the new token in its `[enrollment]` section, keeping the section's other keys:

   ```toml
   [enrollment]
   token = "<token>"
   ```

4. Start it from the elevated PowerShell, as [install.md](install.md#on-the-endpoint-install-and-run-the-agent) does. With either file absent, it enrolls with the token and writes both again (SPEC-002 FR-001, FR-002).

### Browsers

The dashboard's cookies are named `__Host-cgsess` and `__Host-cgcsrf`, and the dashboard reads no other name. A browser that holds the earlier `cgsess` and `cgcsrf` is signed out, and its user signs in again. The api session behind such a cookie stays in the key-value store until its lifetime ends.

### The development stack

The development stack's own steps are in [infra/dev/README.md](../../infra/dev/README.md): its database, recreated once; its Redis volume, which Valkey does not load; and its certificates volume, which an ingest running as root wrote and the non-root ingest cannot use, removed once, which also gives ingest a pair from its new CA (*The database*, above). The bundle's ingest has never run as root, so the bundle has no such step.
