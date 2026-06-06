# CyberGuard Enterprise Architecture Blueprint

> Bootstrap document. Designed to serve as the foundation for Phase 0 of the project. Every reasoned decision here should become a formal ADR before any code is written.

---

## 1. Executive Summary

CyberGuard is a **self-deployable SOC/XDR platform** composed of two artifacts: a **central server** (ingestion, detection, correlation, cases, forensics, dashboard) and an **endpoint agent** (normalized telemetry, heartbeat, encrypted channel, future controlled response).

**Three non-negotiable principles:**

1. **Rules and ML models detect in parallel; both can generate alerts autonomously. Automatic actions scale by reversibility.** The AI never makes destructive decisions on its own.
2. **Spec-Driven + Harness from day one.** No detection enters `main` without a simulated scenario that validates it.
3. **Traceability via ADRs.** Every architectural decision has a short document justifying the "why", not just the "what".

**Honest scope statement:** Wazuh + Velociraptor + TheHive + Sentinel + XSIAM + Falcon represent, together, more than a decade of R&D from teams of hundreds of people. CyberGuard must not attempt to match them: it must be a **focused, opinionated, demonstrable platform** that covers the end-to-end flow (agent → ingest → detect → alert → case → forensic report) with enterprise-grade quality in a reduced but realistic subset.

**Summary technical thesis:**

- Agent in **Rust** (Windows first).
- Ingestion and correlation pipeline in **Go**.
- API/BFF + dashboard in **TypeScript** (Fastify + Next.js).
- Local ML/AI as an **isolated Python microservice**.
- Polyglot storage: **PostgreSQL+pgvector / ClickHouse / Redis / MinIO / NATS JetStream**. **No** OpenSearch, **no** Kafka, **no** Qdrant in MVP.

---

## 2. Product Vision

**Who it is for:** SMBs and mid-market organizations that cannot afford commercial XDR but need real visibility over endpoints and network, with exportable forensic capability and incident traceability.

**Product promise:** "Deploy a functional, self-hosted, auditable SOC in under 30 minutes, with real detections on day one and an exportable forensic report on the first incident."

**Differentiators against open-source competition:**

| Existing product | Gap CyberGuard targets |
|---|---|
| Wazuh | Poor SOC UX, no real SOAR, weak forensics |
| TheHive | No agent or detection, only case management |
| Velociraptor | Powerful forensics but no SIEM or UEBA |
| ELK/Wazuh stack | Heavy operation, hard to deploy |

**What CyberGuard is NOT** (worth tattooing): it is not antivirus, not a firewall, not DLP, not CSPM, not CASB. It is an **XDR + SIEM + Case Management + lightweight Forensics**; anything else is integrated as a source or destination.

---

## 3. System Architecture

### Logical view (layers)

```
┌─────────────────────────────────────────────────────────────────┐
│                        Dashboard (Next.js)                       │
└─────────────────────────────────────────────────────────────────┘
                                │
                          REST + WS (JWT)
                                │
┌─────────────────────────────────────────────────────────────────┐
│           API Gateway / BFF (TypeScript - Fastify)              │
│   AuthN/AuthZ, RBAC, OTP/MFA, audit log, rate limiting          │
└─────────────────────────────────────────────────────────────────┘
                                │
        ┌───────────────────────┼───────────────────────────┐
        ▼                       ▼                           ▼
┌───────────────┐     ┌──────────────────┐      ┌────────────────────┐
│ Case/SOAR svc │     │ Ingest svc (Go)  │      │ ML svc (Python)    │
│   (Go)        │     │  mTLS agent API  │      │ Embeddings, RAG,   │
│               │     │                  │      │ anomaly, summary   │
└───────────────┘     └──────────────────┘      └────────────────────┘
        │                       │                           │
        │                       ▼                           │
        │            ┌──────────────────────┐               │
        │            │  NATS JetStream      │               │
        │            │  events.raw          │◀──────────────┘
        │            │  events.normalized   │
        │            │  alerts              │
        │            │  incidents           │
        │            └──────────────────────┘
        │                       │
        │                       ▼
        │      ┌──────────────────────────────┐
        │      │ Pipeline workers (Go)         │
        │      │ normalize → enrich →          │
        │      │ correlate → score → emit      │
        │      └──────────────────────────────┘
        │                       │
        ▼                       ▼
┌──────────────────────────────────────────────────────────────┐
│    PostgreSQL+pgvector   ClickHouse    Redis    MinIO        │
│    (state, RBAC, cases,  (events,      (cache,  (artifacts,  │
│     embeddings)           UEBA agg)     sessions) reports)   │
└──────────────────────────────────────────────────────────────┘
                                ▲
                                │ mTLS + HMAC + nonce
                                │
┌──────────────────────────────────────────────────────────────┐
│              CyberGuard Agent (Rust) — endpoints              │
└──────────────────────────────────────────────────────────────┘
```

### Independent services

