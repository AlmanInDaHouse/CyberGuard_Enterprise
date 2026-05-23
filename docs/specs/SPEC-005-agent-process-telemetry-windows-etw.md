# SPEC-005: Agent process telemetry — Windows ETW Kernel-Process

- **ID:** SPEC-005
- **Title:** Agent process telemetry — Windows ETW Kernel-Process
- **Status:** Proposed
- **Depends on:** ADR-0001, ADR-0002, ADR-0006, ADR-0008, ADR-0009, ADR-0010, ADR-0011, SPEC-001 (§Behavior reused; amendment 2026-05-23 co-located in this SPEC's ratification commit), SPEC-002 (identity), SPEC-003 (envelope E2; amendment 2026-05-23 part (a))
- **Authors:** Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)
- **Created:** 2026-05-23
- **Last updated:** 2026-05-23

## Motivation

SPEC-005 is the first SPEC under which the CyberGuard agent emits actual security telemetry rather than pure liveness signals. It is the first consumer of ADR-0008 (ETW crate — ferrisetw 1.2.0 + raw-Win32 side-channel), ADR-0009 (at-least-once delivery + in-memory ring), ADR-0010 (elevated user process privilege model), ADR-0011 (per-class jurisprudence — Process Activity v0.1), and the SPEC-003 amendment 2026-05-23 part (a) E2 wire shape with `events[]` + `batch_hash`.

Scope of v0.1: Windows-only, `Microsoft-Windows-Kernel-Process` provider, activity_id ∈ {1 (Launch), 2 (Terminate)} per ADR-0011 §3. All other ETW providers (file, network, registry, image-load), all other Process Activity activity_ids (Open, Inject, Set User ID), and all non-Windows agents are deferred to successor SPECs.

The polyglot marquee (AC-001 below) is the architecture's reason-to-exist: real `cg-agent` → real ETW capture → real signed envelope → real `services/ingest/` → real ClickHouse, end-to-end within the D7 45 s budget with elapsed-time logging in CI per the Phase 3 opening protocol.

## Scope

### In scope

- Real-time consumption of `Microsoft-Windows-Kernel-Process` ETW events on Windows via ferrisetw 1.2.0 per ADR-0008.
- Emission of CGES Process Activity v0.1 events (OCSF class 1007) for activity_id ∈ {1, 2} per ADR-0011.
- Agent-side in-memory ring buffer (sizing per §NFR, forthcoming) per ADR-0009.
- Worker-on-ring-drain path: ring events → JCS-canonicalize → compute `batch_hash` → build outer signed envelope per SPEC-003 → POST to `/v1/agents/heartbeat`.
- Multi-POST-per-interval behavior (event-driven flushes + 30 s timer fallback); the SPEC-001 amendment 2026-05-23 reconciling `sequence_number` semantics is co-located in this SPEC's ratification commit.
- Server-side persistence of received events as rows in the ClickHouse `cges_events` table per ADR-0009 + D6.
- Agent-local volatile `(PID → creation_time)` cache for `process.created_time` enrichment at Terminate events; the cache is non-load-bearing for `process.uid` (which is computed at Launch from ETW data directly per ADR-0011 §6). Full mechanism in §Operational (forthcoming).
- Clean-fail on insufficient privilege at agent startup per ADR-0010, with a clear stderr message identifying the cause.
- Test harness with one Rust integration test per AC; the marquee (AC-001) uses real services per the Spec-Driven Development with harness-first invariant.

### Out of scope

Full per-item rationale forthcoming in a dedicated subsection in a subsequent commit of this Phase 3.3 series. Items already named in the inherited contracts:

- Other ETW providers (file, network, registry, image-load) — successor SPECs.
- Other Process Activity activity_ids (`0` Unknown, `3` Open, `4` Inject, `5` Set User ID, `99` Other) — per-class amendment to ADR-0011 or a successor per-class ADR when load-bearing.
- Persistent disk-backed buffer — future SPEC per ADR-0009 §Decision part 4 + ADR-0004 amendment 2026-05-23 part (a).
- Windows Service packaging + MSI installer — future packaging SPEC per ADR-0010 §Decision part 2 (supersedes ADR-0010 §Decision part 2 when it lands).
- Cross-platform agent support (Linux audit, macOS Endpoint Security Framework) — per-platform SPECs with their own per-class ADRs.
- CommandLine PII redaction — deferred per ADR-0006 §Out-of-scope (Blueprint §17.11) and ADR-0011 §Out-of-scope; captured as-is in v0.1 with accepted-risk decision recorded in this SPEC's §Out-of-scope full subsection (forthcoming).

## Acceptance criteria

Each AC maps 1:1 to a Rust integration test under `agent/cg-agent/tests/`, named `process_ac_NNN_*` to avoid collision with SPEC-001 (`ac_NNN_*`), SPEC-002 (`enroll_ac_NNN_*`), and SPEC-003 (`mtls_ac_NNN_*`) test naming.

