# ADR-0003: Polyglot storage

- Status: Accepted
- Date: 2026-05-20
- Deciders: Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

CyberGuard's workloads are heterogeneous. The system must store transactional state (users, agents, cases, RBAC), high-volume time-series events with second-scale aggregation requirements, low-latency cache for sessions and rate limits, large binary artifacts (forensic dumps, PDF reports), a persistent event bus capable of replay, and vector embeddings for forensic similarity and retrieval-augmented generation.

No single database serves all of those workloads well. Forcing one to do so creates either operational pain — Postgres running at ClickHouse-scale ingest — or capability gaps — ClickHouse running transactional case management. The cost of operating more than one backend is real but bounded; the cost of fighting a single backend against a workload it was not built for is unbounded.

ADR-0001 locked the monorepo layout. ADR-0002 locked the language per component, with several language justifications already citing the maturity of clients for specific storage backends (NATS Go client, ClickHouse Go client). This ADR locks the storage backends themselves, the workloads each one owns, and the quantitative thresholds that would justify revisiting any choice.

The decision recorded here must:

1. Match each workload to the backend designed for it.
2. Bound the operational footprint to the minimum the product genuinely needs.
3. Pre-commit to quantitative migration thresholds, so that future revisits are not religious debates.
4. Account for the lifecycle of stored data (hot, warm, cold) and for the retention requirements that audit obligations imply.

## Decision

CyberGuard MVP runs on five storage backends, each owning a workload.

| Workload | Backend | Justification |
|---|---|---|
| Relational state, RBAC, cases, audit log | PostgreSQL 16 | Transactions, foreign keys, JSONB, `LISTEN` / `NOTIFY` for live updates |
| Vector embeddings (forensic RAG, similarity) | pgvector on the same Postgres instance | One fewer database to operate; sufficient up to approximately 1M vectors |
| Raw and normalized events, UEBA aggregations | ClickHouse | Column-store compression, second-scale aggregations over billions of rows |
| Cache, sessions, rate limit, SOAR locks, anti-replay nonces | Redis 7 | Low-latency ephemeral key-value with TTL semantics |
| Forensic artifacts, PDF / HTML reports, archived events | MinIO (S3-compatible) | Standard object storage, self-hostable |
| Event bus (durable, replayable) | NATS JetStream | Persistent, hierarchical subjects, replay support; simpler operationally than Kafka |

The total backend surface is bounded to **five**: PostgreSQL (with pgvector), ClickHouse, Redis, MinIO, NATS JetStream.

### Cross-cutting storage rules

**Rule 1.** The `pgvector` extension is enabled from day one in the bootstrapped Postgres instance, even before `cg-ml` exists. Enabling it later requires a migration; enabling it preemptively is a one-line configuration change with zero runtime cost while unused.

**Rule 2.** ClickHouse partitions are by day, ordered by `(org_id, occurred_at, event_id)`. No tenant-level sharding in MVP; the single physical `org_id` field exists but is not leveraged for multi-tenancy (out of scope per Blueprint §17.1).

**Rule 3.** MinIO buckets are versioned and immutable for forensic artifacts and audit-log mirrors. Object lifecycle rules drive cold-storage transitions per the retention table below.

**Rule 4.** NATS JetStream subjects are hierarchical:

```text
events.raw.{org}.{agent}
events.normalized.{org}
events.enriched.{org}
alerts.{org}
incidents.{org}
```

Consumers are durable, named, and load-balanced via queue groups for horizontal scaling within a service.

**Rule 5.** Redis is treated as ephemeral. No data lives in Redis whose loss would corrupt system state. Anti-replay nonces (used by the agent-server protocol locked in ADR-0004) are an explicit acceptable exception: their loss yields a degraded security window, not corruption.

### Retention model

The retention model below is the initial setting. It is revisable per organisation but is tracked centrally to ensure audit obligations are met.

| Data type | Hot | Warm | Cold |
|---|---|---|---|
| Raw events | 7 d ClickHouse | 30 d ClickHouse (zstd-compressed) | 365 d MinIO |
| Normalized events | 30 d ClickHouse | 180 d ClickHouse | 365 d MinIO |
| Alerts | Postgres + ClickHouse | Postgres + ClickHouse | — |
| Cases / incidents | Postgres | Postgres | — |
| Forensic artifacts | MinIO standard | MinIO IA | MinIO archive (1 y+) |
| Audit log | Postgres append-only | 1 y | MinIO signed 7 y |

### Migration thresholds

Each backend is paired with a quantitative threshold that triggers a new ADR to revisit the choice. Until the threshold is breached, the current stack stands.

