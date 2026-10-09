# SPEC-018: Detection read-model — arrival cursor (late events)

- **ID:** SPEC-018
- **Title:** Detection read-model — arrival cursor (late events)
- **Status:** Accepted
- **Depends on:**
  - SPEC-006 — amends **by scope** its §In scope read-model line, §Operational §1, NFR-006-002 and the wording of detect_ac_005, where §Data contracts and §Operational below differ. Scoring, dedup, status preservation and the other acceptance criteria are unchanged.
  - SPEC-016 — its §Operational §1 (parent resolution, by event time and per agent) is unchanged and relied on (§Operational §5).
  - SPEC-017 — relies on its §Operational §2–§3 (POSTs in order, a resent event byte-identical); delivers its §Out of scope item "Events that arrive after the detection watermark has passed their `time`".
  - SPEC-007 and SPEC-014 — incident grouping and notify-on-create are unchanged; a late event reaches them when it arrives (§Operational §4).
  - ADR-0012 — §7 amended in place on 2026-10-09 (the cursor), with the matching carve-out in §2 and §Compliance.
  - ADR-0006 — the dual timestamp: the arrival time read here is CGES `cg_ingested_at`, stored as `arrived_at`.
  - ADR-0009 — at-least-once delivery over `ReplacingMergeTree`.
  - `docs/product/roadmap.md` — §H, the phase this SPEC is the contract for.
- **Authors:** Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

SPEC-006 specified a read-model that polls `cges_events` forward by a `time` watermark. Observed at `7a9160a`:

1. **The cursor runs on the agents' clocks.** `readNewEvents` (`services/ingest/src/detect/read-model.ts`) selects `time > watermark` for the org, and `runDetectionCycle` (`services/ingest/src/detect/index.ts`) advances the watermark to the batch's maximum `time`. The watermark is one row per org (`detect_watermark`). An event whose `time` is not after the watermark when it arrives is never read.
2. **Every agent shares one org.** The heartbeat route writes `org_id = 'default'` for every event, and enrollment does the same for every agent, so "two agents in one org" is any two agents.
3. **Three ways to lose events**, the first observed in the code and the other two deduced from it:
   - Another agent delivers first: whatever a second agent delivers with a `time` at or before the first one's last evaluated event is skipped.
   - A clock runs ahead: the server does not bound an event's `time` (`services/ingest/src/schemas.ts` accepts any string; the route only converts it). The envelope check tolerates a `sent_at` up to 5 minutes off, so an agent whose clock is ahead within that tolerance keeps the watermark ahead of every other agent's events.
   - A clock steps back: a single agent then loses its own events until its clock passes the watermark again.
