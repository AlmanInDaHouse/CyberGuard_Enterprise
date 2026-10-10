# ADR-0018: Per-class CGES jurisprudence — Network Activity v0.1 (Kernel-Network source, TCP connections opened, endpoints and process attribution)

- Status: Accepted
- Date: 2026-10-10
- Last updated: 2026-10-10
- Deciders: Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

Roadmap §D is MVP criterion 2: the agent captures network and logins besides processes. Observed at `b42317d`:

1. **The class exists only as a schema.** `schemas/cges/v0.1/classes/4001_network_activity.json` and one example are in the repo; no code under `agent/` or `services/` names class 4001. ADR-0012 §Context and §Out of scope call the class schema-only.
2. **The pattern is set.** ADR-0011 §1 gives each concrete CGES class its own jurisprudence ADR. ADR-0011 §Out of scope and SPEC-005 §Out of scope §1 defer the network provider to a per-class ADR and a successor SPEC. This ADR is the second instance of the pattern; SPEC-019 is that successor SPEC.
3. **The capture crate was chosen with this in mind.** ADR-0008 §Decision adopts ferrisetw for "process, file, network, registry, image-load", and ADR-0011 §6 foresees "future Kernel-Network events tagged with the same process" sharing `process.uid`.
4. **The Blueprint names another source.** Its §15 lists "network (WFP passive filters)" for the agent, and the roadmap declares §15 superseded for planning. Its §18 MVP acceptance criteria ask for an agent "with: processes, basic network, logins" and do not say what basic network covers.
5. **Storage was decided for one class.** Decision D6 (`docs/handoff-session-10.md`) — one `cges_events` table with a `class_uid` discriminator — was ratified for SPEC-005's scope. No ADR or SPEC extends it to a second class.
6. **What Windows declares.** Read from the provider's metadata, without elevation, on Windows 10.0.26200 (S34): `Microsoft-Windows-Kernel-Network` (`{7DD42A49-5329-4832-8DFD-43D979153A88}`) has the keywords `0x10` (IPv4) and `0x20` (IPv6). Its TCP events over IPv4 are 10 (data sent), 11 (data received), 12 (connection attempted), 13 (disconnect), 14 (retransmit), 15 (connection accepted), 16 (reconnect) and 18 (copy); over IPv6 they are 26–32 and 34, in the same order. Event 17 (a failed connection) carries only a protocol and a failure code. The UDP events are 42 and 43 (sent and received, IPv4) and 58 and 59 (IPv6). Events 12, 15, 28 and 31 carry `PID`, `saddr`, `daddr`, `sport` and `dport`; the addresses are declared as a 32-bit integer for IPv4 and as binary for IPv6. No event carries an image name, a user or a direction, and the provider has no event for a listening socket or a DNS query.
7. **What was not measured.** No elevated run of this provider preceded this ADR (S34). Five facts are open: the byte order of the ports and addresses; which of `saddr` and `daddr` is the local endpoint of an accepted connection; whether `PID` is the connecting process on 12 and 28 and the accepting process on 15 and 31; whether ETW honours a filter by event id for this provider; and whether event 12 is also written for an attempt that fails. §10 says how each is settled.

## Decision

### 1. Source

The agent reads TCP connections from `Microsoft-Windows-Kernel-Network`, through ferrisetw, as a second provider of the ETW session it already runs for Kernel-Process. There is no second session, no other capture API and no raw Win32 call (ADR-0008 §Compliance), and the agent needs no privilege beyond the one that session already requires (ADR-0010 §Compliance).

### 2. Scope of v0.1 — connections opened

The agent emits one event when Windows reports a TCP connection that the host opened or accepted, over IPv4 or IPv6: provider events 12 and 28 (outbound), 15 and 31 (inbound). All four are `activity_id = 1` (Open).

- The class file's `activity_id` enum stays OCSF-permissive; the agent emits only `1`. This is the dual layer of ADR-0011 §3: a permissive schema and a narrower agent.
- What Open means here is "Windows reported the connection". For an inbound event the connection was accepted. For an outbound event Windows' own name is "connection attempted", and this ADR does not claim that the peer answered (§10).

### 3. Field mapping

The wire element uses these paths as written (SPEC-019 §Data contracts).

| CGES path | Source | Notes |
|---|---|---|
| `class_uid` | — | `4001`. |
| `activity_id` | the event id | `1` for events 12, 15, 28 and 31. |
| `time` | the event header's timestamp | String-encoded Unix nanoseconds (§6). |
| `connection_info.protocol_name` | — | `tcp`. |
| `connection_info.direction` | the event id | `outbound` for 12 and 28; `inbound` for 15 and 31 (§4). |
| `src_endpoint.ip`, `src_endpoint.port` | `saddr`, `daddr`, `sport`, `dport` | The initiator's address and port (§4). |
| `dst_endpoint.ip`, `dst_endpoint.port` | `saddr`, `daddr`, `sport`, `dport` | The acceptor's address and port (§4). |
| `actor.process.pid` | `PID` | The local process the connection belongs to (§5). |
| `actor.process.uid` | the agent's creation-time cache | The ADR-0011 §6 recipe, when the agent knows the process (§5). |

