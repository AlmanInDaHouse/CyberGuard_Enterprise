# Data protection

This document is for the data protection officer who keeps the record of processing of an organisation that runs CyberGuard. It lists the personal data one installation of the bundle holds: where it is kept, who reads it, how long it stays, what is encrypted, what leaves the server, what the backups hold, and how it is erased. The purposes and the legal basis of the processing are the organisation's to record; the periods it decides are applied by the settings this document names, which [configuration.md](configuration.md) describes with the other bundle variables.

A command runs as root in the bundle's directory. `<project>` is the installation's project name: `cyberguard-enterprise`, unless `.env` sets `COMPOSE_PROJECT_NAME` ([configuration.md](configuration.md)). Each volume of the installation is named `<project>_<name>`.

## Whose data

- The people who use the Windows endpoints an agent runs on: the logons to their accounts, and the processes and network connections of those hosts.
- The hosts at the other end of a connection or a network logon, and the people who use them: their IP addresses and workstation names.
- CyberGuard's users, who sign in to the dashboard: their accounts, their sign-ins and their actions.
- The clients that reach the dashboard, by IP address.
- The recipient of the incident notifications, by email address.

## Where it is kept

| Place | What it holds |
|---|---|
| `<project>_ch_data` | ClickHouse's database `cyberguard`: `cges_events`, the endpoints' events, and `heartbeats`. The ClickHouse server also keeps its system log tables there ([ClickHouse's own logs](#clickhouses-own-logs)). |
| `<project>_pg_data` | Postgres's database `cyberguard`: ingest's schema `ingest` (agents, enrollment tokens, the CA, alerts, incidents, playbook records) and the api's schema `api` (users, the audit log, the forensic key). |
| `<project>_redis_data` | The key-value store, Valkey (the compose's service `redis`), which runs with its append-only file on: sessions, rate-limit counters, TOTP replay keys and heartbeat nonces. |
| `<project>_ch_backups` | ClickHouse's backup disk: a backup's archive, in clear, while `backup.sh` or `restore.sh` runs. |
| `<project>_ingest_certs`, `<project>_proxy_data`, `<project>_proxy_config` | Ingest's server certificate and key, and the proxy's state. Certificates/configuration can identify deployment hosts; these are not stores of endpoint events. Private keys require protected custody. |
| The container logs | Each container's output, written by Docker's `json-file` logging driver and rotated by size ([Logs](#logs)). |
| The `clickhouse` container | The ClickHouse server's own log, in files that no volume holds ([ClickHouse's own logs](#clickhouses-own-logs)). |
| `CG_BACKUP_DIR` | The encrypted backups: the directory `backups` of the bundle's directory, unless `.env` names another ([Backups](#backups)). |
| `.env` | The secrets, the relay's credentials and the notification recipient's address. |

## The endpoints' events

Ingest writes each event an agent delivers as one row of `cges_events`. Every row carries the agent's id, the event's id and class, the event's time as the endpoint's clock gave it, and the time it arrived at the server (`arrived_at`). The enrollment request and each heartbeat carry the endpoint's host name, which ingest checks and does not store: a row names its endpoint by the agent's id alone. Ingest accepts the three classes below and no other.

| Class | What a row holds |
|---|---|
| Process activity, 1007 | A process's start or end: its id and its parent's, its unique id, its name, its image path, its creation time and, on an end, its exit code. The image path can hold a user name. The columns `process_command_line` and `subject_user_sid` exist and are empty: the agent's event source gives neither a command line nor a process's user, and roadmap §B2, which would capture them under ADR-0020, is parked. A release that captures them changes this document. |
| Network activity, 4001 | A TCP connection a process of the endpoint opened or accepted, over IPv4 or IPv6: the source and destination IP addresses and ports, the direction, and the process's id. The agent does not report its own connections. |
| Authentication, 3002 | A logon to an account on the endpoint, successful or failed ([The logon telemetry](#the-logon-telemetry)). |

`heartbeats` holds one row per heartbeat that passes ingest's checks of its agent, its time, its nonce and its signature: the agent's id, the heartbeat's sequence number, the times the agent sent it, its status, the agent's uptime, its nonce, and its arrival. A heartbeat refused after those checks, because its events could not be stored, keeps its row.

On the endpoint, the agent keeps the events it has not yet delivered in memory and writes none to disk. It writes its log to its standard output and keeps no log file; that log names no user, SID, domain, workstation or address of a logon (SPEC-020 §Operational §11).

## The logon telemetry

The agent reads its host's Windows Security log and reports each logon written there while it runs: a successful one (event 4624) or a failed one (event 4625). A host whose audit policy does not log logons sends none, and the agent cannot tell that from a host where nobody signed in. A logon while the agent is not running is not reported.

- A record that names no account SID, or whose time is before 1970, is not reported, whether its logon succeeded or failed.
- A successful logon is not reported when its account is not a person's: `SYSTEM` (`S-1-5-18`), `LOCAL SERVICE` (`S-1-5-19`), `NETWORK SERVICE` (`S-1-5-20`), a service SID (`S-1-5-80-…`), an application pool (`S-1-5-82-…`), a virtual machine (`S-1-5-83-…`), a user-mode driver (`S-1-5-84-…`), the Window Manager (`S-1-5-90-…`), the Font Driver Host (`S-1-5-96-…`), and a computer account, whose name ends in `$`.
- Every other successful logon is reported, the anonymous logon (`S-1-5-7`) included.
- Every other failed logon is reported.

Each logon row holds, beside the agent's id and the time:

| Column | What it holds |
|---|---|
| `user_uid` | The account's SID. |
| `user_name`, `user_domain` | The account's name and domain as the event wrote them. On a failed logon both are `<withheld>` unless its failure code names an existing account: a wrong password; an account locked out, disabled or expired; a password expired or that must change; a logon outside the account's hours, from a workstation it may not use, or of a type it is not granted. A person sometimes types a password into the name field; no length or other trace of a withheld name is kept. |
| `logon_type_id` | The logon type. |
| `status_id`, `status_code`, `status_detail` | Success or failure and, on a failure, its two codes. |
| `auth_protocol`, `auth_protocol_id` | The authentication package. |
| `src_ip`, `src_hostname` | The source's IP address and workstation name, when the event has an address. |
| `elevated_token` | On a success, whether the token was elevated. |

No other field of the event is kept: not the logon's ids, its process, the source port, or the event as written.

The logon telemetry is personal data: each row ties an account, and through it a person, to a host and a time, and a network logon to the address it came from. The threat model records the logons, and the connections, as personal data. No route of the api, page of the dashboard or detection rule reads them: the api's ClickHouse user sees the rows of class 1007 alone, and detection reads class 1007 alone. Logons are kept for `CG_RETENTION_LOGON_DAYS`, 365 days by default ([Retention](#retention)).

## Accounts, records and sessions

### Postgres

| Table | What it holds |
|---|---|
| `api.users` | Each user's email address, which is the sign-in name; the password as an Argon2id hash; the TOTP secret, encrypted; the role and the status; when the user was created, last changed and last signed in. |
| `api.audit_log` | Authentication, account-management, refusal and forensic-document records: time, acting user when available, action, target, outcome, request id and client address where the action has them, and details such as role/status, refusal reason or what the forensic key signed. Host CLI user creation has no authenticated acting user or HTTP client address; failed sign-in for an unknown account has no user id. It holds no email address, password, TOTP code or enrollment token. |
| `ingest.alerts`, `ingest.incidents`, `ingest.playbook_executions` | Alerts, incidents and playbook execution records name agents/rules and cite events by id; an alert's deduplication key names a process. An incident's `assigned_to` is written by nothing yet. A dry-run email step stores its rendered subject in its detail, so operator-supplied templates can add personal data to these records. |
| `ingest.agents`, `ingest.enrollment_tokens` | Each enrolled agent's id, public key, certificate and times; each enrollment token, its state and the agent that used it. |

### The key-value store

| Keys | What they hold | Expiry |
|---|---|---|
| `cgsess:*` | A session: the user's id, org and role, its CSRF token, its sign-in time and its last use. | `CG_SESSION_IDLE_MINUTES` after its last use, 480 minutes by default, and `CG_SESSION_MAX_HOURS` after its sign-in at the latest, 24 hours by default. |
| `rl:*` | The sign-in counters, by the email address a sign-in gives and by the client's IP address, and each user's counters of TOTP confirmations and password changes. | 15 minutes after the first attempt a counter counts, restarted by each failure. |
| `totp:replay:*` | Each accepted TOTP code, by user id. | 90 seconds. |
| `nonce:*` | Each heartbeat's nonce. | 10 minutes. |

The backups leave the key-value store out ([Backups](#backups)).

The browser holds two cookies of the dashboard's, `__Host-cgsess` (the session) and `__Host-cgcsrf` (its CSRF token), `HttpOnly`, `Secure` and `SameSite=Strict`, until the browser's session ends or the user signs out.

## Who reads what

Each service reaches each store with its own credential, granted what its code uses.

| Store | Credential | What it reads and writes |
|---|---|---|
| ClickHouse | `cg_ingest`, ingest | Reads and inserts every row of `cges_events` and `heartbeats`. |
| ClickHouse | `cg_api`, the api | Reads the rows of `cges_events` of class 1007 alone, and writes nothing; no heartbeat. |
| ClickHouse | `cg_admin`, the administrator | Everything. Used by the store's initialisation, the `ingest-migrate` one-shot, `backup.sh` and `restore.sh` alone. |
| Postgres | `cg_ingest`, ingest | Reads, inserts and updates agents, enrollment tokens, incidents, playbook records and the detection cursor; reads and inserts the CA and alerts, and does not change them. No user, audit row or forensic key. |
| Postgres | `cg_api`, the api | Users, which it reads, inserts and updates; the forensic key, which it reads and inserts; the audit log, which it appends to and never reads; alerts, incidents and playbook records, which it reads. No agent, enrollment token or CA. |
| Postgres | `postgres`, the superuser | Everything. Used by the store's initialisation, `backup.sh` and `restore.sh` alone. |
| Valkey | `cg_ingest`, ingest | The nonces alone. |
| Valkey | `cg_api`, the api | The sessions, the rate-limit counters and the TOTP replay keys alone. |

Neither runtime role in Postgres deletes a row, and neither ingest's nor the api's ClickHouse user reads the server's system log tables. The two migration one-shots run as each service's Postgres owner role.

Of class 1007, the api reads the events an incident's alerts cite: their ids, agent, activity, process id, the process's unique id, its name, image path, parent process id and time. Through the api, a signed-in user of any role reads the incidents, the alerts, an incident's events and its playbook records; an admin or an analyst requests an incident's forensic export or report, which leaves the server with the user who requested it, CyberGuard keeping no copy and recording each in the audit log; an admin lists the users, with their email addresses, roles and statuses, and manages them. An api route the dashboard does not call reaches no browser: until the dashboard's later pages land, the users' routes are reached from inside the api's container on the host (SPEC-023 §Operational §8).

Root on the host reads `.env`, and with it every store's credentials. So does whoever can use the host's Docker daemon, without `.env`: the compose gives each container, in its environment, the credentials and passphrases it uses, the relay's password among them. The host's administrators are trusted: the threat model accepts the insider at server-administrator level in the MVP.

## Retention

| Data | Kept | Setting |
|---|---|---|
| Process events, class 1007 | 365 days after their arrival | `CG_RETENTION_PROCESS_DAYS` |
| Connections, class 4001 | 365 days after their arrival | `CG_RETENTION_NETWORK_DAYS` |
| Logons, class 3002 | 365 days after their arrival | `CG_RETENTION_LOGON_DAYS` |
| Heartbeats | 30 days after their arrival | `CG_RETENTION_HEARTBEAT_DAYS` |
| Users, the audit log, alerts, incidents, playbook records, agents, enrollment tokens | Without a period | None |
| Sessions, rate-limit counters, TOTP replay keys, nonces | Until their expiry ([The key-value store](#the-key-value-store)) | `CG_SESSION_IDLE_MINUTES`, `CG_SESSION_MAX_HOURS` |
| ClickHouse's `query_log`, `query_thread_log`, `part_log` and `text_log` | 7 days | None |
| ClickHouse's other system log tables | As the pinned ClickHouse image sets them, some without a period | None |
| ClickHouse server's own log | Until its rotation by size drops it, or its container is removed or recreated | None |
| Container logs | Five files of 10 MB per container | None |
| Backups | The operator's: no longer than the longest retention set ([Backups](#backups)) | None |

- Each period is a whole number of days from 7 to 3650, written in decimal digits. A value outside it stops the `ingest-migrate` one-shot and ingest, with the variable named ([configuration.md](configuration.md)).
- A period counts from the row's arrival at the server, not from the event's time, which the endpoint's clock sets.
- ClickHouse deletes a row at its time-to-live merges, so a row may outlive its period by the time a merge takes.
- An event an agent delivers twice is stored twice. Once the table has merged, it keeps the copy that arrived last; until then each copy's period counts from its own arrival.
- A changed period takes effect when `ingest-migrate` next runs and applies it to rows already stored, which on a large table takes time and I/O. With both the periods and this release's TTL rules unchanged, it changes no TTL. Follow [upgrade.md](upgrade.md#a-change-of-retention) when applying a release or retention change.
- Nothing in the product deletes a Postgres row. An alert keeps the ids of the events it cites after those events are deleted.

## Logs

### Container logs

The compose gives every container Docker's `json-file` driver, in five files of 10 MB, the oldest lines discarded as the files rotate. The threat model asks for a year of service logs: keeping them that long is the installation's log shipping, set by a compose override file that [configuration.md](configuration.md#logs) gives. Among the lines it ships are these:

- **The proxy** writes a JSON line per request, which names, among its fields, the IP address the proxy saw the client at (`request.remote_ip`) and the request's URI. The bundle sets no format for that line: its other fields are those the proxy writes by default.
- **The api**, at `info`, the default `CG_LOG_LEVEL`, writes a line when a request arrives and one when it is answered, with the request's method, URL, status and request id and its peer's IP address (SPEC-008). A browser's request reaches the api from the dashboard, so that peer is the dashboard; the client's IP address is in the audit log.
- **Ingest**, at `info`, writes the same two lines for each request. Its peer is the address the agent connected from, so these lines record each endpoint's IP address as the server sees it, except where Docker loses a client's address on its way in ([configuration.md](configuration.md#client-addresses-and-the-docker-daemon)).
- In both services, these request lines also carry the host the request named and its peer's port. They are the defaults of the web framework this release ships, Fastify 5.12: no SPEC fixes ingest's request lines, nor those two fields of the api's.
- **Ingest** also writes a line for each enrollment and heartbeat it accepts or refuses, with the agent id the heartbeat names and a refusal's reason, and lines for its playbook runs and its server certificate. A failed insert of events is logged with the error's name, code and type, not its message, which can quote the row; a failed notification with the error's code, not its message, which can quote the recipient. A store ingest cannot reach, or a detection pass that fails, is logged with the error's text.
- **Postgres** logs a statement, on an error or when an operator turns statement logging on, without its parameters, with two exceptions:
  - A session can raise `log_parameter_max_length_on_error` for itself, which only a compromised service or the superuser would do; its failed statements are then logged with their parameters (threat model R10, debt #125).
  - An error is logged with its detail, which can quote the key it concerns: a user created with the email address of another user of the org logs that address.

The dashboard, ClickHouse, Valkey and the two migration one-shots write to their container logs too. The dashboard writes its start-up check's line, and a one-shot that fails one line with the error's message. The rest of what these containers write is their programs' own, which the bundle does not set and this document does not list.

### ClickHouse's own logs

- The ClickHouse server applies a seven-day TTL to `query_log`, `query_thread_log`, `part_log` and `text_log`, with deletion at merges. Failed-insert details can include a logon or connection row. Other system log tables retain the settings of the pinned image, some with no period. The service runtime credentials cannot read the query/text logs; some system metadata, such as visible tables and columns, remains accessible. The administrator can read the logs, and the backups do not hold them.
- The server writes its own log, at `warning` or above, to files inside the `clickhouse` container, which no volume holds. The log is rotated by size: the bundle's ClickHouse configuration sets `size` to `100M` and `count` to `5`. It can hold a failed insert's row, a logon or a connection among them, for as long as its rotation keeps it (SPEC-023 §Operational §4). It goes with the container when the container is removed or recreated, and it is not in the container logs or the backups.

## Notifications

With the relay set (`CG_SMTP_HOST`, `CG_SMTP_PORT`, `CG_SMTP_USER`, `CG_SMTP_PASS`, `CG_SMTP_FROM` and `CG_NOTIFY_RECIPIENT`, all six or none), ingest emails `CG_NOTIFY_RECIPIENT` through the relay:

- for each new incident: its id, its severity, its title, which names its tactics and its agent's id, and its org;
- by the playbook the image carries, for a new incident of severity 4 or more that a rule raised: the incident's title, id and severity, its agent's id, the playbook's and the execution's ids, and the outcome of the steps run before the email.

The supplied notification templates include no account name, IP address or event body. Review any operator-supplied playbook's email template separately. The send authenticates to the relay only over TLS; a message to a relay that asks no credentials and offers no TLS travels in clear. `CG_SOAR_PLAYBOOKS_DIR=/app/playbooks-none` leaves the playbooks idle ([configuration.md](configuration.md)).

## Encryption

### At rest

Encrypted:

- In Postgres, with pgcrypto, each under a passphrase of `.env`: the CA's private key (`CG_INGEST_CA_PASSPHRASE`), each user's TOTP secret (`CG_API_DB_ENC_PASSPHRASE`) and the forensic private key (`CG_API_FORENSIC_PASSPHRASE`). Passwords are stored as Argon2id hashes.
- The backups, with `gpg` (AES256) under `CG_BACKUP_PASSPHRASE`.

Not encrypted by CyberGuard:

- The stores' volumes: the events, the users' email addresses, the audit log and the sessions are on the host's disk as Docker keeps them. Encrypting that disk is the operator's.
- A backup's ClickHouse archive, on `<project>_ch_backups` while `backup.sh` or `restore.sh` runs.
- The container logs, and the logs shipped from them.
- The ClickHouse server's own log, in its container.
- `.env`, a plain text file of mode `0600`, owned by root.

### In transit

- Agents reach ingest over TLS 1.3, each trusting the server by its configured trust anchor alone: the enrollment over TLS, and the heartbeats, which carry the events, over mutual TLS.
- Browsers reach the proxy over HTTPS, its responses for the dashboard's hostname carrying HSTS; the HTTP port answers with a redirect.
- Inside the host, the proxy reaches the dashboard, the dashboard the api, and the services the stores, in plain text, on the compose's internal networks. The leg from the dashboard to the api carries the session token, and a sign-in's password and TOTP code. The bundle deploys to one host alone.
- The relay, as [Notifications](#notifications) says.

## Backups

`backup.sh` writes two files into `CG_BACKUP_DIR`, mode `0700`, named by the time of the backup in UTC:

- `cyberguard-postgres-<stamp>.dump.gpg`, a dump of the Postgres database `cyberguard`: the users with their password hashes and their TOTP secrets encrypted, the audit log, the alerts, the incidents, the playbook records, the agents, the enrollment tokens, and the CA's and the forensic key's private keys, encrypted;
- `cyberguard-clickhouse-<stamp>.zip.gpg`, an archive of the ClickHouse database `cyberguard`: every event of every class, past its period too while no merge has deleted it, and the heartbeats.

They hold personal data, and they do not hold the key-value store, the certificates' and the proxy's volumes, the ClickHouse server's system log tables, the logs or `.env`. Each is encrypted as it leaves its container; the ClickHouse archive exists in clear on `<project>_ch_backups` until the script deletes it. When that delete, the last step of a run, fails, the script names the file to remove; a run that fails earlier deletes the archive as it exits and reports nothing of it. After any failed run, this command lists what the backup disk holds, and prints nothing when no archive is left:

```sh
docker compose exec clickhouse ls -A /backups
```

- The backups have no period of their own. Keep them no longer than the longest of the retention periods set ([Retention](#retention)), 365 days by default. `backup.sh` deletes no earlier backup: removing the files past that age, on the host and wherever they are copied, is the operator's.
- A backup opens only with the `CG_BACKUP_PASSPHRASE` of the `.env` it was written under. Keep `.env` apart from the backups ([backup-and-restore.md](backup-and-restore.md)): a backup copied with `.env` beside it is readable by whoever holds both.
- A restore brings back Postgres and ClickHouse as they were at the backup. The rows past their period that it brings back stay until the next time-to-live merge deletes them; the Postgres rows come back as they were then.

## Erasure

### The whole installation

This command, in the bundle's directory, removes the installation's containers and networks and its volumes, with the events, the accounts, the audit log, the sessions and the ClickHouse server's system log tables, and, with the `clickhouse` container, the server's own log:

```sh
docker compose down -v
```

Run it before removing `.env`: a `docker compose` command in the bundle's directory reads the compose's variables, and the project name when one is set, from it. Then remove what lives outside the volumes:

- the backups, in `CG_BACKUP_DIR` and every copy made off the host;
- `.env`, and every copy kept apart;
- the logs the installation's log shipping sent elsewhere;
- the forensic exports and reports users requested, wherever they keep them.

Volume/file removal is logical deletion, not a secure-erasure guarantee for the underlying media, snapshots or storage-provider backups. The organisation's decommissioning process must account for those copies and the externally managed certificate keys too.

### One person's data

The bundle has no command or route that deletes one person's rows. A user is disabled, not deleted (`POST /v1/users/:id/status`); neither runtime role in Postgres deletes a row, the audit log is append-only by grant, and an execution record refuses deletion. An event, a logon among them, leaves ClickHouse at the end of its retention period, or, for the earlier of two copies an agent delivered, when a merge keeps the later one; it stays in the backups made before then until they are removed.

The one setting that removes events sooner is a shorter period ([Retention](#retention)), 7 days at the least. It applies to every row of its class, every person's alike, and takes effect at the next `docker compose up -d --wait`.
