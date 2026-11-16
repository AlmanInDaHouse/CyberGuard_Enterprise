# Installing CyberGuard

This document installs CyberGuard Enterprise on one Linux host from the bundle a release publishes, creates its first admin, and installs a first agent on a Windows endpoint with the agent's setup (SPEC-023, SPEC-024, ADR-0022). `<version>` is the release's version, whose tag is `v<version>`.

Each command on the server runs as root in the bundle's directory unless its step says otherwise. The bundle's `.env` is mode `0600` and owned by root, and `docker compose` reads it there, so `docker compose` runs as root too.

On the endpoint the setup installs the agent as the Windows service `CyberGuardAgent`, which runs as LocalSystem and starts with Windows. An endpoint where an earlier version of this document installed the agent by hand, its files directly in `C:\ProgramData\CyberGuard\agent`, holds a console installation: the setup refuses it (code 28, or 27 when the agent never enrolled there) unless its migration is asked for (SPEC-024 Amendment 2026-11-05), a procedure this document does not give yet. Until it is migrated, its private key, `key.dat`, sealed with DPAPI at the machine's scope, can be read and decrypted by any process of the account that enrolled it, unelevated ones included (R1 of the [threat model](../security/threat-model.md)); and nothing in CyberGuard set the permissions of that directory, of `server-ca.pem` or of `agent.toml`, so an account that can write there can replace the trust anchor or the configuration, and one that can read `agent.toml` could read its token until the agent removed it.

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
- `enrollment.token` is the token `issue-token.js` printed, typed or pasted at the `Enrollment token` prompt, which does not echo it. It reaches neither a command line nor PowerShell's history.
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

The setup never deletes the bootstrap file. Deleting it does not erase it: a backup or a shadow copy of the volume taken meanwhile keeps it. A refusal from 20 to 31 does not use the token: the retry may use it while it is valid, in the same file or, after 23, 24 or 25, in a file written again; after 30, the setup runs without `CG_BOOTSTRAP`.

#### Or install with the wizard

The setup run without `-quiet` shows its window, and, on an endpoint where the agent has no configuration, its wizard:

<!-- deploy_ac_021: endpoint-wizard -->

```powershell
$log = "$dir\setup-$(Get-Date -Format yyyyMMdd-HHmmss).log"
$run = Start-Process -FilePath "$dir\cyberguard-agent-<version>-x64-setup.exe" -ArgumentList '-norestart', '-log', $log -Wait -PassThru
$run.ExitCode
```

After the setup's elevation and its first checks, the dialog *CyberGuard Agent enrollment* asks for the bootstrap file's values: the server's URL and the heartbeat URL, as above; the trust anchor's path, typed (`server-ca.pem` in `$dir`, `C:\CyberGuardSetup\server-ca.pem` above); the fingerprints, one per line; and the token, in a masked field. It fills in nothing and computes no fingerprint from the anchor. It shows a refused field or anchor (codes 24 and 25) and asks again; any other refusal closes it, and the setup exits with that code. Issue the token just before you submit it. On an endpoint whose agent was configured and never enrolled, the dialog offers to keep that configuration or to replace it. On an enrolled endpoint no dialog shows, and the setup repairs or upgrades the agent with its identity.

Cancelling the dialog, or leaving it 15 minutes without an accepted submission (a refused one does not restart them), closes it, and the setup exits 22, the endpoint unchanged by the dialog; so does a submission after the setup's process has ended. Do not close the setup's window while the dialog shows: the setup then exits 1602, but the dialog can stay open, and a submission or a choice to keep then stages the input and stops the service, with nothing to start it again until a later setup, an administrator or the next restart does (SPEC-024 Amendment 2026-11-13, point 8). A token that expires while the dialog waits is refused once the service starts, as any expired token is (code 42).

