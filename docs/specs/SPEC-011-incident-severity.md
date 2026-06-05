# SPEC-011: Incident severity aggregation — MAX of member alerts

- **ID:** SPEC-011
- **Title:** Incident severity aggregation — `severity_id` = MAX of member alerts
- **Status:** Accepted
- **Depends on:** SPEC-007 (the `incidents` table, the `INSERT … ON CONFLICT … DO UPDATE` upsert, `IncidentGroupingInput`, and the triage-preservation invariant this SPEC re-words — `services/ingest/src/detect/incidents.ts:43-60`, `:70-90`); SPEC-006 / ADR-0012 §6 (`alerts.severity_id smallint NOT NULL`, OCSF ordinal 0–6 — `services/ingest/src/db/migrations/0002_alerts.ts:29,50`); SPEC-009 (the read-models extended — `services/api/src/read/types.ts:16-38`); SPEC-010 (this SPEC realises its named deferral *"Severity / score aggregation per incident … A future read/scoring increment"*, `docs/specs/SPEC-010-forensic-event-drill.md:32`); ADR-0013 §1 (event-time windowing — unchanged).
- **No new ADR.** This SPEC opens no service and no trust boundary: the aggregation lives entirely inside the existing detection write-path (`services/ingest/src/detect/`) and the existing read-slice (`services/api/src/read/`). It adds a column, an upsert clause, and a read-model field — the kind of additive change ADR-0011's per-class jurisprudence and the SPEC-only workflow already cover.
- **Authors:** Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

An incident groups N alerts (SPEC-007), but today it carries **no severity of its own**: the `incidents` table stores `cg_mitre` and `alert_ids` but no severity/score column (`services/ingest/src/db/migrations/0005_incidents.ts:23-44`), and severity lives only per-alert (`alerts.severity_id`, `0002_alerts.ts:29`). The SOC's incident list (`docs/product/blueprint.md:556` — *"Incidents | Active cases | Severity, hosts, MITRE, age"*) names severity as a first-class incident attribute, and SPEC-010 explicitly **deferred** incident severity with a named destination (`SPEC-010:32`). This SPEC is that increment: derive an incident's severity from its member alerts and surface it through the read-slice.

Two findings ground the design (forensic audit, Sessions 20–21):

1. **The CGES contract already models incident `severity_id`.** The canonical wire example carries `"severity_id": 5` on an incident (`schemas/cges/v0.1/examples/06_incident_grouped.json:11`), even though the JSON-Schema class does not yet declare it (`schemas/cges/v0.1/classes/incident.json:8-56`, `additionalProperties: true` permits it) and the Postgres table omits it. Adopting the **OCSF ordinal 0–6** scale — identical to `alerts.severity_id` (`0002_alerts.ts:50`, `CHECK severity_id BETWEEN 0 AND 6`) — closes this schema-vs-wire divergence rather than inventing a new scale.
2. **The blueprint prescribes no derivation semantics.** It lists incident severity as a *display* attribute (`blueprint.md:556`) and describes the incident phase only as *"Groups related alerts"* (`blueprint.md:390`) — there is no max/weighted/count rule anywhere. The blueprint's lone numeric use, a playbook example `incident.severity >= 8` (`blueprint.md:423`), is on a scale inconsistent with the OCSF 0–6 ordinal and is **out of scope** (see below). The aggregation semantics are therefore a design decision, taken here and ratified by Manuel.

## Scope

### In scope

1. **A `severity_id smallint` column on `incidents`** (OCSF ordinal 0–6), added by migration `0006_incident_severity.ts`, **backfilled** for existing incidents, then made `NOT NULL` with a range CHECK mirroring `alerts`.
2. **MAX aggregation, recomputed in the upsert.** The incident's `severity_id` is `GREATEST(incidents.severity_id, excluded.severity_id)` on the `ON CONFLICT … DO UPDATE` path — a new correlated alert can only *raise* an incident's severity, never lower it.
3. **`IncidentGroupingInput` gains `severityId: number`**, populated from the matched rule's always-present `severity_id` at the detection call site.
4. **The triage-preservation invariant is re-worded** (not weakened) to declare two field classes: machine-recomputed (`alert_ids`, `severity_id`) vs. preserved triage-state (`status`, `assigned_to`, `activity_id`, `cg_mitre`, `title`).
5. **The read-models gain `severity_id`** — `IncidentListItem` and `IncidentDetail` (`services/api`) project it; the dashboard's wire-contract types mirror it additively.
6. **Acceptance criteria** (§AC) as integration tests against the real upsert, the real migration, and the real read-API + backends (testcontainers).

