# SPEC-010: Forensic event drill (step 1) — raw-event timeline per incident

- **ID:** SPEC-010
- **Title:** Forensic event drill — incident → raw `cges_events` timeline
- **Status:** Accepted
- **Depends on:** ADR-0015 (the read-only ClickHouse reader in `services/api` this SPEC implements), SPEC-009 (the read-slice this extends — its session preHandler `makeRequireSession`, org-scope, route pattern, and the `getIncidentDetail` incident→alerts resolution; `:34` deferred this drill as "a read-depth increment"), SPEC-007 (`incidents` + `alert_ids`), SPEC-006 (`alerts.source_events`), SPEC-005 (`cges_events` shape), ADR-0003 (events → ClickHouse), ADR-0014 (the human-auth model the read runs under), ADR-0012 §4 (`event_id IN source_events` is version-agnostic).
- **Authors:** Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

The forensic-report promise (*"… an exportable forensic report on the first incident"*, `docs/product/blueprint.md:33`) starts with a **drill**: from a grouped incident down to the **raw events** that produced its alerts. SPEC-009 shipped the read-slice (incidents list / incident detail / alerts) but **cut the drill** with a named destination — *"The alert→source-event drill (incident detail → the raw `cges_events`). Data-unblocked … but cut by UI cost + it would add the first ClickHouse client to `services/api`. Destination: a read-depth increment."* (`docs/specs/SPEC-009-read-slice.md:34`). ADR-0015 settled the boundary (a read-only ClickHouse reader in `services/api`, a singleton on `Services`, the request-path pattern). **This SPEC is that read-depth increment** — step 1 of the forensic report: the event timeline.

The data is already wired (the SPEC-009 audit + the ADR-0015 audit): `alerts.source_events uuid[]` holds `cges_events.event_id` values (`0002_alerts.ts:34`), and `cges_events.event_id` is `UUID` (`migrate.ts:137`). No schema change is required (mirroring SPEC-009 §Data contracts: *"the reads project the existing … tables; no schema change"*).

## Scope

### In scope

1. **`GET /v1/incidents/:id/events`** on `services/api` — a nested read endpoint behind SPEC-008's session preHandler, returning the raw-event timeline for one incident, org-scoped.
2. **The read-only ClickHouse reader** in `services/api` (per ADR-0015): a `ch: ClickHouseClient` singleton on `Services`, built from `API_CH_*` config, closed in `Services.close()`.
3. **The cross-store resolution + query:** resolve the incident → alerts → `source_events` in Postgres (reusing the `getIncidentDetail` resolution, `services/api/src/read/queries.ts:151-182`), flatten + dedup the event ids in TS, and read `cges_events` by `event_id` in ClickHouse (`WHERE … toString(event_id) IN (…)`, `FINAL`, `ORDER BY time ASC`).
4. **The `TimelineEvent` read-model** and the `{ incident_id, events: TimelineEvent[] }` response shape.
5. **Acceptance criteria** (§AC) as integration tests against the real route + real backends.

### Out of scope

Each with a named destination:

- **Hash-chain / evidence integrity** over the returned events — the `evidence_n.hash`/`chain_n`/`root_signature` design (`docs/product/blueprint.md:527-535`). No integrity primitive over stored events exists today (audit); a later forensic-integrity phase.
- **MinIO artifact persistence** of the timeline/report — ADR-0003 homes forensic artifacts in MinIO; no MinIO consumer code exists today. A later phase.
- **PDF / HTML render** of the report — greenfield (no render dependency in any manifest). The forensic-render SPEC.
- **Severity / score aggregation per incident** — `incidents` stores `cg_mitre` but no severity/score (`0005_incidents.ts`); those live per-alert (`0002_alerts.ts:29,38`). A future read/scoring increment.
- **Empty-in-v0.1 CGES fields.** `process_command_line` and `subject_user_sid` are `DEFAULT ''` and structurally empty in CGES v0.1 (`services/ingest/src/detect/read-model.ts:57-60`); the timeline does **not** project them. They return when a richer ETW capture populates them.
- **WebSocket / real-time push** of the timeline — already deferred by SPEC-009 §Out of scope; request/refetch only.
- **Parent-image resolution** (the self-join `read-model.ts` does for detection) — the timeline returns `process_parent_pid` (raw), not a resolved parent image; resolving lineage for the report is a later enrichment.

