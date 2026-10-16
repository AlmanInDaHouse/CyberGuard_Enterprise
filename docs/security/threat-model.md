# Threat Model — CyberGuard

- Status: Accepted
- Date: 2026-05-20
- Authors: Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Purpose

This document is the Phase-0 threat model for CyberGuard. It applies STRIDE — Spoofing, Tampering, Repudiation, Information Disclosure, Denial of Service, Elevation of Privilege — per component, at every interface where data crosses a trust boundary.

It serves three audiences:

1. **SPEC authors** designing security-sensitive behaviour — as the reference against which mitigations must be justified.
2. **Customer due-diligence and audit reviewers** — as a transparent statement of what is mitigated, what is deferred, and what is explicitly accepted.
3. **Harness authors** — as the source for security-category scenarios that exercise the mitigations claimed below.

## Scope

In scope (matches ADR-0001 top-levels):

1. CyberGuard Agent (endpoint).
2. `cg-ingest` (mTLS endpoint).
3. `cg-api` (BFF and dashboard backend).
4. `cg-pipeline` (normalize / enrich / correlate / score).
5. `cg-soar` (playbook executor).
6. `cg-ml` (Python ML service).
7. `cg-forensic` (report generator).
8. Storage layer (PostgreSQL, ClickHouse, Redis, MinIO, NATS JetStream).
9. Dashboard (Next.js frontend).

Out of scope for this iteration:

- Insider threats at server-administrator level — assumed trusted in MVP; documented as a future enterprise concern.
- Supply-chain attacks against build dependencies — covered by a separate runbook on dependency review; not duplicated here.
- Physical attacks on server hardware — operational concern, not product concern.

## Components

### 1. CyberGuard Agent

**Trust boundary:** runs on customer endpoints, potentially under attacker control at user level. Communicates outbound only with `cg-ingest`. Holds private key material in OS-protected storage.

**Assets at risk:** Ed25519 private key, X.509 client certificate, locally buffered events (encrypted at rest), agent binary on disk.

**STRIDE:**

- **Spoofing:** stolen enrollment token (mitigated by single-use, TTL 15 min, scope-bound JWT per ADR-0004 §Enrollment); cloned agent identity via key material extraction (mitigated by OS-protected key storage; not mitigated against root-level local compromise — see *Threats explicitly accepted* below).
- **Tampering:** local attacker modifies the agent binary or the buffered events. Installer code-signing mitigates the first install. Buffered events are encrypted at rest with a key derived from DPAPI; tampering invalidates the buffer. Runtime binary-integrity attestation is deferred to enterprise phase.
- **Repudiation:** agent denies sending an event. Mitigated by monotonic `sequence_number` plus signed envelope per ADR-0004 §Message integrity per batch; every accepted event is non-repudiable against the agent's private key.
- **Information Disclosure:** local attacker extracts the private key from DPAPI (requires LocalSystem on Windows; Linux and macOS keyring deferred per ADR-0002 Rule 2). Network eavesdropping is mitigated by mTLS 1.3 per ADR-0004 §Transport. On-endpoint telemetry visibility is intrinsic — the agent sees what it sees.
- **Denial of Service:** malformed inbound config crashes the agent — mitigated by Rust memory safety (ADR-0002 §A2 rejection rationale) and by schema validation on config load. Agent consuming excessive endpoint resources is mitigated by configurable resource caps; specific limits live in the agent SPEC.
- **Elevation of Privilege:** an agent vulnerability is exploited for endpoint privilege escalation. Mitigated by (a) Rust memory safety eliminating large classes of CVEs, (b) least-privilege execution (agent runs as a dedicated service account, not LocalSystem unless a specific sensor requires it), (c) no remote code execution capability in MVP. Sandboxing via AppContainer or namespaces is deferred to enterprise phase.

**Open questions:** runtime binary-integrity attestation (TPM-backed remote attestation) is desirable but deferred.

### 2. cg-ingest

**Trust boundary:** receives mTLS connections from agents over the internet or intranet; publishes to NATS JetStream. Internal-only consumers downstream.

**Assets at risk:** internal CA trust chain, NATS publish credentials, integrity of the ingested event stream.

**STRIDE:**

- **Spoofing:** agent impersonation is mitigated by mTLS plus the signed envelope per ADR-0004 §Transport and §Message integrity per batch. The `agent_id` in the envelope must match the `CN` of the client certificate.
- **Tampering:** event injection past TLS termination (a malicious reverse-proxy operator, or a compromised termination layer) is mitigated by message-level Ed25519 signature, which is verified server-side after termination.
- **Repudiation:** server claims an event was not received. Mitigated by an audit log of accepted envelopes with their nonces, sequence numbers, and acceptance timestamps.
- **Information Disclosure:** agent metadata leak via verbose error messages. Mitigated by structured errors with PII and identifier sanitisation; specific error policy lives in the `cg-ingest` SPEC.
- **Denial of Service:** flood of malformed requests, replay flood, slow-loris connections. Mitigated by the nonce cache plus sequence-number defense (ADR-0004 §Server validation order), TLS-level connection caps, and per-agent rate limits.
- **Elevation of Privilege:** a deserialisation vulnerability in the event parser yields arbitrary code execution. Mitigated by Go strict typing, by schema validation against the CGES contract (ADR-0006), and by a fuzz-test obligation that every CGES schema bump must pass.

**Open questions:** trust-forwarding of client-certificate details when terminating mTLS at an external reverse proxy is operational and is documented separately.

### 3. cg-api

**Trust boundary:** receives REST and WebSocket traffic from authenticated dashboard sessions and from authorised external clients. Backs the dashboard and is the only path through which users mutate state.

**Assets at risk:** session tokens, TOTP secrets, RBAC role assignments, every mutation to the audit log.

**STRIDE:**

