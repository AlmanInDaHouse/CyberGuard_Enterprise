# ADR-0019: Per-class CGES jurisprudence — Authentication v0.1 (Security log source, logons succeeded and failed, people's accounts, the name of a failed logon)

- Status: Accepted
- Date: 2026-10-10
- Last updated: 2026-10-10
- Deciders: Manuel (project owner), Claude Code (proposal and implementation)

## Context

Roadmap §D is MVP criterion 2: the agent captures network and logins besides processes. The network half landed in S35 (ADR-0018, SPEC-019). Observed at `850a3fe` and on Manuel's machine (Windows 11 Home 10.0.26200, in a workgroup) on 2026-10-10; the measurements on the machine are summarised, redacted, in `docs/handoff-session-35.md` §Logon facts:

1. **The class exists only as a schema.** `schemas/cges/v0.1/classes/3002_authentication.json` and the example `03_auth_login_success.json` are in the repo. No code emits, stores or reads class 3002; one ingest test uses it as a class the route refuses (`services/ingest/test/net-ac-002-validation.test.ts`). ADR-0012 §Context calls it schema-only. The class requires `user`, whose object requires `name`; `src_endpoint` is a network endpoint, which requires `ip`.
2. **The pattern is set.** ADR-0011 §1 gives each concrete CGES class its own jurisprudence ADR, and ADR-0018 §Compliance asks the next one to store its class in `cges_events` unless it argues otherwise. This ADR is the third instance; SPEC-020 is its SPEC.
3. **Where Windows records logons.** In the Security log: provider `Microsoft-Windows-Security-Auditing`, channel `Security`. Read from the provider's manifest without elevation: event 4624 (an account successfully logged on) has versions 0 to 3, and version 3 carries 28 fields — the subject (`SubjectUserSid`, `SubjectUserName`, `SubjectDomainName`, `SubjectLogonId`), the target (`TargetUserSid`, `TargetUserName`, `TargetDomainName`, `TargetLogonId`), `LogonType`, `LogonProcessName`, `AuthenticationPackageName`, `WorkstationName`, `LogonGuid`, `TransmittedServices`, `LmPackageName`, `KeyLength`, `ProcessId`, `ProcessName`, `IpAddress`, `IpPort`, `ImpersonationLevel`, `RestrictedAdminMode`, `RemoteCredentialGuard`, `TargetOutboundUserName`, `TargetOutboundDomainName`, `VirtualAccount`, `TargetLinkedLogonId` and `ElevatedToken`; `ElevatedToken` exists from version 2. Event 4625 (an account failed to log on) has one version with 21 fields, among them `Status`, `FailureReason` and `SubStatus` and no target logon id.
4. **Who can read it.** Unelevated, reading the Security log is denied. An elevated Administrator token, which is the agent's under ADR-0010 §1, reads it. The channel's access list is `O:BAG:SYD:(A;;0xf0005;;;SY)(A;;0x5;;;BA)(A;;0x1;;;S-1-5-32-573)`: SYSTEM, Administrators and Event Log Readers may read. `SeSecurityPrivilege` is present and disabled in that token, and reading did not need it.
5. **What the machine's audit policy writes.** Logon: success and failure. Logoff: success. Special logon: success. Other logon/logoff events (lock and unlock), credential validation and the Kerberos subcategories: no auditing. So, on this machine, 4624 and 4625 are written without changing the policy. Whether that is the default on every Windows edition was not checked.
6. **What a workstation writes** (151.5 hours of one machine, read elevated and reported redacted). The Security log is circular, 20 MB, and held 33 444 records, 6.3 days; 64 % were event 5379 and 22 % event 4798. Event 4624: 1532 records, about 243 a day, all version 3; 93 % were logons of type 5 (service) of SYSTEM, and 94 % came from `services.exe`. People's logons were 44, all of one local account in the domain `MicrosoftAccount`: type 11 (cached interactive) by `svchost.exe` from `127.0.0.1`, and type 7 (unlock) by `lsass.exe` without an address. Each was written twice, once with an elevated token and once without, the pair linked by `TargetLinkedLogonId` — the split token of an administrator under UAC. Type 2 appeared only for the system accounts `DWM-n` and `UMFD-n`. Event 4625: none in the window.
7. **What was not measured.** A failed logon on this build — its `Status`, `SubStatus` and `TargetUserSid`; logons of type 3 (network) or 10 (remote interactive); a domain-joined host, which also writes logons of domain and computer accounts; an Entra ID account (`S-1-12-1-…`); whether the Security-Auditing provider can be received by an ETW session of the agent's; the delay from a logon to its record reaching a subscriber. §10 says how the ones this ADR relies on are settled.