### 4. Endpoints and direction

- `src_endpoint` is the endpoint that initiated the connection and `dst_endpoint` the one that accepted it, in both directions. On an outbound event `src_endpoint` is the host's local address and port and `dst_endpoint` is the remote peer's; on an inbound event `src_endpoint` is the remote peer's and `dst_endpoint` is the host's local address and port.
- So the two events of one connection — reported by the agents at both ends, or by one agent for a connection between two local processes — carry the same `src_endpoint` and `dst_endpoint`, and differ in `direction` and in the process.
- `direction` is the host's point of view. The agent emits `outbound` or `inbound`, never `lateral` or `unknown`: it does not know the network's layout.
- `ip` is text: dotted decimal for IPv4, the RFC 5952 form for IPv6. An IPv4-mapped IPv6 address is emitted as the IPv4 address, so one peer has one spelling. `hostname` is never emitted: the agent resolves no names.
- How the provider's `saddr`, `daddr`, `sport` and `dport` are assigned to these endpoints, and their byte order, are fixed by outcome: the emitted values equal the real ones of the connection (§10).

### 5. Process attribution

- `actor.process.pid` is the provider's `PID`: the process that connected, or the process that accepted. It is always present.
- `actor.process.uid` is built with the ADR-0011 §6 recipe from the creation time the agent holds for that PID (the cache of SPEC-005 §Operational §2), so a connection carries the same uid as the Launch of its process. When the agent holds no creation time for the PID — the process started before the agent, for instance — `uid` is omitted. It is never built from another timestamp.
- The event carries no process name, image path or user: the provider has none. A consumer reaches them through the process's Launch event.

### 6. `time`

`time` is the event header's timestamp as string-encoded Unix nanoseconds, the encoding ADR-0011's Amendment 2026-05-28 fixed for class 1007. This is the per-class format that `schemas/cges/v0.1/event.json` calls authoritative. Every class the agent emits uses one encoding: the ingest route converts one form, and events of different classes order against each other as they are.

### 7. No raw payload

A Network Activity event carries neither `raw_data` nor `cg_raw_ref`. Every field the class takes from the source event is a typed field; the rest of the provider's payload (TCP options, window sizes, the sequence number) is not kept.

### 8. The agent's own connections

A connection whose `PID` is the agent's own process is not reported.

- **Why.** A POST to the server can open a connection; reported, that connection becomes an event, which becomes a POST. On an idle host the agent would report itself indefinitely.
- **The cost.** A connection made from inside the agent's process is not visible in this telemetry. This ADR accepts that for v0.1.

### 9. Storage — one table

D6 is extended to this class: Network Activity events are rows of `cges_events`, told apart by `class_uid`, and the table gains the columns the class needs (SPEC-019 §Data contracts).

- **Why.** One arrival order across classes (the `(arrived_at, event_id)` cursor of SPEC-018), one partitioning and one duplicate collapse; and whatever later joins a connection to its process reads one table.
- **The obligation it creates.** `activity_id` values belong to their class: `1` is Launch in 1007 and Open in 4001. A reader that interprets a row's `activity_id` or its class-specific columns MUST select by `class_uid`. The forensic drill is the one reader this ADR excepts, on the condition that follows.
- **The exception.** The forensic drill interprets the rows it reads and has no class filter: it reads the events whose `event_id` an alert cites. It stays correct while every cited event is of one class, which holds today: every rule is a process rule. Before a rule cites an event of another class, the drill's row must carry `class_uid` (SPEC-019 §Out of scope).

### 10. Settled at the gate

The facts of §Context 7 are fixed by SPEC-019's acceptance criteria on real ETW, in the elevated gate, by outcome: the endpoints and processes the agent emits equal those of connections the tests make.

- If that gate contradicts a statement of this ADR — the provider reports nothing for a loopback connection, say, or `PID` is not the accepting process — this ADR is amended before the code lands.
- Whether event 12 is written for an attempt that fails is recorded at that gate, not asserted. Until it is known, an outbound Open means "attempted".

## Alternatives considered

### A1 — The Windows Filtering Platform

The Blueprint's §15 names it. Reaching it means a callout driver, which is kernel code, or the platform's audit events in the Security log, which exist only when an audit policy enables them.

Rejected for v0.1. A driver is outside the elevated-user posture of ADR-0010. The audit events depend on a policy the agent does not set, and reading the Security log is the business of the authentication class (roadmap §D), not of this one.

### A2 — The NT Kernel Logger's TCP/IP events

Rejected: a second trace session, of another kind, with its own name, reclaim and loss counter, for connections the manifest provider already reports in the session the agent has.

### A3 — UDP and DNS in v0.1

Rejected. The provider's UDP events are sends and receives: there is no connection to report, so they need an aggregation that nothing consumes yet. DNS needs another provider and a CGES class that `schemas/cges/v0.1/` does not have. DNS is the first follow-up after roadmap §D.

