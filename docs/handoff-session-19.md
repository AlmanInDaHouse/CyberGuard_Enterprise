# Handoff — End of Session 19

Canonical state-of-the-world at the close of Session 19. This document is the contract Session 20 resumes from; it is written to let a **cold or compacted** session recover the thread in one read — nothing here assumes the Session 19 conversation.

Session 19 delivered **Phase 6 = C — the user-facing slice of the MVP**, in full: the human authentication model (ADR-0014), the auth-core (SPEC-008), the read-API + minimal SOC dashboard (SPEC-009), and the pnpm-workspace that made in-process cross-package testing possible. C is the project's **first UI** and its **first human-facing security surface**. All three `ts-ci` jobs are green and Known CI debt is back to zero.

- **Anchor commit:** `03a6eb1` (`docs(handoff): Session 19 close`) — substituted into the placeholder by this follow-up commit per the two-commit anchor pattern (cf. Session 18 `5df7f72`/`2d2d6fc`).
- **Phase 6 C GREEN delivery SHA (closes C):** `d017630` (`feat(spec-009): dashboard GREEN — data-access + render impl turns check-dashboard green, closes slice C`).
- **Branch:** `main`
- **Date:** 2026-06-04
- **CI verdict at the GREEN SHA (`d017630`):** `ts-ci` **success** — all three jobs green: `check` (ingest — tsc + biome + vitest), `check-api` (api — 31 tests), `check-dashboard` (dashboard — next build + tsc + biome + vitest 6). `markdown-lint` success. Verdict: **ALL GREEN**.
- **Known CI debt:** **ZERO rows** ([CLAUDE.md](../CLAUDE.md) §Known CI debt). The two RED-by-design rows opened this session (`ts-ci/check-api` at the auth + read-API RED gates, `ts-ci/check-dashboard` at the dashboard RED gate) were each removed in the same SHA that turned the job green (Convention #13).
- **Working tree:** clean (only `.claude/` untracked, pre-existing local config).
- **Catalogs:** ADR — 14 entries (`0001`–`0014`), all Accepted ([docs/adr/README.md](adr/README.md)). SPEC — 9 entries (`SPEC-001`–`SPEC-009`), all Accepted ([docs/specs/README.md](specs/README.md)).

## Session 19 commit arc (C, end to end)

The arc is a chain, not a list — each link's *why* is in the right column. SHAs verified against `git log`.

| Commit | SHA | Scope / why |
|---|---|---|
| ADR-0014 Proposed | `e21413a` | Human authentication model — local self-hosted, password + TOTP. Drafted Proposed for the gate. |
| ADR-0014 Accepted | `47d0f74` | Flip → Accepted + atomic landing checklist. Decides: identity (local password Argon2 + TOTP RFC 6238, §1); session form (**opaque server-side token in Redis**, stateful, immediately revocable, §2 — JWT rejected, A2); API location (a **new `services/api`** component, distinct from the agent-mTLS `services/ingest` boundary, §3); RBAC (three fixed roles `admin`/`analyst`/`viewer`, role-on-record, server-side, §4). No storage re-decision, no ADR amendment (lands on ADR-0003's homes). |
| SPEC-008 Proposed | `7db71ed` | auth-core: local password + TOTP, opaque Redis sessions, server-side RBAC, rate-limit, CSRF. Realises ADR-0014, decides nothing new. |
| SPEC-008 Accepted (+ migration-ownership correction) | `0f35984` | Flip → Accepted + **Option A** migration ownership: `services/api` owns its `0001_users`/`0002_audit_log` under its **own** runner (own Kysely ledger + own advisory-lock key, same Postgres instance — ADR-0003 one instance), so human-secret DDL (`password_hash`, `totp_secret`) never lives in the agent-facing `services/ingest` tree (`SPEC-008-auth-core.md:167`). Extends ADR-0014 §3 component separation from runtime to deploy-time; contradicts no Accepted ADR. |
| SPEC-008 harness-first RED | `fc33274` | `services/api` scaffold (Fastify+Zod, config, migrate runner) + 9 `auth_ac` tests red by design; `check-api` job added to `ts-ci`; Known CI debt co-located with `<RED-SHA>` placeholder. |
| RED SHA fill | `81b1a99` | Two-commit pattern: substitutes the debt placeholder with `fc33274`. |
| **SPEC-008 GREEN** | **`f693089`** | auth-core logic; **9 `auth_ac` GREEN**; `check-api` debt removed. Argon2id, otplib@12 TOTP, opaque Redis sessions (immediate-revocation = `DEL`), `makeRequireSession`/`makeRequireRole`/`makeRequireCsrf` preHandlers, dual-axis rate-limit, append-only `audit_log`, CSRF on mutations. |
| Deployment wire (api) | `843405f` | chore(infra-dev): wire `services/api` into `docker-compose.dev.yml`. Infra-dev only, no app code. |
| SPEC-009 Proposed | `ac3821e` | read-slice: incident/alert read-API + minimal SOC dashboard. |
| SPEC-009 Accepted | `652938e` | Flip → Accepted + catalog row. Scope fixed at the audit gate: read-API + minimal dashboard (login → list → detail) + RBAC-on-reads; **WebSocket, the alert→event drill, triage writes, the agents view all deferred** with destinations (§Out of scope). |
| read-API harness-first RED (PART 1/2) | `17b81c7` | read endpoints stub (501) + 5 `read_ac` tests; structure green, read ACs red by design; Known CI debt co-located (placeholder). Materialisation option (c): a test-only DDL mirror (named architecture debt (b)). |
| RED SHA fill | `4b60f72` | Two-commit pattern: substitutes with `17b81c7`. |
| **read-API GREEN** | **`5e467cc`** | read-API logic; `read_ac_001/002/003/005` GREEN (`004` RBAC green-guard); `check-api` debt removed. Org-scoped reads (`WHERE org_id = session.org_id`), keyset pagination (cap 200), incident detail resolves `alert_ids` via join. |
| pnpm-workspace introduced (the (B) gate) | `15d05b4` | `pnpm-workspace.yaml` (`packages: services/*`) realises ADR-0001 monorepo at the tooling level — one root lockfile, hoisted node_modules, so a package can import another **in-process**. **Not an ADR** (it realises 0001, it decides nothing new). Unblocks both the dashboard's in-process read-API test and the (b) retiral. |
| (b) architecture debt RETIRED | `e1fbbde` | read-API harness materialises ingest's **REAL** Postgres migrations in-workspace (own Kysely `Migrator` over ingest's migrations folder, skipping the ClickHouse-coupled `runMigrations`); the (c) DDL mirror is gone. All 31 api tests stayed green, **zero `read_ac` result changes** → the mirror had not drifted on read columns; drift now impossible by construction. |
| dashboard harness-first RED (PART 2/2) | `d3483c5` | Next.js 15 scaffold + UI harness; structure green (next build/tsc/biome/workspace-of-3), 5 `dash_ac` red by design + 1 green harness-sanity guard; `check-dashboard` job added; Known CI debt co-located (placeholder). |
| RED SHA fill | `1c3a68d` | Two-commit pattern: substitutes with `d3483c5`. |
| **dashboard GREEN — closes C** | **`d017630`** | data-access (`getIncidents`/`getIncidentDetail`) + presentational render (`IncidentsTable`/`IncidentDetailView`); **5 `dash_ac` GREEN**; `check-dashboard` debt removed. The auth→read→render chain works end to end against the real read-API in-process. |

## Phase 6 C delivery declaration

Phase 6 C is **delivered in full**.

**Delivered:**

- **auth-core (SPEC-008)** — `services/api` (Fastify 5 + Zod + Kysely + pg + ioredis). Local password (Argon2id) + TOTP (RFC 6238, otplib@12, replay-rejected) login → opaque Redis session (`cgsess:<token>`, `HttpOnly`/`Secure`/`SameSite=Strict` cookie) with **immediate revocation** (single `DEL`); three roles enforced server-side via `makeRequireSession`/`makeRequireRole`; dual-axis rate-limit; append-only `audit_log`; CSRF token on every mutation. 9 `auth_ac` green (CI-able on Linux `ts-ci` via testcontainers Postgres + Redis — no ETW, no agent, so no developer-local marquee).
- **read-API (SPEC-009)** — `GET /v1/incidents` (list + keyset pagination, cap 200), `GET /v1/incidents/:id` (detail with `alert_ids` resolved + MITRE — the teachable view), `GET /v1/alerts` (list). Every read behind `makeRequireSession`; role + `org_id` from the session, never the request; every query org-scoped; a cross-org `:id` → `404` (no existence oracle); errors sanitised. `read_ac_001–005` realised in `services/api/test/read-ac-00{1..5}-*.test.ts` (`read_ac_006` error-sanitisation exercised via the generic 404 path, `services/api/src/read/routes.ts:44-47`).
- **minimal SOC dashboard (SPEC-009)** — Next.js 15 App Router + Tailwind + TanStack Query (`dashboard/`): login → incidents list → incident detail. Server-side data-access forwards the `cgsess` cookie to the read-API; a 401 → `redirect("/login")`, a detail 404 → `notFound()`. 5 `dash_ac` green (3 data-access in-process + 2 render) + the harness-sanity green-guard.
- **pnpm-workspace** — `services/*` + `dashboard` under one root lockfile; `ts-ci` has three jobs.
- Known CI debt: zero — every RED-by-design row co-located at its red SHA and removed at its green SHA (Convention #13).

**Structure realised:**

- `services/api/src/`: `app.ts` (`buildApp`), `services.ts` (`buildServices`), `config.ts`, `db/migrate.ts` + `db/migrations/0001_users.ts`/`0002_audit_log.ts` (api-owned runner, Option A), `auth/` (`password`/`totp`/`session`/`ratelimit`/`audit`/`prehandlers`/`routes`/`service`), `read/` (`queries`/`routes`/`types`), `cli/create-user.ts`, `errors.ts`.
- `dashboard/`: `app/` (layout, providers, login, incidents list + `[id]` detail), `src/components/` (`incidents-table`, `incident-detail`, `ui/button`), `src/lib/api/` (`client`, `incidents`, `result`, `types`), `test/` (`dash-ac-001-auth-read`, `dash-ac-002-render`); `next.config.mjs`, `tsconfig.json`, `tailwind.config.ts`, `biome.json`, `vitest.config.ts`.

## Architecture decisions C generated (the trail that must survive)

Each with its *why* and *status*. These are not re-litigable without a new gate.

1. **pnpm-workspace — realises ADR-0001 at the tooling level (`15d05b4`).** One root lockfile, hoisted/symlinked node_modules across `services/*` + `dashboard`, so a package imports another in-process. **NOT an ADR** — it realises the existing monorepo decision (ADR-0001), it does not decide a new one. It is what made the dashboard's in-process read-API test and the faithful (b) retiral possible. Status: **in force.** Consequences handled: one root lockfile (per-package lockfiles removed); `pnpm.onlyBuiltDependencies` must live at the root `package.json`; `pnpm install --frozen-lockfile` per-package still installs the whole workspace; `ts-ci` path filter widened to the root workspace files.
2. **Opaque server-side session in Redis, not JWT (ADR-0014 §2, `0014-human-authentication-model.md:29-38`).** Chosen for **immediate revocation** — logout / forced reset / role change / suspected compromise kills a session *now* via one server-side `DEL`; "a security tool must be able to kill an operator session now." JWT was rejected (A2, `:58-60`): revocation needs a denylist that reintroduces server-state and would have forced an **amendment to ADR-0003** (sessions → Redis). The deciding factor is revocation immediacy, not merely ADR-0003's placement signal. Status: **Accepted, realised in SPEC-008** (the immediate-revocation invariant is a product invariant, `SPEC-008-auth-core.md:15`).
3. **Migration ownership Option A — `services/api` owns its auth migrations (`SPEC-008-auth-core.md:167`, Manuel's Session 19 gate).** `services/api` runs its own migration runner with its **own** Kysely ledger table + **own** advisory-lock key against the same Postgres instance (ADR-0003, one instance, two runners). Human-secret DDL (`password_hash`, `totp_secret` encrypted with pgcrypto) lives in the user-facing service's tree, never the agent-facing `services/ingest` tree. Extends ADR-0014 §3's component separation from runtime to deploy-time; contradicts no Accepted ADR. Status: **Accepted, realised.** The api runner is self-contained (its `0001` ensures `pgcrypto` itself, so it does not depend on ingest having run).

## Debts / deferred — the inventory the next phase inherits (each with a named destination)

- **Architecture debt (c)→(b) — RETIRED (`e1fbbde`).** The read-API test harness no longer mirrors the read-target tables with a test-only DDL copy; it applies ingest's **real** Postgres migrations in-workspace. Drift was confirmed **non-existent** (31 read-AC green with zero result changes) and is now **impossible by construction**. Closed — see `engineering-notes.md` §"(b) schema-sharing debt RETIRED" + SPEC-009 §Amendment 2026-06-03.
- **Object-level authz scoped to org (deferred-with-destination).** Every read enforces `makeRequireSession` + `org_id = <session>` (the enforcement *point* exists and is CI-tested). The MVP is single-org (ADR-0003 one `org_id`), so the object grain collapses to org. Finer grain (per-agent / per-analyst ACL) deferred to a **multi-tenancy / scoping ADR** (SPEC-009 §Security considerations). Not a silently-accepted threat-model gap — the mandate met at the MVP grain, finer grain named as debt.
- **RBAC of reads = session + org, not role-differentiated (SPEC-009 §Operational §2, `:86`).** All three roles read in the MVP; the role × read-capability matrix is deferred. The enforcement seam is fixed so a restricted capability is a one-line `makeRequireRole` at the same point — it returns when triage-writes and the drill land.
- **NFR-008-002 session absolute cap (minor increment).** Sliding idle TTL is implemented (recommended idle 8 h); the **absolute lifetime cap** (recommended 24 h, per-org configurable) is named in SPEC-008 §NFR (`SPEC-008-auth-core.md:183`) and is a minor pending increment of auth-core. Destination: an auth-core hardening increment.
- **CSRF — CLOSED, not debt (noted so it is not re-sought).** ADR-0014 deferred CSRF to "the C auth SPEC"; SPEC-008 **is** that destination and realised it (session-bound token required on every mutation, `auth_ac_007`, `SPEC-008-auth-core.md:144-146`). Done.
- **alert→source-event drill + agents inventory view (SPEC-009 §Out of scope, `:34`/`:36`).** Data-unblocked (value-match `event_id IN source_events` is version-agnostic) but cut: the drill would add the **first ClickHouse client** to `services/api` (today pg+redis only); the agents view reads the ingest-domain `agents` table (cross-boundary) and drags the SPEC-004 token-endpoint amendment. Destinations: a read-depth increment; an agents-management slice.
- **Carried forward, still open:** `event_time` historical backfill grain (S18); `user` grouping dimension absent in v0.1 / window-boundary artifact / per-tactic-set grouping (S18, SPEC-007); the agent `event_id` v4/v7 reconciliation (ADR-0009/0011 domain, surfaced for the drill); the Phase 0 spike executable preservation (S16); the dep-graph SPEC-node convention (S13); the ajv-cli pinning note (S12). None actioned in Session 19.

## UI testing precedent (the project's first — must be inherited)

Recorded in `engineering-notes.md` §"Phase 6 = C — dashboard scaffold". The binding patterns for any future UI:

- **Data-access tested in-process against the real read-API** (`services/api`'s `buildApp` + `app.inject`), backed by Postgres + Redis testcontainers — **no mocks**, no api Docker image (the (A) toll avoided; the (B) workspace + (b) cross-import payoff). `services/api` is loaded by **dynamic import with a non-literal specifier** so its NodeNext source is never typechecked under the dashboard's `isolatedModules` config (a literal specifier fails `TS2748` on an ambient const enum from `@node-rs/argon2`).
- **Render tested light** — React Testing Library + jsdom over **pure presentational components**. **No E2E / Playwright** for the MVP (deferred; heaviest).
- **The "harness sanity" green-guard as a technique** — a green test inside a RED suite that proves the *infrastructure* is alive (the real read-API returns 200 + data in-process). It makes the RED failures unambiguously logic-absent, never setup-broken; the same guard stays green at the GREEN gate as a permanent proof the in-process harness works.
- **Two render-fidelity choices — the thin-slice boundary, not debt.** The MITRE technique is rendered **once** (at the incident level, the canonical grouping mapping) so the assertion matches uniquely; a **plain `<a>`** (not `next/link`) keeps the presentational components renderable standalone under jsdom without the App Router context. When the UI grows (client-side prefetch/routing, per-alert MITRE), these are revisited **in the RSC layer** — they are the limit of the thin slice, not a defect.

## Procedural notes — Session 19 (engineering-notes)

Recorded in `engineering-notes.md` §Session 19 (candidates for convention promotion at a future gate):

- **A green AC inside a RED suite deserves an audit of WHAT it asserts, not just that it passes.** A spurious-green (a test that passes because its setup silently no-ops) is the opposite of a legitimate green-guard (one that passes *because* it proves live infrastructure). The harness-sanity guard was written to be the latter — it asserts a real 200 + data from the in-process API.
- **Adding a NOT NULL column / changing a schema requires auditing ALL writers** (production + test fixtures), reconfirmed: the (b) retiral switched the read-API harness to ingest's real schema, which forced the seed helpers to supply every real NOT NULL/CHECK the test-only mirror had omitted. (First recorded S18, `68c95f8`.)
- **Stop-and-ask at architecture forks worked — three catches that prevented disguised RED-by-setup.** The materialisation fork (a/a'/(c)) and the dashboard-test fork (Docker-image (A) vs workspace (B)) were each PARA'd to Manuel rather than forced; the workspace decision (B) then enabled both the faithful (b) retiral and in-process UI testing. Stopping at the fork, not improvising past it, is what kept the RED gates honest.

## Deployment follow-up (declared, outside C's DoD)

Wire `dashboard/` into `infra/dev/docker-compose.dev.yml` + `.env.example` (`API_BASE_URL`/port), mirroring how `services/api` was wired (`843405f`). A standalone infra-dev chore in its **own SHA** — the dashboard talks HTTP to the api, so compose wiring is dev convenience, not part of C's definition-of-done.

## Hook to what's next — MVP gap inventory (NOT a phase decision)

C completed the **user-facing slice** of the MVP (`services/api` + `dashboard` are now real, not README-only). What remains of the MVP, mapped against the Blueprint, each with its repo hook — **for Manuel to choose Session 20's scope with the map in front of him; this document does not decide it:**

- **Detection breadth — 1 rule vs the 10-rule bar.** Only `rules/windows/office_spawns_script_host.yml` exists; the MVP bar is ~10 rules (Blueprint §"Honest scope"; SPEC-006 §Out of scope `:42`, §Open questions `:248`/`:256` — one rule ratified for the MVP to prove the *chain*, breadth deferred). Hook: the rule engine + `rules/windows/` are ready; adding rules is incremental over the existing evaluator.
- **SOAR / notification (`email.send`) — never opened.** `services/soar/` is README-only; Blueprint §10 (SOAR engine) + MVP actions `email.send`/`incident.assign`/`alert.acknowledge` (`blueprint.md:471`). Phase 6's sequence was A→C; **B (SOAR) was never opened.** Hook: the `incidents`/`alerts` model + `status`/`assigned_to` are ready as triggers; Redis is the decided SOAR-locks home (ADR-0003).
- **Forensic / PDF report — never opened.** `services/forensic/` is README-only; Blueprint `cg-forensic` (`:112`), `report` entity → PDF/HTML (`:393`), and the product promise "exportable forensic report on the first incident" (`:33`). Hook: MinIO is the decided artifact home (ADR-0003); the `incidents` + `cges_events` model is the report's input.
- **Packaging / production deploy posture — deferred by ADR-0010 §part 2.** The agent runs as a **manually-launched elevated user process** today; Windows Service (LocalSystem) + MSI installer + lifecycle/auto-restart are deferred to a **future packaging SPEC** (`0010-agent-privilege-model-mvp.md:16`/`:32`). Hook: the privilege baseline is validated; only the packaging machinery is missing.
- **ML / UEBA — parallel track, not started.** `services/ml/` is README-only; ADR-0005 (rules + ML in parallel) is the standing decision; the `alerts` schema already carries `ueba_score`/`ml_score` columns + `cg_score` anyOf. Hook: the scoring seam exists; an ML producer would populate the reserved columns.
- **Go correlate / event firehose extraction.** `services/pipeline/` is README-only; the correlate step lives transitorily in the TS `services/ingest/src/detect/` seam (Phase 5/6). Hook: a future event-firehose ADR (named in prior handoffs) extracts it to Go.
- **Read-depth + real-time increments** (already itemised under Debts): the alert→event drill, the agents view, triage writes, WebSocket push.

Prior deferrals are itemised in SPEC-006 §Out of scope and the Session 13–18 handoffs.

## Invariants carried into Session 20

Binding for every session. Authoritative text in [CLAUDE.md](../CLAUDE.md) + the per-session procedural notes in [docs/engineering-notes.md](engineering-notes.md). All invariants from prior handoffs carry forward unchanged.

- The **thirteen numbered conventions** (1–9 Session 10, #5 extended Session 11, #10–#13 Session 18) remain in force. Session 19 produced three new **candidates** (above), not yet promoted.
- The **CLAUDE.md §"Local pre-commit gate"** now spans three TypeScript workspaces — `services/ingest`, `services/api`, **and `dashboard`** (per-package `pnpm typecheck` + `lint` + `test`; the dashboard adds `next build`). The standing developer-local marquee gate (SPEC-005 + SPEC-006, elevated + Docker) still applies to any change under `services/ingest/src/detect/` — **not exercised in Session 19** (C touched only `services/api` + `dashboard`, never the detection path).
- **CI monitoring after every push** (CLAUDE.md) — mandatory; the post-push inspection caught real issues across the session's RED/GREEN gates.
- **Known CI debt co-locality** (Convention #13) — the two-commit SHA-placeholder pattern was used for all three RED gates this session (auth, read-API, dashboard) and for this handoff's anchor.

## How Session 20 resumes

1. Read this document and prior handoffs ([docs/handoff-session-18.md](handoff-session-18.md) back through [docs/handoff-session-9.md](handoff-session-9.md)) plus [CLAUDE.md](../CLAUDE.md), [ADR-0014](adr/0014-human-authentication-model.md), [SPEC-008](specs/SPEC-008-auth-core.md), and [SPEC-009](specs/SPEC-009-read-slice.md). They are the binding contract.
2. Confirm `main` is at the Session 19 anchor and all workflows are green. `git status` should show the working tree clean (modulo `.claude/`). Known CI debt: zero rows. ADR catalog: 14 Accepted; SPEC catalog: 9 Accepted.
3. **Phase 6's A→C sequence is complete** (incidents → user-facing slice). The next scope is **Manuel's to choose** from the MVP gap inventory above (detection breadth; SOAR/email — the never-opened B; forensic/PDF; packaging per ADR-0010 §part 2; ML/UEBA; the Go correlate extraction; or a read-depth/triage increment). Architect-Claude at Session 20 opening determines scope by reading the contracts + consulting Manuel — this handoff inventories the options, it does not pick one.
4. The deployment follow-up (dashboard in dev compose) is a small named chore that can land in any SHA when convenient.
5. Architect-Claude's auto-memory carries narrative; this document plus prior handoffs plus the ADR/SPEC catalogs and engineering-notes carry the facts; the repo at `main` is the ultimate source of truth. Where any two disagree, re-verify against `main`.
