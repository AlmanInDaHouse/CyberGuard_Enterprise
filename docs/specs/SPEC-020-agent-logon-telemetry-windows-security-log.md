# SPEC-020: Agent logon telemetry — Windows Security log

- **ID:** SPEC-020
- **Title:** Agent logon telemetry — Windows Security log
- **Status:** Accepted
- **Depends on:**
  - ADR-0019 — the decisions this SPEC implements: the source, the logons reported, the fields, the name of a failed logon, live capture, storage and retention, and the gate's logons.
  - SPEC-019 — the shared ring, batch and table it extends to a third class. Amends **by scope** its §Data contracts "Event element" and §Operational §6 (an element is one of three shapes, not two), its §Operational §5 and NFR-019-002 (the ring and its triggers are shared by three classes), and its net_ac_002, whose first refused element now carries a class that no shape accepts.
  - SPEC-017 — amends **by scope** its §Data contracts "Event element" (three shapes), its §Operational §1 (startup also opens the logon subscription, with its own failures) and its §Operational §4 (shutdown also stops it).
  - SPEC-005 — amends **by scope** its §Failure modes: exit code 9 also means insufficient privilege to read the Security log (§Operational §6). Its AC-006 empty-name rule and its FILETIME conversion (§Operational §1) are relied on, unchanged.
  - SPEC-018 — `arrived_at` and the detection read-model, unchanged: the read-model selects class 1007.
  - ADR-0009, ADR-0010 and ADR-0011 — delivery and buffer, the privilege model, the `time` encoding; all unchanged.
  - `docs/product/roadmap.md` — §D, the phase whose login half this SPEC delivers.
- **Authors:** Manuel (project owner), Claude Code (design and implementation)

## Context

ADR-0019 decides how the agent reports logons. Observed at `63d3f8d`:

1. **No code handles class 3002.** The agent emits 1007 and 4001 (`agent/cg-agent/src/etw/`, `cges/emit.rs`); the server's element schema is a union of those two, and net_ac_002 posts an element with `class_uid` 3002 as one the route refuses (`services/ingest/src/schemas.ts`, `services/ingest/test/net-ac-002-validation.test.ts`).
2. **The agent has one capture source.** `open_platform_capture` opens the ETW session and returns its ring (`agent/cg-agent/src/lib.rs`); the shutdown stops that session before the final POST. A failed start maps to exit code 9 or 1 through `EtwError` (`errors.rs`, `startup.rs`).
3. **The Event Log functions are not enabled.** `windows-sys` 0.59 is a Windows dependency of the agent without the `Win32_System_EventLog` feature (`agent/cg-agent/Cargo.toml`).
4. **The table has no retention.** `cges_events` has no time-to-live (`services/ingest/src/db/migrate.ts`); its `arrived_at` is server-assigned (SPEC-018).
5. **Every row sets the process columns that have no default** — `process_pid`, `process_uid`, `process_name`. A 4001 row fills the first two from its actor and writes `process_name` as `''` (`routes/heartbeat.ts`).
6. **The route logs an insert failure with the database's message** (`routes/heartbeat.ts`), and ClickHouse quotes the offending row in that message.
7. **What the Security log holds on the measured machine** is in `docs/handoff-session-35.md` §Logon facts: 4624 version 3, no 4625 in 6.3 days, and an effective audit policy that logs logon success and failure.

## Scope

### In scope

- A logon subscription in the agent beside the ETW session: the `Security` channel, events 4624 and 4625, live only (§Operational §1).
- The decoding: the accounts reported, the name of a failed logon, the field mapping (§Operational §2–§4).
- The ring, the batch and the envelope carry a third class (§Operational §5).
- Startup and shutdown with a second source (§Operational §6).
- The server validates, converts and stores class 3002; `cges_events` gains eleven columns and a time-to-live for class 3002 (§Data contracts, §Operational §7–§8).
- The CGES schema files follow: `classes/3002_authentication.json` gains the properties this SPEC emits, and the description of `time` in `event.json` names class 3002.
- The tests of §Acceptance criteria: in CI, and in the elevated gate.