- Type the token: a pasted token passes through the clipboard of your session, its clipboard history and any clipboard synchronisation the session has.
- The dialog shows during the setup's own run alone, after its elevation, in front of the setup's window. A window of your session can imitate it: a request for the token at any other moment is not the setup's.
- The token may stay in the memory of the setup's elevated process, `cg-setup.exe`, until that process exits. SYSTEM and elevated administrators can read that memory, and so can whoever reads a dump of the process ([Record the endpoint's audit and dump settings](#record-the-endpoints-audit-and-dump-settings)).
- The dialog shows above other windows and asks for the keyboard; when Windows refuses it that, its taskbar button flashes: click the button, or the dialog, to type into it.

#### The setup's result

The setup's exit code is the result. The second column is the code as Burn's log writes it (below).

| Code | Hexadecimal | What happened | The endpoint after it | Retry |
|---|---|---|---|---|
| 0 | `0x0` | The service runs, and the agent logged its identity and an acknowledged heartbeat within 60 seconds of its start. | Installed and reporting. | None. |
| 3010 | `0xbc2` | As 0, and Windows Installer asked for a restart, which `-norestart` held. | Installed and reporting; the MSI's log gives why Windows Installer asked for the restart. | Restart the computer when convenient. |
| 3017 | `0xbc9` | The run failed, by a step's code, a cancellation or another of Burn's results, and Windows Installer asked for a restart. | As that code gives it: the failed step's `Applied execute package` line in Burn's log, or else its `Apply complete` line. | That code's, after a restart. |
| 1602 | `0x642` | Cancelled: the elevation prompt was declined, or the setup's window was closed while it ran. | Declined: unchanged. Closed during the MSI: rolled back, as for 1603. Closed during the setup's checks: they may go on, and may leave the service stopped and the staged configuration, which holds the token, readable by SYSTEM and Administrators alone. Closed during the last step: the agent this run installed stays installed, and its start goes on, may enroll with the token, and reports to no one. | Check the agent first ([Check that the agent reports](#check-that-the-agent-reports)). Enrolled: run the setup without `CG_BOOTSTRAP`. Not enrolled: run it again with its input, with a new token if the old one was used. A setup that passes its checks removes what an earlier attempt staged. |
| 1638 | `0x666` | A newer version of the agent's setup is installed. | Unchanged. | The installed version's setup, or a newer one. |
| 1603, or another Windows Installer code | `0x643` for 1603; another code, its own | The MSI failed, and its changes were rolled back. | Not installed by this run; on an upgrade, the version installed before runs again. | Read the MSI's log (below), correct its cause, run the setup again. |
| -2146889721 | `0x80091007` | A package inside the setup does not match its digest. A setup damaged otherwise may fail with another code, Burn's log holding an `e310` or `e312` line. | Unchanged: nothing ran. | Download and verify the setup again. |
| 20 | `0x14` | Windows 10 is below build 17763, the setup runs emulated on a machine that is not x64, or a compatibility mode is applied to the setup. A Windows before 10 does not run the setup's code, and the setup fails with another code, which Burn's log gives. | Unchanged. | A supported Windows, with no compatibility mode. |
| 21 | `0x15` | The setup's code ran unelevated. | Unchanged. | An elevated PowerShell. |
| 22 | `0x16` | No input: the bootstrap file's command without `CG_BOOTSTRAP` on an endpoint with no configuration, or the wizard cancelled, left 15 minutes or closed. | Unchanged. | Give a bootstrap file, or complete the wizard. |
| 23 | `0x17` | The bootstrap file cannot be used: its path is not absolute, it does not exist or cannot be read, it is not a regular file on a local volume, it is open for writing elsewhere, it holds more than 16 MiB, or an account other than SYSTEM and Administrators owns it or has access to it. | Unchanged. | Write it again as above. |
| 24 | `0x18` | An input is invalid: the file is not UTF-8 TOML, a key is missing or not admitted, a value has the wrong form, `trust_anchor_path` is not absolute, a URL is not `https://`, or the configuration generated from the input fails the agent's own checks. | Unchanged. | Correct the input; the same token while it is valid. |
| 25 | `0x19` | The anchor cannot be read, or its distinct certificates are not exactly those of the fingerprints. | Unchanged. | Correct the anchor or the fingerprints, from the server's record. |
| 26 | `0x1a` | A service named `CyberGuardAgent` exists whose configuration the setup cannot read, or that is not the agent's: its type is not its own process, its account not LocalSystem, or its command line not the agent's. | Unchanged. | Read its configuration (`sc.exe qc CyberGuardAgent`). Not the agent's: remove it, or have it renamed. The agent's: find why it cannot be read. |
| 27 | `0x1b` | `C:\ProgramData\CyberGuard` or its `agent` directory is a reparse point or not a directory, is owned by an account other than SYSTEM or Administrators, or gives any access to another account; or the setup cannot read the names in it. | Unchanged. | Keep what it holds, remove it, and run the setup again. When `agent` holds `cert.pem`, `key.dat` or `identity.json` directly, it is a console installation: remove neither directory. |
| 28 | `0x1c` | A console installation of the agent, or its interrupted migration, is on the endpoint; or a migration was asked for where there is none. | Unchanged. | The migration of SPEC-024's Amendment 2026-11-05, which this document does not give yet. |
| 29 | `0x1d` | The agent installed is newer than this setup. | Unchanged. | The installed version's setup, or a newer one. |
| 30 | `0x1e` | The agent is enrolled on this endpoint, and a bootstrap file was given. | Unchanged; on an upgrade, the version installed before runs again. | The setup without `CG_BOOTSTRAP`, which repairs or upgrades the agent with its identity. |
| 31 | `0x1f` | The service was not stopped 75 seconds after the setup asked it to stop: still running, starting or stopping. | Unchanged, but the service may be left stopped. | Find why it does not stop (`sc.exe query CyberGuardAgent`, and the Application log's entries from `CyberGuardAgent`), then run the setup again. |
| 40 | `0x28` | The setup could not read or write its own state or the host's, in its checks before the MSI or in its last step after it; or, after the MSI, the agent's executable in place is not this version and no restart is pending. | Before the MSI: not installed by this run, and a service that was running may be left stopped. After it: installed, the service not started and possibly stopped, and the staged configuration, which holds the token, left for the next setup. | Run the setup again, with the same input while its token is valid, or a new token. |
| 41 | `0x29` | The service control manager refused to start the service, for instance a disabled service. | Installed, not running; the staged configuration left. | Correct the cause (`sc.exe qc CyberGuardAgent`), then as for 40. |
| 42 | `0x2a` | The service started and then stopped with a code within the 60 seconds. | Installed and stopped; it is not restarted. Its own code is in `sc.exe query CyberGuardAgent` and Application-log entry 1002 ([The agent's exit codes](#the-agents-exit-codes)). | By the service's code: for 3, a token refused, used or expired, a new token in a new bootstrap file, which replaces the configuration of an agent that never enrolled; for 6, correct the URL's host name or the anchor, then run the setup again with a new bootstrap file, and a new token once the old one has expired. A server the service cannot reach is 44, not 42. |
| 43 | `0x2b` | At the end of the 60 seconds the service was neither running nor stopped: still starting, stopping, or paused. | Installed, the service in that state (`sc.exe query CyberGuardAgent`). | Wait, then check the service and the agent as below. |
| 44 | `0x2c` | The service runs, but the agent's identity line and then its first acknowledged heartbeat were not both seen in its log within the 60 seconds; or the log could not be read; or, after the start, the service's state could not be read or the service no longer existed. | Installed, and usually running: still enrolling, retrying a server it cannot reach a minute or more apart, or failing. | Read the agent's log (below); the agent goes on retrying by itself. |
| 45 | `0x2d` | The agent's executable in place is not this version, and Windows replaces it at the next restart. When Windows Installer asked for that restart in this run, the setup exits 3017 instead. | Installed; the service not started by this setup, and the staged configuration, which holds the token, left. | Restart the computer, then run the setup again, with the same input while its token is valid, or a new token. |

After a refusal of the setup's checks, or a failure of the MSI, Burn runs the setup's reversal. When an earlier attempt left its record behind, after its 40, 41 or 45, a failed deletion of its record, or a crash, the reversal removes what that record names, and starts the service if the record says it was running. On an upgrade, a refusal from 20 to 31, and the failures after it, are followed by the installed setup's own run, which may stop and start the agent, set its failure actions and start a service an administrator had left stopped ([upgrade](upgrade.md)).

Running the setup again takes no token on an enrolled endpoint, and neither enrolls again nor overwrites an identity already there. It stops a running service before it changes anything, and the cases above (1602, 31, 40, 45, an upgrade's restore) may leave the service stopped until a later setup, an administrator or the next restart starts it. Re-enrolling an enrolled endpoint is not in this document. A code that is not in the table is Burn's own or Windows Installer's: Burn's log gives the step it came from.

#### Burn's log

`$log` is Burn's log. Windows Installer's log of the MSI is beside it, named after it, ending in `_Agent.log`. The setup runs four steps, which the log names: `Preflight`, its checks and the staging of the configuration; `Agent`, the MSI; `Purge`, which does nothing on an installation; and `Configure`, the start of the service and the wait for its outcome. These lines carry the result:

<!-- deploy_ac_021: endpoint-log -->

```powershell
Select-String -Path $log -Pattern 'Applied execute package', 'Apply complete', 'Exit code'
```

- `Applied execute package: <step>, result: 0x...`, with a result other than `0x0`, names the step that failed; in a cancelled run the result, `0x80070642`, is the cancellation's, not the step's. On an upgrade, a line whose package is a code in braces is the setup installed before, which the upgrade removes.
- `Apply complete, result: 0x...` carries the run's result, and is the line of a failure no step reports, such as 1638, a declined elevation or a damaged package.
- The last line, `Exit code: 0x...`, is the code the setup exits with, in hexadecimal.

A result that begins with `0x8007` ends with the code in its last four hexadecimal digits: `0x8007001e` is 30, `0x80070643` is 1603 and `0x80070666` is 1638. Any other result is itself the code, such as `0x80091007`. When Windows Installer asked for a restart, the setup exits 3010 or 3017 whatever the step's result, which its `Applied execute package` line keeps. A failure before Burn's `Apply begin` line writes neither of the first two lines: the `Exit code` line alone carries it. A setup run with its window shows, on a failure, a page that names the log and the first two lines; the `Exit code` line is written after the window closes. The setup's own steps write their reason to no log: their code and the table above are their result. Burn's log holds the setup's variables and command lines, among them the bootstrap file's path, and never the token. Keep the log with the installation's record.

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

Entry 1000 is the service running, with its version: `cg-agent <version>: the service is running`. 1001 is a clean stop, 1002 a stop with a code and its message, 1003 a stop whose 60-second deadline expired. Any interactive user can write entries under any source, so these entries are diagnostics, not evidence. When no entry matches, `Get-WinEvent` says so with an error, as it does when the source is not registered, before the setup's MSI has run.

The agent's log, `C:\ProgramData\CyberGuard\agent\logs\agent.jsonl`, is one JSON object per line, whose `fields` hold the event's `message` and values. Elevated Administrators can read it:

<!-- deploy_ac_021: endpoint-agent-log -->

```powershell
$agentLog = "$env:ProgramData\CyberGuard\agent\logs\agent.jsonl"
Select-String -Path $agentLog -Pattern '"enrollment response received"', '"agent identity ready"' | ForEach-Object Line
Select-String -Path $agentLog -Pattern '"signed heartbeat sent"' | Select-Object -Last 1 | ForEach-Object Line
```

- `enrollment response received`, once, after the enrollment: `fields.agent_id`, the id the server assigned, and `fields.expires_at`, the end of the agent's certificate. Record that end: the certificate lasts 90 days, this release does not renew it, and past it the server refuses the agent, whose service stops with code 7. The log rotates (below), and drops this line in time.
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

After it enrolls, the agent removes the token from `agent.toml`, as a best-effort rewrite: if that fails, its log has a warning, and the token, already used, stays in the protected file. A token that the service never took, because a setup failed or was cancelled after it staged the configuration, stays in `agent.toml.setup`, readable by SYSTEM and Administrators alone, until a later setup that passes its checks, other than a migration, removes it; a backup or a shadow copy of the volume may keep it.

### The agent's exit codes

Before it exits with a code other than 0, the agent gives a message that begins as the table shows: the service in Application-log entry 1002, as the paragraph after the table says, and the agent run by hand, as development does, on standard error.

| Code | Cause | The message begins |
|---|---|---|
| 0 | A clean stop. | |
| 1 | Another runtime error, for instance an ETW session or the Security log's subscription that fails to open for a reason other than privilege; a token that can control trace sessions but is not an elevated administrator's or SYSTEM's (it is refused before enrollment); or any other failure to take the per-host instance lock. | `cg-agent:` or `cg-agent: instance lock failed:` |
| 2 | The configuration is refused, for instance: the file is missing or is not valid TOML; a required key is missing; `agent.id` is not a lowercase UUIDv7; `agent.hostname` or `trust_anchor_path` is empty; `server.url` is not `https://` beside `[enrollment]`, or `server.heartbeat_url` is not `https://`; `interval_seconds`, `request_timeout_seconds`, `backoff_initial_ms` or `backoff_max_ms` of `[heartbeat]` is 0, or its `backoff_factor` is below 1; `tls.minimum_version` or `envelope.canonical_form` has another value; there is no token at the first start. | `cg-agent: invalid config:` |
| 3 | Enrollment is refused: the token is rejected, already used or expired, or the server's answer is malformed, for the service also a certificate document TLS would refuse. | `cg-agent: enrollment failed:` |
| 4 | The server is unreachable after the retries, or it offers TLS 1.2 alone. Run by hand only: the service waits 60 seconds after an exhausted round, and tries again. | `cg-agent: enrollment failed: server unreachable after` |
| 5 | The identity cannot be written, restricted or loaded, for instance a `key.dat` copied from another machine or, for the service, a `cert.pem` TLS would refuse. | `cg-agent: identity error:` |
| 6 | The server's certificate is not one the trust anchor issued, has expired, or does not name the URL's host (a name outside `CG_AGENT_SERVER_NAMES`); or the trust anchor file is missing or holds no usable certificate. Enrollment ends at once, without retries. | `cg-agent: tls: server certificate verification failed:` or `cg-agent: tls: client configuration failed:` |
| 7 | The server refuses the agent's certificate, for instance once it has expired. | `cg-agent: tls: server rejected client certificate` |
| 8 | Signing a heartbeat fails. | `cg-agent: signing failed:` |
| 9 | The agent is not elevated, or, running without the instance lock, its ETW start was refused because a session of the agent's name already exists (which it may not reclaim). | `cg-agent: insufficient privilege to open Microsoft-Windows-Kernel-Process ETW session; run as elevated user or LocalSystem`, or `cg-agent: insufficient privilege to read the Security log; run as elevated user` |
| 10 | Another CyberGuard agent is already running on this host; this instance exits before enrolling and stops no ETW session. | `cg-agent: another CyberGuard agent instance is running on this host` |
| 11 | `--service` was given, but the Windows service control manager did not start the agent. It runs nothing else and reads no configuration. | `cg-agent: --service requires a start by the Windows service control manager` |
| 12 | The service only: its stop did not end within its 60-second deadline. Application-log entry 1003 names the work still pending and counts the events not delivered. | The service writes no standard error; see below. |
| 13 | The service only: its log file, `%ProgramData%\CyberGuard\agent\logs\agent.jsonl`, cannot be opened at startup, for instance because a directory on its path is not owned by SYSTEM or Administrators or lets another user in, which the service checks for `%ProgramData%\CyberGuard` and `CyberGuard\agent` before it reads its configuration (an administrator removes that directory, after keeping what it holds, and recreates the directory and its files, owned by SYSTEM with their protected DACLs, before starting the service again; when `CyberGuard\agent` holds `cert.pem`, `key.dat` or `identity.json` directly, it holds a console installation's identity: neither it nor `CyberGuard` is removed before SPEC-024's migration moves it, or, when the migration refuses it, before an administrator keeps its files aside); or, while it runs, a write to it fails again after the agent reopens the file, and the service stops. | `cg-agent: the service log cannot be written:` |

A start of the service that migrates a console installation (SPEC-024's Amendment 2026-11-05) reports through the same codes: 2 for start arguments it refuses (`cg-agent: invalid migration arguments:`) or a console configuration it cannot migrate; 5 for a console installation it refuses or a migration it cannot finish; 6 for a trust anchor whose certificates are not exactly those of the fingerprints given (`cg-agent: tls: client configuration failed: migration:`); 13 for a directory it cannot close. From the instance lock on, a refusal says where the console installation is and, before the commit, what the migration restricted of it.

Run as the Windows service (`--service`), the agent writes nothing to standard error. The service control manager's status carries the code: `STOPPED` with Win32 exit code 0 after a clean stop, otherwise 1066 (`ERROR_SERVICE_SPECIFIC_ERROR`) with the code above as the service-specific exit code (`sc.exe query <service>` shows both). The Application log carries the message, under the source `CyberGuardAgent`: entry 1000 when the service runs, with its version; 1001 after a clean stop; 1002 after a failure, with its code and message; 1003 when the stop deadline expired. A configuration error's entry names the file, and the line and column of a TOML error, but quotes no line of it. In service mode a `--config` that is not an absolute path is code 2 (`cg-agent: invalid config: --config must be an absolute path in service mode`). The service runs the secure path alone, at fixed paths under `%ProgramData%` (SPEC-024 Amendment 2026-11-04): `--config` must be `%ProgramData%\CyberGuard\agent\agent.toml`; in it, `server.trust_anchor_path` must be `%ProgramData%\CyberGuard\agent\server-ca.pem` and `cert_path`, `key_path` and `identity_path` of `[enrollment]` must be `cert.pem`, `key.dat` and `identity.json` in `%ProgramData%\CyberGuard\agent\identity\`, each spelled exactly so but for the case of ASCII letters (`<key> must be <path> in service mode`), and `server.heartbeat_url` must be set (`the service requires server.heartbeat_url`); a configuration without `[enrollment]` and `server.trust_anchor_path` is code 2 too. Before it reads `agent.toml`, the service checks `%ProgramData%\CyberGuard` and `CyberGuard\agent` as its log's directories (code 13 above), then `agent.toml` itself: owned by SYSTEM, with a protected DACL that gives access to SYSTEM and Administrators alone, a regular file with one link, not a reparse point; otherwise code 2 (`cg-agent: invalid config <file>: <reason>`). An editor that replaces `agent.toml` with a new file, rather than rewriting it, gives it another owner and DACL, and the next start refuses it; one that holds it open for writing while the service starts makes that start fail with code 2 too. After it reports `RUNNING`, it checks `server-ca.pem` the same way (code 6, `cg-agent: tls: client configuration failed: <reason>`), then `identity\` and its files, which only SYSTEM may access (code 5, `cg-agent: identity error: <reason>`). An `identity\` directory without all three of its files, an empty one included, is code 5 (`cg-agent: identity error: incomplete identity`), with the repair: restore the identity, or remove the directory and enroll with a new token; so is one whose certificate is not for its key or names another agent (`inconsistent identity`): the service neither enrolls again nor overwrites what is there. A certificate in the server's enrollment answer for another key or agent is code 3 (`malformed response`), and nothing is written. The service writes the identity with SYSTEM alone allowed from the moment each file is created, and its removal of the token from `agent.toml` keeps the file's owner and DACL. If the stop deadline expires before the agent can count what it lost, entry 1003 says the count is unknown. The service's log is JSON lines, one record per line: `agent.jsonl` and at most five archives, `agent.1.jsonl` (the newest) to `agent.5.jsonl`, each at most 10 MiB, so at most 60 MiB in all; the oldest is deleted when the active file rotates. A record of 64 KiB or more, its newline included, is replaced by a line that says so. If another process holds the active file open without sharing its deletion (an editor, a tail), the file cannot rotate: the service writes on in it past 10 MiB and tries again each second, and stops with code 13 only if rotation still fails after 60 seconds. Only SYSTEM writes these files and only elevated Administrators read them.

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
- [Upgrade](upgrade.md): moving the installation to a newer bundle.
- [Data protection](data-protection.md): the personal data the installation holds, for the data protection officer.
- [Forensic verification](forensic-verification.md): checking a forensic export against the fingerprint recorded above.
