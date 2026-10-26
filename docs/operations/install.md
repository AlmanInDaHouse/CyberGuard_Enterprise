# Installing CyberGuard

This document installs CyberGuard Enterprise on one Linux host from the bundle a release publishes, creates its first admin, and enrolls a first agent on a Windows endpoint (SPEC-023, ADR-0022). `<version>` is the release's version, whose tag is `v<version>`.

Each command on the server runs as root in the bundle's directory unless its step says otherwise. The bundle's `.env` is mode `0600` and owned by root, and `docker compose` reads it there, so `docker compose` runs as root too.

Until the agent's installer lands, the agent is installed and run by hand from an elevated PowerShell, and it does not survive a reboot (SPEC-023 Â§Scope).

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

- 64-bit Windows. The agent has been run on Windows 11; Windows 10 and Windows Server 2019 have not been tested.
- An administrator's account: the agent runs elevated.
- A clock kept in time by NTP.
- Logon auditing on in its audit policy: an endpoint whose policy does not log logons sends no logon events and reports no error (SPEC-020 Â§Risks).
- A route to `CG_ENROLL_PORT` and `CG_HEARTBEAT_PORT` on a name of `CG_AGENT_SERVER_NAMES`.

## Download the release

From the release `v<version>` on the repository's releases page, `https://github.com/AlmanInDaHouse/CyberGuard_Enterprise/releases`, download into one directory of the server:

- `cyberguard-deploy-<version>.tar.gz`, the bundle;
- `cg-agent-<version>-x86_64-pc-windows-msvc.exe`, the agent;
- `SHA256SUMS`, the SHA-256 of the bundle, the agent and the licence report, in `sha256sum`'s format.

The release also carries `licences-<version>.md`, the licence report of its dependencies.

### Verify the files

In the directory the files were downloaded to:

<!-- deploy_ac_021: verify -->

```sh
sha256sum --check --ignore-missing SHA256SUMS
gh attestation verify cyberguard-deploy-<version>.tar.gz --repo AlmanInDaHouse/CyberGuard_Enterprise
gh attestation verify cg-agent-<version>-x86_64-pc-windows-msvc.exe --repo AlmanInDaHouse/CyberGuard_Enterprise
```

`sha256sum` prints `OK` beside each file it checks, and skips those of `SHA256SUMS` that were not downloaded. `gh attestation verify` checks the build-provenance attestation the release's workflow made for the file. Stop if any check fails.

The agent's executable is not code-signed, so Windows SmartScreen may warn when it first runs: its integrity rests on these checks and on the one the endpoint makes before it copies the executable (ADR-0022 Open question 1). The bundle's compose names each image by its digest.

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

Keep a copy of `.env` apart from the backups. It is the installation's master secret: it holds the keys to the CA, to the users' TOTP secrets and to the forensic key, which the database holds encrypted, and `CG_BACKUP_PASSPHRASE`, which the backups are encrypted under. Losing it with no copy loses the CA, the TOTP secrets and the forensic key (ADR-0022 Â§Consequences), and leaves every backup encrypted under a passphrase no one holds.

## Start the stack

Two points come before the first start.

- **Ingest's server certificate.** At its first start ingest issues its server certificate from its own CA, for the names of `CG_AGENT_SERVER_NAMES`, and it issues a new one at a start when the list has changed; the agents trust the CA, so they need no change. A pair another CA issued must be in the `ingest_certs` volume before ingest's first start (SPEC-023 Â§Operational Â§6), and the `docker compose up` below is that start. This document installs with ingest's own certificate, and gives no step that places another pair; [configuration](configuration.md) describes an operator-provided pair.
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

The first command prints ingest's CA certificate, in PEM, and, last, its fingerprint: `sha256:` and the SHA-256 of the certificate's DER in lowercase hexadecimal, which lets you compare the file an agent holds with the server's CA. Record that fingerprint beside the forensic key's. The second writes the same output to `server-ca.pem`, the agent's trust anchor: the agent reads the certificate from the file and skips the `sha256:` line after it. When ingest has no CA yet, the command prints `cg-ingest export-ca: no CA yet: ingest generates it at its first start` on standard error and exits with status 1, and the second leaves `server-ca.pem` empty.

Issue the token last, just before the endpoint's steps:

<!-- deploy_ac_021: token -->

```sh
docker compose exec -T ingest node dist/cli/issue-token.js
```

It prints the token on standard output, and on standard error `cg-ingest: token issued for org 'default', expires <time>`. The token is single-use and valid for 15 minutes. It is a secret on your terminal: carry it to the endpoint by a channel you trust. A token that has expired or been used is refused, and the agent exits with code 3: issue another.

### On the endpoint: install and run the agent

Open PowerShell with *Run as administrator*, and confirm that it is elevated:

<!-- deploy_ac_021: endpoint-elevation -->

```powershell
net session
```

An unelevated PowerShell answers it with access denied.

