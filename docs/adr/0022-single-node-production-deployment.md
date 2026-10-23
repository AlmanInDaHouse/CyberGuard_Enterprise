# ADR-0022: Single-node production deployment — Docker Compose behind a TLS proxy, a credential per service, published images

- Status: Accepted
- Date: 2026-10-19
- Last updated: 2026-10-23
- Deciders: Manuel (project owner), Claude Code (proposal and implementation)

## Context

MVP criterion 7 is "Installation documentation in <30 minutes" (`docs/product/blueprint.md:752`), and the MVP paragraph opens with "An administrator installs the CyberGuard server with Docker Compose on their network" (`docs/product/blueprint.md:740`). Roadmap §F is the phase that delivers it, and the last MVP phase by dependency (`docs/product/roadmap.md`). The repository fixes the ground:

1. **Only a development stack exists.** `infra/dev/docker-compose.dev.yml` runs Postgres, ClickHouse, Redis, ingest and the api, and says "Dev only. Not for production" (`:4`). It publishes every port on `127.0.0.1`, gives ingest and the api one credential per store, and defaults every secret inline. The root `docker-compose.yml` and `docker-compose.dev.yml` are `services: {}` stubs, and the second shares its basename with the dev stack's file (debt #12). `deploy/` holds READMEs only.
2. **Images.** The ingest and api Dockerfiles build from the repository's root and run as root (`services/ingest/Dockerfile:41-42`; debt #92), with no health check. The dashboard has no image.
3. **The edge.** The dashboard's cookies are `Secure`, and `next start` serves no HTTPS. SPEC-009's second Amendment 2026-10-15, item 5, asks the deployment to serve the dashboard only behind a TLS proxy that appends its peer's address to `X-Forwarded-For` or sets it, and passes the browser's `Host` or sets `X-Forwarded-Host` to it (debt #103). The api has no reason to be reachable from a browser: the dashboard calls it server-side (SPEC-009).
4. **The stores.** Ingest and the api both act as the Postgres superuser (debt #86), share one Redis credential, with which ingest, the component exposed to agents, can forge an api session (debt #85), and share one ClickHouse credential, with which the api can write the events the forensic key signs (debt #108).
5. **The agents' side of TLS.** The ingest CA is generated at first start and kept in Postgres, its private key encrypted with `INGEST_CA_PASSPHRASE` (`services/ingest/src/ca.ts`). The certificate ingest issues itself names only `localhost` and `127.0.0.1` (`services/ingest/src/cert.ts:31-36`), so an agent on another host can reach it only through a certificate the operator provides. No tool exports the CA certificate that agents need as their trust anchor. Enrollment runs over plain HTTP (SPEC-004 FR-002), its token readable on the wire (debt #89), and the agent's enrollment client trusts the public web roots, not its trust anchor.
6. **Secrets.** Three pgcrypto passphrases (`INGEST_CA_PASSPHRASE`, `API_DB_ENC_PASSPHRASE`, `API_FORENSIC_PASSPHRASE`) and the stores' passwords reach the services as environment variables. The threat model plans environment-variable secrets with restricted file permissions for the MVP (`docs/security/threat-model.md:200`).
7. **The threat model's open risks that name roadmap §F:** R2, R3, R5, R7, R8, R10, R11, R12 through debt #112 (its SBOM and licences, whose destination is roadmap §F's packaging), R13, R15 and R16, and R4's TLS half (`docs/security/threat-model.md:316-332`).
8. **ADR-0004 §Compliance** (`docs/adr/0004-agent-server-protocol.md:157`) says that "Any change to enrollment, transport, … opens a superseding ADR". Its two amendments, of 2026-05-23 and 2026-10-04, made such changes in place; this ADR does too, through ADR-0004's Amendment 2026-10-19.
9. **The dashboard calls four api routes:** the incident list and detail, the login and the logout (`dashboard/src/lib/api/incidents.ts`, `dashboard/src/lib/auth/api.ts`). The forensic export and report, an incident's events (the drill), the alerts, the playbook records, the password change, the TOTP confirmation and the user routes have no page, and the dashboard shows no agent and issues no enrollment token, though the MVP paragraph has the operator do both from it.
10. **Delegation.** Manuel delegated the deployment-contract decisions on 2026-10-14 (roadmap Part (b), `docs/product/roadmap.md:471-480`) and again on 2026-10-19, an ADR still needing his explicit "sí". They are decided for an enterprise buyer: a security lead and a data-protection officer, under GDPR and PCI DSS.

## Decision

### 1. One Linux host, Docker Compose, a versioned bundle

- The MVP's server installation is one Linux x86-64 host running Docker Engine with the Compose plugin, and `gpg` for the backups (§10). Kubernetes stays out until a customer asks for it (`docs/product/blueprint.md:722`).
- The production compose and the files it reads live in `deploy/docker/` (ADR-0001 Amendment 2026-10-19). Each release publishes them as a bundle whose compose names every image of that release by digest, so an operator installs from the bundle, not from a clone of the repository.
- The stack: a TLS proxy, the dashboard, the api, ingest, PostgreSQL (with pgvector, the image the dev stack runs), ClickHouse and the key-value store ADR-0003 calls Redis, run as Valkey. NATS JetStream and MinIO are not run (ADR-0003 Amendment 2026-10-16: the deployment runs the backends its SPEC lists).
- **The key-value store runs Valkey** (ADR-0003 Amendment 2026-10-23), in the deployment, the dev stack and the tests' backends. Valkey is a fork of Redis 7.2.4 under the BSD-3-Clause licence that speaks Redis's protocol and runs its commands, its ACL and its Lua scripts, which are all the services use. Redis is under RSALv2 and SSPLv1 from 7.4, and Redis 8 adds AGPLv3, licences a buyer's policy may refuse (A9). The compose service, its volume and the services' variables keep the name `redis`: it names the protocol the services speak (`redis://`) and the store as the other ADRs and SPECs call it.
- The developer environment stays in `infra/dev/`, which builds the services from source and is not part of the bundle (ADR-0001 Amendment 2026-10-19).

### 2. The edge: a TLS reverse proxy the product ships

- The product ships a reverse proxy, Caddy (Apache-2.0), as an image of its own built from Caddy's official image, with its configuration inside it. Browsers reach the proxy alone, on 443 by default; port 80 redirects to it (both the operator's settings, SPEC-023).
- It terminates TLS with the certificate and key the operator provides for the dashboard's hostname. For an evaluation, the operator may instead let the proxy issue one from its own internal CA, which browsers do not trust until the operator installs that CA's root.
- It forwards to the dashboard only. Toward the dashboard it sets `X-Forwarded-For` to its own peer's address and ignores the value a client sends, passes the browser's `Host`, and sets `X-Forwarded-Host` to that `Host`; to the browser, its responses carry `Strict-Transport-Security`. That meets SPEC-009's second Amendment 2026-10-15, item 5, and the dashboard is the hop the api lists in `API_TRUSTED_PROXIES`.
- The api is reachable from no network outside the host, so a browser reaches only what the dashboard serves (§Context 9): the dashboard SPEC of roadmap §F adds the pages for the rest (§Out of scope). Agents reach ingest directly: ingest terminates their TLS itself, as built, so no proxy stands in the agents' path (the accepted trust in an mTLS-terminating proxy, `docs/security/threat-model.md:218`, stays inapplicable).

