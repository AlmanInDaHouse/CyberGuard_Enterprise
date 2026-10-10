# Handoff — End of Session 34

Full state-of-the-world at the S34 close. Written so a cold or compacted session
recovers the thread in one read.

Session 34 wrote the contract of roadmap **Phase D**'s network half —
**ADR-0018** and **SPEC-019**, Accepted and on `main` as `a5514c2` — and
implemented it on a review branch. The implementation is **not on `main`**: it
is five commits on `review/s34-d-net` (draft PR #4). Manuel ran the elevated
gate on the branch tip `2b0d3c9` at the close and it passed; the advisor reports
that it has not reviewed the branch, and the branch has not landed. The login
half (ADR-0019, SPEC-020) is not written. Catalogs: ADR 18 / SPEC 19. Known CI
debt: ZERO.

The advisor reports that at the close Manuel decided to change the development
methodology: the executor decides and implements, and Manuel keeps the overall
architecture and the questions that need the owner's attention. It lands in
the next commit (S34-06).

S34-06 landed it in `docs(claude-md): executor-led methodology`, the commit
after `a68e085` (Anchor commits): CLAUDE.md, `docs/engineering-notes.md`
§Session 34 and the resume steps below. Session 35 starts under it.

## Anchor commits

On `main`, pushed:

| SHA | What |
|---|---|
| `a5514c2` | `docs(spec-019)`: ADR-0018 and SPEC-019 Accepted; ADR and SPEC catalogs; roadmap §D contract. |
| `a68e085` | handoff-34; README and roadmap pointers; roadmap §D status. |
| (this commit) | `docs(claude-md)`: the executor-led methodology (S34-06). |

`markdown-lint` ran on `a5514c2` and succeeded (GitHub REST API).

**Not on `main`.** The branch `review/s34-d-net` holds five commits on top of
`a5514c2`, each with `NOT YET RATIFIED` in its subject. They are the
implementation of SPEC-019, and the only place it exists:

| SHA | What |
|---|---|
| `cf12719` | `feat(cges)`: `actor` in the 4001 class file; `time` in `event.json` names 4001. |
| `aa0d422` | `feat(ingest)`: the route accepts and stores class 4001; six new columns. |
| `011a456` | `refactor(agent)`: the ring, the batch and the envelope carry both classes. |
| `5742c33` | `feat(agent)`: Kernel-Network in the session; decoding; dispatch. |
| `2b0d3c9` | `test(ingest)`: the network marquee `net_ac_010`. |

Draft PR #4 (`SPEC-019: network telemetry (review, NOT YET RATIFIED)`) is open
on the branch. Each commit was pushed on its own and the PR's CI ran on each,
all green (GitHub REST API): `schema-validation` on the five, `ts-ci` on all but
`cf12719`, `rust-ci` on `011a456`, `5742c33` and `2b0d3c9`. On the tip,
`rust-ci` ran its Linux and Windows jobs and `ts-ci` its ingest, api and
dashboard jobs. `markdown-lint` was not triggered: the branch changes no `.md`
file.

## ADR-0018 and SPEC-019

- **Owner delegation.** Manuel delegated the three owner decisions explicitly
  ("elige tú la mejor opción", 2026-10-10); the advisor decided, and Manuel's
  ratification of SPEC-019 ratified them (its §Ratification record, 1–3): the
  scope of §D — capture, delivery and storage of both classes, a marquee for
  each, no detection rule and no product surface; "basic network" — TCP
  connections opened, outbound and inbound, over IPv4 and IPv6, from
  Kernel-Network; the vehicle — one per-class ADR and one SPEC for each class,
  network first. Decisions 4–12 are the advisor's, in the reversible lane.
- **Where it started.** The advisor reports that S34 continued in the advisor
  chat of S33 and opened with S34-01, a read-only audit of phase D: hypotheses
  D1 to D14, and a section E with the metadata of the Windows providers, read
  without elevation. ADR-0018 §Context 6 comes from it.
- **What the contract fixes** (ADR-0018 §1–§9; SPEC-019 §Data contracts and
  §Operational):
  - Source: `Microsoft-Windows-Kernel-Network` as a second provider of the
    agent's one ETW session, keywords `0x10` and `0x20`, filtered by event id to
    12, 15, 28 and 31. The callback checks the id too and counts discards.
  - Element: class 4001, `activity_id` 1 (Open); the direction from the event
    id; `src_endpoint` the initiator and `dst_endpoint` the acceptor in both
    directions; an IPv4-mapped IPv6 address as IPv4; `actor.process.pid`
    always, `actor.process.uid` only from the creation-time cache, omitted
    otherwise; `time` as string-encoded Unix nanoseconds; no raw payload.
  - A record of the agent's own PID is discarded at dispatch.
  - Storage: rows of `cges_events` (D6 extended), six new columns with
    defaults. A reader that interprets a row's `activity_id` or class-specific
    columns MUST select by `class_uid`; the forensic drill is the named
    exception while every alert cites process events.
  - The server is upgraded before the agents.
- **What it leaves to the elevated gate.** ADR-0018 §Context 7 names five facts
  no run had measured: the byte order of ports and addresses; which of `saddr`
  and `daddr` is the local endpoint of an accepted connection; whether `PID` is
  the connecting process on 12 and 28 and the accepting one on 15 and 31;
  whether ETW honours the filter by event id; whether event 12 is written for an
  attempt that fails. §10 settles them by outcome in the gate (`net_ac_008` to
  `net_ac_010`): a contradiction of a statement of the ADR amends it before the
  code lands, and the failed-attempt fact is recorded, not asserted.

## The spike that was not done

The advisor reports: it wrote two prompts, S34-02 and S34-02b, to measure the
Kernel-Network provider and facts of the logon events on Manuel's machine, with
a throwaway crate outside the repo. The executor's safeguards stopped both
before anything was built; the executor stopped and did not reformulate them.
The advisor withdrew the spike, and with it the idea of the executor launching
elevated commands through UAC. Nothing was built or measured. What the spike
would have measured became acceptance criteria of the elevated gate (SPEC-019
§Ratification record, decision 11).

## What the document review surfaced

The advisor reports three versions of the docs patch; the design did not change
between them. The SHAs below belong to review branches already deleted and are
not on origin: they are the advisor's account.

1. **v1 (`2d67b7e`) — three blocking entries.** "Basic network" attributed to
   the Blueprint's MVP paragraph rather than to its acceptance criteria;
   ADR-0018 obliged every reader of `cges_events` to filter by class while
   SPEC-019 left the forensic drill without a filter; and the ring's FIFO drop
   attributed to NFR-005-002 rather than to ADR-0009 §3.
2. **v2 (`b79d9b6`) — one.** The rewritten rule ("selects rows by anything
   other than their `event_id`") still reached the drill, which also filters by
   `org_id`.
3. **v3 (`7fcf669`) — ratified**, squashed to `main` as `a5514c2`. The rule is
   stated by what the reader does with the row, and the drill is an exception
   named inside the rule.

The lesson: a rule defined by the shape of a query leaves gaps; define it by
what the reader does, and name the exception inside the rule.

The review's blocking rule changed too: list A blocks (what the repo
contradicts, or two statements of the patch that clash); list B does not
(wording, what the repo does not let one check, implementation difficulties).

## The implementation on the review branch

What each commit does, read from its message and diff:

- **`cf12719` — schemas.** `classes/4001_network_activity.json` gains the
  `actor` property of the 1007 class file; the description of `time` in
  `event.json` names 4001 beside 1007. The examples are unchanged.
- **`aa0d422` — server.** An `events[]` element is a union discriminated by
  `class_uid`; the 4001 shape checks every member of SPEC-019 §Data contracts
  (`ip` with `z.string().ip()`, `port` 0–65535). The route writes one row per
  element in the POST's one `INSERT`. The bootstrap adds the six columns with an
  idempotent `ALTER TABLE … ADD COLUMN IF NOT EXISTS`, and the api's test mirror
  of the table runs the same `ALTER`. `getCgesEvents` reads class 1007 only;
  `getNetworkEvents` and `insertNetworkEvents` are new. Tests: `net_ac_001` to
  `net_ac_004`.
- **`011a456` — agent types.** `RingEvent` is a process event or a
  `NetworkEvent`; the ring holds it and applies the empty-image-name rule to
  process events only; `cges/emit.rs` gains the 4001 wire shape and an untagged
  union of the two; the envelope and the delivery loop render each event in its
  class's shape. Nothing produces a network event yet.
- **`5742c33` — capture.** `session.rs` enables Kernel-Network with a filter by
  event id; the callback checks the id and hands the raw bytes of `PID`,
  `saddr`, `daddr`, `sport` and `dport` to `dispatch_network_record`
  (`dispatch.rs`), which decodes, drops the excluded PID uncounted, converts the
  timestamp, looks the creation time up without purging it, and enqueues.
  `EtwSession::open` excludes no PID; `open_excluding` takes one, and the agent
  passes its own (`lib.rs`). The session counts discarded network records and
  keeps the first; the hygiene pass logs an increase at `warn`. Tests:
  `net_ac_005` to `net_ac_007` in CI; `net_ac_008` and `net_ac_009` on real
  ETW, `#[ignore]`d, with `curl.exe` as the probe (`tests/net_probe/mod.rs`).
- **`2b0d3c9` — the marquee.** `net_ac_010`; the marquee helper's `run()` also
  returns the agent's PID.

**The three hypotheses.** No run on real ETW preceded the code. Each reading of
the payload lives in one function of `agent/cg-agent/src/etw/network.rs`,
marked `HYPOTHESIS`:

- **H1** — `port`: `sport` and `dport` hold the port big-endian.
- **H2** — `address`: `saddr` and `daddr` hold the address's bytes in network
  order, 4 for IPv4 and 16 for IPv6.
- **H3** — `local_and_remote`: in all four events `saddr`/`sport` is the host's
  local endpoint and `daddr`/`dport` the remote one.

Correcting one is changing that function and its vectors in `net_ac_005`. The
`PID` and the filter are not coded as hypotheses: `decode_pid` reads a
little-endian `u32`, and the callback checks the id whatever the filter does.

**What the elevated gate covers.** `net_ac_008` and `net_ac_009` on real ETW;
the marquee `net_ac_010`; and, for `net_ac_011`, the four existing real-ETW
tests and the existing marquees, now with both providers in the session.

**Review.** The advisor reports that it did not review the branch; it read only
`agent/cg-agent/src/etw/network.rs` at `2b0d3c9`.

## The elevated gate (tip `2b0d3c9`, Manuel, 2026-10-10)

Manuel ran the gate at the close, with commands Claude Code gave, and pasted the
output in chat. Claude Code also read the two logs Manuel saved outside the repo
(`C:\tmp\s34-gate-cargo.log`, `C:\tmp\s34-gate-vitest.log`).

- `net session` answered `No hay entradas en la lista.`: elevated. HEAD
  `2b0d3c9`; `git status --short` printed nothing.
- `cargo build --release -j 2 -p cg-agent` compiled nothing (`Finished` in
  0.45 s). `cg-agent.exe` was written at 15:32:06, before `5742c33` was
  committed (15:41); Claude Code checked that no file under `agent/` is newer
  than the binary (the last edit is at 15:28:10). The marquees ran the branch's
  agent.
- `cargo test -j 2 -p cg-agent -- --ignored --test-threads=1 --show-output`:
  6 passed, 0 failed — `capture_ac_006` (elevated shutdown), `process_ac_004`,
  `process_ac_007`, `process_ac_009`, `net_ac_008` (8.26 s) and `net_ac_009`
  (3.04 s). `net_ac_008` printed `net_ac_008 refused_attempt_reported=false`.
- `pnpm test` in `services/ingest`: **54 files / 173 tests** passed, nothing
  skipped; `ac-001-marquee` ran. `detect_ac_001` passed in 46 547 ms and logged
  both image paths in Win32 form; the SPEC-005 marquee logged
  `marquee_elapsed_seconds` 40.056 against a budget of 45; `net_ac_010` logged
  25.086 against 45, with 69 network rows for its agent.

### What the output says about the open facts

Claude Code's reading; the advisor has not reviewed it.

- **Byte order (H1, H2).** The tests compare the decoded addresses and ports
  with the real ones, exactly: `net_ac_008` over IPv4 (`127.0.0.1`) and IPv6
  (`::1`), `net_ac_009` and `net_ac_010` over IPv4. A byte-swapped port or
  address would not match. Consistent with H1 and H2, on loopback.
- **The local endpoint (H3).** Event 12: `net_ac_008` over IPv4, `net_ac_009`,
  `net_ac_010`. Event 28: `net_ac_008` over IPv6. Event 15: the inbound event of
  `net_ac_009` and `net_ac_010`, whose destination is the listener. Consistent
  with H3 for those three. **No test exercises event 31** (IPv6, accepted): in
  `net_ac_008` the IPv6 listener lives in the test process, whose PID the agent
  excludes, and `net_ac_009` and `net_ac_010` use IPv4 only. H3 for event 31 is
  unmeasured.
- **`PID`.** The probe's on events 12 and 28; the listening process's on
  event 15. Unmeasured on event 31, for the same reason.
- **The filter by event id.** `net_ac_009` sent a 1 MiB body and counted 0
  discarded network records: the filter is honoured. The reopen condition of
  SPEC-019 §Open questions 1 is not met.
- **A failed attempt.** `refused_attempt_reported=false`: one attempt to a
  loopback port with no listener produced no 4001 element. One run, on
  loopback, for a refused connection; a connection that gets no answer was not
  tried. It is the first answer to SPEC-019 §Open questions 4, and it
  contradicts no statement of ADR-0018, which claims only "attempted" (§2,
  §10).
- On this reading, no statement of ADR-0018 is contradicted.

## Test baselines

All on Windows at `2b0d3c9`, the review tip. `a5514c2` changes documents only,
so `main`'s baselines are those of handoff-33.

- **Elevated** (Manuel, above): cargo `--ignored` 6 passed; vitest
  `services/ingest` 54 files / 173 tests, nothing skipped.
- **Unelevated** (Claude Code, this session; the S34-04 numbers were not in its
  context, so it ran the suites once):
  - `cargo test --all`: 57 targets (56 test binaries and the doc-tests),
    100 passed, 0 failed, 6 ignored — the six real-ETW tests.
  - vitest `services/ingest`: 54 files — 49 passed, 1 skipped
    (`ac-001-marquee`), 4 failed; 173 tests — 168 passed, 1 skipped, 4 failed.
    Three failures are the capture marquees (SPEC-005, `detect_ac_001`,
    `net_ac_010`): the agent exits with code 9. The fourth is debt #41.
  - vitest `services/api`: 31 files / 62 tests passed.
- **CI** at `2b0d3c9`: green (above). No Linux cargo run was made locally;
  `rust-ci`'s Linux job is green.

## Notes for the record

- **"Class-specific columns" are not enumerated.** ADR-0018 §9 and §Compliance
  oblige a reader that interprets a row's `activity_id` "or its class-specific
  columns" to select by `class_uid`; neither ADR-0018 nor SPEC-019 lists which
  columns those are. SPEC-019 §Data contracts lists the six new columns and the
  shared columns a 4001 row sets.
- **`z.string().ip()` accepts a zone id.** With zod 3.25.76 (`services/ingest`),
  `fe80::1%eth0` passes the 4001 `ip` check of `services/ingest/src/schemas.ts`
  at `2b0d3c9`. The agent renders `ip` from an `IpAddr`
  (`agent/cg-agent/src/cges/emit.rs`), which carries no zone, so it emits none.
- **A PowerShell 5.1 artifact in the gate's output.** The `NativeCommandError`
  block around cargo's `Finished` line is Windows PowerShell wrapping cargo's
  stderr, not a failure; its `Tee-Object` writes the logs in UTF-16.
- **The agent's POSTs appear not to reuse a connection** (deduced, not
  investigated). In `net_ac_010`'s log, the test process, which hosts the
  ingest server, has an inbound 4001 row about once a second to one port, each
  from a new source port on `127.0.0.1`; the marquee's agent heartbeats every
  second. They are the server's end of the agent's connections, which SPEC-019
  §Operational §4 reports.

