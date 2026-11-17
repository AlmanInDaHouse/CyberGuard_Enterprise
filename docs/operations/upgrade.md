# Upgrade

This document covers three cases: an installation of the bundle upgraded to a newer release; the agent on a Windows endpoint, which the agent's setup installed as a service, upgraded by a newer release's setup; and a deployment built from the repository before SPEC-023, which meets the changes SPEC-023 makes when it runs its code. The installation is in [install.md](install.md), the variables in [configuration.md](configuration.md), and the backups in [backup-and-restore.md](backup-and-restore.md).

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

## Upgrading the agent on an endpoint

The agent that the agent's setup installed ([install.md](install.md#on-the-endpoint-install-and-run-the-agent)) is upgraded by the setup of a newer release, run on the endpoint without a bootstrap file. On an endpoint whose agent is enrolled, the agent keeps its configuration, its trust anchor and its identity, and reports again as the same agent, with no token and no new enrollment (SPEC-024 §Operational §4, install_ac_011). On one whose agent never enrolled, the upgrade starts the service on the configuration already there, with its token, which may have expired meanwhile (42; not read for an upgrade). CI's tests have run the upgrade between two versions built from one source tree; an upgrade between two releases has not been run, and, like the installation's, these endpoint steps have not yet been run on an endpoint (SPEC-024 install_ac_016). Read the newer release's notes before scheduling it.

### The newer setup

Download the newer release's `cyberguard-agent-<version>-x64-setup.exe`, `cg-agent-<version>-x86_64-pc-windows-msvc.exe` and `SHA256SUMS`, and check them as [install.md](install.md#verify-the-files) does, with `sha256sum` and `gh attestation verify`: the executable's attestation ties its line in `SHA256SUMS`, which the check after the upgrade reads, to the release's build. Below, `<version>` is the newer release's version.

