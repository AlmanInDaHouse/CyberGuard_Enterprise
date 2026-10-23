# ADR-0020: Process command line and user v0.1 (the kernel's process events joined to Kernel-Process, secrets replaced on the agent, the process's own SID)

- Status: Accepted
- Date: 2026-10-11
- Last updated: 2026-10-23
- Deciders: Manuel (project owner), Claude Code (proposal and implementation)

## Context

Roadmap §B2 gives the agent a real source for the command line and the user of a process and carries them end to end; it also names the parent stamped by the agent (SPEC-016 §Out of scope), which §5 places. Observed at `3b21b5f` and on Manuel's machine (Windows 11 Home 10.0.26200, in a workgroup) on 2026-10-11, and one more measurement there on 2026-10-12 (§Context 10). The measurements on the machine came from scripts and probes that wrote only redacted aggregates; they are summarised in `docs/handoff-session-36.md` §Progress and `docs/handoff-session-37.md` §Progress.

1. **The fields exist and are always empty.** The 1007 element carries `process.command_line` and `process.subject_user_sid` (`services/ingest/src/schemas.ts:45-46`), the route stores them in `process_command_line` and `subject_user_sid` (`services/ingest/src/routes/heartbeat.ts:109-110`, `services/ingest/src/db/clickhouse-bootstrap.ts:63-64`), and the agent fills them from the Kernel-Process properties `CommandLine` and `UserSID` (`agent/cg-agent/src/etw/session.rs:413-414`), which that provider does not have, so both are always `""`. The read-model does not project them (`services/ingest/src/detect/read-model.ts:114-115`), nor does the forensic drill (`services/api/src/read/types.ts:52-53`); the evaluator refuses a rule over `CommandLine` or `User` (SPEC-015 §Out of scope). ADR-0011 §4 maps `process.cmd_line ← CommandLine` and `process.user.uid ← SubjectUserSid`, and ADR-0012 §3 flagged that neither ETW field exists.
2. **Three texts wait for these fields.** ADR-0012 §5 plans to add the user to the alerts' `dedup_key` "when user-SID capture lands"; SPEC-007 §Operational §3 leaves the `user` dimension out of the incident `grouping_key` for want of a user; SPEC-010 §Out of scope says the drill's two empty fields "return when a richer ETW capture populates them".
3. **Kernel-Process does not carry them.** Its ProcessStart, versions 0 to 4, has no command line and no user SID (read from the manifest unelevated, and on live events at version 4). With the SID enable property, no SID appeared among the events' extended data in two runs.
4. **The kernel's process events do.** The kernel's process class ("Windows Kernel Trace", process flag), read through ferrisetw 1.2's `KernelTrace` in a session of its own name (system-logger mode, not the "NT Kernel Logger" singleton), opened while another tool held the NT Kernel Logger:
   - `CommandLine`, `UserSID`, `ImageFileName` and `ParentId` were read in 404 of 404 records, with no schema error, and the SIDs decoded to their string form;
   - 20 of 20 probes (`cmd /d /c rem`, which exit at once) were seen at their start, each with its marker in the command line;
   - the session's rundown (opcode 3) listed the 357 processes running when it opened, with their command lines;
   - the command line is cut at **1024 characters** at start, end and rundown alike (probes of 3000, 6000 and 8000 ASCII characters arrived with 1024; whether the limit counts UTF-16 code units for other text was not measured);
   - `UserSID` is the process's **own** user, not its creator's: in 72 records it differs from the parent's (services of `S-1-5-19` and `S-1-5-20`, and the user's processes, under `S-1-5-18` parents);
   - delivery to the callback took a median of 0.73 s and, at the 90th and 99th percentiles, about 1.0 s (ferrisetw's default flush timer is 1 s).
5. **But they carry no path.** The kernel's `ImageFileName` is the image's base name, never a path (no backslash in 81 of 81), not truncated (a 52-character name arrived whole). On a Launch, Kernel-Process's `ImageName` is the full device path (80 of 80), which the agent translates to the Win32 form the rules match (SPEC-017 §Operational §5, SPEC-016). Matched by PID, the base names agree in 79 of 79, and the two events' timestamps differ by at most 0.01 ms; at the median the kernel's reached its callback 0.14 ms before the Kernel-Process event reached its own; in a tenth of the pairs the kernel's arrived about 0.8 s earlier or more (10th percentile, −794 ms), and at the 90th percentile the Kernel-Process event arrived 0.02 ms earlier. Neither longest lead was measured.
6. **The other sources.**
   - *Security 4688.* Process Creation is not audited on this machine and the setting that adds the command line is absent; the 112 records in the log are boot processes, none with a command line. Using 4688 means changing the audit policy of every host, which is the deployment contract (roadmap Part (b) §6), and it writes each command line into the host's Security log as well.
   - *Reading a running process's command line* (`NtQueryInformationProcess`). Elevated, 347 of 354 running processes were read, at about 32 µs each. A `cmd /d /c rem` probe was read 0 and 10 ms after `Start-Process` returned and not from 50 ms on (`0xC000010A`); of the processes that started and ended in a 90 s window, 42 of 53 lived under 50 ms, median 35 ms. A reader driven by the ETW start event, about 0.7 s late, misses nearly every short-lived process.
7. **What a command line holds.** User names inside paths, document names, addresses, and at times secrets: a password after `net use`, `psexec -p`, `cmdkey /pass:` or `schtasks /rp`, a connection string's `Password=`, a token or key passed as an argument, credentials inside a URL. On the measured machine, 2 of the 352 non-empty command lines of the rundown contained the word "token"; none matched a password pattern. The rules that need the command line look for its text: an encoded PowerShell, `certutil -urlcache`, `mshta http`, `vssadmin delete shadows`.
8. **Names on the wire.** The 1007 wire names are flat; ADR-0011's Amendment 2026-10-04 made the realized names the wire contract and the OCSF paths the mapping they realise. The newer classes follow OCSF on the wire: `src_endpoint` and `actor.process.uid` for 4001, `user.uid` for 3002 (`services/ingest/src/schemas.ts:82`, `:92`, `:108`). CGES's `user` object requires `name` (`schemas/cges/v0.1/objects/user.json`).
9. **Retention today.** ADR-0019 §9 gives logon rows 365 days from arrival; rows of the other classes, 1007 included, are kept without limit (debt #47). Alerts and incidents live in Postgres (ADR-0012 §6, SPEC-007) with no retention.
10. **One session, measured; the builds that allow it.** On 2026-10-12 one ferrisetw 1.2 `UserTrace` in system-logger mode, holding the agent's two providers (Kernel-Process, keyword 0x10; Kernel-Network, keywords 0x30) and the system process provider (`SystemProcessProviderGuid`, keyword `SYSTEM_PROCESS_KW_GENERAL`), ran for 45 s:
    - it started and delivered all three: 67 Launches, each with the device path; 1312 network records; and 488 of the kernel's process records, 356 of them the rundown at the session's open, with `CommandLine`, `UserSID`, `ImageFileName` and `ParentId` read in all 488, with no schema error;
    - the kernel's records carry the process class's GUID (`3d6fa8d0-fe05-11d0-9dda-00c04fd7ba7c`), not the system provider's, as Microsoft documents ("the outputted events are not marked as being emitted from the individual system providers"); ferrisetw hands a record only to the provider whose GUID it carries, so the callback is registered under the class's GUID;
    - 20 of 20 probes were seen with their marker and the SID of the program that started them, and a probe of about 3000 characters arrived with 1024;
    - matched by PID, each of the 67 Launches was followed directly by its kernel record, with no record between them, the same base name and the same timestamp; the two reached the callback a median of 0.01 ms apart, and at most 0.71 ms;
    - none of the 1997 records delivered arrived after one with a later timestamp, under ferrisetw's default buffering (`EVENT_TRACE_NO_PER_PROCESSOR_BUFFERING`).

    Enabling the kernel's events through a system provider exists from build 20348 (Microsoft Learn, *System Providers*: "Beginning in Windows 10 SDK build 20348"), that is Windows Server 2022 and Windows 11; Windows 10 (build 19045) and Windows Server 2019 (build 17763) do not have it. A session of the kernel's flags in system-logger mode, which `KernelTrace` opens, exists from Windows 8 and Windows Server 2012, which deliver the kernel's events to at most eight logger sessions, two of them reserved for the NT Kernel Logger and the Circular Kernel Context Logger (Microsoft Learn, *Configuring and Starting a SystemTraceProvider Session*).

## Decision

### 1. Source — Kernel-Process for the event, the kernel's process events for its command line and user

- **Kernel-Process stays the source of the process events.** Launch and Terminate, the image path, the parent PID, the exit code, the timestamp and the `process.uid` keep coming from `Microsoft-Windows-Kernel-Process` in the agent's existing session, as SPEC-005 and SPEC-017 specify, and the creation-time cache keeps its role.
- **The kernel's process events are added** in a second session of the agent's own name, opened through ferrisetw's `KernelTrace` after the existing session, which keeps being the first to open and so the one whose failure ADR-0010 §1 reports. One session for both would deliver each kernel record right after its Launch (§Context 10), but only from build 20348, and the agent runs on Windows 10 and Windows Server 2019 too (§Decision record); a session of the kernel's flags opens on both (§Context 10, A13). A process that starts between the two opens has a Launch and no kernel record, and goes with `cg_kernel_record_unavailable` (§2). Only the kernel session's process-start records (opcode 1) are used; its end and rundown records are ignored. The rundown is not used to close the gap between the two opens: it lists every process running when the session opens, most with no Launch to join, and matching a rundown record to a Launch of the gap by PID would be a second join, for the gap alone, whose Launches the marker already shows. Where the Security log subscription of ADR-0019 opens in that order is SPEC-021's.
- **The join.** It is made by the two callbacks through the join table, a structure of the agent's bounded in count, keyed by PID and matched by event timestamp within a tolerance (§Context 5: equal within 0.01 ms). The table holds, for each pair, the first of the two to arrive: a kernel record's command line and user SID, or a Launch's slot, which the Launch also carries into the ring. When the second arrives, the values go into the slot and the entry leaves the table; from then on they live only in the slot, in the ring, and leave it when the Launch is rendered, redacted, into its batch, or dropped from the ring (ADR-0009 §3). Each callback keeps to what ADR-0009's Amendment 2026-10-13 allows: no I/O beyond the anomaly lines its SPEC names, no lock of its own beyond the structures it uses, each kept small (the ring, the creation-time cache, the join table; the locks inside ferrisetw are those every callback takes, SPEC-017 Amendment 2026-10-11), and no wait for another event. When the Launch's batch is formed, on the drain path, its slot is read. A slot still empty then waits at most a bound measured from the Launch's dispatch, of the order of twice the flush timer; after it the Launch goes with an empty command line, no user and the marker `cg_kernel_record_unavailable` (§2), which says that both are unknown. An entry whose pair does not complete leaves the table at the same bound, measured from its arrival, removed by a purge that runs whether or not a batch is being formed or delivered, so that the bound holds while the server cannot be reached (where the purge runs is SPEC-021's): a Launch's entry then leaves its slot empty, and a kernel record's is dropped and counted, as is a kernel record that arrives after its Launch's entry left. When the table is full, its oldest entry leaves first, on the same terms. How the wait fits SPEC-017's batch triggers and its order across POSTs is SPEC-021's, which amends SPEC-017 by scope where it must.
- **No new crate and no new privilege.** `KernelTrace` is part of ferrisetw 1.2 (ADR-0008); the elevated token of ADR-0010 §1 opens it, as measured. ADR-0008's side-channel helper (`events_lost`, the reclaim of a zombie session) applies to the second session by its name.
- **A session that cannot start does not degrade.** If the kernel session cannot open, the agent exits as SPEC-017 §Operational §1 rules: 9 for a privilege failure, 1 for any other, with the cause on stderr, as SPEC-019 ruled for its provider. Windows runs at most eight logger sessions that receive the kernel's events, two of them reserved (§Context 10); how many a host has free was not measured.
- **Why.** It is the only measured source that sees short-lived processes with their command line and their user without changing the host's policy (§Context 4, 6), on every build the agent runs on (§Context 10), and Kernel-Process keeps the verified path the rules match (§Context 5).

### 2. The command line — as the source delivers it, secrets replaced before it leaves the host

- **Content.** `process.command_line` is the kernel's `CommandLine` of the process start, which the source delivers cut at 1024 (characters for ASCII text, §Context 4; the unit for other text is SPEC-021's to measure), after the replacement below; the replacement can lengthen it.
- **Secrets are replaced on the agent.** Before a command line is serialized, the agent replaces each value that a closed list of patterns marks as a secret with `<redacted>`, keeping the key or flag that introduces it (`password=<redacted>`, `/pass:<redacted>`), so that a rule still sees that a password was passed. When the line may have been cut (below) and a pattern's key appears with its value running to the end of the line, that value is replaced too. The list, its matching rules and a test per pattern are SPEC-021's; since the list decides which secrets leave the host, Manuel ratifies it with SPEC-021; the matching uses the standard library, and a direct dependency on a matching crate such as `regex` would need its own decision (a new dependency of `cg-agent` is Manuel's): the agent's shipped tree holds only `regex-automata` and `regex-syntax`, through `tracing-subscriber`, and `regex` itself only through a dev-dependency (`cargo tree -p cg-agent -i regex`). The replacement runs on the drain path, when the event is rendered (as the path translation does, SPEC-017 §Operational §5).
- **Where the unredacted line lives.** In the ETW session's buffers, in kernel memory, which another elevated process could also consume by the session's name; then in the agent's memory only. Every record of the kernel session passes through it as ETW delivers the session's buffers, end and rundown records included (the rundown lists every running process at each open, 357 and 356 records on the measured machine, §Context 4, 10), and the callback keeps none but the start records: in the join table, for at most the join's bound when it meets no Launch; in its Launch's slot while the Launch is in the ring, which is ephemeral (ADR-0009 §3); and on the drain path while the Launch is rendered. Once rendered, the replaced line and the user SID stay in the agent's memory, in their batch, until it is delivered, which an outage of the server delays without a limit (SPEC-017 §Operational §3); like the ring, the batch does not survive the agent (ADR-0009 §3). The agent never writes it to disk or to its own output, and it does not clear the memory it frees. Windows may page that memory out, or keep a crash dump of the agent and send it through Windows Error Reporting, and a kernel or complete memory dump of the host holds the session's buffers; whether a deployment turns those off is the deployment contract's (Open questions 4).
- **Markers.** `process.cg_cmd_line_redacted` is true when at least one value was replaced. `process.cg_cmd_line_possibly_truncated` is true when the line as delivered has 1024 characters, the source's limit, so that it may have been cut. `process.cg_kernel_record_unavailable` is true when the Launch was sent without its kernel record (§1), so that an unknown command line and user are told apart from empty ones. All three are `cg_*` extensions (ADR-0006), documented in `schemas/cges/v0.1/objects/process.json`, stored as `process_cmd_line_redacted`, `process_cmd_line_possibly_truncated` and `process_kernel_record_unavailable`. The new names take OCSF's `cmd_line`, as the user takes its OCSF path (§3); the existing field keeps its flat wire name `process.command_line` (§Context 8). The last defaults to true, so that rows written before, and rows from older agents, read as unknown rather than empty; on rows of other classes it means nothing, as their readers select by class (ADR-0018 §9). A Terminate from an agent that implements this ADR carries all three false; rows written before, Terminates included, read `cg_kernel_record_unavailable` by its default. `cg_cmd_line_possibly_truncated` counts the line's length as the source delivers it; whether a line with non-ASCII text reaches the limit at 1024 characters or at 1024 UTF-16 code units is SPEC-021's to measure.
- **Size.** SPEC-017 §Operational §7 accepts a body of up to 4 MiB, about 4 KiB per event in a batch of 1024. A command line of 1024 characters with non-ASCII text or escapes, beside the image path, can pass that share; SPEC-021 states the bound and what happens to an event above it (SPEC-017 Open questions 3).
- **Only on Launch.** A Terminate carries an empty command line and no user, as no rule reads them (SPEC-016 evaluates Launch events).
- **Retention: 365 days from arrival.** `process_command_line` is reset to empty by a column time-to-live of 365 days from the row's `arrived_at`, which ClickHouse applies at its time-to-live merges; the rest of the row keeps its retention (debt #47). The server's arrival time is used, not the event's `time` (ADR-0019 §9, debt #31). The markers keep no time-to-live: they hold no text, though `cg_cmd_line_redacted` still says, after the line has expired, that a value was replaced in a process's line; it is kept as long as its row, as the image path is, whose retention is debt #47's. On a Launch, an empty line with `cg_kernel_record_unavailable` true was not known; an empty line with it false was empty at the source or, on a row older than 365 days, has expired, and which of the two is not recorded. A Terminate's line is empty by §2. A row past its 365 days may still hold its line until a time-to-live merge runs, and when an event was resent (ADR-0009 §1), the table keeps the copy with the latest `arrived_at` (`services/ingest/src/db/clickhouse-bootstrap.ts:68`); until a merge leaves one copy, each copy's 365 days count from its own arrival.
  - **Why.** The command line is the field detection needs most and the one that carries the most personal data. The text serves the security lead; the replacement at source, the limit and a documented period serve the data-protection officer. Twelve months matches PCI DSS's audit history and ADR-0019's logons.
- Decided by Manuel (option B, §Decision record); the third marker, the markers' lack of a time-to-live, the matching with the standard library and the replacement on the drain path are Claude Code's, under this ratification.

### 3. The user — the SID of the process's own user, under a name that says so

- **Content.** The SID of the user whose token the process runs under, the kernel's `UserSID` (§Context 4), in string form. No account name or domain: the agent resolves none, and asks nothing of a domain controller.
- **The name of the field.** The wire carries it as `process.user.uid`, the OCSF path, as the newer classes name their fields (§Context 8), and the column is `process_user_uid`. `subject_user_sid` names, in Windows' vocabulary, the creator, which is not what is stored; it has never carried a value, so it is replaced now. The column `subject_user_sid` stays in `cges_events`, empty and unread; the server accepts `process.subject_user_sid` from an older agent and ignores it, and is upgraded before the agents (SPEC-019 §Operational §9), since an older server requires it. The mapping changes with it: ADR-0011 §4 maps `process.user.uid` from `SubjectUserSid`, the creator; it now holds the user the process runs as, which is what OCSF's `process.user` names ("The user under which this process is running"). That makes the rename a breaking change of the 1007 wire, and the new mapping a change of meaning, for which ADR-0006 §Compliance asks a major version; CGES's JSON Schema keeps valid every document valid before (`user.json` loosens, below, and the markers are new optional properties). ADR-0006 is amended co-located: while CGES is 0.x and no consumer outside the repository reads it, a change keeps the version, additive or breaking, as ADR-0011's Amendment 2026-05-28 did, and a breaking one lands with its migration, the server upgraded first. CGES stays at `schema_version` 0.1.0.
- **CGES's `user` object** requires `name`, which §3 does not send. `objects/user.json` is relaxed to require a non-empty `name` or a non-empty `uid`; a document valid before stays valid. Class 3002 keeps requiring its user's `name` (ADR-0019 §Context 1): today that comes from `user.json`, so SPEC-021 states it in `classes/3002_authentication.json` in the same change.
- **Retention: 365 days from arrival,** as the command line (§2), by a column time-to-live on `process_user_uid`. This is Claude Code's proposal under this ratification, not one of Manuel's three decisions: a SID is a pseudonym, and keeping it longer than the command line it explains would serve no reader. It is not copied anywhere that outlives it (§4).
- **Why.** The minimum that answers "who ran it". A person's account name is known from the logons the agent reports (ADR-0019 carries SID, name and domain), for the logons that happened while the agent ran (ADR-0019 §8), and well-known SIDs map statically.
- Decided by Manuel ("1 + i", §Decision record); the OCSF form of the new name is Claude Code's, under this ratification.

### 4. Delivery, storage and detection

- The new fields ride the 1007 element of SPEC-017's wire. The new columns are added with an idempotent `ALTER TABLE cges_events ADD COLUMN IF NOT EXISTS`, as SPEC-019 §Operational §7 did, `process_user_uid` with its time-to-live; the existing `process_command_line` gains its time-to-live with `ALTER TABLE cges_events MODIFY COLUMN`.
- The server, like the agent, writes no command line or user SID to its own output; its insert-failure line already leaves out ClickHouse's message, which can hold the row (SPEC-020 §Operational §7). ClickHouse's own logs and system tables can also hold a row it failed to insert, and a framework may log a request it refuses; SPEC-021 states the settings that keep row data out of them, as the threat model's query-log filtering of PII fields assumes (`docs/security/threat-model.md`, §8 Storage layer); those that are settings of the ClickHouse server, rather than settings the ingest service sends with its queries, belong to the deployment contract (roadmap §F).
- The detection read-model, whose forward read SPEC-018 §Operational §1 specifies, projects `process_command_line`, `process_user_uid` and the three markers, `process_cmd_line_redacted`, `process_cmd_line_possibly_truncated` and `process_kernel_record_unavailable`, for class 1007, carrying the fields end to end as roadmap §B2 states; they live only in the cycle's memory, and the markers let the SPEC of a rule over them tell an unknown command line from an empty one, and a replaced or cut one from a whole one. The evaluator still refuses rules over `CommandLine` and `User` (SPEC-015), and which rules may use them is not decided here (§Out of scope). SPEC-021 corrects the refusal's message, which calls the fields empty and defers them to §B2, where the rules over them are now §B3.
- **Nothing that outlives the 365 days copies the user SID.** ADR-0012 §5's `dedup_key` and SPEC-007 §Operational §3's incident `grouping_key` do not gain it: both are stored in Postgres, which keeps alerts and incidents without retention. They gain a user when alerts and incidents have a retention a user identifier can live under. ADR-0012 is amended co-located; SPEC-021 amends SPEC-007 by scope. ADR-0003's retention table, not implemented (debt #47), moves raw events to MinIO after ClickHouse; the SPEC that implements it keeps no command line or SID past their 365 days.
- **No surface shows the command line or the SID yet.** The drill keeps not projecting them (SPEC-010 §Out of scope), which SPEC-021 restates by scope: a drill, report or dashboard view that shows them decides its access to them in its own SPEC. The evidence an alert seals is the drill's output (ADR-0016), so the first SPEC that puts the command line or the SID into the drill or a report reconciles their 365 days with that evidence, as ADR-0019 §9 asks for logons.

### 5. The parent at the edge — its own contract

Roadmap §B2 also names the parent stamped by the agent from its own process table, seeded at startup, so that a process started before the agent still gives its children a `ParentImage` (SPEC-006 §Operational §2, SPEC-016 §Out of scope). It touches the creation-time cache, which also builds the `process.uid` of a Terminate and of a connection (ADR-0011 §6, SPEC-019 §Operational §3), and the point where paths are translated (SPEC-017 §Operational §5). It gets its own contract — an amendment of this ADR or a successor, with its SPEC — after SPEC-021, inside phase §B2. Nothing in §1–§4 depends on it.

### 6. Settled at the gate

What the elevated gate shows on real events before the implementation lands:

- a probe's Launch carries its command line, joined, and its user SID equals that of the process that started it;
- a probe whose command line holds a secret of each pattern of SPEC-021 arrives with that value replaced and `cg_cmd_line_redacted` true;
- a probe of more than 1024 characters arrives with `cg_cmd_line_possibly_truncated` true;
- every Launch of the run carries a non-empty command line, or was joined to a record whose command line is empty, or has `cg_kernel_record_unavailable` true, and the gate reports how many of each;
- both sessions open, the Kernel-Process session first, and stop on shutdown leaving no session of the agent's names behind.

The gate runs as an elevated user, as every gate so far; the LocalSystem posture ADR-0010 §2 foresees for packaging is not measured.

## Alternatives considered

### A1 — The kernel's process events as the only source

The first form of option 1, chosen by Manuel before the third probe. Rejected after it: the kernel's event carries only the base name (§Context 5), and the rules match image paths.

### A2 — Security 4688 with the command line

Rejected: it requires changing the audit policy of every host (the deployment contract), and it writes every command line into the host's Security log too (§Context 6).

### A3 — Reading the command line from the running process

Rejected: it misses nearly every short-lived process (§Context 6), which are the ones an attack chain is made of.

### A4 — The image path from the kernel's event alone

From `QueryFullProcessImageNameW` after each start event, or from the first token of the command line. Rejected: the first misses short-lived processes; the second is text the process's creator chooses.

### A5 — The command line as delivered, or only its hash, or kept 90 days

Not chosen by Manuel, who chose option B (§Decision record). The reasons put to him: the first sends secrets to the server, the second removes the text the rules need, the third falls short of a year of audit history.

### A6 — The account name and domain beside the SID

Not chosen by Manuel ("1 + i", §Decision record). The reasons put to him: the name in every event, and a lookup that may reach a domain controller.

### A7 — Keep the name `subject_user_sid`

Not chosen by Manuel ("1 + i", §Decision record). The reasons put to him: the name says the opposite of what is stored, and it is cheapest to change before it carries data.

### A8 — A flat `process.user_sid`

The flat style of the other 1007 names. Rejected: the newer classes name their fields by OCSF path (§Context 8), and ADR-0006 asks that of a field mapping onto OCSF.

### A9 — Run without command lines when the kernel session cannot open

Rejected: an agent that silently loses a field is a blind spot no one sees; there is no agent-health surface yet (SPEC-017 Open questions 2). Exiting is visible, and SPEC-019 ruled the same for its provider.

### A10 — The kernel records through the ring

The ring carries the events the agent sends; kernel records would take its capacity, be subject to its FIFO drop and count toward its 1024-event trigger without being sent. Rejected for a separate bounded table, which ADR-0009's amendment allows.

### A11 — The user in the `dedup_key` and the incident key now

As ADR-0012 §5 and SPEC-007 §Operational §3 planned. Rejected for now (§4): alerts and incidents keep no retention, and the SID would outlive §3's 365 days in them.

### A12 — Unknown as an absent field, without a marker

The pattern of ADR-0011 §5 and its A4 for an unresolved parent name: omit the field, no resolution flag. Rejected for the store: `process_command_line` is a non-nullable string, so an absent command line is stored as empty, as an empty one is, and the reader cannot tell them apart. A `Nullable` column would change the type of an existing column and still read every older row as empty rather than unknown; the marker's default reads them as unknown (§2). On the wire the user is omitted when unknown, as that pattern does.

### A13 — One session for all the providers

The agent's existing session in system-logger mode, with the system process provider beside Kernel-Process and Kernel-Network (§Context 10). It would remove the second session, the gap between two opens and the lead of one session's delivery over the other's; the join would stay, as the two records of one start still meet, one right after the other in one stream. Rejected for coverage: it needs build 20348, and the agent runs on Windows 10 and Windows Server 2019 (Manuel, §Decision record). Reopen: a floor of build 20348 for the agent.

One session of the kernel's flags beside the manifest providers, which Windows 8 and later may allow, was put to Manuel as a further measurement and not chosen (§Decision record). ferrisetw 1.2's builders do not open one — a `UserTrace` sets no kernel flags, a `KernelTrace` enables no provider — so it needs a raw `EnableTraceEx2` call, which ADR-0008 §Compliance admits only by extending its helper module or by a new ADR.

## Consequences

### Positive

- Process events gain the command line and the user, for short-lived processes too, with no change to the host's policy and no new crate or privilege.
- The agent sends no secret that the closed list names, and the command line and the user SID are stored for a documented period that nothing copies past.
- The data model names the user field for what it holds, in the OCSF form.

### Negative

- A second ETW session, of a kind Windows limits in number; another product holding many can stop the agent from starting.
- A Launch whose kernel record has not arrived when its batch is formed can wait up to about two seconds from its dispatch, which can hold a batch or reorder it; SPEC-021 states how, against SPEC-017's batching.
- The two sessions' events are not ordered with each other (§Context 5): a kernel record can arrive before its Launch, which the join table absorbs, or after the wait, and the Launch then goes with `cg_kernel_record_unavailable`. Within one session, delivery followed the events' timestamps in the one run that measured it (§Context 10), which puts the premise of debt #43 in doubt; SPEC-021 settles it for the agent's sessions.
- A command line longer than 1024 characters reaches detection cut; the marker shows it.
- A closed list of secret patterns misses formats it does not know: a positional password the list does not name travels as typed.
- A process can shape its command line against detection: a payload where a pattern marks a secret reaches detection as `<redacted>`, text past 1024 characters does not reach it, and a burst of starts larger than the join table's capacity sends Launches without their kernel record. The markers show each case (§2), and the threat model records them (its Amendment 2026-10-13). A parent that creates a child suspended and rewrites its arguments before it runs leaves a decoy in the start event, which no marker shows (argument spoofing).
- Two firings of one rule on one process name, in one five-minute bucket of one agent, under two users, still collapse into one alert (§4).
- The api's ClickHouse reader is read-only in use (ADR-0015 §Compliance), and no ClickHouse grant enforces that or limits its columns: the dev stack gives the api the ingest's ClickHouse user, which can write (`infra/dev/docker-compose.dev.yml:98`, `:174`): a compromised api can read the command lines and SIDs, as it can the logons, for which ADR-0019 §9 adds no access control either, and with those credentials copy them or remove their time-to-live. Users with scoped grants are the deployment contract's (roadmap §F).
- Image paths in 1007 rows still carry user names and are kept without limit (debt #47).
- How the two sessions behave on Windows 10 and Windows Server 2019, which the agent covers, rests on Microsoft's documentation (§Context 10): every measurement here is from build 26200, and no gate or CI job runs on either.

### Neutral

- Kernel-Process keeps its role: the process events, their path and the `process.uid` of ADR-0011 §6 are unchanged. SPEC-017's wire and batching change as §1–§3 state, through SPEC-021's amendments by scope; SPEC-021 also amends by scope SPEC-005 (its accepted risk on the command line), SPEC-006 (the two fields' description as always empty), SPEC-007 (§4), SPEC-010 (§4), SPEC-015 (the refusal's message), SPEC-017 (§Data contracts, the event element's fields; §Operational §1 and §4, the second session's start and stop; §Operational §6, the callback's locks), SPEC-018 (§Operational §1, the projection) and SPEC-019 (the agent's sessions, §Operational §1), and SPEC-020 if the startup order of the three sources changes.
- ADR-0018 §1's "There is no second session" concerns the network provider, which stays in the existing session.
- The kernel's process end and rundown events also carry command lines; neither is used.
- Between §B2 and §B3 the agent captures the two fields and no rule reads them. No deployment comes before roadmap §F, which follows both, so no customer's command lines or SIDs are collected for a purpose not yet served; a release before §B3 would have to state the purpose they are kept for (purpose limitation, GDPR Art. 5(1)(b)).

## Compliance

- The agent MUST apply the closed list of SPEC-021 to every command line before it is serialized, wherever in the event the line is sent (`raw_data` included, ADR-0011 §2).
- Neither the agent nor the server writes a command line or a user SID to its own output.
- No image path is ever taken from a command line.
- A surface that shows command lines or user SIDs MUST state its access rules in its SPEC (§4).
- `process_command_line` and `process_user_uid` MUST carry the time-to-live of 365 days from arrival, and no copy of either, a backup included, may be kept longer.

## Out of scope

- Rules over the command line or the user: roadmap §B3, a phase of its own right after §B2 (§Decision record). This ADR delivers the fields; which rules use them, that is which threats they cover, is Manuel's decision for that phase's SPEC (roadmap Part (b) §9).
- The parent at the edge (§5), its own contract inside §B2.
- A product surface that shows them (§4).
- The retention of the rest of the 1007 row (debt #47), of alerts and of incidents.
- Field-level encryption of personal data (ADR-0006 §Out of scope); user names inside image paths travel as before.
- An operator setting for the retention or the pattern list (the deployment contract, roadmap §F).

## Open questions

1. **Command lines beyond 1024 characters.** Whether a second read of the full line, from the running process, is worth its race for long-lived processes. Reopen: a rule that needs text past the limit.
2. **The system-logger limit in the field** (at most eight such sessions, two reserved, §Context 10). Reopen: an agent that fails to start for it.
3. **The user in the `dedup_key` and the incident key.** Reopen: a retention for alerts and incidents (§4).
4. **Crash dumps of the agent and memory dumps of the host.** Whether a deployment keeps or sends them, with the memory they hold. Reopen: the deployment contract (roadmap §F).

One session for all the providers, an open question of this ADR's draft, was measured on 2026-10-12 (§Context 10) and is A13.

## Decision record

- Manuel ratified this ADR and its six co-located amendments (ADR-0006, ADR-0009, ADR-0011, ADR-0012, ADR-0019 and the threat model) in chat on 2026-10-13 ("sí a ADR-0020 y a sus seis enmiendas").
- Manuel, 2026-10-11, in chat, on the source (§1): option 1, the kernel's process events ("creo que lo mejor ligado a lo que es cyberguard y destinado a ser vendido a empresas … es la primera opcion"); after the third probe showed they carry no path, the form joined to Kernel-Process ("sigo tu recomendacion sobre esa decision").
- Manuel, 2026-10-11, on the command line (§2): option B, the text as delivered with secrets replaced on the agent, the markers of "redacted" and "possibly truncated", and 365 days ("opcion B si es la recomendacion, vamos a intentar lllegar a la perfeccion total en el proyecto").
- Manuel, 2026-10-11, on the user (§3): the SID only, the field renamed before it carries data ("1 + i"). The names proposed to him were `process.user_sid` and the column `process_user_sid`; the review of this ADR showed the newer classes' OCSF names on the wire, and they became `process.user.uid` and `process_user_uid` (A8).
- Manuel, 2026-10-12, in chat: the threat model gains the asset this ADR adds, co-located (`docs/security/threat-model.md`, Amendment 2026-10-13); and the rules over the two fields are a phase of their own right after §B2, roadmap §B3 ("sigo tu recomendacion sobre esa decision", read as accepting both recommendations).
- Manuel, 2026-10-12, in chat, after the measurement of §Context 10, on the builds the agent runs on: Windows 10 and Windows Server 2019 included (the option he chose: "Incluir Win10 y Server 2019 (Recomendado)"), so the kernel's events come from a second session (§1, A13). The options put to him were that, a floor of Windows 11 and Windows Server 2022 with one session, and measuring one session of the kernel's flags beside the providers first.
- Claude Code, under this ratification: the join completed by the callbacks through a slot the Launch carries, the table outside the ring, its bounds, the kernel session opened after the existing one, and its end and rundown records ignored (§1, against A10); the matching with the standard library, and the replacement on the drain path (§2); exiting rather than degrading when the kernel session cannot open (§1, against A9); the third marker, its default, and no time-to-live for the markers (§2, against A12); the three markers projected to detection (§4); an empty command line and no user on a Terminate (§2); the OCSF form of the name and the relaxed `user.json` (§3, against A8); no major version for the rename, and ADR-0006's amendment that allows it while CGES is 0.x (§3); the SID's retention of 365 days (§3); no user in the `dedup_key` or the incident key until they have a retention (§4, against A11); the parent at the edge as its own contract (§5).

## Amendment 2026-10-23 (ADR-0022): its two items for roadmap §F go with SPEC-021

**What surfaced it.** ADR-0022 and SPEC-023, roadmap §F's first step. This ADR gives roadmap §F two items: §Decision §4's ClickHouse settings that keep row data out of the server's logs and system tables, those that are settings of the server; and §Out of scope's operator setting for the retention of the command line and the user SID, or for the pattern list. Both bear on the two columns of class 1007 that SPEC-021 is to fill, and roadmap §B2, which SPEC-021 belongs to, is parked: no code captures the command line or the user SID, and SPEC-021, which states the settings (§Decision §4) and the closed list (§Decision §2), is not written.

**The amendment.** The two items join the deployment contract with SPEC-021's code, not with roadmap §F's first SPEC: the bundle's ClickHouse configuration takes the settings SPEC-021 states, the operator surface takes the retention and pattern-list settings, and the api's ClickHouse user is narrowed to the columns its routes return. Until then the two columns stay empty. The bundle keeps the server's system log tables for seven days (SPEC-023 §Operational §4), which bounds how long a failed insert's row, of any class, stays in them, and is not this ADR's setting.

**Effect.** §Decision: the last sentence of §4's second bullet (`:73`), as the amendment says; the rest stands. §Consequences: the line that gives users with scoped grants to roadmap §F (`:167`) holds for the api's ClickHouse user as a whole, a read-only user SPEC-023 creates; its narrowing to columns goes with SPEC-021's code. §Out of scope: its operator setting goes with SPEC-021. §Compliance: the 365 days of the two columns, a backup included (`:184`), hold; SPEC-021 reconciles them with the bundle's backups, which the operator is asked to keep no longer than the longest retention set (SPEC-023 §Operational §10, against the retentions of §Operational §9: 365 days by default, up to 3650 by §Data contracts §3), and until then the two columns are empty. §Alternatives considered: none. Proposed by Claude Code on 2026-10-23 under Manuel's delegation (ADR-0022 §Decision record), and ratified by Manuel on 2026-10-23 with ADR-0022, in chat: "Sí, ratifico". Status stays `Accepted`.

## References

- [ADR-0003](0003-polyglot-storage.md) — ClickHouse for events; the retention table (debt #47).
- [ADR-0006](0006-cges-ocsf-alignment.md) — the `cg_*` extensions; OCSF naming; encryption deferred; amended co-located (changes while CGES is 0.x keep the version).
- [ADR-0008](0008-etw-crate-selection.md) — ferrisetw and its side-channel helper.
- [ADR-0009](0009-event-delivery-and-buffer.md) — the dispatch callback and the ring; amended co-located (what a callback may do).
- [ADR-0010](0010-agent-privilege-model-mvp.md) — the elevated token.
- [ADR-0011](0011-cges-process-activity-v0-1.md) — the Process Activity mapping; amended co-located.
- [ADR-0012](0012-normalize-before-correlate-pipeline.md) — the read-model, §3's flag, §5's `dedup_key`; amended co-located.
- [ADR-0015](0015-readonly-clickhouse-reader-in-api.md) — the api's reader, read-only in use.
- [ADR-0016](0016-forensic-evidence-hash-chain.md) — the evidence an alert seals.
- [ADR-0018](0018-cges-network-activity-v0-1.md) — the second provider in one session; readers select by class (§9).
- [ADR-0019](0019-cges-authentication-v0-1.md) — the logons that carry a person's name; the 365 days; amended co-located (the other classes' time-to-live).
- [SPEC-005](../specs/SPEC-005-agent-process-telemetry-windows-etw.md), [SPEC-006](../specs/SPEC-006-detection-mvp.md), [SPEC-007](../specs/SPEC-007-incident-grouping-mvp.md), [SPEC-010](../specs/SPEC-010-forensic-event-drill.md), [SPEC-015](../specs/SPEC-015-detection-evaluator-generalization.md), [SPEC-016](../specs/SPEC-016-detection-rule-set-v1.md), [SPEC-017](../specs/SPEC-017-agent-capture-normal-run-path.md), [SPEC-018](../specs/SPEC-018-detection-read-model-arrival-cursor.md), [SPEC-019](../specs/SPEC-019-agent-network-telemetry-windows-etw.md), [SPEC-020](../specs/SPEC-020-agent-logon-telemetry-windows-security-log.md) — capture, incidents, the drill, the evaluator, the rules, delivery, the read, the precedents of a second and a third source.
- [roadmap](../product/roadmap.md) — §B2; §B3, the rules over the two fields; Part (b) §8 and §9.
- [threat model](../security/threat-model.md) — §1 and §8; amended co-located (the command lines and SIDs as assets).
- [handoff-36](../handoff-session-36.md), [handoff-37](../handoff-session-37.md) — §Progress, the measurements.
- Microsoft Learn, [System Providers](https://learn.microsoft.com/en-us/windows/win32/etw/system-providers) and [Configuring and Starting a SystemTraceProvider Session](https://learn.microsoft.com/en-us/windows/win32/etw/configuring-and-starting-a-systemtraceprovider-session) — the build that brings system providers; the logger sessions that receive the kernel's events.
