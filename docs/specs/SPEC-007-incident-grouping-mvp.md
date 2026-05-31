# SPEC-007: Incident grouping MVP — alert correlation into incidents (event-time windowed, create-or-update with triage preservation)

- **ID:** SPEC-007
- **Title:** Incident grouping MVP
- **Status:** Proposed
- **Depends on:** ADR-0013 (the windowing-basis contract this SPEC realises — event-time, own window distinct from the dedup bucket; not re-decided here), ADR-0012 (the alert pipeline + dedup mechanism this SPEC extends; the `alerts` table it reads + augments), ADR-0003 (incidents → Postgres-only, §Retention), ADR-0005 (detection philosophy; per-org configuration surface for the window), ADR-0006 (CGES/OCSF; the Incident class 10002), ADR-0011 (Process Activity / the MITRE tactics grouping keys on), SPEC-006 (the producer of the `alerts` rows this SPEC groups), SPEC-004 (the ingest service hosting the slice)
- **Authors:** Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)
- **Created:** 2026-05-31
- **Last updated:** 2026-05-31

## Motivation

SPEC-007 is the next link in the blueprint pipeline (`collect → normalize → … → score → alert → incident → …`, [Blueprint](../product/blueprint.md) §9): it groups **distinct, correlated alerts** into **incidents**. SPEC-006 emits alert-level only and explicitly deferred incidents (SPEC-006 §Out of scope: "Incidents + alert grouping … the incident assertion is deferred"). This SPEC closes that deferral for the MVP.

It is the production realisation of ADR-0013 (incident correlation windowing). It re-decides nothing from ADR-0013: it inherits the **event-time basis** (group by when activity *occurred*, not when it was detected) and the **own-window** decision (a correlation window distinct from and wider than the 300 s dedup bucket of ADR-0012 §5/§8). This SPEC derives the verifiable acceptance criteria, the `incidents` table, the `alerts.event_time` column ADR-0013 §Consequences requires, the `incident.json` MITRE field, the create-or-update lifecycle, and the concrete window value ADR-0013 §2 left to the SPEC.

The detection slice stays hosted transitorily in TypeScript at `services/ingest/src/detect/` (ADR-0012 §1); incidents persist to Postgres only (ADR-0003 §Retention — incidents are mutable triage state, like alerts). Grouping runs in the **same `runDetectionCycle` seam** as Phase 5 (`services/ingest/src/detect/index.ts:22-45`), as a sibling step after alert persistence (§Operational §6).

## Scope

### In scope

The preconditions are listed in **dependency order** (each builds on the prior):

