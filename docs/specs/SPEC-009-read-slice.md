# SPEC-009: Read-slice — incident/alert read API + minimal SOC dashboard

- **ID:** SPEC-009
- **Title:** Read-slice
- **Status:** Accepted
- **Depends on:** SPEC-008 (the auth-core this slice sits behind — the `makeRequireSession`/`makeRequireRole` preHandler factories (`services/api/src/auth/prehandlers.ts`, realised at SPEC-008's GREEN gate), the `cgsess` HttpOnly cookie, the `POST /v1/auth/login` the dashboard logs in through, the Fastify+Zod scaffold + pg pool the read endpoints extend; it explicitly deferred the read endpoints + their RBAC matrix to this SPEC), SPEC-007 (the `incidents` model A produced — the rows this slice reads), SPEC-006 (the `alerts` producer; its §Out of scope filed the WebSocket dashboard as a later phase), ADR-0014 (the human-auth model), ADR-0003 (incidents/alerts → Postgres; the reads run there), ADR-0001/ADR-0002 (`services/api` = TS/Fastify; `dashboard/` = Next.js 15), the [threat model](../security/threat-model.md) § cg-api + § Dashboard (the read-authz + cookie mandates)
- **Authors:** Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)
- **Created:** 2026-06-02
- **Last updated:** 2026-06-03

## Motivation

SPEC-009 is the **last** slice of Phase 6 C and the project's **first UI**. It makes the detection pipeline *visible*: a SOC operator logs in and sees the grouped incidents A produces, each with its correlated alerts and MITRE mapping (Blueprint §18 MVP: *"the SOC … sees a grouped incident in the Incidents view"*). It is the read half of C, gated behind SPEC-008's auth: SPEC-008 explicitly deferred *"all incident/alert reads … the list, the incident detail with grouped alerts + MITRE … SPEC-009 (the read-slice). The RBAC matrix rows for those read capabilities are SPEC-009's"* (`docs/specs/SPEC-008-auth-core.md:40`, `:275`).

It re-litigates nothing from SPEC-008 or ADR-0014: it inherits SPEC-008's realised auth primitives — the session preHandler (`makeRequireSession`) and role gate (`makeRequireRole`), the opaque `cgsess` cookie, and the Fastify+Zod scaffold; it adds **read endpoints** on `services/api` (greenfield — 008 was auth-only, `services/api/src/auth/routes.ts` is the only route surface today) and a **strongly-scoped minimal dashboard** (`dashboard/`, README-only today). It reads the data model A landed: `incidents` (`services/ingest/src/db/migrations/0005_incidents.ts`) + `alerts` (`0002_alerts.ts` + `event_time` from `0004_alerts_event_time.ts`), Postgres-only (ADR-0003).

The scope was fixed at the Session 19 audit gate: **read-API + a minimal dashboard (login → incidents list → incident detail), RBAC on reads, with WebSocket, the alert→event drill, triage writes, and the agents/token view all deferred** (each with a destination, §Out of scope).

## Scope

### In scope

1. **Read endpoints on `services/api`** (extending the SPEC-008 `routes.ts` pattern, behind its session preHandler `makeRequireSession`): `GET /v1/incidents` (list + filters + pagination), `GET /v1/incidents/:id` (detail with resolved alerts + MITRE — the teachable view), `GET /v1/alerts` (list + filters). §Data contracts §1–§3, §Operational §1.
2. **The read RBAC matrix** SPEC-008 deferred (`:40`, `:275`): role + org enforcement on **every** read, server-side, never from a client-claimed role (threat model `:84` cg-api; the same server-side-RBAC rule is restated platform-wide for cg-pipeline at `:103`). §Operational §2.
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

The reads project the existing Postgres tables; **no schema change** (incidents `0005`, alerts `0002`+`0004`). Read-models are response shapes, Zod-validated on the way out (ADR-0002 Rule 4). All reads are **scoped to the session's `org_id`** (§Operational §2).

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

**The amendment.** §Operational §5's materialisation moves from option (c) to **option (b)**: the api test harness (`services/api/test/helpers/read-schema.ts`, `applyReadSchema`) now applies **ingest's REAL Postgres migrations in-workspace** via its own Kysely `Migrator` over ingest's migrations folder (distinct ledger `ingest_kysely_migration` in the throwaway api test PG). It deliberately does NOT call ingest's `runMigrations` (which couples a private ClickHouse bootstrap, `services/ingest/src/db/migrate.ts:83`/`:86`); the migration files are pure Postgres, so ClickHouse is never touched and `migrate.ts` needed no refactor. The cross-package `kysely` resolution that broke option (a)-dynamic pre-workspace now works because `services/ingest/node_modules` is workspace-symlinked.

**Effect.** §Operational §5 is superseded where it states the materialisation is option (c) and that a workspace cross-import "breaks in `check-api`"; the closure marker is inline there. **Backward-compatible and AC-neutral:** the seed-helper signatures are unchanged, so the 5 read-AC tests were not touched (HOW the read-target tables are materialised changed, not WHAT the read-ACs assert). All 31 api tests stayed GREEN with zero read-AC result changes — confirming the (c) mirror had not drifted on the read columns. No other section is affected; §Data contracts, §Acceptance criteria, and §Security considerations stand unchanged. Status stays **Accepted**.

## Amendment 2026-06-05: the alert→source-event drill destination is realised (SPEC-010)

**What surfaced it.** §Out of scope `:34` filed *"The alert→source-event drill (incident detail → the raw `cges_events`) … cut by UI cost + it would add the first ClickHouse client to `services/api` … Destination: a read-depth increment."* — a deferred-with-destination item. The increment is now specified: **ADR-0015** (a read-only ClickHouse reader in `services/api`, a singleton on `Services`) + **SPEC-010** (`GET /v1/incidents/:id/events`, the raw-event timeline).

**The amendment (by scope, not by contradiction).** The drill moves from *deferred-with-destination* to *delivered* by SPEC-010; `:34`'s "Destination: a read-depth increment" is realised. This SPEC's other Out-of-scope items (WebSocket, triage writes, agents/token, per-agent ACL, cases/playbooks/forensic) are unaffected and stay deferred. The read-slice's three Postgres routes (`GET /v1/incidents`, `/v1/incidents/:id`, `/v1/alerts`) are **unchanged**; SPEC-010 adds a fourth, nested route (`/v1/incidents/:id/events`) and the api's first ClickHouse client (read-only).

**Effect.** **Additive and AC-neutral for this SPEC:** SPEC-009's read-models (`IncidentDetail`, `ResolvedAlert`) are **untouched** — SPEC-010 adds `source_events` only to the internal `AlertRow`/`ALERT_COLS`, never to the SPEC-009 response contracts the dashboard depends on. The five read-AC tests are not revised. Status stays **Accepted**.

## References

- [SPEC-008](SPEC-008-auth-core.md) — the auth-core this slice sits behind; `:40`/`:275` defer the reads + their RBAC matrix to here; the `makeRequireSession`/`makeRequireRole` preHandler factories (`services/api/src/auth/prehandlers.ts`, realised at SPEC-008's GREEN gate), `cgsess` cookie, `POST /v1/auth/login`, and Fastify+Zod scaffold this SPEC reuses.
- [SPEC-007](SPEC-007-incident-grouping-mvp.md) — the `incidents` model (`0005_incidents`) this slice reads; the create-or-update triage preservation that makes deferred triage-writes safe.
- [SPEC-006](SPEC-006-detection-mvp.md) — the `alerts` producer (`0002_alerts`); its §Out of scope (`:42`) already filed the WebSocket dashboard as a later phase.
- [ADR-0014](../adr/0014-human-authentication-model.md) — the human-auth model (local session + RBAC) this slice enforces reads under.
- [ADR-0003](../adr/0003-polyglot-storage.md) — incidents/alerts → Postgres; the single `org_id` (no multi-tenancy in MVP) that scopes object-level authz.
- [ADR-0001](../adr/0001-monorepo-layout.md) / [ADR-0002](../adr/0002-language-per-component.md) — `services/api` (TS/Fastify) + `dashboard/` (Next.js 15) components.
- [Threat model](../security/threat-model.md) — § cg-api (`:84` object-level authz on every read + CI-blocking RBAC tests, `:85` mandatory pagination, `:86` per-role capability-matrix tests) + the platform-wide server-side-never-client-role rule restated for cg-pipeline at `:103`; § Dashboard (`:186` cookie posture).
- `services/ingest/src/db/migrations/0005_incidents.ts`, `0002_alerts.ts`, `0004_alerts_event_time.ts` — the read targets.
- `services/api/src/auth/prehandlers.ts`, `routes.ts` — the enforcement + route pattern the read endpoints extend.
- [Blueprint](../product/blueprint.md) §18 (the MVP "sees a grouped incident in the Incidents view"), §13 (dashboard), §17 (multi-tenancy deferred — the object-grain destination).
