# SPEC-019: Agent network telemetry — Windows ETW Kernel-Network

- **ID:** SPEC-019
- **Title:** Agent network telemetry — Windows ETW Kernel-Network
- **Status:** Accepted
- **Depends on:**
  - ADR-0018 — the per-class decisions this SPEC produces: the source, the scope, the field mapping, endpoints, process attribution, `time`, the agent's own connections and the single table.
  - SPEC-005 — delivers its §Out of scope §1 for the network provider, on the capture infrastructure it built. Its ring, its cache and its timestamp conversion are reused. The cache gains one use that SPEC-005 §Operational §2 does not list: a lookup that leaves the entry in place (§Operational §3).
  - SPEC-017 — amends **by scope** its §Data contracts "Event element" (an element is now one of two shapes, §Data contracts below) and the description of the dispatch callback in its §Operational §6 (a network record looks the cache up without purging it, §Operational §3). Startup, batching, retry, shutdown and the server's body limit are unchanged and relied on.
  - SPEC-018 — the detection read-model is unchanged: it reads class 1007 through the arrival cursor, and the rows of this SPEC do not reach it (§Operational §8). Its §Out of scope sends "Classes other than Process Activity 1007" to roadmap §D, "which reads through this cursor"; §D stores class 4001 and does not evaluate it, so that destination becomes the detection SPEC that §Out of scope below names.
  - ADR-0011 — the `process.uid` recipe (§6) and the dual layer of a permissive schema and a narrower agent (§3).
  - ADR-0008, ADR-0009 and ADR-0010 — ferrisetw, the delivery and buffer model, and the privilege model, all unchanged.
  - `docs/product/roadmap.md` — §D, the phase this SPEC is the first contract for.
- **Authors:** Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

MVP criterion 2 asks the agent to capture network and logins besides processes (roadmap §D). Observed at `b42317d`:

1. **The agent captures one class.** The ETW session enables one provider, Kernel-Process, and its callback returns on any event id other than 1 and 2 (`agent/cg-agent/src/etw/session.rs`). The captured event, the ring, the batch and the envelope's `events` are typed for process events (`etw/types.rs`, `etw/ring.rs`, `delivery.rs`, `envelope.rs`).
2. **The ring refuses an event without an image name.** `EventRing::enqueue_or_drop` drops any event whose `image_file_name` is empty: the rule of SPEC-005 AC-006, which that criterion states for Process Activity events.
3. **The cache answers one question.** `CreatedTimeCache` maps a PID to its creation time; a Launch fills it and a Terminate reads the entry and removes it (`etw/cache.rs`).
4. **The server accepts one class.** The element schema has `class_uid: z.literal(1007)` and a required `process` (`services/ingest/src/schemas.ts`). A POST with any other element fails structural validation and is answered 400 as a whole, its process events included (`routes/heartbeat.ts`); the agent then drops that batch after `max_retries` attempts (SPEC-017 §Operational §3).
5. **The table has process columns.** In `cges_events`, the columns `agent_id`, `event_id`, `class_uid`, `activity_id`, `process_pid`, `process_uid`, `process_name` and `time` have no default (`db/migrate.ts`).
6. **Who reads the table.** The detection read-model selects `class_uid = 1007` in each of its three queries (`detect/read-model.ts`). The forensic drill selects by `event_id` and projects process columns; its row has no `class_uid` (`services/api/src/read/queries.ts`). The test helper `getCgesEvents` reads every row of an agent, whatever its class (`services/ingest/test/helpers/db.ts`).
7. **Nothing was measured on real ETW** for this provider before this SPEC (ADR-0018 §Context 7). The acceptance criteria that run in the elevated gate settle it (net_ac_008 to net_ac_010).

## Scope

### In scope