1. **`alerts.event_time` column** (root; migration `0004_alerts_event_time`). The event-occurrence timestamp ADR-0013 §1 windows on — materialized on `alerts`, which today has no event-time column (only `created_at`/`updated_at`, both `now()`; `0002_alerts.ts:43-44`). §Data contracts specifies the column, its at-write population, and the backfill of pre-existing rows.
2. **`cg_mitre` field on `incident.json`** (additive, backward-compatible). The grouping tactic must persist on the incident; `incident.json` declares no such field today (it would survive only via `additionalProperties: true`, unacceptable for a load-bearing field). §Data contracts specifies the additive schema field.
3. **`incidents` table** (migration `0005_incidents`, Postgres-only) and the **create-or-update grouping** mechanism (declarative `grouping_key` + `ON CONFLICT DO UPDATE`, mirroring SPEC-006's dedup), with **human-triage preservation** (a new alert never resets `status`/`assigned_to`). §Data contracts + §Operational.
4. **`incidents.agent_id → agents` FK**, production-faithful (Convention #12): a synthetic test tripping it is fixed by enrolling the agent, never by weakening the constraint.
5. **The correlation window value** (NFR-007-001): a named constant, event-time windowed, `> 300 s` dedup bucket per ADR-0013 §2.
6. **Write seam**: a sibling step inside `runDetectionCycle` after a newly-inserted alert (§Operational §6).
7. **Acceptance criteria** (harness-first RED to be authored at the next gate): the five ACs in §Acceptance criteria, the first of which closes the gate-zero "proved by code, not by a persisted row" nuance.

### Out of scope

Each with a named destination:

- **Notification / email** (the SOC "receives an email with the summary" of Blueprint §18). A future notifier slice (Blueprint §16 "notifier abstraction with SMTP fallback"); needs SMTP/Gmail credentials (ask-first). Not this SPEC.
- **SOAR playbook engine** (`services/soar/`). Roadmap Phase 4 (Blueprint §15); a future `SPEC-XXX-soar`.
- **Stateful multi-event correlation beyond single-tactic grouping** (attack-sequence detection, multi-step joins, the rule+ML hybrid join). Deferred per ADR-0012 §Out of scope; inherits ADR-0013 §1's event-time basis when it lands.
- **The `user` grouping dimension.** Blueprint §9 groups by "host/user/MITRE tactic"; v0.1 has **no user identity** (`subject_user_sid` is structurally empty — the Kernel-Process provider emits no user SID, ADR-0012 §3, [handoff-session-17.md](../handoff-session-17.md) §Known follow-ups). v0.1 grouping therefore collapses to **host (agent) + tactic + window**, without `user` (§Operational §3). Destination: revisited when a user-SID-bearing provider lands (the same forward path as ADR-0012 §5's `dedup_key` subject omission).
- **Alert/incident drill-down and the alert→source-event navigation** (the v4/v7 `event_id` reconciliation it would touch). Part of C (the user-facing slice) and ADR-0009/0011 domain; not this SPEC.
- **The SOC dashboard / WebSocket push.** Later phase (C); Blueprint §13.
- **ML / UEBA contribution to incidents.** Forward-compatible only; no models (ADR-0005 §Out of scope).
- **Incident severity / priority rollup, MITRE-technique aggregation views, incident titles beyond a deterministic default.** Cosmetic/derivable; a dashboard-era concern.

## Data contracts

### 1. `alerts.event_time` — the windowing basis (migration `0004_alerts_event_time`)

ADR-0013 §1 binds incident grouping to **event-time**, not insert-time. The `alerts` table has no event-time column today; this SPEC adds one.

- **Column:** `event_time timestamptz NOT NULL` on `alerts`. Semantics: the originating event's **occurrence** time (UTC), i.e. the `time` of the child `cges_events` row that produced the alert — the *same* value SPEC-006's `buildDedupKey` already consumes (`services/ingest/src/detect/alerts.ts:21`, `match.sourceEvent.time`). Distinct from `created_at`/`updated_at` (insert/update time).
- **At-write population (new rows):** set from the source event's `time` at alert insert. The value is already in hand at persist time (`match.sourceEvent.time`), so there is **no ClickHouse round-trip** — the population is local to the existing `upsertAlert` path. (Wiring the `INSERT` column is an implementation concern for the RED→GREEN gate; this SPEC fixes the contract.)
- **Backfill of pre-existing rows (the delicate decision — named, not implicit):** existing `alerts` rows have no stored event-time. The migration reconstructs `event_time` **from the `dedup_key` bucket** — the contract: `dedup_key`'s last `::`-delimited component is `bucket_5min = floor(unix_seconds(time) / 300)` (ADR-0012 §5; built at `alerts.ts:22-24`), so the bucket component × 300 s, read as a UTC timestamp, recovers the event time **floored to the 5-minute dedup bucket**. Migration order: add the column nullable → backfill it from the bucket component → set `NOT NULL`. (The exact `split_part`/`to_timestamp` SQL is an implementation concern for the RED→GREEN gate, like the at-write wiring above; this SPEC fixes the reconstruction *contract*, not the statement body.)
  - **Why the bucket, not a ClickHouse join:** self-contained in Postgres (no cross-store dependency, no ClickHouse availability requirement, no `FINAL` semantics during a schema migration), and **consistent with the event-time basis the dedup already uses** (`alerts.ts:16-18`). The 5-minute coarseness is well within the correlation window's tolerance (NFR-007-001 ≥ 1800 s ⇒ ≥ 6× the bucket).
  - **Validity:** `dedup_key` is `NOT NULL` and always bucket-formatted because only the detection slice writes `alerts` rows in v0.1 (`buildDedupKey` is the sole producer). The backfill assumes that v0.1 invariant and states it.
  - **Alternatives (rejected, named):** (a) join `source_events` → ClickHouse `cges_events.time` for the exact time — precise but couples a Postgres migration to a cross-store `FINAL` query; rejected for migration simplicity, retained as the path if 5-minute coarseness ever proves insufficient for historical rows. (b) nullable column + forward-only population (pre-existing rows stay `NULL`, excluded from grouping) — rejected because `NOT NULL` is a cleaner windowing invariant and the bucket backfill is cheap and deterministic.

### 2. `incident.json` — add the `cg_mitre` grouping field (additive)

`incident.json` (`schemas/cges/v0.1/classes/incident.json`) declares `class_uid 10002`, `category_uid 10`, `cg_kind "incident"`, `activity_id` enum `[0,1,2,3,4,5,6,99]`, `incident_id` (UUIDv7), `title`, `status` enum `[open,acknowledged,investigating,contained,resolved,false_positive]`, `alert_ids` (array of UUIDv7, `minItems:1`, `uniqueItems`), `assigned_to`, `created_at`, `updated_at`. It carries **no MITRE field** — the grouping tactic would survive only via `additionalProperties: true`, which is unacceptable for a load-bearing grouping key.

- **Add** an optional `cg_mitre` property that **`$ref`s the canonical `../common/cg_mitre.json`** — the shape the alert's MITRE block already uses — so the incident field genuinely *mirrors* it by construction rather than re-stating a looser inline copy. The canonical schema sets `additionalProperties: false`, both `tactics`/`techniques` `required` + `uniqueItems: true`, and the technique `pattern ^T[0-9]{4}(\.[0-9]{3})?$`; `$ref` inherits all of it. Relative-path `$ref` is the established convention (`schemas/cges/v0.1/README.md` §Conventions; e.g. `event.json` → `common/cg_mitre.json`):

  ```json
  "cg_mitre": { "$ref": "../common/cg_mitre.json" }
  ```

- **Additive and backward-compatible:** not added to `required` (so existing fixtures, if any, still validate); the incident's top-level `additionalProperties: true` is **left untouched** (the known CGES permissive-class / `oneOf`-wrapping gotcha is respected, not "fixed"). The incident's `cg_mitre` is the **canonical tactic-set** the grouping keyed on (§Operational §3).

### 3. `incidents` table (migration `0005_incidents`, Postgres-only)

Per ADR-0003 §Retention (incidents → Postgres) and the class-vs-table split SPEC-006 established for `alerts` (the table is a superset of the class schema, adding `org_id`/`agent_id`/grouping columns). Lands as a Kysely migration extending the chain `0001_initial` → `0002_alerts` → `0003_detect_watermark` → `0004_alerts_event_time` → **`0005_incidents`** (numbering verified free), under the same `pg_try_advisory_lock` applier.

```sql
CREATE TABLE IF NOT EXISTS incidents (
  incident_id   uuid        PRIMARY KEY,                 -- UUIDv7, slice-generated (like alert_id)
  org_id        text        NOT NULL DEFAULT 'default',
  agent_id      uuid        NOT NULL REFERENCES agents (agent_id),   -- production-faithful FK (precondition 4)
  category_uid  smallint    NOT NULL DEFAULT 10,
  class_uid     integer     NOT NULL DEFAULT 10002,      -- CGES Incident
  cg_kind       text        NOT NULL DEFAULT 'incident',
  activity_id   smallint    NOT NULL DEFAULT 1,          -- 1 = Created
  title         text        NOT NULL,
  status        text        NOT NULL DEFAULT 'open'
                  CHECK (status IN ('open','acknowledged','investigating','contained','resolved','false_positive')),
  cg_mitre      jsonb,                                   -- the canonical grouping tactic-set { tactics:[], techniques:[] }
  alert_ids     uuid[]      NOT NULL,                    -- >= 1; the grouped alerts (uniqueItems)
  assigned_to   text,                                    -- nullable human triage
  grouping_key  text        NOT NULL,                    -- declarative correlation key (see below)
  window_start  timestamptz NOT NULL,                    -- event-time bucket start (the window this incident covers)
  created_at    timestamptz NOT NULL DEFAULT now(),
  updated_at    timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT incidents_grouping_key_unique  UNIQUE (grouping_key),
  CONSTRAINT incidents_alert_ids_nonempty   CHECK (cardinality(alert_ids) >= 1)
);
```

The `status` enum mirrors `incident.json`; `activity_id` default 1 (Created) mirrors the alert pattern; `alert_ids` is the denormalized UUID array (not an FK — matches `incident.json`'s `alert_ids`); `cg_mitre` persists the grouping tactic-set (§Data contracts §2). `window_start` records the event-time window the incident covers (the human-readable companion to the bucket encoded in `grouping_key`). The `UNIQUE (grouping_key)` constraint is what makes create-or-update declarative (§Operational §5).

### 4. `grouping_key` format

```text
grouping_key = "<org_id>::<agent_id>::<canonical_tactics>::<window_bucket>"
canonical_tactics = sort(unique(alert.cg_mitre.tactics)) joined by ","     -- e.g. "execution,initial-access"
window_bucket     = floor(unix_seconds(alert.event_time) / INCIDENT_CORRELATION_WINDOW_SECONDS)
```

- `agent_id` is the per-endpoint key (the host substitute; v0.1 has no hostname — ADR-0012 §5). `org_id` scopes the tenant.
- `canonical_tactics` makes the tactic dimension deterministic: an alert's `cg_mitre.tactics` array is sorted, de-duplicated, and joined, so two alerts with the same tactic-set route to the same incident regardless of array order. For the MVP's single rule (`rule.office_spawns_script_host`, tactics `[execution, initial-access]`) every alert shares one canonical token — the v0.1 grouping is exercised end-to-end by one rule.
- `window_bucket` embeds the event-time window (analogous to SPEC-006's `bucket_5min` in `dedup_key`), enabling `ON CONFLICT (grouping_key) DO UPDATE` without a read-then-write race.

## Operational

### 1. Grouping is declarative, mirroring dedup

Incident grouping reuses SPEC-006's declarative-key mechanism, one level coarser. Where alert dedup is `INSERT … ON CONFLICT (dedup_key) DO NOTHING` (collapse *identical* re-fires), incident grouping is `INSERT … ON CONFLICT (grouping_key) DO UPDATE` (accrete *distinct* correlated alerts). No cross-alert query is needed: each alert's `grouping_key` deterministically routes it to its incident. The two keys are nested by coarseness:

- `dedup_key` = `agent::rule::process_name::bucket_5min` (fine — one row per identical re-fire).
- `grouping_key` = `org::agent::canonical_tactics::window_bucket` (coarse — one incident per agent+tactic-set+window).

Distinct alerts (different `process_name`, or different rules sharing a tactic-set) collapse to one incident; dedup operates *before* alerts are distinct rows, grouping *after*. They do not overlap (audit item 6).

### 2. Dedup vs grouping (stated to prevent conflation)

Dedup (ADR-0012 §5) prevents the *same* alert from being written twice. Grouping (this SPEC) joins *different* alerts into one incident. An alert is first dedup-collapsed at write (SPEC-006), then — if newly inserted — grouped (this SPEC). A dedup-collapsed re-fire (the alert already existed) is **not** re-grouped: it was grouped at its first insert.

### 3. The `user` dimension is unavailable in v0.1 (declared, not silently dropped)

Blueprint §9 groups by "same host/**user**/MITRE tactic in window." v0.1 has **no user identity**: `subject_user_sid` is structurally empty (the `Microsoft-Windows-Kernel-Process` provider emits no user SID — ADR-0012 §3, manifest-verified). The v0.1 `grouping_key` therefore **collapses to host (agent) + tactic-set + window, without `user`** — the same audit-grounded omission as ADR-0012 §5's `dedup_key` subject exclusion. Forward path: a `user` component is added when a user-SID-bearing provider lands.

### 4. Create-or-update lifecycle with triage preservation (the invariant)

When an alert is grouped:

- **No incident for its `grouping_key`:** a new incident is created — `incident_id` UUIDv7, `status = 'open'`, `activity_id = 1` (Created), `alert_ids = [alert_id]`, `cg_mitre` = the alert's canonical tactic-set, `window_start` = the bucket start, `title` = a deterministic default (e.g. `"<canonical_tactics> activity on <agent_id>"`).
- **An incident already exists for that `grouping_key`:** the alert's `alert_id` is appended to `alert_ids` (only if not already present — `uniqueItems` per `incident.json`) and `updated_at` is bumped. **The following MUST be preserved unchanged:** `status`, `assigned_to`, `activity_id`, `title`, `cg_mitre`, `incident_id`, `created_at`.

**Invariant (binding):** a new correlated alert entering an existing incident's window MUST NOT modify any human-triage field. If an analyst moved the incident to `investigating` or set `assigned_to`, a subsequent correlated alert leaves those values intact and only grows `alert_ids` + bumps `updated_at`. This is the incident analogue of SPEC-006's `DO NOTHING` status-preservation (§Operational §6 there), realised here as a *targeted* `DO UPDATE` whose `SET` list contains only `alert_ids` and `updated_at` — never a triage column. `activity_id` is set to `1` (Created) at incident creation and is **never modified by grouping** (it is in the preserve-list above); whether analyst-driven status transitions also advance `activity_id` (mirroring `status`) is a triage/dashboard concern (C), out of scope here.

### 5. Window boundary artifact (accepted, documented)

`window_bucket` is a floor bucket (like the dedup bucket), so two correlated alerts straddling a window-boundary edge fall in different buckets and produce two incidents. Accepted for the MVP — the same posture and rationale as ADR-0012 §5's dedup bucket-boundary artifact. Destination: a sliding-window correlation in a later increment if boundary splits prove noisy.

### 6. Write seam — sibling step in `runDetectionCycle`

Grouping runs **inside the existing detection cycle** (`services/ingest/src/detect/index.ts:31-40`), as a sibling step immediately after a *newly inserted* alert:

```text
for each event, for each rule:
  match = evaluateRule(rule, event)         # SPEC-006 5c
  finalScore = scoreAlert(match)            # SPEC-006 5d
  inserted = upsertAlert(...)               # SPEC-006 5e — true iff a NEW row (ON CONFLICT DO NOTHING)
  if inserted:
    upsertIncident(persisted alert)         # SPEC-007 — group the new alert
```

- Grouping rides the **new-alert** insert (`upsertAlert` returns `true`). A dedup-collapsed re-fire (`false`) is skipped — its alert was grouped at first insert (§Operational §2).
- `upsertIncident` consumes the persisted alert's `{ alert_id, org_id, agent_id, cg_mitre, event_time }`. Surfacing these from the `upsertAlert` path (today it returns only a boolean and generates `alert_id` internally, `alerts.ts:48,61`) is an implementation concern for the RED→GREEN gate; this SPEC fixes only that the grouping step receives them.
- **Rationale:** order-causal (the alert row exists before an incident references its `alert_id`), the **same TS seam and skill** as Phase 5 (no new service, no Go), and **no separate watermark** (grouping rides the alert write rather than re-polling). **Alternative (rejected, named):** a dedicated incident cycle with its own watermark over the `alerts` table — rejected for the MVP (a second poll loop + watermark for no benefit when grouping is declarative); named as the path if grouping must later consider alerts from non-detection sources or be independently re-runnable.

## Non-functional requirements

NFR identifiers scoped to this SPEC (`NFR-007-NNN`).

- **NFR-007-001 (correlation window).** `INCIDENT_CORRELATION_WINDOW_SECONDS = 1800` (30 minutes), a **named constant** (analogous to SPEC-006's `DEDUP_BUCKET_SECONDS = 300`, `alerts.ts:12`), **per-org configurable** (the per-org window surface ADR-0012 §8 establishes, extended per ADR-0013 §2). **Justification:** a macro→script-host→payload chain of *distinct* alerts unfolds over minutes to tens of minutes; 30 min groups a multi-step intrusion while staying under an hour (avoids conflating distinct user sessions or unrelated activity). It is `> 300 s` (6× the dedup bucket), satisfying ADR-0013 §2's "wider than dedup" requirement. Event-time windowed (ADR-0013 §1). This is the tunable most likely to need field calibration (§Open questions).
- **NFR-007-002 (grouping cost).** Grouping adds one `INSERT … ON CONFLICT` per newly-inserted alert within the existing cycle (no extra poll, no extra watermark). At MVP volume this is negligible relative to the SPEC-006 read-model + `FINAL` reads.
- **NFR-007-003 (idempotency).** Re-running the detection cycle over the same events MUST NOT change incident state beyond what the alerts already imply: dedup makes alert re-insertion a no-op, so no `upsertIncident` fires for a re-processed event; and `alert_ids` append is set-semantic (`uniqueItems`), so even a direct re-group is idempotent.

## Acceptance criteria

Each AC maps 1:1 to a test under `services/ingest/test/`: test file `incident-ac-NNN-<slug>.test.ts` (kebab-case, mirroring the realized `detect-ac-NNN-*.test.ts`) with logical identifier `incident_ac_NNN` (snake, as `detect_ac_NNN`). TypeScript/vitest; the slice is hosted in `services/ingest/src/detect/` per ADR-0012 §1. All five are **CI-able** on Linux `ts-ci` (synthetic alerts / synthetic events via testcontainers — incidents are downstream of alerts and need **no ETW**, so SPEC-007 has no developer-local marquee). The harness-first RED phase (next gate) turns `ts-ci` red with the Known CI debt co-located in that SHA (Convention #13), green when the impl lands.

| AC | Gate | Why |
| --- | --- | --- |
| incident_ac_001 | **CI-able** | `cg_mitre` on a *persisted* alert (closes gate-zero); synthetic events |
| incident_ac_002 | **CI-able** | grouping — N distinct alerts → 1 incident; synthetic |
| incident_ac_003 | **CI-able** | no over-group (distinct tactic / outside window); synthetic |
| incident_ac_004 | **CI-able** | triage preservation on append; synthetic |
| incident_ac_005 | **CI-able** | production-faithful FK; synthetic + enroll |

- **incident_ac_001 (cg_mitre is populated on a PERSISTED alert — closes the gate-zero nuance).** Given synthetic matching `cges_events` (an Office `ParentImage` parent + a script-host child) inserted into ClickHouse under the test's **unique `agent_id` + `org_id`** and one `runDetectionCycle` run, when the alert is persisted, then a `SELECT cg_mitre FROM alerts` scoped to that `(agent_id, org_id)` (e.g. `org_id = 'incident-ac-001'`, mirroring the per-test isolation of `detect-ac-003`/`004`) returns a **single** row whose `cg_mitre` is **non-null** `jsonb` with **non-empty** `tactics` and `techniques` (`tactics ⊇ {execution, initial-access}`, `techniques ⊇ {T1059.001, T1566.001}`) and `rule_id = 'rule.office_spawns_script_host'`. **This is the first test to assert the persisted `cg_mitre` column** — SPEC-006's `engine.test` asserted only the *parsed rule* (`engine.test.ts:43-44`), not the Postgres row; the audit's gate-zero verdict was "populated, proved by code-trace + zod invariant, not by a persisted row," and this AC closes that gap. (A bare `WHERE rule_id = …` predicate is non-isolating — the single-rule MVP makes it match every sibling test's alerts in the shared testcontainer Postgres; scope by `(agent_id, org_id)`.) **Gate: CI-able.**
- **incident_ac_002 (grouping — N distinct alerts → 1 incident).** Given `N = 3` **distinct** alerts (different child `process_name` ⇒ different `dedup_key`: `powershell.exe`, `cmd.exe`, `wscript.exe`) for the **same** `agent_id` and the **same** canonical tactic-set, with `event_time`s inside one correlation window, when grouping runs, then exactly **one** `incidents` row exists for that `grouping_key`, its `alert_ids` contains all three `alert_id`s (`uniqueItems`), `status = 'open'`, and `cg_mitre` is the canonical tactic-set. **Gate: CI-able.**
- **incident_ac_003 (no over-group — distinct tactic OR outside window → separate incidents).** Two sub-cases: (a) two alerts with the **same** agent+window but **different** canonical tactic-sets produce **two** incidents; (b) two alerts with the **same** agent+tactic-set but `event_time`s in **different** windows (one `> INCIDENT_CORRELATION_WINDOW_SECONDS` apart) produce **two** incidents. Synthetic alert rows with controlled `(agent_id, cg_mitre, event_time)` drive both. **Gate: CI-able.** (Exercises the event-time window basis from precondition 1.)
- **incident_ac_004 (triage preservation — a new alert does not reset human state).** Given an incident with `status = 'investigating'` and `assigned_to = 'analyst-x'` (one alert), when a new distinct alert with the same `grouping_key` is grouped, then the incident's `status` remains `'investigating'`, `assigned_to` remains `'analyst-x'`, `alert_ids` has grown by one, and `updated_at` advanced. **Gate: CI-able.** (Verifies the §Operational §4 invariant.)
- **incident_ac_005 (production-faithful FK).** Given the `incidents.agent_id → agents` FK, when grouping would create an incident for an `agent_id` with no `agents` row, then the `INSERT` fails the FK constraint — the constraint **doing its job**. The test satisfies the production precondition by enrolling the agent (`enrollTestAgent`, the SPEC-006 helper) — **never by weakening the FK** (Convention #12). **Gate: CI-able.**

**Migration coverage note.** A `0004_alerts_event_time` migration test (analogous to SPEC-006's `migration-0002-alerts.test`) verifies the column add, the at-write `NOT NULL`, and the dedup-bucket backfill of pre-existing rows (§Data contracts §1); a `0005_incidents` migration test verifies the table, the `UNIQUE (grouping_key)`, the FK, and the `alert_ids` cardinality `CHECK`. Both are CI-able.

## Test scenarios

Per ADR-0005 §Harness obligation. Incident scenarios extend the SPEC-006 SC catalogue:

- **SC-INC-001 — multi-alert incident (positive grouping).** Input: three distinct script-host alerts (`powershell`/`cmd`/`wscript`) from the same agent + tactic-set within one window. Expected: one incident, `alert_ids` cardinality 3, `status = open`. Realized by incident_ac_002.
- **SC-INC-002 — separation (no over-group).** Input: alerts differing by tactic-set or by window. Expected: separate incidents. Realized by incident_ac_003.
- **SC-INC-003 — triage stability.** Input: an `investigating` incident receives a new correlated alert. Expected: status preserved, `alert_ids` grows. Realized by incident_ac_004.

## Risks

| Risk | Mitigation |
| --- | --- |
| Backfilled `event_time` is coarse (5-min dedup-bucket floor) for pre-existing rows | Within the ≥ 1800 s window tolerance (§Data contracts §1); new rows carry exact event-time; ClickHouse-join backfill named as the path if coarseness ever bites |
| Window-boundary artifact (a chain straddling a window edge splits into two incidents) | Accepted, documented (§Operational §5); same posture as ADR-0012 §5 dedup; sliding-window deferred |
| Single-rule MVP ⇒ one tactic-set ⇒ grouping looks trivial | `canonical_tactics` + window + agent still exercise the full create-or-update path; incident_ac_003 forces the distinct-tactic and out-of-window cases with synthetic alerts |
| Multi-tactic alert routing (an alert whose `tactics` set differs from another's by one element groups separately) | v0.1 uses the **canonical tactic-set** as one token (deterministic, ON-CONFLICT-able); per-individual-tactic grouping (one alert → multiple incidents) is an §Open question, deferred |
| `user` dimension absent ⇒ two different users on one host group together | v0.1 limitation, declared (§Operational §3); forward path named; same as ADR-0012 §5 |
| `upsertAlert` signature change to surface `alert_id`/`event_time` to the grouping step | Implementation concern flagged (§Operational §6); contract fixed here, wiring at the RED→GREEN gate |

## Open questions

1. **Correlation window value.** `1800 s` (30 min) is the recommended MVP default (NFR-007-001) with rationale; field calibration may revise it. **Recommendation: 1800 s, per-org configurable.**
2. **Per-tactic-set vs per-individual-tactic grouping.** v0.1 groups by the canonical tactic-**set** (deterministic, declarative). Per-individual-tactic grouping (an alert with N tactics joining N incidents) is richer but breaks the one-alert→one-incident `ON CONFLICT` simplicity. **Recommendation: tactic-set for the MVP; revisit when the rule count grows.**
3. **End-to-end incident marquee.** detect_ac_001 (SPEC-006) proves ETW→alert developer-local; an incident marquee would extend it to assert the incident row. The five SPEC-007 ACs are CI-able (synthetic), so no dev-local marquee is required for the MVP. **Recommendation: defer an end-to-end incident marquee; the CI-able ACs + the SPEC-006 marquee cover the chain.**

## Ratification record

Load-bearing decisions for Manuel's gate. Recommended-default-and-rationale pattern per SPEC-003/005/006.

1. **`event_time` backfill from the dedup-bucket (the most delicate decision).** New alert rows carry exact event-time (from `match.sourceEvent.time`, at write, no round-trip); pre-existing rows are backfilled from the `dedup_key` bucket (`split_part(dedup_key,'::',4)::bigint * 300`), 5-minute-floored. Self-contained in Postgres, consistent with the dedup's existing event-time basis (`alerts.ts:16-24`), within the window tolerance. Alternatives (ClickHouse join for exactness; nullable+forward-only) named and rejected (§Data contracts §1). **This is the decision most needing architect review.**
2. **Correlation window = 1800 s, event-time, per-org.** Distinct from and 6× wider than the 300 s dedup bucket per ADR-0013 §2; rationale in NFR-007-001.
3. **Declarative `grouping_key` + `ON CONFLICT DO UPDATE`, mirroring dedup.** Create-or-update is declarative (no read-then-write race); the boundary artifact is accepted and documented like dedup's (§Operational §1/§5).
4. **Triage preservation invariant.** A new correlated alert never resets `status`/`assigned_to`/triage fields; the `DO UPDATE` `SET` touches only `alert_ids` and `updated_at` (§Operational §4). The incident analogue of SPEC-006's `DO NOTHING`.
5. **`user` dimension declared absent in v0.1.** Grouping collapses to host(agent)+tactic-set+window; `subject_user_sid` empty (ADR-0012 §3). Forward path named (§Operational §3).
6. **Same-seam, sibling-step write.** Grouping rides `runDetectionCycle` after a new alert; no Go, no new service, no separate watermark (§Operational §6). The Go extraction stays gated on the firehose ADR (ADR-0012 §1).
7. **Production-faithful FK (Convention #12).** `incidents.agent_id → agents`; a tripping synthetic test enrolls the agent, never weakens the constraint (incident_ac_005).

## References

- [ADR-0013](../adr/0013-incident-correlation-windowing.md) — the windowing-basis contract this SPEC realises (event-time §1; own window distinct from the 300 s dedup bucket §2). SPEC-007 re-decides none of it; it sets the window *value* ADR-0013 left to the SPEC and materializes the `alerts.event_time` ADR-0013 §Consequences requires.
- [ADR-0012](../adr/0012-normalize-before-correlate-pipeline.md) — the alert pipeline + dedup mechanism this SPEC extends one level coarser (grouping vs dedup); the `alerts` table (§6) it augments with `event_time`; the transitory TS seam (§1) grouping rides.
- [ADR-0003](../adr/0003-polyglot-storage.md) — incidents → Postgres-only (§Retention); no amendment needed (already Postgres-only).
- [ADR-0005](../adr/0005-detection-rules-and-ml-in-parallel.md) — detection philosophy; the per-org configuration surface the window reuses.
- [ADR-0006](../adr/0006-cges-ocsf-alignment.md) — CGES/OCSF; the Incident class 10002 and `cg_mitre` shape.
- [ADR-0011](../adr/0011-cges-process-activity-v0-1.md) — Process Activity 1007; the MITRE tactics the grouping keys on; the empty `subject_user_sid` (no `user` dimension).
- [SPEC-006](SPEC-006-detection-mvp.md) — the producer of the `alerts` rows this SPEC groups; the dedup mechanism (`ON CONFLICT DO NOTHING`) grouping mirrors; the `enrollTestAgent` helper and migration-test pattern the ACs reuse; the gate-zero `cg_mitre`-on-parsed-rule assertion incident_ac_001 extends to the persisted row.
- [SPEC-004](SPEC-004-server-ingest-minimal.md) — the ingest service hosting the slice; the testcontainers harness the CI-able ACs reuse.
- `schemas/cges/v0.1/classes/incident.json` — the Incident class this SPEC augments with `cg_mitre` (§Data contracts §2).
- `services/ingest/src/db/migrations/0002_alerts.ts` — the `alerts` table (`created_at`/`updated_at` are insert/update-time; no event-time column → migration `0004`).
- `services/ingest/src/detect/alerts.ts` — `buildDedupKey` (the event-time + bucket source the backfill reuses) and `upsertAlert` (the seam grouping rides).
- `services/ingest/src/detect/index.ts` — `runDetectionCycle`, the loop the grouping sibling-step joins.
- [Blueprint](../product/blueprint.md) §9 (the `alert → incident` link), §18 (the MVP's "grouped incident in the Incidents view").
- [MITRE ATT&CK](https://attack.mitre.org/) — the tactics (`execution`, `initial-access`) the grouping keys on.