- **Spoofing:** stolen session token, TOTP bypass, credential stuffing. Mitigated by short-lived signed sessions with rotation, RFC 6238 TOTP enforced on every login, password hashing with Argon2 or bcrypt, and login rate-limiting with progressive back-off per account and per source IP.
- **Tampering:** CSRF, parameter pollution, mass assignment. Mitigated by Fastify-issued CSRF tokens on every mutation, by Zod strict schemas per ADR-0002 Rule 4 (no implicit type coercion, no extra fields), and by explicit allow-listed update fields per resource.
- **Repudiation:** user denies an action. Mitigated by an audit log with `user_id`, `org_id`, `action`, `target_id`, `timestamp`, and the request correlation id, written append-only to Postgres per ADR-0003.
- **Information Disclosure:** insecure direct object references (IDOR), verbose error messages, RBAC bypass. Mitigated by object-level authorisation checks on every read and write, by error sanitisation, and by RBAC tests that block CI on missing checks.
- **Denial of Service:** expensive queries, unbounded list endpoints, payload bombs. Mitigated by query timeouts, mandatory pagination on list endpoints, per-endpoint and per-user rate limits, and request-size caps.
- **Elevation of Privilege:** RBAC misconfiguration, privilege accumulation across roles. Mitigated by a role-assignment audit trail, by the principle of least privilege on default roles (admin, analyst, viewer per Blueprint §10), and by integration tests that verify each role's capability matrix.

**Open questions:** WebAuthn upgrade from TOTP is an enhancement target, not an MVP requirement.

### 4. cg-pipeline

**Trust boundary:** internal-only. Consumes `events.normalized.*` from NATS, writes alerts to Postgres and ClickHouse, publishes `alerts.*` and `incidents.*` to NATS.

**Assets at risk:** rule-engine integrity, scoring weights configuration, dedup state, correlation-window state.

**STRIDE:**

- **Spoofing:** spoofed NATS publish to pipeline-owned subjects. Mitigated by JetStream auth with per-account permissions; only `cg-ingest` can publish to `events.raw.*` and only `cg-pipeline` to `events.normalized.*` and `alerts.*`.
- **Tampering:** rule injection at runtime. Rules are loaded only from `rules/` at startup; runtime rule modification is disabled in MVP. Adding or modifying a rule requires a deploy.
- **Repudiation:** the pipeline silently drops an event. Mitigated by structured logging of every drop with reason code, plus a `dropped_events_total` Prometheus metric per reason.
- **Information Disclosure:** an alert leaks to the wrong organisation's subject. Mitigated by subject auth scopes keyed on `org_id`; each pipeline worker enforces the `org_id` of the consumed event onto the published alert.
- **Denial of Service:** queue poisoning via oversized events or pathological correlation patterns. Mitigated by JetStream max-message-size enforcement, by a dead-letter subject for non-parsable events, and by correlation-window caps per host and per user.
- **Elevation of Privilege:** a code path overrides RBAC on alert visibility. Mitigated by server-side enforcement on every read; the pipeline never trusts client-claimed roles.

**Open questions:** hot-reload of detection rules (without redeploy) is deferred to a later phase; the operational tradeoff is real but the MVP forbids it.

### 5. cg-soar

**Trust boundary:** internal-only. Triggered by `alerts.*` and `incidents.*` from NATS; executes playbook steps against internal and external systems.

**Assets at risk:** playbook integrity, execution audit log, third-party integration credentials.

**STRIDE:**

- **Spoofing:** forged trigger event injected directly onto `alerts.*` or `incidents.*`. Mitigated by NATS account isolation; only `cg-pipeline` and `cg-api` can publish to these subjects.
- **Tampering:** playbook YAML tampered on disk. Playbooks are loaded from `playbooks/` at startup; runtime modification goes through `cg-api`, which writes to git or to a managed store with audit logging.
- **Repudiation:** execution denial. Mitigated by an immutable execution audit log with a hash-chained record per step, per ADR-0005 §Compliance.
- **Information Disclosure:** integration credentials leaked via execution logs. Mitigated by credential redaction in logs and by credential storage in a secrets backend (vault integration is a future SPEC; MVP uses environment variables with restricted file permissions).
- **Denial of Service:** runaway playbook loop or pathological retry. Mitigated by execution timeouts, by maximum step counts per execution, and by idempotency keys per execution that block re-entry.
- **Elevation of Privilege:** a destructive action is executed without human approval. Forbidden by the action policy of ADR-0005 §Action policy by reversibility; destructive actions always require an explicit logged `human.approve` step. The policy is enforced in code, not merely documented.

**Open questions:** vault integration for secrets is reserved for a future SPEC; MVP runs on env-var secrets with operational compensating controls.

### 6. cg-ml

**Trust boundary:** internal-only. Receives feature vectors from `cg-pipeline`, returns confidence scores. Never executes external actions directly.

**Assets at risk:** ML model files (pre-trained, pinned by SHA), embedding vectors, feature snapshot integrity.

**STRIDE:**

- **Spoofing:** model file replaced on disk or in MinIO. Mitigated by SHA pinning per ADR-0005 §ML model governance; startup validates the checksum and refuses to load on mismatch.
- **Tampering:** model file modified between SHA validation and inference. Mitigated by loading the model into memory once at startup and refusing on-disk reload; model updates require redeploy.
- **Repudiation:** model output denial. Mitigated by recording `model_version`, `confidence_score`, and the input feature snapshot per alert per ADR-0005 §ML model governance.
- **Information Disclosure:** feature snapshot leaked via service logs. Mitigated by storing the snapshot only in the alert payload (per ADR-0006); the service logs the snapshot hash, not its content.
- **Denial of Service:** large input crashes inference. Mitigated by input-size caps and per-request timeouts.
- **Elevation of Privilege:** ML output drives a destructive action without a corroborating rule. Forbidden by the reversibility policy of ADR-0005; semi-reversible actions require a rule confirmation even if ML triggered, and destructive actions always require human approval.

