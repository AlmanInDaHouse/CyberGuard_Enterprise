# Handoff — End of Session 28

Full state-of-the-world at the S28 close. Written so a cold or compacted session
recovers the thread in one read.

Session 28 landed roadmap **Phase A'** (the ingest container packaging), closed
the Class B residue the S27 pass had missed, and archived the two merged feature
branches. No new ADR; SPEC-004 was amended in place. Catalogs stay ADR 17 /
SPEC 14. Known CI debt: ZERO.

## Anchor commits (all on main, pushed)

| SHA | What |
|---|---|
| `48e3bd6` | `feat(ingest)`: configurable listener bind host (`INGEST_BIND_HOST`, D3). |
| `a67315c` | `feat(ingest)`: container packaging — squash of `03005f0` (D1 + D2). |
| `e01c1f5` | `docs`: Class B residue — three altitude clauses missed by the S27 pass. |
| (this commit) | handoff-28 + roadmap / README pointers + the CLAUDE.md relay rules and the `:239` count fix. |

The three commits were pushed one per push (rule 3a): `48e3bd6` (ts-ci +
markdown-lint), `a67315c` (ts-ci) and `e01c1f5` (ts-ci), all green. `a67315c`
and `e01c1f5` touch no `.md`, so markdown-lint did not trigger for them.

## Phase A' — delivered

Landing `feat/ingest-container-packaging` (@`03005f0`) was framed as the
one-line `Dockerfile:31` fix. The WIP already carried the build-context fix
(D1); running the image for the first time surfaced two more reactive
corrections (D2, D3), neither anticipated — latent defects the image had never
exposed because it had never been run:

- **D1 — build context / lockfile (cause `15d05b4`), already fixed in the WIP.**
  The pre-A' Dockerfile built from a `services/ingest`-scoped context and copied
  a package-local `pnpm-lock.yaml`, which `15d05b4` removed when the workspace
  moved to a single root lockfile. The WIP builds from the repo root, so the
  workspace lockfile, the workspace manifest and the detection `rules/` are
  reachable.
- **D2 — dist layout (cause `e18f2e1`).** `tsc` emitted `dist/src/index.js`
  (`tsconfig` `rootDir: "."`, `include` `test/**`), so the runtime `CMD
  ["node","dist/index.js"]` never resolved: the image was **never EXECUTABLE**
  (the CMD points at a file `tsc` does not emit); it could only have been
  BUILDABLE before `15d05b4`. Fixed **at the cause**: a new
  `tsconfig.build.json` (`rootDir: "src"`, src-only) + the `build` script →
  `tsc -p tsconfig.build.json` yields a flat `dist`, and the Dockerfile copies
  `/repo/services/ingest/dist`. `start`, the `bin` entry, and the **monorepo
  fallback** in `config.ts:37` all assumed a flat `dist`.
- **D3 — loopback bind (cause `f407c05`).** Both listeners bound a hard-coded
  `127.0.0.1`, correct for the host-process test topology but unreachable inside
  a container. Fix: `INGEST_BIND_HOST` (default `127.0.0.1`; a container sets
  `0.0.0.0`). This is a **deployment-contract decision (owner-STOP)** ratified by
  Manuel; SPEC-004 carries an **Amendment 2026-09-21** (additive,
  backward-compatible — an agent dialing any host other than
  `localhost` / `127.0.0.1` needs an operator-provided cert at
  `INGEST_SERVER_CERT_PATH` / `INGEST_SERVER_KEY_PATH`).

Gate (developer-local, real backends): G0 flat build; G1 image build; G2 rules
at `/app/rules/windows` + flat `dist` in the image; G3 fail-loud negative
(missing / empty rules dir refuses to start, message asserted, `driver.ts:145` /
`:150`); G4 `up --wait` healthy + reachable from the host + driver started
(`interval_ms 10000`, zero `detect_driver_pass_error`); G5 typecheck / lint /
test 57.

**Refuted hypothesis.** pnpm's docs say `deploy` honours the package
`.gitignore` (which lists `dist/`), so `/deploy/dist` was expected absent. In
practice `pnpm deploy --legacy` **did** copy the ignored `dist/`. The image
relies on neither behaviour — it copies `dist` straight from the `tsc` build
output.

**Errata (the commit is not touched).** `a67315c`'s body says "`config.ts`'s
packaged-rules resolution" assumed a flat `dist`. It is actually the **monorepo
fallback** (`config.ts:37`). The packaged path (`config.ts:35`) is
`cwd`-relative (`/app/rules/windows`) and does not depend on the `dist` layout.

## Class B residue — `e01c1f5`

0.8 (S28) confirmed three live clauses that still said no production detection
driver exists, in the SMTP/notify comments of `config.ts`,
`docker-compose.dev.yml`, and `.env.example`. They escaped the S27 Class B pass
because that grep keyed on the "no production caller" / "test-validated"
vocabulary, whereas these use the "prod detection driver" wording; and the
`.env.example` clause was split across a line break, so a line-oriented search
missed it. Each was flipped from "no driver" to "detection still runs, only the
email is skipped", preserving statement form (rule 3b).

`config.ts`'s embedded citation "ADR-0017 §Consequences" was preserved verbatim
(rule b). Before the edit it vouched for the **opposite** of its own sentence
(§Consequences records the altitude as resolved by the prod-driver); after it,
it vouches for the new sentence. compose and `.env.example` carry no embedded
citations.

## Process changes

Four new rules landed in CLAUDE.md under a new **### Relay transport and
verification** subsection (relay corrupts long lines → `--word-diff`, one file
per message, `[n/m]` short-line partitions for the rest; literal advisor text
is verified by SHA-256 of the resulting lines, not re-sent; long docs are
reviewed on a pushed branch with a `NOT YET RATIFIED` draft commit, squashed to
main without the marker after ratification, executor runs markdownlint locally
because CI does not run on branches; conditioned ratifications get the literal
verification output).

