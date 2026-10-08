# SPEC-017: Agent capture on the normal run path — startup, delivery, path translation

- **ID:** SPEC-017
- **Title:** Agent capture on the normal run path — startup, delivery, path translation
- **Status:** Accepted
- **Depends on:**
  - SPEC-005 — realises on the normal run path what SPEC-005 specified and only the test-mode path partly had; amends it **by scope** where §Data contracts and §Operational below differ (the wire shape, the rejected-envelope row of §Failure modes, AC-007's `parent_process`, where the cache is consulted, where the timestamp is converted); NFR-005-003 is deferred (§Out of scope).
  - SPEC-003 — amends its Amendment 2026-05-23 part (a) **by scope**: events travel inside the signed `body`; there is no `batch_hash`.
  - SPEC-001 — its Amendment 2026-05-23 (one `sequence_number` per POST) is implemented here; FR-009 is amended **by scope** for the two startup lines of §Operational §1.
  - ADR-0004 and ADR-0011 — amended in place on 2026-10-04 (the wire shape; the realized field names and the Win32 form).
  - ADR-0009 §1 and §3 — at-least-once delivery and the ephemeral ring, implemented here.
  - ADR-0010 §1 — the elevated-process privilege model; exit code 9.
  - `docs/product/roadmap.md` — §G, the phase this SPEC is the contract for; §H, a destination named below.
- **Authors:** Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

SPEC-005 specified process capture for the agent. Observed at `e19f782`:

1. **Capture runs only in test mode.** `main.rs` reaches `run_test_mode`, the only caller of `EtwSession::open`, only when `CG_AGENT_TEST_MODE=1`. The normal secure path, `run_secure`, sends heartbeats and captures nothing.
2. **A failed session start is swallowed.** `EtwSession::open` returns `Ok` before `trace.start()` runs on its own thread; a failure there, such as a missing privilege, is only logged, and the agent stays alive sending nothing. Exit code 9 (SPEC-005 AC-002) is unreachable with real ETW, and `main.rs` would map an ETW error to exit code 1.
3. **Delivery loses events.** The test-mode loop drains the whole ring into one envelope each second, drops the batch on any transient failure or rejection, sends nothing while no events are drained, and skips the going-offline handshake. Nothing stops the ETW session on shutdown.
4. **The realized wire differs from SPEC-003.** Events travel inside `body` (`body.events`), which the signature covers; no `batch_hash` exists on the agent or the server. The event element has the shape of `CgesProcessActivitySchema` (`services/ingest/src/schemas.ts`), with a flat `process.parent_pid`, where SPEC-005 AC-007 and ADR-0011 §4 name `process.parent_process.pid`.
5. **No path translation.** `process.image_file_name` carries ETW's device path unchanged; the S31 marquee recorded `\Device\HarddiskVolume3\…`.
6. **The harness is blind.** `rust-ci` runs on Linux only, so the Windows ETW code is never compiled, linted or tested in CI. The three Rust tests that reach real ETW (`process_ac_004` hit, `process_ac_007`, `process_ac_009`) fail on Windows even elevated. No CI test posts events to the heartbeat endpoint, and `ts-ci` does not run on changes under `agent/`, although it builds and launches the agent.

## Scope

### In scope

- Capture on the secure run path on Windows; the test-mode path and its environment switch are removed (§Operational §1).
- A session start that reports its result: exit code 9 or 1 on failure (§Operational §1).
- Delivery: bounded batches, in-order at-least-once retry, liveness, the going-offline handshake, a clean session stop (§Operational §2–§4).
- The device-path → Win32 translation of SPEC-005 §Operational §3 (§Operational §5).
- Hygiene for a long-running agent: the cache sweep, the `events_lost` poll, a throttled overflow warning, UUIDv7 event ids (§Operational §6).
- The wire contract, ratified as realized (§Data contracts).
- The server accepts a full batch (§Operational §7).
- The harness: the ETW tests repaired, delivery tests that run without ETW, a Windows job in `rust-ci`, `ts-ci` on `agent/**`, a CI test that posts events, and both marquees on the normal path (§Acceptance criteria).

### Out of scope

Each item has its destination in brackets.

- `events_dropped_total` on the wire (SPEC-005 NFR-005-003): the counter stays agent-side, in the log [a later agent-health SPEC, with the dashboard surface that reads it].
- The log file of ADR-0010 §2 [roadmap §F].
- Server-side offline detection (ADR-0004 §Heartbeat and degraded mode) [a later agent-health SPEC].
- Events that arrive after the detection watermark has passed their `time` [roadmap §H].
- Capture on the plain path (`run`, no trust anchor): events need the signed envelope [not planned].
- Capture on non-Windows platforms [ADR-0002 Rule 2; post-MVP].
- Two agents, or an agent and a test, sharing a host: the session name is fixed and the startup reclaim stops a live session of the same name [not planned for the MVP].
- `CommandLine` and `User` capture, and the parent stamped by the agent [roadmap §B2].
- The persistent disk buffer [ADR-0009 §4, unchanged].
- Refreshing the prefix map on volume changes [SPEC-005 NFR-005-007, unchanged].

## Data contracts

Ratified as realized; amends SPEC-003 Amendment 2026-05-23 part (a) and SPEC-005 by scope.

- **Envelope.** The outer signed envelope is SPEC-003's original shape: `outer_envelope_version`, `agent_id`, `sequence_number`, `nonce`, `sent_at`, `body`, `signature`. There is no top-level `events` and no `batch_hash`.
- **Events.** `body` is the SPEC-001 heartbeat envelope plus one optional member, `events`: an array of event elements, omitted when empty. The signature covers the canonical envelope minus `signature`, which contains `body`, so the events are signed directly. SPEC-003 FR-011 ("verbatim and unchanged") holds for a heartbeat without events.
- **Event element.** The shape `CgesProcessActivitySchema` validates: `event_id`; `class_uid` 1007; `activity_id` 1 or 2; `time` (string-encoded Unix nanoseconds); and `process` with `pid`, `uid`, `name`, `created_time` (string or `null`), `exit_code` (absent on Launch), `parent_pid` (integer, or `null` when ETW reports 0), `command_line`, `subject_user_sid` and `image_file_name`.
- **`event_id`** is a UUIDv7 generated at capture (ADR-0009 §1).
- **`process.image_file_name`** carries the Win32 form when the translation resolves, and the device form verbatim otherwise (§Operational §5). `process.name` is its last path segment in either form.
- **A resent event is byte-identical.** Each event is rendered once, when its batch is formed; a retry resends the same elements, so `event_id`, `time` and `process.uid` never change between attempts and the server's `ReplacingMergeTree` collapses the copies.

## Operational

### 1. Startup

- On the secure path (`server.trust_anchor_path` set) on Windows, the agent loads or enrolls its identity, then opens the ETW session, then enters the delivery loop. `run_test_mode` and `CG_AGENT_TEST_MODE` are removed.
- `EtwSession::open` returns only when the session has started or failed; the Win32 result of the start reaches the caller.
- Start refused for privilege (Win32 error 5 or 1314): the agent exits with code **9**, after writing the SPEC-005 AC-002 line to stderr and logging it at `error`. It sends no heartbeat and no event POST. The enrollment that preceded it persists, so the elevated rerun needs no new token.
- Any other start failure: exit code **1**, with `cg-agent: ETW session open failed: <code> <message>` on stderr and in the log.
- On a non-Windows build there is no capture backend: the secure path sends heartbeats only and logs once, at `info`, that capture is unavailable on this platform. Release builds for non-Windows stay a compile error.
- Events captured before the delivery loop starts are delivered like any others; nothing is captured before the identity exists.

### 2. Batching and liveness

One POST is in flight at a time, and POSTs go out in order.

- A POST is formed, when none is in flight, as soon as one of these holds: **1024** events are buffered; a buffered event is **5000 ms** old; or a heartbeat tick is due.
- Heartbeat ticks follow SPEC-001 FR-011's absolute timeline (`start_time + k · interval_seconds`, the first at startup). A tick sends a POST without events only if no POST was sent since the previous tick: every POST is a heartbeat (SPEC-001 Amendment 2026-05-23).
- A POST carries at most 1024 events, the oldest first, and takes the next `sequence_number` (one per POST).

### 3. Retry

A retry is the same POST: same `sequence_number`, same events; fresh `nonce`, `sent_at` and signature.

- **Transient failure** (connection error, timeout, reset, or a 5xx): the POST is retried with the SPEC-001 backoff. A POST that carries events is retried until it is delivered or rejected; later events wait behind it in the ring, which keeps its FIFO drop. A POST without events follows SPEC-001: `max_retries`, then the next tick.
- **Rejection** (a 4xx): retried up to `heartbeat.max_retries` attempts in total; then the batch is dropped, its events are added to the agent's dropped count, and one `error` line records the status and the count. The loop continues with the next batch. This replaces "the events remain in the ring" of SPEC-005 §Failure modes, which had no bound.
- Fatal TLS and signing failures keep their SPEC-003 exit codes (6, 7, 8).

### 4. Shutdown

On the shutdown signal the agent stops the ETW session and waits for its thread, then makes one attempt at a final POST with status `going_offline`. That POST carries the batch in flight, or else the next batch, or no events. Events still undelivered are lost (the ring is ephemeral, ADR-0009 §3); their count is logged at `warn`.

### 5. Path translation

As SPEC-005 §Operational §3 specifies (a prefix map built once at startup with `QueryDosDeviceW`, the UNC rule, the fallthrough cases), with two precisions: the translated value is `process.image_file_name`, the only path field on the wire; and it is applied when the event is rendered, never in the dispatch callback. A match requires the device prefix to end at a path separator, and the longest prefix wins.

### 6. Capture hygiene

- **Dispatch callback** (amends SPEC-005 NFR-005-001 by scope): it parses the record, converts the timestamp, generates the `event_id`, inserts into the cache on Launch or consults and purges it on Terminate (SPEC-005 §Operational §2), and enqueues. It does no I/O and takes no lock beyond the cache's and the ring's. The ring does not log while it holds its lock.
- **Cache sweep** every 60 s, as SPEC-005 NFR-005-006 specifies.
- **`events_lost` poll** every 60 s: an increase is logged at `warn` with the new total.
- **Ring overflow** is logged at `warn`, at most once per 60 s, with the dropped total.

### 7. Server

The heartbeat listener accepts bodies up to **4 MiB** (a constant), so a batch of 1024 events with long paths is not refused by the framework's 1 MiB default. Nothing else changes on the server: the route already persists `body.events`.

## Acceptance criteria

Each maps to a test named `capture_ac_NNN_*`, under `agent/cg-agent/tests/` or `services/ingest/test/`. The delivery criteria run without ETW, on every platform, by feeding the ring synthetic events against the TLS mock.

- **capture_ac_001 (startup failure).** A privilege failure exits 9 with the AC-002 line and no heartbeat POST; another start failure exits 1 with its line; `EtwSession::open` returns the failure. One test launches the real binary unelevated on Windows and expects exit code 9 (developer-local; skipped when elevated or not on Windows).
- **capture_ac_002 (batching).** With 1024 or more events buffered, a POST carries exactly 1024, without waiting; a single event is delivered within 5000 ms plus scheduling tolerance; events keep their order across POSTs.
- **capture_ac_003 (at-least-once).** After a transient failure or a 5xx the same POST is retried: same `sequence_number`, events byte-identical, a fresh `nonce`; no later event is sent before it; it is delivered once the server recovers.
- **capture_ac_004 (rejection).** A 4xx is retried up to `max_retries` attempts, then the batch is dropped, counted and logged, and the next batch is sent.
- **capture_ac_005 (liveness).** Without events, one POST per tick on the absolute timeline; a POST with events since the previous tick suppresses the empty one; `sequence_number` grows by one per POST.
- **capture_ac_006 (shutdown).** The final POST has status `going_offline` and carries the remaining events up to 1024; on Windows, elevated, no ETW session of the agent's name is left behind.
- **capture_ac_007 (render once).** A Terminate carries the `created_time` resolved at dispatch; a PID reused before the next batch does not change it.
- **capture_ac_008 (translation).** The pure translation function maps a drive prefix, prefers the longest prefix, respects the separator boundary, applies the UNC rule, and leaves an unresolved path verbatim.
- **capture_ac_009 (platform).** On a non-Windows build the secure path sends heartbeats only and logs the notice once; `mtls_ac_001`–`009` stay green.
- **capture_ac_010 (hygiene).** The sweep evicts entries per NFR-005-006; an `events_lost` increase and a ring overflow each produce their `warn`, the latter throttled; `event_id` is a UUIDv7.
- **capture_ac_011 (server).** A signed POST with events persists them; the same events posted again leave the `FINAL` count unchanged; a 1024-event batch with 1 KiB paths is accepted. Runs in CI.
- **capture_ac_012 (marquees).** `spec-005-marquee` and `detect_ac_001` pass with the agent on the normal path and no environment switch, and assert that the captured image path is in Win32 form. Developer-local and elevated, run by Manuel.
- **capture_ac_013 (harness).** `cargo test --all` is green on Linux and on unelevated Windows; the tests that need real ETW (`process_ac_004` hit, `process_ac_007`, `process_ac_009`) run with one documented command in the elevated gate and are green there; `rust-ci` has a Windows job (format, lint, unelevated tests); `ts-ci` runs on `agent/**`.

## Test scenarios

No detection scenario changes. The SPEC-016 scenarios and fixtures already cover both path forms, and stay green.

## Risks

| Risk | Mitigation |
| --- | --- |
| A long outage fills the ring and drops the oldest events | By design (ADR-0009 §3); the overflow warning and the dropped count make it visible in the agent log |
| A retried batch delays the events behind it | In-order delivery is the point: the server never sees a later event first; the backoff is capped by `backoff_max_ms` |
| The Windows CI job cannot run the ETW tests | They stay in the elevated developer-local gate (§Open questions 1) |
| Translation changes `image_file_name` for new events while old rows keep the device form | Every rule matches both forms (SPEC-016); the stored rows are not rewritten |
| An unelevated agent no longer appears in the dashboard | It exits with code 9 and a stderr line that names the cause; ADR-0010 §1 already requires elevation |

## Open questions

1. **ETW tests on the hosted Windows runner.** ADR-0010 §3 left the runner's ETW privilege untested. **Reopen once the Windows job exists:** if the runner can open the session, move the ETW tests into CI.
2. **`events_dropped_total` for the operator.** Deferred with NFR-005-003. **Reopen with the agent-health surface.**
3. **Batch size in bytes.** The cap is in events; the server limit is in bytes. 4 MiB leaves about 4 KiB per event. **Reopen if a field larger than that reaches the wire (roadmap §B2 adds `command_line`).**

## Ratification record

Load-bearing decisions for Manuel's gate. Manuel delegated the four owner decisions explicitly ("elige tú", 2026-10-04); the advisor decided, and Manuel's ratification of this SPEC ratifies them.

1. **An unelevated agent exits with code 9** (SPEC-005 AC-002, ADR-0010 §1), instead of degrading; no new configuration key.
2. **Scope: the honest core** — capture, real startup failure, at-least-once delivery, translation, hygiene, and the harness. `events_dropped_total` on the wire, the log file and offline detection are deferred, each with its destination.
3. **The realized wire is the contract** — events inside the signed `body`, no `batch_hash`, a flat `process.parent_pid`. SPEC-003 and SPEC-005 are amended by scope; ADR-0004 and ADR-0011 in place.
4. **Late events are their own phase** — roadmap §H, after G and before D.
5. **A rejected batch is dropped after `max_retries` attempts**, where SPEC-005 kept it in the ring without a bound.
6. **`image_file_name` carries the Win32 form**; no second path field.
7. **The test-mode path is removed**, not kept beside the normal one.
8. **Doc-only gate first.** The code is the next gate (a review branch, relay rule 5) and includes the elevated gate: the Rust ETW tests and both marquees.

## Amendment 2026-10-09: a Terminate carries the image base name

**Surfaced by** the elevated gate on the review branch (S32, 2026-10-04): the SPEC-005 marquee's Terminate row had `image_file_name` `cmd.exe` while its Launch row was in `C:\…` form. ETW's ProcessStop event carries in `ImageName` only the image's base name, not a path.

**Amendment.** §Data contracts, `process.image_file_name`: on a Launch it carries the Win32 form when the translation resolves, and the device form verbatim otherwise; on a Terminate it carries ETW's base name unchanged, which has nothing to translate (the SPEC-005 §Operational §3 rule for a value no prefix matches). `process.name` is its last path segment in every case. **capture_ac_012:** the Win32 form is asserted on the Launch; the Terminate must not be in device form.

**Effect.** No change to the agent's behaviour or to the wire; the agent always emitted this. Rules evaluate Launch events only (SPEC-016), so detection is unaffected. Carrying the Launch's path to the Terminate is recorded as debt #29 (`docs/handoff-session-32.md`).

## References

- [SPEC-005](SPEC-005-agent-process-telemetry-windows-etw.md) — the capture specification this SPEC realises and amends by scope.
- [SPEC-003](SPEC-003-mtls-signed-envelope.md) — the signed envelope; its Amendment 2026-05-23 part (a) is amended here by scope.
- [SPEC-001](SPEC-001-agent-heartbeat.md) — scheduling, retry and backoff; its Amendment 2026-05-23 on `sequence_number`.
- [SPEC-004](SPEC-004-server-ingest-minimal.md) — the ingest service and its heartbeat route.
- [SPEC-016](SPEC-016-detection-rule-set-v1.md) — the rules that read `image_file_name` in both forms.
- [ADR-0004](../adr/0004-agent-server-protocol.md), [ADR-0011](../adr/0011-cges-process-activity-v0-1.md) — amended in place on 2026-10-04.
- [ADR-0008](../adr/0008-etw-crate-selection.md), [ADR-0009](../adr/0009-event-delivery-and-buffer.md), [ADR-0010](../adr/0010-agent-privilege-model-mvp.md) — the ETW crate, delivery and buffer, and the privilege model.
- [roadmap](../product/roadmap.md) — §G (this SPEC), §H, §B2, §F.
- `agent/cg-agent/src/lib.rs`, `agent/cg-agent/src/etw/`, `agent/cg-agent/src/cges/emit.rs`, `services/ingest/src/routes/heartbeat.ts`, `services/ingest/src/schemas.ts` — the code this SPEC governs.
