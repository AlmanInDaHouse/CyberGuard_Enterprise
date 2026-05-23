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

Six items are explicitly deferred from SPEC-005's v0.1 scope. Each is named, with rationale for the deferral and the document or SPEC that absorbs the item when it lands. Each deferral is a scope discipline call: SPEC-005 ships process telemetry MVP under one ETW provider with two activity_ids; expanding scope further blocks the first event SPEC on subsystems that are not its actual deliverable.

**1. Other ETW providers (file, network, registry, image-load, and others).** SPEC-005 consumes only `Microsoft-Windows-Kernel-Process` per ADR-0008 §Decision part 1. Other Windows ETW providers — `Microsoft-Windows-Kernel-File`, `Microsoft-Windows-Kernel-Network`, `Microsoft-Windows-Kernel-Registry`, `Microsoft-Windows-Kernel-ImageLoad`, and others — surface different OCSF event classes (File System Activity 1001, Network Activity 4001, Registry Key Activity 201001, Module Activity 1002, and others). Each requires its own per-class ETW field mapping, its own emission scope decisions, and its own per-class jurisprudence ADR following the pattern ADR-0011 establishes. Deferral target: a dedicated successor SPEC per provider when that telemetry becomes load-bearing for a downstream product feature; no schedule or pre-committed ordering. The agent-side ETW capture infrastructure built in SPEC-005 (ferrisetw session lifecycle, side-channel helper module per ADR-0008, ring buffer + worker drain per ADR-0009) is reusable across providers; each successor SPEC inherits the infrastructure and adds the provider-specific dispatch handlers and field mappings.

**2. Other Process Activity activity_ids (`0` Unknown, `3` Open, `4` Inject, `5` Set User ID, `99` Other).** SPEC-005 emits only activity_id ∈ {1 (Launch), 2 (Terminate)} per ADR-0011 §3, while the CGES schema's `activity_id` enum stays OCSF-permissive at `[0, 1, 2, 3, 4, 5, 99]`. The remaining activity_ids require either different ETW providers (Open and Inject involve handle-creation events from providers other than Kernel-Process; Set User ID involves authentication events) or instrumentation beyond Kernel-Process. Per the Phase 0 spike per ADR-0008 §Empirical justification, Launch and Terminate are the two foundational events from `Microsoft-Windows-Kernel-Process`; the other activity_ids are not reachable from this provider. Deferral target: either a per-class amendment to ADR-0011 (when the new activity_id maps cleanly onto the Process Activity class semantics) or a successor per-class ADR (when the addition requires substantive jurisprudence changes) plus a SPEC-005 amendment narrowing the v0.1 agent emission scope to allow the new activity_id.

**3. Persistent disk-backed encrypted buffer.** SPEC-005 ships only the in-memory ring buffer per ADR-0009 §Decision part 3 (ephemeral, FIFO drop on overflow, lost on agent restart). The persistent disk-backed encrypted buffer originally specified in ADR-0004 §Heartbeat and degraded mode (200 MB / 24 h, DPAPI-derived key, drain on reconnect) was deferred at the time of ADR-0009's ratification per ADR-0004 amendment 2026-05-23 part (a). Building it would require addressing at-rest encryption (DPAPI on Windows in MVP; Linux and macOS platform keyrings separately deferred per ADR-0002 Rule 2), file format and rotation, crash recovery, cross-restart `sequence_number` persistence (a renewed concern in that future scope), and replay-on-reconnect semantics including how replayed events interact with the server-side dedup keyed on `event_id` per ADR-0009. Bundling all of those into SPEC-005 would block the first event SPEC on a subsystem that is not its actual deliverable. Deferral target: a dedicated future SPEC per ADR-0009 §Decision part 4 + ADR-0004 amendment 2026-05-23 part (a); when it lands, it will need to re-evaluate whether the in-memory determinism of ADR-0011 §6's `process.uid` recipe still holds under the persistent-buffer-replay path per ADR-0011 §Out-of-scope fifth bullet.

**4. Windows Service packaging + MSI installer + auto-update.** SPEC-005 ships the agent as an elevated user process manually launched by an operator per ADR-0010 §Decision part 2; there is no Windows Service registration, no MSI / NSIS / WiX installer, no auto-start-at-boot mechanism, no scheduled task, no service crash-recovery policy, and no auto-update. This posture is MVP-only and not production-deployable for end-customer use per ADR-0010's explicit acknowledgment. Building the packaging subsystem requires service registration code (`sc create` or `windows-service-rs`), service lifecycle handlers (start / stop / pause / resume / shutdown), Event Log integration (no stdout when running as a service), log rotation, installer (MSI / NSIS / WiX) with uninstaller, and auto-update mechanism — each a non-trivial subsystem. Deferral target: a dedicated future packaging SPEC per ADR-0010 §Decision part 2 (the installation-posture portion of ADR-0010 is explicitly marked as superseded by this future SPEC when it lands; the privilege-model portion of ADR-0010, the elevated-baseline requirement, carries through unchanged).