### Out of scope — each with a named destination

- **A visual `Severity` column in the incidents table UI** (`dashboard/src/components/incidents-table.tsx`). The datum is delivered through the read-model and the dashboard wire-type, so the UI can render it whenever; the column is a cosmetic increment, not a data dependency. **Destination:** a dashboard polish increment.
- **Any 0–1 continuous incident score.** Alerts carry both an ordinal `severity_id` and a continuous `final_score` (`0002_alerts.ts:29,38`); this SPEC aggregates only the **ordinal severity**. An incident-level score (and its aggregation policy) is a separate scoring increment. **Destination:** a future read/scoring SPEC.
- **Reconciling the blueprint's `incident.severity >= 8` playbook scale** (`blueprint.md:423`) with the OCSF 0–6 ordinal. The blueprint example is internally inconsistent with the alert severity scale; harmonising the two is a blueprint-level correction. **Destination:** a blueprint reconciliation pass.
- **`cg_mitre` aggregation.** `cg_mitre` stays **frozen-at-creation** (first writer wins — `incidents.ts:85`, preserved on conflict); this SPEC does not touch it. The asymmetry (severity recomputes, `cg_mitre` does not) is deliberate and documented in §4. **Destination:** a `cg_mitre`-union SPEC if ever wanted.
- **Per-org / weighted severity policy.** MVP is unconditional MAX. **Destination:** a configurable-aggregation increment (mirrors the per-org scoring-weights deferral, `scorer.ts:3-5`).

## Data contracts

### 1. `incidents.severity_id` (the new column)

- **Type:** `smallint NOT NULL`, **OCSF severity ordinal 0–6**, with `CONSTRAINT incidents_severity_range CHECK (severity_id BETWEEN 0 AND 6)` — a byte-mirror of `alerts_severity_range` (`0002_alerts.ts:50`).
- **Meaning:** the **maximum** `severity_id` over the incident's member alerts (the alerts in `alert_ids`).
- **Population:** set on incident creation from the creating alert's `severity_id`; recomputed as `GREATEST(current, incoming)` whenever a new correlated alert joins (§3). Backfilled for pre-existing incidents from `MAX(alerts.severity_id)` over `alert_ids` (§Operational §1 / the migration).

### 2. `IncidentGroupingInput.severityId` (the grouping input)

`IncidentGroupingInput` (`services/ingest/src/detect/types.ts:97-104`) gains `severityId: number` — **required, non-nullable**, the same OCSF 0–6 ordinal. Its source is the matched rule's `severity_id`, which is always present and non-null on the path that reaches grouping: `RuleMatch.severityId: number` (`types.ts:87`) ← `cg.severity_id` parsed as `z.number().int().min(0).max(6)` (`services/ingest/src/detect/engine.ts:55`), and `upsertIncident` only fires after a successful `upsertAlert` whose `severity_id` column is `NOT NULL` + range-checked (`services/ingest/src/detect/index.ts:39-45`). So `GREATEST` never receives `NULL`.

### 3. Read-model: `IncidentListItem` / `IncidentDetail` (the read surface)

Both `services/api/src/read/types.ts` shapes gain `severity_id: number` (`IncidentListItem` `:16-25`, `IncidentDetail` `:27-38`); the Postgres reads project the column (`listIncidents` `queries.ts:89-92`, `getIncidentDetail` `:164-165`). The dashboard's decoupled wire-contract (`dashboard/src/lib/api/types.ts:21-43`) mirrors the field additively — the routes declare **no Fastify response schema** (`services/api/src/read/routes.ts:24-54`), so the projected field reaches the JSON unstripped. This is **additive**: no existing read-model field changes, and no consumer assumed an incident severity before this SPEC (forensic audit, Session 20).

