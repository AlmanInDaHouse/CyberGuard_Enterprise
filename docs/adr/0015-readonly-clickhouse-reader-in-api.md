# ADR-0015: Read-only ClickHouse reader in `services/api` — the forensic event-drill boundary

- Status: Accepted
- Date: 2026-06-05
- Last updated: 2026-06-05
- Deciders: Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

The product promise is *"… an exportable forensic report on the first incident"* (`docs/product/blueprint.md:33`). The forensic report's first dependency is a **drill from an incident down to the raw events** that produced its alerts. A read-only audit (this session) established the repo-grounded facts that force one boundary decision before that drill can be specified:

- **The join key is already wired on both sides.** `alerts.source_events uuid[] NOT NULL` holds the raw event ids (`services/ingest/src/db/migrations/0002_alerts.ts:34`, *">= 1 cges_events.event_id"*), and `cges_events.event_id` is `UUID` and the trailing component of the ClickHouse `ORDER BY (org_id, time, event_id)` key (`services/ingest/src/db/migrate.ts:137`, `:153`). The drill is **data-unblocked** — the value-match is version-agnostic (`docs/specs/SPEC-009-read-slice.md:34`).
- **`services/api` reads no ClickHouse today.** The api dependency container wires only Postgres + Redis (`services/api/src/services.ts:11-16`); `config.ts` declares only `API_PG_URL` + `API_REDIS_URL` (`services/api/src/config.ts:8-20`); `@clickhouse/client` is not an api dependency; the api migration applier comment states *"api is Postgres + Redis only"* (`services/api/src/db/migrate.ts:21`). The only ClickHouse-capable service is `services/ingest`.
- **SPEC-009 deliberately cut the drill, naming this exact blocker.** `docs/specs/SPEC-009-read-slice.md:34`: *"… cut by UI cost + it would add the first ClickHouse client to `services/api` (today pg+redis only). Destination: a read-depth increment."* This ADR settles the boundary the increment needs — **where the new ClickHouse read lives** — before its SPEC specifies the mechanics, the same single-layer discipline ADR-0013/ADR-0014 followed.
- **Two ClickHouse client patterns coexist in `ingest`.** A **singleton injected in `Services`** (`services/ingest/src/services.ts:24-33`), used in the **request-path** (the heartbeat route destructures `ch` from `services` — `services/ingest/src/routes/heartbeat.ts:55`), and a **factory-per-call** that opens+closes per invocation (`services/ingest/src/detect/read-model.ts:39-46`), used only by the **background** detection cycle (`services/ingest/src/detect/index.ts:24-27`).
- **The human/agent trust boundary is split (ADR-0014 §3).** `services/api` is the user-facing boundary; `services/ingest` is the agent-mTLS boundary (`services/api/src/app.ts:11-12`). The human session + RBAC primitives (`makeRequireSession`, org-scope) live **only** in `services/api` (`services/api/src/auth/prehandlers.ts`). There is **no** inter-service network call between api and ingest today (audit: grep of `services/api/src` for `fetch(`/`http.request`/`INGEST_` → no matches); they share only the one Postgres instance (`services/api/src/db/migrate.ts:15-21`).

## Decision

### 1. `services/api` acquires a read-only ClickHouse client

`services/api` gains a ClickHouse client used **exclusively for reads** of `cges_events`, to serve the incident→events drill. `services/ingest` retains **sole ownership of writes** to `cges_events` (the agent → ingest → ClickHouse path) and of the table's DDL bootstrap (`services/ingest/src/db/migrate.ts:86-155`). The api never writes, alters, or creates ClickHouse objects.

### 2. The client is a singleton on `Services` — the request-path pattern

The reader is a **singleton injected in the api `Services` container**, built once per process in `buildServices` from `API_CH_*` config (mirroring `services/ingest/src/config.ts:7-13` + `services/ingest/src/services.ts:24-33`) and closed symmetrically in `Services.close()`. This is the **request-path** pattern (the precedent is the ingest heartbeat route, which uses the injected `ch` in response to an incoming request — `services/ingest/src/routes/heartbeat.ts:55`), not the factory-per-call pattern (`read-model.ts:39-46`), which is ingest's **background** detection pattern (open+close per cycle) and is unfit for a synchronous `GET`.