- The agent's session enables Kernel-Network beside Kernel-Process and reports TCP connections opened, outbound and inbound, over IPv4 and IPv6 (§Operational §1–§4).
- The agent's event types, ring, batch and envelope carry both classes (§Operational §5).
- The wire's event element becomes one of two shapes, told apart by `class_uid` (§Data contracts).
- The server validates, converts and stores both classes; `cges_events` gains six columns (§Data contracts, §Operational §6–§7).
- The CGES schema files follow: `classes/4001_network_activity.json` gains the `actor` property that `classes/1007_process_activity.json` has, and the description of `time` in `event.json` names class 4001 beside 1007.
- The tests of §Acceptance criteria: in CI, and in the elevated gate.

### Out of scope

Each item has its destination in brackets.

- Logins, CGES class 3002 [ADR-0019 and SPEC-020, roadmap §D].
- UDP, DNS, connection close and failure, traffic volume, listening sockets [ADR-0018 §Out of scope; DNS is the first follow-up after roadmap §D].
- Detection rules over network events [a later detection SPEC]. Before any rule cites a network event, the forensic evidence unit must carry `class_uid`: today a sealed row would not say which class it is [a debt recorded in the S34 handoff].
- An API endpoint or a dashboard view of network events [the Blueprint's Network view, §13, P1; not in the MVP roadmap]. After this SPEC the events are stored and can be queried in ClickHouse; no product surface shows them.
- Validating or bounding an event's `time` [debt #31, unchanged: a network event's `time` goes through the same conversion].
- Sampling, rate-limiting or aggregating connection events, and a share of the ring reserved per class [§Open questions 2].
- Version negotiation between agent and server [not planned; §Operational §9 gives the upgrade order].
- Aligning the examples under `schemas/cges/v0.1/examples/` with the wire: they show `time` in ISO 8601 for every class, 1007 included, and are not wire elements [a debt recorded in the S34 handoff].
- Capture on non-Windows platforms [ADR-0002 Rule 2; post-MVP].

## Data contracts

### Event element

Amends SPEC-017 §Data contracts "Event element" by scope. An element of `body.events` is one of two shapes, told apart by `class_uid`. An element whose `class_uid` is neither is invalid.

- **`class_uid` 1007** — unchanged (SPEC-017 §Data contracts).
- **`class_uid` 4001** — a TCP connection opened (ADR-0018 §2–§6):

```json
{
  "event_id": "0199d2a4-5b7c-7e1a-9c3d-2f4a6b8c0d1e",
  "class_uid": 4001,
  "activity_id": 1,
  "time": "1791625812345678900",
  "src_endpoint": { "ip": "192.0.2.10", "port": 49213 },
  "dst_endpoint": { "ip": "198.51.100.7", "port": 443 },
  "connection_info": { "protocol_name": "tcp", "direction": "outbound" },
  "actor": {
    "process": {
      "pid": 4321,
      "uid": "01934abc-def0-7000-89ab-000000000001:4321:1791625800000000000"
    }
  }
}
```

- `event_id` is a UUIDv7 generated at capture (ADR-0009 §1). `time` is string-encoded Unix nanoseconds.
- `activity_id` is `1`. `connection_info.protocol_name` is `tcp`. `connection_info.direction` is `outbound` or `inbound`.
- `src_endpoint` is the initiator and `dst_endpoint` the acceptor (ADR-0018 §4). `ip` is a valid IPv4 or IPv6 address in text; `port` is an integer from 0 to 65535.
- `actor.process.pid` is always present. `actor.process.uid` is present only when the agent knows the process's creation time (§Operational §3); otherwise the member is omitted.
- A resent element is byte-identical, as SPEC-017 §Data contracts requires of every element.

### Storage

`cges_events` gains six columns. The table's engine, partitioning, ordering and existing columns are unchanged.