4. **Ties.** The read cuts with `LIMIT` after `ORDER BY time` and then advances to the maximum `time`: rows that share the boundary `time` and fall outside the limit are skipped.
5. **The arrival time exists.** `cges_events.arrived_at` is `DateTime64(3, 'UTC') DEFAULT now64(3)`. No writer supplies it, so ClickHouse assigns it. It is the stored form of CGES `cg_ingested_at` (ADR-0006 makes the dual timestamp mandatory).
6. **Measured by the advisor on ClickHouse 24.8.14.39** (S33, outside the repo's harness; the acceptance criteria pin what matters in CI):
   - All rows of one `INSERT` share one `arrived_at`: 1024 rows sent as `JSONEachRow` over HTTP got a single value.
   - `arrived_at` is fixed when the `INSERT` starts, and its rows become visible when it ends. A slow insert that started first became visible after a later, fast one, carrying the smaller `arrived_at`.
   - Under `FINAL` a resent event shows its newest `arrived_at`, and a filter on `arrived_at` applies after the collapse.
   - A `minmax` skip index on `arrived_at` prunes a read without `FINAL` (3 of 369 granules over 3.0 M rows) only when the bound on `arrived_at` is a predicate of its own; a tuple comparison alone prunes nothing. With `FINAL` and the default settings the index is not used.

## Scope

### In scope

- The forward read advances by arrival, with a cursor that survives ties (§Data contracts, §Operational §1).
- A settle margin that keeps the cursor behind inserts still in flight (§Operational §2).
- The forward read without `FINAL`; a match whose `dedup_key` already exists writes nothing (§Operational §3).
- Late events are evaluated whenever they arrive, with no lateness horizon (§Operational §4).
- The cursor's storage and its start after the upgrade (§Operational §6).
- A skip index on `arrived_at` (§Operational §7).
- The tests of §Acceptance criteria, in CI.

### Out of scope

Each item has its destination in brackets.

- Validating or bounding an event's `time` at ingest [a debt recorded in the S33 handoff].
- A batch the server answers with 500 for a reason other than connectivity, which the agent resends without bound (SPEC-017 §Operational §3) [a debt recorded in the S33 handoff].
- The driver's default interval, 10 000 ms in `services/ingest/src/config.ts` against the 5000 ms of NFR-006-001 [a debt recorded in the S33 handoff].
- Merging `cg_mitre` across the alerts of an incident: the first alert processed sets it (`services/ingest/src/detect/incidents.ts`) [a debt recorded in the S33 handoff].
- The driver's drain ceiling, ten batches per org per tick [a debt recorded in the S33 handoff].
- Classes other than Process Activity 1007 [roadmap §D, which reads through this cursor].
- A commit-ordered event log, which would replace the settle margin [the event-firehose ADR, ADR-0012 §Out of scope].
- Detection across several ingest instances [ADR-0012 Amendment 2026-06-07 §Scope, unchanged].
- Retention of `cges_events` [ADR-0003 §Retention, not implemented; unchanged].

## Data contracts

### Arrival time

`cges_events.arrived_at` is the arrival time: ClickHouse assigns it when the row is inserted, with its own clock, at millisecond precision. The ingest route does not send it. All the events of one POST are one `INSERT` and share one value.

### Cursor

The detection cursor of an org is the pair `(arrived_at, event_id)` of the last row read. It replaces the `time` watermark of SPEC-006 §Operational §1.

- It lives in the Postgres `detect_watermark` row of the org, as `last_arrived_at` (text: the ClickHouse `DateTime64(3)` string, default `1970-01-01 00:00:00.000`) and `last_event_id` (uuid, default the nil UUID). `last_time` is dropped.
- ClickHouse orders and compares both members. The service takes the cursor from the last row the query returned and never orders `event_id` values itself.
- `event_id` is unique per event (ADR-0009 §1), so the pair orders the events of one `INSERT` totally.

### Index

`cges_events` has a data-skipping index `ix_arrived_at` on `arrived_at`, of type `minmax` with granularity 1. The table's engine, partitioning and ordering are unchanged.

## Operational

### 1. Forward read

Amends SPEC-006 §Operational §1 by scope:

```sql
SELECT event_id, agent_id, activity_id,
       process_pid, process_uid, process_name, image_file_name,
       process_parent_pid, time, arrived_at
FROM   cges_events                              -- no FINAL (§3)
WHERE  org_id    = {org}
  AND  class_uid = 1007
  AND  arrived_at >= {cursor.arrived_at}        -- its own predicate (§7)
  AND  (arrived_at, event_id) > ({cursor.arrived_at}, {cursor.event_id})
  AND  arrived_at <= now64(3) - {settle}        -- the settle margin (§2)
ORDER BY arrived_at ASC, event_id ASC
LIMIT  {batch}
```

- After a batch is processed the cursor moves to the `(arrived_at, event_id)` of its last row. An empty read leaves the cursor where it was.
- The batch holds at most 1000 rows (NFR-018-002). Each row is evaluated as before: every rule against every event, Launch events only (SPEC-006 §Operational §1).
- The cycle's other steps (parent resolution, scoring, alert and incident upserts, notification) are unchanged.

### 2. Settle margin

The read takes only rows whose `arrived_at` is at least **5000 ms** older than ClickHouse's `now64(3)`: the same clock that assigned `arrived_at`, so no other host's clock enters the comparison.

- **Why.** `arrived_at` is assigned when an `INSERT` starts and its rows are visible when it ends (§Context 6). Without the margin, a cycle that reads between two overlapping inserts moves the cursor past the slower one and never reads it. The margin also makes the cursor safe across inserts that share a millisecond: every row with a given `arrived_at` is visible before any of them is read.
- **The assumption, stated.** An `INSERT` becomes visible within 5000 ms of its `arrived_at`. One that takes longer may be skipped. So may rows stamped after the ClickHouse host's clock steps back by more than the margin. Both are accepted for the MVP; the exit is a commit-ordered log (§Out of scope).
- **A constant, not an environment variable.** The production driver always uses 5000 ms. Tests may inject another value through the cycle's configuration.

### 3. Duplicates

The forward read does not use `FINAL`. A resent event (SPEC-017 §Operational §3) is a second row with a later `arrived_at`, so the cursor reaches that event a second time and it is evaluated again.

- A match whose `dedup_key` already exists writes nothing: no alert, no change to an incident, no notification. The `dedup_key` is made of `agent_id`, the rule's id, `process_name` and the 300 s bucket of the event's `time` (ADR-0012 §5): none of them depends on when the event arrived or was processed, so the same rule matching the same event gives the same key. The insert is `ON CONFLICT (dedup_key) DO NOTHING` (SPEC-006 §Operational §6); only a newly inserted alert is grouped (SPEC-007 §Operational §6), and only a created incident is notified (SPEC-014 §Data contracts §1).
- A second evaluation can match where the first did not: the rules are loaded on every cycle, and the parent is resolved from the events stored at that moment. Such a match is treated like any other: it writes its alert unless its `dedup_key` already exists.
- `eventsEvaluated` counts rows read, duplicates included.
- The parent look-back keeps `FINAL` (§5).

### 4. Late events

There is no lateness horizon: an event is evaluated when it arrives, however old its `time`.

- Everything derived from the event keeps the event's time: the alert's `event_time`, the `dedup_key` bucket and the incident window (ADR-0013 §1). The alert's `created_at` records when detection processed it, and its `updated_at` starts at the same instant.
- A late alert joins the incident of its event-time window, or creates it; a created incident is notified (SPEC-014), whatever the age of the event. Joining an incident sets that incident's `updated_at`, so the incident list, which the api orders by `updated_at`, shows an old incident as recently changed.
- Alerts and incidents are therefore written in arrival order, not in event-time order. `alert_ids` grows in that order. A client paging the alert list by `(event_time, alert_id)` may not see an alert that lands behind its cursor until it reloads.

### 5. Parent resolution

Unchanged (SPEC-016 §Operational §1): by event time, per child and per agent, independent of the cursor.

- The agent's ring is FIFO and its POSTs go out in order (SPEC-017 §Operational §2), so a parent's Launch travels in the same POST as its child or in an earlier one: it arrived no later than the child and, with the margin of §2, is visible when the child is read. This supposes that ETW dispatches the parent's Launch before the child's, which the repo does not establish across CPUs.
- The two parent queries span the batch from its oldest child, less the look-back, to its newest. A batch in arrival order may mix a late agent with a current one and widen that span: this costs time and changes no result, because each child keeps its own interval.

### 6. Cursor storage and upgrade

- Migration `0007` changes `detect_watermark` as §Data contracts states. It is idempotent, and its `down` restores `last_time` with its default.
- After the migration every org's cursor is at the beginning. The first cycles re-read the events already stored, at the driver's pace, and evaluate them as §3 states: a match whose `dedup_key` already exists writes nothing; any other match writes its alert, and may create an incident and notify, however old the activity. That covers the events that were never evaluated and the matches that did not exist when an event was first evaluated.

### 7. Index

- The bootstrap adds `ix_arrived_at` with an idempotent `ALTER TABLE cges_events ADD INDEX IF NOT EXISTS`, after its `CREATE TABLE IF NOT EXISTS`, which stays as it is. A table that already exists gets the index too.
- Parts written before the index are not rewritten; they are read without pruning until a merge rewrites them.
- The index changes cost only. The read returns the same rows without it.
- The read states the lower bound on `arrived_at` as a predicate of its own (§1), which is what lets the index prune (§Context 6).

## Non-functional requirements

- **NFR-018-001 (settle margin).** 5000 ms, a constant (§Operational §2).
- **NFR-018-002 (batch size).** Each cycle reads at most 1000 rows — Launch and Terminate alike, duplicates included. Amends NFR-006-002 by scope, which counted Launch events and advanced by `time`.
- **NFR-018-003 (latency).** The margin adds up to 5000 ms between an event's arrival and its evaluation. With the agent's 5000 ms batch (SPEC-017 §Operational §2) and the driver's default interval of 10 000 ms, an event is evaluated within about 20 s of its capture, plus processing: inside the 30 seconds of the Blueprint's MVP statement. NFR-006-005 is unchanged.

## Acceptance criteria

Each maps to a test named `late_ac_NNN_*` under `services/ingest/test/`. All run in `ts-ci` against testcontainers with synthetic events; none needs ETW.

- **late_ac_001 (another agent, earlier time).** Agent A's events are evaluated. Agent B's matching parent → child pair, with a `time` earlier than A's, is then inserted. The next cycle evaluates B's events and writes B's alert, whose `event_time` is the child's `time`.
- **late_ac_002 (one agent, its clock steps).** An event with a `time` one hour ahead of the server is evaluated. A matching pair inserted afterwards by the same agent, with a `time` at the server's present, is evaluated and raises its alert.
- **late_ac_003 (ties across the limit).** One `INSERT` with more rows than the read limit, all sharing one `arrived_at`, is read over successive reads: every `event_id` exactly once.
- **late_ac_004 (resend).** An event that was already evaluated is inserted again with the same `org_id`, `time` and `event_id`. The next cycle reads it; there is still exactly one alert, a status moved off `new` is preserved, and its incident is not updated.
- **late_ac_005 (settle margin).** Given a row older than the margin and a row younger than it, a cycle reads only the first and leaves the cursor on it; the second is read by a cycle whose margin it has passed. A cycle configured as the production driver configures it uses 5000 ms.
- **late_ac_006 (cursor and upgrade).** Migration `0007` applies twice without error and its `down` restores `last_time`. With events already stored and alerted, and the cursor at the beginning as the migration leaves it, the next cycle re-reads them, writes no second alert, and leaves the cursor on the last row read.
- **late_ac_007 (index).** After the bootstrap `cges_events` has `ix_arrived_at`, on a new table and on a table created without it; a second bootstrap changes nothing.
- **late_ac_008 (regression and gate).** The suites that run a detection cycle stay green. detect_ac_005 holds as amended: the second cycle evaluates only what was inserted after the first, whatever its `time`. In the elevated gate both marquees and the four real-ETW tests pass, and detect_ac_001 runs its cycle with the production margin.

## Test scenarios

No detection scenario changes. The SPEC-016 scenarios and fixtures stay green.

## Risks

| Risk | Mitigation |
| --- | --- |
| An `INSERT` becomes visible more than 5000 ms after its `arrived_at`, and its events are skipped | The assumption is stated (§Operational §2) and its value is §Open questions 1; the exit is a commit-ordered log |
| The ClickHouse host's clock steps back by more than the margin | Stated (§Operational §2); one host in the MVP |
| The re-read after the upgrade raises alerts and notifications for old activity | Intended (§Operational §6): each is a match that had no alert |
| An agent that reconnects after a long outage raises many late alerts at once | Each is real and carries its event time; a horizon or a notification policy is §Open questions 2 |
| The forward read is slower on parts written before the index, or where the bootstrap did not run | Cost only; the result is the same (§Operational §7) |
| Detection is up to 5 s later than before | Inside the Blueprint's 30 seconds (NFR-018-003) |

## Open questions

1. **The margin's value.** 5000 ms is a choice, not a measurement: no insert has been timed on a loaded server. **Reopen if** an insert is observed to take longer than 1 s, or the 30 s budget comes under pressure.
2. **A lateness horizon, or quieter notifications for old events.** None in the MVP. **Reopen if** a reconnecting agent's late alerts prove noisy in operation (roadmap §E).
3. **A commit-ordered source.** A log offset would replace both the margin and the arrival cursor. **Reopen with** the event-firehose ADR.

## Ratification record

Load-bearing decisions for Manuel's gate. Manuel delegated the three owner decisions (1–3) explicitly ("elige tú", 2026-10-09); the advisor decided, and Manuel's ratification of this SPEC ratifies them. Decisions 4–9 are the advisor's, in the reversible lane; they are recorded because the contract rests on them.

1. **The vehicle:** ADR-0012 §7 amended in place, and this SPEC amending SPEC-006 by scope.
2. **No lateness horizon:** an event is evaluated whenever it arrives, and may raise an alert, an incident and a notification for old activity.
3. **The cursor starts at the beginning after the upgrade:** the stored events are re-read once.
4. **A composite cursor** `(arrived_at, event_id)`, compared by ClickHouse.
5. **No `FINAL` on the forward read;** the `dedup_key` makes a repeated match a no-op. ADR-0012 §2 and §Compliance carry the carve-out.
6. **A settle margin of 5000 ms,** a constant, with its assumption stated.
7. **A `minmax` index on `arrived_at`,** added at bootstrap.
8. **`last_time` is dropped,** not kept beside the new columns.
9. **Doc-only gate first.** The code is the next gate (a review branch, relay rule 5) and includes the elevated gate.

Alternatives considered: a `time` watermark per agent (it does not survive a clock that steps back, and it relies on each agent delivering in `time` order); re-reading an overlap instead of waiting out a margin (every event evaluated several times, and the overlap competes with new rows for the batch limit).

## References

- [SPEC-006](SPEC-006-detection-mvp.md) — the detection MVP this SPEC amends by scope.
- [SPEC-016](SPEC-016-detection-rule-set-v1.md) — parent resolution, relied on unchanged.
- [SPEC-017](SPEC-017-agent-capture-normal-run-path.md) — in-order delivery and byte-identical resends.
- [SPEC-007](SPEC-007-incident-grouping-mvp.md), [SPEC-014](SPEC-014-incident-notification.md) — incident grouping and notification, unchanged.
- [ADR-0012](../adr/0012-normalize-before-correlate-pipeline.md) — amended in place on 2026-10-09.
- [ADR-0006](../adr/0006-cges-ocsf-alignment.md), [ADR-0009](../adr/0009-event-delivery-and-buffer.md), [ADR-0013](../adr/0013-incident-correlation-windowing.md) — the dual timestamp, at-least-once delivery, and event-time windowing.
- [roadmap](../product/roadmap.md) — §H (this SPEC), §D, §E.
- `services/ingest/src/detect/read-model.ts`, `services/ingest/src/detect/index.ts`, `services/ingest/src/db/migrate.ts`, `services/ingest/src/db/migrations/` — the code this SPEC governs.