| Service | Responsibility | Language | Stateful? |
|---|---|---|---|
| `cg-agent` | Endpoint telemetry | Rust | No |
| `cg-ingest` | mTLS endpoint, validation, write to NATS | Go | No |
| `cg-pipeline` | normalize/enrich/correlate workers | Go | No |
| `cg-api` | BFF, REST, WS, RBAC, audit | TypeScript | No |
| `cg-soar` | Playbook executor | Go | Yes (execution state) |
| `cg-ml` | Embeddings, RAG, anomalies, summary | Python | No |
| `cg-forensic` | Timeline and report generation | Go (Python optional for PDF render) | No |

**Principle:** every service is stateless or nearly so; state lives in Postgres/ClickHouse/Redis/MinIO. This allows each to scale horizontally independently.

---

## 4. Recommended Stack

| Layer | Technology | Why |
|---|---|---|
| Agent | **Rust** + `tokio` + `rustls` | Minimal footprint, memory safety, single binary, no runtime |
| Ingest + Pipeline | **Go** + `nats.go` + `clickhouse-go` | Best productivity/performance ratio for I/O and concurrency. Rust here is overkill |
| API/BFF | **TypeScript** + **Fastify** + **Zod** | Dashboard development speed, schema validation unified with the front |
| ML/Local AI | **Python 3.12** + **FastAPI** + **llama.cpp** or **vLLM** + **sentence-transformers** | Only environment where local AI is reasonable; isolated so it doesn't contaminate the rest |
| Dashboard | **Next.js 15** (App Router) + **TailwindCSS** + **shadcn/ui** + **TanStack Query** + **Recharts** | SSR for heavy views, mature ecosystem |
| Messaging | **NATS JetStream** | Simpler than Kafka, persistent, enough for 100k events/s |
| Local orchestration | **Docker Compose** (dev/MVP) → **Kubernetes** (production) | Don't jump to K8s before its time |
| Future IaC | **Terraform** + **Helm** | When the enterprise phase exists |
| Auth | Own **OAuth2/OIDC** + **TOTP** (RFC 6238) | OTP/MFA with `otplib`/`pyotp`. WebAuthn as upgrade |
| Email | **Gmail API** with OAuth2 service account or SMTP relay | Start with SMTP, migrate to API if needed |
| Observability | **OpenTelemetry** + **Prometheus** + **Grafana** + **Loki** | De facto standard |

**Corrections to your initial proposal:**

1. **Don't mix TypeScript/Fastify *or* Python/FastAPI for the API.** Choose TS for BFF and leave Python *exclusively* for ML. Mixing languages on the critical API path multiplies bugs and integration tests by two.
2. **Heavy pipeline → Go, not Rust.** Rust in the server pipeline is a time sink. Go gives you 90% of the performance with 30% of the effort, and the client ecosystem (NATS, ClickHouse, Postgres) is first-class.
3. **Dashboard → Next.js, not plain Vite.** SOC views need SSR for long lists, export, and robust authentication.

---

## 5. Rust Decision

**Is Rust worth it? Yes, but only where it pays off.**

### Where yes

- **Endpoint agent.** Three objective reasons:
  1. **Footprint.** An agent must consume <50 MB RAM and <2% sustained CPU. Rust delivers that without GC pauses.
  2. **Security.** An agent with telemetry and future response actions is attack surface. Memory safety by construction is a sales and audit argument.
  3. **Distribution.** Single binary, no runtime, no system dependencies. Essential for self-deployment.

### Where no

- **Server.** Go covers the needs. Rust in SIEM servers you'll see in giants (Datadog, Cloudflare) at scales that justify the development cost. CyberGuard doesn't need that cost yet.
- **API/BFF.** TypeScript is the natural frontend language; reusing types between front and BFF is a real advantage.
- **ML.** The Python ecosystem is irreplaceable here.

### Honest cost of choosing Rust for the agent

- Real learning curve: 2–3 months to reach decent production quality if you have no prior experience.
- Slow compilation; you need build cache (sccache) from the start.
- Cross-platform (Windows/Linux/macOS) in Rust requires care with native APIs (`windows-rs`, `nix`, `core-foundation`).

**Verdict:** Rust for the agent is justified and is the project's "single technical bet". Everything else, pragmatic languages.

> **ADR-0001** *Languages per component* — record this decision with alternatives evaluated and consequences.

---

## 6. Storage Architecture

### Decision by workload

| Workload | Technology | Justification |
|---|---|---|
| Relational state (users, agents, cases, playbooks, RBAC) | **PostgreSQL 16** | Transactions, FKs, JSONB, `LISTEN/NOTIFY` for live events |
| Embeddings (forensic RAG, incident similarity) | **pgvector** on the same Postgres | One less DB to operate. Sufficient up to ~1M vectors |
| Raw and normalized events, UEBA aggregations | **ClickHouse** | Brutal compression, second-scale aggregations over billions of rows |
| Cache, sessions, rate limit, SOAR locks | **Redis 7** | The usual, no surprises |
| Forensic artifacts, dumps, PDF/HTML reports, archived events | **MinIO** (S3-compatible) | Standard object storage, self-hostable |
| Event bus | **NATS JetStream** | Persistent, replay, hierarchical subjects, much simpler than Kafka |

### What stays out of MVP (and why)