**Open questions:** model-drift threshold tuning per organisation is configurable but the defaults need calibration data that does not exist until production runtime.

### 7. cg-forensic

**Trust boundary:** internal-only. Reads alerts and incidents from Postgres and ClickHouse, reads events from ClickHouse, reads artifacts from MinIO, writes signed reports to MinIO.

**Assets at risk:** forensic report integrity, evidence hash chain, AI-generated narrative correctness.

**STRIDE:**

- **Spoofing:** forged report request. Mitigated by `cg-api` authorisation; only RBAC-allowed roles can request a report, only allowed roles can export.
- **Tampering:** report modified after generation. Mitigated by MinIO immutable bucket per ADR-0003 Rule 3, plus server signature over the report root hash.
- **Repudiation:** report generation denial. Mitigated by logging report generation in the audit log with `requested_by`, `incident_id`, `report_hash`, `timestamp`.
- **Information Disclosure:** report shared via an unauthenticated link. Mitigated by requiring authenticated access on every report URL; export URLs are time-bounded and the export action is itself audit-logged.
- **Denial of Service:** a large incident triggers unbounded report generation. Mitigated by a maximum-evidence count per report and by pagination of evidence sections.
- **Elevation of Privilege:** AI-generated narrative drives a SOAR action. Forbidden by design — reports are read-only artifacts, never triggers.

**Open questions:** signed PDF (PAdES) versus signed HTML versus signed JSON as the canonical export format; decision deferred to the `cg-forensic` SPEC.

### 8. Storage layer

**Trust boundary:** all backends sit on the internal service network and accept connections only from CyberGuard service accounts.

**Assets at risk:** every persistent piece of CyberGuard data — events, alerts, incidents, cases, audit log, forensic artifacts, ML models.

**STRIDE:**

- **Spoofing:** connection from an unauthorised client. Mitigated by network policy (Compose network in MVP, NetworkPolicy in Kubernetes later), per-service database users with scoped privileges, and TLS-encrypted backend connections in the enterprise phase.
- **Tampering:** direct database write bypassing the API. Mitigated by per-service DB users with the minimum privilege required (for example, `cg-ml` is read-only on `alerts`, `cg-pipeline` cannot read `cases`).
- **Repudiation:** data deleted without trace. Mitigated by the Postgres append-only `audit_log` with a signed MinIO mirror per ADR-0003 §Retention model.
- **Information Disclosure:** backup leak, snapshot leak, query log leak. Mitigated by encrypted backups (operational SPEC deferred), by query-log filtering of PII fields, and by access restrictions on backup buckets.
- **Denial of Service:** storage exhaustion via unbounded retention. Mitigated by the retention table of ADR-0003 plus MinIO lifecycle policies.
- **Elevation of Privilege:** a service account escalates to DB superuser. Mitigated by the principle of least privilege on every DB user; superuser operations require human admin intervention through a dedicated bastion.

**Open questions:** at-rest encryption configuration (Postgres TDE, ClickHouse disk encryption, MinIO server-side encryption) is operational and is documented separately.

### 9. Dashboard (Next.js)

**Trust boundary:** served to authenticated SOC operators via browser. Server-rendered components run on the server; client interactivity runs in the browser. The browser is partly trusted (the operator is a trusted user, but the device may not be hardened).

**Assets at risk:** session token in browser storage, sensitive incident and event content rendered on screen.

**STRIDE:**

- **Spoofing:** phishing impersonation of the dashboard URL. Mitigated partially by HSTS plus public-key pinning at deployment; user training on URL verification lives in operations runbook.
- **Tampering:** XSS and CSRF. Mitigated by React's default escaping, by a strict Content Security Policy header, and by Fastify-issued CSRF tokens on every mutation. Inline scripts and event handlers are forbidden.
- **Repudiation:** user denies an action in the UI. The UI alone cannot prove non-repudiation; mitigated server-side by the audit log written by `cg-api`.
- **Information Disclosure:** session token in URL, sensitive data in client state, browser cache retention. Mitigated by storing tokens in `HttpOnly` `Secure` `SameSite=Strict` cookies, by `Cache-Control: no-store` on sensitive views, and by stripping sensitive fields from client-side serialised state.
- **Denial of Service:** large response loads or expensive client renders. Mitigated by server-side pagination, by virtualised lists on long views, and by deferred-skeleton loading for slow widgets.
- **Elevation of Privilege:** privilege visible in UI but not enforced server-side. Mitigated by server-side authorisation on every mutation; the UI displays only what the server allows.

**Open questions:** privileged actions through the dashboard (destructive playbook triggers) are governed by ADR-0005 §Action policy; UI surface for human approval is a SPEC of `cg-soar`.

## Cross-cutting concerns

### Logging and monitoring

All inter-service communication, all authentication events, and all destructive operations are logged. Audit-log retention follows ADR-0003 §Retention model: Postgres append-only with a signed MinIO mirror, 7 years cold. Service operational logs (Loki per Blueprint §4) carry 1-year retention by default. The signing key of the audit-log mirror is rotated annually; old signatures remain verifiable.

### Secrets management

No secrets are committed to git. The repository `.gitignore` excludes `*.key`, `*.pem`, `*.pfx`, `*.p12`, `.env`, and `secrets/`. Local development uses `.env.example` files that document required variables without supplying values. Production secrets management (Vault, AWS Secrets Manager, or equivalent) is reserved for a future SPEC; MVP deployments use env-var secrets with restricted file permissions and operational compensating controls.

### Dependency review