| Column | Type | 4001 row | 1007 row |
| --- | --- | --- | --- |
| `src_ip` | `String DEFAULT ''` | `src_endpoint.ip` | `''` |
| `src_port` | `UInt16 DEFAULT 0` | `src_endpoint.port` | `0` |
| `dst_ip` | `String DEFAULT ''` | `dst_endpoint.ip` | `''` |
| `dst_port` | `UInt16 DEFAULT 0` | `dst_endpoint.port` | `0` |
| `net_protocol` | `String DEFAULT ''` | `tcp` | `''` |
| `net_direction` | `String DEFAULT ''` | `outbound` or `inbound` | `''` |

A 4001 row also sets the columns every class shares: `agent_id`, `org_id`, `event_id`, `class_uid` (4001), `activity_id` (1) and `time`; `process_pid` from `actor.process.pid`; `process_uid` from `actor.process.uid`, or `''` when it is omitted; and `process_name` as `''`. The other process columns keep their defaults.

## Operational

### 1. Capture

- The agent's ETW session keeps its name and enables two providers: Kernel-Process as today, and `Microsoft-Windows-Kernel-Network` with the keywords `0x10` and `0x20`.
- The network provider is enabled with a filter by event id for 12, 15, 28 and 31. The callback also checks the id and discards any other; the session counts those discards, and the hygiene pass logs an increase at `warn`.
- A session that cannot start does not degrade to one provider: SPEC-017 §Operational §1 applies as it is (exit code 9 for a privilege failure, 1 for any other).
- The `events_lost` poll is the session's, so it now covers both providers.

### 2. Decoding

- The record's `PID`, addresses and ports are decoded by a pure function, tested without ETW (net_ac_005).
- The event id gives the direction: 12 and 28 are outbound, 15 and 31 are inbound. The endpoints follow ADR-0018 §4.
- An IPv4-mapped IPv6 address becomes its IPv4 address.
- `time` is the record's header timestamp, converted as SPEC-005 §Operational §1 specifies. A record with a timestamp before 1970 is logged at `error` and dropped, as a process record is.
- A record whose fields cannot be parsed is dropped and counted with the discards of §1.

### 3. Process attribution

- At dispatch the callback looks the record's `PID` up in the creation-time cache, without removing the entry. Found: the event carries the ADR-0011 §6 uid built from that creation time. Not found: the event carries no uid.
- The lookup happens at dispatch, not when the event is rendered, so a PID reused before the batch is formed cannot change the uid.
- Amends SPEC-017 §Operational §6 by scope: for a network record the callback parses, converts the timestamp, generates the `event_id`, looks the cache up and enqueues. It still does no I/O and takes no lock beyond the cache's and the ring's.

### 4. The agent's own connections

A network record whose `PID` is the agent's own process id is discarded at dispatch (ADR-0018 §8). It is not an event: it is not counted as dropped, and not with the discards of §1. The PID to exclude is a parameter of the capture, which the agent sets to its own; it is not configuration. When the server runs on the agent's host, the server's end of those connections belongs to another process and is reported.

### 5. Ring, batch and delivery

- One ring holds both classes. Its capacity and the batch triggers (SPEC-005 NFR-005-002) and its FIFO drop on overflow (ADR-0009 §3) are unchanged. Events leave in the order they entered, whatever their class.
- The empty-name rule of SPEC-005 AC-006 applies to process events, as that criterion states it. A network event has no image name and is enqueued.
- An event is rendered once, when its batch is formed, in its class's shape (§Data contracts). Path translation applies to process events only.
- Retry, rejection, liveness and shutdown are SPEC-017 §Operational §2–§4, unchanged.

### 6. Server

- Structural validation accepts the two shapes of §Data contracts and nothing else. For a 4001 element it checks every member that section lists, including that `ip` is a valid address and `port` is in range. A POST with an invalid element is answered 400 `invalid_request` as a whole, as today.
- The route writes each element as one row, with the mapping of §Data contracts, in the one `INSERT` into `cges_events` it already makes per POST: all the events of a POST share one `arrived_at` (SPEC-018 §Data contracts).
- The route sends every column that has no default.

### 7. Storage migration