## Data contracts

The read projects the existing tables; **no schema change** (`alerts` `0002`+`0004`, `incidents` `0005`, ClickHouse `cges_events`). Read-models are response shapes; all reads are **scoped to the session's `org_id`**.

### 1. `GET /v1/incidents/:id/events` — the event timeline

- **Path param:** `:id` — the incident id (UUID).
- **Response:** `{ incident_id: string, events: TimelineEvent[] }`. `events` is ordered by event time ascending (`time ASC`).
- **`TimelineEvent`** (only the **populated** v0.1 columns of `cges_events`, `services/ingest/src/db/migrate.ts:134-153`):
  - `event_id` (string, the `UUID` join key), `agent_id` (string), `activity_id` (number), `process_pid` (number), `process_uid` (string), `process_name` (string), `image_file_name` (string), `process_parent_pid` (number | null), `event_time` (string — the `cges_events.time` `DateTime64(9)`, projected as `toString(time) AS event_time`, mirroring `read-model.ts`).
- **Not found / cross-org:** a generic `404 { error: "not_found" }` (no cross-org existence oracle — §Operational §1, mirroring `services/api/src/read/routes.ts:36-49`).
- **Empty:** an incident that exists but resolves to **no** `source_events` returns `200 { incident_id, events: [] }` — **not** a `404` (the incident exists; it simply has no resolvable raw events).

### 2. `source_events` projection (the enabling change)

`source_events uuid[]` is added to `ALERT_COLS` and to the `AlertRow` row type (`services/api/src/read/queries.ts:148-149`, `:124-133`) so the shared incident→alerts resolution carries it. It is **node-postgres-typed as `string[]`** — the precedent is `IncidentDetailRow.alert_ids: string[]` (`queries.ts:122`). `source_events` is **NOT** added to `ResolvedAlert` / `IncidentDetail`: the drill consumes it internally to gather event ids; the SPEC-009 `IncidentDetail` contract the dashboard depends on stays **byte-for-byte unchanged** (`services/api/src/read/types.ts:27-38`; `dashboard/src/lib/api/types.ts:32-43`).

## Operational

### 1. The route — nested, session-guarded, org-scoped

`GET /v1/incidents/:id/events` registers on the existing Fastify read surface (`services/api/src/read/routes.ts`).

- **Nesting:** `/v1/incidents/:id/events` — a nested resource path. The precedent for `/v1/<resource>/:id/<sub>` in the api is `services/api/src/auth/routes.ts:63` (`POST /v1/users/:id/role`); the read-slice was flat until now.
- **Auth:** the same `makeRequireSession` preHandler the three SPEC-009 reads use (`routes.ts:16-17`). Role + `org_id` come from the session, never the request. **No `makeRequireRole` gate** (all three roles read in the MVP, SPEC-009 §Operational §2) and **no CSRF** (read-only, SPEC-009 `:117` / NFR-009-004).
- **Errors:** a malformed `:id` (not a UUID) or a non-existent / cross-org incident → `404 { error: "not_found" }`, mirroring the incident-detail handler (`routes.ts:36-49`). *(A transient ClickHouse outage currently surfaces through the same mirror as `404`; distinguishing store-failure from not-found is the named §Open questions item below.)*

### 2. The logic — Postgres resolution + cross-store ClickHouse read

In `getIncidentEventTimeline(pool, ch, orgId, id)` (`services/api/src/read/queries.ts`):