### A4 — Close and traffic events

Events 13 and 29 would give each connection an end, and 10, 11, 26 and 27 its volume. Rejected: the first doubles the events and the second is one event per segment, and no consumer of a duration or a byte count exists in the MVP.

### A5 — `src_endpoint` is always the host's local endpoint

Rejected: a reader would need `direction` to learn who initiated, and the two ends of one connection would describe it with their endpoints swapped.

### A6 — A table per class

Rejected for the reasons of §9.

### A7 — `time` in ISO 8601

`event.json` describes ISO 8601 for classes other than 1007, and ADR-0011's Amendment 2026-05-28 allows it. Rejected: the source timestamp has a resolution of 100 ns, which the usual millisecond form drops; and the ingest route would need a second conversion beside the one it has.

### A8 — Reporting the agent's own connections, or excluding them by destination

Reporting them is the loop of §8. Excluding only the connections to the server would keep the rest visible, but the agent would have to resolve and track its server's addresses. Rejected for v0.1.

## Consequences

### Positive

- Network capture adds no session, no API, no privilege and no dependency to the agent: one more provider on the session, the crate and the posture already ratified.
- A connection is tied to its process by `process.uid`, the same value the process's Launch carries.
- Both ends of a connection describe it with the same endpoints.
- One table and one arrival order serve every class.

### Negative

- An event names its process only by PID and uid. Its image and user need the process's Launch, which the agent does not have for a process that started before it.
- The uid inherits the limits of the creation-time cache, which is keyed by PID: if the agent missed both a process's Terminate and the Launch of the process that reused its PID, a connection of the second process carries the uid of the first.
- An outbound Open may be an attempt that failed, until §10 settles it.
- Connections from inside the agent's process are not reported (§8).
- The ring is shared: a host that opens connections faster than the agent delivers them can push process events out of the ring (SPEC-019 §Risks).
- A reader of `cges_events` now depends on its `class_uid` filter, and the forensic drill on every alert citing events of one class (§9).

### Neutral

- UDP, DNS, closes, failures and traffic volume stay out (§Out of scope).
- The class file `4001_network_activity.json` keeps its required set; SPEC-019 adds the `actor` property to it and updates the description of `time` in `event.json`.

## Compliance

- The agent's Network Activity emission MUST follow §2–§8. Another activity, protocol or source needs an amendment to this ADR or a successor ADR, not agent code alone.
- The agent MUST NOT emit `actor.process.uid` unless it is the ADR-0011 §6 value for that process (§5).
- A reader that interprets a row's `activity_id` or its class-specific columns MUST select by `class_uid`. The forensic drill is excepted, on the condition of §9.
- A future per-class ADR follows the pattern of ADR-0011 §1, and stores its class in `cges_events` unless it argues otherwise.

## Out of scope

- UDP, DNS, connection close, reset and failure, traffic volume, and listening sockets.
- Hostnames, geolocation and threat-intelligence joins: the enrich stage (ADR-0012 §Out of scope).
- Detection rules over Network Activity, and any API or dashboard view of it (SPEC-019 §Out of scope).
- Capture on non-Windows platforms (ADR-0002 Rule 2).
- The authentication class 3002: its own per-class ADR (roadmap §D).

## References

- [ADR-0006](0006-cges-ocsf-alignment.md) — CGES alignment with OCSF v1.3; the framework this per-class ADR sits under.
- [ADR-0008](0008-etw-crate-selection.md) — ferrisetw, and the rule on raw Win32 calls.
- [ADR-0009](0009-event-delivery-and-buffer.md) — at-least-once delivery, the `event_id` and the in-memory ring, unchanged.
- [ADR-0010](0010-agent-privilege-model-mvp.md) — the elevated-user privilege model, unchanged.
- [ADR-0011](0011-cges-process-activity-v0-1.md) — the per-class pattern (§1), the dual layer (§3) and the `process.uid` recipe (§6).
- [ADR-0012](0012-normalize-before-correlate-pipeline.md) — the detection pipeline, which reads class 1007 only.
- [SPEC-019](../specs/SPEC-019-agent-network-telemetry-windows-etw.md) — the production specification of this ADR.
- [SPEC-005](../specs/SPEC-005-agent-process-telemetry-windows-etw.md), [SPEC-017](../specs/SPEC-017-agent-capture-normal-run-path.md), [SPEC-018](../specs/SPEC-018-detection-read-model-arrival-cursor.md) — process capture, delivery, and the arrival cursor.
- [roadmap](../product/roadmap.md) — §D.
- `schemas/cges/v0.1/classes/4001_network_activity.json`, `schemas/cges/v0.1/objects/network_endpoint.json`, `schemas/cges/v0.1/event.json` — the class, its endpoint object and the root schema.
- `docs/handoff-session-10.md` — decision D6.
- [OCSF v1.3 Network Activity class (4001)](https://schema.ocsf.io/1.3.0/classes/network_activity) — class semantics inherited.