Owner-STOP clarification: a **new environment variable is an owner-STOP even
with a safe default** (CLAUDE.md deployment-contract rule). The session briefing
said the opposite; CLAUDE.md won, so `INGEST_BIND_HOST` was surfaced to Manuel
as an owner-STOP and ratified.

## Cleanup

`land/ingest-container-packaging` (merged) and its `C:/tmp` worktree removed
(`git worktree remove` de-registered it but could not delete the directory,
ignored `node_modules` on Windows; the directory was removed manually and
pruned). The two feature branches were archived to tags before deletion (the
commits are preserved because handoffs cite them):

```text
Remote (origin): refs/heads/main = e01c1f5
  archive/feat/detection-prod-driver     -> e761610 (squash-merged as 63019bc)
  archive/feat/ingest-container-packaging -> 03005f0 (squash-merged as a67315c)
Local branches: main only
Worktrees: the main worktree only
Tags archive/*:
  archive/feat/detection-prod-driver
  archive/feat/ingest-container-packaging
  archive/local/feat/detection-prod-driver  (@6f9d5e3, LOCAL-only, not pushed)
```

`feat/detection-prod-driver`'s local tip (`6f9d5e3`) was ahead of origin
(`e761610`) by 10 commits: main's commits brought in by the `b065aa5` merge
(already on origin) plus `b065aa5`, `9c96bc3` and `6f9d5e3`, which exist nowhere
else. Those three are kept only in the local-only tag
`archive/local/feat/detection-prod-driver`; publishing it is an optional
owner-STOP (below) so the SHAs handoff-27 cites resolve on origin.

## Owner-STOP decisions pending (waiting on Manuel)

The handoff-27 list stands unchanged (ADR-0002 Go→TS reconciliation; the
criterion-7 deployment contract; forensic trust anchoring; B2 capture source;
the compose basename collision #12). New this session:

- **`API_BIND_HOST` (roadmap A'').** The same loopback-bind issue will hit the
  api container. No SPEC documents the api's configuration today — `API_PORT`
  appears in none — so the api config surface is undocumented, not just the bind.
- **Amend SPEC-004 with `INGEST_DETECT_*` (#17).** Adding the driver tunables to
  SPEC-004 §Configuration is a SPEC amendment → owner-STOP.
- **Publish `archive/local/feat/detection-prod-driver` (optional).** Pushing the
  local-only tag makes `b065aa5` and `6f9d5e3` (cited by handoff-27) resolve on
  origin. Skip if the local-only record is enough.

## Debts

- **#1–#11:** unchanged — see [handoff-session-26.md](handoff-session-26.md)
  §Debts (Class H, immutable, so the pointer cannot go stale).
- **#12–#14:** unchanged — see [handoff-session-27.md](handoff-session-27.md)
  §Debts. Note on #13 (anchor drift): its cited anchor `roadmap.md:36` is now
  `roadmap.md:38`, which confirms the drift is real and that the sweep must go
  **by pattern**, not by the stored line number.
- **#15 — api image: not buildable since `15d05b4`, never runnable (nested
  `dist` since `fc33274`, the api `tsconfig`'s first commit).** The same three
  defects as A', in `services/api`: D1 (`Dockerfile:8` / `:18` need a
  nonexistent package-scoped lockfile), D2 (`tsconfig` `rootDir: "."` +
  `test/**` → nested `dist`), D3 (`server.ts:7` loopback bind). Effect:
  `task dev:up` **fails on main** building the api image (evidence:
  `docker compose build api` → `Dockerfile:18`, `"/pnpm-lock.yaml": not found`).
  Discharged by A''.
- **#16 — ts-ci never builds ingest or api.** The ingest and api jobs run
  typecheck / lint / test but **no `pnpm run build`**; only the dashboard job
  runs a build (`ts-ci.yml:144`, `next build`). That is why D2 went unnoticed
  from `e18f2e1` — CI never emitted a `dist`. Discharged by A''.
- **#17 — `INGEST_DETECT_*` absent from SPEC-004 §Configuration.** They live in
  `config.ts`, compose, `.env.example`, and `ADR-0012:284` but not in the SPEC.
  Amending the SPEC is owner-STOP (above).
- **#18 — dev compose publishes ports on all host interfaces.** The
  `docker-compose.dev.yml` port mappings bind `0.0.0.0`; the infra SPEC (phase F)
  should pin them to `127.0.0.1:PORT`.
- **#19 — fail-loud does not close the listeners.** `server.ts:99-101`'s catch
  closes `services` but not the listeners bound at `:63-64`. Harmless in a
  container (`index.ts:27-29` does `process.exit(1)`); latent for any in-process
  caller of `startIngest`.

Known CI debt: ZERO rows.

## How Session 29 resumes

1. Read this handoff + prior handoffs (27 back to 9) + CLAUDE.md.
2. Confirm the main tip, tree clean, Known CI debt zero, catalogs ADR 17 /
   SPEC 14.
3. **A'' first** — restore `task dev:up` by applying the A' packaging pattern to
   `services/api` and adding a build step to the ts-ci ingest / api jobs (#15,
   #16); it carries the `API_BIND_HOST` owner-STOP. Then **B1** (evaluator
   generalization) — unblocked, cheap, the prerequisite of C and D.
4. Standing gate: the elevated `detect_ac_001` marquee for any `detect/` change.
   Packaging changes carry their own first-hand container gate (A'' defines its
   own in the roadmap); G0–G5 were A'-specific (G3 exercises the ingest rules).
