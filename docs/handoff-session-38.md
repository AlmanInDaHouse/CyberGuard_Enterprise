# Handoff — Session 38

Session 38 opened on 2026-10-13 and closed on 2026-10-14. The session:

- Fixed debt #63 on its own branch and landed it.
- Ran a first sweep of the open debts that are Claude Code's to fix: #19,
  #36, #18 and #57.
- On 2026-10-14 Manuel delegated every decision pending on him and said he
  could not supply SPEC-021. Claude Code parked roadmap §B2 and §B3, put E
  next, and decided #10 and #33.
- Ran a second sweep for #33, #67 and #10, with the SPEC amendments first.

Its work order for Session 39 is *How Session 39 resumes* at the end.

## State at close

- `main` = `origin/main` = the commit that completes this file (CI ALL
  GREEN, *CI monitoring*); tree clean; no stash.
- Catalogs: ADR 20 / SPEC 20. Known CI debt: zero.
- One branch open, with no PR: `adr/s38-0021-soar` (`e619797`), the
  owner-review branch of ADR-0021 and the ADR-0002 and ADR-0015
  amendments, waiting for Manuel's "sí" (*What waits on Manuel*). No PR
  is open. Each PR below is closed and its branch deleted, local and on
  origin:
  - `fix/s38-debt-63-marquee-session` (PR #7) landed as five cherry-picked
    commits, `a9452b0..928cfbf`.
  - `fix/s38-debt-sweep-1` (PR #8) landed as four, `67f2055..6841aa5`.
  - `fix/s38-debt-sweep-2` (PR #9) landed as four, `bbdf1e3..8ea922e`, in
    an order its landing slip changed (*Progress*).
- Roadmap: §B2 and §B3 PARKED (`4cad976`), §E IN PROGRESS (this record's
  commit).
- Code changed this session. For #63: `services/ingest/test/helpers/marquee-agent.ts`
  and two new test files, `services/ingest/test/marquee-agent-etw.test.ts`
  and `agent/cg-agent/tests/etw_reclaim.rs`. The elevated gate covered it
  (*Gate outputs and baselines*). For the sweep: `services/ingest/src/server.ts`,
  a new `services/ingest/test/server-start-failure.test.ts`,
  `infra/dev/docker-compose.dev.yml`, `infra/dev/README.md` and both test
  backends (`services/*/test/helpers/backends.ts`). No file under
  `agent/cg-agent/src/` changed, and none of the sweep's files is on a gated
  path (*Developer-local SPEC-005 marquee validation* §5, *SPEC-006* §6).
  For the second sweep: `services/ingest/src/config.ts`, the dev compose
  and `.env.example` (#33); `dashboard/src/lib/api/client.ts`, its two
  pages and README (#67); and `services/api/src/` (`app.ts`, `errors.ts`,
  `services.ts`, `read/queries.ts`, `read/routes.ts`) (#10). Each comes
  with its tests, and none is on a gated path. `server.ts` and
  `config.ts` are still runtime inputs under `services/`, so gate 1b (at
  `6fbcd0e`) is not evidence for the tip; the next gated change runs the
  gate again.
- Outside the repository:
  - Docker Desktop, started by Claude Code, is still running.
  - The dev stack (`cyberguard-dev-postgres-1`, `-redis-1`, `-nats-1`,
    `-clickhouse-1`, `-minio-1`) still runs with the old mappings (`0.0.0.0`
    and `[::]`) and ClickHouse `24.8`. The new compose takes effect when the
    containers are recreated (`task dev:down`, then `task dev:up`; the
    volumes survive). Nothing in the session needed the stack, so Claude
    Code did not recreate it.
  - The gate script `C:\tmp\s38-gate1b.ps1` and its logs
    `C:\tmp\s38-gate1b-*.log`, and the logs of gate 1, which ran nothing in
    the repository (`C:\tmp\s38-gate1-*.log`).
  - No ETW session of the agent's name is running.
  - `C:` has 17 GB free (29 GB at the start). The Docker Desktop disk under
    `%LOCALAPPDATA%\Docker` held 101 GB on 2026-10-13 and grows with each
    testcontainers run; `target/` held 8.5 GB.
  - Docker's build cache (20.95 GB) and dangling images (0.73 GB) were
    pruned on 2026-10-14 (`docker builder prune -f`, `docker image prune
    -f`). The volumes and the tagged images, many of them other projects',
    were left alone. The space stays inside the WSL disk: it is reused
    there, and `C:` does not get it back until that disk is compacted,
    which needs WSL shut down (*What waits on Manuel*).

## Start

- `main` = `origin/main` = `3114bdf`, tree clean, no stash; no branch, no
  open PR; CI of the tip `markdown-lint → success`, the only workflow
  triggered.
- Catalogs ADR 20 / SPEC 20; Known CI debt zero; debts up to #63.
- The session #63 left was gone: `logman query -ets` did not list
  `CGAgent-KernelProcess`, and `logman query CGAgent-KernelProcess -ets`
  answered "No se encontró el Conjunto de recopiladores de datos.". The host
  had restarted at 2026-10-13 11:57:28 (`LastBootUpTime`), after gate 3b
  (03:23–03:26).
- Docker Desktop was not running (its engine pipe absent). Claude Code
  started it (`"C:\Program Files\Docker\Docker\Docker Desktop.exe"`;
  `docker version` → server `29.1.2`) for the local gate.
- Manuel's prompt for the session arrived with point 3's two alternatives
  unfilled and its point 4 lost in the paste.

## Progress

Anchor commits on `main`, one per push, CI green on each:

| SHA | What |
|---|---|
| `a9452b0` | Once the marquee's agent has exited, the harness stops the agent's ETW sessions by name (Windows) and rejects the run if one is still running; `stopEtwSessions` tested on a session with no provider. |
| `80b9708` | First review: a query answered with access denied counts as a running session, so the stop is tried and a survivor reported; `logman` injectable, the reported and error paths tested with a fake; the session list checked against the agent's source. |
| `bf08261` | `etw_reclaim.rs`: the startup reclaim of ADR-0008 §Decision part 2 asserted, unelevated and elevated. |
| `da0f38a` | Second review: an orphaned marquee agent does not stop the next one's session; a rejection names the agent's exit code, not its stderr; the session-list tripwire widened. |
| `928cfbf` | Third review: a rejection before the marquee's await is not unhandled; the after-exit test asserts the stop; `etw_reclaim` asks `logman` whether the session runs. |
| `bb0305e` | This record; `README.md` and the roadmap point here; CLAUDE.md's environment fact on the gate script and the gate's expected outcome. |

Manuel's decisions this session, in chat:

- 2026-10-13, asked whether he had SPEC-021's text, with the
  recommendation to fix #63 meanwhile: "no lo tengo". Claude Code read it
  as the recommended option and said so in the same turn.
- 2026-10-14, after the system killed the sweep's landing loop under low
  memory and Claude Code asked whether to resume it: "he liberado memoria,
  puedes seguir". Claude Code resumed the landing.
- 2026-10-14, after Claude Code listed what waited on him (SPEC-021, #10,
  #33, the Docker disk, #64's ADR half, #59, Part (b) §9, #55, Part (b)
  §1–§4): "lo que este pendiente de mi decidelo tu en base al beneficio
  del proyecto, en base a lo de a spec-021 no me acuerdo de que va ahi
  asique es imposible volver a hacerlo". Claude Code read it as a
  delegation of every point, decided for an enterprise buyer, and said in
  the same turn that an ADR still needs his explicit "sí" and that SPEC-021
  stays one Claude Code cannot draft.

The landing (*Integration path* §3): `main` had not moved since the branch
was cut, so no rebase. Each commit cherry-picked in order, the guard
`git diff --quiet <branch-sha> HEAD` before each push, one push per commit:

```text
guard fd5d2ce -> 0
SHA a9452b0 (from fd5d2ce): GREEN ts-ci -> success
guard 2708f9f -> 0
SHA 80b9708 (from 2708f9f): GREEN ts-ci -> success
guard a011a2e -> 0
SHA bf08261 (from a011a2e): GREEN ts-ci -> success | rust-ci -> success
guard ea120c3 -> 0
SHA da0f38a (from ea120c3): GREEN ts-ci -> success | rust-ci -> success
guard 6fbcd0e -> 0
SHA 928cfbf (from 6fbcd0e): GREEN rust-ci -> success | ts-ci -> success
```

### Debt sweep 1

While SPEC-021 waits, a subagent inventoried the open debts: 50 open, sorted
into Claude Code's (18), B2's (8), Manuel's (13) and those that need the
elevated gate (10), with #64 split. The first batch is four of Claude
Code's, small and of value; #10 and #33, also on that list, went to Manuel
(*Decisions*). Anchor commits on `main`, one per push, CI green on each:

| SHA on `main` | Branch SHA | What |
|---|---|---|
| `67f2055` | `d138684` | #19: a failed start closes the listeners it bound, then the services. Test: a failed heartbeat listen closes the enroll listener. |
| `d94ab7b` | `8e333d4` | #36: the ClickHouse image pinned to `24.8.14.39` in the dev compose and in both test backends. |
| `eddadd7` | `1039b1a` | #19, first review: the driver's fail-loud refusal closes both listeners; the first test matches the port whose listen failed. |
| `6841aa5` | `532e997` | #18: every port of the dev compose is published on `127.0.0.1`; the dev README dials `127.0.0.1` and says what an agent on another machine needs. |
| `ac80734` | — | #57: CLAUDE.md's *SPEC amendment workflow* says SPEC-001 to SPEC-009 carry `Last updated` (docs, straight to `main`). |
| `790970c` | — | This record of the sweep. |

On the branch, the first #18 commit (`2534398`, with a `CG_PUBLISH_HOST`
switch) was replaced after the first review by `532e997` (*Decisions*,
reactive), pushed with `--force-with-lease`.

The landing (*Integration path* §3): `main` had not moved since the branch
was cut. The guard ran before each push:

```text
guard d138684 -> 0
SHA 67f2055 (from d138684): GREEN ts-ci -> success
guard 8e333d4 -> 0
(the system killed the landing loop under low memory here, before the
push; resumed on 2026-10-14 after Manuel freed memory)
SHA d94ab7b: ts-ci -> success
guard 1039b1a -> 0
SHA eddadd7: ts-ci -> success
guard 532e997 -> 0
SHA 6841aa5: markdown-lint -> success
SHA ac80734: markdown-lint -> success
```

On `main`, only `markdown-lint` ran for `6841aa5`, for its README: no
workflow's path filter covers `infra/dev/` (debt #68). The compose was
checked by hand at the close (*Gate outputs and baselines*). PR #8 was
closed with a comment that maps each branch SHA to its `main` SHA.

### The delegation and debt sweep 2

On 2026-10-14 Manuel delegated every decision pending on him. Claude Code
decided each one (*Decisions*, delegated) and wrote the contract before
the code. The docs below went straight to `main`, each pushed on its own
with CI green:

| SHA | What |
|---|---|
| `ff20923` | SPEC-009, SPEC-010 and SPEC-013 Amendments 2026-10-14 (#10, read errors). SPEC-018 Amendment 2026-10-14 (#33, the driver's interval). SPEC-005 Amendment 2026-10-14 (#64's SPEC half, the reclaim log line at `warn`). The catalog; CLAUDE.md's list of SPEC amendments that left `Last updated` as it was gains SPEC-005. |
| `4cad976` | Roadmap: §B2 and §B3 parked with a reopen condition, §E next, Part (b) §1–§4 delegated. |
| `6cd1871` | SPEC-009's amendment made exact after the first code review: the inputs Postgres would refuse, what the pool's 5 s bound, and what the log may carry. |
| `56dfc4b` | SPEC-009's amendment: ClickHouse is read for every incident's drill, export and report. A test that tried to seed an alert with no source event failed on `alerts_source_events_nonempty`. |
| `3659e15` | SPEC-009/010/013 amendments made exact after the second code review; debts #69–#72 recorded here. |

The code, on `fix/s38-debt-sweep-2` (PR #9), was rebased twice onto
`main` as the SPEC moved; each rebase was pushed again one commit per push
with CI green. It landed as:

| SHA on `main` | Branch SHA | What |
|---|---|---|
| `bbdf1e3` | `42b08e6` | #67: the dashboard reads the api's default listener, `http://127.0.0.1:8081`, when `API_BASE_URL` is unset. |
| `4a6e981` | `ccda047` | #10: read errors, as SPEC-009 Amendment 2026-10-14 states. |
| `12a7f13` | `de2b826` | The dashboard README: `API_BASE_URL` and its default. |
| `8ea922e` | `843a80e` | #33: the driver's default interval is 5000 ms. |

The landing slip: the branch's order was #33, #67, #10, README. The first
landing command failed before its cherry-pick (`git rev-parse --short`
given two revisions is an error), so #33 was not picked. The next three
commands ran their guard but did not stop on it: `git diff --quiet …;
echo "guard -> $?" && git push` pushes whatever the guard says. Their
guards printed 1, since their landed trees lacked #33 and were not the
trees CI had run on the branch. CI then ran green on each of those `main`
SHAs. #33 landed last, with a guard that stops: `main`'s tree equals the
branch tip `de2b826`, the tree CI ran last.

```text
guard 843a80e -> 128   (rev-parse failed; nothing picked, nothing pushed)
guard 42b08e6 -> 1     SHA bbdf1e3: ts-ci -> success
guard ccda047 -> 1     SHA 4a6e981: ts-ci -> success
guard de2b826 -> 1     SHA 12a7f13: markdown-lint -> success | ts-ci -> success
guard: main tree == branch tip de2b826
SHA 8ea922e: ts-ci -> success
```

Each landed commit is true at its own SHA: none of the three depends on
debt #33, and SPEC-018's amendment says the interval's code follows. What the
slip cost is the guard's guarantee for those three trees, which only CI on
`main` covered. The auto-memory now holds the corrected form of the step
(`feedback-landing-guard-must-stop`).

### Roadmap §E: ADR-0021 proposed

ADR-0021 and two ADR amendments are on the owner-review branch
`adr/s38-0021-soar`, cut from `95a8c9f`. Each commit's subject carries
`NOT YET RATIFIED`. The branch has no PR, so no workflow ran on it, and
markdownlint passed locally on every file (0 errors).

| SHA | What |
|---|---|
| `cc158cf` | ADR-0021, SOAR for the MVP (Proposed), together with ADR-0002's Amendment 2026-10-14 (TypeScript is the server-side default; the harness runner and the event firehose keep Go until the firehose ADR). The threat model's Amendment 2026-10-14 (ADR-0021) for §2, §3 and §5. The READMEs of `services/`, `services/pipeline/`, `services/soar/` and `playbooks/`. The catalog. |
| `e619797` | ADR-0015's Amendment 2026-10-14 (#70): three bullets name the three routes that read `cges_events`. The catalog. |

What ADR-0021 decides:

- SOAR is a TypeScript module of `services/ingest` on the incident-create
  seam, after the commit.
- Its playbooks are YAML, validated at every boot and in CI against
  `schemas/playbooks/v0.1/`, with `ajv` as a direct dependency.
- Its actions are a closed list: `agent.isolate` in dry-run only, and
  `notify.email` live.
- Each run has one record per playbook and incident. The record is claimed
  before the first step, completed once, and refused any later change by a
  Postgres trigger. Its steps form an unsigned SHA-256 chain.
- Bounds: 10 steps, 15 s per step and 30 s per run.
- One playbook ships, for High severity or more.

## Decisions

Anticipated:

- #63 fixed now, on its own branch, not on B2's implementation branch (its
  recorded destination): B2 waits on SPEC-021, and every elevated gate left
  a session that fails the unelevated capture_ac_001 until a restart. The
  same reasoning moved #60 in S37. Alternative: wait for B2. Where:
  `a9452b0`.
- The harness stops the session; the agent is not ended gracefully. On
  Windows Node turns `kill("SIGINT")` into a forced termination and cannot
  send a console control event to one child without a native helper, and
  the agent would need a `Ctrl+Break` handler: an agent change for a test.
  Alternative: that graceful end. Where: `a9452b0`.
- `logman` for the query and the stop, read by exit status: 0 running,
  `PLA_E_DCS_NOT_FOUND` (0x80300002) none, measured from Node, unelevated, on
  a session with no provider. Alternative: a raw `ControlTraceW` from Node,
  which needs a native module.
- A session the harness cannot stop rejects the run instead of logging a
  warning, so a gate cannot pass with a session left.
- The contract of the change is debt #63's entry
  (`docs/handoff-session-37.md:517-532`); no SPEC states how a marquee ends
  its agent.

Reactive:

- A query answered with access denied (0x80070005) counts as a running
  session: unelevated, `logman query` of a session another account started
  answers so (measured on `EventLog-System`, `Eventlog-Security`,
  `DiagLog`), and the helper threw before trying the stop. First review.
  Where: `80b9708`.
- `etw_reclaim.rs`: the startup reclaim was exercised only by the sessions
  the marquees left and asserted by no test, and the fix removes that
  exercise. A test file outside the acceptance-criteria naming, as
  `logon_read_end.rs` is; its contract is ADR-0008 §Decision part 2. First
  review. Alternative: a debt for B2. Where: `bf08261`.
- The orphan guard: a marquee that throws before it awaits its run leaves
  its agent running until the timer, and at that agent's exit the harness
  would stop the next marquee's live session. The live agents are kept in a
  set on `globalThis`, since vitest evaluates the helper once per file in
  its single process, and after an exit the stop runs only when no other
  agent runs. Second review. Alternative: a `try`/`finally` in each of the
  five marquees. Where: `da0f38a`.
- A rejection names the agent's exit code, not its stderr: the marquees
  print their output redacted (SPEC-020 §Operational §11), and a stderr line
  may carry a path with the Windows user name. Second review. Where:
  `da0f38a`.
- `run()` marks its promise handled: a rejection before the marquee's await
  counted as unhandled, which ends the whole vitest run. Third review.
  Where: `928cfbf`.
- The elevated gate as a one-line script: gate 1's pasted block ran in
  reverse order, as gate 2 of S37 did. From `C:\WINDOWS\system32`,
  `cd ..\..` reached `C:\` and every step failed outside the repository.
  CLAUDE.md's *Environment facts* now say so (`bb0305e`).

Debt sweep 1, anticipated:

- A sweep of Claude Code's debts while SPEC-021 waits, chosen from the
  inventory by value and size. Alternative: wait for SPEC-021.
- #18 done in the dev compose, not through the phase F infra SPEC that its
  entry names. Dev compose defaults are Claude Code's (*Decision
  authority*), phase F's list does not hold #18 (`roadmap.md:343-357`),
  and the change narrows what the stack exposes. It touches no deployment:
  the in-container binds (`INGEST_BIND_HOST`, `API_BIND_HOST`) are
  unchanged. Alternative: leave it to phase F. Where: `6841aa5`.
- #36 pinned by the tag `24.8.14.39`, the release that `24.8` resolves to
  today (the same image ID, `1ffa82edee00`), not by digest. The entry asks
  for a patch release, and every other image in the compose is named by a
  tag. Alternative: a digest, which would also survive a re-pushed tag.
  Where: `d94ab7b`.
- #19's `catch` closes the apps built so far, kept in a list, and then the
  services. Alternative: outer `let` variables, which broke the type
  narrowing inside the returned `close()`. Where: `67f2055`.
- #10 and #33 put to Manuel instead of fixed. #10 changes the error
  semantics of the public read API (404 to 503 on a store failure, which
  amends SPEC-009, SPEC-010, SPEC-012 and SPEC-013). #33 changes the default
  of a variable that a deployment reads. Each went to him as one question
  with a recommendation (*What waits on Manuel*).
- #9 left for a later sweep: it changes how the forensic report composes
  its two reads (`services/api/src/forensic/report.ts:137-141`), and all it
  saves is one query.
- #57: the sentence keeps the rule and states the fact. Alternative: remove
  the field from SPEC-001 to SPEC-009, an edit to nine Accepted SPECs that
  gains nothing. Where: `ac80734`.

Debt sweep 1, reactive:

- #19's second test. The entry's "fail-loud" is the driver refusing an
  empty rules directory after both listens. The first test reached the
  `catch` by another path and accepted any `EADDRINUSE`. First review.
  Where: `eddadd7`.
- #18 without a switch. The first version published on
  `${CG_PUBLISH_HOST:-127.0.0.1}`, and `.env.example` documented `0.0.0.0`.
  That was a new setting on a trust boundary, which the entry did not ask
  for. The README's case for it, an agent in a virtual machine, also fails
  the heartbeat's certificate check: the self-issued certificate covers
  `localhost` and `127.0.0.1` only (SPEC-004 Amendment 2026-09-21, `:309`).
  First review. Alternative: keep the switch and correct the README. Where:
  `6841aa5`.
- The dev README dials `127.0.0.1`. The old mappings also served `[::]`. On
  Windows `localhost` tries `::1` first, and a refused connection costs
  about two seconds. First review. Where: `6841aa5`.
- The branch's history rewritten. The first #18 commit (`2534398`) made a
  claim in its README that the next commit retracted. The two became
  `532e997`, so every commit is true at its own SHA. It was Claude Code's
  own branch (*Integration path* §3). Where: PR #8.
- The landing loop stopped. The system killed it under low memory, after
  `d94ab7b`'s cherry-pick and before its push: 633 MB of 16 GB were free,
  and Docker's WSL VM held about 8 GB. Claude Code stopped and asked, then
  resumed once Manuel had freed memory.

Delegated by Manuel on 2026-10-14, decided by Claude Code for an
enterprise buyer (a security lead and a data-protection officer):

- **SPEC-021, §B2, §B3, Part (b) §9:** B2 and B3 parked. Claude Code
  cannot draft SPEC-021 (the platform's safety classifier, S37), Manuel no
  longer holds what it was to say, and neither phase is an MVP criterion.
  Reopen when a SPEC-021 text reaches the repository from outside Claude
  Code. Alternative: B2 on another contract, which would be the same
  content reworded. Where: `4cad976`.
- **The order:** E (SOAR, criterion 6), then F (installation, criterion
  7), the two MVP criteria left. Alternative: F first, which E does not
  depend on but which is last by dependency in the roadmap. Where:
  `4cad976`.
- **#10:** a malformed input is a 404 or a 400, and a store that cannot
  answer is a 503. A 404 during an outage tells an analyst an incident
  does not exist, which is a forensic falsehood. Alternative: a 500, which
  separates an outage from "not found" too, but not from a defect. Where:
  `ff20923`, `4a6e981`.
- **#33:** 5000 ms, the contract's value. Alternative: amend NFR-006-001
  to 10 000. Where: `ff20923`, `8ea922e`.
- **#67:** the api's own default listener, `http://127.0.0.1:8081`.
  Alternative: 18081, the dev compose's host port, which ties the
  dashboard's code to one dev setup. The README says what to set against
  the dev stack. Where: `bbdf1e3`, `12a7f13`.
- **#64:** SPEC-005 follows the code (`warn`). ADR-0008's helper names
  stay, and the code takes them at the next change to
  `agent/cg-agent/src/etw/` under the elevated gate. That keeps the ADR as
  it is, with no "sí" needed. Alternative: amend the ADR to the code's
  names. Where: `ff20923`.
- **#59:** no virtual machine on Manuel's host. The Windows 10 / Server
  2019 coverage matters for B2's second session, which is parked.
- **#55:** Claude Code brings the threat model in line with the system
  before F. The new risks it finds are listed as open, not accepted.
- **The Docker disk:** only what rebuilds was pruned (build cache,
  dangling images). Alternative: `docker system prune` with volumes,
  which would delete other projects' data on the same daemon.
- **Part (b) §1–§4:** §1 by an ADR-0002 amendment, §2–§4 in F's SPEC. The
  ADR amendments still wait for Manuel's "sí". Where: `4cad976`.

Debt sweep 2, anticipated:

- The SPEC amendments land first on `main`, then the code on a branch
  (*Compensating controls* §1).
- #10's errors are decided at two points. Input is checked before the
  read's stores, and every store call goes through one wrapper, `fromStore`,
  so a store's failure is a `StoreUnavailableError` → 503. Alternative:
  classify each driver's errors by code (SQLSTATE, ClickHouse codes),
  which is fragile, and a code defect inside a store call reads as an
  outage either way.
- The UUID check is the canonical form only, and a valid upper-case
  `:id` is read in lower case. Alternative: accept every spelling Postgres
  takes, which echoes the caller's spelling back.
- The api's Postgres pool gets a 5 s connection timeout, so a silent
  Postgres is a 503 rather than a held request. Alternative: a debt only.
  The pool also serves SPEC-008 and the forensic key's load at boot, which
  the amendment states.
- The list parameters are checked so that no value reaches Postgres as a
  failure. An empty parameter stays absent and `severity_id` is read as a
  number, as before, to break as few clients as possible.

Debt sweep 2, reactive:

- The amendment's first review (SPEC text): a Redis outage hangs rather
  than answering 500 (#69); two items clashed on "whatever the `:id`";
  the backward-compatibility list missed two kinds of client; ADR-0015
  says only the drill reads ClickHouse (#70). All fixed in the text before
  `ff20923`, the ADR as a debt.
- The code's first review, the most serious item first. The app-wide
  error handler, newly logging 5xx, logged the whole error. A Postgres
  `detail` can quote a `users` row, password hash and encrypted TOTP
  secret included. Now only type, code, message and stack are logged.
  Also fixed: a cursor year Postgres refuses, and a `status` with a NUL,
  still reached the store. `severity_id` was stricter than before. And
  the impossible "incident without source events" case. Where:
  `6cd1871`, `56dfc4b`, `4a6e981`.
- The code's second review: pino's `err` serializer replaced `type`, so
  the log moved to the key `error`. "Before any store" was not true of
  Redis, and a line citation would move. The pool's reach is now stated,
  and the debts it cites are recorded. Where: `3659e15`, `4a6e981`.
- The #10 commit was rewritten twice on its branch to absorb the review
  fixes, so that no `main` SHA carries the detail-logging state. The
  branch was Claude Code's own (*Integration path* §3).
- The landing slip (*Progress*).

## Self-reviews

Contract: debt #63 (`docs/handoff-session-37.md:517-532`); for
`agent/cg-agent/tests/etw_reclaim.rs`, ADR-0008 §Decision part 2.

Claude Code's matrix at `928cfbf`. The test file is
`services/ingest/test/marquee-agent-etw.test.ts` unless named:

| Criterion | Test | Verdict |
|---|---|---|
| The harness stops the agent's session at each marquee's end | `:127` (real `logman`, a session with no provider), `:105` (a running session is stopped), `:76`, `:82`, `:88` (fake `logman`); `run()`'s exit handler, `helpers/marquee-agent.ts:226-254` | covered for the stop; the wiring in `run()` runs only in the elevated marquees: gate 1b, one `marquee_agent_etw_session_stopped` in each of the five |
| A session it cannot stop is reported, not ignored | `:76` | covered with a fake; that an unelevated query of a session an elevated agent started answers 0x80070005 is measured on system sessions only, and any other answer throws |
| No session of the agent's name after the gate | gate 1b, `logman query` after vitest | covered |
| An unelevated capture_ac_001 after an elevated gate passes | Claude Code's run after gate 1b | covered |
| The harness's session list matches the agent's | `:53` | covered as a tripwire: constants named SESSION or LOGGER, or valued with the agent's prefix |
| An orphaned agent does not stop the next agent's session | `:99`; the registry `helpers/marquee-agent.ts:100-104`, `:236` | weak: the registry's bookkeeping is driven by no test |
| At open, a live session of the agent's name is stopped before the start (ADR-0008) | `etw_reclaim.rs:34-57` | covered: with the reclaim disabled, unelevated, the open failed with 183 |

A reviewer with a fresh context, the fixed code prompt, three passes:

- **Pass 1, `3114bdf..fd5d2ce`.** List 1: the stop at each end weak (a
  stand-in session; the wiring only elevated); no session after the gate
  weak; an unelevated capture_ac_001 after the gate missing; the elevated
  gate missing. Contradicted: the docstring said the survivors are
  returned, while an unelevated query of another account's session threw
  first. List 2: none. List 3: a rejection drops the marquee's output
  dumps; three `logman` calls inside the D7 budget; the reclaim loses its
  only exercise; the test starts a session on the host; an elevated stop by
  name reaches a real agent's session. Dispositions: the contradiction
  fixed in `80b9708`; the dumps answered by the exit code in the rejection
  (`80b9708`, the stderr removed in `da0f38a`); D7 measured, 40.31 s in gate
  1b; the reclaim asserted in `bf08261`; the session on the host and the
  stop by name recorded, no change (the agent's own reclaim stops a session
  of the name, SPEC-017:50); weak and missing answered by gate 1b and the
  unelevated run after it.
- **Pass 2, `3114bdf..a011a2e`.** List 1: the stop weak; capture_ac_001
  after the gate missing; the gate missing; a survivor reported, covered
  (logic); the reclaim covered; a stop's 4201 missing here; `events_lost`
  weak. The early-failure path: an orphaned agent's exit stops the next
  marquee's session, and a throw there is unhandled. List 2: ADR-0008
  §Compliance's "do not duplicate the literal" against
  `helpers/marquee-agent.ts:44`; the tripwire's narrow pattern; older,
  ADR-0008's `stop_zombie` / `StopOutcome` against the code's
  `reclaim_zombie` / `bool`, and SPEC-005:230's `info` with `reclaim_path`
  against a `warn` without it. List 3: the rejection untested; the stderr
  in failure messages against SPEC-020's redaction; six new tests; no
  change to authentication, authorization, cryptography, privileges or the
  data `cg-agent` collects. Dispositions: the orphan path, the tripwire and
  the stderr fixed in `da0f38a`, with `etw_reclaim`'s message on both
  outcomes; the literal recorded, since TypeScript cannot import the Rust
  constant and the tripwire pins it (`capture_ac_010_hygiene.rs:185-188`
  already repeats it); the older mismatches are debt #64; a stop's 4201 and
  `events_lost` are not this change's criteria, no change.
- **Pass 3, `3114bdf..ea120c3`.** List 1: the stop weak (the real case only
  elevated; `run()`'s wiring untested; the after-exit test asserted the
  query only); the gate missing; no session after the gate weak, by two
  orphan paths (a skipped exit waits for the other agent's; vitest's
  process exiting ends an orphan before its exit handler runs); the reclaim
  covered, its `Err` arm accepting a privilege failure whatever the
  elevation; `events_lost` as the probe weak. List 2: the literal;
  SPEC-005:230. List 3: in #63's state `etw_reclaim` fails at `logman start`
  besides capture_ac_001; the early rejection unhandled; the registry and
  the tripwire not asked for; the access-denied answer unverified on an
  agent's session. Dispositions: the after-exit test, the unhandled
  rejection and the probe fixed in `928cfbf`; the orphan ended with
  vitest's process is debt #65; the `Err` arm recorded, since without the
  reclaim the start fails with 183 whatever the elevation, and which arm
  ran is not printed (as capture_ac_001's, handoff-37 B9); the double
  failure recorded, since a green gate no longer leaves that state; the
  access-denied answer recorded in the matrix.
- `928cfbf`'s own lines had no fresh-context pass: each answers an item of
  pass 3.

### Debt sweep 1

Contract: debt #18 and #19 (`docs/handoff-session-28.md:172-178`) and #36
(`docs/handoff-session-33.md:175-177`); for CLAUDE.md, #57
(`docs/handoff-session-36.md:330-336`).

Claude Code's matrix at `532e997`, the landed tree. The test file is
`services/ingest/test/server-start-failure.test.ts`:

| Criterion | Test | Verdict |
|---|---|---|
| #19: a failure after a listen closes the listeners already bound | `:38-56` (the heartbeat listen fails; the enroll port binds again, `:52`); `:58-80` (the driver refuses after both listens; both ports bind again, `:75-76`) | covered: both fail against `bb0305e`'s `server.ts` with `EADDRINUSE` on the re-bind, and pass with the fix |
| #19: the services are still closed | `services/ingest/src/server.ts:106` | weak: no test asserts it; the diff leaves it as it was |
| #18: every published port binds `127.0.0.1` | none; `docker compose … config` renders 11 `host_ip: 127.0.0.1` | missing as a test; checked by hand on the branch and at the close (debt #68) |
| #36: the dev compose and both test backends pin a patch release | `infra/dev/docker-compose.dev.yml:32`, `services/ingest/test/helpers/backends.ts:41`, `services/api/test/helpers/backends.ts:44`; `ts-ci` pulled the tag at `8e333d4` | covered by reading; nothing asserts the server's `version()` |
| #57: the sentence states each SPEC's header | SPEC-001 to SPEC-009 carry `Last updated` at `:9`, SPEC-010 to SPEC-020 do not; the amendments since 2026-06-05 (SPEC-004 2026-09-21, SPEC-006 2026-10-11, SPEC-008 2026-09-23, SPEC-009 2026-06-05) left it as it was | covered by reading |

A reviewer with a fresh context, the fixed code prompt, two passes:

- **Pass 1, `bb0305e..2534398`.** List 1: #19's failed heartbeat listen
  covered (it fails against `bb0305e`); the scenario the entry names, the
  driver's refusal, weak (no test drives it through `startIngest`; the
  reviewer's probe passed with the fix and failed without it); the port
  match loose; the services' close untested and unchanged. #18 missing as a
  test (the compose renders 11 loopback mappings). #36 missing as an
  assertion, with good evidence (the same image ID). List 2: #18's address
  only a default, with `0.0.0.0` documented in `.env.example`; #18 done
  outside phase F's SPEC. List 3: `CG_PUBLISH_HOST` a trust-boundary
  setting the entry did not ask for; the README's virtual-machine case
  fails mTLS (`services/ingest/src/cert.ts:99-111`, SPEC-004:309); IPv6
  loopback no longer served while the README still dialed `localhost`, as
  does SPEC-001:111; the new compose never brought up; the container's
  fail-loud exit now waits for `close()` before `process.exit(1)`; no change
  to authentication, authorization, cryptography, privileges or the data
  `cg-agent` collects. Dispositions:
  - The driver's refusal and the loose port: fixed in `eddadd7`.
  - The switch and the virtual-machine case: the switch removed in
    `6841aa5`, whose README says what another machine needs.
  - IPv6: the README dials `127.0.0.1` (`6841aa5`). SPEC-001:111 is the
    agent's configuration example in an Accepted SPEC, and the marquees
    dial `127.0.0.1`: no change.
  - The phase F route: recorded as a decision (*Decisions*).
  - The compose never brought up: recorded (*State at close*).
  - The wait for `close()`: no change, since the normal `close()` has the
    same property.
- **Pass 2, `bb0305e..532e997`.** List 1: #19's two cases covered (negative
  control: both fail with `EADDRINUSE` against `bb0305e`; the heartbeat
  half, `:76`, was shown only at the tip, since `:75` fails first); the
  services' close weak and unchanged; Fastify's `close()` on an app whose
  listen never ran or failed tolerates `ERR_SERVER_NOT_RUNNING`, so the
  `allSettled` cannot hang; #18 missing as a test and satisfied (11
  mappings); #36 satisfied, a tag and not a digest. List 2: none. List 3:
  the README's claims checked (`cert.ts:110-111`, SPEC-004:309); clients
  that dial `localhost` lose the IPv6 path, among them the dashboard's
  default `API_BASE_URL`, which also points at ingest's enroll port, older
  than the diff; the narrowing of ingest's and the api's exposure to the
  host's loopback "is still a change to a trust boundary, which is on
  Manuel's list, so it should be recorded as a decision"; residuals: the
  driver not stopped in the `catch`, a rejection of `services.close()`
  hiding the original error (as before), and `freePort()`'s release before
  the bind. Dispositions:
  - The dashboard's default: debt #67.
  - The trust boundary: recorded as a decision (*Decisions*, #18) and
    reported to Manuel in the turn. It is a dev compose default, the
    exposure #18's entry asked to pin, and it narrows.
  - The residuals: no change. Nothing after the driver's start can throw
    today; the hidden error predates the diff; the window between
    `freePort()`'s release and the bind is small, and a flake there would
    be a new debt.
- `ac80734` (CLAUDE.md, a fact): Claude Code's own pass only, checked
  against the twenty SPEC headers above.

### The delegation's SPEC amendments and debt sweep 2

**The amendments' text**, a fresh-context pass with the fixed SPEC prompt,
on the working tree before `ff20923`:

- List A, all four items in SPEC-009's amendment:
  - A1: a Redis outage hangs, it does not answer 500.
  - A2: two items clashed ("whatever the `:id`" against a malformed `:id`).
  - A3: the backward-compatibility list missed non-canonical UUID
    spellings and out-of-range `severity_id`.
  - A4: ADR-0015 §Compliance says only the drill reads ClickHouse.
- List B: ten items, among them a text that read as already done, the
  SPEC-005 rationale (the agent's default level is already `info`), the
  undefined "unreachable", the upper-case `:id` echoed, the framework's
  own status, and CLAUDE.md's amendment list missing SPEC-005.
- Dispositions: A1 → item 6 and debt #69; A2 and A3 fixed; A4 → named in
  the amendment and debt #70; from B, each "document change" note, the
  rationale, the definition of "cannot answer", the lower-casing, the
  framework status and the CLAUDE.md list. The revised text had no second
  pass of its own: the code reviews below read it against the diff.

**Claude Code's matrix at `de2b826`** (the landed tree). The api tests are
`services/api/test/read-ac-006-error-sanitisation.test.ts` (006) and
`read-ac-007-store-failure.test.ts` (007):

| Criterion | Test | Verdict |
|---|---|---|
| SPEC-009 §1: a malformed `:id` → 404 before the read's stores, on all four routes | 006 `test.each` over the four routes; 006, an existing incident with braces or no hyphens → 404 (200 before); 007, the four routes with Postgres down → 404 | covered |
| §1: an upper-case `:id` read and answered in lower case | 006, detail, drill and export | covered (the report returns a PDF; same helper) |
| §2: malformed `agent_id`, `severity_id`, `cursor` (years 0, -1, 10000), `status` with a NUL → 400 | 006 `test.each`; 007, list parameters with Postgres down → 400 | covered |
| §2: empty parameter absent, unknown `status` no row, `severity_id` read as a number, year 1 accepted | 006 | covered |
| §3: Postgres dropping, refusing or silent → 503 | 007, the six routes dropping; list and detail refusing; detail silent, 4.9–8 s; the api's own pool carries the 5 s timeout | covered |
| §3: ClickHouse failing → 503 on drill, export and report; detail and lists 200 | 007, dropping (three routes), refusing (drill) | covered |
| §3–§4: 503 and 500 logged at `error`, type and code kept, no `detail` | 007 "what is logged": Postgres 503, ClickHouse 503, a signing failure's 500 | covered |
| §4: a status Fastify raises keeps it | 007, invalid JSON on login → 400 | covered |
| §5: no existence oracle | 007, another org's incident with ClickHouse down → 404; with Postgres down → 503 like any well-formed `:id` | covered |
| SPEC-018: the default is 5000 ms | `services/ingest/test/config-detect-interval.test.ts`; the compose and `.env.example` by reading | covered in code; the dev files missing as a test (#68) |
| #67: the default is the api's own listener | `dashboard/test/api-base-url.test.ts`, against the api's `loadConfig` defaults | covered; the two pages' use by reading |

A reviewer with a fresh context, the fixed code prompt, two passes:

- **Pass 1, `4cad976..8cc1901`.**
  - List 1: the malformed-`:id` test could not tell form from "not found"
    (it used an all-zero UUID); the lower-casing, the logs, the 500, the
    refusing store and the no-source-event case missing; the timeout bound
    loose.
  - List 2: a cursor year Postgres refuses, and a `status` with a NUL,
    still reached the store (503); `severity_id` stricter than before; the
    timeout's comment overclaimed.
  - List 3, the security item: the app-wide handler now logged 5xx on
    SPEC-008's routes with the whole error, and a `users` CHECK failure's
    `detail` quotes the row's password hash and encrypted TOTP secret. Also
    the pool's 5 s reaching auth and boot, `fromStore` wrapping a code
    defect as an outage, the empty cursor, and #67's value against the dev
    compose.
  - Dispositions: all of List 2 and the security item fixed (log type,
    code, message and stack only; inputs refused before the store);
    List 1's gaps tested; the pool's reach and the empty parameter stated
    in the amendment (`6cd1871`); #67's value documented in the dashboard
    README; `fromStore`'s breadth recorded (*Decisions*); a silent open
    connection → #71; the unvalidated role → #72.
- **Pass 2, `56dfc4b..6ade78b`.**
  - No blocking defect.
  - List 2: the logged `type` replaced by pino's serializer; a stale line
    citation; "before any store" untrue of Redis; ClickHouse "for every
    incident" untrue of an incident whose alerts are gone; debts cited but
    not yet recorded.
  - Weak: year 1, list parameters with a dead store, `buildServices`'s
    pool, the ClickHouse log.
  - Dispositions: all fixed (`3659e15`, `4a6e981`); the stale comment in
    `report-ac-003-transport.test.ts` updated.
- The second pass's own fixes had no third pass. Each answers an item of
  pass 2, and the type fix was shown red under the old key.

### ADR-0021 and the ADR-0002 and ADR-0015 amendments

Claude Code's own pass covered the pre-flight against ADR-0001, ADR-0002
Rules 3 and 6, ADR-0003, ADR-0005, ADR-0017 and the threat model's §5. It
also checked each cited line against the tree. A reviewer with a fresh
context, the fixed ADR prompt, made two passes on the working tree:

- **Pass 1, 11 items in List A**, all fixed:
  - the threat model's Spoofing claim: a trigger can be caused by making
    the cycle create an incident;
  - Postgres left out of the trust boundary and the credentials;
  - an idempotency key written after the steps: now the record is claimed
    first;
  - a playbook whose email preceded the step it reported: the order is
    reversed;
  - a delay bound that did not bound: now 30 s per run, stated;
  - ADR-0002's amendment deciding the firehose's language, against ADR-0007
    §Compliance;
  - §Compliance's old Rule 3 sentence;
  - the harness READMEs: the runner keeps Go;
  - `playbooks/README.md`;
  - ADR-0015's third drill-only statement;
  - `services/README.md`'s self-contradiction.

  From List B, also fixed: the severity seen only at creation, the chain's
  real limits, `ajv` and the CI step, the SMTP timeouts, the citations and
  the dashboard destination.
- **Pass 2, 6 items in List A**, all fixed:
  - Rule 6's line, moved by the new blockquote;
  - Rule 3 restated whole, not its first sentence;
  - one reading of the firehose's language: Go stays, and the firehose ADR
    confirms or changes it;
  - READMEs that described SOAR as built: now in the future tense;
  - the shared database users: a fact of the dev stack, not §8;
  - sharing the chain code: possibly a new component, Manuel's (#7).

  From List B, also fixed: the departure from ADR-0017's A2 named; the
  whole send bounded; validation at every boot; `ajv`'s version, licence
  and absence of install scripts; retention against ADR-0003's audit-log
  row; the catalog edges; the Blueprint's names; and the api's read in the
  threat model. Recorded without a change: the implementation runs the
  elevated gate, because the call site is in `detect/`.
- Pass 2's fixes had no third pass; each answers an item of pass 2.

## Debts

New this session:

- **#64 — The startup reclaim's contract texts and the code differ.**
  ADR-0008 §Decision part 2 (`docs/adr/0008-etw-crate-selection.md:26`)
  names `stop_zombie(session_name) -> Result<StopOutcome, u32>`; the code
  has `reclaim_zombie(session_name) -> Result<bool, u32>`
  (`agent/cg-agent/src/etw/events_lost_impl.rs:127`). SPEC-005
  §Observability (`docs/specs/SPEC-005-agent-process-telemetry-windows-etw.md:230`)
  asks for the reclaim logged at `info` with `reclaim_path`
  (`"helper.stop_zombie"`); the code logs it at `warn` with `session_name`
  only (`agent/cg-agent/src/etw/session.rs:132-136`). Surfaced by the
  second review. Destination: SPEC-005's row is Claude Code's (an amendment,
  or the code) with B2's implementation, which changes the open path for
  the second session; ADR-0008's names are an ADR text, Manuel's.
- **#65 — An orphaned marquee agent can still leave its session.** If a
  marquee throws before it awaits its run, its agent runs until the timer;
  if vitest's process exits first, libuv's job object ends the agent and
  `run()`'s exit handler never runs, so its session stays until the next
  start of the agent or a restart. It happens only after a marquee failed.
  Destination: reopen if a gate leaves a session; a `try`/`finally` in each
  marquee that ends its agent closes it.
- **#66 — Gate output noise.** `etw_reclaim.rs`'s `logman` calls inherit
  stdout, so `logman`'s text appears in cargo's output (gate 1b's log); the
  after-exit test with a fake `logman` logs `marquee_agent_etw_session_stopped`
  with the agent's session name though nothing was stopped, so a gate's
  stop lines are counted per test file. Destination: the next change to
  either file.
- **#67 — The dashboard's default API address is ingest's enroll port.**
  `dashboard/app/incidents/page.tsx:11` and
  `dashboard/app/incidents/[id]/page.tsx:9` fall back to
  `http://localhost:8080` when `API_BASE_URL` is unset. The api listens on
  8081 by default (`services/api/src/config.ts:30`), published on the host
  as 18081 by the dev compose (`infra/dev/.env.example:55`); 8080 is
  ingest's enroll listener. The dashboard is not in the dev compose (the
  S19 follow-up, `docs/handoff-session-19.md:95`). Surfaced by the second
  review of the sweep. Destination: the next sweep. Like #33 it changes the
  default of a variable a deployment reads, so it follows Manuel's answer
  on #33.
- **#68 — No workflow checks the dev compose.** No workflow's path filter
  covers `infra/dev/` (`.github/workflows/*.yml`), so a change to
  `docker-compose.dev.yml` runs no CI on `main`: for `6841aa5` only
  `markdown-lint` ran, for its README. #18 and #36 were checked by hand with
  `docker compose config`. Surfaced by both reviews of the sweep.
  Destination: a CI step that renders the compose and asserts its port
  bindings and image tags, Claude Code's, in a later sweep.
- **#69 — An unreachable Redis holds an api request.** `services/api/src/services.ts`
  builds the Redis client with `maxRetriesPerRequest: null`, and ioredis
  keeps its offline queue. With Redis unreachable, the session
  preHandler's `resolveSession` (`services/api/src/auth/prehandlers.ts`)
  waits until Redis returns, and Fastify sets no handler timeout. A Redis
  error reply answers 500. Surfaced by the review of SPEC-009 Amendment
  2026-10-14. Destination: SPEC-008's surface, a bounded retry or a
  command timeout; Claude Code's, in a later sweep.
- **#70 — ADR-0015 says a ClickHouse outage fails only the drill.**
  `docs/adr/0015-readonly-clickhouse-reader-in-api.md:70` (§Compliance:
  "fails **only** the drill route") and `:57` ("for **this route only**").
  Since SPEC-012 and SPEC-013 the forensic export and the report read
  ClickHouse too, and SPEC-009 Amendment 2026-10-14 item 3 names all
  three. Surfaced by the same review. Destination: an ADR-0015
  amendment, which needs Manuel's "sí", in the next batch of ADR texts
  put to him.
- **#71 — The api's Postgres client has no query timeout.** The pool
  gives up after 5 s on a connection it cannot make or get from a busy
  pool. A connection already open whose peer goes silent holds the
  request until the operating system's TCP timeout. The threat model's
  "query timeouts" for cg-api (`docs/security/threat-model.md:85`) are
  not realised. Surfaced by the review of the sweep's code. Destination:
  a client query timeout or a `statement_timeout`; Claude Code's, in a
  later sweep.
- **#72 — An unknown role answers 500 on SPEC-008's user routes.**
  `POST /v1/users` and `POST /v1/users/:id/role`
  (`services/api/src/auth/routes.ts:49-71`) pass the role to Postgres
  unchecked, and the `users` CHECK (`services/api/src/db/migrations/0001_users.ts:24-25`)
  fails with a 500 rather than a 400. Before the sweep's log fix, that
  failure's `detail`, the whole row with its password hash, would have
  reached the log. Surfaced by the review of the sweep's code.
  Destination: SPEC-008's input validation; Claude Code's, in a later
  sweep.

Closed this session:

- **#63 — The marquees kill the agent, so the last one leaves its session
  running.** Fixed by `a9452b0..928cfbf`: once a marquee's agent has
  exited, the harness stops the agent's ETW session (Windows). Gate 1b: one
  stop per marquee, no reclaimed session, none left after the run; then,
  unelevated, capture_ac_001 passed.
- **#19 — Fail-loud does not close the listeners.** Fixed by `67f2055` and
  `eddadd7`: a failure after a listen closes the listeners already bound,
  then the services. Both cases are tested, red without the fix.
- **#36 — The ClickHouse image tag floats.** Fixed by `d94ab7b`:
  `24.8.14.39` in the dev compose and both test backends.
- **#18 — The dev compose publishes ports on all host interfaces.** Fixed
  by `6841aa5`: all 11 mappings bind `127.0.0.1`, with no setting that
  undoes it.
- **#57 — CLAUDE.md says a SPEC header has no `Last updated`.** Fixed by
  `ac80734`.
- **#10 — The event drill answers 404 when ClickHouse is down.** Widened
  to the read API's error semantics and fixed by SPEC-009 Amendment
  2026-10-14 (`ff20923`, made exact in `6cd1871`, `56dfc4b`, `3659e15`)
  and `4a6e981`.
- **#33 — The driver's default interval is 10 000 ms.** Fixed by SPEC-018
  Amendment 2026-10-14 and `8ea922e`.
- **#67 — The dashboard's default API address is ingest's enroll port.**
  Fixed by `bbdf1e3`, documented by `12a7f13`.

Changed this session: #64 keeps only its ADR-0008 half. SPEC-005
Amendment 2026-10-14 resolved the SPEC half (`ff20923`). The code takes
ADR-0008's names at the next change to `agent/cg-agent/src/etw/` under
the elevated gate.

The inventory also found where the auto-memory index differed from the
records. The records are right and stay as written: #1 was resolved in S25
(`docs/handoff-session-26.md:148`); #15 and #16 were closed in S29
(`docs/handoff-session-29.md:116-119`); #54 is recorded in handoff-36's
*Progress* (`:168-182`), not its *Debts*; #3's cited collision is gone (the
two UUIDs now differ), while its risk stays (18 hand-numbered agent IDs in
16 files). Open debts at the close: 49. That is the inventory's 50, less
the seven closed by the two sweeps (#18, #19, #36, #57, #10, #33, #67),
plus #67, #68 and #69–#72.

## Gate outputs and baselines

Non-elevated, Claude Code's terminal, at `6fbcd0e` (the tree of `928cfbf`):

- `cargo fmt --all -- --check` clean; `cargo clippy --all-targets
  --all-features -- -D warnings` clean; `cargo test --all`: 126 passed, 0
  failed, 7 ignored (S37 125, plus `etw_reclaim`).
- `services/ingest`: `pnpm run typecheck`, `pnpm run lint` ("Checked 107
  files … No fixes applied."), `pnpm run build`, the entrypoint check
  ("entrypoints ok: [ 'dist/index.js', './dist/cli/issue-token.js' ]");
  `pnpm test` → "Test Files  4 failed | 56 passed | 1 skipped (61)",
  "Tests  4 failed | 200 passed | 1 skipped (205)": the four capture
  marquees with the agent's exit 9 and `ac-001-marquee` skipped, the
  expected unelevated result.
- The red runs, against the tree without each change: the first tests
  failed against a stub ("expected [] to deeply equal [
  'CGAgent-KernelProcess' ]", "no session of the name after the stop:
  expected true to be false"); the reported and error paths against
  `fd5d2ce`'s helper ("expected [] to deeply equal [ 'X' ]", "expected
  [Function] to throw an error"); the after-exit tests against `a011a2e`'s
  ("stopSessionsAfterExit is not a function"); `etw_reclaim` with the
  reclaim disabled ("got Failed { code: 183, message: \"No se puede crear un
  archivo que ya existe.\" }").

Gate 1, elevated, on 2026-10-13 (logs `C:\tmp\s38-gate1-*.log`): the pasted
block ran last line first. `logman` found no `CGAgent-KernelProcess`;
`net session` → "No hay entradas en la lista."; Docker `29.1.2`; every
other step ran in `C:\` and failed there (`pnpm`: "No package.json … was
found in "C:\""; `cargo`: "could not find `Cargo.toml`"; `git`: "not a git
repository"). Nothing ran in the repository.

Gate 1b, elevated, at `6fbcd0e`, on 2026-10-13 (`C:\tmp\s38-gate1b.ps1`,
logs `C:\tmp\s38-gate1b-*.log`, 14:11:37–14:15:07):

- `docker version` → `29.1.2`; `net session` → "No hay entradas en la
  lista."; `6fbcd0e`; `git status --porcelain` → nothing; `logman query
  -ets` → no `CGAgent` session.
- `cargo build --release -j 2 -p cg-agent` → "Finished `release` profile
  [optimized] target(s) in 23.87s".
- `cargo test … --test etw_reclaim --test capture_ac_001_startup_failure`
  → "test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0
  filtered out; finished in 0.04s" and "test result: ok. 1 passed; 0
  failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s".
- vitest `services/ingest` → "Test Files  61 passed (61)", "Tests  205
  passed (205)", "Duration  180.10s". One
  `{"event":"marquee_agent_etw_session_stopped","session":"CGAgent-KernelProcess"}`
  in each of `ac-001-marquee`, `spec-005-marquee`, `detect-ac-001-marquee`,
  `net-ac-010-marquee` and `auth-ac-013-marquee`; no "reclaimed zombie ETW
  session" line.
  `{"event":"spec_005_marquee_complete","marquee_elapsed_seconds":40.31,"budget_seconds":45,"within_budget":true}`;
  `net_ac_010` 25.33 s and `auth_ac_013` 25.379 s within 45; "detect_ac_001
  captured image_file_name — probe:
  C:\Users\<user>\AppData\Local\Temp\cg-detect-probe-ogwpPz\winword.exe;
  child: C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe";
  "spec_005 captured image_file_name — launch: C:\Windows\System32\cmd.exe;
  terminate: cmd.exe".
- After it: `logman query -ets` → no `CGAgent` session; `logman query
  CGAgent-KernelProcess -ets` and `logman stop CGAgent-KernelProcess -ets`
  → "No se encontró el Conjunto de recopiladores de datos."; "gate 1b done
  2026-10-13T14:15:07.3896040+02:00".

After gate 1b, unelevated: `cargo test -p cg-agent --test
capture_ac_001_startup_failure --test etw_reclaim` → "test result: ok. 8
passed" and "test result: ok. 1 passed"; `logman query -ets` → no
`CGAgent` session.

CI for #63: every push green. On the branch, `fd5d2ce` and `2708f9f` got
`ts-ci → success`, and `a011a2e`, `ea120c3` and `6fbcd0e` got `ts-ci` and
`rust-ci → success`; on `main`, the landing block above. `rust-ci`'s
Windows job ran `etw_reclaim`: "test
open_reclaims_a_leftover_session_of_the_agents_name ... ok".

Debt sweep 1, non-elevated, Claude Code's terminal:

- At `2534398`'s tree (`d138684`, `8e333d4` and the first #18 commit):
  - `services/ingest`: `pnpm run typecheck`; `pnpm run lint` ("Checked 108
    files … No fixes applied."); `pnpm run build`; the entrypoint check
    ("entrypoints ok: [ 'dist/index.js', './dist/cli/issue-token.js' ]");
    `pnpm test` → "Test Files  4 failed | 57 passed | 1 skipped (62)",
    "Tests  4 failed | 201 passed | 1 skipped (206)". The four capture
    marquees failed with exit 9 and `ac-001-marquee` skipped.
  - `services/api`: `pnpm run typecheck`; `pnpm run lint` ("Checked 69
    files … No fixes applied."); `pnpm run build`; the entrypoint check
    ("entrypoints ok: [ 'dist/index.js', './dist/cli/create-user.js' ]");
    `pnpm test` → "Test Files  31 passed (31)", "Tests  62 passed (62)".
- At `532e997`, `services/ingest`: `pnpm run typecheck`; `pnpm run lint`
  (108 files, no fixes); `pnpm test` → "Test Files  4 failed | 57 passed |
  1 skipped (62)", "Tests  4 failed | 202 passed | 1 skipped (207)", with
  the same four and the same skip. Build and `services/api` were not run
  again: `1039b1a` adds a test file and `532e997` touches no TypeScript.
- The red runs, against `bb0305e`'s `server.ts`. The first test alone:
  "listen EADDRINUSE: address already in use 127.0.0.1:56297". Both tests
  at their final form: "× a failed heartbeat listen closes the enroll
  listener it follows … → listen EADDRINUSE: address already in use
  127.0.0.1:53215", "× the driver's fail-loud refusal closes both listeners
  … → listen EADDRINUSE: address already in use 127.0.0.1:53221", "Tests  2
  failed (2)". With the fix: "Tests  2 passed (2)".
- The compose, at the close on `ac80734`: `docker compose -f
  infra/dev/docker-compose.dev.yml --env-file infra/dev/.env.example
  config` exited 0 and rendered 11 `host_ip: 127.0.0.1`, published 15432,
  16379, 18081, 4222, 8080, 8123, 8222, 8443, 9000, 9001 and 9002, and
  `image: clickhouse/clickhouse-server:24.8.14.39`.
- CI on the branch (PR #8): `d138684`, `8e333d4` and `1039b1a` got `ts-ci →
  success`; `2534398` and `532e997` got `markdown-lint` and `ts-ci →
  success`. On `main`, the landing block in *Progress*.

Debt sweep 2, non-elevated, Claude Code's terminal:

- `services/ingest`, at #33's commit (`8599993`, before the rebases):
  - `pnpm run typecheck`, `pnpm run lint` ("Checked 109 files … No fixes
    applied."), `pnpm run build` and the entrypoint check, all passing.
  - `pnpm test` → "Test Files  4 failed | 58 passed | 1 skipped (63)",
    "Tests  4 failed | 204 passed | 1 skipped (209)": the four capture
    marquees with exit 9, and `ac-001-marquee` skipped.
  - The red run: "expected 10000 to be 5000".
- `dashboard`, at #67's commit:
  - `pnpm run build`, `pnpm run typecheck`, and lint ("Checked 25 files …
    No fixes applied."), all passing.
  - `pnpm test` → "Test Files  3 passed (3)", "Tests  8 passed (8)".
  - The red run: "expected 'localhost' to be '127.0.0.1'".
- `services/api`, at the final #10 tree:
  - `pnpm run typecheck`, `pnpm run lint` ("Checked 71 files … No fixes
    applied."), `pnpm run build` and the entrypoint check ("entrypoints
    ok: [ 'dist/index.js', './dist/cli/create-user.js' ]"), all passing.
  - `pnpm test` → "Test Files  33 passed (33)", "Tests  111 passed (111)".
- #10's red runs:
  - Against the code before #10, read_ac_007 failed 10 of 13: 404 or 500
    where 503 is due, and the silent Postgres "Test timed out in
    120000ms".
  - The extended read_ac_006 failed 16 of 23 against `main`'s `src/`.
  - The log tests failed with the seeded `detail` in the 503 and 500 lines
    ("not to contain 'argon2id-hash-that-must-not-be-logged'"), and again
    under the old `err` key.
- CI: every push green.
  - On the branch: the first push `8599993`, `be9ef90` and `8cc1901`; the
    first rebase `755cc87`, `177fc70`, `a4bf90d` and `6ade78b`; the second
    `843a80e`, `42b08e6`, `ccda047` and `de2b826`.
  - On `main`: the landing block in *Progress*, and each docs commit's
    `markdown-lint → success`.

Baselines at the close:

- Cargo, non-elevated: 126 passed / 7 ignored.
- Cargo, elevated `--ignored`: 7 (S37; not run again, since no agent
  source changed).
- vitest `services/ingest`, unelevated: 63 files / 209 tests (204 passed
  / 4 failed with exit 9 / 1 skipped).
- vitest `services/ingest`, elevated: 61 files / 205 tests at `6fbcd0e`
  (gate 1b, before the two sweeps' tests). Expected 63 / 209 at the tip,
  not run.
- `services/api`: 33 files / 111 tests.
- `dashboard`: 3 files / 8 tests.

## What waits on Manuel

Manuel delegated every pending decision on 2026-10-14 (*Decisions*). What
still needs him:

1. His explicit "sí" on each text of the owner-review branch
   `adr/s38-0021-soar` (read on GitHub, the branch's compare view):
   - ADR-0021, SOAR for the MVP, with the threat model's amendment (roadmap
     §E). It is meant to be ratified together with the next item.
   - ADR-0002's Amendment 2026-10-14 (Part (b) §1).
   - ADR-0015's Amendment 2026-10-14 (#70).

   Later, with F's SPEC, the ADR-0001 amendment (§4) and the ADR-0016
   amendment (§3).
2. Compacting the Docker WSL disk to give `C:` back the space the prune
   freed inside it. That needs WSL shut down, which is his to do or to
   allow (*Local environment operations*). Optional.
3. If he wants §B2 back: a SPEC-021 text from outside Claude Code
   (roadmap §B2).

## How Session 39 resumes

1. *Session protocol* §At the start. Expected:
   - `main` = `origin/main` at this file's last commit; tree clean; no
     stash;
   - one branch, `adr/s38-0021-soar` at `e619797`, with no PR, unless
     Manuel has ratified it and it has landed;
   - catalogs ADR 20 / SPEC 20; Known CI debt zero;
   - `logman query -ets` lists no `CGAgent` session.
2. Roadmap §E, criterion 6: "1 SOAR playbook operational (dry-run +
   notification, no destructive actions yet)" (`docs/product/blueprint.md:751`).
   - First Manuel's "sí" on `adr/s38-0021-soar`. On it, land the two
     commits by cherry-pick, with the marker stripped from each subject
     and ADR-0021's status set to Accepted. The catalog rows lose
     "proposed, NOT YET RATIFIED", and the dates of ratification go into
     the texts.
   - Then its SPEC and the implementation: tests failing first, CI on every
     push, the self-review, landing by cherry-pick with a guard that stops
     (`feedback-landing-guard-must-stop`).
3. Meanwhile, the next sweep of Claude Code's debts, each on a branch with
   a PR, a failing test first where a test applies, CI on every push and
   the self-review. In order of value:
   - #72 (the role check), #69 (Redis hold) and #71 (query timeout), on
     the api's availability and input;
   - #68 (a compose check in CI);
   - #6 (the api's copy of the `cges_events` table);
   - #30 (real-ETW tests on the hosted runner);
   - #39 and #40 (CGES examples and `time`);
   - #13 (stale citations, swept by pattern);
   - #27 and #41 (diagnosis first);
   - #3, #8, #9, #11.

   #7 may count as a new component, which is Manuel's: ask first.
4. #55: the threat model brought in line with the system, before F.
5. Roadmap §F, criterion 7: its SPEC decides Part (b) §2–§4 (delegated).
   The ADR amendments go to Manuel for his "sí".
6. §B2 and §B3 stay parked until a SPEC-021 text arrives (roadmap §B2).
