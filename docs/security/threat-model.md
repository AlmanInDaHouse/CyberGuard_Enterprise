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
- **Tampering:** CSRF, parameter pollution, mass assignment. Mitigated by Fastify-issued CSRF tokens on every mutation, by Zod strict schemas (no implicit type coercion, no extra fields), and by explicit allow-listed update fields per resource.
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

Build-dependency review is covered by a separate runbook, [dependency-review.md](dependency-review.md): `pnpm audit` for Node and `cargo audit` for Rust, in CI on each change to the dependencies and every week, and, on each change to them and every week, the licences of the production pnpm dependencies and the agent's runtime crates against an allowlist (`.github/scripts/check-licences.mjs`, debt #112). Go and Python, when either enters the repository, add `govulncheck` and `pip-audit`. This document does not duplicate dependency-CVE handling.

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
  - *A compromised api reads them, and more.* The api's ClickHouse reader is read-only in use (ADR-0015 §Compliance), and no ClickHouse grant enforces that or limits its columns: in the dev stack it logs in as the ingest's user (`infra/dev/docker-compose.dev.yml:98`, `:174`), which can also write and alter tables: with those credentials an attacker could copy the rows or remove the time-to-live. Not mitigated in the MVP; users with scoped grants are the deployment contract's (roadmap §F). No surface shows the fields yet; the first that does states its access rules in its SPEC (ADR-0020 §4).
- **Threats explicitly accepted in MVP**, add:
  - **A secret in a form the closed list does not name leaves the endpoint.** A positional password, or a token after a flag the list does not know, travels as typed and is kept for 365 days. The list is ratified with SPEC-021 and grows by amendment.
  - **An elevated process on the endpoint reads command lines from the kernel.** The kernel's process events belong to the endpoint, not to the agent; as with the root-level compromise above, an attacker elevated on the endpoint sees what the agent sees.

**Effect on other sections.** None is restated. §8's per-service database users with scoped privileges (Spoofing, Tampering) describe the plan, not the repository: the ingest service and the api share one ClickHouse user (`infra/dev/docker-compose.dev.yml:98`, `:174`), as the api entry above states. Its query-log filtering of PII fields is not configured yet: SPEC-021 and the deployment contract set it (ADR-0020 §4). Those statements, with the others of this document that later ADRs changed, wait for their own amendment (`docs/handoff-session-36.md`, debt #55).

## Amendment 2026-10-14 (ADR-0021): cg-soar as an in-process module of the MVP

**Status.** Co-located with ADR-0021; Accepted with it on 2026-10-14 (Manuel, in chat: "si a 1, 2 y  3"). It restates §5's trust boundary and mitigations for the MVP and adds to §2, §3, *Threats explicitly accepted in MVP* and *References*. The threat model remains `Accepted`.

**Context.** ADR-0021 runs SOAR as a TypeScript module inside `cg-ingest`. The detection cycle calls it after the commit of an incident it has just created; there is no bus and no separate service. Its actions are a closed list: `agent.isolate`, run in dry-run only, and `notify.email`, run live.

**Amendment.**

- **§5 cg-soar, trust boundary**, for the MVP: in-process in `cg-ingest`. It reaches two things outside the process: Postgres, where it writes its execution records with the ingest service's credential, and SPEC-014's SMTP relay, through `notify.email`.
- **§5, Spoofing**: the trigger is the detection cycle's own signal that it created an incident, so nothing arrives across a boundary as a trigger. A trigger can still be caused from outside, by making the cycle create an incident:
  - an enrolled agent can send events that match a High or Critical rule;
  - anyone holding the ClickHouse credential ingest uses can insert such events; in the dev stack the api shares it (`infra/dev/docker-compose.dev.yml:98`, `:174`), as the Amendment 2026-10-13 above records.

  What a forged trigger yields in the MVP is an email and a dry-run record, never an action on a host (ADR-0021 §3). A writer to Postgres can also suppress a trigger, by placing an incident with the same grouping key first. The mitigations are those of the agent's identity (§1, §2) and of the stores' access (§8).
- **§5, Tampering**: playbooks are read from `INGEST_SOAR_PLAYBOOKS_DIR` at boot and validated against their schema and the action policy (ADR-0021 §2–§3). The MVP has no surface for changing them at run time. A playbook changed on disk takes effect at the next boot, and only if it passes the same validation; whoever can write that directory can change what runs, a host-level control.
- **§5, Repudiation**: one record per execution, claimed before its first step and completed once, with its steps in an unsigned hash chain (ADR-0021 §4). A Postgres trigger refuses any change to a completed record. Both hold against a writer that goes through the application, not against the owner of the database. In the dev stack, ingest and the api share one Postgres user, the image's superuser (`infra/dev/docker-compose.dev.yml:96`, `:167`).
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