### 3. The cross-store join is served by the api reading both stores

The drill resolves the incident and its grouped alerts in **Postgres** (org-scoped, reusing the `getIncidentDetail` resolution pattern), collects the alerts' `source_events`, and reads the raw events from **ClickHouse** by `event_id`. The join `cges_events.event_id = ANY(alerts.source_events)` is a **cross-store** read inside one api request — there is no DB-level foreign key between the Postgres `alerts` and the ClickHouse `cges_events` (different stores), and none is introduced.

## Alternatives considered

### A1 — Endpoint in `ingest` + api proxy

`services/api` calls a **new ingest HTTP endpoint** that serves the timeline; api proxies the result to the dashboard. *Pros:* keeps all ClickHouse access in the one CH-capable service; api stays Postgres+Redis. *Cons (decisive):*

- It introduces the **first inter-service network hop** in the system — there is no precedent today (audit: `services/api/src` has zero `fetch`/HTTP calls to ingest; they communicate only through the shared Postgres instance, `migrate.ts:15-21`). A new network dependency, its failure modes, and its auth all become greenfield.
- It would require **replicating or proxying the human session auth** — `makeRequireSession` + org-scope live **only** in `services/api/src/auth/prehandlers.ts` — into `services/ingest`, the **agent-mTLS** boundary (ADR-0014 §3). That merges two trust boundaries the threat model keeps separate (`services/api/src/app.ts:11-12`).

A read-only CH client in api is the **smaller, boundary-preserving** change. **Rejected.**

### A2 — Factory-per-call client in the request path

Use the `read-model.ts:39-46` factory (open a client, query, `ch.close()` in `finally`) inside the `GET` handler. *Pros:* no long-lived connection. *Cons:* that is ingest's **background** pattern (one client per detection cycle); the **request-path** precedent is the injected singleton (`heartbeat.ts:55`). A connect+close per request adds latency and connection churn to a synchronous read. **Rejected** in favour of the singleton (§2).

## Consequences

### Positive

- **The drill's last structural blocker is removed.** SPEC-009:34 named "no CH client in api" as the cut reason while confirming the data is unblocked; this ADR adds exactly that reader.
- **Trust boundaries preserved.** api stays the user-facing reader; ingest stays the agent-facing writer; no inter-service network hop; the human session auth stays in api.
- **Canonical patterns reused.** Singleton-in-`Services` (request-path, `heartbeat.ts:55`); `API_CH_*` config as a 1:1 mirror of `INGEST_CH_*` (`ingest/config.ts:7-13`); symmetric `ch.close()` in `Services.close()` (`ingest/services.ts:45`).

### Negative

- **`services/api` gains a ClickHouse dependency** (`@clickhouse/client`) and `API_CH_*` config — for **this route only**. The three existing Postgres read routes (`GET /v1/incidents`, `/v1/incidents/:id`, `/v1/alerts`) never touch `ch`. Because `createClient` connects **lazily** (no connection until the first query), a **down ClickHouse does not break boot or the Postgres routes** — only the drill route fails. The Postgres read surface does not degrade.
- **A second store on the api's operational surface.** ClickHouse must be reachable for the drill; this is a new runtime dependency for that one capability.

### Neutral

- **Amends SPEC-009 §Out of scope `:34` BY SCOPE, not by contradiction.** SPEC-009 filed the drill as *deferred-with-destination* ("a read-depth increment"); this ADR + its SPEC (**SPEC-010**) **are** that destination. The drill moves from deferred to delivered. A co-located amendment note is added to SPEC-009 (`## Amendment 2026-06-05`).
- **Does NOT supersede ADR-0003.** ADR-0003 already homes events in ClickHouse; this ADR adds a **reader** in api, it does not re-route storage. `ingest`'s migration isolation (`migrate.ts:15-21`, distinct advisory-lock key + Kysely ledger on the shared Postgres instance) is **untouched** — the api adds **no migration** (no schema change; `source_events` and `cges_events` already exist).
- **Does NOT supersede ADR-0014.** The human/agent boundary stands; api remains the user-facing service. This ADR is consistent with ADR-0014 §3 (it keeps human-facing reads in api rather than crossing into the agent boundary).