### Out of scope

Each item has its destination in brackets.

- Logoff, explicit credentials, special privileges, credential validation, lock and unlock events, the Kerberos events [ADR-0019 §Out of scope].
- Logons that happened while the agent was not running [ADR-0019 §8].
- Reading or reporting the audit policy [ADR-0019 Open questions 1, roadmap §F].
- Retention as an operator's setting, and retention of the other classes [ADR-0019 Open questions 2 and debt #47].
- Detection rules over logons, and any API or dashboard view of them, with their access rules [a later SPEC; ADR-0019 §9].
- Validating or bounding an event's `time` [debt #31].
- Aligning the CGES examples with the wire [debt #39].
- Capture on non-Windows platforms [ADR-0002 Rule 2].

## Data contracts

### Event element

Amends SPEC-017 and SPEC-019 §Data contracts "Event element" by scope. An element of `body.events` is one of three shapes, told apart by `class_uid`; an element whose `class_uid` is none of them is invalid. 1007 and 4001 are unchanged (SPEC-017, SPEC-019), and:

- **`class_uid` 3002** — a logon succeeded or failed (ADR-0019 §2–§6):

```json
{
  "event_id": "0199d2a4-5b7c-7e1a-9c3d-2f4a6b8c0d1f",
  "class_uid": 3002,
  "category_uid": 3,
  "activity_id": 1,
  "time": "1791625812345678900",
  "status_id": 2,
  "user": { "uid": "S-1-0-0", "name": "<withheld>", "domain": "<withheld>" },
  "logon_type_id": 3,
  "auth_protocol": "NTLM",
  "auth_protocol_id": 1,
  "src_endpoint": { "ip": "127.0.0.1", "hostname": "WS-0042" },
  "status_code": "0xc000006d",
  "status_detail": "0xc0000064"
}
```

- `event_id` is a UUIDv7 generated at capture. `time` is string-encoded Unix nanoseconds. `category_uid` is `3`, `activity_id` `1`.
- `status_id` is `1` for a 4624, `2` for a 4625.
- `user.uid` is `TargetUserSid` in string form. `user.name` and `user.domain` are `TargetUserName` and `TargetDomainName` as written, `-` when the event has no value, or `<withheld>` under §Operational §3.
- `logon_type_id` is an integer from 0 to 99 (§Operational §4).
- `auth_protocol` is `AuthenticationPackageName` as written, `-` when the event has no value; `auth_protocol_id` is its OCSF code, one of 0, 1, 2 and 99 (§Operational §4).
- `src_endpoint` is present only when `IpAddress` is an address; `ip` is a valid IPv4 or IPv6 address in text, and `hostname`, present only when `WorkstationName` is neither `-` nor empty, is a non-empty string. There is no `port`.
- `status_code` and `status_detail` are present on a failure and only there: `Status` and `SubStatus` as lowercase hexadecimal text with the `0x` prefix (`0x0` for zero or absent).
- `cg_elevated_token` is a boolean, present on a success whose record carries `ElevatedToken` as `%%1842` or `%%1843`, and only there.
- A resent element is byte-identical, as SPEC-017 §Data contracts requires of every element.

### Storage

`cges_events` gains eleven columns. Its engine, partitioning, ordering and existing columns are unchanged.

| Column | Type | 3002 row | 1007 and 4001 rows |
| --- | --- | --- | --- |
| `user_uid` | `String DEFAULT ''` | `user.uid` | `''` |
| `user_name` | `String DEFAULT ''` | `user.name` | `''` |
| `user_domain` | `String DEFAULT ''` | `user.domain` | `''` |
| `logon_type_id` | `UInt8 DEFAULT 0` | `logon_type_id` | `0` |
| `status_id` | `UInt8 DEFAULT 0` | `status_id` | `0` |
| `status_code` | `String DEFAULT ''` | `status_code`, or `''` | `''` |
| `status_detail` | `String DEFAULT ''` | `status_detail`, or `''` | `''` |
| `auth_protocol` | `String DEFAULT ''` | `auth_protocol` | `''` |
| `auth_protocol_id` | `UInt8 DEFAULT 0` | `auth_protocol_id` | `0` |
| `src_hostname` | `String DEFAULT ''` | `src_endpoint.hostname`, or `''` | `''` |
| `elevated_token` | `Nullable(Bool) DEFAULT NULL` | `cg_elevated_token`, or `NULL` | `NULL` |

A 3002 row also sets the columns every class shares: `agent_id`, `org_id`, `event_id`, `class_uid` (3002), `activity_id` (1), `time`; `src_ip` from `src_endpoint.ip`, or `''`; and the process columns without a default as `process_pid` `0`, `process_uid` `''`, `process_name` `''`.

**Retention.** `cges_events` carries a time-to-live that deletes a row of class 3002 once its `arrived_at` is 365 days old — at the first merge of its part after that, so possibly some hours later — and no other row (ADR-0019 §9):

```sql
TTL toDateTime(arrived_at) + toIntervalDay(365) DELETE WHERE class_uid = 3002
```

## Operational

### 1. Capture

- The agent subscribes to the `Security` channel with the query `*[System[(EventID=4624 or EventID=4625)]]`, receiving only events written after the subscription opens. No bookmark is kept.
- The subscription is pulled: a thread of the agent's own waits on the subscription's signal and on a stop signal, and on each wakeup reads the available records in batches until none is left.
- Each record is rendered through one values context, by name: `EventID`, `Version` and `TimeCreated/@SystemTime` from `System`; `TargetUserSid`, `TargetUserName`, `TargetDomainName`, `LogonType`, `AuthenticationPackageName`, `WorkstationName`, `IpAddress`, `ElevatedToken`, `Status` and `SubStatus` from `EventData`. A value the record does not have renders as absent.
- A record that cannot be rendered, that has no `TargetUserSid`, or whose event id is neither 4624 nor 4625, is dropped and counted. The thread logs an increase of that count at `warn`, at most once per 60 s, with the first such record's event id when it is known and the Win32 code when there is one; nothing else of the record.
- A failed read of the subscription is logged at `warn` with its Win32 code, at most once per 60 s. The thread then closes the subscription and, 5 s later, opens it again from new events; records written in between are lost and not counted.

### 2. The accounts reported

- A 4624 is not reported when its `TargetUserSid` is `S-1-5-18`, `S-1-5-19` or `S-1-5-20`, or begins with `S-1-5-80-`, `S-1-5-82-`, `S-1-5-83-`, `S-1-5-84-`, `S-1-5-90-` or `S-1-5-96-`; or when its `TargetUserName` ends in `$`. It is not counted: it is not an event.
- Every other 4624 is reported, `S-1-5-7` (ANONYMOUS LOGON) included.
- Every 4625 is reported.

### 3. The name of a failed logon

- The code tested is `SubStatus`, or `Status` when `SubStatus` is absent or zero.
- When that code is one of `0xC000006A`, `0xC0000234`, `0xC0000072`, `0xC000006F`, `0xC0000070`, `0xC0000193`, `0xC0000071`, `0xC0000224` or `0xC000015B`, compared as numbers, `user.name` and `user.domain` are as §Data contracts gives them.
- For any other code, `0xC0000064` included, `user.name` and `user.domain` are `<withheld>`, whatever the event wrote — `-` included. No length or other trace of the name is kept.
- `user.uid`, the codes, the logon type, the authentication package and the source are reported as for any failure.

### 4. Field mapping

- **`logon_type_id`.** `LogonType` 0 maps to 1 (System); 2, 3, 4, 5, 7, 8, 9, 10, 11, 12 and 13 map to themselves; any other value, or none, maps to 99 (Other).
- **`auth_protocol_id`.** `NTLM` maps to 1 and `Kerberos` to 2, compared without regard to case; `-`, or no value, to 0 (Unknown); any other package — `Negotiate` among them — to 99 (Other).
- **`src_endpoint`.** Present when `IpAddress`, with any zone suffix (`%` and what follows) removed, parses as an IPv4 or IPv6 address; an IPv4-mapped IPv6 address is written as IPv4, as SPEC-019 does. `hostname` from `WorkstationName` when it is neither `-` nor empty.
- **`cg_elevated_token`.** On a 4624, `%%1842` is `true` and `%%1843` `false`, whatever the record's version; any other value, or none, omits the member. A 4625 never carries it.
- **`time`.** `TimeCreated` converted as SPEC-005 §Operational §1 converts a FILETIME. A record before 1970 is logged at `error` without its user data and dropped.
- **`event_id`.** A UUIDv7 generated when the record is decoded.
- The decoding is a function of the rendered values alone, apart from the `event_id` it generates; it runs and is tested on every platform.

### 5. Ring, batch and delivery

- The logon thread enqueues into the ring the ETW session fills: one ring, three classes, events leaving in the order they entered. Its capacity, the batch triggers and the FIFO drop are unchanged (SPEC-019 §Operational §5).
- The empty-name rule of SPEC-005 AC-006 applies to process events only.
- An event is rendered once, when its batch is formed, in its class's shape. Retry, rejection and liveness are SPEC-017 §Operational §2–§3, unchanged.

### 6. Startup and shutdown

Amends SPEC-017 §Operational §1 and §4, and SPEC-005 §Failure modes, by scope.

- **Startup.** On the secure path on Windows the agent opens the ETW session, then the logon subscription, then enters the delivery loop. If the subscription cannot open, the agent stops the session and exits, writing one line to stderr and to the log at `error`:
  - access denied or privilege not held (Win32 error 5 or 1314): exit code **9** — the code SPEC-005 gives insufficient privilege — with `cg-agent: insufficient privilege to read the Security log; run as elevated user`;
  - any other failure: exit code **1**, with `cg-agent: Security log subscription failed: <code> <message>`.
- On a build without a capture backend there is no subscription either; the single `info` line of SPEC-017 stands.
- **Shutdown.** On the shutdown signal the agent signals the logon thread to stop, waits for it and then closes the subscription; stops the ETW session and waits for its thread; then makes the final POST of SPEC-017 §Operational §4.

### 7. Server

- Structural validation accepts the three shapes of §Data contracts and no other class. For a 3002 element it checks every member that section lists: `event_id` a UUID; `category_uid` 3; `activity_id` 1; `status_id` 1 or 2; `user.uid`, `user.name` and `user.domain` non-empty strings; `logon_type_id` an integer from 0 to 99; `auth_protocol` a non-empty string and `auth_protocol_id` one of 0, 1, 2 and 99; `src_endpoint.ip` an address and `src_endpoint.hostname`, when present, a non-empty string; `status_code` and `status_detail` matching `^0x[0-9a-f]{1,8}$`, present when `status_id` is 2 and absent when it is 1; `cg_elevated_token` a boolean, absent when `status_id` is 2. A member a shape does not list is ignored, as for the other classes. A POST with an invalid element is answered 400 `invalid_request` as a whole.
- The route writes a 3002 element as one row with the mapping of §Data contracts, in the one `INSERT` per POST.
- When the `INSERT` fails, the route logs the database's error code and not its message, for every class: the message can quote a row (§Context 6).

### 8. Storage migration

- The bootstrap adds the eleven columns with an idempotent `ALTER TABLE cges_events ADD COLUMN IF NOT EXISTS`, after the statements of SPEC-018 and SPEC-019. A table that already exists gets them; rows written before read the defaults.
- The bootstrap then sets the time-to-live of §Data contracts with `ALTER TABLE cges_events MODIFY TTL …` and the setting `materialize_ttl_after_modify = 0`, so that rerunning it rewrites no part. Rows of class 3002 did not exist before this SPEC.
- The api's test mirror of the table (`services/api/test/helpers/events-schema.ts`) gains the same columns; the api reads none of them.

### 9. Readers

- The detection read-model is unchanged: it selects class 1007.
- The forensic drill is unchanged: no alert cites a 3002 event (ADR-0019 §7).
- Tests and helpers that read `cges_events` to assert on one class select by `class_uid`.

### 10. Upgrade order

The server is upgraded before the agents (SPEC-019 §Operational §9). A server without this SPEC answers 400 to any POST that carries a logon element, and the agent drops that batch after `max_retries` attempts.

### 11. Logon data in output

- The agent's logs and stderr name no user, SID, domain, workstation or address of a logon; they may count logons and name event ids and Win32 codes (ADR-0019 §8).
- The tests print logon data only redacted: a user as `<user>`, a host or domain as `<host>`, a SID as its prefix and RID (`S-1-5-21-…-1001`), and well-known SIDs as they are. The elevated gate's logs are read off the machine.

## Non-functional requirements

- **NFR-020-001 (logon thread).** The thread renders, decodes and enqueues; it does no network I/O and writes nothing to disk but its log lines (§Operational §11).
- **NFR-020-002 (shared ring).** The ring's capacity (65536), the batch size (1024) and the latency trigger (5000 ms) are unchanged and shared by the three classes.
- **NFR-020-003 (marquee budget).** The logon marquee (auth_ac_013) completes within 45 s and logs its elapsed time on every run.

## Acceptance criteria

Each maps to a test named `auth_ac_NNN_*`, under `agent/cg-agent/tests/` or `services/ingest/test/`.

Server, in `ts-ci` against testcontainers:

- **auth_ac_001 (a mixed POST persists).** A signed POST with one 1007, one 4001 and two 3002 elements — a success with `src_endpoint`, `auth_protocol` and `cg_elevated_token` `true`, and a failure with `<withheld>` names and both codes — is answered 200. Each 3002 row carries the columns of §Data contracts, `elevated_token` `true` on the success and `NULL` on the failure; the 1007 and 4001 rows carry the eleven columns at their defaults; the four rows share one `arrived_at`.
- **auth_ac_002 (validation).** Each of these POSTs is answered 400 `invalid_request` and stores nothing, its valid elements included: a 3002 element with `category_uid` 1; with `activity_id` 2; with `status_id` 3; without `user.uid`; with an empty `user.name`; without `user.domain`; with `logon_type_id` 100; with `auth_protocol_id` 3; without `auth_protocol`; with a `src_endpoint.ip` that is not an address; a success with `status_code`; a failure without `status_detail`; with `status_code` `0xC000006D` (upper case); a failure with `cg_elevated_token`. And net_ac_002's first case now posts a class that no shape accepts.
- **auth_ac_003 (storage migration and retention).** After the bootstrap `cges_events` has the eleven columns with the types and defaults of §Data contracts, on a new table and on a table created without them; a row written before reads the defaults; a second bootstrap leaves the create statement unchanged; that statement carries a time-to-live of 365 days on `arrived_at` restricted to `class_uid = 3002`, compared in the form ClickHouse normalizes it to. In a throwaway database, a 3002 row and a 1007 row whose `arrived_at` is 366 days old and a 3002 row whose `arrived_at` is 364 days old: after `OPTIMIZE TABLE cges_events FINAL`, the first is gone and the other two remain.
- **auth_ac_004 (detection is unaffected).** With a matching parent → child pair of class 1007 stored among 3002 rows of the same agent, a detection cycle evaluates exactly the 1007 rows and writes the one alert the pair raises.
- **auth_ac_005 (no row in the server's log).** The function that builds the fields of the route's insert-failure log line, given a ClickHouse error whose message quotes a row with a distinctive user name, returns the error's code and name and not the user name.

Agent, in `rust-ci` on every platform, without Windows:

- **auth_ac_006 (the accounts reported).** Through the decoding: a 4624 whose target is each SID or SID prefix of §Operational §2, or a name ending in `$`, gives no event; a 4624 of `S-1-5-21-…-1001`, of `S-1-12-1-…` and of `S-1-5-7` gives one; a 4625 gives one whatever its target, `S-1-5-18` included; a record without `TargetUserSid` is counted as unusable.
- **auth_ac_007 (the name of a failed logon).** Each code of §Operational §3, given as `SubStatus` in upper or lower case, keeps the name and the domain; a code given as `Status` with `SubStatus` zero or absent is tested the same way; `0xC0000064` and a code outside the list give `<withheld>` for both, also when the event wrote `-`, and the rendered element contains neither the submitted name nor its length; the codes are written in lowercase hexadecimal.
- **auth_ac_008 (the mapping).** The `logon_type_id` and `auth_protocol_id` tables of §Operational §4; `src_endpoint` absent for `IpAddress` `-`, present with an IPv4, an IPv6, a zoned link-local address without its zone, and an IPv4-mapped address written as IPv4; `hostname` absent for `-` and empty; `cg_elevated_token` from `%%1842` and `%%1843`, absent for another value and on a failure; `user.domain` and `auth_protocol` `-` when the event has no value; a UUIDv7 `event_id`; the converted `time`; a record before 1970 dropped.
- **auth_ac_009 (render and delivery).** A ring holding 1007, 4001 and 3002 events interleaved is delivered in that order against the TLS mock, each element in its class's shape; after a transient failure the resent elements are byte-identical.
- **auth_ac_010 (the agent's own output).** With the agent's log captured, logon records whose user, SID, domain, workstation and address are distinctive strings go through the decoding, the ring and the delivery, and through the counting of an unusable record; none of those strings appears in the captured log.
- **auth_ac_011 (startup failures).** The mapping of a subscription failure to the agent's exit: Win32 error 5 and 1314 give exit code 9 and the stderr line of §Operational §6; another code gives exit code 1 and `cg-agent: Security log subscription failed: <code> <message>`.

Agent on the real Security log, elevated, in the gate (`#[ignore]`d like the ETW tests):

- **auth_ac_012 (logons on the real log).** The test opens the subscription itself, then:
  - Refuses two network logons with `LogonUserW`, each with a random password: one for a random name no account has, one for the built-in Administrator — the local account whose SID is the machine's SID with RID 500, its name looked up. The ring then holds a failure whose `user.name` and `user.domain` are `<withheld>` and whose `status_detail` is `0xc0000064`, and a failure whose `user.name` is not `<withheld>` and whose code is in the list of §Operational §3.
  - Removes any connection to `\\127.0.0.1`, then runs `net use \\127.0.0.1\IPC$` with the current credentials, which succeeds, and removes it. The ring then holds a success whose `user.uid` is the current user's SID, whose `user.name` and `user.domain` are non-empty and not `<withheld>`, whose `logon_type_id` is 3 and which carries `cg_elevated_token`.
  - Reads the last 200 records 4624 of the log with a one-off query through the same rendering, and decodes them: none whose target §Operational §2 excludes gives an event, and at least one such record is among them.
  - Prints, redacted (§Operational §11), each failure's `user.uid` and codes, the success's `src_endpoint` when present, and how long each event took to reach the ring: the session's handoff records them (ADR-0019 §10).

End to end, elevated, developer-local:

- **auth_ac_013 (marquee).** The real agent binary on its normal path against the real stack. The test removes any connection to `\\127.0.0.1`, runs `net use \\127.0.0.1\IPC$` once with a random user name and password, which fails, and once with the current credentials, which succeeds, and removes the connection. Among that agent's 3002 rows in `cges_events`, read with `FINAL`, there is a failure whose `user_name` is `<withheld>` and whose `status_detail` is `0xc0000064`, and a success whose `user_uid` is the current user's SID and whose `logon_type_id` is 3. Its diagnostic output is redacted (§Operational §11).

Regression:

- **auth_ac_014 (regression and gate).** `cargo test --all` is green in `rust-ci` on Linux and Windows and unelevated on a developer's Windows, and `ts-ci` is green. In the elevated gate the existing real-ETW tests and the existing marquees pass with the logon subscription open.

## Test scenarios

No detection scenario changes. No scenario is added: this SPEC raises no alert.

## Risks

- **A host whose audit policy does not log logons** produces no event and no error (ADR-0019 Open questions 1).
- **The measured Security log rolled over in 6.3 days**; live capture is unaffected, and the elevated tests read only what they cause, apart from the one-off query of auth_ac_012.
- **`net use` needs the Server service.** If it is stopped, auth_ac_012 and auth_ac_013 fail on the logon itself; their failure message says so.
- **The built-in Administrator** may be renamed or enabled on another machine; auth_ac_012 finds it by SID, and one refused attempt counts toward its lockout like any other.
- **A burst of failed logons** shares the ring with the other classes and can push process events out (ADR-0019 §Consequences).
- **The Event Log service restarting** ends the subscription's reads; the thread reopens it after a failed read (§Operational §1), and what was written in between is lost.
- **The delay from a logon to the subscription** was not measured; the elevated tests wait up to 15 s and print it.
- **For a Microsoft account**, `TargetUserName` may be the account's email rather than the local name; auth_ac_012 asserts the SID, not the name.

## Open questions

1. **The delay from a logon to its event.** Reopen: if auth_ac_012 shows more than 5 s.

## Decision record

Decisions of basic architecture, taken by Claude Code under ADR-0019 (CLAUDE.md §Decision authority):

1. **A pulled subscription on the agent's own thread** (§Operational §1), rather than a callback on a system thread: shutdown stops a thread the agent owns, as it does the ETW pump.
2. **Values rendered by name** rather than the event's XML, which would need a parser the agent does not have.
3. **`category_uid` 3 on the wire**, which the 1007 and 4001 elements do not carry, because ADR-0019 §4 maps it; the server checks it.
4. **`-` for a name, domain or package the event does not have**, as ADR-0019 §4's "as written" gives the values Windows writes, rather than omitting the member.
5. **`elevated_token` a `Nullable(Bool)`**, so that a failure and a 4624 without the field read `NULL`, not `false`.
6. **The time-to-live set on each bootstrap with `materialize_ttl_after_modify = 0`**, rather than reading the table's TTL first: idempotent and without rewriting parts.
7. **Exit codes 9 and 1** for a subscription that cannot open, on the same Win32 codes as the ETW session's (SPEC-017 §Operational §1).
8. **Reopening the subscription after a failed read**, rather than ending the agent or leaving it with one source silently.
9. **No database message in the server's insert-failure log**, for every class: process command lines and logon names are both data about people.
10. **The gate's logons:** `LogonUserW` for the two failures; `net use` to the loopback for the success, in the Rust test and in the marquee alike, rather than an SSPI exchange coded in the test; and the exclusion tested on the log's own recent records, since a system logon cannot be caused without changing the host.

## References

- [ADR-0019](../adr/0019-cges-authentication-v0-1.md) — the decisions.
- [SPEC-005](SPEC-005-agent-process-telemetry-windows-etw.md) — exit codes, the empty-name rule, the FILETIME conversion.
- [SPEC-017](SPEC-017-agent-capture-normal-run-path.md) — startup, exit codes, delivery and shutdown, amended by scope.
- [SPEC-019](SPEC-019-agent-network-telemetry-windows-etw.md) — the shared ring and table; amended by scope.
- [SPEC-018](SPEC-018-detection-read-model-arrival-cursor.md) — `arrived_at` and the read-model.
- `agent/cg-agent/src/` (a new `logon` module, `lib.rs`, `errors.rs`, `etw/types.rs`, `cges/emit.rs`, `delivery.rs`) — the agent code this SPEC governs.
- `services/ingest/src/schemas.ts`, `services/ingest/src/routes/heartbeat.ts`, `services/ingest/src/db/migrate.ts` — the server code this SPEC governs.
- `schemas/cges/v0.1/classes/3002_authentication.json`, `schemas/cges/v0.1/event.json` — the schema files this SPEC updates.
- `docs/handoff-session-35.md` §Logon facts — the measurements.