- `cg-agent`, Rust, run by hand as an elevated administrator, or as a Windows service that the service control manager starts as LocalSystem (ADR-0010; SPEC-024). It captures processes, network connections and logons, and keeps its events in memory only (ADR-0009).
- `cg-ingest`, TypeScript, one process: the enrollment listener over TLS (SPEC-023 §Operational §6; plain HTTP only with `INGEST_ENROLL_TLS=false`, which the test suites set), the agents' mTLS listener, the CA, detection (ADR-0012), incidents, the email notification (SPEC-014) and SOAR (ADR-0021).
- `cg-api`, TypeScript: human auth (SPEC-008), the read API (SPEC-009), and the forensic drill, export and report (SPEC-010, SPEC-012, SPEC-013) as a module.
- The dashboard, Next.js.
- Postgres, ClickHouse and Redis. NATS and MinIO are not run, the dev stack included, until a SPEC uses them (debt #96). There is no `cg-pipeline`, `cg-ml` or `cg-forensic` service, no bus, no Loki.

**Where the system differs from the plan.** Each item names the section and what is built; the open risks it raises are numbered R1–R16 below.

- **§1 Agent.**
  - The buffer is an in-memory ring, nothing is written to disk (ADR-0009), so "locally buffered events (encrypted at rest)" and the DPAPI-derived buffer key do not apply.
  - Key custody: the private key is protected with DPAPI at machine scope, and its file's ACL grants the enrolling user and SYSTEM (`agent/cg-agent/src/secure_storage.rs`, `agent/cg-agent/src/identity.rs`). Any process of the enrolling user, unelevated, can read and decrypt it, not only LocalSystem. The files are written before their ACL is tightened, and keep the inherited one if tightening fails (R1).
  - The enrollment token is an opaque random value, single-use with a 15-minute time-to-live (`services/ingest/src/cli/issue-token.ts`), not a scope-bound JWT. It travels over TLS 1.3, the agent trusting the roots of its trust anchor alone (SPEC-023 §Operational §6), and is stored as typed (R4).
  - There is no setup and no code signing (roadmap §F; SPEC-024's setup is not built, and no release carries install_ac_015's assets). The agent runs as an elevated administrator started by hand, or as a Windows service that the service control manager starts as LocalSystem with `--service` (SPEC-024 §Data contracts §3), not as a dedicated service account (ADR-0010 §Decision part 1). An MSI registers that service, with the security of SPEC-024's Amendment 2026-11-12, point 2; it admits an installation only with the property the setup passes, which an elevated administrator can pass too, or on an installed product (its point 1), and no setup or release installs it yet. The service refuses a data directory an ordinary user created first, and fails closed (R17, Amendment 2026-11-03).
  - The signature of each batch is verified and not stored, and the sequence number restarts with the process and is informational only, by ADR-0004's Amendment 2026-05-23, so an accepted event is not kept as evidence non-repudiable against the agent's key.
  - Resource limits are fixed constants (the ring, the batch); there is no CPU or memory cap.
  - Command lines and users (Amendment 2026-10-13) are not captured: roadmap §B2 is parked. That amendment describes ADR-0020's design, not what is built: `cges_events` has `process_command_line` and `subject_user_sid`, both empty, no `process_user_uid`, and no column time-to-live on either: they go with their row. The `image_file_name` of a process can hold a user name inside its path; a process event is deleted the period the operator sets after its arrival, 365 days by default (SPEC-023 §Operational §9).
- **§2 cg-ingest.** No NATS: it writes ClickHouse, Postgres and Redis itself. Its event schemas are zod copies of the CGES shapes, not the JSON Schemas under `schemas/cges/`, and no fuzz test exists. They check an element's time and a process's creation time as decimal nanoseconds without a leading 0, below 2^63, and a process event's id, pids and exit code against their columns (SPEC-017 Amendment 2026-10-16). The nonce cache is built; the per-agent rate limits and the TLS-level connection caps are not, and the sequence-number defense was dropped by ADR-0004's Amendment 2026-05-23. The heartbeat table that records accepted envelopes is unsigned. The agent sets each event's `time`, with no bound against its arrival (R9). Ingest terminates the agents' TLS itself and reads the CN from its own socket, so the accepted trust in a reverse proxy that terminates mTLS (*Threats explicitly accepted in MVP*) does not apply as built. The same process runs detection, and holds the CA's private key and the SMTP credential that SPEC-014's notification and SOAR share (R6).
- **§3 cg-api.** SPEC-008 Amendment 2026-10-15 brings these mitigations in line with this section: strict schemas, org-scoped user routes, the audit row's actor, target and correlation id, query timeouts, account disable. The authority for strict schemas is this section's Tampering line itself, not ADR-0002 Rule 4, which makes TypeScript the default for user-facing layers and locks no framework (debt #84, closed in Session 41). Still not as planned:
  - Sessions are opaque random tokens in Redis, not signed. One ends after 8 h without use, and 24 h after its login at the latest (SPEC-008's second Amendment 2026-10-16); neither is set per org (debt #20).
  - The login limit is fixed counters, 5 per account and 10 per address within 15 minutes of the last failure, with no progressive back-off; an attempt reserves its slot atomically before its credentials are checked, so attempts in flight count too. Rate limits exist only on the login, the TOTP confirmation and the password change (SPEC-008 Amendment 2026-10-16).
  - In the dev stack, `audit_log` is append-only by the application only, since the API uses the superuser. In the production bundle, `cg_api` has `INSERT` alone on it: database grants enforce append-only access for that runtime role (SPEC-023 §Operational §4). A request the role or CSRF check refuses writes an `access.denied` row; one refused for want of a session writes none: such a request presents no credential the api checks, so it is not an authentication event (SPEC-008 Amendment 2026-10-19, Manuel's decision, which closes #80). The login, the TOTP confirmation and the password change write one for a failure. A read writes none, a refused one aside; an export or a report writes one row for each document produced (SPEC-009's third Amendment 2026-10-15).
  - `GET /v1/users` has no pagination.
  - Every role reads every incident (SPEC-009). Behind a proxy, the api reads a request's address from `X-Forwarded-For` only when its peer is one of the hops `API_TRUSTED_PROXIES` lists (SPEC-008's second Amendment 2026-10-15); with the variable unset it counts the proxy's address. The login deletes no session the caller already held (debt #102). There is no WebSocket and no external-client credential.
- **§4 cg-pipeline.** Detection is a module of `cg-ingest` that polls `cges_events` and writes alerts and incidents to Postgres. Its production driver loads its rules from `INGEST_DETECT_RULES_DIR` once, at its start (SPEC-016 Amendment 2026-10-16), so a change to that directory takes effect at the next start, which refuses a file that fails to load. In ingest's image the directory is the image's, owned by root and not writable by its process (SPEC-023 §Data contracts §2): another rule set takes another image, or a compose override that mounts another directory (R8, closed). There is no `dropped_events_total` metric and no Prometheus: the agent counts the events a full ring drops and logs other drops, its own connections' records excepted by design, and the server logs a refused batch. The org scope exists in the SQL, and the org is always `default` (a single-org MVP, SPEC-008 Amendment 2026-10-15 item 5).
- **§5 cg-soar.** As Amendment 2026-10-14 states. The SMTP send now authenticates only over TLS (SPEC-022 Amendment 2026-10-15). A message to a relay that asks no credentials still travels in clear text when the relay offers no TLS. The send trusts, for the relay's certificate, Node's roots and those `NODE_EXTRA_CA_CERTS` adds, or, with `INGEST_SMTP_CA_FILE` set, that file's certificates instead of both; a send whose relay certificate fails verification records the verification's code, not `ESOCKET` (SPEC-023 §Operational §14).
- **§6 cg-ml.** Not built. The engine refuses any detection source but `rule`.
- **§7 cg-forensic.** A module of `cg-api`, run on request; nothing is stored, and the PDF is unsigned. The export signs, with the forensic key, the evidence that ClickHouse returns at the request. Only an admin or an analyst may request an export or a report, and each document produced is recorded in `audit_log` with the events signed, the chain head and, for a report, its SHA-256; none is sent unrecorded (SPEC-009's third Amendment 2026-10-15). The export signs what the shared stores hold (R7). There is no maximum evidence count.
- **§8 Storage layer.** In the production bundle each service has its own credential in each store (SPEC-023 §Operational §4): in Postgres an owner role that migrates and a runtime role granted table by table what its code uses, neither a superuser, the runtime roles taking no advisory lock; in ClickHouse `cg_ingest` (`SELECT` and `INSERT`) and `cg_api` (read-only, `cges_events` alone, rows of class 1007 alone); in Valkey `cg_ingest` (its nonces) and `cg_api` (its sessions, counters and replay keys), the `default` user off. The stores sit on an internal network with ingest, the api and their two migration one-shots alone, and neither runtime user of ClickHouse reads its system log tables. In the dev stack, ingest and the api share one credential per store: Postgres as the image's superuser, ClickHouse, and Redis (`infra/dev/docker-compose.dev.yml`). There is no TLS to the stores, no signed mirror of `audit_log`, and no retention in Postgres. ClickHouse deletes an event of class 1007, 4001 or 3002 the period the operator sets after its arrival (`INGEST_RETENTION_*`, 365 days by default; another class at the longest of the three), and a heartbeat the period the operator sets after its arrival, 30 days by default (SPEC-023 §Operational §9). The application encrypts three secrets with pgcrypto, passing the passphrase as a SQL parameter (R10). Assets not listed above, both personal data, which the api's ClickHouse credential does not read in the bundle (R11): the logons of class 3002, which hold user names, domains, source addresses and host names; and the connections of class 4001, which hold source and destination addresses and ports by process; both kept for their class's period.
- **§9 Dashboard.** The login and the logout are route handlers on the dashboard's origin (SPEC-009's second Amendment 2026-10-15). Each refuses a request whose `Origin` is not the host it was addressed to, or whose `Sec-Fetch-Site` is not `same-origin`; with the cookies' `SameSite=Strict`, that check takes the role this section's Tampering line gives Fastify-issued CSRF tokens between the browser and the dashboard. The CSRF token is an `HttpOnly` cookie that the dashboard's server sends to the api. The cookies are named `__Host-cgsess` and `__Host-cgcsrf`, so a sibling host cannot set one the dashboard reads; the dashboard reads no other name (SPEC-023 §Operational §7). The dashboard enrolls no TOTP and manages no user (debt #83). Every response of the dashboard's server carries a Content Security Policy whose `script-src` is `'self'`, the response's nonce, new per response, and `'strict-dynamic'`, with `X-Content-Type-Options: nosniff`, `Referrer-Policy: same-origin` and a `Permissions-Policy` (SPEC-023 §Operational §7); `next dev` sets no CSP. The inline scripts Next needs run only with the response's nonce, never under `'unsafe-inline'`: SPEC-023's reading of this section's "inline scripts … forbidden". Public-key pinning, which the Spoofing line names, is not done, browsers having dropped it. HSTS is the proxy's (SPEC-023 §Operational §5), which the production compose puts in front of the dashboard, the dashboard published on no host port (debt #104). Its server pages forward the session cookie alone to the api, over HTTP by default (`API_BASE_URL`), which a login's password and TOTP code now cross too (R13). When the dashboard is listed in `API_TRUSTED_PROXIES` without a proxy in front of it that appends or sets `X-Forwarded-For`, a client chooses the address the api counts (R16). Admins set users' passwords and receive their TOTP secret (R14).
- **Cross-cutting.** The product images run as non-root users, ingest's, the api's and the dashboard's as uid 1000 and the proxy's as uid 10001. What they run, the code under `/app` and the Caddyfile, the detection rules and the playbooks among them, is not writable by the process: owned by root, but for Caddy's binary, which its image gives uid 1001. The process owns the directories a volume is mounted on, ingest's `/certs` and the proxy's `/data` and `/config`, and, in the three Node images, its home directory; `/tmp` is every user's. The bundle's compose runs each product container read-only, with a tmpfs at `/tmp` (SPEC-023 §Data contracts §2, §Operational §1; R8, R15). Ingest writes its TLS key file `0600` on Linux, where its container runs, since SPEC-004 Amendment 2026-10-15, and at each start sets a key file it issued itself to `0600` when it is wider and its own (SPEC-023 §Operational §6, debt #92); a volume an ingest running as root wrote holds files the non-root ingest cannot narrow or replace, and is removed once (`infra/dev/README.md`; the production bundle has never run ingest as root); an operator-provided key file keeps the mode its operator gave it. The agents' certificates last 90 days with no renewal; the ingest server reissues its own self-issued certificate before it expires (SPEC-004 Amendment 2026-10-15); the CA's lasts 10 years (R5). Service logs are pino lines on standard output, not Loki. In the dev stack `.env.example` supplies the dev values and the compose file sets secrets inline; the production bundle's secrets are generated by `setup.sh` into `.env`, mode `0600`, and every one a container uses is required by the compose, which holds none (SPEC-023 §Operational §1, §3). `dependency-audit.yml` runs `pnpm audit` over the production dependencies and `cargo audit` over `Cargo.lock` on each change to them and every week, and `docs/security/dependency-review.md` is the dependency runbook (debt #93, closed in Session 41); the development pnpm dependencies fail it only on a critical advisory, and `cargo audit` lets its warnings pass (R12).
- **Threats explicitly accepted in MVP.** "Root-level local compromise of an endpoint defeats agent key custody" understates the exposure: the enrolling user's own processes suffice (R1), which is not accepted.

**New risks.** None is accepted. Each open one is a debt with a destination, and a closed one names what closed it (`docs/handoff-session-39.md`; R16, `docs/handoff-session-40.md`; R12, `docs/handoff-session-40.md` and `docs/handoff-session-41.md`):

- **R1 — The agent's private key is readable by the enrolling user's unelevated processes** (debt #87). Destination: roadmap §F's agent-installation SPEC (F2), whose service account decides the key's owner (ADR-0022, Accepted 2026-10-23).
- **R2 — A compromised ingest could forge an api session: closed** with debt #85, as ADR-0022's *Consequences* foresaw. In the bundle ingest's Valkey user reaches its nonces alone and the api's its sessions, counters and replay keys, the `default` user off (SPEC-023 §Operational §4, deploy_ac_006). The dev stack keeps one password.
- **R3 — Both services acted as the Postgres superuser: closed** with debt #86, as ADR-0022's *Consequences* foresaw. In the bundle each service connects as its runtime role, granted table by table what its code uses: ingest cannot read `users`, `forensic_key` or `audit_log`, nor replace `ca`; the api writes `audit_log` and `forensic_key` and changes neither (SPEC-023 §Operational §4, deploy_ac_004). The dev stack keeps the superuser.
- **R4 — Enrollment: a token in clear text at rest, its org and scope ignored, no rate limit**, and the CA signs a certificate for every request before the token is checked (debt #89). On the wire it travels over TLS since SPEC-023 §Operational §6, debt #89's TLS half. Destination: roadmap §F's renewal SPEC, with the rest of debt #89 (SPEC-023 §Out of scope).
- **R5 — Certificates last 90 days, with no renewal and no revocation list.** An agent stops being accepted 90 days after its enrollment, and a stolen certificate with its key is cut off only by deleting its agent's row, which the enrollment token's reference to it blocks until the token row is cleared. The ingest server's own certificate, which this risk named until Session 40, is reissued before it expires (SPEC-004 Amendment 2026-10-15); an operator-provided one is the operator's to renew, and the server logs its approaching expiry (debt #88). Destination: the agents' renewal before roadmap §F's installation is called done.
- **R6 — One process holds the internet-facing listeners and the CA's private key**, so a parser defect in ingest reaches the key that issues agent identities (debt #99). Destination: if the CA is to leave ingest, an ADR, a component decision.
- **R7 — The forensic key signs what the shared credentials can write.** An admin or an analyst has the forensic key sign the events an incident's alerts name in Postgres and ClickHouse holds; whoever holds a credential that writes them can rewrite them before an export, and the key then attests the rewritten evidence. Each signing is recorded (debt #90, closed by `8d3c61d`). In the bundle the api's credentials write no event and no alert, and to `audit_log` they only append (debts #86, #108); ingest's still insert the events, whose newest row for an id ClickHouse keeps, and write the alerts, so a compromised ingest can still rewrite evidence before an export (debt #124; destination: F4's pre-release evidence-integrity control proposal, delegated in Session 48; any required ADR still needs Manuel's approval).
- **R8 — Detection rules changed at a restart, by whoever could write the rules directory: closed** with debt #109 (#91, closed by `3da53e3`, recorded their re-reading on every cycle), as ADR-0022's *Consequences* foresaw. The rules and the playbooks are in ingest's image, owned by root and not writable by its process (SPEC-023 §Data contracts §2): another rule set takes another image, or a compose override that mounts another directory, both of them the host's Docker, which the host's own access control guards.
- **R9 — The agent sets the events' time**, which drives ClickHouse partitions and incident windows (debt #31, `docs/handoff-session-33.md`). Destination: a bound ahead of arrival, specified once a measurement shows what an honest agent's times do across a correction of its host's clock; no bound behind arrival in the MVP, which would be the lateness horizon ADR-0012 and SPEC-018 decided against (SPEC-017 Amendment 2026-10-19, Manuel's decision; its Amendment 2026-10-16 checks the time's shape only).
- **R10 — Encryption passphrases travel as SQL parameters**, so a Postgres that logs statements with their parameters records them (debt #94). The bundle's Postgres logs no statement's parameters (SPEC-023 §Operational §4, deploy_ac_005), unless a session raises `log_parameter_max_length_on_error` for itself, which only a compromised service or the superuser would (debt #125; destination: F4's pre-release database/logging control proposal, delegated in Session 48; any required ADR still needs Manuel's approval).
- **R11 — Logon and connection telemetry is personal data**: logons and connections kept for the period the operator sets, 365 days by default, since SPEC-023's retention (debt #95). In the bundle the api's ClickHouse user reads class 1007 alone (deploy_ac_007); ingest's reads every class. Destination: the DPO's record of processing (`docs/operations/`).
- **R12 — Not every dependency is audited.** The development pnpm dependencies fail CI only on a critical advisory, and carry three lesser ones, one of them high with no fixed release (debt #110). `cargo audit` lets its warnings pass; since Session 43 it prints none, the unmaintained crate the agent read its certificates with replaced by rustls's own PEM reader (debt #111). The production pnpm dependencies' and the agent's runtime crates' licences are checked against an allowlist; what an image ships beyond them is not, though each released image's index carries an SBOM attestation that lists the operating-system and Node packages installed in it, and each image carries its third-party notices (SPEC-023 Amendment 2026-10-26; debt #112). The release and deploy workflows pin their third-party actions by commit SHA; other workflows still use tags (debt #113). Destination: each debt's.
- **R13 — The session token, and a login's password and TOTP code, cross the leg between the dashboard and the api in clear text** unless the deployment encrypts it: `API_BASE_URL` is plain HTTP by default (debt #97). The pages forward the session cookie alone since Session 40. In the bundle that leg is the internal `app` network, which holds the dashboard and the api alone (ADR-0022 §3). Destination: a deployment across hosts, which ADR-0022 leaves out of the MVP.
- **R14 — Admins set users' passwords and receive their TOTP secret** (debt #98). Destination: roadmap §F's dashboard SPEC (F3), an invitation the user completes (ADR-0022, Accepted 2026-10-23).
- **R15 — The containers' privileges** (debt #92). The four product images run as non-root users (SPEC-023 §Data contracts §2), and a TLS key file ingest issued itself before SPEC-004 Amendment 2026-10-15 no longer keeps the default file mode: ingest sets it to `0600` at its next start (SPEC-023 §Operational §6) when the file is its own; one an ingest running as root wrote is in a volume removed once (`infra/dev/README.md`). The production compose runs them with a read-only root filesystem, every capability dropped and no new privileges (SPEC-023 §Operational §1, deploy_ac_001): closed with debt #92, as ADR-0022's *Consequences* foresaw.

- **R16 — Without a proxy in front of the dashboard, a client chose the address the api counts: closed** with debt #103, as ADR-0022's *Consequences* foresaw. In the bundle the dashboard is reached through the proxy alone, which sets `X-Forwarded-For` to its peer and trusts no client's header, and the api trusts the dashboard's fixed address alone (SPEC-023 §Operational §5, deploy_ac_008). A deployment that publishes the dashboard otherwise reopens it.

**Effect on other sections.** None is restated. The amendments of 2026-10-13 and 2026-10-14 stand, the first as ADR-0020's design (§1 above); their citations of the dev compose file's api Postgres URL and ClickHouse user had drifted a few lines, and debt #13's sweep corrected them (`infra/dev/docker-compose.dev.yml:167` and `:174`).

## Amendment 2026-11-03 (SPEC-024): the service, the installer's elevated code, the instance lock and endpoint dumps

**Status.** Debt #141 (`docs/debts.md:86`). It adds to §1, to *Threats explicitly accepted in MVP* and to *References*, and adds R17. It updates two lines of the Amendment 2026-10-15, which that amendment keeps current as the system changes (`:278`), and supersedes one clause of the Amendment 2026-10-13. Proposed by Claude Code on 2026-11-03 (debt #141), and ratified by Manuel on 2026-11-04, in chat: "doy mi si a los textos 141". The threat model remains `Accepted`.

**Context.** SPEC-024, roadmap §F's agent installation (F2), gives the agent a Windows service run as LocalSystem, and a setup bootstrapper with an MSI built by WiX v7 (SPEC-024 §Data contracts §3, `docs/specs/SPEC-024-agent-installer-service.md:144-152`; its Amendment 2026-11-01, point 1, SPEC-024:490-537). install_ac_009 gave the console and the service a per-host instance lock (its second Amendment 2026-11-01, item 1, SPEC-024:707-761), and install_ac_007 the service mode, `--service` (item 2, SPEC-024:762-769). No setup or MSI exists: the packaging, install_ac_015, has not started. §1 and the Amendment 2026-10-15 name neither the code the installer runs elevated, nor the lock, nor the service's data directory. SPEC-024's Amendment 2026-11-01, point 5, disposes for F2 of the dumps that the Amendment 2026-10-13 left to the deployment contract (SPEC-024:629-667). The service does not yet check the custody of the identity it reads, nor validate its installed-mode paths (debt #142, with #87; R1 covers the key's custody).

**Amendment.**

- **Amendment 2026-10-15, two lines updated in place.** Its *Context* line on `cg-agent` (`:282`) adds the service: "run by hand as an elevated administrator, or as a Windows service that the service control manager starts as LocalSystem (ADR-0010; SPEC-024)". Its *Where the system differs from the plan*, §1 Agent, line on the installer (`:294`) read "There is no installer and no code signing (roadmap §F). The agent runs as an elevated administrator started by hand, not a service account (ADR-0010)."; it now names the service, the package not yet built, and R17.
- **§1 Agent, Elevation of Privilege**, add: *the installer's third-party code runs elevated on the endpoint.* The setup and the MSI carry WiX binaries, Burn's engine and an extension's custom actions, which run elevated on the buyer's endpoints, unmodified (SPEC-024:513-516). To be mitigated by the packaging change, the first that builds a setup; none of the following exists yet:
  - WiX's tool and every extension the package uses are pinned in the repository at one exact release, with the .NET SDK that tool needs (SPEC-024:491-494);
  - each nuget package is restored by its pinned version and checked against its recorded content hash before use (SPEC-024:529-531);
  - the binaries the setup or the MSI carries enter the licence allowlist by name, under MS-RL, as a reviewed change (SPEC-024:521-526);
  - an advisory check of those pins joins `dependency-audit` (SPEC-024:531-532).

  Not mitigated: a WiX release compromised upstream at the pinned version. Its hash is recorded as published, and an advisory check finds only what has been published (accepted below).
- **§1, Denial of Service**, add:
  - *An elevated administrator or SYSTEM holds the instance lock.* Only an elevated administrator's token or SYSTEM can create the lock, and it lasts as long as the process that took it (SPEC-024:711-715). While a process of either holds it, the agent does not start: the console and the service exit with code 10 before they enroll, the service with its Application-log entry 1002 (SPEC-024:716-722). An ordinary user can neither take nor hold it. Not mitigated, and inside the administrator's trust: an elevated administrator can as well stop or delete the service, so the lock gives no capability an administrator lacks (ADR-0010's Amendment 2026-11-02, *Effect*).
  - *An ordinary user creates the service's data directory first.* `%ProgramData%` lets ordinary users create subdirectories. The service writes its log under `%ProgramData%\CyberGuard\agent\logs\` (SPEC-024 §Data contracts §2, SPEC-024:99), whose parents within `CyberGuard` must prohibit unprivileged creation, replacement, deletion and ACL or owner changes (SPEC-024:101-102). It refuses a directory on that path that is not owned by SYSTEM or Administrators, or whose DACL admits another principal: its file log cannot be opened, and it reports `STOPPED` with code 13 before `RUNNING` (SPEC-024:784-790), having written nothing into that directory. An ordinary user who creates `%ProgramData%\CyberGuard` before the agent is installed therefore keeps the service from starting. It fails closed: no privileged write lands where that user could redirect it. Open (R17).
- **§1, Information Disclosure.** The Amendment 2026-10-13's clause "whether a deployment turns those off is the deployment contract's (ADR-0020 Open questions 4)" (`:234`) is superseded, for F2, by SPEC-024's Amendment 2026-11-01, point 5 (SPEC-024:629-667). The product writes, collects and sends no crash or memory dump, and does not turn the host's off, a change to host policy that SPEC-024 §Operational §5 excludes (SPEC-024:666-667). The endpoint runbook of install_ac_016, not yet written, is to read, without changing anything, and to record with the installation evidence the kernel dump settings, the page file's and hibernation's, and Windows Error Reporting's `LocalDumps` settings, global and for `cg-agent.exe`, with its consent and upload policy (SPEC-024:645-651). It is to state that a dump of the agent's process can hold its decrypted private key and the events it holds, and a kernel dump any memory of the host, and that who reads such files and how long they are kept follow the host's policy, which it is to record (SPEC-024:655-658). Not mitigated by the product: a host whose policy keeps or sends dumps keeps or sends that memory, the private key and the events today, and the command lines and user SIDs once roadmap §B2 captures them. The runbook is to record that exposure; it will not prevent it. ADR-0020 Open question 4 stays open, reopened when a buyer asks the product to collect dumps (ADR-0020's Amendment 2026-11-03). The rest of that bullet stands.
- **New risk.**
  - **R17 — An ordinary user can keep the service from starting by creating `%ProgramData%\CyberGuard` before the agent is installed** (§1, Denial of Service, above). Debt #144. Destination: SPEC-024's package. It refuses an untrusted pre-existing data directory before it changes the host (§Operational §1, SPEC-024:158-161; install_ac_011, SPEC-024:343), and creates the protected directories before it writes any sensitive byte (§Operational §2, SPEC-024:187); a default uninstall keeps them, with their ACLs (SPEC-024:292-293). Once installed, the protected directory leaves an ordinary user nothing to create; before installation, a pre-created directory fails the installation, before it changes the host, instead of the service's start. Until the package exists, the installation runbook's remediation for code 13 applies (`docs/operations/install.md`): an administrator removes that directory, after keeping what it holds, and starts the service again. A host set up by the console runbook has the same refusal: its `New-Item` creates `C:\ProgramData\CyberGuard\agent` with the ACL it inherits from `%ProgramData%` (`docs/operations/install.md`), so the service started there reports code 13; moving such a host, whose identity lives in that directory, to the service is the explicit migration of install_ac_013 (SPEC-024:345), not that remediation.
- **Threats explicitly accepted in MVP**, add:
  - **A WiX release compromised upstream at its pinned version runs elevated on the endpoint.** The project ships WiX's published binaries, not a build of its source; the pin, the content hash and the advisory check detect a change after the release is pinned, not a release compromised before it.
- **References**, add: SPEC-024.

**Effect on other sections.** None is restated. §1's plan stays as the plan: its Elevation of Privilege (b), a dedicated service account, "not LocalSystem unless a specific sensor requires it" (`:52`), and its Tampering line, "Installer code-signing mitigates the first install" (`:48`), differ from what is built, as the updated line `:294` records; Authenticode stays ADR-0022 Open question 1 (SPEC-024:50-51). *Out of scope*'s "Supply-chain attacks against build dependencies" (`:34`) concerns what builds the product; the WiX binaries the setup carries also run on the endpoint, which is why §1 lists them. The Amendment 2026-10-13 stands but for the clause superseded above. The Amendment 2026-10-15 stands but for its two lines updated in place; R1 to R16 are unchanged. ADR-0010's Amendment 2026-11-02 gave the lock's availability surface to this document's entry for the installer's elevated code (`docs/adr/0010-agent-privilege-model-mvp.md:135`); it is the first entry under §1, Denial of Service, above.

## Amendment 2026-11-05 (SPEC-024): the legacy migration

**Status.** Proposed by Claude Code on 2026-11-05 with SPEC-024's Amendment 2026-11-05 (install_ac_013, debt #87), and ratified by Manuel on 2026-11-05, in chat, with it: "Sí, apruebo". It adds to §1 and to R1 and R17, and restates nothing. The threat model remains `Accepted`.

**Context.** SPEC-024's Amendment 2026-11-05 lets the service, as SYSTEM, move a host that the console runbook set up into SYSTEM custody when an elevated administrator or SYSTEM starts it with migration arguments: it closes the console installation's directories, checks and reads its files once, restricts them in place, records the request, builds a new data root with its final security from the bytes it verified, commits by renaming, and keeps or removes the old copies (SPEC-024 Amendment 2026-11-05, items 1 to 5). R1 names the enrolling user's processes; it does not name the other exposures a console-layout host has had.

**Amendment.**

- **§1 Agent, Tampering**, add: *the service's start arguments and the files it adopts.* Only the holders of the service's start right, SYSTEM and elevated Administrators, can pass the arguments, and they carry no secret: the copies' choice and the trust anchor's fingerprints, which the operator obtains independently (SPEC-024 Amendment 2026-11-05, item 1). A wrong fingerprint refuses the migration. The migration adopts only files owned by SYSTEM or Administrators, which an ordinary user's process cannot create there, but for the repair of item 2.2, which rests on an administrator's check that an owner is the account that set up or enrolled the agent; ownership does not prove authorship, so an ordinary user who, before the console agent's first run, moved into the data root files of Administrators' that it can write from elsewhere on the volume, and enrolled first with the token it could read, could have an identity of its own adopted, a residual the migration cannot detect (R1); the account that set up the agent, whose unelevated processes could read the console's enrollment token, could have enrolled an identity of its own into the console installation before its first run, which the migration cannot tell from the agent's (R1); it uses or deletes a staging or legacy directory only when SYSTEM owns it (items 2 and 4). Not mitigated, and inside the administrator's trust: an elevated administrator can give fingerprints of an anchor of his choosing, as he can write `server-ca.pem` itself. Not mitigated, and R1's: the enrolling account's processes, which could already read the key, can replace the identity files before the migration reads them, with an identity they resealed on this host; the migration cannot tell it from the one enrolled here (item 7).
- **§1 Agent, Denial of Service**, add to the entry on the data directory: an ordinary user can stop a migration with an entry it does not expect, created before the service closes the directories, and a process that holds the data root or one of its files open, or an opportunistic lock on one, with a handle opened before the service restricted it, can make each attempt fail until that process ends, the service cancelling any operation that waits on it after 30 seconds rather than hanging; an entry an ordinary user planted, or a link it made to a file of the installation from elsewhere on the volume, is refused with a repair an administrator applies (removing the entry; restricting the file and removing its other names), never followed, so no security change, rename or deletion of the migration lands outside `%ProgramData%\CyberGuard`; the migration refuses, the console installation in place and runnable by an elevated console agent, its directories and the files it reached restricted to SYSTEM and Administrators. A start without migration arguments after such a stop reports code 2 with item 6's reason, or, once the staging directory exists, code 5, which names the interrupted migration, where it reported 13; on such a start a name an ordinary user created under `%ProgramData%\CyberGuard` changes no outcome (SPEC-024 Amendment 2026-11-05, items 2, 4 and 6).
- **R1**, add: on a host the console runbook set up, the exposure is wider than the enrolling user's processes. Every local user could read the identity files until the console agent restricted them, and an account that created their names before the console agent wrote them kept, as their owner, the right to read them after, and `agent.toml`, with its enrollment token until the token was removed, until the migration; backups, shadow copies, crash dumps, the page file, antivirus quarantine and the console's command history may keep copies of the key or the token; a clone of the machine's disk unseals the same DPAPI-sealed key; a handle opened before the migration's restriction keeps the access it was granted, which for the enrolling account includes taking a copy's ownership, out of the service's reach, which then reports it at each start, and for an account that owned a directory includes changing its DACL, which is why item 2.2's repair is made after a restart, before that account signs in again, from another administrator's session. The migration restricts every product-owned copy it reaches and, with `remove`, deletes them; it cannot recall a copy already made, which its runbook discloses, and the remedy for a suspected compromise is a new enrollment and the removal of the old agent on the server, not the migration (SPEC-024 Amendment 2026-11-05, items 2, 5 and 7). R1 stays open until the migration and the package that writes the configuration and the anchor owned by SYSTEM are implemented (debt #87).
- **R17**, add: a console-layout host's route to the service is the migration, which keeps its identity, or, when the migration refuses that identity, a new installation and enrollment after an administrator keeps its files aside; the runbook's repair of code 13 removes no such directory before either (SPEC-024 Amendment 2026-11-05, item 6 and *Effect*).

**Effect on other sections.** None is restated. R1's destination and debt #87 are unchanged; R17's destination, the package, is unchanged.

## Amendment 2026-11-07 (SPEC-024): the migration's residuals

**Status.** Proposed by Claude Code on 2026-11-07 with SPEC-024's Amendment 2026-11-07 (second), after the fresh-context review of install_ac_013's implementation. It adds to §1 and restates nothing. Ratified by Manuel on 2026-11-07, in chat, with it: "Sí, apruebo". The threat model remains `Accepted`.

**Context.** SPEC-024's Amendment 2026-11-05 says that from the closing of the directories on, no ordinary user can create an entry in them. An entry of a file transaction created before the closing is not stopped by it.

**Amendment.**

- **§1 Agent, Denial of Service**, add to the entry on the data directory: an ordinary user who, before the service closed the directories, created an entry in a file transaction can afterwards commit it, or keep it pending. A pending named stream on `%ProgramData%\CyberGuard` or the data root can stop the closing (code 13), and a pending entry a later step (code 5), whose line says that a transaction holds it when Windows says so. A hard link the enrolling account creates in a transaction escapes the link checks while pending; the old copies' pass then reports that copy, restricted, as not reached. Before the first rename the service reads the names again, so a committed entry stops the migration with the repair of SPEC-024's Amendment 2026-11-05, item 2.2. One committed after that read stays under `%ProgramData%\CyberGuard`, or in the legacy directory, where `remove` then does not finish; the service neither reads nor uses it (SPEC-024 Amendment 2026-11-07, second, item 1). A named stream on those two directories is not read either (its item 2). A read-handle opportunistic lock on `%ProgramData%\CyberGuard` itself stops the closing with code 13, and a batch opportunistic lock on a file an ordinary user planted at the staging or legacy name delays a start by up to about 35 seconds for that name, which then does not count. The enrolling account can make a file too large for the migration, which refuses it (its item 3), as it can refuse the migration in other ways.

**Effect on other sections.** None is restated. The Denial of Service entry's "Once installed, the protected directory leaves an ordinary user nothing to create" is read with this limit on a host that was migrated. R1 and R17 and their destinations are unchanged.

## Amendment 2026-11-09 (SPEC-024): the setup's elevated code and the adoption

**Status.** Proposed by Claude Code on 2026-11-09 with SPEC-024's Amendment 2026-11-09, and ratified by Manuel on 2026-11-09, in chat, with it: "Sí, apruebo". It adds to §1 and to R17, and restates nothing. The threat model remains `Accepted`.

**Context.** SPEC-024's Amendment 2026-11-09 gives the setup its own elevated executable, `cg-setup`, which enables no privilege, stages the configuration and the trust anchor owned by Administrators in a directory only SYSTEM and Administrators can reach, and starts the service; the service, as SYSTEM, adopts those files when started with an argument the SCM does not keep.

**Amendment.**

- **§1 Agent, Tampering**, add: *the configuration the service adopts.* The staged files are created with their final DACL, SYSTEM and Administrators, in directories whose DACL admits those two alone, which the setup creates or has checked, and a filtered administrator's token has no access through the Administrators entry. An elevated administrator can replace them before adoption, as they can rewrite `agent.toml` afterwards: elevated administration is not a threat boundary (SPEC-024 §Data contracts §2), and ownership does not prove which administrator wrote a file. Adoption refuses a host that has an `identity\` name, so it never points an enrolled identity at new inputs.
- **§1 Agent, Information Disclosure**, add: *the token between staging and enrollment.* The token rests in one file the setup writes, `agent.toml.setup` and then `agent.toml`, readable by SYSTEM and elevated Administrators, from staging until the service removes it after enrollment; the bootstrap file holds it too, in the operator's custody until their procedure removes it, and a pending host's earlier `agent.toml` keeps its own token until the adoption deletes it. If the service never adopts it, it stays there until a later setup that passes its checks removes it, on any route but a migration's; a default uninstall leaves it, and the runbook discloses this. Backups and shadow copies may keep it. It is single-use. Burn's bundle variables, the MSI's properties, command lines, the environment and the setup's logs never carry it.
- **§1 Agent, Elevation of Privilege**, add to the entry on the installer's code: `cg-setup` is the project's own elevated code on the endpoint; it enables no privilege (ADR-0010 §Compliance), and its inputs come from an elevated administrator; on a host with a migration in progress or a console layout it reads no file under `<P>`, the service checking them.
- **R17**, add: on a fresh installation `cg-setup` creates `<P>` and the data root with their final DACL, or refuses a pre-existing object that fails the service's own check of them, before the MSI and the service; an ordinary user who creates `<P>` first still blocks the installation, which is this risk's denial of service.

**Effect on other sections.** None is restated. The clause of debt #87 that names "the configuration and anchor owned by SYSTEM" is met by the setup and the service together: the setup stages, and the service makes them SYSTEM's.

## Amendment 2026-11-11 (SPEC-024): a reversal that leaves the agent stopped

**Status.** Proposed by Claude Code on 2026-11-11 with SPEC-024's Amendment 2026-11-11 (second), after its fourth text review, and ratified by Manuel on 2026-11-11, in chat, with it: "Sí, apruebo". It adds to §1 and restates nothing. The threat model remains `Accepted`.

**Context.** SPEC-024's setup stops the product service in preflight. If the MSI fails, Burn runs the reversal, which starts the service again only from a record that passes its checks (SPEC-024's Amendment 2026-11-11, point 2, and its Amendment 2026-11-11 (second), point 1). A reversal that changes nothing, or that cannot read the record, starts nothing.

**Amendment.**

- **§1 Agent, Denial of Service**, add to the entry on the data directory: the ordinary user's levers that the Amendments 2026-11-05 and 2026-11-07 name, a read-handle opportunistic lock on `%ProgramData%\CyberGuard` and, on a host that was migrated, an entry of a file transaction or a handle opened before the migration's restriction, can also make a setup's reversal change nothing, or fail with 40 before any start, after a preflight that stopped the agent. The agent then stays stopped until a later setup on that host starts it, an administrator starts it, or, while its start type is automatic, Windows starts it at the next restart. The setup whose rollback it is has failed already, and the operator reads that failure; the stopped agent is not reported apart from it (SPEC-024's Amendment 2026-11-11 (second), *Effect*).

**Effect on other sections.** None is restated.

## Amendment 2026-11-12 (SPEC-024): the setup bundle

**Status.** Proposed by Claude Code on 2026-11-12 with SPEC-024's Amendment 2026-11-12 (second). Ratified by Manuel on 2026-11-12, in chat, with it: "apruebo". It adds to §1 and to the accepted threats, and restates nothing. The threat model remains `Accepted`.

**Context.** SPEC-024's setup is WiX's Burn engine and standard bootstrapper application, whose chain runs `cg-setup` preflight, the MSI and `cg-setup` configure (SPEC-024 Amendment 2026-11-09, item 1). The entry for the installer's elevated code (the Amendment 2026-11-03 (SPEC-024) above, §1 Elevation of Privilege) names Burn's engine among the WiX binaries that run elevated, with the pins that mitigate a change to them. It names neither what Burn does before it elevates, nor its package cache, nor its log. The probe pull request #123, closed without landing, read part of these on a hosted runner; the rest is Burn's source at `v7.0.1` (SPEC-024 Amendment 2026-11-12 (second)).

**Amendment.**

- **§1 Agent, Elevation of Privilege**, add to the entry on the installer's code: *the setup before its elevation.* Started without elevation, Burn copies its engine into the starting user's temporary folder and asks for elevation there, so that user's other processes, R1's actor among them, can reach what the administrator then elevates (Burn's source). The runbook's unattended route starts the setup from an elevated administrator's console or as SYSTEM, as a deployment tool does, so that Burn makes no such copy in a user's folder. Not mitigated, and disclosed by the runbook: the interactive route, which an administrator starts from their own desktop (accepted below). Burn's published advisories GHSA-7wh2-wxc7-9ph5, a library loaded from the user's temporary folder when the setup starts without elevation, and GHSA-rf39-3f98-xr7r, binaries staged in `C:\Windows\Temp` when it runs as SYSTEM, are fixed in WiX 4.0.4 and 4.0.5, before the pinned release; the reach of the user's processes to the copy, above, is not what they fixed.
- **§1 Agent, Elevation of Privilege**, add to the same entry: *the package cache.* Burn keeps the setup and its packages under `%ProgramData%\Package Cache`, which admits SYSTEM and Administrators with full access and ordinary users and Everyone to read and run (read), and runs `cg-setup` elevated from there, that folder its working directory. `cg-setup` is to load the system libraries it imports from the system directory only, which its build sets (SPEC-024 Amendment 2026-11-12 (second), point 10).
- **§1 Agent, Information Disclosure**, add to the entry on the token between staging and enrollment: what it says Burn's variables, command lines and the setup's logs never carry was read with a per-run token on a hosted runner, in Burn's and Windows Installer's logs, the temporary folders, the package cache, Windows Installer's cache, the registry's installation, uninstallation, `RunOnce`, service and current-user keys, and the latest entries of the Application, System and PowerShell logs; it was found in none. Burn's log records the bundle variables and command lines, which carry the bootstrap file's path, not its contents. Started as SYSTEM, the setup writes its log under `%SystemRoot%\SystemTemp`, which admits SYSTEM and Administrators alone (read on the runner's Windows; the floor's builds, which may lack that folder, are read on the isolated machine).
- **§1 Agent, Denial of Service**, add: *a cancelled or failed setup.* Closing the setup's window while it applies the chain cancels it, and Burn does not wait for a running `cg-setup`: a configure goes on after the setup exits, and a preflight can go on alongside its own reversal and leave the agent stopped, with or without its staged files, until a later setup starts it, an administrator does, or, while its start type is automatic, Windows does at the next restart. A failed upgrade runs the installed older setup again, which may stop and start the agent or refuse and leave both setups registered with the newer agent running (SPEC-024 Amendment 2026-11-12 (second), points 3 and 6). Each follows an elevated administrator's own action or a failure of the setup, with a code the setup reports; a failure may come from the ordinary user's levers the Amendment 2026-11-11 above names, and these cases add none.
- **Threats explicitly accepted in MVP**, add:
  - **The interactive setup's engine before elevation.** An administrator who starts the setup from their own desktop, without elevation, lets that desktop's other processes reach Burn's copy of its engine before the elevation prompt. Manuel chose on 2026-11-12 to disclose it, with the unattended route started elevated or as SYSTEM; this amendment's ratification is its acceptance.
  - **A vulnerability of the pinned Burn not yet published runs elevated on the endpoint.** As for a release compromised upstream, the advisory check finds only what has been published.

**Effect on other sections.** None is restated. R1 and its destination are unchanged; the interactive route's residual is accepted above, apart from R1.

## References

- [ADR-0001](../adr/0001-monorepo-layout.md) — Monorepo layout (defines components in scope)
- [ADR-0002](../adr/0002-language-per-component.md) — Language per component (Rust memory safety mitigates Elevation of Privilege in the agent; TypeScript, Fastify and Zod for the API in its language table)
- [ADR-0003](../adr/0003-polyglot-storage.md) — Polyglot storage (retention table for logs, MinIO immutability, Redis ephemeral with anti-replay nonces)
- [ADR-0004](../adr/0004-agent-server-protocol.md) — Agent-Server secure protocol (mitigates many agent and ingest threats)
- [ADR-0005](../adr/0005-detection-rules-and-ml-in-parallel.md) — Detection rules and ML in parallel (reversibility classes mitigate Tampering and Elevation via SOAR)
- [ADR-0015](../adr/0015-readonly-clickhouse-reader-in-api.md) — The api's ClickHouse reader, read-only in use (cited by the Amendment 2026-10-13)
- [ADR-0020](../adr/0020-process-command-line-and-user.md) — Process command line and user (the assets and mitigations of the Amendment 2026-10-13)
- [ADR-0021](../adr/0021-soar-mvp-playbook-executor.md) — SOAR for the MVP, in-process in cg-ingest (the trust boundary and mitigations of the Amendment 2026-10-14)
- [ADR-0009](../adr/0009-event-delivery-and-buffer.md), [ADR-0010](../adr/0010-agent-privilege-model-mvp.md), [ADR-0012](../adr/0012-normalize-before-correlate-pipeline.md) — the agent's in-memory buffer, its privilege model, the in-process detection (cited by the Amendment 2026-10-15)
- [SPEC-024](../specs/SPEC-024-agent-installer-service.md) — the agent's installer and service (cited by the Amendment 2026-11-03)
- [Foundational Blueprint](../product/blueprint.md) — §7 (protocol), §10 (RBAC roles), §17 (non-goals)