## Compliance

- The api ClickHouse client MUST be **read-only in use**: the api MUST NOT issue `INSERT` / `ALTER` / `CREATE` / `DROP` against ClickHouse. `ingest` owns writes and the `cges_events` DDL bootstrap (`migrate.ts:86-155`).
- The reader MUST be a **singleton on `Services`** (the request-path pattern, `heartbeat.ts:55`), built from `API_CH_*` and closed in `Services.close()`.
- The Postgres read routes MUST NOT depend on ClickHouse availability: the client is lazy; a ClickHouse outage fails **only** the drill route, never boot or the Postgres reads.
- No new ClickHouse migration or table may be authored in `services/api`; the api reads ingest's `cges_events` as the source of truth.

## Out of scope

Each deferred item names its destination:

- **The drill route / query / read-model / acceptance criteria.** SPEC-010 (the mechanics).
- **Hash-chain / evidence integrity** over the returned events (`docs/product/blueprint.md:527-535`). A later forensic phase.
- **MinIO artifact persistence** and **PDF/HTML render** of the report (ADR-0003 MinIO home; the forensic SPEC). Later phases.
- **Severity / score aggregation per incident** (the incident table stores `cg_mitre` but no severity/score; `0005_incidents.ts`). A future read/scoring increment.
- **A `503`-vs-`404` distinction for a ClickHouse outage on the drill route.** Tracked as a named open question in [SPEC-010](../specs/SPEC-010-forensic-event-drill.md) §Open questions (the drill currently mirrors the detail route's `404`).

## Landing checklist (atomic on flip to Accepted)

When this ADR is ratified Proposed→Accepted, the same commit:

1. Flips the status header to `Accepted`.
2. Adds the catalog row to `docs/adr/README.md`.
3. Adds these dependency edges to `docs/adr/README.md` §Dependencies:
   - `ADR-0015 → ADR-0003` (consumes the ClickHouse storage home as a **reader**; does **not** amend or re-route storage).
   - `ADR-0015 → ADR-0014` (preserves the human/agent trust-boundary split — keeps the read in the user-facing api, not the agent boundary).
   - `ADR-0015 → SPEC-009` (amends-by-scope `:34`: the drill it deferred-with-destination is delivered by this ADR + SPEC-010).

## References

- [SPEC-009](../specs/SPEC-009-read-slice.md) — `:34` cut the drill citing "first ClickHouse client in api"; this ADR removes that blocker. The 3 Postgres read routes (`services/api/src/read/routes.ts`) this ADR leaves untouched.
- [SPEC-010](../specs/SPEC-010-forensic-event-drill.md) — the mechanics this ADR's boundary enables (route, query, read-model, AC).
- [ADR-0003](0003-polyglot-storage.md) — homes events in ClickHouse; consumed (read) here, not amended.
- [ADR-0014](0014-human-authentication-model.md) — the human/agent boundary split (§3) this ADR preserves; the session/RBAC primitives the drill route reuses live in `services/api`.
- `services/ingest/src/services.ts:24-33` + `routes/heartbeat.ts:55` — the singleton-in-`Services` request-path pattern §2 adopts.
- `services/ingest/src/detect/read-model.ts:39-46` — the factory-per-call background pattern §2 rejects for the request path.
- `services/ingest/src/db/migrate.ts:86-155` — ingest's ownership of the `cges_events` write/DDL this ADR does not touch.
- [Blueprint](../product/blueprint.md) §2 (`:33` the forensic-report promise the drill serves).