Build-dependency review is covered by a separate runbook, [dependency-review.md](dependency-review.md): `pnpm audit` for Node and `cargo audit` for Rust, in CI on each change to the dependencies and every week, and a licence checked by hand when a dependency is added. Go and Python, when either enters the repository, add `govulncheck` and `pip-audit`. This document does not duplicate dependency-CVE handling.

### Incident response for CyberGuard itself

How CyberGuard's operators respond to a CyberGuard-server incident — distinct from how a CyberGuard customer responds to a SOC incident the product detects — is out of scope for this document and is deferred to the operations runbook.

## Threats explicitly accepted in MVP

The following threats are recognised and not mitigated in MVP. Each is accompanied by a one-line justification:

- **Root-level local compromise of an endpoint defeats agent key custody.** The agent is not a rootkit. If the attacker is root on the endpoint, they own the endpoint.
- **Internal CA private-key compromise is catastrophic.** MVP runs the CA in software; HSM-backed CA is deferred to enterprise phase per ADR-0004 §Consequences.
- **Network-layer DDoS is not addressed by the product.** DDoS protection is a deployment concern (Cloudflare, AWS Shield, or an on-prem appliance), not a product feature.
- **Dashboard phishing.** User training on the dashboard URL is an operations concern; the product surfaces TOTP MFA, not anti-phishing primitives.
- **Trust in the reverse-proxy mTLS-termination layer.** The proxy must forward client-certificate details to `cg-ingest` correctly; trust in that layer is operational, not cryptographic.
- **At-rest encryption at the storage layer.** Configuration of Postgres TDE, ClickHouse disk encryption, and MinIO server-side encryption is operational and is documented separately.
- **Insider threat at server-administrator level.** Assumed trusted in MVP; future enterprise concern requiring auditable admin actions and break-glass procedures.

## Amendment 2026-10-13 (ADR-0020): the command lines and users of processes

**Status.** Co-located with ADR-0020; Accepted with it on 2026-10-13. It adds to §1, to §8, to *Threats explicitly accepted in MVP* and to *References*, and restates nothing. The threat model remains `Accepted`.

**Context.** ADR-0020 gives each process start the agent reports the command line the process was started with and the SID of the user it runs as. A command line can hold secrets — a password after `net use`, `psexec -p`, `cmdkey /pass:` or `schtasks /rp`, a connection string's `Password=`, a token or key passed as an argument, credentials inside a URL — and personal data: user names inside paths, document names, addresses (ADR-0020 §Context 7). A SID identifies an account, and through it a person. Neither is named among the assets of §1 or §8.

**Amendment.**

- **§1 CyberGuard Agent, assets at risk**, add: the command lines of the processes the agent observes, before and after their secrets are replaced, and the SIDs of their users. On the endpoint they live in the buffers of the agent's kernel ETW session, in the agent's memory for the times ADR-0020 §2 states, and in what Windows keeps of that memory: the paging file, a crash dump of the agent, and a kernel or complete memory dump of the host.
- **§1, Tampering**, add: *a process shapes its own command line against detection.* A payload placed where a pattern marks a secret reaches detection as `<redacted>`; text placed past the source's 1024 characters does not reach it; a burst of process starts larger than the join table's capacity sends Launches without their command line and user (ADR-0020 §1). Partly mitigated: the key before a replaced value stays, and the markers `cg_cmd_line_redacted`, `cg_cmd_line_possibly_truncated` and `cg_kernel_record_unavailable` mark each case on the event and reach detection (ADR-0020 §2, §4), so a rule can key on them once the SPEC of roadmap §B3 admits them. Not mitigated: a parent that creates a child suspended and rewrites its arguments before it runs leaves a decoy in the start event (argument spoofing); the command line is the one the process was created with (ADR-0020 §Consequences).
- **§1, Information Disclosure**, add:
  - *A secret leaves the endpoint inside a command line.* Mitigated by the agent: before an event is serialized, each value that the closed list of SPEC-021 marks as a secret is replaced with `<redacted>` (ADR-0020 §2). A format the list does not name is not mitigated (accepted below).
  - *An unredacted command line reaches disk.* The agent writes none to disk or to its own output (ADR-0020 §2, §Compliance). Windows may page its memory out, or keep a crash dump of the agent and send it through Windows Error Reporting, and a kernel or complete memory dump of the host holds the kernel session's buffers; whether a deployment turns those off is the deployment contract's (ADR-0020 Open questions 4).
  - *Another elevated process reads the kernel's process events.* It can consume the agent's kernel session by its name, or open a session of its own; either way it gets what elevation already gives it. Not mitigated by the agent (accepted below).
- **§8 Storage layer, assets at risk**, add: the columns `process_command_line` and `process_user_uid` of `cges_events` in ClickHouse.
- **§8, Information Disclosure**, add:
  - *Command lines and SIDs kept longer than needed.* Mitigated by a column time-to-live of 365 days from arrival on both columns, and by no copy that outlives it, a backup included (ADR-0020 §Compliance): neither enters an alert's `dedup_key` or an incident's `grouping_key`, which Postgres keeps without retention (ADR-0020 §2–§4). A row can hold its values past the 365 days until ClickHouse runs a time-to-live merge.
  - *Row data in a log.* Neither the agent nor the server writes a command line or a SID to its own output (ADR-0020 §Compliance). The ClickHouse settings that keep row data out of its own logs and system tables are SPEC-021's for those the ingest service sends with its queries, and the deployment contract's for the server's own (ADR-0020 §4): the query-log filtering of PII fields this section names.
  - *A compromised api reads them, and more.* The api's ClickHouse reader is read-only in use (ADR-0015 §Compliance), and no ClickHouse grant enforces that or limits its columns: in the dev stack it logs in as the ingest's user (`infra/dev/docker-compose.dev.yml:134`, `:207`), which can also write and alter tables: with those credentials an attacker could copy the rows or remove the time-to-live. Not mitigated in the MVP; users with scoped grants are the deployment contract's (roadmap §F). No surface shows the fields yet; the first that does states its access rules in its SPEC (ADR-0020 §4).