- The bootstrap adds the six columns with an idempotent `ALTER TABLE cges_events ADD COLUMN IF NOT EXISTS`, after its `CREATE TABLE IF NOT EXISTS`, which stays as it is. A table that already exists gets the columns too.
- Rows written before the columns existed read with the defaults.
- The api's test mirror of the table (`services/api/test/helpers/events-schema.ts`) gains the same `ALTER`, so it keeps describing the table the api reads. The api itself reads none of the new columns.

### 8. Readers

- The detection read-model is unchanged. It selects class 1007, so a network row is never evaluated and never moves the cursor (SPEC-018 §Operational §1).
- The forensic drill is unchanged. It resolves the events an alert cites by `event_id`, without a class filter, and every alert comes from a process rule (ADR-0018 §9).
- Tests and helpers that read `cges_events` to assert on process events select `class_uid = 1007`.

### 9. Upgrade order

The server is upgraded before the agents. A server without this SPEC answers 400 to any POST that carries a network element, and the agent drops that batch, its process events included, after `max_retries` attempts (SPEC-017 §Operational §3). A server with this SPEC accepts an older agent's POSTs as before.

## Non-functional requirements

- **NFR-019-001 (dispatch).** The callback's work for a network record is that of §Operational §3: no I/O, no lock beyond the cache's and the ring's. SPEC-005 NFR-005-001 holds as SPEC-017 §Operational §6 amended it.
- **NFR-019-002 (shared ring).** The ring's capacity (65536), the batch size (1024) and the latency trigger (5000 ms) are unchanged and shared by both classes.
- **NFR-019-003 (marquee budget).** The network marquee (net_ac_010) completes within the 45 s of SPEC-005 NFR-005-004 and logs its elapsed time on every run.

## Acceptance criteria

Each maps to a test named `net_ac_NNN_*`, under `agent/cg-agent/tests/` or `services/ingest/test/`. A probe is a short-lived process, started by the test, that opens the TCP connection the criterion names; which program it is is the test's choice.

Server, in `ts-ci` against testcontainers, without ETW:

- **net_ac_001 (a mixed POST persists).** A signed POST whose `events` holds one 1007 element and two 4001 elements — one outbound over IPv4 with a uid, one inbound over IPv6 without — is answered 200. Each 4001 row has `class_uid` 4001, `activity_id` 1, the endpoint, protocol and direction columns of its element, its `process_pid`, its `process_uid` (`''` for the element without uid), and the `time` of its element. The 1007 row is stored as before, with the six new columns at their defaults. The three rows share one `arrived_at`.
- **net_ac_002 (validation).** Each of these POSTs is answered 400 `invalid_request` and stores nothing, its valid elements included: an element with a `class_uid` other than 1007 and 4001; a 4001 element without `dst_endpoint`; with a `port` of 65536; with an `ip` that is not an address; with `activity_id` 2; with a `direction` of `lateral`.
- **net_ac_003 (storage migration).** After the bootstrap `cges_events` has the six columns with the types and defaults of §Data contracts, on a new table and on a table created without them; a second bootstrap changes nothing; a row written before the columns existed reads with the defaults.
- **net_ac_004 (detection is unaffected).** With a matching parent → child pair of class 1007 stored among 4001 rows of the same agent and the same PIDs, a detection cycle evaluates exactly the 1007 rows and writes the one alert the pair raises without them.

Agent, in `rust-ci` on every platform, without ETW:

- **net_ac_005 (decoding).** The pure decoding gives, for records of events 12, 15, 28 and 31, the direction and the endpoints of ADR-0018 §4, with ports and addresses in their text form, including an IPv4-mapped IPv6 address emitted as IPv4. A record of another event id gives no event.
- **net_ac_006 (dispatch).** Synthetic network records through the dispatch logic: the event has a UUIDv7 `event_id` and the converted `time`; it carries the ADR-0011 §6 uid when a Launch of its PID was dispatched before it, and none otherwise; the cache entry is still there afterwards, so a later Terminate of that PID finds its creation time; a record with the excluded PID is not enqueued and does not change the dropped total; a network event is enqueued although it has no image name, and a process event with an empty image name is still dropped.
- **net_ac_007 (render and delivery).** A ring holding process and network events interleaved is delivered in that order, each element in its class's shape, with `actor.process.uid` omitted where it is unknown; after a transient failure the resent elements are byte-identical.