- **OpenSearch:** ClickHouse + `tokenbf_v1` index on text fields covers 80% of the use case. OpenSearch adds operations, RAM and another database to maintain. **Defer to enterprise phase** if real need for advanced full-text search appears.
- **Kafka/Redpanda:** NATS JetStream amply reaches 50–100k events/s. Migrate to Redpanda when justified. Starting with Kafka is planning for a scale you probably won't reach.
- **Qdrant:** pgvector up to 1M vectors. When exceeded, migrate.

### Retention model (initial proposal)

| Data | Storage | Hot | Warm | Cold |
|---|---|---|---|---|
| Raw events | NATS → ClickHouse → MinIO | 7 d ClickHouse | 30 d ClickHouse (compressed) | 365 d MinIO |
| Normalized events | ClickHouse | 30 d | 180 d | 365 d MinIO |
| Alerts | ClickHouse + Postgres | always | always | — |
| Cases/incidents | Postgres | always | always | — |
| Forensic artifacts | MinIO | 90 d | 365 d | archive |
| Audit log | Postgres (append-only) + signed MinIO mirror | 1 year | 7 years | immutable |

> **ADR-0002** *Polyglot storage* — record why each choice and the migration thresholds (e.g. "migrate to OpenSearch if query latency p95 > 5s on free search for 7 days").

---

## 7. Agent-Server Secure Protocol

### Threat model (summary)

- Attacker with endpoint control (can read disk, memory, agent keys).
- MITM attacker on the corporate network.
- Attacker with stolen credentials (must be detectable by anomaly).
- Compromised server: out of technical scope (operational controls).

### Enrollment

1. **Operator generates an enrollment token** from the dashboard: signed JWT, single-use, TTL 15 min, scope `enroll`, bound to `org_id`.
2. **Agent starts with the token and server URL** (installer passes these parameters).
3. **Agent generates Ed25519 keypair locally** and sends CSR + token to `/v1/agents/enroll`.
4. **Server validates token, issues client certificate** signed by CyberGuard's internal CA, with CN = `agent_id` (UUIDv7) and SAN = `org_id`.
5. **Agent stores private key** in OS-protected storage (Windows DPAPI / Linux keyring / macOS Keychain).

> **Decision:** the private key **never** leaves the endpoint. If the endpoint is compromised, the `agent_id` is revoked from the dashboard.

### Communication in operation

- **Transport:** mTLS 1.3 mandatory. Internal CA certificate pinning.
- **Message signing:** every event batch includes:
  - `agent_id`
  - `sequence_number` (monotonic, persistent on the agent)
  - `timestamp` (UTC, with ±5 min server-side tolerance)
  - `nonce` (UUIDv4, stored in server Redis with TTL 10 min for anti-replay)
  - `signature` = Ed25519 over SHA-256 hash of payload + previous headers
- **Server-side:** rejects if `sequence_number <= last_seen`, if `nonce` already seen, if `timestamp` outside window, or if signature invalid.

### Rotation and revocation

- Client certificates with **90-day** TTL, automatic rotation starting day 75.
- Instant revocation by `agent_id` (Redis list checked on every connection).
- Internal CA rotation annually.

### Degraded mode

- If agent cannot connect: **local encrypted buffer** (key derived from DPAPI/keychain) up to N MB (config, default 200 MB) and M hours.
- On connection recovery, drains in order with exponential backoff.
- Heartbeat every 30s; agent considered "offline" if >3 consecutive heartbeats missed.

### Versioning

- Mandatory `X-CG-Agent-Version` header. Server rejects versions below configured `min_supported_version`.

> **ADR-0003** *Agent–Server Protocol* — include sequence diagrams and mitigated-attacks matrix.

---

## 8. Common Event Schema

### Principles

1. **Alignment with OCSF v1.3** (Open Cybersecurity Schema Framework). Don't reinvent; OCSF is the emerging standard backed by AWS, Splunk, IBM, CrowdStrike. Huge technical credibility.
2. Preserve the original **raw** event alongside the normalized one (`raw` field) for forensics and reprocessing.
3. MITRE ATT&CK mapping as a first-class citizen.

### CGES (CyberGuard Common Event Schema) skeleton

```json
{
  "$schema": "https://cyberguard.io/schemas/cges/v0.1.json",
  "schema_version": "0.1.0",
  "event_id": "01J9X3K4M7N8P9Q0R1S2T3U4V5",
  "ingested_at": "2026-05-19T10:23:11.482Z",
  "occurred_at": "2026-05-19T10:23:10.901Z",
  "category": "process",
  "type": "process.start",
  "severity_initial": 0,

  "agent": {
    "id": "ag_01J9...",
    "version": "0.4.2",
    "platform": "windows",
    "hostname": "FIN-PC-014"
  },

  "org": {
    "id": "org_acme",
    "tenant": "default"
  },

  "host": {
    "os": "Windows 11 23H2",
    "arch": "x86_64",
    "ip": ["10.0.4.18"],
    "fqdn": "fin-pc-014.acme.local",
    "domain": "acme.local"
  },

  "user": {
    "name": "j.lopez",
    "sid": "S-1-5-21-...",
    "is_admin": false,
    "session_id": "sess_8821"
  },

  "process": {
    "pid": 7144,
    "ppid": 4012,
    "name": "powershell.exe",
    "path": "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
    "cmdline": "powershell -enc JABzAD0...",
    "hash": {
      "sha256": "a7b3...",
      "md5": "1d2e..."
    },
    "signed": true,
    "signer": "Microsoft Windows"
  },

  "parent_process": {
    "pid": 4012,
    "name": "winword.exe"
  },

  "network": null,
  "file": null,
  "auth": null,
  "dns": null,

  "enrichment": {
    "geo": null,
    "threat_intel": {
      "matched": false,
      "feeds": []
    },
    "asset": {
      "criticality": "high",
      "owner": "finance"
    }
  },

  "mitre": {
    "tactics": ["execution", "defense-evasion"],
    "techniques": ["T1059.001", "T1027"]
  },

  "score": {
    "heuristic": 0.0,
    "ueba": 0.0,
    "ml": 0.0,
    "final": 0.0
  },

  "labels": [],
  "raw": {
    "source": "etw.process",
    "payload": { "...": "..." }
  }
}
```