### 3. Networks: the stores and the api on internal networks

- Five networks. Four carry the browser's path and the stores: `public` (the proxy's published ports), `edge` (proxy and dashboard), `app` (dashboard and api) and `data` (api, ingest and the stores). `edge`, `app` and `data` are internal: no route leaves the host from them.
- Ingest joins the fifth, `agents`, which is not internal, for its published agent ports and its egress to the SMTP relay. The api and the dashboard have no egress.
- **The leg between the dashboard and the api stays plain HTTP** (debt #97), on the internal `app` network, which no packet leaves the host by. Reading it needs root on the host, or code running in the dashboard or the api, the leg's two ends, which handle that traffic in clear in any case. The stores are reached without TLS on `data`, which ingest, the component agents reach, shares with the api: a compromised ingest cannot read the api's traffic there, since no container on `data` holds the capability to intercept another's packets on the bridge (`NET_RAW`: the product's containers drop every capability, and the stores drop that one), and reading it from the host needs root. TLS on these legs is the requirement of a deployment that splits the stack across hosts, which the MVP does not support.

### 4. A credential per service, scoped to what it uses

- **PostgreSQL.** Two login roles per service. Its owner role owns the service's schema, `ingest` or `api`, and every object in it, and runs the service's migrations, in a one-shot container before the service starts. Its runtime role, which the running service connects as, has the data rights its code needs on those objects and no right to create, alter or own anything, temporary objects included; the api's has no right to change or delete a row of `audit_log`, which is then append-only by grant. Since no runtime role owns a table, neither running service can attach a policy, a trigger, a view or a function to what the other reads, or create anything in `public`, where the extensions live (the family of CVE-2018-1058). The api's runtime role reads the three ingest tables its routes serve (`alerts`, `incidents`, `playbook_executions`) through a group role with `SELECT` on them alone; it cannot read ingest's `ca`, `enrollment_tokens` or `agents`, and ingest's runtime role cannot read the api's `users`, `forensic_key` or `audit_log`. A later route that reads or writes another ingest table gets a grant scoped to that table, in the SPEC that adds the route and through ingest's migrations; no grant to the api ever reaches `ca`. The superuser is used by the database's initialisation and by the backup and restore scripts alone.
- **Redis.** An ACL user per service, limited to its own key prefixes (ingest's anti-replay nonces; the api's sessions, rate-limit counters and TOTP replay keys; `SCAN` still lists other key names, never their values) and to the commands its client and its scripts run. The default user is disabled.
- **ClickHouse.** The migration one-shot creates and alters ingest's tables, as the administrator; ingest's runtime user inserts and selects. The api's user is read-only, reads `cges_events` and, when a later route needs them, the tables that route's SPEC grants, and of `cges_events` only the rows of the classes its routes return (class 1007 today). The administrator is used by the initialisation, the migration one-shot and the backup and restore scripts alone.

### 5. Secrets: generated at installation, in one file the operator protects

- The bundle's set-up script generates every store password and every passphrase from the operating system's random source, 256 bits each in hexadecimal, and writes them to the bundle's environment file with mode `0600`. It never overwrites a value already set. The containers receive them as environment variables, each container only those it uses.
- The bundle carries no secret value and no default for one. The secrets that come from outside, the SMTP password and the TLS private key, are the operator's to set.
- The environment file holds the keys to the encrypted CA, TOTP secrets and forensic key: the operator keeps it, and backs it up, apart from the database's backups.

### 6. Hardened containers

- The containers of the product, ingest, the api, the dashboard and the proxy, run as a non-root user, with a read-only root filesystem and a temporary filesystem for scratch, all Linux capabilities dropped (the proxy's image removes the file capability Caddy's binary carries), `no-new-privileges`, a health check, and a bounded, rotated log. The proxy listens on unprivileged ports inside its container, and the compose maps 443 and 80 to them.
- The stores run their publishers' images, whose entrypoints drop to the image's own unprivileged user; each has a health check and a rotated log, and drops `NET_RAW`.
- The detection rules and the playbooks are files of the ingest image, owned by root and read-only to the process (debt #109). Ingest writes only its certificate volume and its temporary filesystem.

### 7. Images published, pinned and attested

- On a release tag, CI builds the ingest, api, dashboard and proxy images for linux/amd64, pushes them to the GitHub Container Registry, public, with an SBOM and a build-provenance attestation each, and publishes the bundle. The Dockerfiles pin their base images by digest, and the bundle's compose pins every image by digest; the stores' images carry what their publishers attest.
- The release also carries the agent's executable, built on a Windows runner, with its SHA-256 and a build-provenance attestation. It is not code-signed (Open question 1).
- The workflow that publishes holds `packages: write`, `id-token: write` and `attestations: write`, each only in the job that needs it; its third-party actions are pinned by commit SHA.
- A release is cut by pushing a version tag. The first release waits for the other SPECs of roadmap §F: the agent-installation SPEC, since ADR-0010 asks the packaging SPEC to close the agent's installation posture before any external deployment (`docs/adr/0010-agent-privilege-model-mvp.md:95`); the dashboard's pages, without which the installation ends in no usable product; and the agents' certificate renewal, without which a released agent stops being accepted at 90 days (R5). Publishing being an outward-facing act, it also waits for Manuel's explicit OK.

### 8. The agents' TLS, from the ingest CA

- The certificate ingest issues itself names the hostnames and addresses the operator lists for agents to reach the server, and is reissued when that list changes. An operator-provided pair stays supported, as SPEC-004 provides it.
- **Enrollment moves to TLS** (debt #89's TLS half; ADR-0004 Amendment 2026-10-19). The enrollment listener serves the same server certificate and asks for no client certificate. The agent enrolls only over `https`, and trusts for enrollment, as it does for its heartbeats, its configured trust anchor and nothing else.
- A command of the ingest image prints the CA certificate and its SHA-256 fingerprint, which the operator distributes with the agents' configuration.
- The CA stays inside ingest; separating it (debt #99) is not decided here.

### 9. The forensic key's trust anchor

As ADR-0016's Amendment 2026-10-19 states: at installation the operator records the forensic public key's SHA-256 fingerprint outside CyberGuard, read from a command of the api image, never from an export; a verifier the product ships checks an export against that recorded fingerprint.

### 10. Retention: the operator's setting, per class

- Each class of `cges_events` keeps its rows for a number of days from their arrival that the operator sets, 365 by default for each: ADR-0019 §9's period for logons (3002), and for processes (1007) and connections (4001) the total period ADR-0003's retention table gives normalized events. ADR-0003's cold tier in MinIO is not built, so a row stays in ClickHouse for the whole period (ADR-0003 Amendment 2026-10-19; ADR-0019 Amendment 2026-10-19).
- Container logs, the api's request log among them, are rotated by size on the host. The threat model's year of retention for service logs (`docs/security/threat-model.md:196`) is the operator's log shipping, which the runbook shows; the bundle runs no log store.
- Backups are encrypted on the host, with a passphrase the set-up script generates, as they are streamed out of the stores' containers; a ClickHouse archive exists in clear only on ClickHouse's backup volume, on the host's disk, until it is streamed out and deleted. The threat model names encrypted backups as the mitigation of a backup's leak (`docs/security/threat-model.md:169`). The dumps hold personal data and the encrypted keys; the runbook has the operator copy them off the host.

## Alternatives considered

### A1 — nginx as the proxy

Well known to operations teams. Rejected: SPEC-009 item 5's header behaviour must be written by hand in its configuration, where a missed directive passes a client's `X-Forwarded-For` on, and it has no internal CA for an evaluation. Caddy's defaults are the required behaviour, and the acceptance tests assert it.

### A2 — No proxy in the bundle; the operator's own

Many enterprises have a load balancer. Rejected for the MVP: the thirty-minute installation needs a working edge out of the bundle, and a proxy that misses item 5's behaviour lets a client choose the address the api counts (R16). The SPEC states the proxy's requirements, so a later version can let an operator replace it.

### A3 — One origin for the dashboard and the api

Rejected by SPEC-009's second Amendment 2026-10-15 (its *Decision*): it makes the api reachable from browsers.

### A4 — TLS on the dashboard–api leg and to the stores

A second internal CA, or the ingest CA, issuing the api and the stores' certificates. Rejected for the single host: the legs never leave it (§3), and the certificates would add rotation and trust distribution for no reader the internal networks admit.

### A5 — Docker secrets files, read through `*_FILE` variables

Secrets out of the containers' environment. Rejected for the MVP: whoever can read the environment file or the Docker socket is root on the host already, a file mounted into a non-root container must be readable by that container's user, and each service would gain a second variable per secret. Reopen: the vault integration the threat model reserves for a later SPEC (`docs/security/threat-model.md:200`).

### A6 — Shared credentials, as the dev stack has them

Rejected: it leaves R2, R3 and R7 open.

### A7 — Images built on the operator's host

Rejected: the operator would need the repository and a toolchain, the build would not be reproducible, and no attestation would tie the running code to a commit.

### A8 — Enrollment on the heartbeat listener

One port fewer for a firewall. Rejected: the heartbeat listener would have to accept connections without a client certificate and refuse them per route, which moves the agents' authentication from the TLS layer into application code.

### A9 — Redis's own image

The image the dev stack and the tests run, `redis:7.4-alpine`, until SPEC-023's code replaces it. Rejected by Manuel on 2026-10-23 (Open question 2): from 7.4 its licences are RSALv2 and SSPLv1, and Redis 8's third option is AGPLv3, each of which a buyer's licence review may refuse, and the bundle would ship it to every installation. Valkey runs every command, ACL rule and script the services use.

## Consequences

### Positive

- An installation is a bundle, a set-up script and `docker compose up`, with no store password or passphrase chosen by hand and no port open but the proxy's and the agents'.
- Once SPEC-023's code lands, R2, R3, R8, R15 and R16 close, R13 narrows to the host's internal network, and R4, R7, R10, R11 and R12 narrow (R10: a statement is logged without its parameters unless a session raises `log_parameter_max_length_on_error` for itself, which only a compromised service or the superuser would; R12: an SBOM and a licence check). R5 stays for the renewal SPEC of §F, R1 moves with debt #87 to the agent-installation SPEC, and R14 with debt #98 to the dashboard SPEC. A playbook record's write-once triggers belong to ingest's owner role, which the running ingest does not hold: the record, which ADR-0021 leaves open to the database's owner until roadmap §F, resists at least the running service. The threat model records each change for Manuel's audit.
- The running code is tied to a commit by an attestation an operator can check.

### Negative

- A proxy to maintain and patch with the product.
- A single host: no high availability, and a host's compromise is the deployment's.
- The environment file holds every generated secret; its loss, with no copy, loses the CA, the TOTP secrets and the forensic key.
- An incident older than its events' retention exports and drills the events that remain.
- Until the dashboard SPEC of §F lands, the export, the report and the other routes the dashboard does not call reach no browser.
- Publishing images publicly exposes the build to scrutiny; it also makes the release process a supply-chain surface, which the pinned actions and attestations mitigate and do not remove.

### Neutral

- The dev stack keeps building from source, its stores on the loopback, one credential per store; it takes enrollment over TLS, the non-root images and Valkey's image (SPEC-023 §Operational §13).
- ADR-0003's cold tier and NATS stay unbuilt.

## Compliance

- The production compose MUST publish only the proxy's ports and ingest's agent ports.
- The ingest, api, dashboard and proxy containers MUST NOT run as root, and every long-running container of the stack MUST have a health check.
- In the production compose, ingest and the api MUST NOT share a credential to any store, the api's ClickHouse user MUST NOT be able to write, and no runtime role MAY own an object or create one.
- The bundle MUST NOT contain a secret value or a default for one.
- An image the published bundle names MUST be pinned by digest, and the product's images, ingest, the api, the dashboard and the proxy, MUST carry an SBOM and a provenance attestation.
- The agent MUST NOT enroll over plain HTTP, and, enrolling or enrolled, MUST trust for its server only its configured trust anchor, never the public web roots it bundles.

## Out of scope

- Later SPECs of roadmap §F:
  - the agent's installer and its service mode (ADR-0010 §Decision part 2's supersession), with the agent's key custody (debt #87);
  - the dashboard: the TOTP enrollment and user management (debts #83, #98), the api's login deleting the prior session (debt #102), the agents and the enrollment tokens, the alerts, the forensic export and report, an incident's events (the drill), the playbook records and the password change;
  - the agents' certificate renewal (debt #88), with the rest of debt #89 (the token checked before the certificate is signed, its scope and org, a stored hash of it, a rate limit).
- Separating the CA from ingest (debt #99), high availability, Kubernetes and Helm, a vault, TLS to the stores, ADR-0003's MinIO tier.

## Open questions

1. **Code-signing the agent.** An Authenticode certificate costs money, which is Manuel's. Until then the agent's integrity rests on its published SHA-256 and attestation, and SmartScreen warns on first run. On 2026-10-23 Manuel answered "Todavía no (Recommended)": the first public release waits for roadmap §F's other SPECs (§7), and a certificate bought now would go unused until then. Reopen: before the first public release.
2. **The Redis image's licence.** Resolved on 2026-10-23: Manuel chose Valkey ("Cambiar a Valkey (Recommended)"), which §1 records and A9 explains.

## Decision record

Proposed by Claude Code on 2026-10-19 under Manuel's delegation of 2026-10-14 and of 2026-10-19 ("lo que dependa de mí, decídelo tú en beneficio del proyecto, para un comprador empresarial, registrando cada decisión como delegada"). Ratified by Manuel on 2026-10-23, in chat: "Sí, ratifico", his answer to the question that put this final text to him; the same day he had answered "Sí, ratifico" to the text of 2026-10-19, before the Valkey change (Open question 2) and the review fixes that followed. The load-bearing decisions, each with the alternatives it discards:

- §1, one host with Compose and a bundle per release (against Kubernetes, `docs/product/blueprint.md:722`);
- §1, the key-value store run as Valkey (A9): Manuel's decision of 2026-10-23, in chat ("Cambiar a Valkey (Recommended)"), his answer to Open question 2; the name `redis` kept for the service and the variables is Claude Code's;
- §2, a shipped proxy, Caddy (A1, A2, A3);
- §3, internal networks and no TLS on the host's internal legs (A4);
- §4, a scoped credential per service (A6);
- §5, generated secrets in the environment file (A5);
- §7, published images pinned and attested (A7);
- §8, enrollment over TLS with the agent's trust anchor (A8);
- §8, through ADR-0004's amendment of the same day; §9 and §10, through ADR-0016's, ADR-0003's and ADR-0019's; ADR-0020's two items for roadmap §F, through its Amendment 2026-10-23, which gives them to SPEC-021's code;
- the later §F SPECs' scope (*Out of scope*): the dashboard SPEC takes the pages the MVP paragraph names (the agents, the enrollment tokens, the alerts) and those the api serves without a page, since an installation must end in a usable product; the agent's key custody (debt #87) goes with the service mode, whose account decides who owns the key; the admins' setting of passwords (debt #98) and the login that keeps a prior session (debt #102) go with the dashboard's user pages, from "a later SPEC-008 change". Each moves a destination Manuel had set, and is a delegated decision.

## References

- [ADR-0001](0001-monorepo-layout.md) — the repository layout; its Amendment 2026-10-19 places the production compose.
- [ADR-0003](0003-polyglot-storage.md) — the stores, one instance each; the retention table; its Amendments 2026-10-16, 2026-10-19 and 2026-10-23.
- [ADR-0004](0004-agent-server-protocol.md) — the agent–server protocol; its Amendment 2026-10-19 records enrollment over TLS.
- [ADR-0010](0010-agent-privilege-model-mvp.md) — the agent's installation posture, which a later SPEC of §F supersedes.
- [ADR-0014](0014-human-authentication-model.md) — the human trust boundary in `services/api`.
- [ADR-0015](0015-readonly-clickhouse-reader-in-api.md) — the api's read-only ClickHouse reader, which a grant now enforces.
- [ADR-0016](0016-forensic-evidence-hash-chain.md) — the forensic key; its Amendment 2026-10-19.
- [ADR-0019](0019-cges-authentication-v0-1.md) — the logons' retention; its Amendment 2026-10-19.
- [ADR-0020](0020-process-command-line-and-user.md) — the command line and the user SID; its Amendment 2026-10-23.
- [ADR-0021](0021-soar-mvp-playbook-executor.md) — the playbook records, whose write-once triggers ingest's owner role holds.
- [SPEC-023](../specs/SPEC-023-production-deployment.md) — the implementing SPEC.
- [Threat model](../security/threat-model.md) — R2–R16, the secrets plan.