Agent on real ETW, elevated, in the gate (`#[ignore]`d like the SPEC-017 ETW tests):

- **net_ac_008 (a real connection).** With the agent on its normal path against the TLS mock, a probe process connects over IPv4 loopback and over IPv6 loopback to listeners the test holds. For each connection the agent delivers one outbound 4001 element whose `actor.process.pid` is the probe's, whose `dst_endpoint` is the listener's address and port, whose `src_endpoint` is the address and port the listener saw as its peer, and whose `actor.process.uid` equals the `process.uid` of the probe's Launch element. No 4001 element carries the PID of the test process, which hosts the agent, connects to the mock and accepts the probe's connections. The probe also attempts a connection to a loopback port with no listener; the test logs whether a 4001 element was delivered for it and does not assert it (ADR-0018 §10). On failure the test prints the 4001 elements it received and the endpoints it expected.
- **net_ac_009 (an accepted connection, and the filter).** The test opens the capture session itself, with no PID excluded, and holds a listener; a probe process connects to it over loopback and exchanges data. The ring then holds one inbound event for that connection, with the test process's PID, the listener's address and port as destination and the peer the listener saw as source; and one outbound event with the probe's PID and the same endpoints. The session's count of discarded network records stays at 0: the filter by event id is honoured. On failure the test prints the network events in the ring and the endpoints it expected.

End to end, elevated, developer-local:

- **net_ac_010 (marquee).** The real agent binary on its normal path, against the real stack: a probe process connects to a listener the test process holds. Among that agent's 4001 rows in `cges_events`, read with `FINAL`, those whose `dst_port` is the listener's port and whose `src_port` is the port the listener saw as its peer's are exactly two: one outbound with the probe's PID and one inbound with the test process's PID, both with the probe's address as `src_ip` and the listener's as `dst_ip`. The outbound row's `process_uid` equals the `process_uid` of the probe's Launch row. No 4001 row of that agent carries the agent's own PID. The test process also talks to the stack during the run, so its other rows are not asserted.

Regression:

- **net_ac_011 (regression and gate).** `cargo test --all` is green on Linux and on unelevated Windows, and `ts-ci` is green. In the elevated gate the four existing real-ETW tests and the existing marquees pass; those that run the agent's session run it with both providers.

## Test scenarios

No detection scenario changes. No scenario is added: this SPEC raises no alert.

## Risks

| Risk | Mitigation |
| --- | --- |
| The gate contradicts an assumption: the provider reports nothing on loopback, `PID` is not the accepting process, the byte order differs | The criteria assert outcomes, so a wrong assumption fails the gate instead of shipping; net_ac_008 prints what it received; ADR-0018 §10 names the amendment path |
| The filter by event id is not honoured, and every segment reaches the callback | net_ac_009 fails; the callback's own check keeps the output correct, and the cost is decided before landing (§Open questions 1) |
| A host that opens connections faster than the agent delivers them pushes process events out of the ring | The overflow warning and the dropped total make it visible (SPEC-017 §Operational §6); no reservation in v0.1 (§Open questions 2) |
| An agent is upgraded before its server, and loses whole batches | §Operational §9 states the order; the agent's `error` line records each dropped batch |
| A reader of `cges_events` forgets the class filter and takes an Open for a Launch | ADR-0018 §Compliance; net_ac_004 guards detection; the helpers filter (§Operational §8) |
| An outbound event is read as an established connection | ADR-0018 §2 and §10 say "attempted" until the gate records the fact |
| The session's buffers now carry two providers | The `events_lost` poll covers the session (§Operational §1); net_ac_009 shows whether the filter keeps the volume to connections |

