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

Build-dependency review is covered by a separate runbook (path TBD). The cadence is per-language: `cargo audit` for Rust, `go mod tidy` plus `govulncheck` for Go, `npm audit` plus license review for Node, `pip-audit` for Python. This document does not duplicate dependency-CVE handling.

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

## References

- [ADR-0001](../adr/0001-monorepo-layout.md) — Monorepo layout (defines components in scope)
- [ADR-0002](../adr/0002-language-per-component.md) — Language per component (Rust memory safety mitigates Elevation of Privilege in the agent; Zod schemas mitigate Tampering at the API)
- [ADR-0003](../adr/0003-polyglot-storage.md) — Polyglot storage (retention table for logs, MinIO immutability, Redis ephemeral with anti-replay nonces)
- [ADR-0004](../adr/0004-agent-server-protocol.md) — Agent-Server secure protocol (mitigates many agent and ingest threats)
- [ADR-0005](../adr/0005-detection-rules-and-ml-in-parallel.md) — Detection rules and ML in parallel (reversibility classes mitigate Tampering and Elevation via SOAR)
- [ADR-0015](../adr/0015-readonly-clickhouse-reader-in-api.md) — The api's ClickHouse reader, read-only in use (cited by the Amendment 2026-10-13)
- [ADR-0020](../adr/0020-process-command-line-and-user.md) — Process command line and user (the assets and mitigations of the Amendment 2026-10-13)
- [Foundational Blueprint](../product/blueprint.md) — §7 (protocol), §10 (RBAC roles), §17 (non-goals)