Bring the verified `cg-agent-<version>-x86_64-pc-windows-msvc.exe` and `server-ca.pem` from the server to the endpoint. Before you install the executable, check it against its line in `SHA256SUMS`, where `<agent executable>` is its path on the endpoint:

<!-- deploy_ac_021: endpoint-checksum -->

```powershell
(Get-FileHash -Algorithm SHA256 '<agent executable>').Hash -eq '<its SHA-256 in SHA256SUMS>'
```

It prints `True` when the two match. Stop if it prints `False`.

Copy the executable where only administrators can change it, and the trust anchor to the agent's directory, where `<server-ca.pem>` is the anchor's path on the endpoint:

<!-- deploy_ac_021: endpoint-copy -->

```powershell
New-Item -ItemType Directory -Force 'C:\Program Files\CyberGuard'
New-Item -ItemType Directory -Force 'C:\ProgramData\CyberGuard\agent'
Copy-Item '<agent executable>' 'C:\Program Files\CyberGuard\cg-agent.exe'
Copy-Item '<server-ca.pem>' 'C:\ProgramData\CyberGuard\agent\server-ca.pem'
```

The agent's configuration needs an id that is a lowercase UUIDv7. `New-Guid` prints a version 4 UUID, which the agent refuses; this sets `$id` to a version 7 one and prints it:

<!-- deploy_ac_021: endpoint-uuid -->

```powershell
$ms = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$b = New-Object byte[] 16
[System.Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($b)
for ($i = 0; $i -lt 6; $i++) { $b[$i] = [byte](($ms -shr (8 * (5 - $i))) -band 0xff) }
$b[6] = ($b[6] -band 0x0f) -bor 0x70
$b[8] = ($b[8] -band 0x3f) -bor 0x80
$h = -join ($b | ForEach-Object { $_.ToString('x2') })
$id = '{0}-{1}-{2}-{3}-{4}' -f $h.Substring(0, 8), $h.Substring(8, 4), $h.Substring(12, 4), $h.Substring(16, 4), $h.Substring(20, 12)
$id
```

In the same PowerShell, write the configuration, with `<agent server name>` and `<token>` replaced:

<!-- deploy_ac_021: endpoint-config -->

```powershell
$server = '<agent server name>'
$token = '<token>'
$config = @"
[server]
url = "https://${server}:8444"
heartbeat_url = "https://${server}:8443"
trust_anchor_path = "C:/ProgramData/CyberGuard/agent/server-ca.pem"

[agent]
id = "$id"
hostname = "$env:COMPUTERNAME"

[enrollment]
token = "$token"
cert_path = "C:/ProgramData/CyberGuard/agent/cert.pem"
key_path = "C:/ProgramData/CyberGuard/agent/key.dat"
identity_path = "C:/ProgramData/CyberGuard/agent/identity.json"

[tls]
minimum_version = "1.3"

[envelope]
canonical_form = "JCS"
"@
Set-Content -Encoding ascii -Path 'C:\ProgramData\CyberGuard\agent\agent.toml' -Value $config
```

- `server.url` is enrollment's URL: `https://`, a name of `CG_AGENT_SERVER_NAMES`, and `CG_ENROLL_PORT` (8444 by default; change it in the URL when `.env` sets another). With `[enrollment]` it must be `https://`.
- `server.heartbeat_url` is the heartbeats' URL: the same name and `CG_HEARTBEAT_PORT` (8443 by default), `https://`.
- `server.trust_anchor_path` names the trust anchor's PEM file, and is required with `[enrollment]`. Windows paths in this file are written with forward slashes.
- `agent.id` is the UUIDv7 the snippet set. The agent refuses to start without a valid one, and once enrolled it uses the id the server assigned.
- `agent.hostname` is the endpoint's name, `$env:COMPUTERNAME`; it must not be empty. The agent sends it at enrollment and with each heartbeat.
- `enrollment.token` is the token `issue-token.js` printed. `cert_path`, `key_path` and `identity_path` are where the agent writes its certificate, its private key, sealed with DPAPI to the machine, and its identity.
- `tls.minimum_version` and `envelope.canonical_form` accept only these values.
- Without a `[heartbeat]` section, and without `timeout_seconds`, `max_retries`, `backoff_initial_ms` and `backoff_factor` in `[enrollment]`, the agent keeps their defaults: among them, a heartbeat every 30 seconds ([configuration](configuration.md)).

Run the agent:

<!-- deploy_ac_021: endpoint-run -->

```powershell
& 'C:\Program Files\CyberGuard\cg-agent.exe' --config 'C:\ProgramData\CyberGuard\agent\agent.toml'
```

It runs in the foreground and writes its log, one JSON line per event, to standard output; it keeps no log file until its installer lands (SPEC-023 Â§Scope). At its first start it enrolls with the token and writes `cert.pem`, `key.dat` and `identity.json`, then sets on each a DACL that admits `SYSTEM` and the account that ran it alone. It then tries to remove the token from `agent.toml`, opens its capture (an ETW session named `CGAgent-KernelProcess` and a subscription to the Security log) and starts its heartbeats. When it cannot rewrite `agent.toml`, it logs `could not hygienize enrollment token from config` at `WARN` and runs on with the token left in the file: delete the `token` line from `agent.toml` then. A later start loads the identity and needs no token. An agent that enrolled and then exited with code 9, not elevated, needs no new token either: run it again elevated.