- **pgvector → Qdrant (or equivalent):** when vector count exceeds 1M *and* p95 similarity-query latency exceeds 200 ms sustained for 7 days.
- **ClickHouse text indexes → OpenSearch:** when free-text search latency p95 exceeds 5 s sustained for 7 days over a 30-day window.
- **NATS JetStream → Redpanda / Kafka:** when sustained ingest exceeds 100k events per second for 24 hours, *or* when cross-cluster replication becomes a hard requirement.
- **Postgres → sharded Postgres or CockroachDB:** when single-node write saturation exceeds 60% sustained for 7 days under normal load.

Each threshold breach opens a new ADR; the migration plan lives in that ADR.

## Alternatives considered

### A1 — Single backend (Postgres for everything, JSONB for events)

Pros: one database to operate, uniform ACID guarantees.

Cons: Postgres cannot sustain 10-100k events per second with aggregation queries under second-scale latency; storage cost balloons as events accumulate; UEBA aggregations over billions of rows are unfeasible at scale.

Rejected. ClickHouse exists for exactly the workload Postgres struggles with.

### A2 — Single backend (ClickHouse for everything, including transactional state)

Pros: best ingest and analytics performance, one database to operate.

Cons: ClickHouse is not transactional, lacks foreign keys, lacks `LISTEN` / `NOTIFY`; case management, RBAC and audit-log integrity become application-level problems that Postgres solves for free.

Rejected. ClickHouse is the wrong tool for transactional state.

### A3 — Add OpenSearch from day one for full-text search

Pros: best-in-class search UX from MVP.

Cons: significant operational overhead (cluster, JVM tuning, shard management) for a capability that ClickHouse with `tokenbf_v1` indexes covers approximately 80% of in MVP. Operating two analytical stores in parallel is premature.

Rejected. The migration threshold above defines when this will be revisited.

### A4 — Replace NATS with Kafka or Redpanda from day one

Pros: industry-standard event log, larger connector ecosystem, multi-cluster replication built in.

Cons: operational complexity (ZooKeeper or KRaft for Kafka, partition planning, broker tuning) far exceeds NATS for the throughput targets of MVP. NATS JetStream provides persistence, replay and subject hierarchy with a fraction of the operational surface.

Rejected. The migration threshold above defines when this will be revisited.

### A5 — Skip MinIO, store artifacts on local disk

Pros: one fewer service to operate.

Cons: local disk does not scale across multiple server instances; forensic artifacts must outlive the lifetime of any single server; a signed audit-log mirror requires immutable object storage to be defensible.

Rejected. MinIO stays.

## Consequences

### Positive

- Each workload runs on the backend matched to it; no backend is doing a job it is poorly suited to.
- ClickHouse handles the ingest and aggregation paths that would saturate Postgres within weeks.
- Forensic artifacts are object-storage-native, which is the only defensible long-term home for them.
- Migration thresholds are quantitative and pre-agreed. There will be no religious debate when the time comes to revisit a backend.

### Negative

- Five backends to operate, monitor, back up, and patch. The operational cost is real and is mitigated only partially by Docker Compose in MVP and by Helm / Terraform in the enterprise phase.
- Backup strategy is per-backend and not unified. A future ADR or runbook section must cover this before production.
- Cross-backend consistency (an alert in ClickHouse plus an incident in Postgres) is eventual, not transactional. The pipeline must handle re-delivery and idempotency explicitly.
- The local development environment requires all five services to be up for realistic integration testing. Mitigated by `docker-compose.dev.yml`.

### Neutral

- Future migration of any backend (pgvector → Qdrant, ClickHouse text → OpenSearch, NATS → Kafka) is pre-planned via thresholds; the migration itself will require its own ADR.
- The single physical `org_id` field is leveraged by ClickHouse partitioning but does not enable multi-tenancy. Multi-tenancy is a future ADR (out of MVP scope per Blueprint §17.1).

## Compliance

Subsequent ADRs and SPECs that introduce a sixth storage backend — or that move data across backends in ways that contradict the table above — must reference this ADR and explicitly justify the addition or movement.

Breach of any migration threshold above must open a new ADR that supersedes the corresponding row of the decision table. The new ADR documents the breach evidence (timestamps, metrics, sustained-duration measurement) and the migration plan.

## References

- [ADR-0001](0001-monorepo-layout.md) — Monorepo layout (defines the components whose data this ADR routes)
- [ADR-0002](0002-language-per-component.md) — Language per component (justifies Go for ingest and pipeline citing NATS and ClickHouse client maturity)
- Blueprint §6 — Storage Architecture
- Blueprint §9 — SIEM/XDR Pipeline (defines the data flows this storage serves)
- Blueprint §17.1 — Real multi-tenancy not in MVP