- **Threats explicitly accepted in MVP**, add:
  - **A secret in a form the closed list does not name leaves the endpoint.** A positional password, or a token after a flag the list does not know, travels as typed and is kept for 365 days. The list is ratified with SPEC-021 and grows by amendment.
  - **An elevated process on the endpoint reads command lines from the kernel.** The kernel's process events belong to the endpoint, not to the agent; as with the root-level compromise above, an attacker elevated on the endpoint sees what the agent sees.

**Effect on other sections.** None is restated. §8's per-service database users with scoped privileges (Spoofing, Tampering) describe the plan, not the repository: the ingest service and the api share one ClickHouse user (`infra/dev/docker-compose.dev.yml:134`, `:207`), as the api entry above states. Its query-log filtering of PII fields is not configured yet: SPEC-021 and the deployment contract set it (ADR-0020 §4). Those statements, with the others of this document that later ADRs changed, wait for their own amendment (`docs/handoff-session-36.md`, debt #55).

## Amendment 2026-10-14 (ADR-0021): cg-soar as an in-process module of the MVP

**Status.** Co-located with ADR-0021; Accepted with it on 2026-10-14 (Manuel, in chat: "si a 1, 2 y  3"). It restates §5's trust boundary and mitigations for the MVP and adds to §2, §3, *Threats explicitly accepted in MVP* and *References*. The threat model remains `Accepted`.

**Context.** ADR-0021 runs SOAR as a TypeScript module inside `cg-ingest`. The detection cycle calls it after the commit of an incident it has just created; there is no bus and no separate service. Its actions are a closed list: `agent.isolate`, run in dry-run only, and `notify.email`, run live.

**Amendment.**

- **§5 cg-soar, trust boundary**, for the MVP: in-process in `cg-ingest`. It reaches two things outside the process: Postgres, where it writes its execution records with the ingest service's credential, and SPEC-014's SMTP relay, through `notify.email`.
- **§5, Spoofing**: the trigger is the detection cycle's own signal that it created an incident, so nothing arrives across a boundary as a trigger. A trigger can still be caused from outside, by making the cycle create an incident:
  - an enrolled agent can send events that match a High or Critical rule;
  - anyone holding the ClickHouse credential ingest uses can insert such events; in the dev stack the api shares it (`infra/dev/docker-compose.dev.yml:134`, `:207`), as the Amendment 2026-10-13 above records.

  What a forged trigger yields in the MVP is an email and a dry-run record, never an action on a host (ADR-0021 §3). A writer to Postgres can also suppress a trigger, by placing an incident with the same grouping key first. The mitigations are those of the agent's identity (§1, §2) and of the stores' access (§8).
- **§5, Tampering**: playbooks are read from `INGEST_SOAR_PLAYBOOKS_DIR` at boot and validated against their schema and the action policy (ADR-0021 §2–§3). The MVP has no surface for changing them at run time. A playbook changed on disk takes effect at the next boot, and only if it passes the same validation; whoever can write that directory can change what runs, a host-level control.
- **§5, Repudiation**: one record per execution, claimed before its first step and completed once, with its steps in an unsigned hash chain (ADR-0021 §4). A Postgres trigger refuses any change to a completed record. Both hold against a writer that goes through the application, not against the owner of the database. In the dev stack, ingest and the api share one Postgres user, the image's superuser (`infra/dev/docker-compose.dev.yml:132`, `:200`).
- **§5, Information Disclosure**: the module uses two credentials, both from the environment: the ingest service's Postgres credential and SPEC-014's SMTP password. A step record holds no credential, and no event data beyond the incident's id, severity and title and its agent's id. The module logs step outcomes, never its configuration.
- **§5, Denial of Service**: at most 10 steps per playbook, 15 s per step and 30 s per run; one execution per playbook and incident, claimed before its first step; and no retry. The detection cycle awaits each run, so a slow relay, or a burst of matching incidents, delays detection by up to 30 s per run.
- **§2 cg-ingest, assets at risk**, add: the playbooks directory, and SPEC-014's SMTP password, which `notify.email` uses.
- **§3 cg-api, Information Disclosure**, add: the api reads execution records, scoped to the session's org like its other reads (ADR-0021 §4).
- **§5, Elevation of Privilege**: the MVP has no live semi-reversible or destructive action, and the loader refuses one at boot (ADR-0021 §3). That enforces in code that no destructive action runs without `human.approve`, which does not exist yet.
- **Threats explicitly accepted in MVP**, add:
  - **Containment is not performed.** A playbook records the isolation it would do, and an analyst acts. Accepted until a channel from the server to the agent exists, under its own ADR (ADR-0021 A4).
  - **SOAR delays detection.** The cycle awaits each run, by up to 30 s per run, when the relay is slow or matching incidents come in bursts. Accepted until a queue separates them (ADR-0021 A7).
  - **The database's owner can rewrite or delete an execution record.** Accepted until a separate role or an external anchor exists (ADR-0021 §Out of scope).
- **References**, add: ADR-0021.

**Effect on other sections.** §5's description through NATS and `cg-api` stays as the plan for a future bus. Its *Open questions* (vault integration) are unchanged. §8's per-service database users remain the plan, not the dev stack, as the Amendment 2026-10-13 records.

## Amendment 2026-10-15 (debt #55): the system as built