### The agent's files

Until the agent's installer lands:

- `key.dat`, the agent's private key, is sealed with DPAPI at the machine's scope, and its DACL admits the account that enrolled the agent: any process of that account, unelevated included, can read and decrypt it (R1 of the [threat model](../security/threat-model.md)).
- The agent sets the DACL of the three files it writes, and changes no file's owner. Nothing in CyberGuard sets the permissions of `C:\ProgramData\CyberGuard\agent`, of `server-ca.pem`, or of `agent.toml`, which holds the token until the agent removes it. An account that can write in that directory can replace the trust anchor or the configuration, or put files where the agent writes its identity, before the agent reads or writes them; one that can read `agent.toml` can read the token.

### Check that the agent reports

The dashboard has no agents page yet; a later release adds it. The agent's own log shows that it enrolled and reports. Each line is a JSON object whose `fields` hold the event's `message` and values: after its enrollment, a line whose `fields.message` is `identity persisted`; then, every 30 seconds, one whose `fields.message` is `signed heartbeat sent`, with `fields.response_status` 200. A refused heartbeat is logged as `signed envelope rejected by server`, with the server's status in `fields.response_status`; the agent keeps running, and after the retries it drops that batch's events (`batch dropped after repeated rejections`). A clock more than 5 minutes off the server's gives status 422.

On the server, ingest logs a `heartbeat accepted` line, with the agent's `agent_id`, for each heartbeat it accepts, at `info`, `CG_LOG_LEVEL`'s default:

<!-- deploy_ac_021: heartbeat-logs -->

```sh
docker compose logs --no-color --tail 60
```

The `agent_id` is the one `identity.json` holds on the endpoint, the id the server assigned.

The agent's certificate lasts 90 days, and this release does not renew it: `expires_at` in `identity.json` gives its end. Past it, the server refuses the agent's certificate and the agent exits with code 7.

### Stop and restart the agent

Ctrl+C stops the agent: it stops its capture, sends one last heartbeat and exits with code 0. Closing its window or ending its process instead leaves its ETW session running, which `logman query -ets` lists. The agent's next start stops a leftover session of its name itself; to stop it without starting the agent, from an elevated PowerShell:

<!-- deploy_ac_021: endpoint-stop -->

```powershell
logman stop CGAgent-KernelProcess -ets
```

After a reboot, start the agent again with the same command.

### The agent's exit codes

Before it exits with a code other than 0, the agent writes to standard error a message that begins as the table shows.

| Code | Cause | The message begins |
|---|---|---|
| 0 | Stopped with Ctrl+C. It writes nothing to standard error. | |
| 1 | Another runtime error, for instance an ETW session or the Security log's subscription that fails to open for a reason other than privilege. | `cg-agent:` |
| 2 | The configuration is refused, for instance: the file is missing or is not valid TOML; a required key is missing; `agent.id` is not a lowercase UUIDv7; `agent.hostname` or `trust_anchor_path` is empty; `server.url` is not `https://` beside `[enrollment]`, or `server.heartbeat_url` is not `https://`; `interval_seconds`, `request_timeout_seconds`, `backoff_initial_ms` or `backoff_max_ms` of `[heartbeat]` is 0, or its `backoff_factor` is below 1; `tls.minimum_version` or `envelope.canonical_form` has another value; there is no token at the first start. | `cg-agent: invalid config:` |
| 3 | Enrollment is refused: the token is rejected, already used or expired, or the server's answer is malformed. | `cg-agent: enrollment failed:` |
| 4 | The server is unreachable after the retries, or it offers TLS 1.2 alone. | `cg-agent: enrollment failed: server unreachable after` |
| 5 | The identity cannot be written, restricted or loaded, for instance a `key.dat` copied from another machine. | `cg-agent: identity error:` |
| 6 | The server's certificate is not one the trust anchor issued, has expired, or does not name the URL's host (a name outside `CG_AGENT_SERVER_NAMES`); or the trust anchor file is missing or holds no usable certificate. Enrollment ends at once, without retries. | `cg-agent: tls: server certificate verification failed:` or `cg-agent: tls: client configuration failed:` |
| 7 | The server refuses the agent's certificate, for instance once it has expired. | `cg-agent: tls: server rejected client certificate` |
| 8 | Signing a heartbeat fails. | `cg-agent: signing failed:` |
| 9 | The agent is not elevated. | `cg-agent: insufficient privilege to open Microsoft-Windows-Kernel-Process ETW session; run as elevated user or LocalSystem`, or `cg-agent: insufficient privilege to read the Security log; run as elevated user` |

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