AC-001 is the polyglot marquee per the harness-first invariant: real `cg-agent` binary, real `services/ingest/`, real Postgres / ClickHouse / Redis via testcontainers, no mocks at any layer.

- **AC-001 (marquee — polyglot end-to-end).** Given the full agent stack running against real `services/ingest/`, real Postgres, real ClickHouse, and real Redis (testcontainers; escalation order per the harness-first invariant: testcontainers → image pin → GHCR mirror → backends started via `task dev:up` by test setup), and given the agent enrolled with a loadable SPEC-002 identity, when a probe process is launched and terminated (`cmd.exe /c exit 0`), the agent captures both ETW Kernel-Process events via ferrisetw 1.2.0 (per ADR-0008), wraps them in one or more outer signed envelopes per SPEC-003 amendment 2026-05-23 part (a), POSTs to the `/v1/agents/heartbeat` endpoint over mTLS 1.3, and the ingest service persists both events as rows in the ClickHouse `cges_events` table.

  The persisted rows MUST satisfy the following five conditions: **(a)** two rows with `class_uid = 1007`, distinguished by `activity_id` values `1` (Launch) and `2` (Terminate) per ADR-0011 §3; **(b)** identical `process.uid` bytes across both rows, matching the hand-computed expected string `<agent_id>:<probe_pid>:<probe_created_time_unix_nanos_utc>` byte-for-byte per ADR-0011 §6; **(c)** Launch row carries `process.created_time` as integer nanoseconds since Unix epoch UTC per ADR-0011 §4 amendment (a), and has no `process.exit_code` field present per ADR-0011 §4 amendment (b) conditional emission contract; **(d)** Terminate row carries `process.exit_code = 0` as signed int32 (matching the probe's `exit 0`), and carries `process.created_time` either matching the Launch row's value (cache hit per §Operational, forthcoming) or `null` (cache miss per same); **(e)** both rows carry top-level `process.name = "cmd.exe"` per ADR-0011 §5 agent normative — top-level `process.name` MUST be emitted, log-and-drop on absence at capture time.

  The harness logs `marquee_elapsed_seconds` at level `info` from probe-spawn timestamp to both-rows-readable-in-ClickHouse timestamp. The field is visible in the CI run output per the Phase 3 opening protocol (D7). The elapsed time MUST NOT exceed `45.0` seconds. If the threshold is exceeded, the AC fails with a clear log line identifying the elapsed value.

  CI privilege assumption per ADR-0010 §Decision part 3: this AC is the first-use validation of whether `windows-latest`'s `runneradmin` carries the same effective ETW-open privileges as a locally-elevated user. If AC-001 fails specifically because `runneradmin` cannot open the Kernel-Process provider (Win32 error code `5` `ERROR_ACCESS_DENIED` or `1314` `ERROR_PRIVILEGE_NOT_HELD`), one of ADR-0010's two named fallback paths fires; the choice between fallback path 1 (SYSTEM trampoline) and fallback path 2 (move marquee out of CI) is deferred until the contradiction surfaces.

- **AC-002 (clean-fail on insufficient privilege).** When the agent's ETW session open path returns a Win32 error indicating insufficient privilege to open the `Microsoft-Windows-Kernel-Process` provider — specifically Win32 error code `5` (`ERROR_ACCESS_DENIED`) or `1314` (`ERROR_PRIVILEGE_NOT_HELD`) per ADR-0010 §Decision part 1 — the agent MUST exit with code `9` (introduced by this SPEC; reserved for "insufficient privilege to open ETW provider"; full §Failure modes table forthcoming in a subsequent Phase 3.3.X commit), MUST write the exact line `cg-agent: insufficient privilege to open Microsoft-Windows-Kernel-Process ETW session; run as elevated user or LocalSystem` to stderr, and MUST NOT panic, MUST NOT silently degrade, MUST NOT emit any heartbeat or event POST.

  Test mechanism: integration test exercises the agent startup path with the ETW session open boundary returning a synthetic `ERROR_PRIVILEGE_NOT_HELD` `Err`. The test captures the process's stderr and exit code via the standard Rust integration test harness; no real ETW session is opened in this AC. This AC is independent of CI privilege status and runs the same on any runner regardless of elevation.

- **AC-003 (`process.uid` byte-level recipe pinned by fixture).** Given the fixed input triple `agent_id = "01934abc-def0-7000-89ab-000000000001"` (canonical 8-4-4-4-12 lowercase UUIDv7 per ADR-0011 §6), `pid = 7144` (decimal, no padding, no sign), and `created_time_unix_nanos_utc = 1716123612901000000` (decimal nanoseconds since Unix epoch UTC strict per ADR-0011 §4 amendment (a) + §6), the agent's `process.uid` formatter MUST produce the byte-for-byte exact string `01934abc-def0-7000-89ab-000000000001:7144:1716123612901000000` of length 61 characters.

  Additionally, the formatter MUST produce a string of length 58 characters for the boundary input `pid = 1` (smallest legal Windows PID; agent_id and created_time as above), and a string of length 67 characters for `pid = 4294967295` (largest legal Windows PID, 2^32 − 1, ten decimal digits; agent_id and created_time as above). These two boundary assertions pin the 58–67 character bound declared in ADR-0011 §6 directly against the formatter's output.

  Test mechanism: unit test in the agent crate (`agent/cg-agent/`). The formatter is a pure function on the input triple; no ETW, no ingest, no testcontainers required. The hand-computed expected strings live verbatim in the test source as the regression anchor.

- **AC-004 (`process.created_time` integer-nanos UTC + cache hit/miss for Terminate).** For activity_id=1 (Launch) events, the agent MUST emit `process.created_time` as integer nanoseconds since Unix epoch UTC strict per ADR-0011 §4 amendment (a); the value is the ETW event's own timestamp converted to UTC nanoseconds via the conversion mechanism specified in §Operational (forthcoming). The emitted value MUST be `>= 0` and MUST fit a signed 64-bit integer (Windows FILETIME range comfortably bounds this for any plausible event time).

  For activity_id=2 (Terminate) events, the agent MUST consult the agent-local volatile `(PID → creation_time)` cache populated at Launch capture per §Operational (forthcoming). Two cache outcomes, both tested:

  - **Cache hit:** the Terminate row's `process.created_time` MUST equal the corresponding Launch row's `process.created_time` byte-for-byte. The test exercises the nominal lifecycle (probe Launch captured → Terminate captured during the same agent session → both rows persisted to ClickHouse) and asserts equality at the persisted row layer.
  - **Cache miss:** when no cache entry exists for the Terminated PID (entry was never populated because Launch occurred before agent start, or entry was purged by a prior Terminate consultation, or the cache was lost by agent restart per §Operational forthcoming), the Terminate row's `process.created_time` MUST be `null`. The test exercises this by issuing a Terminate event for a PID whose Launch was not observed by the running agent.

  Test mechanism: two integration tests, both running against real ETW capture and real ingest (testcontainers per the harness-first invariant). The cache-miss test deliberately bypasses the Launch capture path to force the miss condition.

- **AC-005 (`process.exit_code` conditional emission).** For activity_id=1 (Launch) events, the agent MUST NOT emit a `process.exit_code` field. The persisted row MUST satisfy `process.exit_code IS NULL` in ClickHouse, distinguishable from a row where the field was emitted as `0` (the field is absent, not zero). This is the stricter-than-schema agent normative declared in ADR-0011 §4 amendment (b), §Compliance: "the conditional emission contract (`exit_code` MUST be present when activity_id=2 and MUST be absent when activity_id=1) is documented in SPEC-005 §Acceptance Criteria as a stricter-than-schema agent normative."

  For activity_id=2 (Terminate) events, the agent MUST emit `process.exit_code` as a signed 32-bit integer per ADR-0011 §4 amendment (b); the value is the ETW event's `ExitStatus` field interpreted as Windows `LONG` (`int32_t` signed). The test exercises three values pinning the contract: `exit_code = 0` (nominal clean exit from `cmd.exe /c exit 0`); `exit_code = 1` (nominal non-zero exit from `cmd.exe /c exit 1`); `exit_code = -1073741819` (the signed-int32 interpretation of `0xC0000005` `STATUS_ACCESS_VIOLATION`, exercising the NTSTATUS-as-negative-signed-int32 case explicitly named in ADR-0011 §4 amendment (b)).

  Test mechanism: integration test running against real ETW capture and real ingest (testcontainers). The `STATUS_ACCESS_VIOLATION` case requires a probe that deliberately faults; one option is a tiny Rust binary built at test setup that dereferences a null pointer via `unsafe`. The test asserts both the presence/absence of the field per activity_id and the exact integer value for Terminate events.

AC-006 through AC-009 forthcoming in subsequent Phase 3.3.X commits.

## References

Full reference list forthcoming with the ratification commit. Cross-references already in scope for this commit:

- [ADR-0001](../adr/0001-monorepo-layout.md), [ADR-0002](../adr/0002-language-per-component.md), [ADR-0006](../adr/0006-cges-ocsf-alignment.md), [ADR-0008](../adr/0008-etw-crate-selection.md), [ADR-0009](../adr/0009-event-delivery-and-buffer.md), [ADR-0010](../adr/0010-agent-privilege-model-mvp.md), [ADR-0011](../adr/0011-cges-process-activity-v0-1.md).
- [SPEC-001](SPEC-001-agent-heartbeat.md), [SPEC-002](SPEC-002-agent-enrollment.md), [SPEC-003](SPEC-003-mtls-signed-envelope.md).
- [docs/spikes/2026-05-23-etw-process-events.md](../spikes/2026-05-23-etw-process-events.md) — Phase 0 spike empirical baseline.
- [Foundational Blueprint](../product/blueprint.md) — §7 (Agent-Server Secure Protocol), §17.11 (Advanced anonymisation out of MVP).
