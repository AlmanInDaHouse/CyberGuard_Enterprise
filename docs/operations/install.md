# Installing CyberGuard

This document installs CyberGuard Enterprise on one Linux host from the bundle a release publishes, creates its first admin, installs a first agent on a Windows endpoint with the agent's setup, and removes an agent from an endpoint (SPEC-023, SPEC-024, ADR-0022). `<version>` is the release's version, whose tag is `v<version>`.

Each command on the server runs as root in the bundle's directory unless its step says otherwise. The bundle's `.env` is mode `0600` and owned by root, and `docker compose` reads it there, so `docker compose` runs as root too.

On the endpoint the setup installs the agent as the Windows service `CyberGuardAgent`, which runs as LocalSystem and starts with Windows. The product supports the agent the setup installs, and nothing else. An earlier version of this document installed the agent by hand, its files directly in `C:\ProgramData\CyberGuard\agent`: that console installation is a development layout, which no release of the setup writes and the product neither adopts nor migrates. It exists only on a development host, which is formatted, not migrated, before the product is installed on it; the setup refuses it (code 28), and the agent run by hand on an endpoint, the installed executable's console mode included, is outside support. On an endpoint the setup installed, its names are another account's or an administrator's ([No repair, and no removal by hand](#no-repair-and-no-removal-by-hand)).

## Before you start

### The server

- One Linux x86-64 host with rootful Docker Engine 29.1.2 or later and Docker Compose 2.40.3 or later. Use maintained, patched versions at or above these support floors. Docker Desktop is a development environment, not this production installation's host.
- `gpg` 2.2.7 or later, which `backup.sh` and `restore.sh` use ([backup and restore](backup-and-restore.md)).
- The GitHub CLI, `gh`, which verifies the release's attestations.
- GNU coreutils (`sha256sum`), `tar`, a POSIX shell and `iproute2` (`ip route`).
- A clock kept in time by NTP, as on the endpoint: ingest refuses a heartbeat whose time is more than 5 minutes from its own.

Check the installed versions before continuing:

<!-- deploy_ac_021: versions -->

```sh
docker version --format '{{.Server.Version}}'
docker compose version
gpg --version
gh --version
```

The release has not shipped yet: F2's installer, F3's pages and F4's renewal,
then the owner's release approval, still gate the first public installation.
These instructions are tested with locally built images; the first release must
also pass the published-asset verification and timed installation of SPEC-023.
Local developer tests may use a nested Docker daemon with a different Engine
version; those runs do not certify the production support floor. CI checks the
floor separately and records the versions it exercised.

### Names, ports and the firewall

- A DNS name for the dashboard, which users' browsers resolve to the host: `CG_DASHBOARD_HOSTNAME`. In `provided` TLS mode, a certificate that names it, with its private key.
- The names and addresses the agents reach the server by: `CG_AGENT_SERVER_NAMES`, from one to twenty entries, each a DNS name of at most 253 characters or an IPv4 or IPv6 address. Ingest's server certificate carries them, and an agent's URLs use one of them.

The bundle publishes four ports, each on `CG_BIND_ADDRESS` (`0.0.0.0` by default), and no other:

| Port (default) | Variable | Reached by | What answers |
|---|---|---|---|
| 443 | `CG_HTTPS_PORT` | Browsers | The proxy, serving the dashboard over HTTPS |
| 80 | `CG_HTTP_PORT` | Browsers | The proxy, redirecting to HTTPS |
| 8444 | `CG_ENROLL_PORT` | Agents | Ingest's enrollment, over TLS 1.3 |
| 8443 | `CG_HEARTBEAT_PORT` | Agents | Ingest's heartbeats, over mutual TLS |

The api, the dashboard's own port and the three stores are on the stack's internal networks alone. The browsers that use the dashboard must reach the first two ports, and the endpoints whose agents report to the server the other two. The bundle sets no firewall rule of its own. Ingest terminates the agents' TLS itself: put no proxy or load balancer that terminates TLS in front of its two ports.

### The Windows endpoint