## Decision

### 1. Source — the Security log, through the Windows Event Log API

The agent subscribes to the `Security` channel with a query for events 4624 and 4625, receiving only events written after the subscription opens, on a thread of its own beside the ETW session. It renders the values it maps (§4) by name, not the event's XML. The calls are the Event Log functions of `windows-sys`, a dependency the agent already has; it gains the feature that exposes them and no new crate.

- **Why.** The Security log is the documented interface to these records; the agent's token reads it (§Context 4); the measured policy writes both events (§Context 5).
- ADR-0008's rule on raw Win32 calls concerns ETW consumption, and its picture of the ETW helper as the agent's one direct Win32 surface is already incomplete. The Event Log calls are a further surface, authorised here.

### 2. Scope of v0.1 — logons succeeded and failed

The agent emits one event per reported 4624 or 4625. Both are `activity_id` 1 (Logon); `status_id` is 1 (Success) for 4624 and 2 (Failure) for 4625. No other Security-log event is read.

### 3. Whose logons — people's accounts

*Manuel's decision, 2026-10-10. The list of what is not a person's account is Claude Code's, under the delegation recorded in §Decision record.*

- A 4624 whose target is not a person's account is not reported. Those are the targets whose `TargetUserSid` is `S-1-5-18` (SYSTEM), `S-1-5-19` (LOCAL SERVICE) or `S-1-5-20` (NETWORK SERVICE), or begins with `S-1-5-80-` (service SIDs), `S-1-5-82-` (application pools), `S-1-5-83-` (virtual machines), `S-1-5-84-` (user-mode drivers), `S-1-5-90-` (Window Manager, `DWM-n`) or `S-1-5-96-` (Font Driver Host, `UMFD-n`); and computer accounts, whose `TargetUserName` ends in `$`.
- Every other 4624 is reported, whatever its type: network and remote-interactive logons included. A SID does not tell a person's account from a domain account a service runs as, so the second travels too.
- `S-1-5-7` (ANONYMOUS LOGON) is reported although it is no person's account: it carries no data about a person, and its network logons — null sessions — are a reconnaissance signal a SOC expects to see.
- Every 4625 is reported, whatever its account: a failed logon is where an attack against accounts shows.
- **Why.** On the measured workstation the excluded targets were 1488 of 1532 records; the 44 left are the logons of a person. Computer accounts and the virtual accounts of services, application pools and drivers are no person's and are frequent on servers.

### 4. Field mapping

*Which data about the person: Manuel's decision, 2026-10-10.*

| CGES | Source | Rule |
|---|---|---|
| `event_id` | — | UUIDv7 generated at capture (ADR-0009 §1). |
| `category_uid`, `class_uid`, `activity_id` | — | 3, 3002, 1. |
| `status_id` | the event id | 1 for 4624, 2 for 4625. |
| `time` | `System/TimeCreated` | String-encoded Unix nanoseconds, the encoding of ADR-0011's Amendment 2026-05-28 and ADR-0018 §6. |
| `user.uid` | `TargetUserSid` | The SID in its string form. |
| `user.name`, `user.domain` | `TargetUserName`, `TargetDomainName` | As written, except a failed logon's under §5. |
| `logon_type_id` | `LogonType` | OCSF's enumeration; SPEC-020 fixes the table. |
| `auth_protocol` | `AuthenticationPackageName` | As written (`Negotiate`, `NTLM`, `Kerberos`, …); SPEC-020 sets `auth_protocol_id`. |
| `src_endpoint.ip`, `src_endpoint.hostname` | `IpAddress`, `WorkstationName` | Only when `IpAddress` is an address; `hostname` only when `WorkstationName` is neither `-` nor empty. |
| `status_code`, `status_detail` | `Status`, `SubStatus` | 4625 only, as the hexadecimal text the event writes. |
| `cg_elevated_token` | `ElevatedToken` | 4624 version 2 or later: `true` for `%%1842`, `false` for `%%1843`; omitted on earlier versions, which have no such field. |