## Owner-STOP decisions pending (waiting on Manuel)

Unchanged from handoff-33, whose list is handoff-31's minus what handoff-32
resolved: ADR-0002 Go→TS reconciliation; the criterion-7 deployment contract;
forensic trust anchoring; B2 capture source; the compose basename
collision #12; amending SPEC-004 with `INGEST_DETECT_*` (#17); and the
optional items of handoff-29. D's three owner decisions were delegated and are resolved in
SPEC-019 §Ratification record. New:

- **Landing `review/s34-d-net`:** its ratification (relay rule 5).
- **ADR-0019 (logins):** a new ADR.
- **What an outbound Open asserts** (SPEC-019 §Open questions 4): if the
  answer moves ADR-0018 §2 away from "attempted", that amends an Accepted ADR.

## Debts

- **#1–#14, #17–#21, #24, #27–#33, #35–#37:** unchanged — see
  [handoff-session-26.md](handoff-session-26.md) to
  [handoff-session-33.md](handoff-session-33.md).
- **#25 — Clients per call in the detection cycle**
  ([handoff-session-31.md](handoff-session-31.md)), restated with SPEC-018's
  names: `upsertAlert`, `upsertIncident`, `getCursor` and `advanceCursor` open a
  `pg.Pool` per call — the cursor helpers are what handoff-31 calls "the
  watermark helpers" — and `readNewEvents` a ClickHouse client per batch,
  although the driver amendment (ADR-0012 Amendment 2026-06-07, §Decision) says
  the driver shares the service's clients. The cost grows with the number of
  matches.