- 64-bit Windows 10 version 1809 (build 17763) or later, or Windows Server 2019 or later: the setup refuses an earlier Windows 10 build (code 20), and does not run its code on a Windows before 10. The setup has been run by CI's tests on GitHub's hosted Windows Server runners, and the agent on Windows 11. Windows 10 and Windows Server 2019 have not been tested, and these endpoint steps have not yet been run on an endpoint (SPEC-024 install_ac_016).
- An administrator's account, to run the setup elevated. The agent runs as the service, as LocalSystem.
- A clock kept in time by NTP.
- Logon auditing on in its audit policy: an endpoint whose policy does not log logons sends no logon events and reports no error (SPEC-020 §Risks). The setup does not change it; [Record the endpoint's audit and dump settings](#record-the-endpoints-audit-and-dump-settings) reads it.
- A route to `CG_ENROLL_PORT` and `CG_HEARTBEAT_PORT` on a name of `CG_AGENT_SERVER_NAMES`.
- No `C:\ProgramData\CyberGuard` but the setup's: the setup creates it with its own protection, and refuses one that is owned by an account other than SYSTEM or Administrators or gives any access, reading included, to another account (code 27).
- No `C:\Program Files\CyberGuard` that an account other than SYSTEM and Administrators can change: the setup does not check one that exists, which keeps its own security, and the agent's executable, which runs as LocalSystem, is installed in it (SPEC-024 Amendment 2026-11-12, point 3).
- An endpoint imaged before the setup runs on it, never after: the agent's key is sealed with the machine's own keys, which a copy of the system volume carries, so a copy of an enrolled endpoint's disk, an image a deployment tool or a virtual desktop's template takes included, presents the same agent, which the server cannot tell from the original (R18 of the [threat model](../security/threat-model.md)). A copy of an enrolled endpoint, an image or a clone, is purged and installed anew with a new token, as [The setup's result](#the-setups-result) says after its table, its old certificate staying valid on the server until it expires or its row is deleted (R5).
- The system volume encrypted, for instance with BitLocker: that is the deployment's control against an offline read of the agent's key, a lost or stolen computer's or a virtual machine's backup (R18). The setup does not check it.

## Download the release

From the release `v<version>` on the repository's releases page, `https://github.com/AlmanInDaHouse/CyberGuard_Enterprise/releases`, download into one directory of the server:

- `cyberguard-deploy-<version>.tar.gz`, the bundle;
- `cyberguard-agent-<version>-x64-setup.exe`, the agent's setup, which installs it on the endpoint;
- `cg-agent-<version>-x86_64-pc-windows-msvc.exe`, the agent's executable, which the setup carries: its line in `SHA256SUMS` is the digest the installed agent is checked against;
- `SHA256SUMS`, the SHA-256 of the release's six payload assets, in `sha256sum`'s format.

The release also carries `licences-<version>.md`, the licence report of its dependencies, with the
notices and MS-RL's text of the WiX files the setup carries; `cyberguard-agent-<version>-x64.msi`,
the MSI the setup carries, published to be verified and not as a way to install: run on its own, it refuses to
install (1603), and only the setup installs it with the agent's configuration and starts the service; and
`wix-<release>-source.tar.gz`, the source of those WiX files and of the setup's own (SPEC-024
Amendment 2026-11-14).

### Verify the files

In the directory the files were downloaded to:

<!-- deploy_ac_021: verify -->

```sh
sha256sum --check --ignore-missing SHA256SUMS
gh attestation verify cyberguard-deploy-<version>.tar.gz --repo AlmanInDaHouse/CyberGuard_Enterprise
gh attestation verify cyberguard-agent-<version>-x64-setup.exe --repo AlmanInDaHouse/CyberGuard_Enterprise
gh attestation verify cg-agent-<version>-x86_64-pc-windows-msvc.exe --repo AlmanInDaHouse/CyberGuard_Enterprise
```

`sha256sum` prints `OK` beside each file it checks, and skips those of `SHA256SUMS` that were not downloaded. `gh attestation verify` checks the build-provenance attestation the release's workflow made for the file: the executable's ties its line in `SHA256SUMS`, which is not attested itself, to the release's build. Stop if any check fails.

Neither the setup nor the agent's executable is code-signed (ADR-0022 Open question 1): Windows SmartScreen may warn when the setup starts, and its elevation prompt names no verified publisher. Their integrity rests on these checks and on the ones the endpoint makes, before it runs the setup and after it; a checksum alone authenticates no publisher. The bundle's compose names each image by its digest.

### Unpack the bundle

<!-- deploy_ac_021: unpack -->

```sh
tar -xzf cyberguard-deploy-<version>.tar.gz
cd cyberguard-deploy-<version>
```

This directory is the installation's: its `.env`, its backups (`backups/` by default) and every command that follows are here.

## Set up `.env`

<!-- deploy_ac_021: setup-template -->

```sh
sh setup.sh
```

The first run writes `.env` from `.env.example`, with mode `0600`, and stops with exit status 1:

<!-- deploy_ac_021: output-setup -->

```text
setup.sh: wrote .env from .env.example; set CG_DASHBOARD_HOSTNAME, CG_AGENT_SERVER_NAMES and CG_TLS_MODE in it, then run setup.sh again
```

Set these lines in `.env`:

<!-- deploy_ac_021: config-hostnames -->

```dotenv
CG_DASHBOARD_HOSTNAME=<dashboard hostname>
CG_AGENT_SERVER_NAMES=<agent server names, comma-separated>
CG_TLS_MODE=provided
```

- With `CG_TLS_MODE=provided`, the proxy serves your certificate. Put it in `tls/dashboard.crt`, in PEM, the leaf first and its chain after it, readable by uid 10001, the proxy's user; put its private key in `tls/dashboard.key`.
- With `CG_TLS_MODE=internal`, the proxy issues a certificate from a CA of its own. Browsers warn until that CA's root is installed in them: the mode is for an evaluation, and production uses `provided`.

The other variables keep their defaults. When a port of the table above is taken on the host, or the published ports must bind one address, [configuration](configuration.md) gives the variables. A second installation on the same host sets, before its first start, its own project name (`COMPOSE_PROJECT_NAME` in its `.env`), its own `app` addresses and its own published ports ([configuration](configuration.md)).

### Check the `app` network's addresses

The dashboard reaches the api on an internal network, `app`, whose addresses `.env` sets:

<!-- deploy_ac_021: config-network -->

```dotenv
CG_APP_SUBNET=10.254.254.0/28
CG_APP_IP_RANGE=10.254.254.8/29
CG_DASHBOARD_APP_ADDRESS=10.254.254.2
```

Before the first start, list the networks the host routes:

<!-- deploy_ac_021: routes -->

```sh
ip route
```

When a route the list shows overlaps `CG_APP_SUBNET`, set the three variables to a subnet the host does not route, with `CG_APP_IP_RANGE`, the addresses Docker hands out, inside it, and `CG_DASHBOARD_APP_ADDRESS` inside it and outside that range. The compose sets the dashboard's address and the api's `API_TRUSTED_PROXIES` from `CG_DASHBOARD_APP_ADDRESS`. Changing them once the stack has started is in [configuration](configuration.md).

### Fill the secrets

<!-- deploy_ac_021: setup-secrets -->

```sh
sh setup.sh
```

The second run fills each empty secret with 32 random bytes in lowercase hexadecimal and keeps any value already set. In `provided` mode it gives `tls/dashboard.key` to the proxy's user, uid 10001, with mode `0400`. It prints the names of what it filled, never a value, and exits with status 0:

<!-- deploy_ac_021: output-secrets -->

```text
setup.sh: filled: CG_PG_SUPERUSER_PASSWORD CG_PG_INGEST_OWNER_PASSWORD CG_PG_INGEST_PASSWORD CG_PG_API_OWNER_PASSWORD CG_PG_API_PASSWORD CG_CH_ADMIN_PASSWORD CG_CH_INGEST_PASSWORD CG_CH_API_PASSWORD CG_REDIS_INGEST_PASSWORD CG_REDIS_API_PASSWORD CG_INGEST_CA_PASSPHRASE CG_API_DB_ENC_PASSPHRASE CG_API_FORENSIC_PASSPHRASE CG_BACKUP_PASSPHRASE
```

While a hostname is empty, or `provided` mode lacks a TLS file, it changes nothing, names what is missing (for instance `setup.sh: missing: tls/dashboard.crt tls/dashboard.key`) and exits with status 1. Once the secrets are filled, a further run changes nothing and prints `setup.sh: nothing to fill`. When you replace `tls/dashboard.key` later, run `sh setup.sh` again, which gives the new key to the proxy's user ([configuration](configuration.md)).

Keep a copy of `.env` apart from the backups. It is the installation's master secret: it holds the keys to the CA, to the users' TOTP secrets and to the forensic key, which the database holds encrypted, and `CG_BACKUP_PASSPHRASE`, which the backups are encrypted under. Losing it with no copy loses the CA, the TOTP secrets and the forensic key (ADR-0022 §Consequences), and leaves every backup encrypted under a passphrase no one holds.

## Start the stack

Two points come before the first start.

- **Ingest's server certificate.** At its first start ingest issues its server certificate from its own CA, for the names of `CG_AGENT_SERVER_NAMES`, and it issues a new one at a start when the list has changed; the agents trust the CA, so they need no change. A pair another CA issued must be in the `ingest_certs` volume before ingest's first start (SPEC-023 §Operational §6), and the `docker compose up` below is that start. This document installs with ingest's own certificate, and gives no step that places another pair; [configuration](configuration.md) describes an operator-provided pair.
- **The clients' addresses.** Before starting the stack, merge these settings
  into the rootful Linux daemon's `/etc/docker/daemon.json`, preserving its other
  settings. Arrange a maintenance window to restart Docker through the host's
  service manager: the restart can interrupt other containers on that host.

  <!-- deploy_ac_021: config-daemon -->

  ```json
  {
    "userland-proxy": false,
    "ip-forward": true,
    "iptables": true
  }
  ```

  Use direct IPv4 access and an IPv4 `CG_BIND_ADDRESS`, with no intervening
  proxy or source NAT. This procedure does not certify hairpin or IPv6-only
  routes. Disabling the userland proxy is not a guarantee for every network
  path: verify the actual route below. Docker's [daemon reference](https://docs.docker.com/reference/cli/dockerd/)
  and [port publishing guide](https://docs.docker.com/engine/network/port-publishing/)
  describe these settings. Keep the api and stores unpublished.

Then start the stack:

<!-- deploy_ac_021: start -->

```sh
docker compose up -d --wait
```

It pulls the images and initialises the three stores on their empty volumes. It runs the migration one-shots, `ingest-migrate` and then `api-migrate`, and starts ingest once `ingest-migrate` has completed, the api once both have, then the dashboard and the proxy, each once the service before it is healthy. When it returns with status 0, the seven services are healthy and the two one-shots have exited with status 0. At its first start ingest generates its CA and its server certificate, and the api the forensic key.

When it fails, the services' last lines show why:

<!-- deploy_ac_021: diagnostics -->

```sh
docker compose logs --no-color --tail 60
```

A service variable that ingest, the api or a one-shot refuses stops it with one line that names the variable and the reason, never the value: `cg-ingest: fatal: invalid configuration: <VAR>: <reason>`, `cg-api: fatal: invalid configuration: <VAR>: <reason>`, or, for the one-shots, the same after `cg-ingest migrate: fatal:` or `cg-api migrate: fatal:`. The dashboard's line is `cg-dashboard: fatal: invalid configuration: API_BASE_URL: <reason>`. `<VAR>` is the service variable the compose sets from a bundle variable, `INGEST_RETENTION_LOGON_DAYS` for `CG_RETENTION_LOGON_DAYS` for instance; [configuration](configuration.md) maps one to the other. Other refusals come from Docker Compose, Docker or the proxy, each with its own message: a secret still empty or a hostname unset makes `docker compose` refuse to start, naming the variable, with `run setup.sh`; a `CG_TLS_MODE` other than `provided` or `internal` stops the proxy; and a published port, `CG_BIND_ADDRESS` or an `app` address that Compose or Docker refuses stops `docker compose up`.

### Check the clients' addresses

From another host on each intended client network, open the dashboard's
`/login` and ingest's `https://<agent server name>:<CG_ENROLL_PORT>/health`
with a client that trusts the corresponding issuer. After the agent below
enrolls, also check its heartbeat requests. Read the server's logs:

<!-- deploy_ac_021: addresses -->

```sh
docker compose logs --no-color --no-log-prefix proxy ingest
```

The proxy's JSON access records carry `request.remote_ip`; ingest's incoming
request records carry `req.remoteAddress`. Compare them with that remote
host's address. Stop and correct the network path if they show a proxy or
gateway address instead. A loopback request from the server is not this check.
At a log level above `info`, temporarily use `CG_LOG_LEVEL=info` for ingest's
request records. This checks the real deployment route; the bundle's local
tests do not certify the buyer's network.

## Record the forensic key's fingerprint

<!-- deploy_ac_021: forensic-key -->

```sh
docker compose exec -T api node dist/cli/forensic-key.js
```

It prints the forensic public key in base64url, then its fingerprint, the SHA-256 of the key's 32 bytes:

<!-- deploy_ac_021: output-fingerprint -->

```text
<public key in base64url>
sha256:<64 lowercase hexadecimal characters>
```

Record the `sha256:` line outside CyberGuard: in the installation's change record, signed by whoever performed the installation, and in a copy handed to whoever audits the deployment. An auditor checks a forensic export against that record, never against the key the export carries ([forensic verification](forensic-verification.md)).

## Create the first admin

<!-- deploy_ac_021: admin -->

```sh
docker compose exec api node dist/cli/create-user.js --email <email>
```

The command asks for the password twice, without echo, and refuses one that is not 12 to 128 characters with a letter and a digit, before it writes anything. It then writes the user, an admin, and prints the TOTP provisioning URI once, an `otpauth://` URI. Add it to the admin's authenticator, then type the authenticator's current code at the `Current TOTP code:` prompt; the command answers `cg-api: the TOTP factor is confirmed`. After three invalid codes it leaves the factor unconfirmed and prints the user's id and how to confirm it later ([Recover an interrupted confirmation](#recover-an-interrupted-confirmation)). A user whose factor is unconfirmed cannot sign in.

The command refuses `--password` (exit status 2): the password is read from standard input alone. The provisioning URI is a secret on your terminal: keep no copy of it once the authenticator holds it.

Sign in at `https://<dashboard hostname>/login`, adding `:<CG_HTTPS_PORT>` when it is not 443, with the address, the password and a current code. The proxy redirects a request on `CG_HTTP_PORT` to HTTPS. Over HTTPS it answers a request whose `Host` names another host with `421 Misdirected Request`, and forwards it nowhere: reach the dashboard by `CG_DASHBOARD_HOSTNAME`.

## Enroll the first agent

### On the server: the trust anchor and a token

<!-- deploy_ac_021: ca -->

```sh
docker compose exec -T ingest node dist/cli/export-ca.js
docker compose exec -T ingest node dist/cli/export-ca.js > server-ca.pem
```

The first command prints ingest's CA certificate, in PEM, and, last, its fingerprint: `sha256:` and the SHA-256 of the certificate's DER in lowercase hexadecimal, which lets you compare the file an agent holds with the server's CA. Record that `sha256:` line beside the forensic key's fingerprint, in the installation's change record: the endpoint's setup checks the trust anchor against it. The second writes the same output to `server-ca.pem`, the agent's trust anchor: the setup reads the certificate from the file and ignores the `sha256:` line after it, which is no check of the file. When ingest has no CA yet, the command prints `cg-ingest export-ca: no CA yet: ingest generates it at its first start` on standard error and exits with status 1, and the second leaves `server-ca.pem` empty, which the setup refuses (code 25).

Issue the token last, once the endpoint is ready for it ([Install with a bootstrap file](#install-with-a-bootstrap-file), or the wizard):

<!-- deploy_ac_021: token -->

```sh
docker compose exec -T ingest node dist/cli/issue-token.js
```

It prints the token on standard output, and on standard error `cg-ingest: token issued for org 'default', expires <time>`. The token is single-use and valid for 15 minutes, within which the agent must enroll with it. It is a secret on your terminal: carry it to the endpoint by a channel you trust. A token that has expired or been used is refused at the agent's enrollment, whose service then stops with code 3, and the setup exits 42: issue another.

### On the endpoint: install and run the agent

The setup, `cyberguard-agent-<version>-x64-setup.exe`, installs the agent and its service, writes the agent's configuration, starts the service and waits up to 60 seconds for the agent to enroll and for its first heartbeat to be acknowledged. It takes its inputs from a bootstrap file, with no window, or from its wizard, a dialog that asks for them; both are checked the same way, and neither puts the token on a command line, in the environment or in a log (SPEC-024 §Data contracts §3). The setup changes neither the endpoint's firewall, audit policy or antivirus settings nor Windows' trust store.

Run it from an elevated PowerShell, as below. A setup started from Explorer, without elevation, copies its engine into the starting user's temporary folder before the elevation prompt, where that user's other processes can reach it; started elevated, it does not (SPEC-024 Amendment 2026-11-12 (second), point 10). A deployment tool that runs as SYSTEM runs the bootstrap file's command below, and reads its result as the exit codes of [The setup's result](#the-setups-result) give it.

Open Windows PowerShell (`powershell.exe`, which every supported Windows includes) with *Run as administrator*, and confirm that it is elevated. The steps below use .NET Framework calls that create a directory and a file with their security, which PowerShell 7 does not have:

<!-- deploy_ac_021: endpoint-elevation -->

```powershell
net session
```

An unelevated PowerShell answers it with access denied.

#### A directory for the setup and its inputs

The setup, the trust anchor and the bootstrap file, which holds the token, go in a new local directory that SYSTEM and Administrators alone can open, created with that protection before anything is put in it. This creates `C:\CyberGuardSetup`, owned by Administrators with a protected DACL for SYSTEM and Administrators, then prints its attributes, its owner and DACL, and the number of entries it holds:

<!-- deploy_ac_021: endpoint-directory -->

```powershell
$dir = 'C:\CyberGuardSetup'
$security = New-Object System.Security.AccessControl.DirectorySecurity
$security.SetSecurityDescriptorSddlForm('O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)')
$null = [System.IO.Directory]::CreateDirectory($dir, $security)
$item = Get-Item -Force $dir
'{0} {1} {2}' -f $item.Attributes, $item.GetAccessControl().GetSecurityDescriptorSddlForm('Owner, Access'), @(Get-ChildItem -Force $dir).Count
```

It prints `Directory O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA) 0`. Stop if it prints anything else: a `C:\CyberGuardSetup` that existed before keeps its own owner, DACL and entries, which `CreateDirectory` does not change. Remove it, or set `$dir` to another new path, without spaces, on a local volume, whose parent directories only administrators can change, in every step below. Keep the directory out of `C:\ProgramData\CyberGuard`, which the setup creates itself. Every step below runs in this PowerShell, whose `$dir` it uses.

Bring the setup and `server-ca.pem`, verified on the server above, to the endpoint, and copy them into the directory, where `<setup>` and `<server-ca.pem>` are their paths on the endpoint:

<!-- deploy_ac_021: endpoint-copy -->

```powershell
Copy-Item '<setup>' "$dir\cyberguard-agent-<version>-x64-setup.exe"
Copy-Item '<server-ca.pem>' "$dir\server-ca.pem"
```

Then check the setup against its line in `SHA256SUMS`, in the directory, where only an administrator can replace it:

<!-- deploy_ac_021: endpoint-checksum -->

```powershell
(Get-FileHash -Algorithm SHA256 "$dir\cyberguard-agent-<version>-x64-setup.exe").Hash -eq '<its SHA-256 in SHA256SUMS>'
```

It prints `True` when the two match. Stop if it prints `False`.

#### The trust anchor's fingerprints

The setup checks the certificates in `server-ca.pem` against a list of fingerprints, and refuses the file unless its distinct certificates are exactly those of the list (code 25). The list is the `sha256:` line recorded on the server from `export-ca.js`'s first command, taken from the installation's change record and not from the copy of `server-ca.pem`, whose last line is part of the file being checked. With an operator-provided pair for ingest ([configuration](configuration.md)), the anchor is the file of the CA that issued it, and the list holds one fingerprint for each distinct certificate in that file: `sha256:` and the SHA-256 of the certificate's DER in lowercase hexadecimal, obtained from its issuer apart from the file.

#### Install with a bootstrap file

The token is valid for 15 minutes from its issue, and the agent must enroll with it within them: issue it on the server now. In the same PowerShell, with `<agent server name>` replaced by a name of `CG_AGENT_SERVER_NAMES` and `<fingerprint>` by the recorded `sha256:` line, this reads the token without echo and writes `bootstrap.toml`, created new with owner Administrators and a protected DACL for SYSTEM and Administrators before the token is written into it:

<!-- deploy_ac_021: endpoint-bootstrap -->

```powershell
$server = '<agent server name>'
$fingerprint = '<fingerprint>'
$token = Read-Host -AsSecureString -Prompt 'Enrollment token'
$bootstrap = @"
[server]
url = "https://${server}:8444"
heartbeat_url = "https://${server}:8443"
trust_anchor_path = '$dir\server-ca.pem'

[enrollment]
token = "$([System.Net.NetworkCredential]::new('', $token).Password)"

[setup]
trust_anchor_sha256 = ["$fingerprint"]
"@
$security = New-Object System.Security.AccessControl.FileSecurity
$security.SetSecurityDescriptorSddlForm('O:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)')
$file = New-Object System.IO.FileStream("$dir\bootstrap.toml", [System.IO.FileMode]::CreateNew, [System.Security.AccessControl.FileSystemRights]::Write, [System.IO.FileShare]::None, 4096, [System.IO.FileOptions]::None, $security)
$bytes = [System.Text.Encoding]::UTF8.GetBytes($bootstrap)
$file.Write($bytes, 0, $bytes.Length)
$file.Close()
Remove-Variable bootstrap, bytes, token
```

- `server.url` is enrollment's URL: `https://`, a name of `CG_AGENT_SERVER_NAMES`, and `CG_ENROLL_PORT` (8444 by default; change it in the URL when `.env` sets another).
- `server.heartbeat_url` is the heartbeats' URL: the same name and `CG_HEARTBEAT_PORT` (8443 by default), `https://`.
- `server.trust_anchor_path` is the anchor's path in the directory. The setup reads it once, and copies the certificates it checked into the agent's own directory.
- `enrollment.token` is the token `issue-token.js` printed, typed or pasted at the `Enrollment token` prompt, which does not echo it. It reaches neither a command line nor PowerShell's history. A pasted token passes through the clipboard of the session, its clipboard history and any clipboard synchronisation it has. Elevate in your own session: on a standard user's desktop, an elevated PowerShell's clipboard and the desktop's other processes are that user's.
- `setup.trust_anchor_sha256` is the list of fingerprints, `["sha256:...", "sha256:..."]` for more than one.
- The file admits these five keys and no other. The setup supplies the agent's placeholder ID, the endpoint's host name and the agent's fixed paths, and every other setting keeps its default ([configuration](configuration.md)).

The setup refuses a bootstrap file (code 23) whose path is not absolute, that does not exist, is a reparse point such as a symbolic link, is not a regular file on a local volume, holds more than 16 MiB, is open for writing in another program, or is not owned by SYSTEM or Administrators or gives access to another account. The block creates the file new, and stops if one exists: to write it again, delete it first.

Run the setup, which this command waits for before it prints the setup's exit code:

<!-- deploy_ac_021: endpoint-setup -->

```powershell
$log = "$dir\setup-$(Get-Date -Format yyyyMMdd-HHmmss).log"
$run = Start-Process -FilePath "$dir\cyberguard-agent-<version>-x64-setup.exe" -ArgumentList '-quiet', '-norestart', '-log', $log, "CG_BOOTSTRAP=$dir\bootstrap.toml" -Wait -PassThru
$run.ExitCode
```

`-quiet` shows no window and asks nothing. `-norestart` keeps the setup from restarting the computer when Windows Installer asks for a restart, which the setup reports instead (3010, 3017). `-log` writes Burn's log, the setup engine's, to `$log`. PowerShell's call operator does not wait for the setup, a windowed program, and leaves `$LASTEXITCODE` unset: `Start-Process -Wait -PassThru` waits and keeps the exit code. 0 is an agent enrolled and reporting; [The setup's result](#the-setups-result) gives every other code.

Once the setup has exited 0, or once its token can no longer be used, delete the bootstrap file, then close this PowerShell, whose memory holds the token until it exits:

<!-- deploy_ac_021: endpoint-cleanup -->

```powershell
Remove-Item "$dir\bootstrap.toml"
```

The setup's elevated process, `cg-setup.exe`, may keep the token in its memory until it exits: it zeroes the copies it reads and the configuration it stages, but other steps leave copies of the token that it does not zero, among them the serialization and the check of that configuration, the parse of a token written with an escape, and a bootstrap file it refuses as invalid (24). SYSTEM and elevated administrators can read that memory, and so can whoever reads a dump of the process ([Record the endpoint's audit and dump settings](#record-the-endpoints-audit-and-dump-settings)).

The setup never deletes the bootstrap file. Deleting it does not erase it: a backup or a shadow copy of the volume taken meanwhile keeps it. A refusal from 20 to 31 does not use the token: the retry may use it while it is valid, in the same file or, after 23, 24 or 25, in a file written again; after 30, the setup runs without `CG_BOOTSTRAP`.

#### Or install with the wizard

The setup run without `-quiet` shows its window, and, on an endpoint where the agent has no configuration, its wizard:

<!-- deploy_ac_021: endpoint-wizard -->

```powershell
$log = "$dir\setup-$(Get-Date -Format yyyyMMdd-HHmmss).log"
$run = Start-Process -FilePath "$dir\cyberguard-agent-<version>-x64-setup.exe" -ArgumentList '-norestart', '-log', $log -Wait -PassThru
$run.ExitCode
```

After the setup's elevation and its first checks, the dialog *CyberGuard Agent enrollment* asks for the bootstrap file's values: the server's URL and the heartbeat URL, as above; the trust anchor's path, typed (`server-ca.pem` in `$dir`, `C:\CyberGuardSetup\server-ca.pem` above); the fingerprints, one per line; and the token, in a masked field. It fills in nothing and computes no fingerprint from the anchor. It shows a refused field or anchor (codes 24 and 25) and asks again; any other refusal closes it, and the setup exits with that code. Issue the token just before you submit it. On an endpoint whose agent was configured and never enrolled, the dialog offers to keep that configuration or to replace it. On an enrolled endpoint no dialog shows, and the setup installs or upgrades the agent with its identity; the installed version's setup repairs it when run with `-repair` ([upgrade](upgrade.md#upgrading-the-agent-on-an-endpoint)).

Cancelling the dialog, or leaving it 15 minutes without an accepted submission (a refused one does not restart them), closes it, and the setup exits 22, the endpoint unchanged by the dialog; so does a submission after the setup's process has ended. Do not close the setup's window while the dialog shows: the setup then exits 1602, but the dialog can stay open, and a submission or a choice to keep then stages the input and stops the service, with nothing to start it again until a later setup, an administrator or the next restart does (SPEC-024 Amendment 2026-11-13, point 8). A token that expires while the dialog waits is refused once the service starts, as any expired token is (code 42).

- Elevate in your own session: on a standard user's desktop, the desktop, its clipboard and its other processes are that user's, whether you elevate the setup that user started or a PowerShell you start the setup from.
- Type the token: a pasted token passes through the clipboard of your session, its clipboard history and any clipboard synchronisation the session has. Whether a process of lower integrity in your session can observe the keys typed into the dialog has not been read.
- The dialog shows during the setup's own run alone, after its elevation, in front of the setup's window. A window of your session can imitate it: a request for the token at any other moment is not the setup's.
- The token may stay in the memory of the setup's elevated process, `cg-setup.exe`, until that process exits: neither the field's memory, which Windows keeps, nor the copies that the serialization and the check of the configuration generated from a submission leave, are zeroed. SYSTEM and elevated administrators can read that memory, and so can whoever reads a dump of the process ([Record the endpoint's audit and dump settings](#record-the-endpoints-audit-and-dump-settings)).
- The dialog shows above other windows and asks for the keyboard; when Windows refuses it that, its taskbar button flashes: click the button, or the dialog, to type into it.

#### The setup's result

The setup's exit code is the result. The second column is the code as Burn's log writes it (below).

| Code | Hexadecimal | What happened | The endpoint after it | Retry |
|---|---|---|---|---|
| 0 | `0x0` | The service runs, and the agent logged its identity and an acknowledged heartbeat within 60 seconds of its start. | Installed and reporting. | None. |
| 3010 | `0xbc2` | As 0, and Windows Installer asked for a restart, which `-norestart` held. | Installed and reporting; the MSI's log gives why Windows Installer asked for the restart. | Restart the computer when convenient. |
| 3017 | `0xbc9` | The run failed, by a step's code, a cancellation or another of Burn's results, and Windows Installer asked for a restart. | As that code gives it: the failed step's `Applied execute package` line in Burn's log, or else its `Apply complete` line. | That code's, after a restart. |
| 1602 | `0x642` | Cancelled: the elevation prompt was declined, or the setup's window was closed while it ran. | Declined: unchanged. Closed during the MSI: rolled back, as for 1603. Closed during the setup's checks: they may go on, and may leave the service stopped and the staged configuration, which holds the token, readable by SYSTEM and Administrators alone. Closed during the last step: the agent this run installed stays installed, and its start goes on, may enroll with the token, and reports to no one. | Check the agent first ([Check that the agent reports](#check-that-the-agent-reports)). Enrolled: run the setup without `CG_BOOTSTRAP`, with `-repair` when this version's setup is installed ([upgrade](upgrade.md#a-repair)). Not enrolled: run it again with its input, with a new token if the old one was used. A setup that passes its checks removes what an earlier attempt staged. |
| 1638 | `0x666` | A newer version of the agent's setup is installed. | Unchanged. | The installed version's setup, or a newer one. |
| 1603, or another Windows Installer code | `0x643` for 1603; another code, its own | The MSI failed, and its changes were rolled back. | Not installed by this run; on an upgrade, the installed setup's own run follows ([upgrade](upgrade.md#the-upgrades-result)). | Read the MSI's log (below), correct its cause, run the setup again. |
| -2146889721 | `0x80091007` | A package inside the setup does not match its digest. A setup damaged otherwise may fail with another code, Burn's log holding an `e310` or `e312` line. | Unchanged: nothing ran. | Download and verify the setup again. |
| 20 | `0x14` | Windows 10 is below build 17763, the setup runs emulated on a machine that is not x64, or a compatibility mode is applied to the setup. A Windows before 10 does not run the setup's code, and the setup fails with another code, which Burn's log gives. | Unchanged. | A supported Windows, with no compatibility mode. |
| 21 | `0x15` | The setup's code ran unelevated. | Unchanged. | An elevated PowerShell. |
| 22 | `0x16` | No input: the bootstrap file's command without `CG_BOOTSTRAP` on an endpoint with no configuration, or the wizard cancelled, left 15 minutes or closed. | Unchanged. | Give a bootstrap file, or complete the wizard. |
| 23 | `0x17` | The bootstrap file cannot be used: its path is not absolute, it does not exist or cannot be read, it is not a regular file on a local volume, it is open for writing elsewhere, it holds more than 16 MiB, or an account other than SYSTEM and Administrators owns it or has access to it. | Unchanged. | Write it again as above. |
| 24 | `0x18` | An input is invalid: the file is not UTF-8 TOML, a key is missing or not admitted, a value has the wrong form, `trust_anchor_path` is not absolute, a URL is not `https://`, or the configuration generated from the input fails the agent's own checks. | Unchanged. | Correct the input; the same token while it is valid. |
| 25 | `0x19` | The anchor cannot be read, or its distinct certificates are not exactly those of the fingerprints. | Unchanged. | Correct the anchor or the fingerprints, from the server's record. |
| 26 | `0x1a` | A service named `CyberGuardAgent` exists whose configuration the setup cannot read, or that is not the agent's: its type is not its own process, its account not LocalSystem, or its command line not the agent's. | Unchanged. | Read its configuration (`sc.exe qc CyberGuardAgent`). Not the agent's: remove it, or have it renamed. The agent's: find why it cannot be read. |
| 27 | `0x1b` | `C:\ProgramData\CyberGuard` or its `agent` directory is a reparse point or not a directory, is owned by an account other than SYSTEM or Administrators, or gives any access to another account; or the setup cannot read the names in it; or the state they make is untrusted, such as `agent.legacy` beside an `agent` with no `identity`. | Unchanged. | Find what changed it, and keep what it holds: this document gives no repair, and neither the directory nor what it holds is removed by hand ([No repair, and no removal by hand](#no-repair-and-no-removal-by-hand)). |
| 28 | `0x1c` | What the product does not support: a layout no release of the setup writes, a console installation of the agent, a directory `agent.migrating`, or `agent.legacy` with no `agent`, which the product neither adopts nor migrates, on a development host or as names another account made on an endpoint; or a migration asked for, a non-empty `CG_MIGRATE_LEGACY` or `CG_TRUST_ANCHORS`, on any state but an untrusted one (27). | Unchanged. | No migration. A development host is formatted, then the product is installed on it; on an endpoint, nothing is removed by hand; a migration variable is left out of the setup's command ([No repair, and no removal by hand](#no-repair-and-no-removal-by-hand)). |
| 29 | `0x1d` | The agent installed is newer than this setup. | Unchanged. | The installed version's setup, or a newer one. |
| 30 | `0x1e` | The agent is enrolled on this endpoint, and a bootstrap file was given. | Unchanged; on an upgrade, the installed setup's own run follows, which may stop and start its agent ([upgrade](upgrade.md#the-upgrades-result)). | The setup without `CG_BOOTSTRAP`, which installs or upgrades the agent with its identity; the installed version's repairs it with `-repair` ([upgrade](upgrade.md#upgrading-the-agent-on-an-endpoint)). |
| 31 | `0x1f` | The service was not stopped 75 seconds after the setup asked it to stop: still running, starting or stopping. | Unchanged, but the service may be left stopped. | Find why it does not stop (`sc.exe query CyberGuardAgent`, and the Application log's entries from `CyberGuardAgent`), then run the setup again. |
| 40 | `0x28` | The setup could not read or write its own state or the host's, in its checks before the MSI or in its last step after it; or, after the MSI, the agent's executable in place is not this version and no restart is pending. | Before the MSI: not installed by this run, and a service that was running may be left stopped. After it: installed, the service not started and possibly stopped, and the staged configuration, which holds the token, left for the next setup. | Run the setup again, with the same input while its token is valid, or a new token. |
| 41 | `0x29` | The service control manager refused to start the service, for instance a disabled service. | Installed, not running; the staged configuration left. | Correct the cause (`sc.exe qc CyberGuardAgent`), then as for 40. |
| 42 | `0x2a` | The service started and then stopped with a code within the 60 seconds. | Installed and stopped; it is not restarted. Its own code is in `sc.exe query CyberGuardAgent` and Application-log entry 1002 ([The agent's exit codes](#the-agents-exit-codes)). | By the service's code: for 3, a token refused, used or expired, a new token in a new bootstrap file, which replaces the configuration of an agent that never enrolled; for 6, correct the URL's host name or the anchor, then run the setup again with a new bootstrap file, and a new token once the old one has expired; for 5 naming `agent.migrating` or `agent.legacy`, as for 28 ([No repair, and no removal by hand](#no-repair-and-no-removal-by-hand)). A server the service cannot reach is 44, not 42. |
| 43 | `0x2b` | At the end of the 60 seconds the service was neither running nor stopped: still starting, stopping, or paused. | Installed, the service in that state (`sc.exe query CyberGuardAgent`). | Wait, then check the service and the agent as below. |
| 44 | `0x2c` | The service runs, but the agent's identity line and then its first acknowledged heartbeat were not both seen in its log within the 60 seconds; or the log could not be read; or, after the start, the service's state could not be read or the service no longer existed. | Installed, and usually running: still enrolling, retrying a server it cannot reach a minute or more apart, or failing. | Read the agent's log (below); the agent goes on retrying by itself. |
| 45 | `0x2d` | The agent's executable in place is not this version, and Windows replaces it at the next restart. When Windows Installer asked for that restart in this run, the setup exits 3017 instead. | Installed; the service not started by this setup, and the staged configuration, which holds the token, left. | Restart the computer, then run the setup again, with the same input while its token is valid, or a new token. |
| 46 | `0x2e` | An uninstallation with `CG_PURGE=yes`: the purge removed nothing. | Installed, its data kept. | As [The uninstallation's result](#the-uninstallations-result) gives it. |
| 47 | `0x2f` | An uninstallation with `CG_PURGE=yes`: the purge may have removed some of the agent's data, or its outcome could not be read. | Installed, part of its data possibly removed, the service as the purge left it. | As [The uninstallation's result](#the-uninstallations-result) gives it. |

After a refusal of the setup's checks, or a failure of the MSI, Burn runs the setup's reversal. When an earlier attempt left its record behind, after its 40, 41 or 45, a failed deletion of its record, or a crash, the reversal removes what that record names, and starts the service if the record says it was running. On an upgrade, a refusal from 20 to 31, and the failures after it, are followed by the installed setup's own run, which may stop and start the agent, set its failure actions and start a service an administrator had left stopped ([upgrade](upgrade.md#the-upgrades-result), which gives what has been read of it).

Running the setup again takes no token on an enrolled endpoint, and neither enrolls again nor overwrites an identity already there. It stops a running service before it changes anything, and the cases above (1602, 31, 40, 45, an upgrade's restore) may leave the service stopped until a later setup, an administrator or the next restart starts it. To enroll an enrolled endpoint again, as when its certificate has expired (code 7) or its server is a new installation with another CA, purge it ([Remove the agent from an endpoint](#remove-the-agent-from-an-endpoint)) and run the setup again with a new token, and the server's current anchor and fingerprints, in a new bootstrap file or the wizard: the endpoint then enrolls a new agent. The purge revokes nothing: the old agent stays on the server that enrolled it, which has no command that removes it yet, and its certificate stays valid there until it expires or an operator deletes its row, which no runbook gives, for a copy of its key that a backup or a shadow copy kept (R5, and the copies of the key outside the product's custody that the [threat model](../security/threat-model.md) accepts); a server restored from a backup taken before the agent enrolled holds no row of it. Between the purge and the new setup an ordinary user can create `C:\ProgramData\CyberGuard` first, and the setup then refuses it (27 or 28). The purge followed by an enrollment with a new token has not been run, in CI or on an endpoint. An agent installed by hand, before the setup, whose identity's files are directly in `C:\ProgramData\CyberGuard\agent`, is a console installation, which the setup refuses (28) and the product neither adopts nor migrates: its development host is formatted, then the product is installed on it with the setup and a new token ([upgrade.md](upgrade.md#agents)). A code that is not in the table is Burn's own or Windows Installer's: Burn's log gives the step it came from.

#### No repair, and no removal by hand

The setup's 27 and 28, the service's 13, its 2 and 5 for the migration's argument and directories, its purge start's 14, and the purge's 27, 28, 46 and 47 leave the endpoint as they found it or as the purge left it, and each line of theirs that names a console installation or the migration sends here. This document gives no repair of `C:\ProgramData\CyberGuard`. Do not remove that directory, or anything it holds, by hand: a recursive removal can follow a junction planted in it into the junction's target, and the agent's `identity` is SYSTEM's alone, which an administrator reaches only by taking ownership of it, which the service then refuses (code 5). Find what changed it, and keep the endpoint as it is.

A console installation, and the directories `agent.migrating` and `agent.legacy` of its migration, are a development layout: no release of the setup writes them, and the product neither adopts nor migrates them. A development host that holds one, its migration interrupted or not, is formatted, not migrated, before the product is installed on it with the setup and a new token. On an endpoint the setup installed, those names are another account's or an administrator's. A setup run with a non-empty `CG_MIGRATE_LEGACY` or `CG_TRUST_ANCHORS` ends 28, or 27 on an untrusted state, before it reads any other input or changes anything: run it without them.

#### Burn's log

`$log` is Burn's log. Windows Installer's log of the MSI is beside it, named after it, ending in `_Agent.log`. The setup runs four steps, which the log names: `Preflight`, its checks and the staging of the configuration; `Agent`, the MSI; `Purge`, which on an installation runs with no confirmation and changes nothing, and runs first on an uninstallation ([Remove the agent from an endpoint](#remove-the-agent-from-an-endpoint)); and `Configure`, the start of the service and the wait for its outcome. These lines carry the result:

<!-- deploy_ac_021: endpoint-log -->

```powershell
Select-String -Path $log -Pattern 'Applied execute package', 'Apply complete', 'Exit code'
```

- `Applied execute package: <step>, result: 0x...`, with a result other than `0x0`, names the step that failed; in a cancelled run the result, `0x80070642`, is the cancellation's, not the step's. On an upgrade, a line whose package is a code in braces is the setup installed before, which the upgrade removes.
- `Apply complete, result: 0x...` carries the run's result, and is the line of a failure no step reports, such as 1638, a declined elevation or a damaged package.
- The last line, `Exit code: 0x...`, is the code the setup exits with, in hexadecimal.

A result that begins with `0x8007` ends with the code in its last four hexadecimal digits: `0x8007001e` is 30, `0x80070643` is 1603 and `0x80070666` is 1638. Any other result is itself the code, such as `0x80091007`. When Windows Installer asked for a restart, the setup exits 3010 or 3017 whatever the step's result, which its `Applied execute package` line keeps. A failure before Burn's `Apply begin` line writes neither of the first two lines: the `Exit code` line alone carries it. A setup run with its window shows, on a failure, a page that names the log and the first two lines; the `Exit code` line is written after the window closes. During the MSI's step, or for an error Burn reports outside a step's code, that window can also show a message box with Windows Installer's message or the system's text of the error, which the page does not. The setup's own steps write their reason to no log: their code and the table above are their result. Burn's log holds the setup's variables and command lines, among them the bootstrap file's path, and never the token. Keep the log with the installation's record.

### Check that the agent reports

The dashboard has no agents page yet; a later release adds it. Check the installed agent's executable against its line in `SHA256SUMS`, the line of `cg-agent-<version>-x86_64-pc-windows-msvc.exe`:

<!-- deploy_ac_021: endpoint-binary -->

```powershell
(Get-FileHash -Algorithm SHA256 "$env:ProgramFiles\CyberGuard\Agent\cg-agent.exe").Hash -eq '<the agent executable's SHA-256 in SHA256SUMS>'
```

It prints `True`. Then the service:

<!-- deploy_ac_021: endpoint-service -->

```powershell
sc.exe query CyberGuardAgent
sc.exe qc CyberGuardAgent
sc.exe qfailure CyberGuardAgent
sc.exe qfailureflag CyberGuardAgent
sc.exe sdshow CyberGuardAgent
```

`sc.exe` writes its labels in Windows' language; the values are these. `query`: state `4 RUNNING`. `qc`: type `10 WIN32_OWN_PROCESS`, start type `2 AUTO_START`, the command line `"C:\Program Files\CyberGuard\Agent\cg-agent.exe" --service --config "C:\ProgramData\CyberGuard\agent\agent.toml"`, the display name `CyberGuard Agent` and the account `LocalSystem`. `qfailure`: a reset period of 86400 seconds and restarts after 60000, 120000 and 300000 milliseconds. `qfailureflag`: false, so the service control manager restarts an agent whose process dies, and never one that stopped with a code. `sdshow`: the `D:` part `D:(A;;CCLCSWRPWPDTLOCRRC;;;SY)(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA)(A;;CCLCSWLORC;;;IU)(A;;CCLCSWLORC;;;SU)`, which lets SYSTEM and Administrators alone change, start, stop or delete the service, and interactive and service logons query it.

The service writes to the Application log, under the source `CyberGuardAgent`:

<!-- deploy_ac_021: endpoint-events -->

```powershell
Get-WinEvent -FilterHashtable @{ LogName = 'Application'; ProviderName = 'CyberGuardAgent' } -MaxEvents 10 | Format-List TimeCreated, Id, Message
```

Entry 1000 is the service running, with its version: `cg-agent <version>: the service is running`. 1001 is a clean stop, 1002 a stop with a code and its message, 1003 a stop whose 60-second deadline expired. Any interactive user can write entries under any source, so these entries are diagnostics, not evidence. When no entry matches, `Get-WinEvent` says so with an error, as it does when the source is not registered, before the setup's MSI has run. An uninstallation removes the source: Windows keeps the entries and their strings, which hold each entry's text, without the source's message table, and this command, which names the source, then answers with an error.

The agent's log, `C:\ProgramData\CyberGuard\agent\logs\agent.jsonl`, is one JSON object per line, whose `fields` hold the event's `message` and values. Elevated Administrators can read it:

<!-- deploy_ac_021: endpoint-agent-log -->

```powershell
$agentLog = "$env:ProgramData\CyberGuard\agent\logs\agent.jsonl"
Select-String -Path $agentLog -Pattern '"enrollment response received"', '"agent identity ready"' | ForEach-Object Line
Select-String -Path $agentLog -Pattern '"signed heartbeat sent"' | Select-Object -Last 1 | ForEach-Object Line
```

- `enrollment response received`, once, after the enrollment: `fields.agent_id`, the id the server assigned, and `fields.expires_at`, the end of the agent's certificate. Record that end: the certificate lasts 90 days, this release does not renew it, and past it the server refuses the agent, whose service stops with code 7; the endpoint then enrolls again as [The setup's result](#the-setups-result) says, after its table. The log rotates (below), and drops this line in time.
- `agent identity ready`, at each start of the service: the same id, under the key `agent.id` of `fields`.
- `signed heartbeat sent`, at least every 30 seconds and with each batch of events delivered in between, with `fields.response_status` 200. A refused heartbeat is logged as `signed envelope rejected by server`, with the server's status in `fields.response_status`; the agent keeps running, and after the retries it drops that batch's events (`batch dropped after repeated rejections`). A clock more than 5 minutes off the server's gives status 422.

Read the log with a command that opens it and closes it, as these do. An editor or a tail that keeps `agent.jsonl` open stops its rotation, and the service stops with code 13 if the rotation still fails after 60 seconds.

On the server, ingest logs a `heartbeat accepted` line, with the agent's `agent_id`, for each heartbeat it accepts, at `info`, `CG_LOG_LEVEL`'s default:

<!-- deploy_ac_021: heartbeat-logs -->

```sh
docker compose logs --no-color --tail 60
```

The `agent_id` is the one the agent's log gives.

### Record the endpoint's audit and dump settings

The agent neither reads nor manages the endpoint's audit policy, and writes, collects and sends no crash or memory dump (SPEC-024 Amendment 2026-11-01, point 5). These commands read the settings that decide both, change nothing, and need the elevated PowerShell. Record their output with the installation's record, beside Windows' edition and build, the account that ran the setup, the setup's version and SHA-256, the installed executable's SHA-256, the setup's exit code with its Burn log, and the files removed afterwards.

The audit policy of the Logon and Account Lockout subcategories, by their identifiers, since their names are in Windows' language, and the accounts with a per-user audit policy, which can keep their logons out of auditing:

<!-- deploy_ac_021: endpoint-audit -->

```powershell
auditpol /get /subcategory:"{0CCE9215-69AE-11D9-BED3-505054503030},{0CCE9217-69AE-11D9-BED3-505054503030}" /r
"auditpol exit code: $LASTEXITCODE"
$perUser = @(auditpol /list /user /v /r | Select-String 'S-1-')
"auditpol exit code: $LASTEXITCODE; accounts with a per-user audit policy: $($perUser.Count)"
```

The policy read is the effective one, Group Policy's included. Record success and failure apart for each subcategory: Logon's success is event 4624, and Logon's and Account Lockout's failures are 4625. One not enabled is coverage the agent does not have: the agent then sends no such logon events, and reports no error (SPEC-020 §Risks). One that `auditpol` could not read, with an exit code other than 0, is unknown. A working subscription to the Security log does not show that logons are audited. Record the number of accounts with a per-user audit policy, not their names: the command counts the security identifiers `auditpol` lists, and prints none of them. With no such account, `auditpol` lists none and the count is 0; with an exit code other than 0 the count is unknown, as unelevated `auditpol` gives (1314).

The settings of the dumps and of the files that can hold the agent's memory:

<!-- deploy_ac_021: endpoint-dumps -->

```powershell
reg query 'HKLM\SYSTEM\CurrentControlSet\Control\CrashControl' /reg:64
reg query 'HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management' /v PagingFiles /reg:64
reg query 'HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management' /v ClearPageFileAtShutdown /reg:64
reg query 'HKLM\SYSTEM\CurrentControlSet\Control\Power' /v HibernateEnabled /reg:64
powercfg /a
reg query 'HKLM\SOFTWARE\Microsoft\Windows\Windows Error Reporting' /reg:64
reg query 'HKLM\SOFTWARE\Microsoft\Windows\Windows Error Reporting\Consent' /reg:64
reg query 'HKLM\SOFTWARE\Microsoft\Windows\Windows Error Reporting\LocalDumps' /s /reg:64
reg query 'HKLM\SOFTWARE\Policies\Microsoft\Windows\Windows Error Reporting' /s /reg:64
reg query 'HKU\S-1-5-18\Software\Microsoft\Windows\Windows Error Reporting' /s
reg query 'HKCU\Software\Microsoft\Windows\Windows Error Reporting' /s
```

`CrashControl` holds the kernel dump's settings; `PagingFiles`, `ClearPageFileAtShutdown`, `HibernateEnabled` and `powercfg /a`, the page file's and hibernation's. Windows Error Reporting's `LocalDumps`, global and per program, among them `cg-agent.exe` and `cg-setup.exe`, keeps dumps of a failing program on the endpoint; its consent and policy keys, the Group Policy ones, SYSTEM's and those of the administrator who runs the setup included, decide whether its reports and their dumps leave the endpoint. `reg query` says that it cannot find a key or value that is not set, so that Windows' default applies, and that access is denied to one it could not read, which is unknown: record its words.

A dump of the agent's process can hold its decrypted private key and the events it holds; one of the setup's process, while its wizard runs or before it exits, the token. A kernel dump, the page file and the hibernation file can hold any memory of the endpoint. Who reads such files, and how long they are kept, follow the endpoint's own policy, which these settings record: none of them is a protection that CyberGuard verifies.

### The agent's service

The service starts with Windows, and needs no one logged on. From an elevated PowerShell, `Stop-Service CyberGuardAgent` stops it and `Start-Service CyberGuardAgent` starts it again. A stop ends the agent's capture, tries once to send a last heartbeat and ends within 60 seconds, or stops with code 12 and entry 1003; the events the agent has not delivered by then are lost, and none is captured while it is stopped. The agent keeps no events on disk: while the server cannot be reached, it holds them in memory alone, and drops the oldest when that buffer is full. The service control manager restarts an agent whose process dies, after 60, then 120, then 300 seconds, and counts the failures again after a day without one; it does not restart an agent that stopped with a code, which waits for an administrator.

The setup installs the agent's executable as `C:\Program Files\CyberGuard\Agent\cg-agent.exe` and keeps its data in `C:\ProgramData\CyberGuard\agent`:

- `agent.toml`, the configuration, and `server-ca.pem`, the trust anchor, owned by SYSTEM, with protected DACLs for SYSTEM and Administrators;
- `identity\`, the agent's certificate, its private key, sealed with DPAPI to the machine, and its identity, which SYSTEM alone can open: an administrator who takes ownership of it, or changes its DACL, makes the service refuse it at its next start (code 5);
- `logs\`, the agent's log, which SYSTEM writes and elevated Administrators read.

After it enrolls, the agent removes the token from `agent.toml`, as a best-effort rewrite: if that fails, its log has a warning, and the token, already used, stays in the protected file. A token that the service never took, because a setup failed or was cancelled after it staged the configuration, stays in `agent.toml.setup`, readable by SYSTEM and Administrators alone, until a later setup that passes its checks removes it, or the purge does; a default uninstallation keeps it ([Remove the agent from an endpoint](#remove-the-agent-from-an-endpoint)), and a backup or a shadow copy of the volume may keep it.

### The agent's exit codes

Before it exits with a code other than 0, the agent gives a message that begins as the table shows: the service in Application-log entry 1002, as the paragraph after the table says, and the agent run by hand, as development does, on standard error.

| Code | Cause | The message begins |
|---|---|---|
| 0 | A clean stop. | |
| 1 | Another runtime error, for instance an ETW session or the Security log's subscription that fails to open for a reason other than privilege; a token that can control trace sessions but is not an elevated administrator's or SYSTEM's (it is refused before enrollment); or any other failure to take the per-host instance lock. | `cg-agent:` or `cg-agent: instance lock failed:` |
| 2 | The configuration is refused, for instance: the file is missing or is not valid TOML; a required key is missing; `agent.id` is not a lowercase UUIDv7; `agent.hostname` or `trust_anchor_path` is empty; `server.url` is not `https://` beside `[enrollment]`, or `server.heartbeat_url` is not `https://`; `interval_seconds`, `request_timeout_seconds`, `backoff_initial_ms` or `backoff_max_ms` of `[heartbeat]` is 0, or its `backoff_factor` is below 1; `tls.minimum_version` or `envelope.canonical_form` has another value; there is no token at the first start. For the service, also the start argument `--migrate-legacy=` (below). | `cg-agent: invalid config:` |
| 3 | Enrollment is refused: the token is rejected, already used or expired, or the server's answer is malformed, for the service also a certificate document TLS would refuse. | `cg-agent: enrollment failed:` |
| 4 | The server is unreachable after the retries, or it offers TLS 1.2 alone. Run by hand only: the service waits 60 seconds after an exhausted round, and tries again. | `cg-agent: enrollment failed: server unreachable after` |
| 5 | The identity cannot be written, restricted or loaded, for instance a `key.dat` copied from another machine or, for the service, a `cert.pem` TLS would refuse. For the service, also a start beside a directory `agent.migrating` or `agent.legacy` that counts (below). | `cg-agent: identity error:` |
| 6 | The server's certificate is not one the trust anchor issued, has expired, or does not name the URL's host (a name outside `CG_AGENT_SERVER_NAMES`); or the trust anchor file is missing or holds no usable certificate. Enrollment ends at once, without retries. | `cg-agent: tls: server certificate verification failed:` or `cg-agent: tls: client configuration failed:` |
| 7 | The server refuses the agent's certificate, for instance once it has expired. | `cg-agent: tls: server rejected client certificate` |
| 8 | Signing a heartbeat fails. | `cg-agent: signing failed:` |
| 9 | The agent is not elevated, or, running without the instance lock, its ETW start was refused because a session of the agent's name already exists (which it may not reclaim). | `cg-agent: insufficient privilege to open Microsoft-Windows-Kernel-Process ETW session; run as elevated user or LocalSystem`, or `cg-agent: insufficient privilege to read the Security log; run as elevated user` |
| 10 | Another CyberGuard agent is already running on this host; this instance exits before enrolling and stops no ETW session. | `cg-agent: another CyberGuard agent instance is running on this host` |
| 11 | `--service` was given, but the Windows service control manager did not start the agent. It runs nothing else and reads no configuration. | `cg-agent: --service requires a start by the Windows service control manager` |
| 12 | The service only: its stop did not end within its 60-second deadline. Application-log entry 1003 names the work still pending and counts the events not delivered. | The service writes no standard error; see below. |
| 13 | The service only: its log file, `%ProgramData%\CyberGuard\agent\logs\agent.jsonl`, cannot be opened at startup, for instance because a directory on its path is not owned by SYSTEM or Administrators or lets another user in, which the service checks for `%ProgramData%\CyberGuard` and `CyberGuard\agent` before it reads its configuration (this document gives no repair, and neither directory nor what it holds is removed by hand, as for the setup's 27: [No repair, and no removal by hand](#no-repair-and-no-removal-by-hand)); or, while it runs, a write to it fails again after the agent reopens the file, and the service stops. | `cg-agent: the service log cannot be written:` |
| 14 | The service only, a purge start: the host's state is untrusted, or it holds a console installation, a directory `agent.migrating` or `agent.legacy` with no `agent`, which the product neither adopts nor migrates ([No repair, and no removal by hand](#no-repair-and-no-removal-by-hand)). Nothing was removed. | `cg-agent: the purge refuses this host:` |
| 15 | The service only, a purge start: it left a name it does not remove, one it removes found in another form (another owner, a second link, a reparse point), or one Windows refused to delete; its 120 seconds ran out; or it ended abnormally. The message names what it left, at most 20 names and then their number; an abnormal end's names nothing. | `cg-agent: the purge left` or `cg-agent: the purge ended abnormally` |

The service has no migration of a console installation (SPEC-024 Amendment 2026-11-20). Started with `--migrate-legacy=`, it stops with code 2 before any change: `cg-agent: invalid migration arguments: --migrate-legacy is given, and the migration of a console installation is not supported; see the installation runbook, install.md`. Started without arguments beside a directory `agent.migrating` or `agent.legacy` that counts, one SYSTEM owns under `agent.toml`'s policy, it stops with code 5 before any change, its line beginning `cg-agent: identity error:`, naming the directories it found and, beside a console installation's identity files, their data directory, and ending `no release of the setup writes a console installation's migration directory, and the product neither adopts nor migrates one; nothing was changed; see the installation runbook, install.md`. Each refusal it makes on a console installation adds `the data directory holds a console installation's identity files, which the product neither adopts nor migrates; see the installation runbook, install.md`. None of them is repaired by hand ([No repair, and no removal by hand](#no-repair-and-no-removal-by-hand)).

A purge start, the service started with the single argument `--purge` by the purge ([The purge](#the-purge), SPEC-024 Amendment 2026-11-15), reads no configuration and never reports `RUNNING`. It stops with 0 once it has removed every name it removes, with entry 1001 `cg-agent: the purge removed the product's data and left nothing`; with 2 when `--purge` is given with another argument (`cg-agent: invalid purge arguments:`) or `--config` is not the agent's, as above; with 10 or 1 for the instance lock, and 1 for another runtime error, as above; with 13 when `%ProgramData%\CyberGuard` or `CyberGuard\agent` fails its check (`cg-agent: the purge's directories fail their check:`); and with 14 or 15. It gives 1, 2, 10, 13 and 14 before it removes anything. The service control manager restarts none of these. A purge process that dies is restarted without arguments, as any agent whose process dies: before the purge has removed `agent.toml`, that start runs the agent as before the purge, and after it, it stops with code 2.

Run as the Windows service (`--service`), the agent writes nothing to standard error. The service control manager's status carries the code: `STOPPED` with Win32 exit code 0 after a clean stop, otherwise 1066 (`ERROR_SERVICE_SPECIFIC_ERROR`) with the code above as the service-specific exit code (`sc.exe query <service>` shows both). The Application log carries the message, under the source `CyberGuardAgent`: entry 1000 when the service runs, with its version; 1001 after a clean stop; 1002 after a failure, with its code and message; 1003 when the stop deadline expired. A configuration error's entry names the file, and the line and column of a TOML error, but quotes no line of it. In service mode a `--config` that is not an absolute path is code 2 (`cg-agent: invalid config: --config must be an absolute path in service mode`). The service runs the secure path alone, at fixed paths under `%ProgramData%` (SPEC-024 Amendment 2026-11-04): `--config` must be `%ProgramData%\CyberGuard\agent\agent.toml`; in it, `server.trust_anchor_path` must be `%ProgramData%\CyberGuard\agent\server-ca.pem` and `cert_path`, `key_path` and `identity_path` of `[enrollment]` must be `cert.pem`, `key.dat` and `identity.json` in `%ProgramData%\CyberGuard\agent\identity\`, each spelled exactly so but for the case of ASCII letters (`<key> must be <path> in service mode`), and `server.heartbeat_url` must be set (`the service requires server.heartbeat_url`); a configuration without `[enrollment]` and `server.trust_anchor_path` is code 2 too. Before it reads `agent.toml`, the service checks `%ProgramData%\CyberGuard` and `CyberGuard\agent` as its log's directories (code 13 above), then `agent.toml` itself: owned by SYSTEM, with a protected DACL that gives access to SYSTEM and Administrators alone, a regular file with one link, not a reparse point; otherwise code 2 (`cg-agent: invalid config <file>: <reason>`). An editor that replaces `agent.toml` with a new file, rather than rewriting it, gives it another owner and DACL, and the next start refuses it; one that holds it open for writing while the service starts makes that start fail with code 2 too. After it reports `RUNNING`, it checks `server-ca.pem` the same way (code 6, `cg-agent: tls: client configuration failed: <reason>`), then `identity\` and its files, which only SYSTEM may access (code 5, `cg-agent: identity error: <reason>`). An `identity\` directory without all three of its files, an empty one included, is code 5 (`cg-agent: identity error: incomplete identity`), with the repair: restore the identity, or enroll the endpoint again as [The setup's result](#the-setups-result) says after its table; so is one whose certificate is not for its key or names another agent (`inconsistent identity`): the service neither enrolls again nor overwrites what is there. A certificate in the server's enrollment answer for another key or agent is code 3 (`malformed response`), and nothing is written. The service writes the identity with SYSTEM alone allowed from the moment each file is created, and its removal of the token from `agent.toml` keeps the file's owner and DACL. If the stop deadline expires before the agent can count what it lost, entry 1003 says the count is unknown. The service's log is JSON lines, one record per line: `agent.jsonl` and at most five archives, `agent.1.jsonl` (the newest) to `agent.5.jsonl`, each at most 10 MiB, so at most 60 MiB in all; the oldest is deleted when the active file rotates. A record of 64 KiB or more, its newline included, is replaced by a line that says so. If another process holds the active file open without sharing its deletion (an editor, a tail), the file cannot rotate: the service writes on in it past 10 MiB and tries again each second, and stops with code 13 only if rotation still fails after 60 seconds. Only SYSTEM writes these files and only elevated Administrators read them.

## Remove the agent from an endpoint

The agent's setup removes what it installed in one of two ways, each an uninstallation of the setup run from an elevated Windows PowerShell (SPEC-024 §Operational §4, Amendments 2026-11-15 and 2026-11-16):

- the **default uninstallation** removes the service and the agent's executable, and keeps the agent's data, its identity included, for a later installation;
- the **purge**, an uninstallation with `CG_PURGE=yes`, first removes the agent's data on the endpoint, then the service and the executable.

Neither removes anything from the server: the agent stays there with the events it sent, and its certificate is not revoked. Like the installation's, these endpoint steps have not yet been run on an endpoint (SPEC-024 install_ac_016).

### The setup to uninstall with

Uninstall with the setup of the version installed, whose version and SHA-256 the installation's change record holds ([Record the endpoint's audit and dump settings](#record-the-endpoints-audit-and-dump-settings)), from the inputs' directory, where only an administrator can replace it. Open Windows PowerShell with *Run as administrator* and confirm with `net session` that it is elevated, as [On the endpoint: install and run the agent](#on-the-endpoint-install-and-run-the-agent) does; a setup started without elevation copies its engine where the starting user's other processes can reach it. Then, with `$dir` that directory:

<!-- deploy_ac_021: endpoint-removal-setup -->

```powershell
$dir = 'C:\CyberGuardSetup'
(Get-FileHash -Algorithm SHA256 "$dir\cyberguard-agent-<version>-x64-setup.exe").Hash -eq '<its SHA-256 in SHA256SUMS>'
```

It prints `True`. Stop if it prints `False`. When the directory or the setup is gone, create the directory, copy that version's setup into it and check it, as [A directory for the setup and its inputs](#a-directory-for-the-setup-and-its-inputs) does; the uninstallation needs neither the trust anchor nor a bootstrap file. The setup of another version is not the one installed: do not uninstall with it. The entry *CyberGuard Agent* of Windows' installed programs runs the default uninstallation, and never purges; started from it, the setup may run unelevated until its elevation prompt, as from Explorer, with the residual the installation's command avoids (not read), so this document uses the command.

### The default uninstallation

<!-- deploy_ac_021: endpoint-uninstall -->

```powershell
$log = "$dir\uninstall-$(Get-Date -Format yyyyMMdd-HHmmss).log"
$run = Start-Process -FilePath "$dir\cyberguard-agent-<version>-x64-setup.exe" -ArgumentList '-uninstall', '-quiet', '-norestart', '-log', $log -Wait -PassThru
$run.ExitCode
```

The setup runs its `Purge` step first, which without `CG_PURGE=yes` checks the platform and the elevation (20, 21), then reads nothing and exits 0. Then Windows Installer stops the service, which ends its capture, tries once to send a last heartbeat and ends within 60 seconds ([The agent's service](#the-agents-service)), and removes the service, the agent's executable with the folders it installed it in, and the Application log's source; then the setup removes its own registration among the installed programs and its copies in Windows' package cache. 0 is the agent uninstalled; [The uninstallation's result](#the-uninstallations-result) gives the other codes.

It keeps `C:\ProgramData\CyberGuard` and what the agent wrote in it, with the owner and DACL each object had:

- in `agent`, `agent.toml` and `server-ca.pem`: on an endpoint whose agent never enrolled, or whose removal of the token after its enrollment failed, `agent.toml` holds the token;
- `agent\identity`, the agent's certificate, its private key and its identity;
- `agent\logs`, the agent's log;
- in `agent`, `agent.toml.setup` with the token it holds, `server-ca.pem.setup` and `setup-attempt.json`, when a setup staged a configuration the service never took, and the temporary file a crash may leave, `agent.toml.hygiene-tmp`.

The inputs' directory, with what it holds (delete `bootstrap.toml` if it is still there), Burn's logs and the Application log's entries stay too. A later setup of the same version or a later one, run as [Install with a bootstrap file](#install-with-a-bootstrap-file) runs it but without `CG_BOOTSTRAP`, finds that data: on an enrolled endpoint the agent loads the same identity and reports again as the same agent, with no token (SPEC-024 install_ac_012); on one with an `agent.toml` and no identity, whose agent never enrolled, it takes that configuration, whose token may have expired meanwhile (42). An endpoint with only `agent.toml.setup` has no configuration: a later setup needs a bootstrap file or the wizard (22 without).

### The purge

<!-- deploy_ac_021: endpoint-purge -->

```powershell
$log = "$dir\purge-$(Get-Date -Format yyyyMMdd-HHmmss).log"
$run = Start-Process -FilePath "$dir\cyberguard-agent-<version>-x64-setup.exe" -ArgumentList '-uninstall', 'CG_PURGE=yes', '-quiet', '-norestart', '-log', $log -Wait -PassThru
$run.ExitCode
```

`CG_PURGE=yes`, in lowercase, is the request, and only the command that carries it purges: the setup does not keep it. An empty `CG_PURGE` purges nothing, and any other value is refused (40) before anything changes. The installed programs' entry, a repair, an installation and an upgrade never purge, and the setup's window offers no purge: run without `-quiet`, the setup purges only with `CG_PURGE=yes` on its command line. Nor, by the setup's contract, does an uninstallation with `-unsafeuninstall`, which has not been run and which this document does not use. Keep `$log` out of `C:\ProgramData\CyberGuard`, and close any window or shell whose current directory is in it: a file there, or a directory held open, keeps the directory, and the purge ends with 47.

The purge runs only while the setup is installed: on an endpoint where it is not, the command has no `Purge` step to run, and what it exits with is not tested ([An endpoint uninstalled without the purge](#an-endpoint-uninstalled-without-the-purge)). After the purge's command, whatever it exits with, check the endpoint:

<!-- deploy_ac_021: endpoint-purge-check -->

```powershell
Test-Path -LiteralPath "$env:ProgramData\CyberGuard"
sc.exe query CyberGuardAgent
```

After a purge that exited 0 or 3010, `Test-Path` prints `False`, and `sc.exe` answers that the service does not exist (1060, in Windows' language); a program that holds the service open, such as the Services console, can delay its deletion until it closes. Anything else is not a completed purge: read the code below.

The setup's `Purge` step, before Windows Installer's uninstallation:

1. exits 0 at once when `C:\ProgramData\CyberGuard` does not exist;
2. reads the endpoint's state as the installation's checks do, and refuses an untrusted directory (27), and a console installation, a directory `agent.migrating` or `agent.legacy` with no `agent`, which the product neither adopts nor migrates (28), before it touches the service;
3. refuses a service `CyberGuardAgent` that is not the agent's (26); with no such service it removes nothing (46), since only the agent's service, which runs as SYSTEM, can reach the identity, which SYSTEM alone can open;
4. stops the service, proved stopped within 75 seconds (31 otherwise);
5. starts the service with the single argument `--purge`, and waits up to 300 seconds for it to stop.

The service's purge start takes the instance lock, checks `C:\ProgramData\CyberGuard` and its `agent` directory as every start of the service does, without following a reparse point, and refuses an untrusted state, a console installation, a directory `agent.migrating` or `agent.legacy` with no `agent` (14). Then, as SYSTEM, it removes the names below, and no others ([The agent's exit codes](#the-agents-exit-codes) gives its codes). Once it has stopped with 0, Windows Installer removes the service and the executable, as in the default uninstallation, and the setup its registration and its cache.

#### What the purge removes

It works only in `C:\ProgramData\CyberGuard` and, in it, `agent`, `agent\identity`, `agent\logs` and `agent.legacy`, each when it is a directory, not a reparse point, owned by SYSTEM or Administrators and open to them alone, `agent.legacy` owned by SYSTEM with a protected DACL. It removes these names, compared without regard to the case of ASCII letters, and no others:

- in `agent`: `agent.toml.setup`, `agent.toml`, `agent.toml.hygiene-tmp`, `server-ca.pem`, `server-ca.pem.setup`, `setup-attempt.json`, `migration.json` and `migration.json.rewrite-tmp`;
- in `agent\identity`: `cert.pem`, `key.dat` and `identity.json`;
- in `agent\logs`: `agent.jsonl`, `agent.1.jsonl` to `agent.5.jsonl` and `agent.rotating.jsonl`;
- in `agent.legacy`, a console installation's legacy directory, which no release of the setup writes and which on an endpoint only an administrator, SYSTEM or an object moved in makes: `agent.toml`, `server-ca.pem`, `cert.pem`, `key.dat`, `identity.json` and `agent.toml.hygiene-tmp`;
- then the five directories, each once it is empty.

It opens each file without following a reparse point and deletes it only as a regular file with one link, owned by SYSTEM or Administrators. It reads no path from the configuration. It removes the configuration first, `agent.toml.setup` before `agent.toml`, then `agent.legacy`, then the identity, each step only once the one before it left nothing, so that it never leaves a configuration without its identity; then the rest, going on past what it leaves. It leaves, never followed, any other name, any other directory, a reparse point such as a junction or a symbolic link, a listed name that is not what the list allows (a file with another link or another owner, a directory that fails the check above), and a file Windows refuses to delete, and stops with code 15, whose message names them. It begins no removal 120 seconds after it took the lock. Run again, the same command goes on from what is left, and finds nothing to remove once nothing is left.

#### What the purge is not

- It removes the endpoint's identity, configuration, token and logs. It does not revoke the agent's certificate, remove the agent or its events from the server, or erase securely: a backup or a shadow copy of the volume, the page file, a dump, an antivirus quarantine and a file that a hard link made elsewhere, the copies of the key outside the product's custody that the [threat model](../security/threat-model.md) accepts, and an image or a clone of the disk (R18) keep what they hold.
- A purged endpoint is a new one: a later setup needs a bootstrap file with a new token, or the wizard (22 without input), and enrolls a new agent. The old agent stays on the server, which has no command that removes it yet (R5).
- By the levers the threat model names, entries made before the directories' protection or while `C:\ProgramData\CyberGuard` is absent, file transactions and handles held open, an ordinary user can make a purge leave names (47) or refuse it (27, 28 or 46). Its entry 1002 names what it left. After 47 the product stays installed, part of its data possibly removed, the identity among what may stay: this document gives no repair, and what the purge left is not removed by hand ([No repair, and no removal by hand](#no-repair-and-no-removal-by-hand)).
- An elevated administrator can also run `cg-setup.exe purge --confirm=yes`, or start the service with `--purge`, outside the setup. This document does neither: when it ends 0, each leaves the agent installed with its data removed, and the service's next start stops with code 2, or 13 once an ordinary user has created `C:\ProgramData\CyberGuard`.
- Closing the setup's window while the `Purge` step runs cancels the uninstallation (1602), but the purge goes on, for up to 75 seconds of the stop and 300 of the purge start: the agent stays installed with what the purge left. A purge that ends 0 before an uninstallation by Windows Installer that fails leaves the same: the agent installed, its data removed, the service's next start code 2. Either way, wait until no `cg-setup.exe` process runs and `sc.exe query CyberGuardAgent` shows the service at `4 RUNNING` or stopped with a code other than a process that died (1067, which the service control manager follows with a restart), then run the purge's command again to finish.
- With `C:\ProgramData\CyberGuard` gone and the agent still installed, an ordinary user can create that directory first; a later setup or purge then refuses it (27 or 28), and neither it nor what it holds is removed by hand ([No repair, and no removal by hand](#no-repair-and-no-removal-by-hand)).
- What it leaves outside `C:\ProgramData\CyberGuard`: the inputs' directory, Burn's logs and the Application log's entries. A trace session an earlier crash of the agent left stays until the computer restarts.

#### An endpoint uninstalled without the purge

No agent's service is left to run the purge. Run a setup of the same version or a later one again: on an endpoint with an `agent.toml`, enrolled or not, without `CG_BOOTSTRAP`; on one with no configuration, with a bootstrap file or the wizard. That setup runs the agent, which captures and reports until the purge, and may enroll it, with the configuration's token or the bootstrap file's, only for the purge to remove it locally: that agent stays on the server too. When the agent's certificate has expired, that setup ends 42 or 44. Then run the purge, with the setup of the version just installed.

### The window without events

From the service's stop until a later setup's service runs, the endpoint captures nothing: no process start or logon of that time reaches the server, then or later. The agent keeps no events on disk: those it holds in memory and has not delivered when its stop ends, at most 60 seconds after it began, are lost; its entry 1001 counts them (`events not delivered`), or 1003 when its stop's deadline expired; the entry keeps that text after an uninstallation removes the source, when the command of [Check that the agent reports](#check-that-the-agent-reports), which names the source, no longer finds it. Every maintenance that stops the service opens this window: an uninstallation, a purge, the setup run again, an upgrade.

### The uninstallation's result

The setup's exit code is the result; the second column is the code as Burn's log writes it. 26, 27, 28, 31, 46 and 47 come from the purge alone, and 40 from the purge or a `CG_PURGE` other than `yes`: without one of them, the `Purge` step reads nothing.

| Code | Hexadecimal | What happened | The endpoint after it | Retry |
|---|---|---|---|---|
| 0 | `0x0` | Uninstalled: by default, the agent's data kept; with `CG_PURGE=yes`, the data removed, which the purge's check above confirms. | As above. | None. |
| 3010 | `0xbc2` | As 0, and Windows Installer asked for a restart, which `-norestart` held. | As 0; the MSI's log gives why. | Restart the computer when convenient. |
| 3017 | `0xbc9` | The run failed, and Windows Installer asked for a restart. | As the failed step's code gives it (below). | That code's, after a restart. |
| 1602 | `0x642` | Cancelled: the elevation prompt was declined, or the setup's window was closed. | Declined: unchanged. Closed while the `Purge` step ran: the purge goes on, and the agent stays installed with what it leaves. Closed during Windows Installer's uninstallation: rolled back, the agent installed, without its data after a purge that ended 0. | As [What the purge is not](#what-the-purge-is-not) says: wait for the purge to end, then run the same command again. |
| 1603, or another Windows Installer code | `0x643` for 1603; another code, its own | Windows Installer's uninstallation failed, and its changes were rolled back. | Installed; after a purge that ended 0, without its data, the service's next start code 2. | Read the MSI's log, correct its cause, run the same command again. |
| 20 | `0x14` | As the installation's 20: the setup runs emulated on a machine that is not x64, a compatibility mode is applied to it, or Windows 10 is below build 17763. | Installed, unchanged. | Run it with no compatibility mode. |
| 21 | `0x15` | The setup's code ran unelevated. | Installed, unchanged. | An elevated PowerShell. |
| 26 | `0x1a` | The purge: a service `CyberGuardAgent` whose configuration cannot be read, or that is not the agent's. | Installed; the data and the service untouched. | As the installation's 26 ([The setup's result](#the-setups-result)). |
| 27 | `0x1b` | The purge: `C:\ProgramData\CyberGuard` or its `agent` directory is a reparse point or not a directory, is owned by another account than SYSTEM or Administrators, or gives access to another account; its names cannot be read; or the state they make is untrusted, such as `agent.legacy` beside an `agent` with no `identity`. | Installed; the data and the service untouched. | Find what changed it, and keep what it holds: this document gives no repair, and neither the directory nor what it holds is removed by hand, as for the installation's 27 ([No repair, and no removal by hand](#no-repair-and-no-removal-by-hand)). |
| 28 | `0x1c` | The purge: a console installation of the agent, a directory `agent.migrating`, or `agent.legacy` with no `agent`, a layout no release of the setup writes and the product neither adopts nor migrates, is on the endpoint. | Installed; the data and the service untouched. | No migration, and on an endpoint no removal by hand: those names are another account's or an administrator's ([No repair, and no removal by hand](#no-repair-and-no-removal-by-hand)). |
| 31 | `0x1f` | The purge: the service was not proved stopped within 75 seconds of the setup's request: still running, starting or stopping, or its state or the stop request failing. | Installed, its data kept; the service may be left stopped. | Find why it does not stop (`sc.exe query CyberGuardAgent`, the Application log), then run the command again. |
| 40 | `0x28` | `CG_PURGE` is neither empty nor `yes`; or the purge could not read Windows' folders, open the service control manager or read the service's state before its stop. | Installed; the data and the service as they were. | Correct the command, and run it again. |
| 46 | `0x2e` | The purge removed nothing: the agent's service does not exist, the service control manager refused its purge start, or that start stopped with a code it gives before any removal (1, 2, 10, 13, 14). | Installed, its data kept. A service that was running is started again, and that start can fail: check it with `sc.exe query CyberGuardAgent`. | When the purge start stopped with a code, entry 1002 (below) gives it with its message: for 1 and 2, correct the cause it names; for 10, stop the other agent that runs; then run the command again. For 13 and 14, a directory or a state this document gives no repair of, as for 27 and 28: nothing is removed by hand ([No repair, and no removal by hand](#no-repair-and-no-removal-by-hand)). With no service, uninstall by default, then as [An endpoint uninstalled without the purge](#an-endpoint-uninstalled-without-the-purge) says; neither has been tested with the service deleted. A refused start: `sc.exe qc CyberGuardAgent`, as the installation's 41. |
| 47 | `0x2f` | The purge may have removed some of the agent's data: its start stopped with 15 or another code, its process died, its state could not be read, or it had not stopped after 300 seconds. | Installed, the service left as it was: stopped after a code; still starting after the 300 seconds, its purge start beginning no removal after its own 120; after a process that died, started again without arguments after the failure actions' delay (60, 120 or 300 seconds), which runs the agent as before the purge while `agent.toml` is there, and stops with code 2 once it is not. | Check the service (`sc.exe query CyberGuardAgent`): a `1 STOPPED` whose Win32 exit code is 1067 is a process that died, which the service control manager starts again after that delay; wait for that start to end, at `4 RUNNING` or stopped with a code. After 15, entry 1002 names what the purge left, which it must not follow or remove, and which stays: the product stays installed, part of its data possibly removed, the identity among what may stay; this document gives no repair, and what the purge left is not removed by hand ([No repair, and no removal by hand](#no-repair-and-no-removal-by-hand)). After another cause, run the same command again, which goes on from what is left. |

A code that is not in the table is Burn's own or Windows Installer's: Burn's log gives the step it came from. A `Purge` step that cannot run, its copy in the package cache missing for instance, blocks every uninstallation, a default one included, until it can. When the `Purge` step fails, the uninstallation stops before Windows Installer's, with the setup, the MSI and the service installed. Burn's log then holds `Applied execute package: Purge, result: 0x8007002e` for 46, `0x8007002f` for 47, or the step's other code, and after it the step's rollback, `cg-setup.exe purge` with no option, which changes nothing ([Burn's log](#burns-log)).

After 46 or 47 the agent is still installed. `cg-setup`'s own line, which names the cause, reaches no log under the setup; when the service's purge start stopped with a code, its entry 1002 gives it:

<!-- deploy_ac_021: endpoint-purge-events -->

```powershell
Get-WinEvent -FilterHashtable @{ LogName = 'Application'; ProviderName = 'CyberGuardAgent'; Id = 1001, 1002 } -MaxEvents 5 | Format-List TimeCreated, Id, Message
```

Entry 1002 reads `cg-agent: the service stopped with code <code>:` followed by the purge start's message ([The agent's exit codes](#the-agents-exit-codes)), for 15 the names it left; compare its time with the purge's, since an earlier stop writes the same entry. An abnormal end's names nothing: the purge run again names what remains. A missing service, a refused start, a process that died, an unreadable state and the 300 seconds write no entry of the purge's. Burn's log holds the step's arguments, `--confirm="yes"` among them, and its exit code. These records are the purge's: diagnostics, not evidence, kept on the endpoint and sent to no server. The purge removes the agent's own log, and once the uninstallation has removed the source, Windows keeps the entries and their strings, which hold each entry's text, without the source's message table, and this command, which names the source, answers with an error.

## Later users

Until F3 supplies the user pages, the server operator uses the same CLI, with
`<role>` set to `admin`, `analyst` or `viewer`:

<!-- deploy_ac_021: later-user -->

```sh
docker compose exec api node dist/cli/create-user.js --email <email> --role <role>
```

It asks for the password twice and confirms TOTP as above. Creation writes
`user.create` with no acting principal and the role in its detail; confirmation
writes `user.totp.enroll`. Record who performed the host operation in the
installation's change record. The host operator temporarily handles the user's
password and provisioning secret; arrange a private handover to that user and
keep neither in tickets, shell history or terminal recordings. F3's invitation
flow is still pending (debt #98). This host-administrator procedure does not
publish the api or change its authenticated user-management routes.

### Recover an interrupted confirmation

Keep the authenticator provisioned from the URI and the user id the creation
command printed. Do not create the account again. Run this inside the api
container; enter that id and a current authenticator code at its prompts:

<!-- deploy_ac_021: confirm -->

```sh
docker compose exec api node --input-type=module -e '
import { createInterface } from "node:readline/promises";
const input = createInterface({ input: process.stdin, output: process.stdout });
try {
  const userId = await input.question("User ID: ");
  const totp_code = await input.question("Current TOTP code: ");
  const response = await fetch("http://127.0.0.1:8081/v1/auth/totp/confirm", {
    method: "POST", headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ userId, totp_code })
  });
  console.log("TOTP confirmation HTTP " + response.status);
  if (!response.ok) process.exitCode = 1;
} finally { input.close(); }
'
```

`TOTP confirmation HTTP 200` confirms the factor. The prompts read standard
input; the code is echoed on this terminal, so do not record it. A refused or
rate-limited request exits non-zero. A code just consumed by confirmation cannot
be reused for login; wait for the authenticator's next code. If the provisioning
secret was lost, this procedure cannot recover it: a privileged operator must
use the authenticated TOTP-reset route of SPEC-008 before provisioning again.

## Service logs

Each container's log is rotated at 10 MB, five files per container, and `docker compose logs` reads the containers' logs. The threat model asks for a year of service-log retention: that is the operator's log shipping, through a Docker logging driver set in a compose override file ([configuration](configuration.md)).

## Next

- [Configuration](configuration.md): the bundle's variables, the project name and the start-up checks.
- [Backup and restore](backup-and-restore.md): the two encrypted files `backup.sh` writes, and their restore with the `.env` kept apart.
- [Upgrade](upgrade.md): moving the installation to a newer bundle, and the agent on an endpoint to a newer setup.
- [Data protection](data-protection.md): the personal data the installation holds, for the data protection officer.
- [Forensic verification](forensic-verification.md): checking a forensic export against the fingerprint recorded above.