### Subschemas by category

`endpoint.process`, `endpoint.file`, `endpoint.registry`, `network.connection`, `network.dns`, `auth.login`, `auth.logout`, `auth.privilege_change`, `alert`, `incident`, `forensic.artifact`.

### Rules

- **IDs:** ULID or UUIDv7 for `event_id`. Time-orderable, collision-free.
- **Timestamps:** ISO 8601 UTC with milliseconds. Always two: `occurred_at` (when it happened) and `ingested_at` (when the server saw it).
- **Backward compatibility:** semver `schema_version`; breaking changes bump major and force offline migration in ClickHouse.
- **Validation:** JSON Schema generated from Rust+Go+TS types to avoid drift (`typeshare` or `quicktype`).

> **ADR-0004** *Adoption of OCSF as CGES base* — record what is inherited, what is extended and why pure OCSF is not used.

---

## 9. SIEM/XDR Pipeline

```
collect ─► normalize ─► enrich ─► correlate ─► score ─► alert ─► incident ─► case ─► playbook ─► report
```

### Phase by phase

| Phase | What it does | Input | Output | Tech | Test |
|---|---|---|---|---|---|
| **collect** | Receives signed batches, validates mTLS/HMAC, writes to `events.raw` | HTTP/2 mTLS from agent | NATS `events.raw.{org}.{agent}` | Go (cg-ingest) | Synthetic load: 10k events/s without loss |
| **normalize** | Transforms `raw` → CGES, maps vendor-specific fields | NATS `events.raw.*` | NATS `events.normalized.*` | Stateless Go workers | Snapshot tests per source |
| **enrich** | GeoIP, threat intel (IOCs), asset criticality, user role | `events.normalized.*` | `events.enriched.*` | Go + Redis cache | Feed mocks |
| **correlate** | Applies rules (Sigma-compatible) and stateful correlations | `events.enriched.*` | `alerts.*` | Go (rule engine) | Harness with synthetic events |
| **score** | Heuristic + UEBA + ML combine final score | `alerts.*` | `alerts.scored.*` | Go invokes `cg-ml` via gRPC | Expected score table |
| **alert** | Persists, deduplicates (5-min sliding window per `dedup_key`) | `alerts.scored.*` | Postgres `alerts` + WS push | Go | Dedup test |
| **incident** | Groups related alerts (same host/user/MITRE tactic in window) | `alerts` | Postgres `incidents` | Go | Grouping test |
| **case** | Human work: notes, evidence, assignment | UI/API | Postgres `cases` | TS API | E2E tests |
| **playbook** | SOAR executes declarative actions | Trigger from alert/incident/manual | Audit log + side effects | Go (cg-soar) | Dry-run and replay |
| **report** | Generates forensic report per incident | `incidents` + `events` + `artifacts` | PDF/HTML/JSON | Go + headless Chromium or WeasyPrint | Golden file tests |

### Detection rules

