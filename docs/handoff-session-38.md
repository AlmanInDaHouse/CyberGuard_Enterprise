# Handoff — Session 38

Session 38 opened and closed on 2026-10-13. Manuel did not have SPEC-021's
text, so roadmap §B2 still waits on it; the session fixed debt #63 on its
own branch and landed it. Its work order for Session 39 is *How Session 39
resumes* at the end.

## State at close

- `main` = `origin/main` = the commit that adds this file (CI ALL GREEN,
  *CI monitoring*); tree clean; no stash.
- Catalogs: ADR 20 / SPEC 20. Known CI debt: zero.
- No branch or PR open: `fix/s38-debt-63-marquee-session` (PR #7) landed as
  five cherry-picked commits, `a9452b0..928cfbf`; the PR is closed and the
  branch deleted, local and on origin.
- Code changed this session: `services/ingest/test/helpers/marquee-agent.ts`,
  and two new test files, `services/ingest/test/marquee-agent-etw.test.ts`
  and `agent/cg-agent/tests/etw_reclaim.rs`. No file under
  `agent/cg-agent/src/` or `services/*/src/` changed. The elevated gate
  covered it (*Gate outputs and baselines*).
- Outside the repository: Docker Desktop, started by Claude Code and left
  running; the gate script `C:\tmp\s38-gate1b.ps1` and its logs
  `C:\tmp\s38-gate1b-*.log`; the logs of gate 1, which ran nothing in the
  repository (`C:\tmp\s38-gate1-*.log`). No ETW session of the agent's name
  is running. `C:` has 17 GB free (29 GB at the start): the Docker Desktop
  disk under `%LOCALAPPDATA%\Docker` holds 101 GB and grows with each
  testcontainers run; `target/` holds 8.5 GB.

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
| (this commit) | This record; `README.md` and the roadmap point here; CLAUDE.md's environment fact on the gate script and the gate's expected outcome. |

Manuel's decision this session, in chat:

- 2026-10-13, asked whether he had SPEC-021's text, with the
  recommendation to fix #63 meanwhile: "no lo tengo". Claude Code read it
  as the recommended option and said so in the same turn.

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
  CLAUDE.md's *Environment facts* now say so (this commit).

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

Closed this session:

- **#63 — The marquees kill the agent, so the last one leaves its session
  running.** Fixed by `a9452b0..928cfbf`: once a marquee's agent has
  exited, the harness stops the agent's ETW session (Windows). Gate 1b: one
  stop per marquee, no reclaimed session, none left after the run; then,
  unelevated, capture_ac_001 passed.

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

Baselines at the close: non-elevated cargo 126 passed / 7 ignored;
elevated cargo `--ignored` 7 (S37; not rerun, no agent source changed);
vitest `services/ingest` elevated 61 files / 205 tests, unelevated 200
passed / 4 failed (exit 9) / 1 skipped; `services/api` unchanged (31 files
/ 62 tests, S35), not run. CI: every push green — on the branch, `fd5d2ce`
and `2708f9f` `ts-ci → success`, `a011a2e`, `ea120c3` and `6fbcd0e` `ts-ci`
and `rust-ci → success`; on `main`, the landing block above. `rust-ci`'s
Windows job ran `etw_reclaim`: "test
open_reclaims_a_leftover_session_of_the_agents_name ... ok".

## What waits on Manuel

1. SPEC-021's text (handoff-37 *SPEC-021*).
2. The Docker Desktop disk: `C:` has 17 GB free. Pruning it
   (`docker system prune`, its volumes) deletes his Docker data.
3. #64's ADR-0008 half.
4. Unchanged: #59, roadmap Part (b) §9, #55, Part (b) §1–§4.

## How Session 39 resumes

1. *Session protocol* §At the start. Expected: `main` = `origin/main` at
   this file's last commit; tree clean; no stash; no branch or PR open;
   catalogs ADR 20 / SPEC 20; Known CI debt zero; `logman query -ets` lists
   no `CGAgent` session.
2. SPEC-021: when Manuel brings the text, land it verbatim (*Integration
   path* §6, with the SHA-256), self-review it, and set its Status
   `Accepted`. It covers the review items marked "for SPEC-021" in
   handoff-37, #60's second session and #62.
3. Then B2's implementation, on a branch with a PR: acceptance tests
   failing first, CI on every push, the elevated gate as a one-line script
   (*Environment facts*), the self-review, landing by cherry-pick. It
   carries #31, #52, #53, #54, #56, #58, #64's SPEC-005 row and #66.
4. Then roadmap §B3, after Part (b) §9.