**5. Cross-platform agent support (Linux audit, macOS Endpoint Security Framework).** SPEC-005 is Windows-only per ADR-0002 Rule 2 (Windows-first agent posture) and ADR-0008 §Decision (ferrisetw is a Windows-only crate consuming Windows ETW). Linux process telemetry via the audit subsystem and macOS process telemetry via the Endpoint Security Framework each require platform-specific capture infrastructure, platform-specific privilege models, and platform-specific identity-loading mechanisms (DPAPI is Windows-only; Linux and macOS platform keyrings are separately deferred per ADR-0002 Rule 2 and the persistent-buffer deferral above). The CGES event schema (ADR-0006) and the agent's emission contracts (ADR-0011 for Process Activity) are platform-agnostic by construction; the per-platform work is the capture layer plus the per-platform privilege and packaging story. Deferral target: a dedicated per-platform SPEC for Linux process telemetry and a dedicated per-platform SPEC for macOS process telemetry, each with its own per-class jurisprudence ADR if the platform's capture mechanism introduces jurisprudence concerns not already covered by ADR-0011.

**6. CommandLine PII redaction (accepted-risk decision for v0.1).** SPEC-005 emits `process.cmd_line` as a raw string per ADR-0011 §4 row 5 with no redaction, no opt-out config, and no downstream sanitisation. Command-line arguments routinely contain personally identifiable information (file paths with user names, mail server addresses with credentials embedded in URLs, application-specific tokens passed as positional arguments, and other sensitive data depending on the user's installed software). The v0.1 accepted-risk decision is to capture command-line strings as-is and rely on operational controls (the security team's access policy to the ClickHouse `cges_events` table; the operator's deployment-time decision of which endpoints run the agent; the closed-test environment posture per SPEC-001 §Scope OUT) rather than agent-side or pipeline-side redaction. Building the redaction subsystem requires defining a redaction-pattern language (regex? structured pattern matchers?), an opt-out configuration surface (per-pattern? per-process? per-user?), a downstream sanitisation layer (apply at ingest? at query? at export?), and a test corpus of PII shapes; each is a non-trivial design decision that conflates with the deferred field-level encryption work per ADR-0006 §Out-of-scope third bullet (Blueprint §17.11 — Advanced anonymisation out of MVP). Triple cross-reference chain for the deferral: ADR-0006 §Out-of-scope third bullet (Blueprint §17.11) is the framework deferral; ADR-0011 §Out-of-scope first bullet is the per-class restatement; this SPEC §Out-of-scope item 6 is the agent-side accepted-risk concretisation. Deferral target: the field-level PII encryption / redaction work referenced in Blueprint §17.11 (Advanced anonymisation out of MVP); a future SPEC under that work will define the redaction surface and amend ADR-0011 §4 row 5 + this SPEC §Out-of-scope item 6 accordingly.

## Operational

Three agent-side mechanisms are scoped here that the inherited contracts (ADR-0008, ADR-0009, ADR-0011) deferred to SPEC-005 as agent implementation concerns. None are exposed in the wire envelope; all are internal to the agent process. Sequenced in dependency order: the conversion mechanism (1) is foundational; the cache (2) consumes the conversion; the path translation (3) is independent.

### 1. ETW timestamp → UTC nanoseconds conversion

ETW event records carry timestamps as Windows `FILETIME` values in the `EVENT_HEADER.TimeStamp` field — 64-bit unsigned integers measuring 100-nanosecond intervals since `1601-01-01 00:00:00 UTC`. CGES emits `process.created_time` as integer nanoseconds since Unix epoch (`1970-01-01 00:00:00 UTC`) per ADR-0011 §4 amendment (a) + §6.

The conversion is deterministic and stateless:

```text
UNIX_EPOCH_FILETIME_DELTA_100NS_TICKS = 116444736000000000
unix_nanos = (filetime_100ns_ticks - UNIX_EPOCH_FILETIME_DELTA_100NS_TICKS) * 100
```

The constant `116444736000000000` is the number of 100-nanosecond intervals between `1601-01-01 UTC` and `1970-01-01 UTC` (`(369 * 365 + 89) * 86400 * 10000000` accounting for leap years 1604–1968 inclusive). It is fixed by calendar arithmetic and identical across all Windows versions and ETW providers.

The agent MUST NOT consult the local timezone, the system clock, or any user-configurable time setting during this conversion. ETW `FILETIME` values are always UTC by definition; converting via the formula above preserves UTC strict per ADR-0011 §4 amendment (a). Pre-1970 timestamps (which would produce a negative result) are not physically realisable for an ETW event captured by a live agent; the agent treats any negative result as a capture-time anomaly and emits an `error`-level log line identifying the event with the suspect timestamp, then drops the event before envelope construction (parallel to the AC-006 log-and-drop pattern for the `process.name` strict normative).

### 2. PID-keyed volatile cache for `process.created_time` retention

ETW Launch events carry the process creation timestamp in `EVENT_HEADER.TimeStamp`; ETW Terminate events do not — they carry the termination timestamp. To emit `process.created_time` consistently on Terminate rows per ADR-0011 §4 amendment (a), the agent maintains an agent-local volatile cache mapping captured-process PIDs to their creation timestamps observed at Launch.

The cache is scoped tightly to honour ADR-0011 §6's "no agent-side mapping table, no state across restarts" wording. The cache:

- Is **keyed on PID**, not on `process.uid`. It does not participate in `process.uid` construction; `process.uid` is computed at Launch from ETW data directly per ADR-0011 §6 and the conversion mechanism above. The cache only enriches Terminate events that arrive after the corresponding Launch was captured in the same agent session.
- Is **volatile** (in-memory only). On agent restart the cache is empty; Terminate events for processes Launched before the agent restart emit `process.created_time = null` per the cache-miss path in AC-004.
- Is **populated at Launch event capture**: when a Launch event is dispatched by the ETW callback, the agent inserts `(PID, created_time_unix_nanos)` into the cache. The insert is the dispatch callback's only cache interaction — consistent with ADR-0009 §Decision part 3's "the dispatch callback does nothing but enqueue + generate event_id" constraint, treating the cache insert as a sibling enqueue operation (the cache lives alongside the ring, not downstream of it).
- Is **consulted and purged at Terminate event capture**: when a Terminate event is dispatched by the ETW callback, the agent looks up the cache entry for the Terminating PID; on hit, the `created_time` value is copied to the worker-bound event payload for emission, and the cache entry is purged immediately to bound the cache's residency; on miss, the worker emits `process.created_time = null` per the cache-miss path.

**PID-reuse race mitigation.** Windows reuses PIDs across process lifecycles. The purge-at-Terminate-consult discipline prevents stale entries from accumulating: any cache entry that survives past its corresponding Terminate is itself a Launch-without-corresponding-Terminate (the process is still alive or its Terminate event was never captured). To bound this, the agent additionally evicts cache entries via a periodic sweep (sweep cadence specified in §NFR forthcoming) using the OS query `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, PID)` — entries whose PID can no longer be opened (the process has terminated and its PID may have been reused) are evicted. The sweep is not load-bearing for correctness — the purge-at-Terminate discipline is — but it bounds cache memory under sustained agent sessions where some Terminate events were missed (ETW callback overflow per AC-009, or agent suspension by the OS scheduler under extreme load).

If a Terminate event arrives for a PID whose cache entry was evicted by the sweep before the Terminate (the rare case of a missed Terminate event followed by PID reuse followed by a captured Terminate for the reused PID), the Terminate row's `process.created_time` is `null` per the cache-miss path — the same emission as a routine cache miss. The agent does not attempt to distinguish missed-then-reused from never-Launched at emission time; both produce `null` honestly.

### 3. Kernel device path → Win32 path translation

ETW `Microsoft-Windows-Kernel-Process` Launch events carry the process image path as `ImageFileName` in kernel device path form (`\Device\HarddiskVolume2\Windows\System32\notepad.exe`) per ADR-0011 §4 row 3. CGES emits `process.file.path` in Win32 form (`C:\Windows\System32\notepad.exe`) per the same row.

The translation uses Windows `QueryDosDeviceW` to build a kernel-device-prefix → drive-letter mapping at agent startup and applies the inverse mapping to each captured `ImageFileName`:

```text
For each drive letter L in A..Z that QueryDosDeviceW(L:) returns a kernel path P for:
  prefix_map[P] = L:
At capture time, for an ETW ImageFileName I:
  if I starts with some prefix P in prefix_map:
    win32_path = prefix_map[P] + I[length(P):]
  else:
    fallthrough cases below
```

The mapping is built once at agent startup and cached for the agent process's lifetime. Volume changes during the agent's lifetime (new USB drive mounted, network drive added) are NOT reflected in the cached mapping in v0.1 — the agent emits the unresolved kernel path verbatim for any captured event whose `ImageFileName` does not match a cached prefix. A future SPEC may add volume-change event subscription to refresh the cache; agent restart is the v0.1 workaround.

**Fallthrough cases** (cases where the translation cannot produce a Win32 path; the agent's emission contract for each is explicit):

- **UNC network paths** (`\??\UNC\server\share\file.exe` or `\Device\Mup\server\share\file.exe`): translated to the standard UNC form `\\server\share\file.exe` via a dedicated rule prepended to the prefix-map lookup.
- **Junction-target paths** (kernel paths produced by reparse-point traversal): emitted as the kernel path verbatim; the agent does not resolve junction targets. The emitted `process.file.path` is the raw kernel form (e.g. `\Device\HarddiskVolume2\Junction\target\file.exe`) — recognisable as unresolved by its leading `\Device\` prefix.
- **Mount-point paths** (volumes mounted at a non-drive-letter NTFS mount point): emitted as the kernel path verbatim — the agent does not enumerate mount-point mappings in v0.1. Same recognisability as junctions.
- **Removable media** (USB drives, optical drives): if mounted at agent startup and visible to `QueryDosDeviceW`, the mapping is cached and translation works normally; if mounted after agent startup, the kernel path is emitted verbatim per the fallthrough.
- **`ImageFileName` empty or absent**: per ADR-0011 §5 agent normative for top-level `process.name`, the Launch event is logged-error and dropped before envelope construction; the `process.file.path` emission never occurs. AC-006 exercises this path.

The agent emits `process.file.path` as-captured-then-translated; no canonicalisation (no resolution of `..`, no case normalisation, no trailing-slash handling) is performed in v0.1. ETW `ImageFileName` values are kernel-canonical by construction (the kernel produces them deterministically per process); preserving the source form aids forensic provenance.

## Non-functional requirements

Concrete parameters and bounds for the agent-side mechanisms specified in §Operational and the wire-emission contracts inherited from ADR-0008 / ADR-0009 / ADR-0011 / SPEC-003. NFR identifiers are scoped to this SPEC (NFR-005-NNN) to avoid collision with SPEC-001/002/003 NFR namespaces.

- **NFR-005-001 (dispatch-callback constraint — load-bearing).** The agent's ETW dispatch callback per ADR-0008 + ADR-0009 §Decision part 3 MUST do nothing but enqueue the captured event into the ring buffer, generate the `event_id` UUIDv7 per ADR-0009 §Decision part 1, and insert the PID-keyed cache entry at Launch per §Operational §2. All per-event heavier work — JCS canonicalisation per SPEC-003, signature computation per SPEC-003 §FR-010, kernel-device-path translation per §Operational §3, ETW-timestamp conversion per §Operational §1, and `process.uid` construction per ADR-0011 §6 — MUST happen on the worker thread draining the ring, not in the callback. The Phase 0 spike per `docs/spikes/2026-05-23-etw-process-events.md` and ADR-0008 §Empirical justification measured **7649 events lost in 25 s under a 80 ms in-callback sleep + 1 KB × 2 buffers + three 200-process bursts** — the falsifiable empirical evidence that any non-trivial callback work risks ETW buffer overflow at the kernel side. This NFR is not negotiable; it locks the architecture's reason-to-exist. AC-009 is the standing regression guard.

- **NFR-005-002 (ring sizing triple).** The agent's in-memory ring buffer per ADR-0009 §Decision part 3 is bounded in events (not bytes) with the following v0.1 parameters:
  - **Ring size:** `65536` events. At the spike's measured steady-state background rate of ~1.1 k events/sec per ADR-0008 §Empirical justification, this provides ~1 minute of buffer headroom against an ingest-side stall. The ring is not designed to ride out extended server outages — that is the persistent buffer's role, deferred per §Out of scope item 3.
  - **Max batch size:** `1024` events. Bounds the JCS canonicalisation cost and signature input size per envelope, and corresponds to ~1 second of buffer drain at the spike's steady-state rate. Reduces dispatch tail latency at the cost of more frequent POSTs under sustained load.
  - **Max latency:** `5000` milliseconds. Time-based flush trigger; bounds end-to-end event freshness when the event rate is too low to trigger the size-based flush. Sub-30 s by construction so the SPEC-001 timer fallback is rarely the effective trigger under any non-quiet load. AC-001 marquee's 45 s budget per D7 leaves comfortable headroom against this 5 s flush bound.

  The triple `(65536, 1024, 5000)` is the v0.1 working set. Any subsequent change requires a SPEC-005 amendment citing the empirical or operational reason for the change.

- **NFR-005-003 (`events_dropped_total` transport surface).** The `events_dropped_total` counter per ADR-0009 §Decision part 3 is exposed on the wire as a field in `body.agent` of the outer signed envelope per SPEC-003 §Data contracts. Field type is unsigned 64-bit integer; emitted in every envelope regardless of value; monotonically non-decreasing within an agent process lifetime; resets to `0` on agent restart (consistent with the ring being volatile per ADR-0009 §Decision part 3). No separate metrics path is introduced in v0.1; the envelope-side field is the single transport surface, alertable at ingest via the existing event pipeline. A future metrics SPEC may add an additional path; the envelope-side field stays load-bearing regardless.

- **NFR-005-004 (D7 marquee budget restatement + CI logging).** The end-to-end marquee per AC-001 MUST complete within `45.0` seconds wall-clock time, measured from probe-spawn timestamp to both-rows-readable in ClickHouse. The harness MUST log `marquee_elapsed_seconds` at level `info` on every AC-001 run, including the value, regardless of pass or fail. The field MUST be visible in the CI run output (not gated behind a verbose flag). This NFR is the D7 ratification's load-bearing artifact per the Phase 3 opening protocol; the threshold is not negotiable in v0.1. Exceeding the threshold fails AC-001 with a clear log line identifying the elapsed value.

- **NFR-005-005 (AC-007 retry tolerance bound).** The AC-007 PPID race test per §Acceptance criteria is timing-sensitive: the ETW dispatch latency between parent termination and child Launch may or may not produce the unresolvable-parent condition on any single test run. The test MUST tolerate up to 3 retry attempts with 500 ms backoff between attempts; if the unresolvable-parent condition is not produced within 3 attempts, the test fails with a log line identifying the retry count and a diagnostic indicating possible runner-side timing assumptions are broken. Tightening the bound (more retries, longer backoff) without empirical justification dilutes the test's signal; loosening it (fewer retries) increases flake risk.

- **NFR-005-006 (PID-keyed cache sweep cadence).** The periodic sweep per §Operational §2 runs every `60` seconds on a dedicated agent thread. Each sweep iterates all cache entries; for each PID, the sweep calls `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, PID)`; entries whose PID can no longer be opened are evicted. The sweep is non-blocking with respect to the dispatch callback (cache reads / writes use a lock-free or fine-grained-locked structure; exact implementation is Phase 3.5 concern). Cadence is conservative: at the spike's steady-state rate and Windows' 22-bit PID space, cache memory stays well under 1 MB worst case under sustained Launch-without-Terminate scenarios. A future SPEC may reduce the cadence under empirical evidence that the cache memory pressure is not material.

- **NFR-005-007 (volume-change subscription deferral restatement).** The kernel-device-prefix mapping per §Operational §3 is built once at agent startup and not refreshed during the agent's lifetime. Volume changes after startup (USB drive mounted, network drive added, removable media inserted) are NOT reflected; events for those volumes emit unresolved kernel paths verbatim per the §Operational §3 fallthrough. Refreshing the mapping in response to `WM_DEVICECHANGE` or similar OS notifications is deferred to a future SPEC that may also generalise to mount-point and junction enumeration. The agent-restart workaround is the v0.1 operational guidance.

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

- **AC-006 (top-level `process.name` strict normative + log-and-drop).** The agent MUST always emit `process.name` for the top-level `process` field of every Process Activity event per ADR-0011 §5 agent normative: "the agent MUST always emit `name` for the top-level `process` field. Launch events always carry `ImageFileName`; failure to produce a top-level `name` indicates a captured event the agent cannot honestly represent — the event is logged-error and dropped before envelope construction." When this condition is violated at capture time (the agent's `basename(ImageFileName)` produces an empty string, or `ImageFileName` itself is empty or unresolvable to a Win32 path), the agent MUST log an `error`-level structured line identifying the dropped event (including the captured `ProcessID`, `activity_id`, and the reason for the drop) and MUST NOT include that event in any subsequent outer signed envelope.

  Test mechanism: integration test that injects a synthetic captured-event record at the ring enqueue boundary with `process.name` set to the empty string (bypassing the ETW capture path, which never produces this condition empirically per ADR-0008's spike evidence; the test exercises the agent's defensive contract, not a reachable runtime path). Two assertions: (i) the agent's `error`-level log stream contains a line matching the drop-event shape with the synthetic event's `ProcessID`; (ii) the next outer signed envelope POSTed contains zero events whose `process.pid` equals the synthetic event's `ProcessID`. The synthetic injection is necessary because the strict normative defends against an agent bug or a future ETW edge case, not against any empirically-observed live condition.

- **AC-007 (`parent_process` pid-only under PPID race).** For Launch events whose `ParentProcessID` does not match any process the agent can introspect at capture time (the parent has already terminated, the PID has been reused, or the parent is otherwise unresolvable), the agent MUST emit `parent_process` as a pid-only object per ADR-0011 §5 agent normative: `parent_process.pid` populated, `parent_process.name` omitted entirely (the field is absent from the emitted JSON, not present with a null or sentinel value), no resolution flag added (no `parent_process.name_resolved` or similar discriminator field). The persisted ClickHouse row MUST satisfy `parent_process.pid IS NOT NULL AND parent_process.name IS NULL`, with the NULL distinguishable from an emitted-null (the field is absent in the source JSON before storage).

  For Launch events whose `ParentProcessID` does match a process the agent can introspect, the agent emits `parent_process.name` populated per the §4 ETW field mapping table — this is the nominal case and is exercised by the AC-001 marquee (where `cmd.exe`'s parent — typically the test harness — is alive throughout the probe lifecycle and resolves cleanly).

  Test mechanism: integration test that launches a probe parent process, captures its PID, terminates the parent, and then launches a child process that the kernel assigns the dead parent's PID as `ParentProcessID` (the PID-reuse path requires sequencing on a Windows test runner; on a slow runner the dead-parent path without PID reuse is the same race observationally and is the simpler test setup). The test asserts the persisted child Launch row satisfies the absent-`parent_process.name` contract. Because the PPID race is timing-sensitive, the test may need retries; the §NFR (forthcoming) for AC-007 retry tolerance is a Phase 3.3.X concern.

- **AC-008 (`events_dropped_total` visibility under ring overflow).** The agent's in-memory ring buffer is bounded in events (not bytes) per ADR-0009 §Decision part 3, with the exact ring size specified in §NFR (forthcoming). On overflow, the agent drops the oldest events in FIFO order and increments an `events_dropped_total` counter exposed as an observable. When the ring's effective capacity is exhausted by a sustained ingest-side stall (server unreachable for longer than the ring can absorb at the active capture rate), `events_dropped_total` MUST become observable to the operator via the surface specified in §NFR (forthcoming: candidate surfaces per ADR-0009 §Decision part 3 are an envelope-level field, a separate metrics path, or both).

  The test exercises a deterministic ring-overflow path: with the ring sized to `N` events for the test configuration (N is small enough to trigger overflow within seconds; exact value is a §NFR concern), the test injects `N + K` synthetic events at the ring enqueue boundary (bypassing the ETW capture path; synthetic injection is the same mechanism as AC-006) and asserts that (i) `events_dropped_total` reports exactly `K` after injection completes; (ii) the events retained in the ring are the most recent `N` (FIFO drop of the oldest `K`); (iii) the counter is monotonic — repeating the overflow cycle increments the counter, never resets within an agent lifetime.

  This AC is independent of the live ETW capture path; it exercises the ring's defensive contract under sustained backpressure, not a routine operating condition. Routine operation per ADR-0008's spike evidence (1.1 k events/sec absorbed with zero loss in 10 s of background activity) keeps the ring well below capacity.

- **AC-009 (`events_lost` ETW buffer pressure via side-channel helper).** Under deliberately-induced ETW buffer pressure, the side-channel helper module's `events_lost(session_name)` function per ADR-0008 §Decision part 2 MUST return a non-zero value monotonically increasing with sustained pressure. The test reproduces the Phase 0 spike's empirical conditions per `docs/spikes/2026-05-23-etw-process-events.md` and ADR-0008's §Empirical justification: ETW session configured with 1 KB × 2 buffers (the smallest valid configuration), the agent's ETW dispatch callback artificially sleeps for 80 ms per event (a test-only configuration hook; production code path has no sleep), and the test generates three bursts of 200 short-lived processes each.

  Three assertions: (i) `events_lost(session_name)` returns a value `> 0` after the bursts complete (the spike measured 7649 lost in 25 s under these exact conditions; the AC asserts `> 0` rather than a specific value because the exact loss count varies with runner CPU contention and is empirically falsifiable rather than deterministically reproducible); (ii) consecutive calls to `events_lost(session_name)` during sustained pressure are monotonically non-decreasing (the counter never goes backwards; an OS-level reset would falsify the spike's REQ-B finding); (iii) the side-channel helper returns successfully — no Win32 error from the underlying `ControlTraceW(EVENT_TRACE_CONTROL_QUERY)` call — confirming the helper's contract is honored under load.

  Test mechanism: integration test running against real ETW (not synthetic), real ferrisetw session, and the agent's actual side-channel helper module. The 80 ms sleep is wired in via a Rust cfg flag or a constructor parameter on the dispatch callback for test-only use; production code MUST NOT contain this sleep (Phase 3.5 implementation gates this). The 200-process bursts use a tiny no-op probe (`cmd.exe /c rem`) spawned in parallel; the test harness collects PIDs and waits for kernel reaping before invoking `events_lost`.

  Empirical falsifiability of the spike claim is the AC's core value: a future ferrisetw upgrade or Windows kernel change that quietly removes the lost-events signal would surface here, not in production telemetry. This is the load-bearing defense for the dispatch-callback NFR forthcoming in §NFR (the spike's 7649-lost evidence is the NFR's empirical anchor).

## Failure modes

The agent's failure-mode contract for SPEC-005 extends SPEC-001/002/003's exit code range with one new terminal code (exit `9`, introduced by AC-002 for ETW privilege failures) and three new non-fatal conditions specific to the ETW capture + ring + envelope path.

### Terminal exits (process terminates with the exit code)

| Failure | Detection | Exit code | Behavior / stderr |
|---|---|---|---|
| ETW session open: insufficient privilege | `OpenTrace` / `StartTrace` returns Win32 error `5` (`ERROR_ACCESS_DENIED`) or `1314` (`ERROR_PRIVILEGE_NOT_HELD`) | `9` | `cg-agent: insufficient privilege to open Microsoft-Windows-Kernel-Process ETW session; run as elevated user or LocalSystem` (per ADR-0010 §Decision part 1; introduced by AC-002) |
| ETW session open: other terminal failure | `StartTrace` / `OpenTrace` returns any other terminal Win32 error not recoverable by retry (e.g., kernel driver missing, manifest corruption) | `1` | `cg-agent: ETW session open failed: <Win32 error code> <Win32 error message>` |
| Worker thread panic during envelope construction or signing | Unrecoverable panic on the worker thread draining the ring per ADR-0009 §Decision part 3 | `1` | Standard Rust panic message to stderr; the agent does not attempt to recover the worker thread; runtime exit |
| Cache thread panic | Unrecoverable panic on the cache sweep thread per §Operational §2 + NFR-005-006 | `1` | Standard Rust panic message; same posture as worker thread |

Exit codes `1`–`8` retained from SPEC-001/002/003 with their original semantics. SPEC-005 adds only exit code `9` (ETW privilege).

### Non-fatal conditions (log + continue; the agent does NOT exit)

| Condition | Detection | Behavior |
|---|---|---|
| Signed envelope rejected by ingest (bad signature, replayed nonce, stale timestamp, unknown agent) | Non-2xx HTTP response from `/v1/agents/heartbeat` per SPEC-003 §FR-012 | Log `warn` per SPEC-003; the events in the rejected envelope remain in the ring for retry on the next flush. If the next flush includes the same events under a fresh envelope (fresh `nonce`, fresh `sent_at`), server-side dedup keyed on `event_id` per ADR-0009 §Decision part 1 collapses duplicates if the ingest accepted the first envelope before responding. |
| Ring overflow — `events_dropped_total` incremented | Ring enqueue path per ADR-0009 §Decision part 3 + §NFR-005-002 | Log `warn` on the first drop after a quiet period, throttled to one log line per `60` seconds during sustained overflow (avoid log flood). The counter is exposed per NFR-005-003 regardless of log emission. |
| ETW `events_lost` observed non-zero by the side-channel helper | Periodic poll of `events_lost(session_name)` per ADR-0008 §Decision part 2; cadence co-located with the cache sweep (every `60` seconds per NFR-005-006) | Log `warn` with the observed `events_lost` value and the delta since the previous poll. Monotonically non-decreasing per AC-009; backwards motion would falsify ADR-0008's spike findings and is treated as a capture-time anomaly logged at `error`. |
| Cache sweep eviction count > 0 | Sweep iteration completion per §Operational §2 + NFR-005-006 | Log `debug` with the eviction count. Routine operation — Launch-without-corresponding-Terminate is normal at low rates (parent processes that outlive the agent; processes Launched before the agent started but Terminated during the agent's lifetime); no `warn` unless the eviction count exceeds a threshold to be defined empirically post-MVP. |
| Kernel-device-prefix translation fallthrough for a single event | The captured `ImageFileName` does not match any cached prefix per §Operational §3 | Log `debug` once per distinct unresolved prefix (throttled to avoid log flood for sustained unresolved paths); the event emits with the kernel path verbatim per §Operational §3 fallthrough cases. No `warn` — the fallthrough is a documented v0.1 limitation, not an anomaly. |

## Observability

All logs follow SPEC-001 / SPEC-003 JSON-on-stdout conventions per SPEC-001 §FR-009 + §Observability. New events specific to the SPEC-005 ETW capture + ring + envelope path:

| Event | Level | Required fields |
|---|---|---|
| ETW session opened | `info` | `session_name`, `provider_guid`, `buffer_size_kb`, `buffer_count` |
| ETW session closed (clean shutdown) | `info` | `session_name`, `events_captured_total`, `events_lost_total` |
| ETW session closed (zombie reclaimed) | `info` | `session_name`, `reclaim_path` (`"helper.stop_zombie"`) — per ADR-0008 §Decision part 2 |
| Process Launch event captured | `debug` | `pid`, `parent_pid`, `image_file_name` (kernel form), `event_id` |
| Process Terminate event captured | `debug` | `pid`, `exit_status`, `event_id` |
| Event dropped — `process.name` empty (log-and-drop) | `error` | `pid`, `activity_id`, `event_id`, `reason` (`"image_file_name_empty"` or `"basename_empty"`) — per AC-006 |
| Ring overflow (first drop after quiet, then throttled) | `warn` | `events_dropped_total`, `delta_since_last_log`, `ring_size_current` |
| `events_lost` observed non-zero (60 s poll) | `warn` | `events_lost`, `delta_since_last_poll`, `session_name` |
| `events_lost` decreased between polls (anomaly) | `error` | `events_lost_current`, `events_lost_previous`, `session_name` — never expected per AC-009; falsifies ADR-0008 spike findings |
| Cache sweep complete | `debug` | `entries_swept`, `entries_evicted`, `duration_ms` |
| Envelope built and POSTed | `info` | `sequence_number`, `events_count`, `batch_hash`, `body_size_bytes`, `signed_bytes_size` |
| Marquee AC-001 elapsed time (test harness only) | `info` | `marquee_elapsed_seconds`, `verdict` (`"pass"` or `"fail"`) — per NFR-005-004 |

Mandatory fields per line (timestamp, level, target, message) carry through from SPEC-001 §Observability; the table above lists only the event-specific structured fields.

## Ratification record

To be populated at the Phase 3.3.J ratification commit. Will record the chat-ratification of each load-bearing decision surfaced during Phase 3.3 drafting:

- The G1 + G5 + ring-sizing-triple + dispatch-callback NFR + D7 marquee budget ratifications established during the audit-first pass.
- The §Operational mechanism decisions (FILETIME conversion formula constant; PID-keyed cache scope per ADR-0011 §6 interpretation; kernel-device-path translation fallthrough cases).
- The §NFR concrete parameters (NFR-005-002 ring sizing triple; NFR-005-003 envelope-side transport; NFR-005-006 sweep cadence; NFR-005-005 AC-007 retry tolerance bound).
- The §Failure modes exit code `9` introduction for ETW privilege.
- Co-located SPEC-001 amendment 2026-05-23 (sequence_number semantics under multi-POST).

Each entry will record the recommended-default-and-rationale pattern established by SPEC-003 §Ratification record.

## References

- [ADR-0001](../adr/0001-monorepo-layout.md) — Monorepo layout. Places `agent/cg-agent/`, `services/ingest/`, `schemas/cges/v0.1/`, `docs/specs/`, `docs/adr/`, `docs/spikes/`.
- [ADR-0002](../adr/0002-language-per-component.md) — Language per component. Rust for the agent.
- [ADR-0006](../adr/0006-cges-ocsf-alignment.md) — CGES alignment with OCSF v1.3. §Out-of-scope third bullet is the PII deferral chain root.
- [ADR-0008](../adr/0008-etw-crate-selection.md) — ETW crate selection. ferrisetw 1.2.0 + side-channel helper. §Empirical justification anchors NFR-005-001.
- [ADR-0009](../adr/0009-event-delivery-and-buffer.md) — Event delivery semantics + buffer model. At-least-once delivery, ReplacingMergeTree dedup, in-memory ring. §Decision part 3 anchors NFR-005-001 (dispatch callback constraint) + NFR-005-002 (ring shape).
- [ADR-0010](../adr/0010-agent-privilege-model-mvp.md) — Agent privilege model + MVP installation posture. §Decision part 1 anchors AC-002 + §Failure modes exit code `9`.
- [ADR-0011](../adr/0011-cges-process-activity-v0-1.md) — Per-class CGES jurisprudence — Process Activity v0.1. §3 (activity_id discriminator), §4 (ETW field mapping table + amendment), §5 (PPID race), §6 (process.uid recipe), §Out-of-scope (PII deferral restatement).
- [SPEC-001](SPEC-001-agent-heartbeat.md) — Agent heartbeat. §Behavior scheduling reused; amendment 2026-05-23 (sequence_number multi-POST) co-located in this SPEC's ratification commit.
- [SPEC-002](SPEC-002-agent-enrollment.md) — Agent enrollment. Identity loaded before ETW session open.
- [SPEC-003](SPEC-003-mtls-signed-envelope.md) — mTLS 1.3 + signed envelope. Amendment 2026-05-23 part (a) (E2 wire shape with `events[]` + `batch_hash`) is the wire contract SPEC-005 emits into.
- [RFC 8785](https://www.rfc-editor.org/rfc/rfc8785) — JSON Canonicalization Scheme (JCS). The events array canonicalisation discipline per SPEC-003 §Security considerations.
- [RFC 9562](https://www.rfc-editor.org/rfc/rfc9562) — UUIDv7 specification. `event_id` format per ADR-0006 + ADR-0009 §Decision part 1.
- [ETW Microsoft-Windows-Kernel-Process schema](https://learn.microsoft.com/en-us/windows/win32/etw/microsoft-windows-kernel-process) — Microsoft's published schema for the events §Acceptance criteria + §Operational map.
- [docs/spikes/2026-05-23-etw-process-events.md](../spikes/2026-05-23-etw-process-events.md) — Phase 0 spike findings. Empirical evidence for NFR-005-001 (7649 lost @ 80 ms callback + 1 KB × 2 buffers) and the steady-state baseline (~1.1 k events/sec absorbed with zero loss) that informs NFR-005-002.
- [docs/engineering-notes.md](../engineering-notes.md) — Session 10 nine conventions + Session 11 Convention #5 extension + Session 11 full-SHA polling operational bullet. All apply to SPEC-005 drafting and the Phase 3.3.J ratification commit's repo-wide sweep.
- [Foundational Blueprint](../product/blueprint.md) — §7 (Agent-Server Secure Protocol). §17.11 (Advanced anonymisation out of MVP) — the framework deferral for the §Out of scope item 6 PII chain.
