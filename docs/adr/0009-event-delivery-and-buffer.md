# ADR-0009: Event delivery semantics and agent buffer model

- Status: Accepted
- Date: 2026-05-23
- Deciders: Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

SPEC-005 (in drafting) is the first SPEC under which the agent emits actual events into the secure envelope. SPEC-001/002/003 only ever carried heartbeats (envelopes with an empty events array); SPEC-004 only persisted heartbeats server-side. With events about to flow, two coupled decisions can no longer be deferred:

- **Delivery semantics.** What does *"the agent emitted event E to the server"* guarantee under transient failures (server 5xx, timeout, connection reset, agent restart mid-flight)?
- **Buffer model.** Where do uncommitted events live on the agent between capture and acknowledged delivery?

ADR-0004 §Heartbeat and degraded mode originally specified a persistent disk-backed encrypted buffer (200 MB / 24 h, DPAPI-derived key, drain on reconnect in `sequence_number` order). SPEC-001/002/003 deferred implementing it ("buffered offline events deferred"). With SPEC-005 imminent, the buffer question becomes load-bearing — but the disk-backed encrypted buffer is itself a non-trivial subsystem (at-rest encryption, file format, rotation, crash recovery, replay-on-reconnect, renewed cross-restart sequence persistence). For SPEC-005's scope (process telemetry MVP), the right call is to ship a minimal in-memory buffer now and defer the persistent variant to its own future SPEC that can address its subsystem questions seriously.

Two prior decisions constrain the design space favourably:

- **ADR-0006 fixed `event_id` as UUIDv7** — time-ordered and collision-resistant. UUIDv7 makes a natural dedup key: any duplicate `event_id` is unambiguously a retransmit, never a coincidence.
- **ADR-0003 fixed ClickHouse partitioning and ordering** — partitioning on `(org_id, toYYYYMMDD(time))`, ordering on `(org_id, time, event_id)`. ReplacingMergeTree keyed on `event_id` slots in without changing either.

The Phase 0 spike ([docs/spikes/2026-05-23-etw-process-events.md](../spikes/2026-05-23-etw-process-events.md)) supplies the empirical input for buffer sizing: 32 KB ETW buffers absorbed ~1.1 k events/sec of background Kernel-Process activity on a developer machine with zero loss; under deliberately induced pressure (1 KB × 2 buffers, 80 ms in-callback sleep, three 200-process bursts) 7649 events were lost in 25 s, monotonically. The lesson — quantified — is that the dispatch path must do nothing but enqueue; any per-event work happens on a separate worker.

## Decision

The agent's event-delivery and buffer model for SPEC-005 and immediate successors is:

### 1. At-least-once delivery, server-side dedup keyed on `event_id`

Events ride inside SPEC-003's outer signed envelope. On any retransmit (server 5xx, timeout, connection reset, sub-envelope partial failure), the agent resends the SAME `event_id` until the server acknowledges acceptance. The server treats two records sharing an `event_id` as one logical event.

`event_id` is UUIDv7 per ADR-0006, generated at **capture time** inside the dispatch callback (not at emit time). UUIDv7's embedded timestamp gives natural ordering across the ring buffer even if events are emitted out of capture order due to retry interleaving.

Exactly-once delivery was considered and rejected: it requires two-phase commit across the agent and server-side stores, which nobody in this space honestly delivers; the industry pattern is at-least-once + dedup. At-most-once (best-effort) was considered and rejected: a security agent that silently drops events on transient failures undermines the analyst's view, which is the whole purpose of the telemetry.

### 2. Server-side dedup mechanism — ClickHouse ReplacingMergeTree on `event_id`

The CGES events table uses `ENGINE = ReplacingMergeTree(event_id)` (working assumption; exact DDL settled by SPEC-005). The merge collapses duplicates asynchronously; queries needing strict immediate deduplication use `FINAL` or post-merge materialized views. The choice of `event_id` as the version column is the simplest correct mechanism given UUIDv7's properties.

Partitioning and ordering are unchanged from ADR-0003 Rule 2 / ADR-0006:

- `PARTITION BY (org_id, toYYYYMMDD(time))` — partitions are per-org per-day.
- `ORDER BY (org_id, time, event_id)` — primary key for indexing within partitions.

`event_id` is **not** a partition key. Partitioning by `event_id` would create one partition per event — an explicit anti-pattern surfaced in the SPEC-005 D6 ratification.

### 3. Agent buffer model — in-memory ring, ephemeral

No disk persistence in SPEC-005. The ring holds CGES events between capture (ETW callback enqueue, per ADR-0008) and flush (envelope POST to server, per SPEC-003). The shape is fixed here; the parameters are settled by SPEC-005.

