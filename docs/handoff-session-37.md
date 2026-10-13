# Handoff — Session 37

Session 37 opened on 2026-10-12 and closed on 2026-10-13. This record was
started early because ADR-0020's owner-review branch cites its
measurement. ADR-0020 is Accepted, gate 2 measured what SPEC-021 needs, and
debt #60 is fixed; SPEC-021 is not written (*SPEC-021*, below). Its work
order for Session 38 is *How Session 38 resumes* at the end.

## State at close

- `main` = `origin/main` = the commit that adds this section (CI ALL
  GREEN, *CI monitoring*); tree clean; no stash.
- Catalogs: ADR 20 / SPEC 20. Known CI debt: zero.
- No branch or PR open: `fix/s37-debt-60-failed-start` (PR #6) landed as
  `abde0a5`; the PR is closed and the branch deleted, local and on origin.
- Code changed this session: debt #60's fix in `agent/cg-agent/src/etw/`
  and capture_ac_001's test. The elevated gate covered it (*Gate outputs
  and baselines*).
- Outside the repository: the probe crate `C:\tmp\s36-b2-probe` (with
  `s37-oq5-probe4.rs` and `s37-spec021-probe5.rs`, and `windows-sys` 0.59
  in its `Cargo.toml`), its aggregates `C:\tmp\s37-*.txt` and the gate logs
  `C:\tmp\s37-gate*.log`. On Manuel's machine, a session
  `CGAgent-KernelProcess` left by gate 3b's last marquee (#63), until he
  stops it elevated (`logman stop CGAgent-KernelProcess -ets`), the agent
  starts again, or the host restarts.

## Start

- `main` = `origin/main` = `9826767`, tree clean, no stash; CI of the tip
  `markdown-lint → success`, the only workflow triggered.
- One branch, `adr/s36-0020-cmdline-user` at `b751bdc` (local and origin),
  five commits `NOT YET RATIFIED` on `bdd1d74`, no PR; no PR open.
- Catalogs ADR 19 / SPEC 20; Known CI debt zero; debts up to #57.
- `C:` 37 GB free. Docker Desktop was not running (its engine pipe absent,
  not hung); it is not needed before the implementation, and is left off
  until then.

## Progress

Anchor commits on `main`, one per push, CI green on each:

| SHA | What |
|---|---|
| `798e750` | This record started: the measurement of ADR-0020 Open questions 5, the first fresh-context pass and its dispositions, debts #58 and #59. |
| `71a5930` | ADR-0020 Accepted with its six co-located amendments: the squash of the owner-review branch `adr/s36-0020-cmdline-user` at `7b65337`. |
| `f3b9b05` | This record: the second pass, the ratification and the landing. Roadmap §B2 states the decided source and its status, Part (b) §6 is resolved, and §F no longer waits on B2 option (2). |
| `b15656f` | This record: gate 2, the measurement for SPEC-021; side effects; debts #60 and #61; #43 closed. |
| `009b103` | SPEC-017 Amendment 2026-10-13: a failed start leaves no session (debt #60). |
| `abde0a5` | Debt #60 fixed: the squash of `fix/s37-debt-60-failed-start` at `cfe70db` (PR #6), guard `git diff --cached --quiet` → exit 0. |
| (this commit) | This record's close; SPEC-017's amendment brought to the landed code; `README.md` and the roadmap point here. |

Manuel's decisions this session, in chat:

- 2026-10-12, the fresh-context pass over `fec2ee4..b751bdc`: "Sí, lánzala
  (Recomendado)".
- 2026-10-12, ADR-0020 Open questions 5 before his ratification: "Mídela
  primero (Recomendado)".
- 2026-10-12, after the measurement below, on the builds the agent runs on:
  "Incluir Win10 y Server 2019 (Recomendado)" — the agent covers Windows 10
  and Windows Server 2019, so ADR-0020 keeps the second session (its A13).
- 2026-10-13, ADR-0020 and its co-located amendments: "sí a ADR-0020 y a
  sus seis enmiendas". It answered a message that asked for the "sí" in
  seven numbered points — ADR-0020, then the amendments to ADR-0006 (named
  as a standing rule of versioning while CGES is 0.x), ADR-0009, ADR-0011,
  ADR-0012, ADR-0019 and the threat model — and said that a one-phrase
  answer would not be read as a ratification. His answer names the ADR and
  the six amendments of that list, so Claude Code read it as the
  ratification of all seven points.
- 2026-10-13, the closed list of secret patterns P1–P6 that ADR-0020 §2
  leaves to SPEC-021, proposed in chat as Claude Code's full
  recommendation: "si". The list enters the repository with SPEC-021.
- 2026-10-13, after the first stop of SPEC-021's drafting (*SPEC-021*):
  "escribelo tu".
- 2026-10-13, after the second, debt #60 first and then the close: "sí,
  arranca con #60 y el cierre".

The landing of ADR-0020 (handoff-36, *How Session 37 resumes* step 3):

- No rebase: the branch's merge base was `main`'s tip `798e750`, so the
  push with `--force-with-lease` was not needed.
- The ratification commit on the branch, `7b65337`, pushed with no PR open
  (no workflow runs on a branch without one): ADR-0020's Status
  `Accepted` and `Last updated` 2026-10-13; the first entry of its Decision
  record, Manuel's words above; its catalog row `Accepted`; the five
  amendments headed "(ADR-0020)" and the threat model's take the date
  2026-10-13 in their headings, in each reference to them (ADR-0012's
  amendment where it cites ADR-0011's; ADR-0020 §1, §Consequences and
  §Decision record; the threat model's References, the two CGES READMEs, the
  catalog's rows) and in `Last updated` of ADR-0006, 0011, 0012 and 0019.
  ADR-0009 and the threat model have no `Last updated` and get none.
  ADR-0012's and ADR-0019's Amendments 2026-10-11 and SPEC-017's keep their
  dates.
- `git merge --squash` onto `main`, guard `git diff --cached --quiet
  adr/s36-0020-cmdline-user` → `guard exit: 0`; committed as `71a5930`,
  without the marker. CI of `71a5930`:

  ```text
  schema-validation -> success
  markdown-lint -> success
  ```

- The branch deleted, local (`git branch -D`, it was `7b65337`) and on
  origin. No branch or PR is open.

ADR-0020 Open questions 5, measured on Manuel's machine (Windows 11 Home
10.0.26200), elevated (`net session` answered "No hay entradas en la
lista."), at `9826767`, on 2026-10-12. The probe
`C:\tmp\s36-b2-probe\src\bin\s37-oq5-probe4.rs` (ferrisetw `=1.2.0`, built
into the repo's `target/release`) wrote only aggregates to
`C:\tmp\s37-oq5-probe4.txt`; the logs are `C:\tmp\s37-gate1-*.log`. One
`UserTrace` in system-logger mode (`LogFileMode 0x12000100`) with
Kernel-Process (0x10), Kernel-Network (0x30), the system process provider
(`SystemProcessProviderGuid`, `SYSTEM_PROCESS_KW_GENERAL`) and a listener
for the process class's GUID, for 45 s:

- It started with the listener and stopped cleanly; no session of its name
  was left (`logman query` did not find it). Of 2003 records that reached
  ferrisetw's dispatch, 1997 reached a callback; no schema error.
- By provider id: Kernel-Network 1312; Kernel-Process event 1 (v4) 67 and
  event 2 65; the process class's GUID (`3d6fa8d0-…`) opcode 1 (v4) 67,
  opcode 2 (v4) 65, opcode 3 (v4) 356, opcode 11 (v2) 65. None carried the
  system provider's GUID, as Microsoft documents.
- The kernel's process records, opcodes 1 to 4, 488: `CommandLine`,
  `UserSID`, `ImageFileName` and `ParentId` read in 488 of 488. Start: 67,
  all with a command line (p50 74, max 1024 characters), no backslash in
  `ImageFileName`, SIDs `S-1-5-18` 1 and `S-1-5-21-...-1001` 66.
- Probes: 20 started, 20 seen at Start with their marker and as a Launch;
  their SID `S-1-5-21-...-1001`, the probe's own. A probe of about 3020
  characters arrived with 1024, at Start and End.
- Launches: 67, all with the device path.
- Pairs by PID: 67, the kernel's name equal to the base name of the
  Launch's path in all 67; the Launch first in all 67 and its kernel
  record the next record delivered in all 67; no other process start
  between; timestamps equal (0.00 ms); arrival 0.01 ms apart at the
  median, 0.03 ms at p90, 0.71 ms at most. (The probe's line "records
  between the two: p50=1.00" prints the distance in the stream, 1 for
  adjacent records.)
- Delivery against timestamps: 0 of 1997 records, and 0 of 685 process
  records, arrived after a record with a later timestamp.
- Delivery latency: p50 0.50 s, p90 0.86 s, p99 1.0 s, for both sources.

Microsoft Learn, *System Providers*: "Beginning in Windows 10 SDK build
20348, the System Trace Provider's events can be enabled … with
EnableTraceEx2", and "the outputted events are not marked as being emitted
from the individual system providers". *Configuring and Starting a
SystemTraceProvider Session*: from Windows 8 and Windows Server 2012 the
kernel's events go to at most eight logger sessions, the first two slots
reserved for the NT Kernel Logger and the Circular Kernel Context Logger.

Gate 2, the measurement for SPEC-021, on Manuel's machine, on 2026-10-13.
The probe `C:\tmp\s36-b2-probe\src\bin\s37-spec021-probe5.rs` (ferrisetw
`=1.2.0` and, for `ControlTraceW`, `windows-sys` 0.59 as the agent uses it;
built into the repo's `target/release`) opens ADR-0020's two sessions in its
order — `CgS37P5User`, a `UserTrace` with ferrisetw's default properties
holding Kernel-Process (0x10) and Kernel-Network (0x30), then
`CgS37P5Kernel`, a `KernelTrace` with the process flag — and writes only
aggregates to `C:\tmp\s37-spec021-probe5.txt`. Logs
`C:\tmp\s37-gate2-*.log`: `net session` → "No hay entradas en la lista.";
`git rev-parse --short HEAD` → `f3b9b05`; the probe → "written
C:\tmp\s37-spec021-probe5.txt". The pasted block ran in reverse order, so
the two `logman query` lines ran before the probe: `CgS37P5User` was
running (a side effect of Claude Code's, below) and `CgS37P5Kernel` was
not found. After the run, `logman query -ets`, which lists every session
unelevated, showed no `CgS37*` session.

- The side channel by name (ADR-0008's helper, ADR-0020 §1): `QUERY` by
  name returned 0 on both sessions; the kernel session's `LogFileMode`
  `0x12400100` (system logger), `EnableFlags` `0x00000001`; the user
  session's `0x10400100`. `STOP` of the kernel session by name returned 0
  and a `QUERY` after it 4201; ferrisetw's own stop after that failed with
  the same instance-not-found error. A second open of the kernel session's
  name while it ran: `EtwNativeError(AlreadyExist)`.
- Lost: `EventsLost`, `RealTimeBuffersLost` and `LogBuffersLost` 0 on both
  sessions before the stop, after two bursts of 200 processes, each spawned
  in about 1.36 s.
- The join: 425 kernel starts and 425 Launches, all 425 matched by PID and
  timestamp, none unmatched. Timestamps equal (kernel minus Launch: max
  0.01 ms). Arrival, kernel minus Launch: min −357.92, p50 −35.64, p90
  703.38, max 965.33 ms; a bound of 0.5 s would separate 101 pairs, of 1 s
  none. In a replay of the two arrival streams, at most 62 kernel starts and
  52 Launches waited at once. 28 PIDs were seen at more than one kernel
  start within the run.
- The limit's unit is the UTF-16 code unit: 3016 ASCII characters arrived
  as 1024; 1517 'é' as 1024 characters (1990 UTF-8 bytes); 816 characters
  of U+1F600 as 541 characters, 1024 UTF-16 units, ending in U+FFFD — the
  cut split a surrogate pair, and ferrisetw's string conversion replaced
  the half.
- Order within each session: 0 of 1986 records of the kernel session, and
  0 of 1349 of the user session (0 of 849 Kernel-Process), arrived after a
  record of the same session with a later timestamp.
- The kernel session by opcode: 1 425, 2 424, 3 352 (the rundown at the
  open), 4 353 (the rundown at the stop), 11 424, 39 8.
- Starts' command lines: p50 61 characters, 68 bytes once JSON-escaped;
  the most, 1998 bytes (the U+1F600 probe).

Microsoft Learn, *Logging Mode Constants*, on
`EVENT_TRACE_NO_PER_PROCESSOR_BUFFERING` (0x10000000, in both sessions'
`LogFileMode`): "Writes events that were logged on different processors to
a common buffer. Using this mode can eliminate the issue of events
appearing out of order …", and "If you expect a high volume of events (for
example, more than 1,000 events per second), you should not use this mode."
Neither session sets `EVENT_TRACE_ADDTO_TRIAGE_DUMP` (0x80000000), "adds
ETW buffers to triage dumps".

Side effects of Claude Code's, outside the repository:

- Before handing gate 2 over, Claude Code ran the probe unelevated to check
  that it fails cleanly (it printed "Acceso denegado."). It left
  `CgS37P5User` running, with no provider, until the elevated run's first
  `ControlTraceW` `STOP` ended it. Reproduced with `logman`, unelevated:
  `logman start CgS37UnelevTest -ets -rt` succeeded, `logman update … -p
  {22fb2cd6-…} 0x10 -ets` answered "Acceso denegado.", `logman stop`
  succeeded. ferrisetw 1.2's `TraceBuilder::start` returns at the failed
  `EnableTraceEx2` without stopping the session it started.
- To check the agent for the same, Claude Code ran the test binary
  `capture_ac_001_startup_failure-53e7d88ef5f60ced.exe` (built 2026-10-11
  02:32; `agent/cg-agent/src/etw/session.rs` unchanged since, `types.rs`
  changed) with `capture_ac_001_open_returns_the_start_result`, unelevated:
  it passed, and `CGAgent-KernelProcess` was left running. Stopped with
  `logman stop CGAgent-KernelProcess -ets`; debt #60.
- The red run of #60's tests, unelevated, before the fix, left
  `CGAgent-KernelProcess` running; Claude Code stopped it with `logman stop
  CGAgent-KernelProcess -ets` ("El comando se completó correctamente.")
  before the green run, after which `logman query -ets` listed no
  `CGAgent` session.
- After gate 3b, `logman query -ets` listed `CGAgent-KernelProcess` (#63).
  Claude Code's unelevated `logman stop` of it answered "Acceso denegado.":
  an elevated agent had started it. It waits on Manuel (*State at close*).

## SPEC-021

Claude Code's drafting of SPEC-021 was stopped twice on 2026-10-13 by the
platform's safety classifier, with the instruction not to produce that
content again, reworded or not. The first stop left an uncommitted partial
file (the header and part of §Context), which was deleted; the second left
nothing. Manuel asked Claude Code to bypass the instruction; Claude Code
declined: it comes from the platform, outside this repository's decision
authority, and no order inside a session lifts it. Nothing of SPEC-021 is
in the repository. What it needs is here: ADR-0020 (Accepted), gate 2's
measurements (*Progress*), Manuel's "si" to P1–P6, the review items marked
"for SPEC-021" (*Self-reviews*), #60's second session and #62 (*Debts*).
The path offered to Manuel: he writes SPEC-021, or has it written, and
Claude Code lands it verbatim (*Integration path* §6), reviews it against
the repository and implements it.

## Self-reviews

- **ADR-0020's branch at `b751bdc`**, the fixed SPEC/ADR prompt over the
  threat model, ADR-0006, 0009, 0011, 0012, 0019, 0020 and the catalog.
  List A 3, all fixed on the branch by its commit "fourth review fixes":
  - A1, `regex` is not in the agent's shipped tree: `cargo tree -p cg-agent
    -e normal -i regex` prints nothing; `regex` comes only through
    `testcontainers` (a dev-dependency); `regex-automata` and
    `regex-syntax` come through `tracing-subscriber`. ADR-0020 §2
    corrected. handoff-36's disposition "`regex` is transitive" rested on
    the same misreading; it is a record and stays.
  - A2, "Events are not ordered across CPUs (debt #43)" stated as fact:
    reworded to the measurement above, the premise in doubt, SPEC-021
    settles it.
  - A3, the two CGES READMEs state the semver rule that ADR-0006's
    amendment narrows, and the amendment did not cover additive changes or
    a change of meaning: the amendment now covers both and the READMEs are
    restated on the branch.

  List B 18, dispositioned:
  - fixed: B1, the join's purge runs whether or not a batch is formed or
    delivered; B2, the rendered batch's lifetime stated in §2; B3,
    "capacity" for the table's count, "bound" for time; B4, the median's
    sign; B5, the probes named; B8, grammar; B9, ADR-0015 cited for what
    it says (read-only in use) and the missing grant stated apart; B10,
    the catalog's "no new raw ETW call" removed; B11, the markers' names
    explained (OCSF's `cmd_line`); B14, SPEC-017 §Data contracts and
    §Operational §1 and §4 in the Neutral list; B15, the rundown sentence
    as a choice; B17, the redacted marker's residue stated (it stays with
    its row, debt #47); B18's first item, the system-logger limit, now
    cited from Microsoft's documentation;
  - at the "sí": B6, ADR-0006's amendment is a standing policy and on
    Manuel's list, so his answer must name it; B7, the amendment dates
    take the ratification date at landing, and ADR-0009, whose header has
    no `Last updated`, gets none;
  - for SPEC-021: B16, the relaxed `user.json` keeps the non-empty rule
    inside the alternative; B18's unchecked items (`ControlTraceW` on a
    system-logger kernel session, LocalSystem);
  - for the implementation branch, the code and rule texts that still call
    the fields empty: `services/ingest/src/db/migrate.ts:207`,
    `services/ingest/src/detect/read-model.ts:114-115`,
    `services/api/src/read/types.ts:52-54`,
    `rules/windows/office_spawns_script_host.yml:8-9`;
  - no change: B12 (`process.user.uid` is new on the 1007 wire, so the
    rule for a new field applies), B13 (the drill does not project the
    marker).

  A side effect of the review: its `cargo tree` without `--offline`
  downloaded crates into the cargo registry cache under `~/.cargo`, outside
  the repository.

- **ADR-0020's branch at `06cc8a7`**, a second pass with the same prompt and
  documents, after the fixes above (the reviewer was told to run cargo
  with `--offline`). List A 1, fixed by the branch's commit "fifth review
  fixes" (`aff7416`):
  - A1, `schemas/cges/README.md:14` said "Producers tag events with
    `schema_version`. Consumers must reject events with unsupported
    `schema_version`.", against ADR-0006's amendment ("The wire carries no
    `schema_version`") and the repo (no `schema_version` under `agent/` or
    `services/`), in the section the amendment says it restates. The line
    now says the schemas require the field and the wire carries none yet.
    Debt #58 is narrowed to match (below).

  List B 19, dispositioned:
  - fixed in `aff7416`: B1, §1's claim that a session of the kernel's
    flags opens on Windows 10 cites §Context 10, not only A13; B2, §2
    states that every record of the kernel session passes through the
    agent's memory, the rundown included (357 and 356 records on the
    measured machine); B3, ADR-0006's amendment names SPEC-019's and
    SPEC-020's change of `time` for 4001 and 3002 from ISO 8601 to
    string-encoded nanoseconds; B4, both READMEs add "the server is
    upgraded before the agents", and `v0.1/README.md:66` no longer opens
    with the unconditional rule; B5, ADR-0009's amendment says "capacity";
    B8, §2's 1024 is "characters for ASCII text, §Context 4", the unit for
    other text SPEC-021's; B9's first item, "a backup included" in
    §Compliance and threat-model §8's mitigation; B10, the threat model's
    §1 asset adds a kernel or complete memory dump of the host; B12, the
    probes' SID is that of the program that started them (§Context 10,
    §6); B15, A13 records the alternative of one session of the kernel's
    flags beside the manifest providers, put to Manuel and not chosen, and
    that ferrisetw 1.2 opens none without a raw `EnableTraceEx2` call
    (ADR-0008 §Compliance); B17, §2's attribution names the matching with
    the standard library and the replacement on the drain path;
  - for SPEC-021: B6, two session names against ADR-0008 §Compliance's
    "Constant-define the session name in one place", and whether a stop or
    query by name works on a system-logger kernel session (`ControlTraceW`,
    already listed); B7, the stderr line when the kernel session fails for
    privilege (SPEC-005 AC-002's line names Kernel-Process) and the stop of
    the first session when the second fails to open; B9's second item, the
    column time-to-live's mechanics (`MODIFY COLUMN … TTL` rewrites every
    part unless `materialize_ttl_after_modify` is 0, as
    `services/ingest/src/db/migrate.ts:215` sets for the table's) and the
    merge cadence that bounds "until a merge runs"; B16, the markers'
    scope, since `objects/process.json` also reaches `parent_process` and
    4001's `actor.process` through `$ref`;
  - recorded here: B13, which source arrived first at the median is not in
    the repo. `C:\tmp\s36-b2-probe3.txt`, line 9: "arrival at the callback,
    kernel minus Kernel-Process: p10=-794.30 p50=-0.14 p90=0.02 ms" — the
    kernel's record first by 0.14 ms at the median, as ADR-0020 §Context 5
    says;
  - no change: B11, the threat model's "encrypted at rest" for buffered
    events is debt #55, deferred by the amendment's Effect; B14, ADR-0018
    §9 excepts the forensic drill, which does not project the marker;
    B18, settled or not under review — ADR-0009's missing `Last updated`
    (first pass B7); `docs/product/blueprint.md:367` ("breaking changes
    bump major"), the founding text whose CGES rules ADR-0006 and its
    amendments supersede (its timestamps rule differs too); ADR-0011 A4,
    H-inline; B19, external claims the repo cannot check (Microsoft Learn,
    OCSF, PCI DSS, GDPR) — the two Microsoft Learn statements are quoted
    above from the pages ADR-0020 cites.

  The first pass's items at the "sí" are done: B6, Manuel's answer named
  the six amendments after the request named ADR-0006's as a standing
  rule; B7, the dates at the landing (above).

Decisions Claude Code took at the landing (anticipated):

- The five Status lines of the co-located amendments, "Co-located with
  ADR-0020; lands when ADR-0020 is Accepted", read "Accepted with it on
  2026-10-13", and the catalog's edges "co-located, landed with ADR-0020".
  Alternative: keep the future tense, which is false once the change is on
  `main`. Where: `71a5930`.
- The ratification as the first entry of ADR-0020's Decision record, as
  ADR-0019 records its own. Where: `71a5930`.

- **SPEC-017 Amendment 2026-10-13**, the fixed SPEC/ADR prompt over
  `docs/specs/SPEC-017-agent-capture-normal-run-path.md` and
  `docs/specs/README.md` in the working tree, two passes before `009b103`.
  - First pass, List A 2, both fixed: A1, "the Windows `rust-ci` job runs
    elevated, where the start succeeds", against SPEC-017 Open questions 1
    (the runner's ETW privilege untested) — now "if its start fails there";
    A2, "the state the other failures already reach", while the
    pump-thread branch (`session.rs:286-293` at `009b103`) stopped nothing —
    that branch is now in the amendment and in the fix.
  - First pass, List B 9: fixed B1 (the measured machine named), B2 ("a
    later start of the agent stops it"), B3 (a failed stop logged at
    `warn`), B4 (`open_excluding` named), B5 (the test named); recorded B6
    (#60's destination and plural, *Debts*), B7 (#62), B8 (the unelevated
    stop was measured with `logman`; the agent's own stop is asserted by the
    unelevated run, and an unelevated stop of a session an elevated agent
    started is refused, *Progress*); B9 lists what it checked.
  - Second pass, List A 2, both fixed: A1, "run only … developer-local"
    against "the runner … on either outcome" — now "they run wherever a
    start fails"; A2, the spawn failure (`session.rs:238-241` at `009b103`)
    also skipped the stop — named as a start that never reached
    `StartTraceW`.
  - Second pass, List B 8: fixed B5 (the `ERROR_ALREADY_EXISTS` clause
    dropped) and B7's first item; B3 answered by the red run (an unelevated
    query by name answers `Ok(0)` for a live session, so `Err(4201)` means
    none); recorded B1, B2 (true once the fix landed), B4 (a `warn` where the
    stop itself is denied with nothing to stop, unmeasured), B6, B7's second
    item (#62), B8 (the elevated gate ran).
- **The fix, `009b103..cfe70db`**, Claude Code's matrix (contract: SPEC-017
  Amendment 2026-10-13 and #60), paths in
  `agent/cg-agent/tests/capture_ac_001_startup_failure.rs`:

  | Criterion | Test | Verdict |
  |---|---|---|
  | After a failed `EtwSession::open`, no session of the agent's name | `:101-107`, `assert_no_session` `:127-133` | covered for the privilege failure; where `StartTraceW` refuses, the stop is not exercised |
  | After the real binary exits 9, no session | `:228-229` | covered |
  | Beyond the criterion: after the in-process secure path fails, no session | `:167` | covered |
  | Elevated, `stop` leaves no session | `:96-99`, gate 3 | covered |
  | The pump thread ending before its report stops the session | none | missing: reached only by a panic inside ferrisetw, no seam |
  | A failed stop is logged at `warn` and the start's failure returned | none | missing: `ControlTraceW(STOP)` cannot be made to fail on demand |

  The fixed code prompt over the same range, with no build or run (the
  gate was on the tree): list 1 as the matrix; list 2 empty; list 3, the
  `info` line when the stop ends a session, which the amendment did not
  state — kept and stated in this commit; the non-privilege path now logs
  the stop's result; `ETW_LOCK` became a tokio mutex held across the async
  tests' awaits; no change to authentication, authorization, cryptography,
  privileges or the data collected (the same stop by name that
  `reclaim_zombie` sends at every open). Not blocking, corrected in this
  commit: the amendment's anchor `session.rs:238-241` became `:239-242`
  with the fix, and its Effect's "every other failure of a start that
  reached `trace.start()`" clashed with the pump-thread case. Recorded: the
  marquees stop leaving a session on a failed start only once the release
  binary is rebuilt (gate 3 rebuilt it); #60's plural.
- **This commit's SPEC-017 change** (the amendment brought to `abde0a5`:
  the `info` line, the anchor, the Effect's two cases), the fixed SPEC/ADR
  prompt over the amendment as changed in the working tree. List A 2, both
  fixed: A1, capture_ac_001's added criterion was unconditional while a
  session an elevated agent left (#63) cannot be stopped unelevated — now
  "from a host where no session of the agent's name is running"; A2, "they
  run wherever a start fails", while the test asserts a privilege failure
  first — now "fails for privilege". List B 12: fixed B1 (the anchor's SHA
  named), B2 (the Effect's two cases named), B6 (the log's new line), B11's
  citation (the refused stop is under *Progress*, not gate 2); recorded B3
  (the bullet described, not quoted, as the amendment first landed), B4
  (the `info` clause stated after the code: a reactive correction,
  *Decisions*), B5 (ferrisetw's `open_trace(...)?`, `src/trace.rs:529`,
  also returns without stopping; the rule covers it), B7 (an unelevated stop
  with nothing to stop may log a spurious `warn`, unmeasured), B8 and B9
  (SPEC-017 Open questions 1: the runner runs as the built-in
  Administrator, so the added assertions most likely never run in CI; its
  test output does not say which branch it took), B10 (the matrix's two
  missing rows), B12 (#62).

Decisions Claude Code took on debt #60 (anticipated):

- #60 fixed now, on its own branch, not on B2's implementation branch (its
  recorded destination): B2 waits on SPEC-021, and the defect left a
  session behind on every unelevated run. Where: `009b103`, `abde0a5`.
- The contract first, as a SPEC-017 amendment, although a fix's contract
  may be its debt entry: no criterion covered a failed start, and the
  amendment makes the rule standing. Alternative: the debt entry alone.
- capture_ac_001's tests that open ETW serialized by a tokio mutex held
  across their awaits, so that a leftover check cannot see another test's
  session. Alternative: a std mutex with clippy's `await_holding_lock`
  allowed.

Reactive:

- The pump-thread branch stops the session too, and the amendment says
  so: the first pass's A2.
- The `info` line on a stop that ends a session, kept and stated in the
  amendment after the code review (this commit). Alternative: remove it,
  which would have needed the elevated gate again for a log line.

## Debts

New this session:

- **#58 — The schemas require `schema_version`; the wire carries none.**
  `schemas/cges/v0.1/event.json:82` requires a top-level `schema_version`,
  and no `schema_version` appears in `agent/cg-agent/src`,
  `services/ingest/src` or `services/api/src`. As first recorded, the entry
  cited `schemas/cges/README.md:12`, "Producers tag events with
  `schema_version`. Consumers must reject events with unsupported
  `schema_version`."; that sentence was at `:14` on ADR-0020's branch and
  `71a5930` replaced it with what is true ("The wire between the agent and
  the ingest service carries none yet, and no consumer checks it"). What
  remains is the gap between the schema and the wire. Destination: B2's
  implementation branch, which changes the CGES schemas.
- **#59 — No environment runs the agent on the builds it covers.** Manuel
  decided on 2026-10-12 that the agent runs on Windows 10 and Windows
  Server 2019. Every measurement and the elevated gate are on 10.0.26200;
  the `rust-ci` Windows job runs on `windows-latest`. Nothing has run the
  agent on Windows 10 or Windows Server 2019. Destination: Manuel — a
  machine or virtual machine of each (Windows 11 Home has no Hyper-V; a
  virtualization tool is outside the approved toolchain).
- **#60 — An unelevated agent leaves its ETW session behind.** Unelevated,
  `StartTraceW` succeeds and `EnableTraceEx2` of Kernel-Process fails with
  access denied; ferrisetw 1.2 returns without stopping the session, and
  `EtwSession::start` stops it only when the failure is not a privilege one
  (`agent/cg-agent/src/etw/session.rs:276-281`, whose comment says "a
  privilege failure creates none"). So every exit 9 — and every unelevated
  run of `capture_ac_001_open_returns_the_start_result` and of the four
  marquees — leaves `CGAgent-KernelProcess` running with no provider until
  an elevated start reclaims it or the host restarts (gate 2, side effects,
  above). No contract states otherwise: SPEC-017 capture_ac_006 asks for no
  leftover session only elevated. `logman stop` ended it unelevated, so the
  agent can stop it itself. Destination: B2's implementation branch, which
  changes the open path for the second session: stop each session by name
  after any failed open, and a test that asserts no session of the agent's
  names is left after an unelevated open.
- **#61 — The agent's sessions buffer in the mode Microsoft advises against
  at high rates.** Both sessions run with
  `EVENT_TRACE_NO_PER_PROCESSOR_BUFFERING` (ferrisetw 1.2's default), which
  orders delivery (#43, closed below) and which *Logging Mode Constants*
  advises against above about 1,000 events per second. The user session
  carries Kernel-Network's connections, whose rate on a busy server is not
  measured; gate 2 lost nothing at 400 process starts in about 3 s.
  Destination: a load measurement before roadmap §F; the lost counters
  (`events_lost`) are where a loss would show.
- **#62 — SPEC-005 names the wrong call for exit 9.** Its §Failure modes
  (`docs/specs/SPEC-005-agent-process-telemetry-windows-etw.md:205`) says
  `OpenTrace` / `StartTrace` returns 5 or 1314; on the measured machine
  the refusal comes at `EnableTraceEx2` (#60). SPEC-017 §Operational §1's
  "Start refused for privilege" covers both by scope. Destination:
  SPEC-021, which amends SPEC-005 §Failure modes.
- **#63 — The marquees kill the agent, so the last one leaves its session
  running.** `services/ingest/test/helpers/marquee-agent.ts:128` ends the
  agent with `child.kill("SIGINT")`, which on Windows Node turns into a
  forced termination: the agent's shutdown, which stops its session
  (SPEC-017 §Operational §4, capture_ac_006), does not run. Each marquee's
  agent reclaims the previous one's session (the SPEC-005 marquee's log in
  gate 3b: "reclaimed zombie ETW session (pre-existing session stopped)"),
  and after the gate `CGAgent-KernelProcess` stays running with its two
  providers and no consumer, until the agent starts again or the host
  restarts; unelevated it cannot be stopped. While it runs, an unelevated
  `cargo test` fails capture_ac_001: the reclaim is refused, the start
  fails with `ERROR_ALREADY_EXISTS`, not for privilege (SPEC-017 Amendment
  2026-10-13, the criterion's precondition). Destination: a harness fix
  that stops the agent's session at each marquee's end (or ends the agent
  gracefully), with the elevated gate; B2's implementation branch touches
  the marquees.

Closed this session:

- **#60 — An unelevated agent leaves its ETW session behind.** Fixed by
  `abde0a5` under SPEC-017 Amendment 2026-10-13: after any failed start
  that reached `trace.start()` the session of the agent's name is stopped,
  and capture_ac_001 asserts that none is left after a failed open, a
  failed in-process secure path and the binary's exit 9. It covers the one
  session that exists: ADR-0020's second session needs the same stop and
  assertion, in SPEC-021 and B2's implementation.

- **#43 — Launch and connect may reach the callback out of order**
  (handoff-35). Its premise, "A real-time ETW session does not order events
  across CPUs", does not hold for the agent's session: it runs with
  `EVENT_TRACE_NO_PER_PROCESSOR_BUFFERING` (`LogFileMode` `0x10400100`,
  gate 2), which writes the events of all processors to a common buffer
  (Microsoft Learn, above), and gate 2 measured 0 of 1349 records of that
  session, Kernel-Process and Kernel-Network together, out of timestamp
  order (gate 1: 0 of 1997 in one session). A process's connections are
  logged after its start, so its Launch is dispatched first. The cost of
  the mode is #61.

## Gate outputs and baselines

Non-elevated, Claude Code's terminal, at `cfe70db`: `cargo fmt --all --
--check` clean; `cargo clippy --all-targets --all-features -- -D warnings`
clean; `cargo test --all`: 125 passed, 0 failed, 7 ignored, as S35. Before
the fix, the three new checks of capture_ac_001 failed with `left: Ok(0)`,
`right: Err(4201)`; with it, `test result: ok. 8 passed; 0 failed`.

Gate 3, elevated, at `cfe70db`, on 2026-10-13 (logs
`C:\tmp\s37-gate3-*.log`, 03:11–03:12):

- `net session` → "No hay entradas en la lista."; `git rev-parse --short
  HEAD` → `cfe70db`; `git status --porcelain` → nothing.
- `cargo build --release -j 2 -p cg-agent` → "Finished `release` profile
  [optimized] target(s) in 19.45s".
- capture_ac_001, elevated → "test result: ok. 8 passed; 0 failed; 0
  ignored; 0 measured; 0 filtered out; finished in 0.02s".
- `--ignored`, 7 passed: `auth_ac_012_logons_on_the_real_log`,
  `capture_ac_006_elevated_shutdown_leaves_no_session`,
  `net_ac_008_a_real_connection_is_reported_with_its_probe`,
  `net_ac_009_an_accepted_connection_and_the_filter`,
  `ac_004_cache_hit_terminate_matches_launch_byte_for_byte`,
  `ac_007_parent_pid_only_when_parent_dead_at_child_launch`,
  `ac_009_events_lost_under_deliberate_etw_buffer_pressure`.
- vitest → "Error: Could not find a working container runtime strategy",
  "Test Files  no tests": Docker Desktop was not running (its engine pipe
  absent). Not a result of the change; rerun as gate 3b.
- `logman query -ets` → no `CGAgent` session.

Gate 3b, elevated, at `cfe70db`, on 2026-10-13 (logs
`C:\tmp\s37-gate3b-*.log`, 03:23–03:26):

- `docker version` → `29.1.2`; `net session` → "No hay entradas en la
  lista."; `cfe70db`; `git status --porcelain` → nothing.
- `cargo build --release -j 2 -p cg-agent` → "Finished `release` profile
  [optimized] target(s) in 0.61s" (gate 3's binary).
- vitest `services/ingest` → "Test Files  60 passed (60)", "Tests  197
  passed (197)", "Duration  188.98s". The marquees: "detect_ac_001 captured
  image_file_name — probe: C:\Users\<user>\AppData\Local\Temp\cg-detect-probe-MQY4Au\winword.exe;
  child: C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe";
  `{"event":"spec_005_marquee_complete","marquee_elapsed_seconds":40.054,"budget_seconds":45,"within_budget":true}`;
  "spec_005 captured image_file_name — launch: C:\Windows\System32\cmd.exe;
  terminate: cmd.exe"; `net_ac_010` 25.1 s and `auth_ac_013` 25.071 s
  within 45; `ac-001-marquee` passed.
- `logman query -ets` → `CGAgent-KernelProcess  Seguimiento  Activo` (#63).

Baselines at the close: non-elevated cargo 125 passed / 7 ignored;
elevated cargo `--ignored` 7 passed; vitest `services/ingest` elevated 60
files / 197 tests; `services/api` unchanged (31 files / 62 tests, S35), not
run. CI: every push to `main` green — the documentation commits
`markdown-lint → success`; `abde0a5` `ts-ci → success`, `rust-ci →
success`; on the branch, `cfe70db` the same two.

## What waits on Manuel

1. SPEC-021's text (*SPEC-021*).
2. `logman stop CGAgent-KernelProcess -ets`, elevated, for the session
   gate 3b left (#63).
3. #59: a Windows 10 and a Windows Server 2019 to run the agent on.
4. Roadmap Part (b) §9: which threats §B3's rules cover, before its SPEC
   is accepted.
5. #55: the threat model brought in line with the system.
6. Unchanged: Part (b) §1–§4.

## How Session 38 resumes

1. *Session protocol* §At the start. Expected: `main` = `origin/main` at
   this file's last commit; tree clean; no stash; no branch or PR open;
   catalogs ADR 20 / SPEC 20; Known CI debt zero.
2. SPEC-021: do not retry S37's drafting (*SPEC-021*); ask Manuel for the
   text. When he brings it, land it verbatim (*Integration path* §6, with
   the SHA-256), self-review it, and set its Status `Accepted`. It covers
   the review items marked "for SPEC-021", #60's second session and #62.
3. Then B2's implementation, on a branch with a PR: acceptance tests
   failing first, CI on every push, the elevated gate through Manuel, the
   self-review, landing by cherry-pick. It carries #31, #52, #53, #54, #56,
   #58 and #63.
4. Then roadmap §B3, after Part (b) §9.