- No other field is reported: not the subject, the logon ids, the logon GUID, the process, the logon process, `IpPort`, `LmPackageName`, `KeyLength`, `TransmittedServices`, `ImpersonationLevel`, `RestrictedAdminMode`, `RemoteCredentialGuard`, the outbound names, `VirtualAccount` or `FailureReason`. No raw payload.
- A logon without an address — a local unlock, for instance — has no `src_endpoint`: its workstation is the host the reporting agent runs on, which the row's `agent_id` identifies. The network endpoint object requires `ip` (§Context 1).
- `cg_elevated_token` is a CGES extension (ADR-0006 §Extensions): this ADR maps the token to no OCSF field, and SPEC-020 documents the extension in the class file as ADR-0006 requires.

### 5. The name of a failed logon

*Manuel's decision, 2026-10-10: the name is kept only when the account exists.*

In a 4625, `TargetUserName` and `TargetDomainName` are what was submitted, and a person sometimes types a password into the name field. `TargetUserSid` does not say whether the account exists: a failed logon creates no token, and Windows commonly writes the null SID (§Context 7, settled in §10). The failure code says it.

- The agent keeps `user.name` and `user.domain` only when `SubStatus` — or `Status`, when `SubStatus` is `0x0` — is a code that names an existing account, compared without regard to case: `0xC000006A` (wrong password), `0xC0000234` (locked out), `0xC0000072` (disabled), `0xC000006F` (outside logon hours), `0xC0000070` (workstation not allowed), `0xC0000193` (account expired), `0xC0000071` (password expired), `0xC0000224` (password must change), `0xC000015B` (logon type not granted).
- Any other code — `0xC0000064` (no such user) and every code this list does not name — replaces both with the marker `<withheld>`, which no Windows account name can be: `<` and `>` are not allowed in one.
- The failure itself, its codes, its logon type and its authentication package are always reported, and its source whenever the event has an address: a burst of failures stays visible.
- **No length is kept.** The option Manuel chose read "a marker and its length"; the marker goes alone, because the length of a password typed by mistake narrows a guess, and detection gains nothing from it. Claude Code's, under the delegation recorded in §Decision record.

### 6. Linked logons

An administrator's logon under UAC is written twice, with an elevated and an unelevated token (§Context 6). The agent reports both, told apart by `cg_elevated_token`; it does not pair them. On 4624 versions before 2 the two cannot be told apart.

### 7. Delivery and storage

- The events join the ring, the batch, the envelope and the retry of the other classes (SPEC-017, SPEC-019); the ring's capacity and the batch triggers are unchanged and shared. The subscription's thread is a second producer into the ring, which ADR-0009 describes as fed by the ETW callback.
- D6 is extended to this class: Authentication events are rows of `cges_events`, told apart by `class_uid`, and the table gains the columns the class needs, with defaults; `src_ip` is the column SPEC-019 added. SPEC-020 lists them.
- The obligation of ADR-0018 §9 holds with its exception: a reader that interprets a row's `activity_id` or class-specific columns selects by `class_uid`, and the forensic drill, which does not, stays correct while every event an alert cites is of one class. No alert cites an Authentication event until a rule over class 3002 exists; before one does, the drill's row must carry `class_uid` (ADR-0018 §9).
- The server is upgraded before the agents, as SPEC-019 §Operational §9 rules: today the route refuses class 3002.

### 8. Lifecycle and the agent's own output

- **Live only.** The subscription receives events written after it opens. A logon while the agent is not running is not reported, and no position in the log is kept on disk: a persistent cursor is agent state that ADR-0009 defers with the disk buffer, and the log's retention (6.3 days measured) bounds what one could recover.
- **No fewer sources.** The subscription opens at startup with the ETW session. If it cannot open, the agent does not start: it does not run with fewer sources, as SPEC-019 §Operational §1 rules for a session that cannot start. SPEC-020 fixes the exit code and the message, beside those SPEC-017 §Operational §1 gives the session, and places the thread in the shutdown of SPEC-017 §Operational §4.
- **No logon data in the agent's logs.** The agent's own output names no user, SID, domain, workstation or address of a logon; it may count them.

### 9. The data about people, once stored

- **Retention: 365 days from arrival.** A row of class 3002 is deleted 365 days after its `arrived_at`, by a time-to-live on `cges_events` that applies to that class alone; SPEC-020 writes it. The server's arrival time is used, not the event's `time`, which the agent's clock sets and the ingest route does not bound (debt #31).
  - **Why.** Data about people needs a finite, documented period (GDPR, storage limitation). Twelve months covers the longest audit requirement a buyer commonly brings — PCI DSS asks for a year of audit history — and the investigation of an intrusion found late.
  - **Not chosen.** No limit, which no data-protection officer accepts; 90 days, which fails the buyer who must keep a year.
  - **Who sets it.** Fixed until the deployment contract (roadmap §F) makes retention an operator's setting (Open questions 2).
  - **The other classes** keep no time-to-live: ADR-0003's retention table is not implemented, a debt this ADR does not settle.
  - Claude Code's, under the delegation recorded in §Decision record.
