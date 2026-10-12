# Handoff — Session 37

Session 37 opened on 2026-10-12. This record is started early because
ADR-0020's owner-review branch cites its measurement; it is completed at
the close.

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

Manuel's decisions this session, in chat:

- 2026-10-12, the fresh-context pass over `fec2ee4..b751bdc`: "Sí, lánzala
  (Recomendado)".
- 2026-10-12, ADR-0020 Open questions 5 before his ratification: "Mídela
  primero (Recomendado)".
- 2026-10-12, after the measurement below, on the builds the agent runs on:
  "Incluir Win10 y Server 2019 (Recomendado)" — the agent covers Windows 10
  and Windows Server 2019, so ADR-0020 keeps the second session (its A13).

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

## Debts

New this session:

- **#58 — CGES's README says producers tag events with `schema_version`.**
  `schemas/cges/README.md:12`: "Producers tag events with `schema_version`.
  Consumers must reject events with unsupported `schema_version`." The wire
  carries none: no `schema_version` in `agent/cg-agent/src`,
  `services/ingest/src` or `services/api/src`, while
  `schemas/cges/v0.1/event.json:82` requires it. Destination: B2's
  implementation branch, which changes the CGES schemas.
- **#59 — No environment runs the agent on the builds it covers.** Manuel
  decided on 2026-10-12 that the agent runs on Windows 10 and Windows
  Server 2019. Every measurement and the elevated gate are on 10.0.26200;
  the `rust-ci` Windows job runs on `windows-latest`. Nothing has run the
  agent on Windows 10 or Windows Server 2019. Destination: Manuel — a
  machine or virtual machine of each (Windows 11 Home has no Hyper-V; a
  virtualization tool is outside the approved toolchain).
