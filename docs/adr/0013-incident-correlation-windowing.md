# ADR-0013: Incident correlation windowing — event-time basis, distinct from the dedup bucket

- Status: Accepted
- Date: 2026-05-31
- Last updated: 2026-05-31
- Deciders: Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

ADR-0012 (normalize-before-correlate pipeline) owns the Detection MVP slice through `alert`: `cges_events` → normalize → Sigma rule → renormalized score → Postgres `alerts`. The next link in the blueprint pipeline (`collect → normalize → enrich → correlate → score → alert → incident → …`, [Blueprint](../product/blueprint.md) §9) is **incident** — grouping related alerts. Phase 6 (Route A continued) implements it as SPEC-007; this ADR settles the one architectural decision SPEC-007 must not be left to improvise: **what time basis the incident correlation window operates on, and how that window relates to the dedup bucket ADR-0012 already defined.**

A read-only audit (Session 18, Part 2 — the audit-first gate for Phase 6 A) surfaced the two repo-grounded facts that force this decision:

- **The `alerts` table has no event-time column.** Its native timestamps are insert/update-time only — `created_at` and `updated_at`, both `timestamptz NOT NULL DEFAULT now()` (`services/ingest/src/db/migrations/0002_alerts.ts`); neither is the event's occurrence time. That occurrence time is recoverable today only indirectly: from the `dedup_key` 5-minute bucket string (5-minute granularity, derived string — not a queryable timestamp) or by joining `source_events` back to ClickHouse `cges_events.time`.
- **ADR-0012's dedup window is event-time based on purpose.** `buildDedupKey` derives its bucket from the event's `time`, not `now()` (`services/ingest/src/detect/alerts.ts`: "The bucket is derived from the EVENT's `time` (not `now()`)"). The dedup contract is therefore already anchored to event time.

ADR-0012 §8 defined the correlation window as a single 300-second tunable and stated the dedup bucket and "the future hybrid join share one tunable"; ADR-0012 §Out of scope further says stateful multi-event correlation "Uses the §8 window." Incident grouping *is* a form of the stateful multi-event correlation those clauses anticipated — but the audit shows the §8 assumption (one shared 300 s tunable) does not survive contact with grouping: a 5-minute window is the right width to collapse *identical re-fires*, and the wrong width to *group a distinct-alert attack chain* that routinely spans more than five minutes. This ADR therefore decides the windowing basis and, honestly co-located, amends ADR-0012 §8's single-tunable framing (see the *Amendment to ADR-0012* block). It deliberately does **not** decide the window's numeric value — that is SPEC-007 tuning.

## Decision

### 1. Incident grouping windows on event-time, not insert-time

The incident correlation window is evaluated against each alert's **originating event time**, never against `created_at` (`now()` at INSERT). Rationale:

- **Consistency with dedup.** ADR-0012's `dedup_key` bucket is already event-time based (`alerts.ts`). Grouping on a different time basis than dedup would split one logical decision (when did this activity happen?) across two clocks.
- **Correctness under replay / backfill.** Insert-time conflates *when the activity occurred* with *when the detection cycle happened to process it*. For near-real-time cycles the two are close, but a replay, a backlog drain, or a watermark catch-up batch would group alerts by processing time and fabricate incidents that never co-occurred (or fail to group ones that did). Event-time is invariant to when detection runs.

This decision binds the *basis* only. The mechanism by which event-time becomes available on the alert row — a materialized column on `alerts` versus a per-grouping join back to ClickHouse — is explicitly SPEC-007's (§Out of scope); this ADR neither ranks nor pre-decides it.

### 2. The incident correlation window is its own tunable, distinct from and wider than the dedup bucket

Incident grouping uses a **dedicated correlation window**, separate from ADR-0012 §8's 300-second dedup bucket:

- **Dedup bucket (ADR-0012 §5/§8): 300 s, unchanged.** Collapses *identical* re-fires of the same `(agent, rule, process, 5-min bucket)` into one alert row. Its role and value are untouched by this ADR.
- **Incident correlation window (this ADR): its own value, wider than the dedup bucket.** Joins *distinct* correlated alerts (different processes, or different rules sharing a MITRE tactic) into one incident. An attack chain's distinct alerts routinely span more than five minutes; reusing the 300 s dedup bucket would fragment one incident into many.

This ADR fixes only that the window is **separate and `> 300 s`**. The **concrete value is not decided here** — it is SPEC-007 tuning, per-org configurable in the same spirit as ADR-0012 §8's per-org window surface and ADR-0005's "set in the SPEC … and revisable" framing.

### 3. Continuity with ADR-0012 (the "how it windows" of the deferred correlation)