Shape:

- **Bounded in events**, not bytes. Easier to reason about and to instrument.
- **Flush trigger:** max batch size OR max latency, whichever fires first.
- **Drop policy on overflow:** FIFO drop — the oldest events are evicted; a counter `events_dropped_total` is exposed as an observable. The exact transport for that observable (in the envelope, via a deferred metrics path, or both) is a SPEC-005 concern.
- **Pre-enrolment events are discarded.** The agent does not buffer events captured before its identity is loaded — there is nothing to sign them with. Pre-enrolment is a brief window in nominal operation; the agent does not attempt to retroactively cover it.

Parameters left to SPEC-005:

- Ring size (in events).
- Max batch size (in events) for the flush trigger.
- Max latency (in seconds or milliseconds) for the flush trigger.
- The observable surface for `events_dropped_total`.

SPEC-005 has the volume profile from the spike; it tunes accordingly.

### 4. Persistent disk-backed encrypted buffer deferred

The 200 MB / 24 h disk buffer originally in ADR-0004 §Heartbeat and degraded mode is deferred to a future SPEC. ADR-0004 is amended in-place in the same commit as this ADR (see ADR-0004's `## Amendment 2026-05-23:` section). The future SPEC will need to address: at-rest encryption (DPAPI on Windows in MVP; platform keyrings on Linux and macOS deferred separately per ADR-0002 Rule 2); file format and rotation; crash recovery; cross-restart `sequence_number` persistence (a renewed concern in that future scope, since it loses its remaining justification under the current amendment); and replay-on-reconnect semantics including how replayed events interact with the dedup mechanism above.

### Co-located amendment to ADR-0004

This ADR ships in a single commit with an in-place amendment to ADR-0004 with dual scope: (a) buffer model deferral driven by this ADR; (b) closure of the Session 7 SPEC-003 §Drift D3 declaration about `sequence_number` persistence and its anti-replay role, which the buffer removal makes literal in ADR-0004's prose. See ADR-0004's `## Amendment 2026-05-23:` section.

## Alternatives considered

### A1 — At-most-once delivery (best-effort), no retries, no buffer

Pros: simplest agent code path. No ring, no retry queue, no dedup logic on the server.

Cons: a security agent that silently drops events on transient failures (server restart, brief network outage, momentary 5xx) is a non-starter for the product thesis. The analyst loses visibility on exactly the windows where bad things happen.

Rejected.

### A2 — Exactly-once delivery

Pros: cleanest semantics. No client- or server-side dedup machinery needed once delivered.

Cons: practically impossible without two-phase commit between the agent and the server-side stores (Postgres + ClickHouse + Redis), and even then only with brittle distributed-transaction protocols. The industry pattern is at-least-once + idempotent server-side dedup; that pattern is what every major SIEM/XDR ships.

Rejected.

### A3 — At-least-once with disk-backed buffer for offline events (the original ADR-0004 contract)

Pros: longest offline tolerance — agents can buffer 200 MB / 24 h as ADR-0004 originally specified. Survives extended server outages. Survives agent crashes.

Cons: substantial subsystem (at-rest encryption, file format, rotation, crash recovery, replay-on-reconnect dedup interaction, cross-restart `sequence_number` persistence with its own concurrency story). Each of those is a non-trivial design decision; bundling all of them into SPEC-005 — whose actual goal is process telemetry — would block the first event SPEC on infrastructure that is not the SPEC's actual scope.

Rejected as the in-MVP path; **retained as the future-SPEC path**. The persistent buffer is named-but-deferred work, not abandoned.

### A4 — At-least-once with in-memory ring, ephemeral; persistent buffer deferred (chosen)

Pros: minimum subsystem to ship the first event SPEC. The ring's correctness is testable in isolation (size, FIFO drop, flush thresholds). The dedup is server-side and handles transient retries trivially through ReplacingMergeTree. The deferred persistent-buffer concerns are all enumerated, not hidden. Backpressure visibility (`events_dropped_total`) ships in MVP.

Cons: an agent that crashes loses its in-flight events — the window is bounded by the flush latency (a few seconds in nominal config); the loss is the events that hadn't yet been POSTed. An extended server outage causes FIFO drop of older events; the agent does NOT switch to disk persistence as a fallback. Both costs are accepted: the threat model already assumes endpoint compromise (lost local state on crash is a milder case), and an extended server outage is an operational concern signalled by `events_dropped_total`.

Chosen.

## Consequences

### Positive

- SPEC-005 unblocked: the delivery contract is settled; the buffer mechanism is small enough to ship in scope; the dedup is a one-line DDL choice.
- The agent has **no on-disk event state** in MVP. Crash-recovery for events, file rotation, and at-rest encryption of event data are all deferred subsystems — named, not built.
- `event_id` (UUIDv7) is the single dedup token. No coordination, no per-agent dedup tables, no fragility on agent restart.
- `events_dropped_total` is an observable that surfaces sustained backpressure or extended server outages. Operators can alert on it.
- The agent dispatch path (ETW callback → ring enqueue) is constrained to do nothing but enqueue, per the spike's empirical evidence on callback latency.

### Negative

- Agent crash loses in-flight events. The window is bounded by the flush latency (seconds, not minutes). The loss is the events that hadn't yet been POSTed when the crash happened. Documented in SPEC-005.
- Extended server outage causes FIFO drop of older events. The agent does **not** switch to disk persistence as a fallback. Tracked via `events_dropped_total`; resolution is operational (restore server connectivity); event loss in this window is accepted for MVP.
- The persistent disk buffer is named-but-deferred work. A future SPEC will need to address all the subsystem questions enumerated above and may also reintroduce cross-restart `sequence_number` persistence — whose closure under this amendment is conditional on the current buffer being ephemeral.

### Neutral

- ClickHouse ReplacingMergeTree collapses duplicates asynchronously, not immediately. Queries needing strict dedup use `FINAL` or post-merge materialized views. SPEC-005 specifies which read paths need which mode (none in SPEC-005 directly, but the future read API SPEC will).
- The ring's exact size and flush thresholds are SPEC-005 concerns; this ADR fixes only the shape so SPEC-005 has room to tune to its volume profile.
- `event_id` is generated inside the dispatch callback at capture time. The callback's only allowed work is "enqueue + generate UUIDv7"; both are sub-microsecond on commodity hardware. Per-event heavier work (JCS canonicalization, signing) happens on the worker that drains the ring.

## Compliance

- New event-emission code in the agent **goes through the ring**. Direct envelope-emit without going through the ring is forbidden — it bypasses the at-least-once retry mechanism and the FIFO-drop observability.
- The agent's `event_id` MUST be UUIDv7 per ADR-0006. Any other format breaks the dedup contract.
- Server inserts of CGES events go through the ReplacingMergeTree table; no other engine is used for the CGES events table.
- Persistent disk-backed buffering is reserved for a dedicated future SPEC. SPEC-005 and immediate successors do NOT introduce on-disk event state. Any change that introduces on-disk event state requires either that future SPEC or a new ADR superseding this one.
- The dispatch callback (ETW path per ADR-0008, future event sources of the same telemetry family) does nothing but enqueue to the ring and generate the `event_id`. Per-event work (canonicalization, signing, network I/O) lives on the worker draining the ring.

## References

- [ADR-0001](0001-monorepo-layout.md) — Monorepo layout.
- [ADR-0003](0003-polyglot-storage.md) — Polyglot storage. ClickHouse is the events store; ReplacingMergeTree is one of its engines. Partitioning on `(org_id, toYYYYMMDD(time))` and ordering on `(org_id, time, event_id)` are unchanged from this ADR.
- [ADR-0004](0004-agent-server-protocol.md) — Agent-Server secure protocol. Amended in-place in the same commit as this ADR; see its `## Amendment 2026-05-23:` section. The amendment supersedes the persistent-disk-buffer prose and closes the SPEC-003 §Drift D3 sequence_number consequence.
- [ADR-0006](0006-cges-ocsf-alignment.md) — CGES alignment with OCSF v1.3. `event_id` is UUIDv7; the dedup contract relies on this.
- [ADR-0008](0008-etw-crate-selection.md) — ETW crate selection. The ring's enqueue side is the ferrisetw callback path; the spike data in ADR-0008's empirical-justification block informs the volume profile.
- [docs/spikes/2026-05-23-etw-process-events.md](../spikes/2026-05-23-etw-process-events.md) — Phase 0 spike. Source of the empirical inputs for the dispatch-callback NFR and the background-traffic baseline that informs SPEC-005's ring sizing.
- [docs/engineering-notes.md](../engineering-notes.md) — Session 10 entry on follow-up co-location and the three-role Deciders convention; the cross-reference-points-to-cause convention from this session's amendment work is added in the ratification commit alongside.
- [SPEC-003](../specs/SPEC-003-mtls-signed-envelope.md) — §Drift D3 is the original declaration of `sequence_number`'s persistence loss; ADR-0004's amendment in this commit closes the prose drift in ADR-0004 for the first time.
- SPEC-005 (forthcoming) — First consumer of this ADR. Specifies the ring's parameters (size, batch-size and latency thresholds), the exact ClickHouse DDL, and the observable surface for `events_dropped_total`.
