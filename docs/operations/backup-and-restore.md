# Backup and restore

`backup.sh` writes an encrypted backup of the two stores that hold the installation's data, Postgres and ClickHouse. `restore.sh` restores one into a stack whose stores are empty (SPEC-023 §Operational §10). Both scripts are in the bundle's directory. Run them there, as root: `.env` is root's, mode `0600`, and the scripts refuse to run when they cannot read it. Besides Docker Engine and its Compose plugin, they need on the host `gpg` 2.2.7 or later ([install.md](install.md)), and a POSIX `sh` with `sed`, `date`, `mktemp`, `cat`, `mkdir`, `chmod` and `rm`.

Both scripts read `.env` and run a plain `docker compose` in the directory they are run in, with no `-p` and no `-f`; that directory must be the bundle's. They then act on the project that every other command in the bundle's directory acts on, including one named by `COMPOSE_PROJECT_NAME` in `.env` ([configuration.md](configuration.md#the-project-name)). Like any `docker compose` command, theirs reads `COMPOSE_PROJECT_NAME` and `COMPOSE_FILE` when they are set.

## What a backup holds

| Part of the installation | In a backup | After a restore |
|---|---|---|
| Postgres, the database `cyberguard` | `cyberguard-postgres-<stamp>.dump.gpg`, a `pg_dump` in custom format | Restored. |
| ClickHouse, the database `cyberguard` | `cyberguard-clickhouse-<stamp>.zip.gpg`, a ClickHouse `BACKUP DATABASE` archive | Restored. |
| The key-value store, the `redis` service and its `redis_data` volume | Not backed up | Not restored. It holds sessions, rate-limit counters, TOTP replay keys and nonces: users sign in again. |
| The `ingest_certs` volume, ingest's server certificate and key | Not backed up | Ingest issues a self-issued pair again from the CA, which the Postgres dump holds. An operator-provided pair is the operator's to put back ([After a restore](#after-a-restore)). |
| The `proxy_data` volume | Not backed up | In `internal` mode the proxy generates a new CA, and browsers must trust its new root. |
| The `proxy_config` volume, the proxy's configuration autosave | Not backed up | Not restored. |
| `.env` | Not copied | The operator brings the copy of the backed-up installation's `.env`. |
| `tls/`, the relay's CA file in `relay/`, `docker-compose.override.yml` | Not copied | The operator brings them. |

`<stamp>` is the date and time of the backup run in UTC, written `YYYYMMDDTHHMMSSZ`. The two files of one backup carry the same one.

The Postgres dump holds ingest's and the api's schemas: the CA, the enrollment tokens, the agents, the alerts, the incidents, the playbook records, the detection cursor, the users, the audit log and the forensic key. The private keys of the CA and of the forensic key, and the users' TOTP secrets, are in it encrypted under passphrases that `.env` holds (`CG_INGEST_CA_PASSPHRASE`, `CG_API_FORENSIC_PASSPHRASE` and `CG_API_DB_ENC_PASSPHRASE`). The ClickHouse archive holds the events of the process, network and logon classes (`cges_events`) and the agents' heartbeats (`heartbeats`). Both files hold personal data, which [data-protection.md](data-protection.md) lists.

## The passphrase and `.env`

- Each file is encrypted with `gpg` (AES256, symmetric) under `CG_BACKUP_PASSPHRASE`, on the host, as its stream leaves its container. Only the encrypted files are written to `CG_BACKUP_DIR`.
- `setup.sh` generates `CG_BACKUP_PASSPHRASE` with the other secrets and keeps it once set. The two scripts read it from `.env`, and it reaches no container. They give it to `gpg` on a file descriptor, never on a command line, with no passphrase cache in `gpg-agent`.
- While a backup or a restore runs, the ClickHouse archive exists in clear on ClickHouse's backup disk, the `ch_backups` volume on the host's disk. `backup.sh` writes it there and streams it out; `restore.sh` decrypts it there and restores it. Each deletes it. The host's disk needs room for the archive there as well as for the encrypted file.
- `backup.sh` does not copy `.env`. Keep a copy of `.env` apart from the backups. It is the installation's master secret: it holds the backup passphrase and the passphrases of the keys the dump holds encrypted. Whoever holds a backup and `.env` can read the backup, and decrypt the CA's key, the TOTP secrets and the forensic key. If `.env` is lost and no copy of it exists, no backup can be decrypted, since its passphrase is in `.env` alone, and the CA, the TOTP secrets and the forensic key are lost.
- `restore.sh` decrypts with the `CG_BACKUP_PASSPHRASE` of the `.env` in the directory it is run in, the bundle's directory, so a backup restores with the `.env` it was written under. A `.env` that `setup.sh` generated anew holds other secrets, and `restore.sh` refuses the files.
- `setup.sh` keeps every value once set, so the secrets of a copy taken after its second run stay those of the installation unless an operator changes one. Copy `.env` again after changing another of its variables, so that a restore brings the change back.

## Taking a backup

With the stores running, in the bundle's directory (the output shows the default `CG_BACKUP_DIR`):

```sh
sh backup.sh
```

```text
backup.sh: the Postgres dump
backup.sh: the ClickHouse archive
backup.sh: wrote backups/cyberguard-postgres-<stamp>.dump.gpg and backups/cyberguard-clickhouse-<stamp>.zip.gpg
```

- `CG_BACKUP_DIR` names the host directory the files go to; empty or absent, it is `backups`. A relative path is relative to the bundle's directory, so the default directory is inside it: removing the bundle's directory removes the backups kept there.
- `backup.sh` creates the directory when it is absent, sets its mode to `0700`, and writes the files under `umask 077`.
- It runs `pg_dump` in the `postgres` container and ClickHouse's `BACKUP` in the `clickhouse` container, which must be running. It stops no service.
- The dumps are sequential, not one atomic transaction across stores. A live backup can capture related records at different moments. For an upgrade, follow [upgrade.md](upgrade.md#stop-the-writers-and-take-the-pre-upgrade-backup), which stops the application writers first. Keep their outage short enough for the endpoints' bounded memory buffers; a backup does not include undelivered endpoint events.
- It never deletes an earlier backup. Deleting the backups that are past their keeping is the operator's ([Keeping backups](#keeping-backups)).
- A run's two files are a backup only when it has printed a line that begins `backup.sh: wrote`. Until both files are whole, a failure, or an interruption by `HUP`, `INT` or `TERM`, deletes the files of that run. A run that is killed, or that the host's stop ends, deletes nothing, and can leave one whole file, or a part of one, in `CG_BACKUP_DIR`: delete the files of a run that printed no `wrote` line, and copy none of them off the host.
- On any exit it tries to delete the archive from the backup disk. When that fails, or when the run is killed or the host stops, the clear archive can remain there. This command prints nothing when the backup disk is empty:

```sh
docker compose exec -T clickhouse ls -A /backups
```

An archive of `backup.sh` there is named `cyberguard-<stamp>.zip`, and one of `restore.sh` `restore-<time>.zip` ([Restoring](#restoring)). Remove one that remains with:

```sh
docker compose exec -T clickhouse rm -f /backups/<archive>
```

### What `backup.sh` refuses

Each of these lines ends the run with exit status 1. A `gpg` failure while it writes a file ends the run with `gpg`'s own message and status. In both cases the run's files are deleted, except in the last row.

| Line | Cause and remedy |
|---|---|
| `backup.sh: no .env: run me in the bundle's directory, after setup.sh` | It was run in another directory, or before `setup.sh` wrote `.env`. |
| `backup.sh: .env is not readable: run me as the user setup.sh ran as` | It was not run as root. |
| `backup.sh: gpg not found: it is a prerequisite of the host` | Install `gpg` 2.2.7 or later on the host. |
| `backup.sh: CG_BACKUP_PASSPHRASE is empty: run setup.sh` | Run `setup.sh`, which fills it. |
| `backup.sh: a backup of <stamp> exists already: run me again` | Another run started in the same second. Run it again. |
| `backup.sh: pg_dump failed` | The dump in the `postgres` container failed, for instance with that container stopped. |
| `backup.sh: BACKUP DATABASE failed` | ClickHouse's `BACKUP` failed, for instance with the `clickhouse` container stopped. |
| `backup.sh: reading the archive failed` | Streaming the archive out of the `clickhouse` container failed. |
| `backup.sh: wrote <pg file> and <ch file>, but the archive could not be deleted from the backup disk: remove /backups/<archive> in the clickhouse container` | Both files are whole and kept. Remove the clear archive with `docker compose exec -T clickhouse rm -f /backups/<archive>`. |

## Keeping backups

- Set a backup schedule and recovery objectives for the installation, and rehearse a restore on isolated infrastructure. Record the backup timestamp, release/digests, outcome, off-host copy and restore checks. The scripts provide no scheduler, off-host transfer, retention job, RPO or RTO guarantee.
- Copy both files of each backup off the host. Keep the copy of `.env` in another place, so that no one place holds a backup together with its passphrase.
- Keep copies of the installation's files that no backup holds and that a restore needs: an operator-provided ingest pair, `server.pem` with its key `server-key.pem`; in `provided` mode, `tls/dashboard.crt` with `tls/dashboard.key`; the file `CG_SMTP_CA_FILE` names, in `relay/`; and `docker-compose.override.yml`, with any host directory it mounts. The two keys are secrets: keep them as you keep the copy of `.env`.
- A backup holds personal data and has no period of its own. Keep each one no longer than the longest retention period `.env` sets (the four `CG_RETENTION_*` variables, 365 days by default), as [data-protection.md](data-protection.md) says. A restored backup brings back rows past their period, until ClickHouse's next time-to-live merge deletes them.

## Restoring

`restore.sh` takes the two files of one backup, the Postgres dump first. They may be anywhere on the host:

```sh
sh restore.sh <dir>/cyberguard-postgres-<stamp>.dump.gpg <dir>/cyberguard-clickhouse-<stamp>.zip.gpg
```

It does not check that the two files come from one backup: give it two files with the same `<stamp>`. It restores into a stack whose stores are empty, with the `.env` of the installation that was backed up. Restore with the bundle of the release that wrote the backup: that is the restore the bundle's tests run, and the migration one-shots then find every migration of the dump applied (SPEC-023 §Operational §10). Move to a newer release afterwards, as [upgrade.md](upgrade.md) describes.

In order, `restore.sh`:

1. Decrypts both files once, and stops if either does not decrypt or `gpg` finds it manipulated. It starts and stops nothing before this check.
2. Starts the stores (`postgres`, `clickhouse` and `redis`), and stops unless Postgres has no table in the schemas `ingest` and `api` and ClickHouse none in the database `cyberguard`. It stops no service and restores nothing before this check.
3. Stops the dashboard, the api and ingest, so that neither service generates a CA or a forensic key before the restore.
4. Restores the Postgres dump as the superuser (`pg_restore --clean --if-exists`), over the roles and schemas that the stack's initialisation created from `.env`.
5. Decrypts the ClickHouse archive onto the backup disk, as `restore-<time>.zip`, where `<time>` is the UTC time `restore.sh` started, written as `<stamp>` is; restores it once `gpg` has found it whole; and deletes it.
6. Starts the stack (`docker compose up -d --wait`). The migration one-shots find every migration applied.

Its own lines, besides those of `docker compose`, are:

```text
restore.sh: checking both files
restore.sh: starting the stores
restore.sh: stopping the dashboard, the api and ingest
restore.sh: the Postgres dump
restore.sh: the ClickHouse archive
restore.sh: starting the stack
restore.sh: restored <pg file> and <ch file>; users sign in again
```

Do not start the stack on the empty stores before `restore.sh`. A `docker compose up` runs the one-shots, which create the tables, and starts ingest and the api, which generate a new CA and a new forensic key. `restore.sh` then refuses the stores as not empty ([Starting again from empty stores](#starting-again-from-empty-stores)).

### On the same host

Recover into a separate project, preserving the failed installation's volumes for diagnosis. Prefer a separate host when there is not enough disk or memory for both sets of stores.

1. Have the matching backup files, the saved `.env`, the original release bundle and all externally managed certificates at hand. Stop the old installation from its own directory with `docker compose stop`; do not delete its volumes.
2. Unpack that release into a new directory and copy the saved `.env` and operator files there. Keep every credential and encryption passphrase. Set a new `COMPOSE_PROJECT_NAME` in the new `.env`, such as `cyberguard-recovery-<id>`, and an unused `CG_APP_SUBNET`, `CG_APP_IP_RANGE` and `CG_DASHBOARD_APP_ADDRESS` as [configuration.md](configuration.md#the-app-networks-addresses) describes. The stopped installation's network still exists. Keep the recovery isolated from clients until checked; published addresses and ports must not conflict with other running projects.
3. Follow the new-host procedure below from its configuration check, using the new directory throughout. An operator-provided ingest pair is placed before `restore.sh` starts ingest. Use absolute backup paths if the files remain in the older directory.
4. Check the restored installation, then switch the intended client route to it and record its new project name. Do not start the old project alongside it on the same published addresses/ports. Retire its volumes only under a separate, deliberate cleanup after recovery acceptance and the required evidence retention.

### On a new host

1. Prepare the host as [install.md](install.md) prepares one: Docker Engine and its Compose plugin, `gpg`, the Docker daemon's settings, and the check that the host routes no network that overlaps `CG_APP_SUBNET`.
2. Unpack the bundle of the release that wrote the backup ([Restoring](#restoring)).
3. Put in the bundle's directory, from the old host or from their copies ([Keeping backups](#keeping-backups)):
   - `.env`, owned by root;
   - in `provided` mode, `tls/dashboard.crt` and `tls/dashboard.key`;
   - when `CG_SMTP_CA_FILE` is set, the file it names, in `relay/`;
   - when the installation has one, its override file, as `docker-compose.override.yml` beside `docker-compose.yml`, with any host directory it mounts. `restore.sh` runs a plain `docker compose`, which reads an override file under that name, or the files `COMPOSE_FILE` names, and no other ([configuration.md](configuration.md#compose-override-files)).
4. Check what in `.env` names the old host. Browsers reach the dashboard by `CG_DASHBOARD_HOSTNAME`. Agents reach ingest by the names and addresses of `CG_AGENT_SERVER_NAMES`, one of which the URLs of their `agent.toml` name. Plan their route to the new host, switching production clients only after verification. `CG_BIND_ADDRESS` must be `0.0.0.0` or an address of the new host. When a network the new host routes overlaps `CG_APP_SUBNET`, set `CG_APP_SUBNET`, `CG_APP_IP_RANGE` and `CG_DASHBOARD_APP_ADDRESS` to addresses it does not route, as [install.md](install.md#check-the-app-networks-addresses) says. Keep the secrets as they are. Make any logging collector and mounted operator files available before the containers start.
5. Run `setup.sh`:

   ```sh
   sh setup.sh
   ```

   With a complete `.env` of the same release it fills nothing and prints `setup.sh: nothing to fill`. It sets the mode of `.env` to `0600` and, in `provided` mode, gives `tls/dashboard.key` to the proxy's user, mode `0400`.
6. If ingest uses an operator-provided pair, install its saved certificate and key using [configuration.md's stopped-container procedure](configuration.md#placing-or-replacing-the-operators-pair). Do not start ingest or run a whole-stack `up`. Keep the actual issuing CA as the agents' server trust anchor.
7. Copy the two backup files to the host, and restore:

   ```sh
   sh restore.sh <dir>/cyberguard-postgres-<stamp>.dump.gpg <dir>/cyberguard-clickhouse-<stamp>.zip.gpg
   ```

8. Check the installation as [After a restore](#after-a-restore) says before admitting production clients.

A `COMPOSE_PROJECT_NAME` line in `.env` moves with it, so the restored installation keeps its project name.

### After a restore

- **The fingerprints.** The CA's and the forensic key's fingerprints are those printed before the backup. Print them:

  ```sh
  docker compose exec -T api node dist/cli/forensic-key.js
  docker compose exec -T ingest node dist/cli/export-ca.js
  ```

  Compare the `sha256:` line of `forensic-key.js`, its second, with the fingerprint recorded at installation ([forensic-verification.md](forensic-verification.md)), and the `sha256:` line of `export-ca.js`, its last, with the one it printed before. After a restore that `restore.sh` completed, the database holds the dump's `forensic_key` row, and the api generates no key. A different forensic fingerprint then means that the restored database holds a key other than the recorded one: the dump held another key, or none, and the api generated one at its start. Find the cause before anything else, for instance a backup taken before the api's first start, or a key substituted on the server; a key that is not the recorded one is what the record exists to reveal (ADR-0016 Amendment 2026-10-19, item 4). Only when the cause is known to be a key the api generated again, record the new fingerprint as at installation ([install.md](install.md#record-the-forensic-keys-fingerprint)); exports signed before stay verifiable under the earlier record (item 5). A different CA fingerprint means that the restored database holds another CA, which no agent enrolled under the earlier one trusts.
- **Users** sign in again, with their passwords and TOTP factors, which the dump holds.
- **Agents.** An agent enrolled before the backup keeps its certificate: ingest accepts its heartbeats with no new enrollment. While the stack is down, a running agent keeps running and retries the batch of events it was sending until ingest accepts it. The events it captures meanwhile wait in its buffer, which holds 65536 events; once the buffer is full, the oldest are dropped and the agent logs `captured events dropped`, so a long outage loses events.
- **Agents enrolled after the backup.** Such an agent has no row in the restored database, so ingest refuses its heartbeats, and logs `heartbeat rejected` with the reason `unknown_agent`. The agent keeps running: it sends each batch up to `max_retries` times in all (`[heartbeat]`, 3 by default, [configuration.md](configuration.md#the-agents-tuning)), then drops it and logs `batch dropped after repeated rejections`. What it captures is lost while the restored database holds no enrollment of it.
- **What came after the backup.** What the stores received after `backup.sh` ran is not in the backup: events, heartbeats, alerts, incidents, users, enrollment tokens, enrolled agents and audit rows.
- **Ingest's server certificate.** With no operator-provided pair, ingest issues a new pair from the restored internal CA, with the names of `CG_AGENT_SERVER_NAMES`; agents anchored on that CA accept it. With an operator-provided pair, the procedure above placed it before the first start. Verify that ingest serves that pair and that its actual issuing CA remains the endpoint anchor. If this step was missed, stop ingest and use [the placement procedure](configuration.md#placing-or-replacing-the-operators-pair) before reconnecting agents. An agent that encountered an untrusted replacement server certificate exits 6 and must be started again by hand.
- **`internal` mode.** The proxy's CA is new: browsers must trust its new root, as at installation ([install.md](install.md)).

### What `restore.sh` refuses

Each of these lines ends the run with exit status 1.

| Line | Cause and remedy |
|---|---|
| `restore.sh: usage: restore.sh <postgres dump .gpg> <clickhouse archive .gpg>` | Give it the two files, the Postgres dump first. |
| `restore.sh: no file <path>` | The path names no file. |
| `restore.sh: no .env: run me in the bundle's directory, with the backed-up installation's .env` | It was run in another directory, or `.env` is not in place. |
| `restore.sh: .env is not readable: run me as the user setup.sh ran as` | It was not run as root. |
| `restore.sh: gpg not found: it is a prerequisite of the host` | Install `gpg` 2.2.7 or later on the host. |
| `restore.sh: CG_BACKUP_PASSPHRASE is empty: this .env is not the backed-up installation's` | Use the backed-up installation's `.env`. |
| `restore.sh: <file> does not decrypt with this .env's passphrase, or gpg finds it manipulated` | The `.env` is not the one the backup was written under, or the file was changed. Nothing was started or stopped. |
| `restore.sh: the stores did not start` | The stores did not become healthy. Nothing was restored. |
| `restore.sh: Postgres is not empty (<n> tables in ingest and api): restore into a new stack` | The stores hold tables, an installation's or an earlier attempt's ([Starting again from empty stores](#starting-again-from-empty-stores)). No service was stopped and nothing was restored. |
| `restore.sh: ClickHouse is not empty (<n> tables in cyberguard): restore into a new stack` | As the row above. |
| `restore.sh: pg_restore failed` | The stores can hold part of the backup ([Starting again from empty stores](#starting-again-from-empty-stores)). |
| `restore.sh: decrypting <pg file> failed` | As the row above. |
| `restore.sh: writing the archive to the backup disk failed` | As the row above. |
| `restore.sh: decrypting <ch file> failed` | As the row above. |
| `restore.sh: RESTORE DATABASE failed` | As the row above. |
| `restore.sh: the archive could not be deleted from the backup disk: remove /backups/<archive> in the clickhouse container` | Both stores are restored, and the stack is not started. Remove the archive with `docker compose exec -T clickhouse rm -f /backups/<archive>`, then start the stack with `docker compose up -d --wait`. |
| `restore.sh: the stack did not start` | Both stores are restored. A service did not start: find its line in `docker compose logs --no-color --tail 60`, remove the cause, then run `docker compose up -d --wait`. For instance, without the relay's CA file in `relay/`, ingest stops with `cg-ingest: fatal: invalid configuration: INGEST_SMTP_CA_FILE: must name a readable file`. |

### Starting again from empty stores

When `restore.sh` stops after it has begun restoring, or when the stack was started on the empty stores before it, the stores can hold tables, which `restore.sh` refuses. Keep any logs needed for diagnosis. Check the project and its containers with `docker compose ps -a`: the following removal is only for the separate recovery attempt, whose stores hold nothing but that attempt, never the preserved original installation. It deletes every named volume of that recovery project, including `ingest_certs`.

<!-- backup: retry-clear -->

```sh
docker compose down -v --remove-orphans
```

**If the installation uses an operator-provided ingest pair, repeat [the stopped-container placement](configuration.md#placing-or-replacing-the-operators-pair) now.** The removal deleted its previous copy. Do not start ingest between placement and restore. Then run the restore again:

<!-- backup: retry-restore -->

```sh
sh restore.sh <dir>/cyberguard-postgres-<stamp>.dump.gpg <dir>/cyberguard-clickhouse-<stamp>.zip.gpg
```