## Operational

### 1. Migration `0006_incident_severity.ts` — transactional, backfill-before-NOT-NULL

Three steps, in this order, mirroring the add-nullable → backfill → set-NOT-NULL precedent of `0004_alerts_event_time.ts:22-30`:

```sql
-- (a) add nullable so existing rows tolerate the column before backfill
ALTER TABLE incidents ADD COLUMN IF NOT EXISTS severity_id smallint;

-- (b) backfill each existing incident from the MAX severity of its member alerts
UPDATE incidents
SET severity_id = (
  SELECT MAX(a.severity_id) FROM alerts a WHERE a.alert_id = ANY(incidents.alert_ids)
)
WHERE severity_id IS NULL;

-- (c) enforce NOT NULL + range — AFTER the backfill has filled every row
ALTER TABLE incidents ALTER COLUMN severity_id SET NOT NULL;
ALTER TABLE incidents ADD CONSTRAINT incidents_severity_range CHECK (severity_id BETWEEN 0 AND 6);
```

- **Transactional:** Kysely's `Migrator.migrateToLatest()` wraps the whole run in a single transaction when the dialect supports transactional DDL, which Postgres does (`services/ingest/node_modules/kysely/dist/cjs/migration/migrator.js:429-430` — `if (adapter.supportsTransactionalDdl) return this.#props.db.transaction().execute(run)`). So steps (a)-(c) are atomic: if step (c)'s `SET NOT NULL` finds a residual NULL (a backfill gap), the **whole migration rolls back** — the column is never half-added. This is why `0004` adds + backfills + sets NOT NULL in three bare `execute(db)` calls with no explicit `BEGIN`/`COMMIT`: the Migrator provides the transaction.
- **Ordering is load-bearing:** step (c) **must** follow step (b). `ADD COLUMN` (step a) leaves every existing row NULL; `SET NOT NULL` before the backfill would fail on those rows. The order here is the same as `0004`'s.
- **Idempotent:** `ADD COLUMN IF NOT EXISTS`, backfill scoped to `WHERE severity_id IS NULL`, and a `down()` that `DROP COLUMN IF EXISTS severity_id` (the CHECK drops with the column).

### 2. Grouping input population

In `runDetectionCycle` (`services/ingest/src/detect/index.ts:45-51`), the `IncidentGroupingInput` literal gains `severityId: match.severityId` — `match` (a `RuleMatch`) is already in scope (it feeds `upsertAlert` two lines up, `:38`), so no new plumbing is needed.

### 3. Upsert — the three additions as one unit (`services/ingest/src/detect/incidents.ts:70-90`)

The `INSERT … ON CONFLICT … DO UPDATE` gains, atomically:

- `severity_id` in the INSERT column list,
- its bound parameter in `VALUES`,
- `severity_id = GREATEST(incidents.severity_id, excluded.severity_id)` in the `DO UPDATE SET`, alongside the existing `alert_ids` CASE and `updated_at = now()`.

`GREATEST(incidents.severity_id, excluded.severity_id)` follows the exact `incidents.<col>` (current row) vs `excluded.<col>` (incoming INSERT values) reference pattern the existing `alert_ids` CASE already uses (`incidents.ts:76-77`).

### 4. The re-worded invariant (`services/ingest/src/detect/incidents.ts:43-60`)

The docstring's clause *"The `SET` list touches ONLY alert_ids and updated_at"* becomes false once `severity_id` joins the SET, so it is re-worded to declare **two field classes** explicitly:

