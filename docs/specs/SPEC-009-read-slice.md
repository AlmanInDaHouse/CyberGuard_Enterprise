# SPEC-009: Read-slice — incident/alert read API + minimal SOC dashboard

- **ID:** SPEC-009
- **Title:** Read-slice
- **Status:** Accepted
- **Depends on:** SPEC-008 (the auth-core this slice sits behind — the `makeRequireSession`/`makeRequireRole` preHandler factories (`services/api/src/auth/prehandlers.ts`, realised at SPEC-008's GREEN gate), the `cgsess` HttpOnly cookie, the `POST /v1/auth/login` the dashboard logs in through, the Fastify+Zod scaffold + pg pool the read endpoints extend; it explicitly deferred the read endpoints + their RBAC matrix to this SPEC), SPEC-007 (the `incidents` model A produced — the rows this slice reads), SPEC-006 (the `alerts` producer; its §Out of scope filed the WebSocket dashboard as a later phase), ADR-0014 (the human-auth model), ADR-0003 (incidents/alerts → Postgres; the reads run there), ADR-0001/ADR-0002 (`services/api` = TS/Fastify; `dashboard/` = Next.js 15), the [threat model](../security/threat-model.md) § cg-api + § Dashboard (the read-authz + cookie mandates)
- **Authors:** Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)
- **Created:** 2026-06-02
- **Last updated:** 2026-06-03

## Motivation

SPEC-009 is the **last** slice of Phase 6 C and the project's **first UI**. It makes the detection pipeline *visible*: a SOC operator logs in and sees the grouped incidents A produces, each with its correlated alerts and MITRE mapping (Blueprint §18 MVP: *"the SOC … sees a grouped incident in the Incidents view"*). It is the read half of C, gated behind SPEC-008's auth: SPEC-008 explicitly deferred *"all incident/alert reads … the list, the incident detail with grouped alerts + MITRE … SPEC-009 (the read-slice). The RBAC matrix rows for those read capabilities are SPEC-009's"* (`docs/specs/SPEC-008-auth-core.md:40`, `:592`).

It re-litigates nothing from SPEC-008 or ADR-0014: it inherits SPEC-008's realised auth primitives — the session preHandler (`makeRequireSession`) and role gate (`makeRequireRole`), the opaque `cgsess` cookie, and the Fastify+Zod scaffold; it adds **read endpoints** on `services/api` (greenfield — 008 was auth-only, `services/api/src/auth/routes.ts` is the only route surface today) and a **strongly-scoped minimal dashboard** (`dashboard/`, README-only today). It reads the data model A landed: `incidents` (`services/ingest/src/db/migrations/0005_incidents.ts`) + `alerts` (`0002_alerts.ts` + `event_time` from `0004_alerts_event_time.ts`), Postgres-only (ADR-0003).

The scope was fixed at the Session 19 audit gate: **read-API + a minimal dashboard (login → incidents list → incident detail), RBAC on reads, with WebSocket, the alert→event drill, triage writes, and the agents/token view all deferred** (each with a destination, §Out of scope).

## Scope

### In scope

1. **Read endpoints on `services/api`** (extending the SPEC-008 `routes.ts` pattern, behind its session preHandler `makeRequireSession`): `GET /v1/incidents` (list + filters + pagination), `GET /v1/incidents/:id` (detail with resolved alerts + MITRE — the teachable view), `GET /v1/alerts` (list + filters). §Data contracts §1–§3, §Operational §1.
2. **The read RBAC matrix** SPEC-008 deferred (`:40`, `:592`): role + org enforcement on **every** read, server-side, never from a client-claimed role (threat model `:84` cg-api; the same server-side-RBAC rule is restated platform-wide for cg-pipeline at `:103`). §Operational §2.
3. **Object-level authorization, scoped-to-org for the MVP, with the finer grain deferred-with-destination.** Honest realisation of threat model `:84` ("object-level authorisation checks on every read and write"); the **write** half is N/A to this read-only slice (NFR-009-004) and stays SPEC-008's — see §Security considerations.
4. **A minimal SOC dashboard** (`dashboard/`, Next.js 15): **only** login → incidents list → incident detail. The RSC ↔ `cgsess` ↔ read-API integration contract (the newest, most uncertain piece of C). §Operational §3.
5. **Acceptance criteria** (harness-first RED at the next gate): the read-API ACs + the dashboard-integration ACs, the RBAC one CI-blocking (threat model `:84`). §Acceptance criteria.

### Out of scope

Each with a named destination:

- **WebSocket / real-time push** of alerts. The MVP uses request/refetch (TanStack Query). Deferred to a real-time increment; already filed as a later phase by SPEC-006 §Out of scope (`docs/specs/SPEC-006-detection-mvp.md:42`, "the WebSocket dashboard") and named in the threat model's cg-api surface (`:75`).
- **The alert→source-event drill** (incident detail → the raw `cges_events`). Data-unblocked (the value-match `event_id IN source_events` is version-agnostic — `schemas/cges/v0.1/classes/alert.json:46`, `docs/adr/0012-normalize-before-correlate-pipeline.md:151`), but cut by UI cost + it would add the first ClickHouse client to `services/api` (today pg+redis only). Destination: a read-depth increment.
- **Triage writes** (analyst acknowledge / assign on incident/alert). The model already preserves human triage (SPEC-007 create-or-update invariant) and CSRF is ready (SPEC-008 §4). Destination: a triage-write increment.
- **The agents inventory view + the authed enrollment-token endpoint.** Deferred by SPEC-008 (`:47`); it reads the ingest-domain `agents` table (cross-boundary) and carries the SPEC-004 §Ratification-decision-2 amendment — none of which this SPEC drags in. Destination: an agents-management slice.
- **Per-agent / per-analyst object ACL** (finer than org-scope). No user↔agent scoping model exists; destination: the multi-tenancy / scoping ADR (Blueprint §17 item 1). §Security considerations.
- **Cases, Playbook executions, Forensic report viewer / PDF.** Later phases (SOAR / forensic, Blueprint §15/§16); not the read-slice.
- **Dashboard visual design** (layout, theming, component polish). The slice is thin: the three views + the auth integration, not a design system.

## Data contracts

The reads project the existing Postgres tables; **no schema change** (incidents `0005`, alerts `0002`+`0004`). Read-models are response shapes, Zod-validated on the way out, a choice of this SPEC (ADR-0002 Rule 4 makes the response types TypeScript and locks no validator). All reads are **scoped to the session's `org_id`** (§Operational §2).

### 1. `GET /v1/incidents` — list

- **Query:** `status?` (enum), `agent_id?` (an opaque equality filter on the `incidents.agent_id` column — it resolves **no** `agents`-table metadata; the agents view is deferred, §Out of scope), `limit` (default 50, **max 200** — mandatory pagination, threat model `:85`), `cursor?` (opaque, keyset over `updated_at`/`incident_id`).
- **Response:** `{ items: IncidentListItem[], next_cursor: string | null }`.
- **`IncidentListItem`:** `incident_id`, `agent_id` (the v0.1 host key, SPEC-007 §Data contracts §4), `status`, `title`, `cg_mitre { tactics[], techniques[] }`, `alert_count` (`cardinality(alert_ids)`), `window_start`, `updated_at`. (Summary — not the full `alert_ids` array.)

### 2. `GET /v1/incidents/:id` — detail (the teachable view)

- **Response:** `IncidentDetail`: `incident_id`, `agent_id` (the v0.1 host key, SPEC-007 §Data contracts §4), `status`, `title`, `cg_mitre`, `window_start`, `assigned_to`, `created_at`, `updated_at`, and **`alerts: ResolvedAlert[]`** — the incident's `alert_ids` **resolved** by querying `alerts` for those ids, scoped to the session's `org_id` (a single set-membership query; the SQL form is a gate concern).
- **`ResolvedAlert`:** `alert_id`, `title`, `severity_id`, `status`, `rule_id`, `cg_mitre`, `event_time`, `final_score`.
- **Not found / cross-org:** a generic `404` (no cross-org existence oracle — §Security considerations).

### 3. `GET /v1/alerts` — list

- **Query:** `status?`, `agent_id?` (opaque column filter, no `agents` metadata — as §1), `severity_id?`, `limit` (default 50, max 200), `cursor?`.
- **Response:** `{ items: AlertListItem[], next_cursor: string | null }`; `AlertListItem` = the `ResolvedAlert` shape (§2).

### 4. The dashboard ↔ session integration contract (the new piece)

The dashboard never holds a client-readable token; it mediates SPEC-008's `cgsess` cookie:

- **Login:** the login view submits `{ email, password, totp_code }` to SPEC-008's `POST /v1/auth/login` **via the dashboard's own origin** (a Next route handler proxies it server-side), so the `cgsess` cookie (`HttpOnly`/`Secure`/`SameSite=Strict`, threat model `:186`) is established on the dashboard origin and is readable by the dashboard's server runtime (never by client JS).
- **Reads:** a React Server Component reads `cgsess` (Next `cookies()`) and **forwards it server-side** to the read-API (a `Cookie` header on the server→server fetch); the browser never calls the read-API cross-origin directly.
- **Expiry / revocation:** a read-API `401` (no/`DEL`-ed session — SPEC-008 §2 immediate revocation) propagates as a **redirect to login**, never a broken view.
- **[DISEÑO — gate decision]** the exact same-origin mechanism (a Next route-handler auth proxy vs a single-origin reverse proxy fronting `dashboard/` + `services/api`) is the one integration mechanism to settle at the RED→GREEN gate; this SPEC fixes the *contract* (dashboard-origin session mediation + server-side cookie forwarding + 401→login), not the proxy mechanism.

## Operational

### 1. Read endpoints — pattern and shape

The read routes register on the existing Fastify app behind SPEC-008's session preHandler `makeRequireSession` (and a per-capability `makeRequireRole` where a future capability restricts — all three roles read in the MVP, §2). They query the pg pool already in `services` (SPEC-008); read-models are projected + Zod-serialised. Lists are keyset-paginated and `limit`-capped (no unbounded scans — threat model `:85`). Errors are sanitised (no SQL / internal detail to the client — threat model `:84`).

### 2. RBAC on reads — role + org, on every read

- **Matrix (auth-core surface was SPEC-008's; this is the read surface):**

  | Read capability | admin | analyst | viewer |
  | --- | --- | --- | --- |
  | List / read incidents | ✅ | ✅ | ✅ |
  | List / read alerts | ✅ | ✅ | ✅ |

  In the MVP all three roles read (the analyst/admin-only differentiators are writes/admin — SPEC-008 + the deferred triage/agents increments). The differentiation returns when triage-writes and the drill land; the **enforcement point** is fixed here so adding a restricted capability is a one-line `makeRequireRole` at the same seam.
- **Enforcement (server-side, never client role — threat model `:84` cg-api; `:103` restates it platform-wide):** every read endpoint carries SPEC-008's realised session preHandler `makeRequireSession` (`services/api/src/auth/prehandlers.ts`, landed at the SPEC-008 GREEN gate); the role + `org_id` come from the session entry, never the request. An unauthenticated/revoked read → `401`.
- **Org-scope on every read:** every query filters `org_id = <session.org_id>`; an `:id` outside the session's org returns `404` (no cross-org read, no existence oracle).

### 3. The minimal dashboard (Next.js 15, strongly scoped)

Three views only, on the README stack (`dashboard/README.md`: Next.js 15 App Router/RSC + Tailwind + shadcn/ui + TanStack Query):

- **Login** — email + password + TOTP code → the dashboard-origin auth proxy (§Data contracts §4).
- **Incidents list** — the `GET /v1/incidents` items (status, title, MITRE tactics, alert count, window); refetch via TanStack Query (no WebSocket — §Out of scope).
- **Incident detail** — the `GET /v1/incidents/:id` detail: the grouped alerts + the MITRE mapping (the teachable view).

No other view (no agents, no drill, no cases/playbooks/forensic, no WebSocket). UI visual design is out of scope (§Out of scope) — the views render the contract, not a design system.

### 4. CI for the dashboard (a new package)

`dashboard/` is a **new package** (Next.js), distinct from the `services/*` TypeScript. `ts-ci` today has two jobs — `check` (ingest) + `check-api` (api) (`.github/workflows/ts-ci.yml`). **[DISEÑO — RED-gate decision]** the dashboard needs either a third job (`check-dashboard`, mirroring how `check-api` was added in SPEC-008's RED gate) or its own workflow, with a `dashboard/**` path filter; note that a **Next build is heavier** than `tsc`+`biome`+`vitest`, so the dashboard job's cost/caching is a real consideration. This SPEC names the requirement; the RED gate wires it.

### 5. Read-target tables in the test harness (materialisation option (c) + its destination)

The read endpoints read `incidents`/`alerts`/`agents`, tables **owned by the ingest service's migrations** (`0001_initial`/`0002_alerts`/`0004_alerts_event_time`/`0005_incidents`). `services/api`'s own migration runner (Option A, SPEC-008 §Operational §7) applies only `users`/`audit_log`, so the api test harness must materialise the read-target tables — otherwise a data-dependent read AC fails with "relation does not exist" (setup-broken) rather than the absent read control. Running ingest's runner or cross-importing its migrations is not cleanly realisable: ingest's `runMigrations` couples a ClickHouse bootstrap (not separable), and `services/api`/`services/ingest` are independent pnpm packages with no workspace (a static cross-import breaks `rootDir` typecheck; a dynamic one breaks in the `check-api` job, which installs only api's deps). The MVP therefore uses **option (c): a test-only DDL mirror** (`services/api/test/helpers/read-schema.ts`), scoped to the columns the read-API consumes; a drift in a read column is caught by a read AC at the GREEN gate. **Named architecture debt → option (b):** replace the mirror with a shared schema package (or a pnpm-workspace cross-import) when the shared api↔ingest surface grows — **trigger:** a second ingest table the mirror does not cover, or the first un-caught drift.

> **RETIRADA (2026-06-03).** The (b) debt is closed: with the pnpm-workspace in place, `applyReadSchema` now applies **ingest's REAL Postgres migrations in-workspace** (its own Kysely `Migrator` over ingest's migrations folder, skipping the ClickHouse-coupled `runMigrations`), so the read-target tables come from the source of truth — option (c)'s mirror is gone and the drift risk is eliminated by construction. All 31 api tests stayed GREEN with zero read-AC result changes (the mirror was faithful for the read columns). See the §Amendment 2026-06-03 below and `engineering-notes.md` §"(b) schema-sharing debt RETIRED".

## Non-functional requirements

NFR identifiers scoped to this SPEC (`NFR-009-NNN`).

- **NFR-009-001 (mandatory pagination).** Every list endpoint is keyset-paginated with a hard `limit` cap (≤ 200); no endpoint returns an unbounded result set (threat model `:85`).
- **NFR-009-002 (org-scope on every read).** Every read filters on the session's `org_id`; no read can return another org's row (the object-level enforcement point — §Security considerations).
- **NFR-009-003 (error sanitisation).** Read errors return a generic status (`400`/`401`/`404`) with no SQL, stack, or internal detail (threat model `:84`).
- **NFR-009-004 (read-only).** The slice performs **no writes** (no triage, no mutation); CSRF therefore does not gate the read endpoints (it remains on SPEC-008's mutations).

## Security considerations

- **Object-level authz — complete at the point, scoped at the grain (the honest reading of threat model `:84`).** The threat model requires object-level authorisation on every read (and write — the write half is SPEC-008's; this slice is read-only, NFR-009-004). This SPEC enforces, on every read, the session preHandler `makeRequireSession` + an `org_id = <session>` filter — so the *enforcement point* exists and is CI-tested (read_ac, §AC). The MVP is **single-org** (ADR-0003 one `org_id`; Blueprint §17 item 1 defers multi-tenancy), so the object grain collapses to **org** (one) — there is no cross-org IDOR because there is one org, and there is no per-agent/per-analyst ACL because **no user↔agent scoping model exists**. The finer grain is **deferred with a destination** (the multi-tenancy / scoping ADR). This is not a threat-model gap silently accepted — it is the mandate met at the MVP's object granularity, with the finer granularity named as debt.
- **IDOR / cross-org:** a cross-org `:id` returns `404` (no existence oracle), per the org-scope filter.
- **Session posture inherited from SPEC-008:** the `cgsess` cookie stays `HttpOnly`/`Secure`/`SameSite=Strict` (threat model `:186`); the dashboard forwards it server-side only, never exposing it to client JS (§Data contracts §4). A revoked session (SPEC-008 §2 immediate `DEL`) fails the next read with `401` → login.
- **No new secret material** and **no writes** (NFR-009-004).

## Acceptance criteria

Each AC maps 1:1 to a test; `read_ac_NNN` (the read-API, under `services/api/test/`) and `dash_ac_NNN` (the dashboard, under `dashboard/`). The read-API ACs are CI-able on Linux via testcontainers (Postgres + Redis, like SPEC-008); the dashboard ACs' harness approach is **a gate decision** (below). The harness-first RED phase (next gate) turns the relevant job(s) red with the Known CI debt co-located (Convention #13), green when the impl lands.

- **read_ac_001 (the teachable detail).** Given an incident with N grouped alerts (seeded), an authenticated `GET /v1/incidents/:id` returns the incident + `alerts[]` resolved (N entries) + `cg_mitre` with non-empty tactics/techniques. **CI-able.**
- **read_ac_002 (incidents list + pagination).** `GET /v1/incidents?limit=k` returns ≤ k items + a `next_cursor` when more exist; the cursor returns the next page without overlap; `limit` is capped at 200. **CI-able.**
- **read_ac_003 (alerts list).** `GET /v1/alerts` returns the alert list items with the documented shape + filters. **CI-able.**
- **read_ac_004 (RBAC enforcement — SECURITY, CI-BLOCKING, threat model `:84`).** A read with **no/invalid/revoked session** → `401`; a read with a valid session (any of the three roles) → `200`. Production-faithful (Convention #12): the passing case is achieved by providing a valid session, **never** by removing the session guard (`makeRequireSession`). **CI-able.**
- **read_ac_005 (org-scope — SECURITY).** Seed incidents in two `org_id`s; a session in org A's `GET /v1/incidents` returns **only** org A's rows, and `GET /v1/incidents/:bId` (org B) → `404`. (Proves the object-level filter even though the MVP runs one org.) **CI-able.**
- **read_ac_006 (error sanitisation).** A malformed id / not-found → a generic `400`/`404` with no internal detail. **CI-able.**
- **dash_ac_001 (auth→read integration — the new terrain).** The dashboard's server-side data layer: with **no** `cgsess` → a read attempt redirects to login (no data); with a **valid** `cgsess` → it returns incident data from the read-API; a read-API `401` → redirect to login. **Harness approach: gate decision (below).**
- **dash_ac_002 (render the teachable view).** Given incident-detail data, the incident-detail view renders the grouped alerts + the MITRE mapping (data→DOM; no auth). **Harness: component render.**

**Dashboard harness approach (the decision most needing architect review).** This is the project's first UI test — new terrain. Proposed: test the **server-side data-access module** (reads `cgsess`, fetches the read-API, maps `401`→redirect) as an **integration test against the real read-API via testcontainers** (this proves the meaningful `auth→read` chain without a browser), plus a **light render test** (React Testing Library / vitest) for `data→DOM` (no auth, no pixels). **Avoid full E2E (Playwright)** for the MVP (heaviest; defer). The exact tooling (vitest + RTL vs Next's testing utilities) is to be confirmed at the RED gate.

## Test scenarios

Per ADR-0005 §Harness obligation.

- **SC-READ-001 — the demo path.** Login → list incidents → open one → see grouped alerts + MITRE. Realised by dash_ac_001/002 + read_ac_001.
- **SC-READ-002 — authz boundary.** Unauthenticated read → 401; cross-org `:id` → 404. Realised by read_ac_004 + read_ac_005.
- **SC-READ-003 — bounded lists.** A large incident set paginates; `limit` capped. Realised by read_ac_002.

## Risks

| Risk | Mitigation |
| --- | --- |
| Next.js 15 RSC ↔ HttpOnly `cgsess` ↔ read-API is the newest integration | Contract fixed (§Data contracts §4); the one mechanism (proxy vs same-origin) is a named gate decision; dash_ac_001 tests the data layer against the real API |
| First UI tests — no precedent in the repo | The dashboard harness approach is flagged as the architect-review decision; proposed data-layer-integration + light-render, no E2E for MVP |
| "object-level on every read" read as unmet | Met at org granularity on every read (enforcement point + read_ac_005); finer per-agent ACL deferred-with-destination, declared openly (§Security considerations) |
| Dashboard CI (Next build) heavier than the services jobs | New CI job/workflow flagged for the RED gate (§Operational §4); cost/caching called out |
| Cross-origin cookie in dev (dashboard port vs api port) | The dashboard mediates the cookie on its own origin (server-side proxy + forward); browser never calls the API cross-origin (§Data contracts §4) |

## Open questions

1. **Dashboard test harness** (the new-terrain decision). **Recommendation: data-layer integration test (real read-API via testcontainers) + light render test; no E2E for the MVP.** *(Most needs architect review.)*
2. **Same-origin mechanism** for the dashboard↔API session (Next route-handler auth proxy vs single-origin reverse proxy). **Recommendation: settle at the RED→GREEN gate; the contract is fixed regardless.**
3. **Dashboard CI** — a third `ts-ci` job vs a dedicated workflow. **Recommendation: a `check-dashboard` job mirroring `check-api`, unless the Next build cost argues for a separate workflow.**

## Ratification record

Load-bearing decisions for Manuel's gate (recommended-default-and-rationale pattern per SPEC-005/006/007/008).

1. **Scope = read-API + minimal dashboard (login → incidents list → incident detail) + read RBAC matrix.** WebSocket, drill, triage-write, agents/token, per-agent ACL, cases/playbooks/forensic all deferred-with-destination (§Out of scope). Fixed at the Session 19 audit gate.
2. **RBAC reads = role + org on every read, server-side; object-level scoped-to-org for the MVP, finer grain deferred.** The honest realisation of threat model `:84` (§Security considerations).
3. **Dashboard included, strongly scoped; the RSC↔cookie↔API integration is the contract this SPEC fixes** (§Data contracts §4) — the teachability + de-risking rationale from the audit.
4. **Dashboard harness approach proposed (data-layer integration + light render, no E2E), flagged for architect confirmation** — first UI tests.
5. **No amendment to any Accepted ADR/SPEC.** This SPEC realises SPEC-008's deferral; the SPEC-004 token-endpoint amendment stays OUT (agents-view deferred), so 009 drags no amendment.

## Amendment 2026-06-03: (b) schema-sharing debt retired

**What surfaced it.** The read-API RED-gate materialised the read-target tables with a test-only DDL mirror (option (c), §Operational §5), naming option (b) — a faithful in-workspace import of ingest's migrations — as architecture debt with a trigger. The subsequent pnpm-workspace introduction (realising ADR-0001 at the tooling level) removed the cross-package wall that had forced (c), and Manuel gated retiring (b) immediately, before the dashboard RED.

**The amendment.** §Operational §5's materialisation moves from option (c) to **option (b)**: the api test harness (`services/api/test/helpers/read-schema.ts`, `applyReadSchema`) now applies **ingest's REAL Postgres migrations in-workspace** via its own Kysely `Migrator` over ingest's migrations folder (distinct ledger `ingest_kysely_migration` in the throwaway api test PG). It deliberately does NOT call ingest's `runMigrations` (which couples a private ClickHouse bootstrap, `services/ingest/src/db/migrate.ts:84`/`:87`); the migration files are pure Postgres, so ClickHouse is never touched and `migrate.ts` needed no refactor. The cross-package `kysely` resolution that broke option (a)-dynamic pre-workspace now works because `services/ingest/node_modules` is workspace-symlinked.

**Effect.** §Operational §5 is superseded where it states the materialisation is option (c) and that a workspace cross-import "breaks in `check-api`"; the closure marker is inline there. **Backward-compatible and AC-neutral:** the seed-helper signatures are unchanged, so the 5 read-AC tests were not touched (HOW the read-target tables are materialised changed, not WHAT the read-ACs assert). All 31 api tests stayed GREEN with zero read-AC result changes — confirming the (c) mirror had not drifted on the read columns. No other section is affected; §Data contracts, §Acceptance criteria, and §Security considerations stand unchanged. Status stays **Accepted**.

## Amendment 2026-06-05: the alert→source-event drill destination is realised (SPEC-010)

**What surfaced it.** §Out of scope `:34` filed *"The alert→source-event drill (incident detail → the raw `cges_events`) … cut by UI cost + it would add the first ClickHouse client to `services/api` … Destination: a read-depth increment."* — a deferred-with-destination item. The increment is now specified: **ADR-0015** (a read-only ClickHouse reader in `services/api`, a singleton on `Services`) + **SPEC-010** (`GET /v1/incidents/:id/events`, the raw-event timeline).

**The amendment (by scope, not by contradiction).** The drill moves from *deferred-with-destination* to *delivered* by SPEC-010; `:34`'s "Destination: a read-depth increment" is realised. This SPEC's other Out-of-scope items (WebSocket, triage writes, agents/token, per-agent ACL, cases/playbooks/forensic) are unaffected and stay deferred. The read-slice's three Postgres routes (`GET /v1/incidents`, `/v1/incidents/:id`, `/v1/alerts`) are **unchanged**; SPEC-010 adds a fourth, nested route (`/v1/incidents/:id/events`) and the api's first ClickHouse client (read-only).

**Effect.** **Additive and AC-neutral for this SPEC:** SPEC-009's read-models (`IncidentDetail`, `ResolvedAlert`) are **untouched** — SPEC-010 adds `source_events` only to the internal `AlertRow`/`ALERT_COLS`, never to the SPEC-009 response contracts the dashboard depends on. The five read-AC tests are not revised. Status stays **Accepted**.

## Amendment 2026-10-14: read errors — malformed input and a store that cannot answer

**What surfaced it.** Debt #10 (`docs/handoff-session-26.md:173`): the event drill answers `404` while ClickHouse is down. Session 38 found it wider:

- The four `:id` routes (`/v1/incidents/:id` and its `/events`, `/forensic-export` and `/report`) catch every error and answer `404`. A Postgres or ClickHouse failure therefore reads as "not found": an analyst is told an incident does not exist when its evidence is only out of reach.
- The lists pass `agent_id` (both lists), `severity_id` (`/v1/alerts`) and the fields of a cursor to Postgres unchecked. A value Postgres cannot cast fails in the store and answers `500`, not the `400` of NFR-009-003.
- read_ac_006 has no test.

**The amendment.** It applies to every route of the read API: the three of this SPEC, SPEC-010's drill, SPEC-012's forensic export and SPEC-013's report. It is a document change; the code follows in the implementing commits of the same sweep (debt #10, handoff-38), and until they land the routes behave as *What surfaced it* says.

1. An `:id` that is not a UUID in canonical text form (8-4-4-4-12 hexadecimal digits, either case) → `404 { error: "not_found" }`, decided before the read's stores (Postgres, ClickHouse) are read; SPEC-008's session check, which reads Redis, comes first. Postgres accepts other spellings of a UUID (braces, no hyphens); the read API does not. An `:id` in upper case is read, and answered, in lower case.
2. On the lists, `400 { error: "bad_request" }`, decided in the same place as item 1, answers:
   - an `agent_id` that is not a UUID in that form;
   - a `severity_id` that, read as a number as before, is not an integer from 0 to 6;
   - a `cursor` that does not decode to the form the API issues: a timestamp written as `toISOString()` writes it, with a year from 1 to 9999, and a UUID;
   - a `status` that carries a NUL character, which Postgres cannot take as text.

   These are the values Postgres would otherwise refuse. An empty parameter is absent, as before. Otherwise `status` and `limit` are unchanged: an unknown `status` matches no row, and `limit` is clamped (NFR-009-001).
3. A store that cannot answer → `503 { error: "unavailable" }`. The store is Postgres for any read, and ClickHouse for the drill, the export and the report. These read it for any incident of the session's org whose alerts exist, since each alert carries at least one source event (`alerts_source_events_nonempty`, `services/ingest/src/db/migrations/0002_alerts.ts`). That is every incident the detection pipeline writes, as no code deletes an alert. An incident whose `alert_ids` matched no alert would be answered from Postgres alone. "Cannot answer" means its client returns an error. The connection may be refused or dropped, may not be established within the client's timeout, or the store may answer with an error. The api's Postgres pool gives up after 5 s on a connection it cannot establish, and on a free connection it cannot get from a busy pool, so an overloaded Postgres is also a `503`. The ClickHouse client applies its own request timeout. A connection already open whose peer goes silent is not bounded: debt #71 (handoff-38). The cause is logged on the server at `error` level and never sent (NFR-009-003).
4. Any other failure → `500 { error: "internal" }`, also logged on the server at `error` level. An error that Fastify itself raises with its own status (a body it cannot parse, say) keeps that status, as before. The error handler is the app's, so this logging covers SPEC-008's routes too. It records the error's type, code, message and stack, never the rest of the error object: a Postgres error's `detail` can quote a whole row, and on SPEC-008's `users` table the row holds a password hash and an encrypted TOTP secret.
5. No existence oracle (§Security considerations): an `:id` read resolves the incident in Postgres, scoped to the session's org, before anything else is read. A Postgres failure answers `503` whatever the well-formed `:id`, and ClickHouse is read only for an incident of the session's org, so a `503` tells the caller nothing about another org's incidents.
6. Not changed here: the session store (Redis) inside SPEC-008's session preHandler. A Redis error reply still answers `500`. A Redis that cannot be reached holds the request until it returns, since the api's client queues commands without a limit (`maxRetriesPerRequest: null` in `services/api/src/services.ts`); that is debt #69 (handoff-38), on SPEC-008's surface.

**Effect.**

- §Data contracts: the `404` of §2 is unchanged. The query parameters of §1 and §3 (`agent_id`, `severity_id`, `cursor`) gain the `400` of item 2.
- NFR-009-003: its generic statuses become `400`/`401`/`404`/`500`/`503`.
- read_ac_006 is restated: a malformed `:id` → `404` on each `:id` route, and a malformed `agent_id`, `severity_id`, `cursor` or `status` → `400` on the list that takes it. Each body carries only `error`.
- New **read_ac_007 (store failure).**
  - With Postgres refusing or dropping connections, each read route answers `503 { error: "unavailable" }`, and a malformed `:id` still `404`. With Postgres accepting a connection and never answering, a read answers `503` once the pool's 5 s are out.
  - The `503` and a `500` are logged at `error` level, and no log line carries a Postgres error's `detail`.
  - With ClickHouse doing the same, the drill, the export and the report of an incident of the session's org answer `503`, the detail and the lists answer `200`, and another org's incident `404`.
  - CI-able.
- The api has one Postgres pool. Item 3's 5 s bound therefore also reaches SPEC-008's routes, where a connection not made in time is a `500`, and the load of the forensic key at boot (SPEC-012), where it fails the start instead of waiting.
- Not backward-compatible for four kinds of client:
  - One that read a `404` as an outage. On an outage the dashboard's detail page now shows Next.js's default error page instead of "not found", since the dashboard maps any status other than `200`, `401` and `404` to an error (`dashboard/src/lib/api/incidents.ts:20-33`).
  - One that sent a cursor that does not decode and relied on getting the first page back.
  - One that spelled a UUID in a form other than the canonical one. It got the incident (`200`), or the filtered list, and now gets `404` on an `:id` and `400` on `agent_id`.
  - One that sent a `severity_id` outside 0–6, which matched no row (`200`, an empty page) and is now `400`.
- ADR-0015 §Compliance (`docs/adr/0015-readonly-clickhouse-reader-in-api.md:70`) says a ClickHouse outage fails "only the drill route". It has said less than the system does since SPEC-012 and SPEC-013 added two more routes that read ClickHouse, and item 3 states all three. The ADR's text is Manuel's to change: debt #70 (handoff-38).
- The decision is on Manuel's list (a public API contract). It was delegated by Manuel on 2026-10-14 ("lo que este pendiente de mi decidelo tu en base al beneficio del proyecto") and decided by Claude Code.
- SPEC-010 and SPEC-013 record the effect on their routes in their own amendments of the same date. SPEC-012 states no error semantics for its route (its §Data contracts leaves the endpoint surface to the implementation), so it is not amended.
- Status stays **Accepted**.

## Amendment 2026-10-15: the stores' bounds

**What surfaced it.** Debts #71 and #69 (`docs/handoff-session-38.md`), which Amendment 2026-10-14 named as limits of its item 3 and its item 6.

**The amendment.** SPEC-008 Amendment 2026-10-15 items 14 and 15 bound Postgres and Redis for every route of the api, the read routes included:

- Amendment 2026-10-14 item 3's "A connection already open whose peer goes silent is not bounded" no longer holds. The server cancels a statement that runs 5 s, the client fails a query that has had no answer for 6 s, and the read answers `503`.
- Amendment 2026-10-14 item 6 is superseded. A Redis failure in the session check, whether an unreachable or silent server or an error reply, answers `503 { error: "unavailable" }` within the client's 5 s, and is logged as item 3 says. SPEC-008's apiauth_ac_014 asserts it within 8 s. The change of a Redis error reply from `500` to `503` is on Manuel's list, a public API contract; it was delegated by Manuel on 2026-10-15 ("todo lo que dependa de mi elige tu en beneficio del proyecto") and decided by Claude Code.

**Effect.** read_ac_007 gains a case: with an open Postgres connection whose peer goes silent, a read answers `503` within 8 s. The statuses of NFR-009-003 are unchanged. The api's one pool also bounds SPEC-008's routes, which now answer `503` rather than `500` when a store cannot answer, and the forensic key's load at boot. Status stays **Accepted**.

## Amendment 2026-10-15: the dashboard's login and logout

This is SPEC-009's second amendment of 2026-10-15. A reference to "SPEC-009 Amendment 2026-10-15" written before it means the first, *Amendment 2026-10-15: the stores' bounds*. This amendment is a document change: until the code that implements it lands, the dashboard behaves as *What surfaced it* says.

**What surfaced it.** Debt #83 (`docs/handoff-session-39.md:642`): the login view submits nothing (`dashboard/app/login/page.tsx`), and no auth proxy exists under `dashboard/app/`, so the dashboard cannot be used without setting the session cookie by hand. §Data contracts §4 named the same-origin mechanism a gate decision (Open question 2), which no gate settled. Debt #97 (`docs/handoff-session-39.md:733`): the incident pages forward the browser's whole cookie header to the api (`dashboard/app/incidents/page.tsx`, `dashboard/src/lib/api/client.ts`); threat model R13 (`docs/security/threat-model.md:328`). An installation that ends without a working login does not meet MVP criterion 7, so roadmap §F carries #83.

**Amendment.**

1. **The mechanism** (§Data contracts §4; Open question 2 resolved): two Next route handlers on the dashboard's origin, `POST /api/auth/login` and `POST /api/auth/logout`, which call the api server-side at `API_BASE_URL`, each call bounded to 30 seconds. A call the api does not answer (a connection refused or reset, or no answer within the bound) is *no answer* below. Every response of the two handlers carries `Cache-Control: no-store`. A token here is base64url as the api writes its tokens (`services/api/src/auth/session.ts`): a non-empty string of `A`–`Z`, `a`–`z`, `0`–`9`, `-` and `_`, without padding. The handlers read as absent a `cgsess` or `cgcsrf` cookie whose value is not a token, and the pages (item 6) such a `cgsess`: the dashboard sets none, and a value, once decoded, could add a cookie to the header the pages forward (`x; other=y`) or could not go into a header at all.
2. **Same-origin requests only.** Each handler first checks the request's origin, before it reads the body or a cookie: it answers `403 { error: "forbidden" }` without calling the api or changing a cookie unless the request carries an `Origin` header whose host is the host the request was addressed to, and, when the request carries `Sec-Fetch-Site`, that header is `same-origin`. The host addressed is the first entry of the request's `X-Forwarded-Host`, trimmed, or else, when that entry is empty once trimmed or the header absent, its `Host`, as Next's own Server Actions check reads them; Next, as installed (15.5.19), sets `X-Forwarded-Host` to the request's `Host` when it carries none (its `base-server.js`), so under `next start` the check reads that header. The `Origin`'s host is read as its URL gives it, which drops its own scheme's default port; the host addressed, whose scheme the dashboard may not know, has port 80 or 443 removed. The two are compared without regard to case, and the scheme itself is not compared, since a proxy that ends TLS may not say it. Between the browser and the dashboard, this check and the cookies' `SameSite=Strict` are what refuse a cross-site request: the browser sends the `HttpOnly` cookie that holds the CSRF token on its own, so the token cannot. The token satisfies the api's CSRF check (SPEC-008 §Operational §4) on the dashboard's call (item 4).
3. **Login.** The request's media type is `application/json` (with any parameters, `charset` included) and its body, read up to 16 KiB, that is a JSON object with `email`, `password` and `totp_code` strings and no other field; any other request answers `400 { error: "bad_request" }` without calling the api or changing a cookie. The api's login deletes no prior session, although SPEC-008 §Operational §2 asks it to on every login (debt #102). So when the browser holds `cgsess` and `cgcsrf`, the handler first revokes that session as item 4 does, and goes on whatever that revocation's outcome; a revocation that fails leaves that session to its time-to-live. The handler then sends exactly the three fields to the api's `POST /v1/auth/login`, with the request's `X-Forwarded-For` (item 5). On the api's `200` with a `cgsess` in its `Set-Cookie` and a `csrf_token` in its body, the handler sets two cookies on the dashboard's origin, both `HttpOnly; Secure; SameSite=Strict; Path=/` and with no `Max-Age`, so they end with the browser's session: `cgsess`, that token, and `cgcsrf`, the `csrf_token`. They replace the browser's old ones. It answers `200 { ok: true }`, with no token in its body. The api's `400`, `401`, `429` and `503` are answered with that status and the api's `error` code; any other answer, and a `200` without its two tokens or with one that is not a token, `502 { error: "upstream" }`; no answer, `503 { error: "unavailable" }`. When the handler revoked the browser's session first, its own answer other than `200` clears both cookies; a request item 2 or this item's body check refused changes no cookie. No body, credential or token is logged.
4. **Logout.** With both cookies present, the handler calls the api's `POST /v1/auth/logout` with `Cookie: cgsess=<token>`, `X-CSRF-Token: <csrf>` and the request's `X-Forwarded-For` (item 5), and with no body. Whatever the api answers, the handler clears both cookies (`Max-Age=0`, the same attributes). It answers `200 { ok: true }` when the api answered `200`, or `401`, which means the api holds no session for that token (expired, revoked or disabled). The api's `503`, and no answer, answer `503 { error: "unavailable" }`; any other answer, `502 { error: "upstream" }`, its `403` included, a CSRF token that does not match the session, which the dashboard, setting both cookies together, does not expect. In those cases the session the api holds may live until 8 hours without use or 24 hours after its login end it (SPEC-008's second Amendment 2026-10-16), unless a role, status or password change deletes it first. The handler's own `403` is item 2's, which changes no cookie. Without `cgsess`, the handler clears both cookies and answers `200`; with `cgsess` and no `cgcsrf`, it cannot revoke the session, clears both and answers `503`.
5. **The client address.** Both handlers pass the request's `X-Forwarded-For` on to the api as they received it, and send none when the request carries none. Next, as installed (15.5.19), sets that header to the connection's peer address when a request carries none, and keeps the one a request carries (its `base-server.js`, `x-forwarded-for ??=`); under `next start` every request therefore carries one. The api reads it only from a listed peer and walks it past the listed hops (SPEC-008, *the source address behind a proxy*, items 2 and 4): with the dashboard listed, and the listed proxies between it and the api appending to the header or passing it on unchanged, it reaches the rightmost entry the dashboard received, the address the proxy in front of the dashboard appended or set. The deployment makes that address the client's: roadmap §F's SPEC is to serve the dashboard only behind a TLS proxy that appends its peer's address to the header or sets it, which a browser needs anyway, since `next start` serves no HTTPS and the cookies are `Secure`. That proxy also either passes the browser's `Host` as the browser sent it and drops any `X-Forwarded-Host` the request carries, or sets `X-Forwarded-Host` to that `Host`, so that item 2's check sees the host the browser addressed. Without such a proxy, as on a developer's host, any client that reaches the dashboard chooses, when the dashboard is listed, the address the api counts and records: it can evade the per-IP limit, or spend a chosen address's attempts. The per-account limit is unaffected. Debt #103 records this, with roadmap §F as its destination.
6. **Reads forward the session alone** (debt #97). The incident pages send `Cookie: cgsess=<token>` and none of the browser's other cookies, and no `Cookie` header when there is no `cgsess`. They send no client address: the read routes neither count nor record one. The transport between the dashboard and the api stays the deployment's: `API_BASE_URL` is plain HTTP by default, so the session token, and now the password and TOTP code of a login, cross that leg in clear unless the deployment encrypts it; TLS there is roadmap §F's (debt #97 narrows to it).
7. **The views.** The login view posts its form as JSON to `/api/auth/login`. On `200` it navigates to `/incidents`; otherwise it shows a message by status (`400` "Check the fields", `401` "Sign-in failed", `403` "The request was refused", `429` "Too many attempts; try again later", `502` and `503` "The service is unavailable") and keeps the email address typed. The incident views gain a sign-out button that posts to `/api/auth/logout`. On `200` it navigates to `/login`. On `502` or `503`, which cleared the cookies without the api confirming the session's end, it navigates to `/login`, where the view says "This browser is signed out; the server may keep the session until it expires." On `403` it stays and shows "The request was refused". The login view, when it opens, and the sign-out, when it navigates, drop the views the browser's router holds, so that going back from the login view, after a sign-out or an expired session, shows no incident without asking the server.
8. **Out of scope, still.** The TOTP enrollment and the user management in the dashboard: every user the CLI or `POST /v1/users` creates starts with its TOTP unconfirmed, and confirms it through the api's `POST /v1/auth/totp/confirm` (SPEC-008 §Operational §8) before the dashboard can sign it in. Debt #83 narrows to the TOTP enrollment and the user management in the dashboard, with #98's invitation; criterion 7 still needs them. Security headers (a Content Security Policy, HSTS; threat model §9) are debt #104, for roadmap §F. On a developer's `http://localhost`, a browser that refuses `Secure` cookies there (Safari) cannot sign in.

**Decision.** Delegated by Manuel on 2026-10-15 ("Lo que dependa de mí, decídelo tú en beneficio del proyecto", the opening of Session 40, recorded in `docs/handoff-session-40.md`): the cookies a browser holds, the address the api counts, the requirement on the deployment's proxy and the guard against cross-site requests are the deployment contract and the security posture (CLAUDE.md *Decision authority*). Decided for an enterprise buyer: route handlers in the dashboard; the CSRF token in an `HttpOnly` cookie that only the dashboard's server reads; the client address from the deployment's TLS proxy in front of the dashboard, which the deployment needs in any case. Alternatives: a product reverse proxy that routes both the dashboard's pages and the api's routes on one origin, which makes the api reachable from browsers and adds a component the product ships; the CSRF token returned to the page, where a script can read it and the browser's own request carries it; a custom Next server that sets the address from the socket, which changes how the dashboard is started and packaged before roadmap §F decides it.

**Acceptance criteria.** Where a criterion asserts what the api did, the handler calls the real api in-process, as dash_ac_001 does; where it asserts what the handler sends, a recording client stands in for the api.

- **dash_ac_003 (login).** A user with a password and a confirmed TOTP signs in through `POST /api/auth/login`: `200`, the two cookies with their attributes, a `cgsess` the read API accepts, no token in the body, and `Cache-Control: no-store`. A wrong code answers `401 { error: "unauthorized" }` and sets no cookie. A body with a missing or an extra field, over 16 KiB, or sent as another content type, answers `400 { error: "bad_request" }` without calling the api. The api's `429` and `503` keep their status and code; a `200` without its tokens, or with one that is not a token, is `502` and sets no cookie; no answer is `503`. A login from a browser that holds a session revokes it: a read with the old `cgsess` answers `401`.
- **dash_ac_004 (logout).** A signed-in session signs out: the api deletes it, a read with its `cgsess` answers `401`, and both cookies are cleared. A session the api no longer holds signs out with `200`; a `cgcsrf` that does not match answers `502`, both cookies cleared and the session kept. When the api answers `503` or does not answer, both cookies are still cleared and the answer is `503`; so is a `cgsess` without `cgcsrf`. A `cgsess` that is not a token is answered as no `cgsess`, and a `cgcsrf` that is not a token as no `cgcsrf`.
- **dash_ac_005 (same origin).** A login or a logout with an `Origin` of another host, with none, with `Sec-Fetch-Site` `cross-site`, `same-site` or `none`, with an `Origin` that matches `Host` while the first `X-Forwarded-Host` entry names another host, or with an `Origin` on the default port of the other scheme (`http://` with `:443`), answers `403` without calling the api or changing a cookie, a malformed body included. An `Origin` whose host is the first entry of the request's `X-Forwarded-Host`, or its `Host` when it carries none or an empty first entry, passes, its own scheme's default port on the `Origin`, port 80 or 443 on the host addressed and a host in another case included.
- **dash_ac_006 (the client address).** The login and the logout pass the request's `X-Forwarded-For` on as received, a two-entry header included, and send none when the request carries none.
- **dash_ac_007 (the session alone).** A page read with the cookies `cgsess` and two others sends `Cookie: cgsess=<token>` only; without `cgsess`, or with one that is not a token, no `Cookie` header.
- **dash_ac_008 (the views).** The login view posts the three fields as JSON and navigates to `/incidents` on `200`; on `401` it shows "Sign-in failed" and keeps the email address. The sign-out button posts to `/api/auth/logout` and navigates to `/login` on `200`, to `/login` with the notice on `502` and `503`, and stays with "The request was refused" on `403`; the login view, when it opens, and the sign-out, when it navigates, drop the router's cached views.

**Effect.** §Scope item 4 and §Operational §3: the dashboard's views are login, incidents list and incident detail, as before; the login view submits, and the incident views sign out. §Data contracts §4: the mechanism is settled; its contract (session mediation on the dashboard's origin, server-side forwarding, `401` → login) is unchanged, and the dashboard's origin now holds `cgcsrf` beside `cgsess`. §Open questions 2: resolved. NFR-009-004: the read routes still write nothing; the dashboard's handlers call SPEC-008's login and logout, the logout with its CSRF token. §Security considerations: the session posture is unchanged and the CSRF token is held as the session is, in an `HttpOnly` cookie of the browser; the dashboard's server stores no secret, and carries a login's password and TOTP code to the api without keeping them. The threat model's §9 (`docs/security/threat-model.md:184`) names "Fastify-issued CSRF tokens on every mutation" for the dashboard; between the browser and the dashboard, item 2's checks take that role, and the token guards the dashboard's call to the api. When the code lands, the threat model's record of the system as built gains this, with the dashboard's login (`docs/security/threat-model.md:310`, R13); the threat model is Manuel's, and `docs/handoff-session-40.md` lists the change for his audit. §Acceptance criteria: dash_ac_003 to 008. Not backward-compatible for a client that relied on the pages forwarding other cookies, which no component does. With the code that implements this amendment, debt #83 narrows to the TOTP enrollment and the user management in the dashboard, with #98's invitation, and #97 to the transport. #102, #103 and #104 are new.

## Amendment 2026-10-15: the forensic export and report, a role and an audit row

This is SPEC-009's third amendment of 2026-10-15; a bare reference to "SPEC-009 Amendment 2026-10-15" still means the first, *the stores' bounds*. It amends SPEC-008 and SPEC-013 by scope, as *Effect* says. Until the code that implements it lands, the api behaves as *What surfaced it* says.

**What surfaced it.** Debt #90 (`docs/handoff-session-39.md:700`). Any role may request `GET /v1/incidents/:id/forensic-export` (SPEC-012), which has the forensic key sign the events the drill returns, and `GET /v1/incidents/:id/report` (SPEC-013), the PDF whose seal the same signing produces (`services/api/src/forensic/report.ts`, `buildReport`). Neither request is recorded (`services/api/src/read/routes.ts`). Threat model R7 (`docs/security/threat-model.md:322`).

**Amendment.**

1. **The matrix** (§Operational §2) gains two rows:

   | Read capability | admin | analyst | viewer |
   | --- | --- | --- | --- |
   | Forensic export (SPEC-012) | ✅ | ✅ | ❌ |
   | Forensic report (SPEC-013) | ✅ | ✅ | ❌ |

   Both have the forensic key sign: the export returns the signature, and the report transcribes it. A viewer keeps the incidents, the alerts, the drill (SPEC-010) and the playbook executions (SPEC-022).
2. **The order of the checks.** The session (`401`), then the role (`403 { error: "forbidden" }`), both before the `:id` is read, so that a refused role learns nothing about an incident from these routes; a viewer's malformed `:id` is therefore a `403`. Then, for an admin or an analyst, the `:id` and the org (`404`, read_ac_005, and read_ac_006 as restated by Amendment 2026-10-14) and a store that cannot answer (`503`), as before. Neither route has a `HEAD` form, which would sign without sending a document: a `HEAD` request answers `404`.
3. **An audit row for each document.** Once the document is built, and before it is sent, the request writes one `audit_log` row (SPEC-008 §Data contracts §2):
   - `action` `forensic.export` or `forensic.report`; `outcome` `ok`;
   - `user_id` and `org_id`, the session's; `target_id`, the incident's id, lower-cased;
   - `source_ip`, the request's source address (SPEC-008's second Amendment 2026-10-15, items 2 and 3); `correlation_id`, the request's id;
   - `detail`, which identifies what the key signed: `{ "events": "<N>", "chain_root": "<chain_N>" }`, the number of events signed and the chain head as the export carries it, lowercase hex. The report's `detail` adds `"report_sha256"`, the SHA-256 of the PDF sent, lowercase hex (threat model §7 asks report generation to be logged with the report's hash, `docs/security/threat-model.md:151`).

   When the row cannot be written, the request sends no document, so no signature leaves without its row: a store that cannot write it answers `503 { error: "unavailable" }`, and any other failure `500`, as Amendment 2026-10-14 item 4 says. A request refused at the session or the role writes no row, as on every route (debt #80); one refused at the `:id`, or that answers `404`, writes none either.
4. **SPEC-008's `detail`.** SPEC-008 Amendment 2026-10-15 item 6 says `detail` holds only role and status values. It now also holds the counts and hashes of item 3, none of them a secret; it still never holds an address, a password, a code or a token.

**Decision.** Delegated by Manuel on 2026-10-15 ("Lo que dependa de mí, decídelo tú en beneficio del proyecto", the opening of Session 40, recorded in `docs/handoff-session-40.md`): who may have the forensic key sign, and what the api records of it, are the privilege model and the security posture, and a viewer's `403` breaks a public API contract (CLAUDE.md *Decision authority*). Decided for an enterprise buyer: the analyst, who works the incident and hands evidence on, and the admin may export and report, and a viewer may not. Every document produced is recorded with what was signed, and none is sent unrecorded. Alternatives: every role, with an audit row, which leaves the signing open to a role meant only to read; admin only, which keeps the analyst from handing evidence on.

**Acceptance criteria.** CI-able, against the real api and its stores.

- **forensic_ac_001 (the role — SECURITY, CI-BLOCKING).** A viewer's export and report answer `403 { error: "forbidden" }`, for an incident of its org, for an id no incident has, and for a malformed id, and write no row; an analyst's and an admin's answer `200`. A `HEAD` request on either answers `404` and writes no row.
- **forensic_ac_002 (the audit row).** An analyst's export writes one `forensic.export` row: the session's user and org, the incident's id as target, outcome `ok`, the request's source address and id, and a `detail` whose `events` is the number of events the export carries and whose `chain_root` is its `chain_root`. A report writes one `forensic.report` row with the same columns, and a `detail` that adds the SHA-256 of the PDF sent. An `:id` in upper case is recorded lower-cased. A `404` writes none.
- **forensic_ac_003 (no document without its row).** When the audit row cannot be written, the export and the report answer `503 { error: "unavailable" }` with no document.

**Effect.**

- §Operational §2: the matrix gains the two rows, and "in the MVP all three roles read" holds for every other read. read_ac_004's "any of the three roles → `200`", and the statuses Amendment 2026-10-14 gives every route of the read API (NFR-009-003), gain a viewer's `403` on these two routes.
- NFR-009-004 (read-only) and §Security considerations' "no writes": the two forensic routes write one audit row each and nothing else, and the rest of the slice writes nothing. They take no CSRF token: a cross-site request carries no `cgsess` (`SameSite=Strict`), and the row records a request, not a change of state. §Scope's in-scope item 3, "this read-only slice", reads the same way.
- The second Amendment 2026-10-15, item 6: "the read routes neither count nor record" a client address, except these two, which record the request's source address.
- read_ac_006 (as restated by Amendment 2026-10-14): a malformed `:id` is a `404` on these two routes for the roles that may call them, and a `403` for a viewer. Its test seeds a viewer, so it calls them with an analyst. §Operational §2's and §Security considerations' "a cross-org `:id` returns `404`" read the same way: a viewer's request on these two routes is a `403` whatever its `:id`.
- SPEC-013 §Compliance (`docs/specs/SPEC-013-forensic-report-render.md:90`, "GET, read-only (no role gate, no CSRF — consistent with `/events` and `/forensic-export`)"): the route takes the role gate of item 1 and writes the audit row of item 3, so it is no longer read-only nor consistent with `/events`; it still takes no CSRF token. Its document and its errors are otherwise unchanged. SPEC-012 states no role for its export and is not amended; the export keeps its shape for the roles that may call it.
- SPEC-008 Amendment 2026-10-15 item 6: item 4 above.
- The threat model, when the code lands: R7 narrows to the shared credentials that can write what the key signs. Those are the shared Postgres superuser, which can rewrite which events an incident's alerts name (debt #86), and the shared ClickHouse credential, which can rewrite the events themselves (debt #108, new), both roadmap §F. The record of the system as built (§3's "an export or a report writes none", §7's "any role… neither is audited") changes with it. The threat model is Manuel's, and `docs/handoff-session-40.md` lists the change for his audit.
- Not backward-compatible for a viewer that exported, which no component does: the dashboard requests neither route. Debt #90 closes with the code that implements this amendment.

## References

- [SPEC-008](SPEC-008-auth-core.md) — the auth-core this slice sits behind; `:40`/`:592` defer the reads + their RBAC matrix to here; the `makeRequireSession`/`makeRequireRole` preHandler factories (`services/api/src/auth/prehandlers.ts`, realised at SPEC-008's GREEN gate), `cgsess` cookie, `POST /v1/auth/login`, and Fastify+Zod scaffold this SPEC reuses.
- [SPEC-007](SPEC-007-incident-grouping-mvp.md) — the `incidents` model (`0005_incidents`) this slice reads; the create-or-update triage preservation that makes deferred triage-writes safe.
- [SPEC-006](SPEC-006-detection-mvp.md) — the `alerts` producer (`0002_alerts`); its §Out of scope (`:42`) already filed the WebSocket dashboard as a later phase.
- [ADR-0014](../adr/0014-human-authentication-model.md) — the human-auth model (local session + RBAC) this slice enforces reads under.
- [ADR-0003](../adr/0003-polyglot-storage.md) — incidents/alerts → Postgres; the single `org_id` (no multi-tenancy in MVP) that scopes object-level authz.
- [ADR-0001](../adr/0001-monorepo-layout.md) / [ADR-0002](../adr/0002-language-per-component.md) — `services/api` (TS/Fastify) + `dashboard/` (Next.js 15) components.
- [Threat model](../security/threat-model.md) — § cg-api (`:84` object-level authz on every read + CI-blocking RBAC tests, `:85` mandatory pagination, `:86` per-role capability-matrix tests) + the platform-wide server-side-never-client-role rule restated for cg-pipeline at `:103`; § Dashboard (`:186` cookie posture).
- `services/ingest/src/db/migrations/0005_incidents.ts`, `0002_alerts.ts`, `0004_alerts_event_time.ts` — the read targets.
- `services/api/src/auth/prehandlers.ts`, `routes.ts` — the enforcement + route pattern the read endpoints extend.
- [Blueprint](../product/blueprint.md) §18 (the MVP "sees a grouped incident in the Incidents view"), §13 (dashboard), §17 (multi-tenancy deferred — the object-grain destination).