This ADR is the windowing-basis decision for the stateful correlation ADR-0012 §8 / §Out of scope deferred. It does not re-open ADR-0012's pipeline shape, scoring, dedup mechanism, or the transitory TS seam; it adds the temporal-basis jurisprudence that incident grouping (SPEC-007) and every later correlation increment inherit. Because incident grouping contradicts ADR-0012 §8's explicit "share one tunable" / "Uses the §8 window" assumption, this ADR amends ADR-0012 §8 in place (co-located amendment block), additively and backward-compatibly: the 300 s dedup tunable is unchanged, so no ADR-0012 / SPEC-006 dedup test is affected.

## Alternatives considered

### A1 — Window on insert-time (`created_at = now()`)

Pros: zero schema change — `alerts.created_at` already exists; simplest possible grouping query. Cons: conflates occurrence time with detection-processing time; under replay/backfill/watermark-catch-up it fabricates or misses incidents; inconsistent with ADR-0012's event-time dedup basis. Rejected — the correctness failure under non-real-time processing is decisive, and detection is explicitly watermark-driven (ADR-0012 §7), so catch-up batches are a normal, not exotic, case.

### A2 — Reuse the 300 s dedup window for grouping

Pros: one tunable, matches ADR-0012 §8 as written; no new surface. Cons: 300 s is calibrated to collapse identical re-fires, not to span a multi-step attack chain of *distinct* alerts; a chain crossing the 5-minute mark fragments into N incidents — the exact failure incidents exist to prevent. Rejected — the dedup width and the correlation width are different quantities that happen to share units; conflating them (ADR-0012 §8's implicit assumption) is what this ADR amends.

### A3 — Recover event-time per grouping op via a join to ClickHouse `source_events`

Pros: no new `alerts` column; ClickHouse `cges_events.time` is the authoritative event time. Cons: couples the Postgres grouping path to a cross-store lookup on every grouping evaluation, rather than letting the alert row carry its own temporal key. **Not rejected — deferred to SPEC-007 as one of two viable mechanisms** for realising §1's event-time basis (the other being a materialized event-time column on the alert). This ADR fixes the *basis*, not the *mechanism*; SPEC-007 chooses. Either mechanism windows on event-time, never insert-time — which is the only thing this ADR binds.

### A4 — Derive event-time from the `dedup_key` bucket string

Pros: no new column; the bucket is already event-time derived. Cons: 5-minute granularity is far too coarse for a correlation window, and the bucket is an opaque derived string (`floor(unix_seconds/300)`), not a timestamp — querying or windowing over it is a parse-and-reconstruct hack. Rejected.

## Consequences

### Positive

- Incident grouping is correct under replay, backfill, and watermark catch-up — the same event-time basis ADR-0012's dedup already trusts, now extended to correlation.
- The dedup window and the correlation window are decoupled, each tunable to its own purpose; ADR-0012 §8's conflation is corrected honestly rather than inherited silently.
- C (alert/incident drill-down) and every future correlation increment (stateful multi-event sequences, the rule+ML hybrid join — all deferred in ADR-0012 §Out of scope / SPEC-006) inherit a single, settled temporal basis instead of each re-deciding it.

### Negative

- Event-time must be **made available on the alert for windowing** — it is not a column today (the alert's native timestamps are insert/update-time only, `created_at` and `updated_at`; the `dedup_key` bucket is coarse and derived). The mechanism (a materialized `alerts` column versus a per-grouping ClickHouse join) and its DDL, nullability, and backfill of pre-existing rows are SPEC-007's; this ADR creates the requirement, not the column.
- A second window tunable is one more thing to configure per org. Mitigated: it reuses ADR-0012 §8's per-org window-configuration surface rather than inventing a new configuration mechanism.

### Neutral

- This ADR does **not** amend ADR-0003. Incidents are already Postgres-only there (§Retention, "Cases / incidents → Postgres"), so SPEC-007's `incidents` table needs no retention amendment — recorded as a non-edge per the dep-graph convention (constraint-bearing edges only; an absence plus an explaining bullet is more informative than an invented edge).
- The window's numeric value, the `alerts` event-time column DDL, the `cg_mitre`/tactic field on `incident.json`, the create-or-update incident lifecycle, and the `incidents.agent_id` FK are all SPEC-007 concerns; this ADR is deliberately silent on them (§Out of scope).

## Compliance

- Incident grouping (SPEC-007) MUST window on event-time, never on `created_at`/insert-time (§1).
- The incident correlation window MUST be a tunable distinct from the 300 s dedup bucket and wider than it; its value is set in SPEC-007 and is per-org configurable (§2). The dedup bucket's 300 s value and role are unchanged.
- SPEC-007 MUST make each alert's originating event-time available for windowing — by a materialized column or a ClickHouse join, SPEC-007's choice — and MUST never window on insert-time (§1).
- Every later correlation increment that windows over alerts (stateful sequences, hybrid join) inherits §1's event-time basis unless a future ADR amends it.

## Out of scope

Each deferred item names its destination:

- **The correlation window's numeric value.** SPEC-007 tuning (this ADR fixes only "own tunable, `> 300 s`").
- **The `alerts` event-time column** (DDL, type, nullability, backfill of pre-existing rows). SPEC-007 — the additive migration that realises §1's basis.
- **`cg_mitre` / grouping-tactic field on `incident.json`.** SPEC-007 schema work (the audit found `incident.json` declares no MITRE field though §9 groups by tactic).
- **Incident lifecycle (create-or-update, status/assignment preservation on append).** SPEC-007; the `ON CONFLICT DO UPDATE` that appends `alert_ids` without clobbering human triage state (Convention #12).
- **`incidents.agent_id` FK to `agents`** (production-faithful per Convention #12). SPEC-007.
- **Stateful multi-event correlation beyond single-tactic grouping** (sequences, multi-step joins) and **the rule+ML hybrid join.** Still deferred per ADR-0012 §Out of scope; they inherit §1's basis when they land.
- **Alert surfacing (API/dashboard, drill-down) and packaging.** Later phases (C; the packaging SPEC per ADR-0010 §Decision part 2).

## Landing checklist (atomic on flip to Accepted)

When this ADR is ratified Proposed→Accepted, the same commit:

1. Flips the status header to `Accepted`.
2. Adds the *Amendment to ADR-0012* block (below) to `docs/adr/0012-normalize-before-correlate-pipeline.md` (before its §References).
3. Adds the catalog row to `docs/adr/README.md`.
4. Adds the dependency edge `0013 → 0012 (amends in part: ADR-0013 §2 amends ADR-0012 §8's single-tunable framing in place; does not supersede)` to `docs/adr/README.md` §Dependencies. The ADR-0003 non-edge is **not** added to the graph (dep-graph edges are constraint-bearing only, engineering-notes §Session 10 Conventions 6–7); it stays documented solely as this ADR's §Consequences > Neutral bullet.

## Amendment to ADR-0012 (co-located, lands atomically when this ADR is Accepted)

The following block is added to `docs/adr/0012-normalize-before-correlate-pipeline.md` (before its §References) in the same commit that flips this ADR to `Accepted`:

> **Amendment 2026-05-31 (ADR-0013): the correlation window is two distinct tunables, not one.** §8 defined a single 300 s correlation window and stated the dedup bucket and "the future hybrid join share one tunable"; §Out of scope stated stateful multi-event correlation "Uses the §8 window." ADR-0013 §2 supersedes that single-tunable framing where they differ: the **dedup bucket** keeps its 300 s value and role (collapsing identical re-fires — unchanged), while **incident/stateful correlation** uses its **own, wider window** (value set in SPEC-007, per-org configurable). ADR-0012 remains `Accepted`; this amendment is additive and backward-compatible — the 300 s dedup tunable and every dedup test are untouched. Rationale: a 5-minute bucket is calibrated to collapse identical re-fires, not to span a multi-step attack chain of distinct alerts; conflating the two widths would fragment one incident into many.

## References

- [ADR-0012](0012-normalize-before-correlate-pipeline.md) — Normalize-before-correlate pipeline. This ADR continues it at the `incident` link; §2 amends its §8 single-window framing (co-located amendment). The dedup bucket (§5) and event-time dedup basis are the consistency anchors for §1.
- [ADR-0005](0005-detection-rules-and-ml-in-parallel.md) — Detection rules and ML in parallel. Its §Consequences defers the correlation-window value to the SPEC and marks it revisable; this ADR fixes the window's *basis* (event-time) and *separateness* (distinct from dedup), not its value.
- [ADR-0003](0003-polyglot-storage.md) — Polyglot storage. §Retention already places incidents in Postgres-only; this ADR does **not** amend it (non-edge, per §Consequences > Neutral).
- [Blueprint](../product/blueprint.md) §9 — the `… → alert → incident → …` pipeline whose `incident` link Phase 6 A / SPEC-007 implements.
- `services/ingest/src/db/migrations/0002_alerts.ts` — the `alerts` table; `created_at = now()` is the insert-time §1 rejects as the window basis, and the absence of an event-time column is the §Consequences materialization requirement.
- `services/ingest/src/detect/alerts.ts` — `buildDedupKey` derives the dedup bucket from the event's `time`, the event-time precedent §1 extends to grouping.