- **Sigma format** (Florian Roth's) as source-of-truth. Sigma → Go rule engine converter.
- `cg-rules/` repository with tests per rule: each `.yml` rule has a sibling `.test.json` with events that must match and events that must not.
- **CI blocks merge if a new rule has no test.**

### Scaling

- NATS JetStream with N consumers per subject (load balancing).
- ClickHouse partitioned by day, `ORDER BY (org_id, occurred_at, event_id)`.
- For >100k eps: consider org sharding. Not before.

> **ADR-0005** *Normalization before correlation* — rules must not depend on vendor-specific formats.

---

## 10. SOAR Engine

### Declarative model (YAML)

```yaml
# playbooks/isolate_endpoint.yml
id: pb_isolate_endpoint
version: 1
name: "Isolate endpoint on high-severity malware"
trigger:
  on: incident.created
  when:
    - incident.severity >= 8
    - "malware" in incident.tags
inputs:
  required: [host_id]
steps:
  - id: notify
    action: email.send
    params:
      to: "{{ org.soc_email }}"
      template: "incident_isolation_pending"

  - id: approval
    action: human.approve
    params:
      message: "Isolate host {{ inputs.host_id }}?"
      timeout: 15m
      required_role: ["analyst_l2", "soc_manager"]

  - id: isolate
    action: agent.isolate
    when: "{{ approval.granted }}"
    params:
      host_id: "{{ inputs.host_id }}"
    dry_run_supported: true
    rollback:
      action: agent.unisolate
      params:
        host_id: "{{ inputs.host_id }}"

  - id: tag_incident
    action: incident.tag
    params:
      tags: ["isolated", "auto-soar"]
```

### Guarantees

| Guarantee | How |
|---|---|
| **Dry-run** | Each action declares `dry_run_supported: true` and runs without side effects, logging what it *would* do |
| **Human approval** | Destructive actions (`agent.isolate`, `process.kill`, `firewall.block`) require `human.approve` before |
| **Rollback** | Each step can declare a `rollback:` with inverse action |
| **Audit** | Every execution generates an immutable entry in `audit_log` with who, what, when, result |
| **Idempotency** | `execution_id` UUID, safe retries |
| **Permissions** | RBAC: only authorized roles can manually trigger playbooks |

### MVP actions

`email.send`, `incident.tag`, `incident.assign`, `alert.acknowledge`, `human.approve`, `agent.isolate` (placeholder, real action in phase 3+), `webhook.post`.

> **What does NOT go in MVP:** actions that touch third-party firewall/AD/EDR. Too much integration surface. Enterprise phase.

---

## 11. UEBA Strategy

### Baselined entities

- **User:** typical login schedule, typical geo, usual hosts, event volume.
- **Host:** typical processes by hour, usual outbound connections, typical users.
- **Process:** usual parents, typical command lines (n-grams), frequency.

### Metrics and algorithms

| Anomaly | Algorithm | Computation location |
|---|---|---|
| Off-hours login | 24h hourly distribution + z-score | ClickHouse scheduled query |
| Login from new geo | Set membership + GeoIP | Ingest enrichment |
| Anomalous event volume | EWMA + 3σ | ClickHouse materialized view |
| Rare process on host | Set of processes seen in 30 days | ClickHouse |
| Rare parent-child combo | Isolation Forest on categorical features | `cg-ml` |
| Rare cmdline | Embeddings + cosine distance to host cluster | `cg-ml` + pgvector |

### Combined score principle

```
final_score = w1 * heuristic + w2 * ueba + w3 * ml
            where w1 + w2 + w3 = 1.0, configurable per org
            default: w1=0.6, w2=0.25, w3=0.15
```

**Heuristic weighs more.** Reason: explainability, defensibility against an auditor, absence of false positives from model drift.

### Baseline period

- **14-day warm-up** per new entity before generating UEBA anomalies. Before that, UEBA weight = 0.

---

## 12. Forensic Engine

### Per incident

| Component | Content |
|---|---|
| **Timeline** | Events ordered by `occurred_at`, grouped by MITRE phase |
| **Evidence** | Artifacts in MinIO: logs, process dumps, hashes, registry snapshots |
| **Hash-chain** | Each piece of evidence is inserted into a per-incident Merkle chain; root hash signed and timestamp-sealed |
| **Chain of custody** | `evidence_custody` table: who accessed, when, what they did |
| **Affected entities** | Host(s), user(s), process(es), IP(s), domains |
| **MITRE mapping** | List of techniques observed, derived from incident events |
| **AI summary** | Generated by `cg-ml`: natural-language narrative, **always labeled "AI-generated"** and reviewable |
| **Export** | Signed PDF (PAdES if applicable), standalone HTML, structured JSON |

### Evidence hash-chain

```
evidence_n.hash = SHA-256(evidence_n.content)
chain_n        = SHA-256(chain_{n-1} || evidence_n.hash || timestamp_n)
root_signature = Ed25519_sign(server_key, chain_N)
```

This makes it possible to prove a posteriori that a set of evidence has not been tampered with since the incident was closed.

**Roadmap note — forensic trust anchoring.** Tamper-evidence that is robust *against a compromised server* requires an **out-of-band anchoring of the forensic key**: a way for an auditor to obtain the authentic public key independently of the server. That distribution-of-trust step is **pending**, tracked as an Open question in SPEC-012; the hash-chain shipped first proves **integrity under a trusted key**, with server-resistant authenticity following once the deployment-trust model is set.

### Reproducibility

- The exported JSON contains **all** information needed to reconstruct the incident without server access.
- Includes triggered rules, component versions, CGES `schema_version`, evidence hashes.

> **Honest positioning:** this is **not** court-admissible evidence on its own. It is robust technical traceability, sufficient for internal audit, academic defense, and as a starting point for a formal forensic process.

---

## 13. SOC Dashboard

### Views and priority

| View | Goal | Key metrics | Actions | MVP |
|---|---|---|---|---|
| **Overview** | SOC pulse | Online/offline agents, 24h alerts, open incidents, top hosts | Drill-down per metric | **P0** |
| **Agents** | Inventory | Status, version, last heartbeat, OS | View detail, revoke, view events | **P0** |
| **Events** | Search and exploration | Filters: host, user, type, time, MITRE | Pivot to alerts/incidents | **P0** |
| **Alerts** | Triage | Severity, score, dedup count, assignee | Ack, suppress, escalate to incident | **P0** |
| **Incidents** | Active cases | Severity, hosts, MITRE, age | Assign, comment, close | **P0** |
| **Cases** | Deep human management | Notes, evidence, timeline | Attach, annotate, export report | **P1** |
| **Network** | Connection view | Top talkers, geo, domains | Pivot to host/process | **P1** |
| **UEBA** | Anomalies per entity | Top anomalous users/hosts | Drill-down to baseline | **P2** |
| **SOAR Playbooks** | Playbook management | Executions, successes, failures | Trigger manually, edit | **P2** |
| **Forensics** | Per-incident view | Visual timeline, MITRE, evidence | Export PDF/HTML/JSON | **P1** |
| **Reports** | Executive reports | Trends, MTTR, top threats | Generate, schedule | **P2** |
| **Settings** | Config | Users, roles, integrations | CRUD | **P0** partial |
| **Audit Log** | Action traceability | Who did what | Search, export | **P0** |
| **System Health** | Internal status | Ingest rate, NATS lag, ClickHouse | Internal alarms | **P1** |

### UX principles

- **Two modes:** *Executive* (KPIs, charts, no noise) and *Technical* (dense, tables, filters). User chooses; don't invent third tiers.
- **Latency target:** first view <2s with 90 days of data. Any widget slower than that must load with a deferred skeleton.
- **No constant red alerts.** A SOC dashboard saturated with red is ignored. Real severity, no inflation.

---

## 14. Harness Engineering

### Harness architecture

```
scenarios/scenario_001/
├── manifest.yml         # metadata and assertions
├── events.jsonl         # events to inject (CGES)
├── expected_alerts.yml  # what alerts it must produce
├── expected_incident.yml
├── expected_mitre.yml
└── expected_report.snap.json
```

### Example: Scenario 001 — PowerShell encoded command

```yaml
# scenarios/scenario_001/manifest.yml
id: SC001
name: "PowerShell encoded command from Office process"
description: >
  WinWord spawns powershell.exe with -EncodedCommand argument.
  Classic phishing-to-execution chain.
mitre:
  expected_tactics: ["execution", "defense-evasion"]
  expected_techniques: ["T1059.001", "T1027"]
severity_expected: 8
incident_expected: true
playbook_suggested: "pb_isolate_endpoint"
assertions:
  - alert_count: 1
  - alert.rule_id: "rule.psh_encoded_from_office"
  - alert.final_score: ">= 0.75"
  - incident.tags_includes: ["malware-suspect", "execution"]
```

### Initial set (the 10 you asked for, refined)

| # | Scenario | Type | Expected severity |
|---|---|---|---|
| SC001 | PowerShell `-EncodedCommand` from Office | Heuristic detection | 8 |
| SC002 | Rare parent-child (`svchost.exe` → `cmd.exe`) | Heuristic + UEBA | 6 |
| SC003 | Brute force: 20 failed logins in 1 min | Stateful correlation | 7 |
| SC004 | New user added to Administrators group | Heuristic | 9 |
| SC005 | Lateral movement: PsExec from non-IT host | Heuristic + asset context | 8 |
| SC006 | Outbound connection to known IOC | Threat intel | 9 |
| SC007 | `\Run` registry modification | Heuristic (persistence) | 7 |
| SC008 | Agent without heartbeat >5 min | Health | 4 |
| SC009 | New service with random name + unsigned executable | Heuristic + ML | 8 |
| SC010 | Login within usual hours from usual host | **False positive** (must not alert) | 0 |

### Execution

- **CI:** every PR runs `cg-harness run --all`. Fails if an assertion is not met.
- **Regression:** forensic report snapshot (JSON) per scenario. Changes require explicit approval.
- **Replay:** scenarios are also the basis for onboarding new analysts.

> This harness is the project's **most valuable asset** after the code itself. Treat it as such.

---

## 15. Roadmap

Estimates for a team of **1–2 full-time equivalents**. If you're solo part-time, multiply by 2.5–3.

### Phase 0 — Design (3–4 weeks)

- ADRs 0001 to 0010 written and signed.
- Threat model documented (simple STRIDE).
- CGES v0.1 with validatable JSON Schema.
- Technology decisions closed.
- Repo initialized with empty but functional CI.
- **Deliverable:** this document turned into real `docs/`.

### Phase 1 — Core (6–8 weeks)

- `cg-api` with OTP/MFA, basic RBAC, users, orgs.
- `cg-ingest` receiving synthetic events via HTTP+mTLS.
- Postgres, ClickHouse, Redis, NATS, MinIO orchestrated with Docker Compose.
- Dashboard with Overview, Agents, Events, Alerts, Incidents views (static + read-only).
- "Stub" Rust agent sending only heartbeats and process events via ETW (Windows).
- 3 operational Sigma rules (SC001, SC003, SC004).

### Phase 2 — Detection (4–6 weeks)

- Complete pipeline `normalize → enrich → correlate → score → alert → incident`.
- 10 operational rules + harness passing.
- Gmail/SMTP notification.
- MITRE mapping in alerts and incidents.

### Phase 3 — Serious agent (8–10 weeks)

- Rust agent with: processes (ETW), network (WFP passive filters), files (sysmon-like via MiniFilter or ETW if enough), logs (Event Log).
- Local encrypted buffer, cert rotation, degraded mode.
- Linux agent (eBPF) as optional second target.

### Phase 4 — SOAR (3–4 weeks)

- Playbook engine, dry-run, human approval, immutable audit log.
- 5 example playbooks.

### Phase 5 — Forensics (4–5 weeks)

- Visual timeline, evidence hash-chain, PDF/HTML/JSON export.
- Forensic view in dashboard.

### Phase 6 — UEBA + local AI (6–8 weeks)

- User/host/process baselines.
- `cg-ml` with incident summary and case similarity (RAG with pgvector).
- FP reduction via feedback.

### Phase 7 — Enterprise hardening (open-ended)

- Full OpenTelemetry, Grafana dashboards.
- Load tests (k6, 50k eps).
- Multi-tenant if there is real demand.
- Helm chart, Terraform.

**Total to "seriously demonstrable product":** ~9–12 months solo or ~5–7 months with one competent collaborator.

---

## 16. Risks

| Risk | Probability | Impact | Mitigation |
|---|---|---|---|
| **Scope creep** ("we want all of XSIAM") | High | Critical | MVP written and signed. Every new feature passes through an ADR |
| **Event volume > design** | Medium | High | Load tests in phase 1, not phase 7. Sharding decisions deferred but budgeted |
| **Rust agent with production bugs** | Medium | Critical | Telemetry of the agent itself, "safe" mode without active sensors, automatic client rollback |
| **AI hallucinates and deletes evidence** | Low if we follow the principle | Critical | **AI never executes destructive actions.** No exceptions. Explicit ADR |
| **False positives flood the SOC** | High | High | Harness with FP scenarios (SC010), feedback loop, suppression rules |
| **Multi-OS compatibility** | High | Medium | Start Windows, Linux later, macOS maybe never or read-only |
| **GDPR / endpoint privacy** | Medium | High | Per-org configuration of what is collected, log of accesses to personal data, configurable retention |
| **Gmail API dependency** | Low | Low | `notifier` abstraction with SMTP fallback from day one |
| **Team operational capacity** | High | Critical | Honest roadmap, don't promise dates externally until phase 3 |
| **CGES schema migration** | Medium | High | Semver versioning, migrations tested in harness |

---

## 17. What Not To Build First

Explicit list of things that **MUST NOT** appear in MVP, for sanity:

1. **Real multi-tenancy.** A single physical `org_id` at first; the field exists but is not leveraged. Real multi-tenant is enterprise phase.
2. **Kubernetes.** Docker Compose until a real customer asks for it. K8s operation in MVP burns 30% of your time.
3. **Mobile/IoT/OT agents.** Each one is a project in itself.
4. **Cloud workload protection (CWPP).** That's a different product.
5. **DLP, CASB, SSPM, ASPM.** Not XDR.
6. **Integrations with vendor X, Y, Z** (Splunk, CrowdStrike, etc.). Big temptation, time sink. One single well-done integration (Gmail) in MVP.
7. **In-house trained ML models.** Use pre-trained (sentence-transformers, quantized instruct models). Training is phase 6+.
8. **GraphQL.** REST with well-defined OpenAPI. GraphQL in SOC adds complexity without clear return.
9. **Rule/playbook marketplace.** Sounds good, doesn't add value in MVP.
10. **Mac support.** No clean equivalent to ETW; adding Mac is a whole project.
11. **Advanced anonymization / privacy-preserving analytics.** Nice, out of scope.
12. **High availability with automatic failover.** Backups and manual restore first.

---

## 18. MVP Definition

> A single paragraph that defines the MVP. If a user story doesn't fit here, it doesn't enter.

> **MVP:** "An administrator installs the CyberGuard server with Docker Compose on their network. Creates a user with OTP. From the dashboard, generates an enrollment token. Installs the CyberGuard agent on a Windows machine with a single `.exe`. The agent appears in the dashboard within 60 seconds. Upon executing a suspicious PowerShell command on that machine, the SOC sees the alert in the Alerts view within 30 seconds, sees a grouped incident in the Incidents view, receives an email with the summary, and can export a PDF forensic report of the incident with timeline and MITRE mapping."

If this works, the rest is **iteration**, not reinvention.

### MVP acceptance criteria

- 10 operational detection rules, all with harness tests.
- 1 functional Windows agent with: processes, basic network, logins.
- OTP login, RBAC with 3 roles (admin, analyst, viewer).
- Gmail/SMTP notification.
- Incident PDF export.
- 1 SOAR playbook operational (dry-run + notification, no destructive actions yet).
- Installation documentation in <30 minutes.

---

## 19. Repository Structure

Monorepo. Proposed structure:

```
cyberguard/
├── README.md
├── LICENSE
├── .editorconfig
├── .gitignore
├── docker-compose.yml
├── docker-compose.dev.yml
├── Makefile                     # targets: bootstrap, test, harness, lint
│
├── docs/
│   ├── README.md
│   ├── adr/
│   │   ├── 0001-language-per-component.md
│   │   ├── 0002-polyglot-storage.md
│   │   ├── 0003-agent-server-protocol.md
│   │   ├── 0004-cges-ocsf-alignment.md
│   │   ├── 0005-normalize-before-correlate.md
│   │   └── template.md
│   ├── architecture/
│   │   ├── overview.md
│   │   ├── data-flow.md
│   │   └── diagrams/
│   ├── security/
│   │   ├── threat-model.md
│   │   └── secure-coding-guidelines.md
│   ├── product/
│   │   ├── vision.md
│   │   ├── mvp.md
│   │   └── roadmap.md
│   └── operations/
│       ├── runbook.md
│       └── observability.md
│
├── schemas/
│   ├── cges/
│   │   ├── v0.1/
│   │   │   ├── event.json
│   │   │   ├── process.json
│   │   │   ├── network.json
│   │   │   ├── auth.json
│   │   │   ├── alert.json
│   │   │   └── incident.json
│   │   └── README.md
│   └── api/
│       └── openapi.yml
│
├── services/
│   ├── api/                     # TypeScript / Fastify
│   │   ├── src/
│   │   ├── package.json
│   │   └── README.md
│   ├── ingest/                  # Go
│   │   ├── cmd/
│   │   ├── internal/
│   │   ├── go.mod
│   │   └── README.md
│   ├── pipeline/                # Go
│   ├── soar/                    # Go
│   ├── ml/                      # Python / FastAPI
│   └── forensic/                # Go
│
├── agent/                       # Rust workspace
│   ├── Cargo.toml
│   ├── crates/
│   │   ├── cg-agent-core/
│   │   ├── cg-agent-windows/
│   │   ├── cg-agent-linux/
│   │   └── cg-agent-cli/
│   └── README.md
│
├── dashboard/                   # Next.js 15
│   ├── app/
│   ├── components/
│   ├── package.json
│   └── README.md
│
├── rules/                       # Sigma rules
│   ├── windows/
│   ├── linux/
│   ├── network/
│   └── tests/
│
├── playbooks/                   # SOAR playbooks YAML
│   ├── isolate_endpoint.yml
│   ├── notify_critical.yml
│   └── tests/
│
├── harness/
│   ├── cmd/cg-harness/
│   ├── scenarios/
│   │   ├── SC001-powershell-encoded/
│   │   ├── SC002-parent-child/
│   │   └── ...
│   └── README.md
│
├── deploy/
│   ├── docker/
│   ├── helm/                    # empty for now
│   └── terraform/               # empty for now
│
└── .github/
    └── workflows/
        ├── ci.yml
        ├── harness.yml
        └── release.yml
```

---

## 20. First Implementation Tasks

Strict order. Don't skip steps.

### Week 1

1. **`docs/product/vision.md`** — copy/adapt section 2 of this document.
2. **`docs/product/mvp.md`** — copy section 18.
3. **`docs/adr/template.md`** — MADR format (https://adr.github.io/madr/).
4. **`docs/adr/0001-language-per-component.md`** — record the Rust/Go/TS/Python decision.
5. **`docs/adr/0002-polyglot-storage.md`** — record the PG/CH/Redis/MinIO/NATS decision and migration thresholds.
6. **`docs/security/threat-model.md`** — STRIDE per component (agent, ingest, api, dashboard, storage).
7. **Root `README.md`** — what CyberGuard is, how to start it, where the documentation lives.
8. **Repository initialized** with `.gitignore`, `.editorconfig`, `LICENSE` (Apache 2.0 suggested).

### Week 2

9. **`schemas/cges/v0.1/event.json`** — validatable JSON Schema for section 8.
10. **`docs/adr/0003-agent-server-protocol.md`** — protocol from section 7.
11. **`docs/adr/0004-cges-ocsf-alignment.md`** — what is inherited from OCSF.
12. **`docker-compose.dev.yml`** — Postgres, ClickHouse, Redis, NATS, MinIO starting with `docker compose up`.
13. **Basic CI** (`.github/workflows/ci.yml`): markdown lint, JSON Schema validation, Sigma rules placeholder lint.

### Week 3

14. **`harness/`** skeleton with SC001 (PowerShell encoded) written by hand: `manifest.yml` + `events.jsonl` + `expected_alerts.yml`. **No execution code yet.** Just the definition.
15. **`rules/windows/psh_encoded_from_office.yml`** — first Sigma rule written, no engine to run it.
16. **`services/api/` skeleton** Fastify with `/healthz` and `/readyz`.
17. **`services/ingest/` skeleton** Go with `/healthz` and `/v1/events` endpoint that only logs.

### Week 4

18. **`agent/crates/cg-agent-core/`** Rust skeleton: `tokio::main` that sends a dummy heartbeat to `cg-ingest` every 30s.
19. **First manual end-to-end test:** agent → ingest → log.
20. **Phase 0 closure.** Internal demo. ADR review. If anything is incomplete, no progression.

---

### Closing

Three things to take away:

1. **Success is not decided in phase 6 with brilliant AI, it is decided in phase 0 with the ADRs and the harness.** Get phase 0 right and everything else is work. Get it wrong and you end up with a pretty mockup that doesn't survive an audit.
2. **Resist the temptation to add features.** Every new feature in MVP is one less week to do the essentials well.
3. **Honesty in external communication.** Until the harness passes the 10 scenarios and a real agent sends real events, this is not a product; it is a serious project under construction. That difference matters.

---

End of Blueprint.