- **Access.** This ADR adds no access control of its own. The read path that exists, the forensic drill (ADR-0015), reads the events alerts cite, and no alert cites class 3002. A product surface that shows logons — a detection rule, an API, a dashboard view — is a later SPEC, and its access to this data is decided there. The first rule that cites a logon also reconciles this retention with the evidence an alert seals (ADR-0016).

### 10. Settled at the gate

SPEC-020's elevated acceptance criteria fix by outcome the facts of §Context 7 that this ADR relies on:

- A failed logon for a name that does not exist and one for an existing account with a wrong password produce 4625s with the codes §5 relies on; the name is withheld in the first and kept in the second.
- A successful logon of a person's account is reported with its SID, name, domain and type, and a system account's is not.
- If the gate contradicts a statement of this ADR — the codes, or the null SID — this ADR is amended before the code lands.
- **No test account.** The gate creates no account on the host. A failed logon of an existing account is attempted against the built-in Administrator (RID 500), which every Windows has and which is disabled by default, so no person's account is touched; a failed logon of a name that does not exist uses a random name; a successful logon of a person's account is a network logon to the loopback with the current credentials. SPEC-020 writes the criteria. Claude Code's, under the delegation recorded in §Decision record.

## Alternatives considered

### A1 — The Security-Auditing provider through ETW

One session for every class would be simpler. Not taken: whether the agent's session can receive that provider was not measured (§Context 7), and the Security log is the documented interface to these records. Revisit only if the subscription proves unworkable.

### A2 — Polling the log

A timer and a query by record number instead of a subscription. Rejected: added latency, and the agent would track the last record it read, which the subscription does for it.

### A3 — Every 4624

*Manuel's decision.* Rejected: about 243 records a day on the measured workstation, 93 % service logons of SYSTEM, for the logons of a person that are 3 % of them.

### A4 — The whole event

*Manuel's decision.* Rejected: more data about people and more volume, for fields no consumer reads.

### A5 — A failed logon's name always, or its hash

*Manuel's decision.* The name always would store passwords typed by mistake on the server. A hash would group attempts by name without the text, at the price of a key per organisation and its distribution.

### A6 — Recovering the logons missed while the agent was down

Rejected for v0.1: a cursor on disk is agent state that ADR-0009 defers; §8.

### A7 — A crate for the Event Log API

Rejected: a new third-party dependency of an agent that runs elevated, for calls `windows-sys` already exposes.

### A8 — Pairing linked logons

Rejected for v0.1: the agent would hold state across records and choose between two tokens.

## Consequences

### Positive

- No change to the privilege, no new crate, and none to the measured audit policy: the agent's token reads the log, `windows-sys` gains a feature, and the machine's policy writes both events.
- Only logons of people's accounts travel, with the service accounts a SID does not tell from them: about 3 % of 4624 on the measured workstation, plus every failure.
- A password typed into a name field never leaves the host.
- Data about people has a finite, documented retention (§9), and anonymous logons are visible.

### Negative

- A second capture source, not ETW, with its own thread, failure modes and tests, and a second producer into the ring.
- A burst of failed logons shares the ring with the other classes and can push process events out of it, as connections can (ADR-0018 §Consequences).
- A logon while the agent is not running is lost.
- An administrator's logon under UAC is two events.
- A withheld name hides which nonexistent accounts an attack tries; only the count, the codes and, when the event has one, the source remain.
- A host whose audit policy turns logon auditing off produces no events, and the agent cannot tell that from a host where nobody logged on.
- Logon data is deleted after 365 days, whatever an investigation still needs; the other classes keep no limit at all (§9).
- The measured shares (3 %, 243 a day) come from one workgroup machine; a domain-joined host was not measured.

### Neutral

- SPEC-020 adds to the class file `3002_authentication.json` the properties it emits (`auth_protocol`, `auth_protocol_id`, `status_code`, `status_detail`, `cg_elevated_token`), and updates the description of `time` in `event.json` to name 3002, as SPEC-019 did for 4001; the class's example keeps ISO 8601, as debt #39 records for the others.
- SPEC-020 turns net_ac_002's refusal of class 3002 into acceptance.
- Logoff, explicit credentials, special privileges, credential validation and the lock and unlock events (4800, 4801) stay out; an unlock is reported through the 4624 of type 7 it writes.