1. **Resolve the incident in Postgres**, org-scoped: `SELECT alert_ids FROM incidents WHERE incident_id = $1 AND org_id = $2`. No row → return `null` → the route answers `404` (no cross-org existence oracle; the malformed-UUID case throws on the `$1` cast and is caught → `404`, mirroring `getIncidentDetail`).
2. **Resolve the grouped alerts** org-scoped, reusing the `getIncidentDetail` set-membership shape: `SELECT ${ALERT_COLS} FROM alerts WHERE alert_id = ANY($1::uuid[]) AND org_id = $2`. Collect every row's `source_events`.
3. **Flatten + deduplicate** the `source_events` arrays in TS — `[...new Set(rows.flatMap(r => r.source_events))]`. This follows the repo's canonical array handling (project the `uuid[]` per row as `string[]` + manipulate in TS, like `alert_ids` at `queries.ts:166`); there is **no** `array_agg`/`unnest` precedent in the repo to reuse. An empty set → return `{ incident_id, events: [] }` (no ClickHouse call).
4. **Read ClickHouse** by event id (the cross-store join), reusing the parameterised array-membership pattern of `read-model.ts:136-158`:

   ```sql
   SELECT toString(event_id) AS event_id, toString(agent_id) AS agent_id, activity_id,
          process_pid, process_uid, process_name, image_file_name,
          process_parent_pid, toString(time) AS event_time
   FROM cges_events FINAL
   WHERE org_id = {org:String}
     AND toString(event_id) IN ({ids:Array(String)})
   ORDER BY time ASC
   ```

   `query_params: { org: orgId, ids: eventIds }`, `format: "JSONEachRow"`. `FINAL` collapses the `ReplacingMergeTree` at-least-once duplicates (ADR-0012 §2); `toString(event_id) IN (Array(String))` mirrors how `read-model.ts` matches `UUID` columns against a string array (`toString(agent_id) IN ({agents:Array(String)})`). The result rows map 1:1 to `TimelineEvent`.

### 3. The ClickHouse reader lifecycle (per ADR-0015)

- **Build:** `buildServices` (`services/api/src/services.ts`) constructs `ch = createClient({ url: API_CH_URL, username: API_CH_USER, password: API_CH_PASSWORD, database: API_CH_DB })` — a 1:1 mirror of `services/ingest/src/services.ts:24-33` — and exposes it as `Services.ch`.
- **Lazy:** `createClient` does not connect until the first query; api boot and the three Postgres routes do not depend on ClickHouse availability (ADR-0015 §Compliance).
- **Close:** `Services.close()` migrates to `Promise.allSettled([pool.end(), redis.quit(), ch.close()])` — the style ratified by the architect, aligning with `services/ingest/src/services.ts:45` and ensuring one rejected close does not prevent the others.

## Acceptance criteria

Each AC maps 1:1 to a test under `services/api/test/` (`drill_ac_NNN`). The drill ACs need **Postgres + Redis + ClickHouse** via testcontainers; ClickHouse-via-testcontainers is the same CI-able pattern `services/ingest` already uses on Linux (`services/ingest/test/helpers/backends.ts:40-49`).

- **drill_ac_001 (the marquee — real cross-store join Postgres → ClickHouse).** Seed real `cges_events` rows in ClickHouse + an incident with alerts whose `source_events` point at those event ids. An authenticated `GET /v1/incidents/:id/events` returns `200` with the events **in `time` ASC order**, org-scoped, each carrying the `TimelineEvent` fields. Proves the end-to-end cross-store join.
- **drill_ac_002 (session + org — SECURITY).** A request with **no/invalid** `cgsess` → `401` **before** the handler (`makeRequireSession`); an incident in **another org** → `404` (no cross-org leak, no existence oracle).
- **drill_ac_003 (empty vs absent).** An existing incident whose alerts resolve to **no** `source_events` → `200 { events: [] }`; a **non-existent** incident **or** a non-UUID `:id` → `404`.

## Test scenarios

Per ADR-0005 §Harness obligation.

- **SC-DRILL-001 — the forensic drill path.** Open an incident → fetch its raw-event timeline → events in time order. Realised by drill_ac_001.
- **SC-DRILL-002 — authz boundary.** Unauthenticated drill → `401`; cross-org `:id` → `404`. Realised by drill_ac_002.
- **SC-DRILL-003 — empty vs absent.** Existing-but-no-events → `200 {events:[]}`; absent/malformed → `404`. Realised by drill_ac_003.

## Risks

| Risk | Mitigation |
| --- | --- |
| The api's first ClickHouse client adds a store dependency | Read-only, lazy client (ADR-0015); the three Postgres routes never touch `ch` and do not degrade if ClickHouse is down |
| Cross-store join has no DB-enforced referential integrity | The join is value-match by `event_id` (version-agnostic, ADR-0012 §4); a dangling `source_events` id simply yields no matching event row — the timeline is the events that exist, never an error |
| Test harness adds ClickHouse to the api suite (was Postgres+Redis only) | Mirrors ingest's CI-able testcontainers ClickHouse (`ingest/test/helpers/backends.ts:40-49`); `cges_events` is materialised in the api test ClickHouse (see §Open questions 1) |
| `:id/events` nesting is new to the read-slice | Precedent exists in the api (`auth/routes.ts:63`, `/v1/users/:id/role`) |
| Output-shape change could break the dashboard | `IncidentDetail` is unchanged; `source_events` is added only to the internal `AlertRow`/`ALERT_COLS`, never to `ResolvedAlert`/`IncidentDetail` (§Data contracts §2). dash_ac_001 binds the contract and would surface any drift |