**Status.** Debt #55 (`docs/handoff-session-36.md`): this document describes the Phase-0 plan, not the system. Manuel delegated it on 2026-10-14 ("lo que este pendiente de mi decidelo tu en base al beneficio del proyecto, …") and again on 2026-10-15 ("todo lo que dependa de mi elige tu en beneficio del proyecto"). Claude Code wrote it, and decided, as `docs/handoff-session-38.md` records, that the new risks are listed as open, not accepted. It restates no section: each section's Phase-0 text stays as the plan, and this amendment says where the system as built at `main` differs, on 2026-10-15 and as each later session's changes keep it. The threat model remains `Accepted`.

**Context.** The MVP as built (roadmap §E done, §F next):

- `cg-agent`, Rust, run by hand as an elevated administrator (ADR-0010). It captures processes, network connections and logons, and keeps its events in memory only (ADR-0009).
- `cg-ingest`, TypeScript, one process: the enrollment listener over plain HTTP (SPEC-004), the agents' mTLS listener, the CA, detection (ADR-0012), incidents, the email notification (SPEC-014) and SOAR (ADR-0021).
- `cg-api`, TypeScript: human auth (SPEC-008), the read API (SPEC-009), and the forensic drill, export and report (SPEC-010, SPEC-012, SPEC-013) as a module.
- The dashboard, Next.js.
- Postgres, ClickHouse and Redis. NATS and MinIO run in the dev stack and nothing uses them. There is no `cg-pipeline`, `cg-ml` or `cg-forensic` service, no bus, no Loki.

**Where the system differs from the plan.** Each item names the section and what is built; the open risks it raises are numbered R1–R16 below.

- **§1 Agent.**
  - The buffer is an in-memory ring, nothing is written to disk (ADR-0009), so "locally buffered events (encrypted at rest)" and the DPAPI-derived buffer key do not apply.
  - Key custody: the private key is protected with DPAPI at machine scope, and its file's ACL grants the enrolling user and SYSTEM (`agent/cg-agent/src/secure_storage.rs`, `agent/cg-agent/src/identity.rs`). Any process of the enrolling user, unelevated, can read and decrypt it, not only LocalSystem. The files are written before their ACL is tightened, and keep the inherited one if tightening fails (R1).
  - The enrollment token is an opaque random value, single-use with a 15-minute time-to-live (`services/ingest/src/cli/issue-token.ts`), not a scope-bound JWT. It travels over the plain-HTTP enrollment listener, as SPEC-004 accepts, and is stored as typed (R4).
  - There is no installer and no code signing (roadmap §F). The agent runs as an elevated administrator started by hand, not a service account (ADR-0010).
  - The signature of each batch is verified and not stored, and the sequence number restarts with the process and is informational only, by ADR-0004's Amendment 2026-05-23, so an accepted event is not kept as evidence non-repudiable against the agent's key.
  - Resource limits are fixed constants (the ring, the batch); there is no CPU or memory cap.
  - Command lines and users (Amendment 2026-10-13) are not captured: roadmap §B2 is parked. That amendment describes ADR-0020's design, not what is built: `cges_events` has `process_command_line` and `subject_user_sid`, both empty, no `process_user_uid`, and no time-to-live on either. The `image_file_name` of a process can hold a user name inside its path, and has no time-to-live.
- **§2 cg-ingest.** No NATS: it writes ClickHouse, Postgres and Redis itself. Its event schemas are zod copies of the CGES shapes, not the JSON Schemas under `schemas/cges/`, and no fuzz test exists. They check an element's time and a process's creation time as decimal nanoseconds without a leading 0, below 2^63, and a process event's id, pids and exit code against their columns (SPEC-017 Amendment 2026-10-16). The nonce cache is built; the per-agent rate limits and the TLS-level connection caps are not, and the sequence-number defense was dropped by ADR-0004's Amendment 2026-05-23. The heartbeat table that records accepted envelopes is unsigned. The agent sets each event's `time`, with no bound against its arrival (R9). Ingest terminates the agents' TLS itself and reads the CN from its own socket, so the accepted trust in a reverse proxy that terminates mTLS (*Threats explicitly accepted in MVP*) does not apply as built. The same process runs detection, and holds the CA's private key and the SMTP credential that SPEC-014's notification and SOAR share (R6).
- **§3 cg-api.** SPEC-008 Amendment 2026-10-15 brings these mitigations in line with this section: strict schemas, org-scoped user routes, the audit row's actor, target and correlation id, query timeouts, account disable. The authority for strict schemas is this section's Tampering line itself, not ADR-0002 Rule 4, which makes TypeScript the default for user-facing layers and locks no framework (debt #84; the References entry for ADR-0002 repeats the miscitation). Still not as planned:
  - Sessions are opaque random tokens in Redis, not signed. One ends after 8 h without use, and 24 h after its login at the latest (SPEC-008's second Amendment 2026-10-16); neither is set per org (debt #20).
  - The login limit is fixed counters, 5 per account and 10 per address within 15 minutes of the last failure, with no progressive back-off; an attempt reserves its slot atomically before its credentials are checked, so attempts in flight count too. Rate limits exist only on the login, the TOTP confirmation and the password change (SPEC-008 Amendment 2026-10-16).
  - `audit_log` is append-only by the application only: no database grant or trigger enforces it. A request the role or CSRF check refuses writes an `access.denied` row; one refused for want of a session writes none, pending Manuel's reading of ADR-0014 (#80). The login, the TOTP confirmation and the password change write one for a failure. A read writes none, a refused one aside; an export or a report writes one row for each document produced (SPEC-009's third Amendment 2026-10-15).
  - `GET /v1/users` has no pagination.
  - Every role reads every incident (SPEC-009). Behind a proxy, the api reads a request's address from `X-Forwarded-For` only when its peer is one of the hops `API_TRUSTED_PROXIES` lists (SPEC-008's second Amendment 2026-10-15); with the variable unset it counts the proxy's address. The login deletes no session the caller already held (debt #102). There is no WebSocket and no external-client credential.