## Open questions

1. **A second session for the network provider.** It would keep network volume from costing process events, at the price of a second name, pump thread, reclaim and loss poll. **Reopen if** net_ac_009 shows the filter is not honoured, or `events_lost` grows in operation.
2. **Volume control.** No sampling, aggregation or per-class share of the ring exists. **Reopen if** the dropped total grows on a real host.
3. **Loopback.** Connections between local processes are reported like any other. **Reopen if** they prove to be noise once something consumes these events.
4. **What an outbound Open asserts.** Recorded by net_ac_008. **Reopen when** the gate has the answer: if a failed attempt is reported, ADR-0018 §2 keeps "attempted" and a rule that needs an established connection must say how it knows.

## Ratification record

Load-bearing decisions for Manuel's gate. Manuel delegated the three owner decisions (1–3) explicitly ("elige tú la mejor opción", 2026-10-10); the advisor decided, and Manuel's ratification of this SPEC ratifies them. Decisions 4–12 are the advisor's, in the reversible lane; they are recorded because the contract rests on them.

1. **The scope of roadmap §D:** capture, delivery and storage of both classes, with a marquee for each. No detection rule and no product surface.
2. **"Basic network"** is TCP connections opened, outbound and inbound, over IPv4 and IPv6, from Kernel-Network. UDP, DNS, closes and traffic volume are out.
3. **The vehicle:** one per-class ADR and one SPEC for each class, network first (ADR-0018 and this SPEC; then ADR-0019 and SPEC-020). This SPEC amends SPEC-017 by scope, and moves the destination of one §Out of scope item of SPEC-018.
4. **One session, two providers,** with a filter by event id.
5. **`src_endpoint` is the initiator** in both directions.
6. **The uid is omitted when unknown,** never invented.
7. **The agent does not report its own connections.**
8. **`time` is string-encoded nanoseconds** for this class too.
9. **One table,** widened by six columns with defaults; D6 extended.
10. **The server is upgraded before the agents;** no negotiation.
11. **No spike.** The facts an elevated spike would have measured are acceptance criteria of the elevated gate instead (net_ac_008 to net_ac_010).
12. **Doc-only gate first.** The code is the next gate (a review branch, relay rule 5) and includes the elevated gate.

## References

- [ADR-0018](../adr/0018-cges-network-activity-v0-1.md) — the per-class decisions.
- [SPEC-005](SPEC-005-agent-process-telemetry-windows-etw.md) — the capture infrastructure and its deferral of other providers.
- [SPEC-017](SPEC-017-agent-capture-normal-run-path.md) — the delivery loop and the wire contract this SPEC amends by scope.
- [SPEC-018](SPEC-018-detection-read-model-arrival-cursor.md) — the arrival cursor; the read-model relied on unchanged.
- [ADR-0008](../adr/0008-etw-crate-selection.md), [ADR-0009](../adr/0009-event-delivery-and-buffer.md), [ADR-0010](../adr/0010-agent-privilege-model-mvp.md), [ADR-0011](../adr/0011-cges-process-activity-v0-1.md) — the crate, delivery, privilege and the `process.uid` recipe.
- [roadmap](../product/roadmap.md) — §D.
- `agent/cg-agent/src/etw/`, `agent/cg-agent/src/cges/`, `agent/cg-agent/src/delivery.rs`, `agent/cg-agent/src/envelope.rs` — the agent code this SPEC governs.
- `services/ingest/src/schemas.ts`, `services/ingest/src/routes/heartbeat.ts`, `services/ingest/src/db/migrate.ts` — the server code this SPEC governs.
- `schemas/cges/v0.1/classes/4001_network_activity.json`, `schemas/cges/v0.1/event.json` — the schema files this SPEC updates.
