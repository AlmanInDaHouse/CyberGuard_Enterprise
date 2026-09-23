# Handoff — End of Session 29

Full state-of-the-world at the S29 close. Written so a cold or compacted session
recovers the thread in one read.

Session 29 landed roadmap **Phase A''** (the api container packaging) — the same
review-branch workflow, this time for **code**: three commits cherry-picked to
`main`, one per push. No new ADR; SPEC-008 was amended in place
(`API_BIND_HOST`). Catalogs stay ADR 17 / SPEC 14. Known CI debt: ZERO.

## Anchor commits (all on main, pushed)

| SHA | What |
|---|---|
| `2cd0231` | `feat(api)`: configurable listener bind host (`API_BIND_HOST`, D3; SPEC-008 Amendment 2026-09-23). |
| `569f92d` | `feat(api)`: package the api service as a container image (D1 + D2; clears #15). |
| `faac8b3` | `ci(ts)`: build ingest and api and assert their entrypoints (clears #16). |
| (this commit) | handoff-29 + roadmap / README pointers + the CLAUDE.md relay rule 5. |

The three commits were pushed one per push (rule 3a), CI green on each:
`2cd0231` (ts-ci + markdown-lint), `569f92d` (ts-ci + markdown-lint) and
`faac8b3` (ts-ci). On `faac8b3` the new **Build (tsc)** and **Entrypoints exist
(start + bin)** steps ran for the first time in CI, green in both the ingest and
api jobs.

## Phase A'' — delivered

`services/api` carried the same three defects A' fixed in `services/ingest`,
latent because the api image had never been run:

- **D1 — build context / lockfile.** The api Dockerfile built from a
  `services/api`-scoped context and copied a `pnpm-lock.yaml` that `15d05b4`
  moved to the repo root, so the image **had not built since `15d05b4`**.
- **D2 — dist layout (cause `fc33274`, the api `tsconfig`'s first commit).**
  `tsc` (`rootDir: "."`, `include` `test/**`) emitted `dist/src/index.js`, so
  `CMD ["node","dist/index.js"]`, the `start` script and the `bin`
  (`dist/cli/create-user.js`) never resolved — **never runnable**.
- **D3 — loopback bind (since `fc33274`).** `server.ts` bound a hard-coded
  `127.0.0.1`; inside a container that reaches neither the published port nor
  the compose network.

Fixes:

- `services/api/Dockerfile` rebuilt from the **repo root**, a mirror of
  ingest's (`pnpm deploy --legacy --prod` → a flat runtime `node_modules`).
- `tsconfig.build.json` (`rootDir: "src"`, src-only) + the `build` script →
  `tsc -p tsconfig.build.json` yields a flat `dist`.
- `API_BIND_HOST` (default `127.0.0.1`; the compose sets `0.0.0.0`) — an
  **owner-STOP ratified by Manuel**; SPEC-008 carries an **Amendment
  2026-09-23** (additive, backward-compatible; `API_PORT` unchanged).
- `docker-compose.dev.yml` api block: build context `../..`, `API_BIND_HOST
  0.0.0.0`, the header and healthcheck comments reconciled (no migration
  number — there are three now).
- `README.md:54` restored to its `e01c1f5` form (the debt-#15 note cleared).

Gate (developer-local, real backends): `task dev:up` first-hand with everything
healthy; the api reachable **from the host** (a Fastify `HTTP 404` on `GET /`);
the native `@node-rs/argon2` (musl) loads inside the image; the api suite at 62
tests = the baseline.

**Two diagnosis findings worth keeping.**

1. The api healthcheck is an **in-container TCP probe** to `127.0.0.1`, so it
   would have reported healthy with the listener bound to loopback while the
   container could not be reached from outside. Reachability must be tested
   **from the host**, not from the healthcheck.
2. A bare CI **build** would not have caught D2 — `tsc` compiled cleanly while
   emitting an unrunnable layout. The guard therefore asserts the
   **entrypoints** `package.json` points at (the `start` target + every `bin`)
   actually exist.

## Process changes

The review-branch workflow (rule 3) was used for **code** this session — three
commits, each landed on `main` by **cherry-pick** with a per-commit tree guard
(`git diff --quiet <review-sha> HEAD` = 0). This is codified as **rule 5** under
CLAUDE.md's *Relay transport and verification*. `569f92d`'s message was
corrected on landing ("never built" → "has not built since `15d05b4`"); the tree
did not change.

## Environment notes

Local-operation facts that recur, not repo state:

- Another local project holds host port **8080** on the owner's machine, so
  `task dev:up` collides on ingest's enroll port. Remap by shell
  (`CG_INGEST_ENROLL_PORT=18080`) without touching `.env` or the compose.
- An `infra/dev/.env` older than `.env.example` lacks the newer keys and falls
  back to the compose defaults (e.g. the api on host `8081`). Compare it against
  `.env.example` when a port collides.
- Docker Desktop may need starting before `task dev:up`.

## Owner-STOP decisions pending (waiting on Manuel)

The handoff-27 list stands unchanged (ADR-0002 Go→TS reconciliation; the
criterion-7 deployment contract; forensic trust anchoring; B2 capture source;
the compose basename collision #12). From handoff-28, with `API_BIND_HOST` now
resolved:

- **Amend SPEC-004 with `INGEST_DETECT_*` (#17).** A SPEC amendment → owner-STOP.
- **Publish `archive/local/feat/detection-prod-driver` (optional).** Pushing the
  local-only tag makes `b065aa5` and `6f9d5e3` (cited by handoff-27) resolve on
  origin. Skip if the local-only record is enough.
- **Process note (optional).** CLAUDE.md's *Decision authority* section
  (2026-05-20) speaks of "the agent" and predates the S27 advisor/executor
  split; worth making the split explicit there.

## Debts

- **#1–#11:** unchanged — see [handoff-session-26.md](handoff-session-26.md)
  §Debts (Class H, immutable, so the pointer cannot go stale).
- **#12–#14:** unchanged — see [handoff-session-27.md](handoff-session-27.md)
  §Debts.
- **#17–#19:** unchanged — see [handoff-session-28.md](handoff-session-28.md)
  §Debts.
- **#15 — CLEARED (`569f92d`).** The api image now builds and runs: repo-root
  build context, and a flat `dist` from `tsconfig.build.json`.
- **#16 — CLEARED (`faac8b3`).** ts-ci builds ingest and api and asserts their
  entrypoints (`start` + `bin`).
- **#20 — no SPEC consolidates the api's configuration surface.** The `API_*`
  variables are spread across ADR-0015, SPEC-008 §7 (and its Amendment
  2026-09-23) and SPEC-010; and the api exposes **no `/health`** endpoint (the
  dev TCP probe suffices). Both feed the operator configuration surface of
  roadmap Phase F.

Known CI debt: ZERO rows.

## How Session 30 resumes

1. Read this handoff + prior handoffs (28 back to 9) + CLAUDE.md.
2. Confirm the main tip, tree clean, Known CI debt zero, catalogs ADR 17 /
   SPEC 14.
3. **B1** (evaluator generalization) —
   `services/ingest/src/detect/engine.ts:32-51`: relax the strict-reject
   validator to admit more Sigma fields and the `contains` operator, and widen
   the read-model projection. No new capture, no agent decision; cheap, and the
   prerequisite of C and D.
4. Standing gate: B1 touches `detect/`, so the gate is the elevated
   `detect_ac_001` marquee, which Manuel runs. Packaging changes carry their own
   first-hand container gate.