## Compliance

- The agent's Authentication emission MUST follow §2–§8. Another event, account set or field needs an amendment to this ADR or a successor ADR, not agent code alone.
- The agent MUST NOT emit a failed logon's name or domain unless §5 allows it.
- The agent MUST NOT write a logon's user, SID, domain, workstation or address to its own output.
- A reader that interprets a row's `activity_id` or class-specific columns MUST select by `class_uid`; the forensic drill is excepted on the condition of ADR-0018 §9 (§7).

## Out of scope

- Logoff (4634, 4647), explicit credentials (4648), special privileges (4672), credential validation (4776), the lock and unlock events (4800, 4801), and the Kerberos events.
- Managing or reporting the audit policy (Open questions 1).
- Logons that happened while the agent was not running (§8).
- Detection rules, API and dashboard views over Authentication, and their access rules (§9).
- Directory-service and domain-controller logs; capture on non-Windows platforms (ADR-0002 Rule 2).

## Open questions

1. **A host that does not audit logons.** Whether the agent should read the audit policy at startup and say so. Reopen: the deployment contract (roadmap §F).
2. **Retention as an operator's setting.** The 365 days of §9 are fixed. Reopen: the deployment contract (roadmap §F).

## Decision record

- Manuel ratified this ADR in chat on 2026-10-10 ("si").
- Manuel, 2026-10-10, in chat: the logons reported are people's accounts and every failure (§3); the fields are SID, name, domain, logon type, source, authentication package and elevated token (§4); a failed logon keeps its name only when the account exists (§5).
- Manuel delegated the four points left open in the draft on 2026-10-10 ("esas 4 decídelas tú en factor de lo más beneficioso en el contexto del proyecto y de cara a que sea producto vendible a una empresa"), for a product sold to companies. Claude Code decided: the retention of 365 days from arrival (§9, against no limit and 90 days); the marker without a length (§5); ANONYMOUS LOGON reported, computer and virtual accounts not (§3); no test account on the host (§10).
- Claude Code, under ratification: the Security log through the Event Log API (§1, against A1 and A2); live only (§8, against A6); both tokens of a linked logon (§6, against A8); the code allow-list as the test of "the account exists" (§5, against the SID, which a 4625 is not known to fill).

## References

- [ADR-0002](0002-language-per-component.md) — Windows-only capture in the MVP (Rule 2).
- [ADR-0003](0003-polyglot-storage.md) — ClickHouse for events; the retention table (§9).
- [ADR-0006](0006-cges-ocsf-alignment.md) — CGES alignment with OCSF v1.3 and the `cg_*` extensions.
- [ADR-0008](0008-etw-crate-selection.md) — ferrisetw and its rule on raw Win32 calls for ETW.
- [ADR-0009](0009-event-delivery-and-buffer.md) — delivery, the in-memory buffer, the deferred disk buffer.
- [ADR-0010](0010-agent-privilege-model-mvp.md) — the elevated-user posture.
- [ADR-0011](0011-cges-process-activity-v0-1.md) — the per-class pattern; the `time` encoding.
- [ADR-0012](0012-normalize-before-correlate-pipeline.md) — class 3002 as schema-only; the detection read-model.
- [ADR-0015](0015-readonly-clickhouse-reader-in-api.md) — the forensic drill's read path.
- [ADR-0016](0016-forensic-evidence-hash-chain.md) — the evidence an alert seals (§9).
- [ADR-0018](0018-cges-network-activity-v0-1.md) — the previous per-class ADR; storage in `cges_events` and the reader's obligation (§9).
- [SPEC-017](../specs/SPEC-017-agent-capture-normal-run-path.md) — startup, exit codes, delivery and shutdown.
- [SPEC-019](../specs/SPEC-019-agent-network-telemetry-windows-etw.md) — the shared ring and table; the upgrade order.
- [handoff-session-10.md](../handoff-session-10.md) — decision D6, one `cges_events` table.
- [handoff-session-35.md](../handoff-session-35.md) — §Logon facts, the redacted measurements.
- [roadmap](../product/roadmap.md) — §D.
- `schemas/cges/v0.1/classes/3002_authentication.json`, `schemas/cges/v0.1/objects/user.json`, `schemas/cges/v0.1/objects/network_endpoint.json`, `schemas/cges/v0.1/event.json` — the schema this ADR maps onto.
