# Handoff — Session 38

Session 38 opened on 2026-10-13 and closed on 2026-10-14. Manuel did not
have SPEC-021's text, so roadmap §B2 still waits on it. The session fixed
debt #63 on its own branch and landed it, then a first sweep of the open
debts that are Claude Code's to fix: #19, #36, #18 and #57. Its work order
for Session 39 is *How Session 39 resumes* at the end.

## State at close

- `main` = `origin/main` = the commit that completes this file (CI ALL
  GREEN, *CI monitoring*); tree clean; no stash.
- Catalogs: ADR 20 / SPEC 20. Known CI debt: zero.
- No branch or PR open. `fix/s38-debt-63-marquee-session` (PR #7) landed as
  five cherry-picked commits, `a9452b0..928cfbf`; `fix/s38-debt-sweep-1`
  (PR #8) as four, `67f2055..6841aa5`. Both PRs are closed and both
  branches deleted, local and on origin.
- Code changed this session. For #63: `services/ingest/test/helpers/marquee-agent.ts`
  and two new test files, `services/ingest/test/marquee-agent-etw.test.ts`
  and `agent/cg-agent/tests/etw_reclaim.rs`. The elevated gate covered it
  (*Gate outputs and baselines*). For the sweep: `services/ingest/src/server.ts`,
  a new `services/ingest/test/server-start-failure.test.ts`,
  `infra/dev/docker-compose.dev.yml`, `infra/dev/README.md` and both test
  backends (`services/*/test/helpers/backends.ts`). No file under
  `agent/cg-agent/src/` changed, and none of the sweep's files is on a gated
  path (*Developer-local SPEC-005 marquee validation* §5, *SPEC-006* §6).
  `server.ts` is still a runtime input under `services/`, so gate 1b
  (at `6fbcd0e`) is not evidence for the tip; the next gated change runs
  the gate again.
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
  - `C:` has 16 GB free (29 GB at the start). The Docker Desktop disk under
    `%LOCALAPPDATA%\Docker` held 101 GB on 2026-10-13 and grows with each
    testcontainers run; `target/` held 8.5 GB.

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
| (this commit) | — | This record of the sweep. |

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

The inventory also found where the auto-memory index differed from the
records. The records are right and stay as written: #1 was resolved in S25
(`docs/handoff-session-26.md:148`); #15 and #16 were closed in S29
(`docs/handoff-session-29.md:116-119`); #54 is recorded in handoff-36's
*Progress* (`:168-182`), not its *Debts*; #3's cited collision is gone (the
two UUIDs now differ), while its risk stays (18 hand-numbered agent IDs in
16 files). Open debts at the close: 48 (the inventory's 50, less the four
closed by the sweep, plus #67 and #68).

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

Baselines at the close: non-elevated cargo 126 passed / 7 ignored;
elevated cargo `--ignored` 7 (S37; not rerun, no agent source changed);
vitest `services/ingest` unelevated 62 files / 207 tests (202 passed / 4
failed with exit 9 / 1 skipped), elevated 61 files / 205 tests at
`6fbcd0e` (gate 1b, before the sweep's two tests; expected 62 / 207 at the
tip, not run); `services/api` 31 files / 62 tests (this session).

## What waits on Manuel

1. SPEC-021's text (handoff-37 *SPEC-021*).
2. #10, widened. The four read routes, `/v1/incidents/:id`, `/events`,
   `/forensic-export` and `/report` (`services/api/src/read/routes.ts`),
   answer 404 to any error, a store that is down included. Recommendation:
   check the `:id`'s UUID form first (404 if malformed), and answer `503
   {"error":"unavailable"}` when a store fails. That amends SPEC-009,
   SPEC-010 (its Open question 3 already points there), SPEC-012 and
   SPEC-013, with tests that take ClickHouse and Postgres down. The
   dashboard already treats any status other than 200, 401 or 404 as an
   error. Asked because it is the error semantics of the public API.
3. #33. NFR-006-001 (`docs/specs/SPEC-006-detection-mvp.md:195`) sets the
   detection interval at 5000 ms; the code (`services/ingest/src/config.ts:74`),
   the compose (`:152`) and `infra/dev/.env.example:64` use 10000.
   Recommendation: bring the code to the contract, 5000 ms. The worst-case
   latency drops from about 20 s to about 15 s, and the load on ClickHouse
   is negligible at the MVP's volume. Alternative: amend the SPEC to 10000.
   Asked because it changes the default of a variable a deployment reads.
   #67 follows the same answer.
4. The Docker Desktop disk: `C:` has 16 GB free. Pruning it
   (`docker system prune`, its volumes) deletes his Docker data; the daemon
   also runs other projects' containers.
5. #64's ADR-0008 half.
6. Unchanged: #59, roadmap Part (b) §9, #55, Part (b) §1–§4.

## How Session 39 resumes

1. *Session protocol* §At the start. Expected: `main` = `origin/main` at
   this file's last commit; tree clean; no stash; no branch or PR open;
   catalogs ADR 20 / SPEC 20; Known CI debt zero; `logman query -ets` lists
   no `CGAgent` session.
2. SPEC-021: when Manuel brings the text, land it verbatim (*Integration
   path* §6, with the SHA-256), self-review it, and set its Status
   `Accepted`. It covers the review items marked "for SPEC-021" in
   handoff-37, #60's second session and #62.
3. If Manuel has answered #10 or #33 (*What waits on Manuel*): the SPEC
   amendments first, self-reviewed, then the code on a branch with tests
   failing first. #67 goes with #33's answer.
4. Meanwhile, the next sweep of Claude Code's debts, each one on a branch
   with a PR, a failing test first where a test applies, CI on every push
   and the self-review. In order of value: #68 (a compose check in CI),
   #6 (the api's copy of the `cges_events` table, before B2 adds columns),
   #30 (the real-ETW tests on the hosted runner), #39 and #40 (CGES
   examples and `time`), #13 (stale citations, swept by pattern), #27 and
   #41 (diagnosis first), #3, #8, #9, #11. #7 may count as a new component,
   which is Manuel's: ask first.
5. Then B2's implementation, on a branch with a PR: acceptance tests
   failing first, CI on every push, the elevated gate as a one-line script
   (*Environment facts*), the self-review, landing by cherry-pick. It
   carries #31, #52, #53, #54, #56, #58, #64's SPEC-005 row and #66.
6. Then roadmap §B3, after Part (b) §9.