- **#26 — Incident MITRE and escalation**, now including #34. An incident's
  `cg_mitre` keeps the techniques of the first alert processed — the upsert's
  `DO UPDATE` leaves it out (`services/ingest/src/detect/incidents.ts`) — and
  with SPEC-018 the first alert processed is the first to arrive. The PDF shows
  the techniques partially when later alerts share the tactic set but not the
  techniques; a severity raise sends no email. Destination: roadmap §E.
- **#34 — retired**, folded into #26: it restated #26's MITRE half.
- **#38 — The forensic evidence unit carries no `class_uid`.** The drill's
  `TimelineEvent` (`services/api/src/read/types.ts`), the unit the hash chain
  canonicalizes and seals (`services/api/src/forensic/canonical.ts`), holds
  process columns and no `class_uid`, and no file under `services/api/src`
  names `class_uid`. A sealed row does not say which class it is. It is a
  precondition of any rule that cites an event other than a process event
  (ADR-0018 §9, SPEC-019 §Out of scope).
- **#39 — `time` in the CGES examples and the schema README.** The eight
  examples under `schemas/cges/v0.1/examples/` give `time` in ISO 8601, the
  1007 and 4001 ones included, while `event.json` describes string-encoded
  nanoseconds for 1007 and 4001; they are not wire elements (SPEC-019 §Out of
  scope). `schemas/cges/v0.1/README.md` §Conventions says timestamps use
  `format: date-time` (ISO 8601 with milliseconds); `event.json`'s `time`
  declares no format.