- **§4 cg-pipeline.** Detection is a module of `cg-ingest` that polls `cges_events` and writes alerts and incidents to Postgres. Its production driver loads its rules from `INGEST_DETECT_RULES_DIR` once, at its start (SPEC-016 Amendment 2026-10-16), so a change to that directory takes effect at the next start, which refuses a file that fails to load; whoever can write the directory and restart ingest changes detection (R8). There is no `dropped_events_total` metric and no Prometheus: the agent counts the events a full ring drops and logs other drops, its own connections' records excepted by design, and the server logs a refused batch. The org scope exists in the SQL, and the org is always `default` (a single-org MVP, SPEC-008 Amendment 2026-10-15 item 5).
- **§5 cg-soar.** As Amendment 2026-10-14 states. The SMTP send now authenticates only over TLS (SPEC-022 Amendment 2026-10-15). A message to a relay that asks no credentials still travels in clear text when the relay offers no TLS.
- **§6 cg-ml.** Not built. The engine refuses any detection source but `rule`.
- **§7 cg-forensic.** A module of `cg-api`, run on request; nothing is stored, and the PDF is unsigned. The export signs, with the forensic key, the evidence that ClickHouse returns at the request. Only an admin or an analyst may request an export or a report, and each document produced is recorded in `audit_log` with the events signed, the chain head and, for a report, its SHA-256; none is sent unrecorded (SPEC-009's third Amendment 2026-10-15). The export signs what the shared stores hold (R7). There is no maximum evidence count.
- **§8 Storage layer.** In the dev stack, ingest and the api share one credential per store: Postgres as the image's superuser, ClickHouse, and Redis (`infra/dev/docker-compose.dev.yml`). There are no per-service users with scoped grants (R2, R3), no TLS to the stores, no signed mirror of `audit_log`, and no retention in Postgres or ClickHouse but the 365 days of class 3002. The application encrypts three secrets with pgcrypto, passing the passphrase as a SQL parameter (R10). Assets not listed above, both personal data readable with the api's ClickHouse credential (R11): the logons of class 3002, which hold user names, domains, source addresses and host names; and the connections of class 4001, which hold source and destination addresses and ports by process, with no time-to-live (debt #47).
- **§9 Dashboard.** The login and the logout are route handlers on the dashboard's origin (SPEC-009's second Amendment 2026-10-15). Each refuses a request whose `Origin` is not the host it was addressed to, or whose `Sec-Fetch-Site` is not `same-origin`; with the cookies' `SameSite=Strict`, that check takes the role this section's Tampering line gives Fastify-issued CSRF tokens between the browser and the dashboard. The CSRF token is an `HttpOnly` cookie that the dashboard's server sends to the api. The cookies carry no `__Host-` prefix, so a sibling subdomain can set a `cgsess` the dashboard reads (debt #106). The dashboard enrolls no TOTP and manages no user (debt #83). No Content Security Policy or HSTS header is configured (debt #104). Its server pages forward the session cookie alone to the api, over HTTP by default (`API_BASE_URL`), which a login's password and TOTP code now cross too (R13). When the dashboard is listed in `API_TRUSTED_PROXIES` without a proxy in front of it that appends or sets `X-Forwarded-For`, a client chooses the address the api counts (R16). Admins set users' passwords and receive their TOTP secret (R14).
- **Cross-cutting.** The ingest and api containers run as root (R15). Ingest writes its TLS key file `0600` on Linux, where its container runs, since SPEC-004 Amendment 2026-10-15; a key file it wrote before keeps its mode until a reissue rewrites it (R15). The agents' certificates last 90 days with no renewal; the ingest server reissues its own self-issued certificate before it expires (SPEC-004 Amendment 2026-10-15); the CA's lasts 10 years (R5). Service logs are pino lines on standard output, not Loki. `.env.example` supplies the dev values and the compose file sets secrets inline. `dependency-audit.yml` runs `pnpm audit` over the production dependencies and `cargo audit` over `Cargo.lock` on each change to them and every week, and `docs/security/dependency-review.md` is the dependency runbook (debt #93, closed in Session 41); the development pnpm dependencies are not audited, and `cargo audit` lets its warnings pass (R12).
- **Threats explicitly accepted in MVP.** "Root-level local compromise of an endpoint defeats agent key custody" understates the exposure: the enrolling user's own processes suffice (R1), which is not accepted.

**New risks, open.** None is accepted. Each is a debt with a destination (`docs/handoff-session-39.md`; R16, `docs/handoff-session-40.md`; R12, `docs/handoff-session-40.md` and `docs/handoff-session-41.md`):

- **R1 — The agent's private key is readable by the enrolling user's unelevated processes** (debt #87). Destination: a SPEC-002 amendment, since its NFR-003 makes the key owner-only and the owner is the enrolling user, with the next agent change under the elevated gate.
- **R2 — A compromised ingest can forge an api session.** Api sessions are Redis records holding the role, and ingest, the component exposed to agents, uses the same Redis credential (debt #85). Destination: roadmap §F, a Redis user per service with key-pattern ACLs.
- **R3 — Both services act as the Postgres superuser**, so either can read or rewrite `users`, `audit_log`, `ca` and `forensic_key` (debt #86). Destination: roadmap §F, a Postgres role per service with the grants it needs.
- **R4 — Enrollment: a token in clear text on the wire and at rest, its org and scope ignored, no rate limit**, and the CA signs a certificate for every request before the token is checked (debt #89). Destination: roadmap §F (TLS on the enrollment listener) and a SPEC-004 amendment.
- **R5 — Certificates last 90 days, with no renewal and no revocation list.** An agent stops being accepted 90 days after its enrollment, and a stolen certificate with its key is cut off only by deleting its agent's row, which the enrollment token's reference to it blocks until the token row is cleared. The ingest server's own certificate, which this risk named until Session 40, is reissued before it expires (SPEC-004 Amendment 2026-10-15); an operator-provided one is the operator's to renew, and the server logs its approaching expiry (debt #88). Destination: the agents' renewal before roadmap §F's installation is called done.
- **R6 — One process holds the internet-facing listeners and the CA's private key**, so a parser defect in ingest reaches the key that issues agent identities (debt #99). Destination: if the CA is to leave ingest, an ADR, a component decision.
- **R7 — The forensic key signs what the shared credentials can write.** An admin or an analyst has the forensic key sign the events an incident's alerts name in Postgres and ClickHouse holds; whoever holds the shared Postgres superuser or the shared ClickHouse credential can rewrite them before an export, and the key then attests the rewritten evidence. Each signing is recorded (debt #90, closed by `8d3c61d`), but `audit_log` is itself writable by the same superuser (R3). Destination: roadmap §F, a Postgres role and a ClickHouse user per service (debts #86, #108).
- **R8 — Detection rules change at a restart, by whoever can write the rules directory** (debt #109; #91, closed by `3da53e3`, recorded their re-reading on every cycle). A file planted while ingest runs lies dormant until the next start, which it then refuses. Destination: roadmap §F's packaging, the rules read-only in the image.
- **R9 — The agent sets the events' time**, which drives ClickHouse partitions and incident windows (debt #31, `docs/handoff-session-33.md`). Destination: Manuel's decision on a bound against arrival, a lateness horizon that ADR-0012 and SPEC-018 decided against for the MVP (SPEC-017 Amendment 2026-10-16 checks the time's shape only).
- **R10 — Encryption passphrases travel as SQL parameters**, so a Postgres that logs statements with their parameters records them (debt #94). Destination: roadmap §F's deployment of Postgres logging.
- **R11 — Logon and connection telemetry is personal data readable with the api's credential**: logons kept 365 days, connections with no time-to-live (debts #95, #47). Destination: roadmap §F's per-service grants and retention, and the DPO's record of processing.
- **R12 — Not every dependency is audited.** The development pnpm dependencies carry advisories and are not audited in CI (debt #110). `cargo audit` lets its warnings pass, among them an unmaintained crate the agent reads its certificates with (debt #111). No tool checks the transitive dependencies' licences, and no SBOM is produced (debt #112). The workflows' actions are pinned by tag, not by commit (debt #113). Destination: each debt's.
- **R13 — The session token, and a login's password and TOTP code, cross the leg between the dashboard and the api in clear text** unless the deployment encrypts it: `API_BASE_URL` is plain HTTP by default (debt #97). The pages forward the session cookie alone since Session 40. Destination: roadmap §F.
- **R14 — Admins set users' passwords and receive their TOTP secret** (debt #98). Destination: a later SPEC-008 change, an invitation the user completes.
- **R15 — The containers run as root, and a TLS key file ingest wrote before SPEC-004 Amendment 2026-10-15 keeps the default file mode** until a reissue rewrites it `0600` (debt #92). Destination: roadmap §F.

- **R16 — Without a proxy in front of the dashboard, a client chooses the address the api counts.** When the dashboard is listed in `API_TRUSTED_PROXIES` and nothing in front of it appends or sets `X-Forwarded-For`, any client that reaches the dashboard can evade the per-IP login limit or spend a chosen address's attempts; the per-account limit is unaffected (debt #103). Destination: roadmap §F, the dashboard only behind a TLS proxy that appends or sets the header.

The dev stack also runs NATS without authentication and MinIO with the dev root credentials of the compose file, which nothing uses (debt #96). Destination: a later sweep, removing them until a SPEC uses them.

**Effect on other sections.** None is restated. The amendments of 2026-10-13 and 2026-10-14 stand, the first as ADR-0020's design (§1 above); their citations of `infra/dev/docker-compose.dev.yml:200` and `:207` now fall a few lines above the lines they meant (the Postgres URL is at `:203`, the ClickHouse user at `:210`), anchor drift for the citation sweep (debt #13).

## References

- [ADR-0001](../adr/0001-monorepo-layout.md) — Monorepo layout (defines components in scope)
- [ADR-0002](../adr/0002-language-per-component.md) — Language per component (Rust memory safety mitigates Elevation of Privilege in the agent; Zod schemas mitigate Tampering at the API)
- [ADR-0003](../adr/0003-polyglot-storage.md) — Polyglot storage (retention table for logs, MinIO immutability, Redis ephemeral with anti-replay nonces)
- [ADR-0004](../adr/0004-agent-server-protocol.md) — Agent-Server secure protocol (mitigates many agent and ingest threats)
- [ADR-0005](../adr/0005-detection-rules-and-ml-in-parallel.md) — Detection rules and ML in parallel (reversibility classes mitigate Tampering and Elevation via SOAR)
- [ADR-0015](../adr/0015-readonly-clickhouse-reader-in-api.md) — The api's ClickHouse reader, read-only in use (cited by the Amendment 2026-10-13)
- [ADR-0020](../adr/0020-process-command-line-and-user.md) — Process command line and user (the assets and mitigations of the Amendment 2026-10-13)
- [ADR-0021](../adr/0021-soar-mvp-playbook-executor.md) — SOAR for the MVP, in-process in cg-ingest (the trust boundary and mitigations of the Amendment 2026-10-14)
- [ADR-0009](../adr/0009-event-delivery-and-buffer.md), [ADR-0010](../adr/0010-agent-privilege-model-mvp.md), [ADR-0012](../adr/0012-normalize-before-correlate-pipeline.md) — the agent's in-memory buffer, its privilege model, the in-process detection (cited by the Amendment 2026-10-15)
- [Foundational Blueprint](../product/blueprint.md) — §7 (protocol), §10 (RBAC roles), §17 (non-goals)