## Open questions

1. **`cges_events` materialisation in the api test ClickHouse.** The api test harness applies ingest's **Postgres** migrations in-workspace (`read-schema.ts`), but `cges_events`'s DDL lives in ingest's **private** `bootstrapClickHouse` (`migrate.ts:86-155`), not a migration file. **Recommendation (this SPEC):** a scoped test-only DDL helper in the api harness (`services/api/test/helpers/events-schema.ts`) that creates `cges_events` from the verbatim DDL + an `insertCgesEvent` seed (mirroring `ingest/test/helpers/db.ts:153-183`). **Named debt → trigger:** replace with a shared ClickHouse-schema module (or an exported `bootstrapClickHouse`) when a second api↔ingest ClickHouse table is shared, or on the first un-caught DDL drift. *(Most needs architect review.)*
2. **`API_CH_URL` required vs optional in `EnvSchema`.** **Recommendation:** required `z.string().url()` (mirroring `INGEST_CH_URL`); the lazy client keeps the Postgres routes resilient to a down ClickHouse, so "required config" does not mean "required-at-boot connectivity".
3. **ClickHouse outage on `GET /v1/incidents/:id/events` — `404` vs `503`.** A transient ClickHouse failure on the drill currently returns `404` (mirroring `services/api/src/read/routes.ts:36-49`), conflating *evidence-inaccessible* with *evidence-absent*. Refine to `503` to distinguish a store failure from not-found. Deferred from step 1; revisit when planning the next forensic step.

## Ratification record

Load-bearing decisions for Manuel's gate (recommended-default-and-rationale pattern per SPEC-005..009).

1. **Drill served by the api reading ClickHouse directly** (ADR-0015 Option A), not an ingest endpoint + proxy (Option A1 rejected: first inter-service hop + session-auth replication across the agent boundary).
2. **Singleton CH client on `Services`** (request-path pattern, `heartbeat.ts:55`), not factory-per-call.
3. **`source_events` added to `ALERT_COLS`/`AlertRow` only; `IncidentDetail` untouched** — the dashboard contract does not change.
4. **No schema change, no migration** — the data is already wired; the api adds a reader.
5. **`Services.close()` → `Promise.allSettled`** (ratified style change, aligns with ingest).
6. **`cges_events` test materialisation = scoped DDL helper in the api harness** (§Open questions 1), with named debt + trigger.

## References

- [ADR-0015](../adr/0015-readonly-clickhouse-reader-in-api.md) — the boundary decision this SPEC implements.
- [SPEC-009](SPEC-009-read-slice.md) — `:34` (the deferred drill this realises), `:117` (read-only / no CSRF), the `makeRequireSession` + org-scope + route pattern (`services/api/src/read/routes.ts:36-49`) and the `getIncidentDetail` resolution (`queries.ts:151-182`) this extends.
- [SPEC-007](SPEC-007-incident-grouping-mvp.md) / [SPEC-006](SPEC-006-detection-mvp.md) / [SPEC-005](SPEC-005-agent-process-telemetry-windows-etw.md) — `incidents.alert_ids`, `alerts.source_events`, `cges_events` respectively.
- [ADR-0012](../adr/0012-normalize-before-correlate-pipeline.md) — `:151` / §4 (`event_id IN source_events` version-agnostic), §2 (FINAL collapses ReplacingMergeTree duplicates).
- `services/ingest/src/detect/read-model.ts:136-158` — the parameterised array-membership ClickHouse read pattern this query reuses.
- `services/ingest/src/services.ts:24-33` / `:45` + `routes/heartbeat.ts:55` — the singleton client + `Promise.allSettled` close + request-path precedents.
- `services/ingest/test/helpers/backends.ts:40-49` / `db.ts:153-183` — the CI-able ClickHouse testcontainer + `insertCgesEvent` patterns the api harness mirrors.
- [Blueprint](../product/blueprint.md) — `:33` (the forensic-report promise), `:527-535` (the deferred hash-chain).