- **Machine-recomputed on every correlated alert:** `alert_ids` (accumulates, set-semantic) and `severity_id` (raised to the running MAX). These reflect the evolving evidence and are owned by the detection path.
- **Preserved triage-state:** `status`, `assigned_to`, `activity_id`, `cg_mitre`, `title`. A new correlated alert **never** resets these — the original triage-preservation promise (a new alert never overrides a human's triage) is kept verbatim in substance.

The substantive invariant is unchanged: **a new correlated alert never resets human triage.** Severity is not triage-state — it is machine-derived evidence — so raising it does not violate the promise; it extends the recomputed-field class.

## Acceptance criteria

Each AC maps to a test exercising the real backend (testcontainers — the CI-able Postgres pattern `services/ingest` and `services/api` already use, `services/ingest/test/helpers/backends.ts`).

- **AC-001 (the MAX marquee — commutativity).** `services/ingest/test/incident-severity-ac-001-max.test.ts`. Two distinct alerts in the same correlation window (same `grouping_key`) with severities `low` then `high` collapse to one incident whose `severity_id = high`; a second incident receiving `high` then `low` (reverse order) also ends at `high`. Proves `GREATEST` is order-independent (a higher-severity alert raises the incident regardless of arrival order, and a lower-severity follower never lowers it).
- **AC-002 (triage preserved while severity escalates).** Extends `services/ingest/test/incident-ac-004-triage-preservation.test.ts`. After an analyst moves an incident to `investigating` / `analyst-x`, a **higher-severity** correlated alert arrives: the three existing assertions survive (`status` stays `investigating`, `assigned_to` stays `analyst-x`, `alert_ids` grows to 2) **and** `severity_id` is recomputed to the MAX. The machine field escalates; the triage fields do not move.
- **AC-003 (read-model projects it).** `services/api/test/incident-severity-ac-003-readmodel.test.ts`. With an incident whose `severity_id` equals the MAX of its seeded alerts, `GET /v1/incidents` (list) and `GET /v1/incidents/:id` (detail) both return `severity_id` equal to that MAX, org-scoped, behind the session preHandler.
- **AC-004 (backfill correctness + ordering).** `services/ingest/test/incident-severity-ac-004-backfill.test.ts`. Against a throwaway database migrated to `0005_incidents` (no severity column), seed an incident referencing alerts of **mixed** severities, then `migrateToLatest()` (applies `0006`): the incident's `severity_id` is backfilled to the MAX of its `alert_ids`, and the migration completes (proving `SET NOT NULL` runs after the backfill — an out-of-order migration would throw on the NULL rows).

## Test scenarios

Per ADR-0005 §Harness obligation.

- **SC-SEV-001 — severity escalates with evidence.** A second, more severe alert raises the incident severity; a less severe one does not lower it. Realised by AC-001.
- **SC-SEV-002 — triage survives escalation.** A severity bump never resets a human's triage. Realised by AC-002.
- **SC-SEV-003 — the SOC sees incident severity.** Both incident read endpoints surface `severity_id`. Realised by AC-003.
- **SC-SEV-004 — existing incidents get a severity.** The migration backfills pre-existing incidents from their alerts' MAX. Realised by AC-004.

## Risks

| Risk | Mitigation |
| --- | --- |
| **Dangling `alert_ids`** (an incident referencing alert ids with no matching `alerts` row) ⇒ backfill `MAX(...)` returns `NULL` ⇒ step (c) `SET NOT NULL` fails ⇒ the whole migration rolls back. | Production incidents are always created from a just-persisted alert and only ever accrete real alert ids (`index.ts:38-45`; FK-faithful per alert, Convention #12), so `MAX` is non-null in a faithful DB. The migration is **fail-loud by design** — a NULL surfaces real data corruption rather than masking it. A `COALESCE(..., 0)` fallback was rejected: it would silently invent a severity for a corrupt incident. **Flagged for Manuel:** confirm fail-loud is the desired posture, or request a COALESCE fallback. |
| Severity recomputes but `cg_mitre` stays frozen-first — an apparent inconsistency in how N-alert fields combine. | Deliberate and documented (§4 + §Out of scope). `cg_mitre.tactics` is part of the `grouping_key`, so all members already share it; `techniques` freezing is a known, separately-destined choice. Severity is the field whose SOC meaning *requires* the running max. |
| The visual `Severity` column is omitted, so the SOC list does not yet show the new datum. | Additive: the read-model and the dashboard wire-type carry `severity_id`; the UI column is a cosmetic follow-up with no data dependency (§Out of scope). |
| Making `severity_id` / `severityId` required ripples into existing test fixtures and grouping-input literals. | Intentional: severity is always present, so a required field is the faithful contract. The affected literals (`incident-ac-002/003/005`, `dash-ac-002` fixtures) are updated in the same change — a typecheck-enforced, mechanical, additive edit. |

## Open questions

1. **Fail-loud vs. COALESCE on a dangling-reference backfill** (see §Risks). Recommendation: fail-loud (surface corruption). Pending Manuel's call.
2. **Blueprint scale reconciliation** — the `incident.severity >= 8` playbook example (`blueprint.md:423`) vs. the OCSF 0–6 ordinal adopted here. Deferred (§Out of scope); revisit in a blueprint pass.
3. **Per-org / weighted aggregation** beyond unconditional MAX. Deferred to a configurable-aggregation increment.

## Ratification record

Load-bearing decisions for Manuel's gate (recommended-default-and-rationale pattern, per SPEC-005..010).

1. **Scale = OCSF ordinal 0–6**, identical to `alerts.severity_id` — closes the schema-vs-wire divergence (`examples/06_incident_grouped.json:11`); not a new scale, and not the blueprint's inconsistent `>= 8` playbook scale.
2. **Semantics = unconditional MAX**, recomputed via `GREATEST` on the upsert — an incident is as severe as its worst alert; severity can only rise as evidence accretes. The blueprint prescribes nothing, so this is a ratified SOC-posture choice.
3. **Recomputed in the upsert, not frozen at creation** — unlike `cg_mitre`. Severity must track the running evidence; `alert_ids` already proves the `DO UPDATE` path recomputes machine fields.
4. **Migration is transactional with `SET NOT NULL` after backfill** — atomic by the Kysely Migrator's transaction (`migrator.js:429-430`); ordering mirrors `0004`.
5. **`IncidentGroupingInput.severityId` is required** — severity is always present on the grouping path; a non-nullable field is faithful and guarantees `GREATEST` never sees NULL.
6. **No new ADR** — additive change within the detection write-path and read-slice; no service or trust boundary opened.

## References

- [SPEC-007](SPEC-007-incident-grouping-mvp.md) — the `incidents` table, the upsert, `IncidentGroupingInput`, and the triage-preservation invariant (`services/ingest/src/detect/incidents.ts:43-60`, `:70-90`) this SPEC extends and re-words.
- [SPEC-009](SPEC-009-read-slice.md) — the read-models (`services/api/src/read/types.ts:16-38`) and routes (`read/routes.ts:24-54`) this SPEC extends additively.
- [SPEC-010](SPEC-010-forensic-event-drill.md) — `:32` (the deferred *"Severity / score aggregation per incident"* this SPEC realises).
- [SPEC-006](SPEC-006-detection-mvp.md) / [ADR-0012](../adr/0012-normalize-before-correlate-pipeline.md) §6 — `alerts.severity_id` OCSF 0–6 (`0002_alerts.ts:29,50`), `RuleMatch.severityId` (`detect/types.ts:87`), the scorer's severity/score independence.
- [ADR-0013](../adr/0013-event-time-incident-windowing.md) — event-time windowing (unchanged by this SPEC).
- `services/ingest/src/db/migrations/0004_alerts_event_time.ts:22-30` — the add-nullable → backfill → set-NOT-NULL migration precedent this SPEC mirrors.
- `schemas/cges/v0.1/examples/06_incident_grouped.json:11` / `classes/incident.json:8-56` — the wire example carrying `severity_id` vs. the schema that omits it (the divergence this SPEC closes).
- [Blueprint](../product/blueprint.md) — `:556` (incident severity as a display attribute), `:390` (incident phase = grouping only), `:423` (the out-of-scope `>= 8` playbook scale).