On the endpoint, open Windows PowerShell with *Run as administrator* and confirm with `net session` that it is elevated, as [install.md](install.md#on-the-endpoint-install-and-run-the-agent) does; a setup started without elevation copies its engine where the starting user's other processes can reach it. `$dir` is the inputs' directory the installation created ([A directory for the setup and its inputs](install.md#a-directory-for-the-setup-and-its-inputs)), `C:\CyberGuardSetup` or the path the installation chose instead. Before a setup runs elevated from it, check that the directory is still the one the installation created; this prints its attributes, its owner and its DACL:

<!-- upgrade: endpoint-directory -->

```powershell
$dir = 'C:\CyberGuardSetup'
$item = Get-Item -Force $dir
'{0} {1}' -f $item.Attributes, $item.GetAccessControl().GetSecurityDescriptorSddlForm('Owner, Access')
```

It prints `Directory O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)`. Stop if it prints anything else: a directory that is a reparse point, or has another owner or DACL, is one an account other than SYSTEM and Administrators may have changed, and a setup run elevated from it may not be the one checked. Use a new directory instead, created as [A directory for the setup and its inputs](install.md#a-directory-for-the-setup-and-its-inputs) does, with `$dir` set to it in every step below. When the directory is gone, create it as [A directory for the setup and its inputs](install.md#a-directory-for-the-setup-and-its-inputs) does, and copy the installed version's setup into it too, checked against its SHA-256 in the installation's change record.

Then copy the newer setup into the directory, beside the installed version's setup, where `<setup>` is its path on the endpoint, and check it against its line in `SHA256SUMS`:

<!-- upgrade: endpoint-copy -->

```powershell
Copy-Item '<setup>' "$dir\cyberguard-agent-<version>-x64-setup.exe"
(Get-FileHash -Algorithm SHA256 "$dir\cyberguard-agent-<version>-x64-setup.exe").Hash -eq '<its SHA-256 in SHA256SUMS>'
```

It prints `True`. Stop if it prints `False`. The upgrade needs neither the trust anchor nor a bootstrap file: if `bootstrap.toml` is still there, delete it as [install.md](install.md#install-with-a-bootstrap-file) does. Keep the installed version's setup in the directory until the upgrade has ended with 0 or 3010.

### Run the newer setup

<!-- upgrade: endpoint-setup -->

```powershell
$log = "$dir\upgrade-$(Get-Date -Format yyyyMMdd-HHmmss).log"
$run = Start-Process -FilePath "$dir\cyberguard-agent-<version>-x64-setup.exe" -ArgumentList '-quiet', '-norestart', '-log', $log -Wait -PassThru
$run.ExitCode
```

It is the installation's command without `CG_BOOTSTRAP` ([install.md](install.md#install-with-a-bootstrap-file)): `-quiet` shows no window, `-norestart` keeps the setup from restarting the computer, and `-log` writes Burn's log to `$log`, outside `C:\ProgramData\CyberGuard`. On an endpoint whose agent is enrolled, the setup takes no input: a bootstrap file is refused (30), and the setup run without `-quiet` shows its window but no dialog. A deployment tool that runs as SYSTEM runs the same command.

The setup runs these steps, as Burn's log names them:

1. Before any step, Burn checks each package the setup carries against its digest, and refuses a setup older than the setup installed (1638).
2. `Preflight` reads the endpoint's state and refuses an installed agent newer than this setup (29), both before it stops anything. It removes what an earlier attempt staged and its record, `agent.toml.setup` with any token in it among them, writes its own record, `setup-attempt.json`, then stops the service and waits up to 75 seconds for its stop (31). The stop ends the agent's capture ([The window without events](#the-window-without-events)).
3. `Agent`, the MSI, removes the older version and installs this one in one transaction: when it fails, Windows Installer rolls both back. Its Restart Manager closes a program that holds the agent's executable open (read for a test's own process; the SPEC names an agent run by hand from the executable among them, which has not been read). An executable that a program it cannot close holds is replaced at the next restart, which Windows Installer asks for: the setup then ends with 3017.
4. `Purge` changes nothing: an upgrade never purges, with `CG_PURGE=yes` too ([install.md](install.md#the-purge)).
5. `Configure` sets the service's failure actions, starts the service with no argument, deletes the record once the service control manager has accepted the start, and waits up to 60 seconds for the agent's identity line and its first acknowledged heartbeat, as on an installation.

On an enrolled endpoint the upgrade neither writes nor removes `agent.toml`, `server-ca.pem` or `identity\` in `C:\ProgramData\CyberGuard\agent` (from the setup's code; no test compares them across an upgrade), and the agent goes on writing its log in `logs\`. The service, which the MSI removes and installs again, comes back as the setup installs it, its start type automatic and its failure actions and security the setup's, and `Configure` starts it, one an administrator had stopped or set to start by hand included (read for a repair, which set back a start type changed to manual).

### The upgrade's result

The setup's exit code is the result. [The setup's result](install.md#the-setups-result) gives each code's hexadecimal value, as Burn's log writes it, and its cause. On an upgrade, a failure is followed by the installed setup's own run, which applies the older version's setup to the endpoint the failure left, without its MSI when Windows Installer still holds the older version or the newer one has superseded it: it may stop and start the agent, set its failure actions and start a service an administrator had left stopped. `setup.exe` exits with the newer setup's code whatever that run does, unless Windows Installer asked for a restart (3017).

| Code | The endpoint after it | Retry |
|---|---|---|
| 0 | The newer version installed and reporting, as the same agent. One setup is registered among the installed programs, the newer one (read). | None. Check the agent (below). |
| 3010 | As 0, and Windows Installer asked for a restart, which `-norestart` held (not read on an upgrade). | Restart the computer when convenient. |
| 3017 | As the step's code in Burn's log gives it, after a restart; the installed setup's run after it has not been read. | That code's, after a restart. |
| 1638 | Unchanged: no step ran, and the agent was not stopped (read). | None: there is no downgrade ([No downgrade](#no-downgrade)). |
| 29 | Refused before the stop; the installed setup's run follows, and its outcome has not been read. This setup's own 29, an agent installed newer than the setup installed, has not been read either. | Find the release of the executable in place by its SHA-256 (below), and run that release's setup or a newer one. |
| 30 | The older version still installed, and its agent running after the installed setup's run (read). | The command above, without `CG_BOOTSTRAP`. |
| 1603 | The MSI's changes rolled back; after the installed setup's run, the older version installed and its agent running (read). After another Windows Installer code, the installed setup's run follows the same way; its outcome has not been read. | Read the MSI's log, correct its cause, run the newer setup again. |
| -2146889721 | Unchanged: nothing ran (read on an installation, not on an upgrade). | Download and verify the newer setup again. |
| 20 to 28, 31 | Refused by the newer setup's checks. The installed setup's run follows; its outcome has not been read. After 31, the service may be left stopped. | Correct the cause [The setup's result](install.md#the-setups-result) gives for that code, then run the newer setup again, without input on an enrolled endpoint. |
| 40 to 43 | The newer version may be installed (40 after the MSI, 41 to 43 always). The installed setup's run follows; its outcome has not been read. | Correct the cause [The setup's result](install.md#the-setups-result) gives for that code, check the agent (below), then run the newer setup again, without input on an enrolled endpoint: the SPEC's retry, whose run has not been read. |
| 44 | The newer version installed and its agent running, though its identity line and acknowledged heartbeat were not both seen within the 60 seconds. The installed setup's run refused the downgrade (29) before it changed anything, and both setups stay registered (read). | Read the agent's log ([install.md](install.md#check-that-the-agent-reports)), then run the newer setup again: the SPEC's retry, whose run has not been read. |
| 45 | The newer version installed, but a rename onto the agent's executable, such as one an earlier run left, waits for the next restart. Until then another executable stays in place, and the installed setup's run may start it under the newer version (not read). | Restart the computer, then run the newer setup again: the SPEC's retry, whose run has not been read. |
| 1602 | Cancelled; on an upgrade, its outcome has not been read, and the service may be left stopped. | Check the agent first, then run the newer setup again (not read). |

A code that is not in the table is Burn's own or Windows Installer's: Burn's log gives the step it came from.

After a failure the version installed may not be the one the change record holds. The executable in place is the release whose line in `SHA256SUMS` its SHA-256 matches, read as [install.md](install.md#check-that-the-agent-reports) reads it; after 40 and 45 it may not be the version installed. While both setups are registered (after 44, read; after 41 to 43 and 45, and 40 after the MSI, not read), which of them uninstalls the agent has not been read: complete the upgrade before an uninstallation.

On an upgrade, Burn's log holds more lines than an installation's ([Burn's log](install.md#burns-log)): the steps the newer setup rolled back, and the installed setup's own run, which a line naming that setup's bundle code, a GUID in braces, gives. These carry them:

<!-- upgrade: endpoint-log -->

```powershell
Select-String -Path $log -Pattern 'Applied execute package', 'Applied rollback package', 'Applied non-vital package', 'Apply complete', 'Exit code'
```

- `Applied execute package:` with a bundle code in braces is the older setup, which a successful upgrade removes; its result is `0x0` (read).
- `Applied rollback package:` is a step the failed run reversed, or, with a bundle code in braces, the installed setup's own run (read); by the SPEC, `Applied non-vital package:` with that code is that run failed (not read).
- The other lines read as on an installation.

### Check the upgraded agent

Check the agent as [Check that the agent reports](install.md#check-that-the-agent-reports) does, with the newer release's `SHA256SUMS`: the installed executable matches the line of the newer `cg-agent-<version>-x86_64-pc-windows-msvc.exe`; the Application log's latest entry 1000 names the newer version, after an entry 1001 or 1003 of the older version's stop, which counts the events it did not deliver; and, on an enrolled endpoint, the agent's log has an `agent identity ready` line written after the upgrade began, and no `enrollment response received` line written since: the agent loaded the identity it had, and enrolled nothing. On an endpoint whose agent never enrolled, the agent enrolls with its token, as on an installation, or stops with code 3 when the token has expired (42).

Then record in the installation's change record the newer version, its setup's SHA-256, the installed executable's SHA-256, the setup's exit code and Burn's log. The newer setup, in the inputs' directory, is now the one to uninstall with ([install.md](install.md#the-setup-to-uninstall-with)); once the upgrade has ended with 0 or 3010, the older setup can be deleted from the directory.

### A repair

A repair runs the installed version's setup again, with `-repair`. It puts back the agent's executable, the service's start type and its failure actions, with no new enrollment (read), and resets neither the configuration, the trust anchor nor the identity (SPEC-024 §Operational §4). Open the elevated PowerShell, check the inputs' directory as [the upgrade's first block](#the-newer-setup) does, and check the installed version's setup in it as [The setup to uninstall with](install.md#the-setup-to-uninstall-with) does; then, with `<version>` the installed version:

<!-- upgrade: endpoint-repair -->

```powershell
$log = "$dir\repair-$(Get-Date -Format yyyyMMdd-HHmmss).log"
$run = Start-Process -FilePath "$dir\cyberguard-agent-<version>-x64-setup.exe" -ArgumentList '-repair', '-quiet', '-norestart', '-log', $log -Wait -PassThru
$run.ExitCode
```

It runs `Preflight`, the MSI and `Configure`, as an upgrade does, and its codes are [The setup's result](install.md#the-setups-result)'s. Run without `-repair`, the installed version's setup has not been read. A second build of the same version, which a rebuild of a release would be, replaces the first as an upgrade does (read).

### No downgrade

There is no downgrade. A setup older than the setup installed is refused (1638) before any step runs, and one older than the agent installed (29) before it stops anything, though the installed setup's run follows a 29. An uninstallation followed by an older version's setup is not a route these runbooks give: whether an older agent reads what a newer one wrote has not been read. After a failed upgrade, the retry is the newer setup, as the table above gives it.

### Settings an administrator set in agent.toml

The keys of [The agent's tuning](configuration.md#the-agents-tuning) that an administrator set in `agent.toml` stay through an upgrade, a repair and the installed setup's run after a failed upgrade, on an endpoint whose agent is enrolled: none of them writes `agent.toml` there (from the setup's code; no test compares the file across an upgrade). On an endpoint whose agent never enrolled, a setup given a bootstrap file or the wizard's input replaces `agent.toml`, and those keys with it.

A `[log] level` above `info`, such as `warn`, hides the agent's identity and heartbeat lines, which `Configure` waits for at `info`: an installation, upgrade or repair that starts the service on that configuration then ends with 44 (inferred from the setup's code, not read), and an upgrade that ends with 44 leaves both setups registered (read). Keep the level at `info` on an endpoint the setup maintains.

### The window without events

From the service's stop in `Preflight` until `Configure` starts the newer version's service, the endpoint captures nothing, and the events the agent has not delivered when its stop ends are lost ([install.md](install.md#the-window-without-events)). After a failure, the installed setup's own run may stop and start the agent again, and a result that may leave the service stopped or not running (31, 40 to 43, 45, 3017, 1602) keeps the window open until the service starts.

## A deployment built before SPEC-023

The bundle comes with SPEC-023, and no release of it predates SPEC-023's code. Moving a deployment built from the repository before SPEC-023 to the bundle is a new installation, from [install.md](install.md), whose stores start empty and whose CA is new; its agents must enroll again (*Agents*, below). A stack that keeps running the services built from the repository, the development stack of `infra/dev/` among them, meets the changes below when it runs SPEC-023's code. For such a stack, in order:

1. Before SPEC-023's code first runs, set the retention periods (*Retention*) and write the integer variables in digits (*Integer values*).
2. Stop the stack and preserve its databases, configuration and certificates before planning the schema transition (*The database*). The development stack's disposable database has its own recreation procedure.
3. Start SPEC-023's code, and create an admin, as [install.md](install.md#create-the-first-admin) does.
4. Each agent must enroll again, which these runbooks do not give yet (*Agents*).

### Retention

SPEC-023's migration step sets a retention on `cges_events`, per class, and on `heartbeats` ([configuration.md](configuration.md#retention)), whether ingest runs it (`INGEST_RUN_MIGRATIONS=true`) or `node dist/db/migrate.js` does. A store that code from before SPEC-023 created had one period at most, 365 days on the logons (class 3002), measured from arrival. The first run of SPEC-023's migration step over such a store sets the new periods and materialises them: with the defaults, it deletes the heartbeats that arrived more than 30 days before and the events that arrived more than 365 days before, at ClickHouse's merges.

To keep more, set ingest's `INGEST_RETENTION_PROCESS_DAYS`, `INGEST_RETENTION_NETWORK_DAYS`, `INGEST_RETENTION_LOGON_DAYS` and `INGEST_RETENTION_HEARTBEAT_DAYS` before that run, each from 7 to 3650 days. The development compose passes no retention variable to ingest, so its ingest runs the defaults: keeping more there takes setting those variables in ingest's `environment` in `infra/dev/docker-compose.dev.yml`.

### Integer values

Ingest and the api read each integer variable they check, the ports, `INGEST_SMTP_PORT`, the detection interval, the session lifetimes and the retention periods, in ASCII decimal digits alone; leading zeros are digits. A value written otherwise, with a sign, whitespace, an exponent, a fraction or a prefix such as `0x`, is refused at start, as a value out of bounds is, with `cg-ingest: fatal: invalid configuration: <variable>: <reason>` or `cg-api: fatal: invalid configuration: <variable>: <reason>`. Code from before SPEC-023 accepted such a form for the ports and the interval when it denotes an integer, for instance `1e3`, `+8080`, `8080.0`, `0x1F90` or a value with a leading space. It also read an empty `INGEST_DETECT_INTERVAL_MS` as 0, the kill switch, which is now refused: write `0`. The interval is at most 2147483647.

### The database

Code from before SPEC-023 created its Postgres tables in the schema `public`. The services now keep theirs in the schemas `ingest` and `api`, each connection's search path naming its own (SPEC-023 §Operational §4). On such a database, the first run of SPEC-023's migrations creates the services' tables again in those schemas, empty, beside the old ones, which are no longer read. What the old tables hold is then lost to the services, whether or not the database is recreated afterwards; among it:

- the agents and their enrollment tokens, the alerts, the incidents, the playbook records, the users and the audit log are no longer read;
- the CA and the forensic key are no longer read either: ingest generates a new CA at its next start, and the api a new forensic key, whose fingerprint is recorded as [forensic-verification.md](forensic-verification.md#recording-the-fingerprint-at-installation) says; the agents must enroll again (*Agents*, below);
- the detection cursor starts again: once an agent of an org has enrolled again, the rules read that org's process events (class 1007) again from the first one ClickHouse keeps and write their alerts and incidents again, and for each incident created again ingest sends its notification, when notification is set up, and runs its playbooks, when playbooks are loaded.

There is no automatic import from `public`. Preserve the old database and keys for access to the old records; starting new tables is not a migration of that evidence. For a non-development deployment, plan and validate the data transition separately before running the newer services. Do not delete its database on the strength of this runbook. The development stack's recreation step below is for its disposable data only.

If the transition creates a new internal CA, ingest also needs a server certificate that CA issued. Unless its pair is operator-provided, retain a protected copy of the old pair, then remove the active server certificate and key, the files `INGEST_SERVER_CERT_PATH` and `INGEST_SERVER_KEY_PATH` name (`server.pem` and `server-key.pem` in ingest's certificates volume), while ingest is stopped: at its next start ingest issues a pair from its CA. A pair the old CA signed is not one its current CA signed, so ingest never rewrites it (SPEC-023 §Operational §6): it goes on serving it, and an agent anchored on the new CA ends with exit code 6.

For the development stack, [infra/dev/README.md](../../infra/dev/README.md) gives the commands that remove its database volume and its certificates volume.

### Agents

An agent of such a deployment was run by hand, before the agent's setup (SPEC-024), with a client certificate the old internal CA issued. The new installation has its own CA: a new installation from the bundle has its own, and a stack that starts with the new, empty schemas generates a new one when ingest starts (*The database*, above). So each agent must enroll again, and its server trust anchor changes to the new internal CA only when that CA also issues the server pair; for an operator-provided server pair, its actual issuer stays the anchor. With an internally issued server pair, until the agent enrolls again:

- while ingest serves a certificate the new CA issued, the agent refuses it and exits with code 6, its line beginning `cg-agent: tls: server certificate verification failed:`;
- while ingest still serves the pair the old CA signed, the heartbeat listener refuses the agent's certificate, which the new CA did not issue, and the agent exits with code 7, its line `cg-agent: tls: server rejected client certificate`.

Ingest's enrollment listener serves HTTPS, TLS 1.3 only, by default, and the agent enrolls over TLS, trusting the roots of its trust anchor alone (SPEC-023 §Operational §6). An agent keeps its `[enrollment]` section after it enrolls, with the token alone removed. With `[enrollment]`, `server.url` must be `https://` and `server.trust_anchor_path` must be set:

- an agent whose `agent.toml` keeps an `http://` `server.url` beside `[enrollment]` exits with code 2 at its next start, and its line on standard error ends with `server.url must use https when [enrollment] is set`;
- an agent without `server.trust_anchor_path` exits with code 2, its line ending with `missing key 'server.trust_anchor_path'`.

Enrolling such an agent again with the new installation is not in these runbooks yet. The agent's setup refuses an endpoint that holds an enrolled agent installed by hand, its identity's files directly in `C:\ProgramData\CyberGuard\agent` (code 28), until SPEC-024's migration moves that installation into the service ([install.md](install.md#the-setups-result)), a procedure the runbooks do not give yet; the migration keeps the agent's identity, the one the old CA issued; and a setup run on an endpoint whose agent is enrolled enrolls it neither again nor with a new token (code 30). Keep each such agent's directory as it is until that procedure is given: [install.md](install.md#the-agents-exit-codes) says why it is not removed.

### Browsers

The dashboard's cookies are named `__Host-cgsess` and `__Host-cgcsrf`, and the dashboard reads no other name. A browser that holds the earlier `cgsess` and `cgcsrf` is signed out, and its user signs in again. The api session behind such a cookie stays in the key-value store until its lifetime ends.

### The development stack

The development stack's own steps are in [infra/dev/README.md](../../infra/dev/README.md): its database, recreated once; its Redis volume, which Valkey does not load; and its certificates volume, which an ingest running as root wrote and the non-root ingest cannot use, removed once, which also gives ingest a pair from its new CA (*The database*, above). The bundle's ingest has never run as root, so the bundle has no such step.