- **#40 — `event.json`'s `time` has no minimum length.** Its description ends
  "this root schema accepts any non-empty string", but the property is
  `"type": "string"` alone, so an empty string validates. `cf12719` changes
  only the description.
- **#41 — `read-model.test.ts` flake.** In Claude Code's unelevated run of the
  ingest suite at `2b0d3c9`, "no FINAL: a resent row arrives after the cursor
  and is read again" failed once: its second read returned no row. It passed in
  three isolated reruns, in the elevated gate and in `ts-ci`; the branch touches
  neither the test nor `services/ingest/src/detect/`. Cause not determined.

Known CI debt: ZERO rows.

## How Session 35 resumes

Session 35 runs under the rules S34-06 wrote into CLAUDE.md (*Decision
authority*, *Session protocol*): Claude Code decides and lands what is not on
Manuel's list; Manuel runs the elevated gate and decides his list.

1. Run CLAUDE.md *Session protocol*, §At the start. Besides `main`, confirm
   `origin/review/s34-d-net` at `2b0d3c9` and draft PR #4 open.
2. **Land `review/s34-d-net`.** Under the new rules landing it is Claude
   Code's: the owner-STOP above, "Landing `review/s34-d-net`: its ratification
   (relay rule 5)", lapses with relay rule 5. The branch was written before
   the rules: SPEC-019 on `main` meets *Compensating controls* §1, and its
   check that each test fails before the code does not apply after the fact.
   What landing needs (CLAUDE.md *Compensating controls*, *Integration path*):
   - The self-review against SPEC-019's acceptance criteria, both halves. It
     reads the gate's output above against H1, H2 and H3 and the
     `refused_attempt_reported` line — the reading above is Claude Code's and
     leaves event 31 unmeasured — and says whether a criterion needs event 31.
     If one does, a test and a new gate run follow.
   - Fixes as new commits. A commit that touches `agent/` or `services/`, the
     `HYPOTHESIS` comments of `network.rs` included, voids the run on
     `2b0d3c9`, and Manuel runs the gate again.
   - If the gate contradicts a statement of ADR-0018, the amendment is
     Manuel's (an accepted ADR) and lands before the code (ADR-0018 §10).
   - The branch is cut from `a5514c2`; `main` has two docs-only commits since.
     Rebase it onto `main` and push the rebased commits one per push, CI on
     each: the run on `2b0d3c9` stays valid if the non-elevated suite count is
     unchanged (CLAUDE.md, *Developer-local SPEC-006 marquee validation*).
     Then land by cherry-pick with the tree guard, the `NOT YET RATIFIED`
     marker stripped from each subject; close PR #4, delete the branch, and
     update roadmap §D and the counts in CLAUDE.md's *Developer-local*
     sections (four real-ETW tests become six).
3. Then ADR-0019 and SPEC-020 (logins). Nothing is written. ADR-0019 is a new
   ADR, and both decide which logon data about people `cg-agent` collects,
   which is on Manuel's list (CLAUDE.md *Decision authority*): Manuel ratifies
   ADR-0019, and SPEC-020's fields need his OK too (owner review, *Integration
   path* §4). The advisor reported that the planned source is the Windows
   Security log (events 4624 and 4625), which is not ETW, and that its facts
   are still to be gathered on Manuel's machine with Windows tools: access, the
   shape of the fields, volume, audit policy. Reading the Security log and the
   audit policy needs an elevated terminal, so Manuel runs those commands. In
   any report of logon data, the user appears as `<user>`, the host or domain
   as `<host>`, and of each SID only the prefix and the RID; raw files do not
   leave the machine.
4. After D: **B2**, **E** and **F**, in the roadmap's order, which is Manuel's.
   DNS is the first network follow-up and is not a phase yet.
