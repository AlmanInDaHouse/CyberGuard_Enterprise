# ADR-0008: ETW crate selection for Windows event capture

- Status: Accepted
- Date: 2026-05-23
- Deciders: Manuel (project owner), Claude (architecture advisor), Claude Code (implementation)

## Context

SPEC-005 (in drafting) is the first SPEC under the agent's Opción A scope (process telemetry). The agent must consume real-time events from the `Microsoft-Windows-Kernel-Process` ETW manifest provider — process Launch (opcode `1`) and Terminate (opcode `2`) on Windows. The wire format is determined by the manifest; the question this ADR settles is **how**, in Rust, the agent talks to ETW.

ADR-0002 fixed the agent in **Rust**, so the candidate space is Rust crates over the Win32 ETW APIs (`StartTraceW`, `EnableTraceEx2`, `OpenTraceW`, `ProcessTrace`, `ControlTraceW`, the `TdhXxx` parsing family). Candidates considered in this space:

- **`ferrisetw 1.2.0`** ([crates.io](https://crates.io/crates/ferrisetw); repo `n4r1b/ferrisetw`, last release 2024-06-27, master last push 2025-10-20 — actively maintained). A Rust port of Microsoft's KrabsETW C++ library. Provides a `UserTrace`/`KernelTrace` builder, callbacks per provider with parsed schemas (`Parser::create(record, &schema).try_parse::<T>("FieldName")`), and session lifecycle bound to a `Drop` impl.
- **`windows-rs` / `windows-sys` raw bindings.** Microsoft's official Win32 bindings. No abstraction over ETW; the agent writes the `EventTraceProperties` blob, the `StartTraceW` call, the `EVENT_TRACE_LOGFILEW` callback, and the `TdhGetEventInformation` parsing logic by hand.
- **Others.** `tracelogging-dynamic` emits ETW from Rust, it does not consume; `etw-reader` parses `.etl` files offline only, no real-time session; `krabsetw` is C++ (no Rust binding); `sealighter` is a binary tool. None match the SPEC-005 consumption use case.

A Phase 0 spike (Session 10, [docs/spikes/2026-05-23-etw-process-events.md](../spikes/2026-05-23-etw-process-events.md)) validated three explicit gates against ferrisetw 1.2.0 — clean Drop releases the session, force-kill leaves a reclaimable zombie, and the lost-event counter is reachable and falsifiable under induced buffer pressure. **All three GREEN.** The spike also surfaced two distinct gaps in the **published 1.2.0 release** (both addressed in unreleased `master`) which this ADR documents and mitigates rather than glosses.

## Decision

For the agent's Windows event consumption (SPEC-005 and successors of the same telemetry family — process, file, network, registry, image-load — when they land), the agent adopts:

1. **`ferrisetw 1.2.0`** for session lifecycle (start, stop, Drop), provider enabling, real-time event dispatch, and per-event TDH schema parsing.
2. **A small agent-local helper module** (working name `agent/cg-agent/src/etw/session.rs` or equivalent under the SPEC-005 module layout) that wraps two raw `ControlTraceW` calls via `windows-sys` 0.59:
   - `events_lost(session_name) -> Result<u32, u32>` — reads `EVENT_TRACE_PROPERTIES.EventsLost` via `EVENT_TRACE_CONTROL_QUERY` without stopping the session.
   - `stop_zombie(session_name) -> Result<StopOutcome, u32>` — calls `EVENT_TRACE_CONTROL_STOP` by name, returning `StopOutcome::NotFound` on `ERROR_WMI_INSTANCE_NOT_FOUND` (4201). Used on agent startup to reclaim a zombie session left by a previous abnormal termination, before opening a fresh session.

ferrisetw owns the session it created; the side-channel reaches the same OS object **by name**. The agent owns the session name.

### Documented gaps in ferrisetw 1.2.0

Disclosed here, not buried. Both are addressed in the unreleased `master` branch; both are recovered by the helper module above.

1. **No `EventsLost` accessor.** Confirmed by source inspection of the published 1.2.0 release: `src/query.rs` exposes only `ProfileSource::sample_interval` and `max_pmc`; `grep -nE "lost|EventsLost|MISSED"` over the entire `src/` tree returns zero hits; `src/native/evntrace.rs:59` carries a maintainer comment — *"ControlTraceW(EVENT_TRACE_CONTROL_QUERY) might tell us if the buffers are empty or not"* — acknowledging the API gap. For a security agent, observable event loss is a non-negotiable telemetry-quality signal; we cannot ship without it.
2. **No `stop_if_exist` method on `TraceBuilder`.** The `master` branch adds `pub fn stop_if_exist(b: bool) -> Self` and defaults `UserTrace::new()` to `true`; neither is in the 1.2.0 release. Without this, `start_and_process` returns `Err(EvntraceNativeError::AlreadyExist)` on a name collision, and the agent cannot recover from its own zombie after a force-kill / OS-level termination. Spike REQ-A.2 demonstrated that zombies are real (Windows ETW sessions are OS-scoped, not process-scoped), so this is a required recovery mechanism, not a nice-to-have.

A third gap also noted, classed differently: **`ferrisetw::trace::TraceError` does not impl `std::error::Error`** in 1.2.0, so it does not `?`-chain into `anyhow::Error`. **Load-bearing:** no. **Mitigation:** explicit `.map_err()` at the call boundary, using the agent's existing `thiserror`-based domain-error pattern. **Impact:** ergonomic only — no behavioural, security, or observability consequence. Recorded for completeness so the audit reads three gaps, not two — the asymmetry between "knows 2 gaps" and "knows 3 gaps" hurts more than the extra line.

### Mitigation — agent-local side-channel helper

`agent/cg-agent/src/etw/session.rs` (final path settled by SPEC-005) exposes the two helpers above. Total surface measured in the spike: ~80 lines including the `#[repr(C)]` `EventTracePropertiesBlob` plumbing, the two function bodies, and the `StopOutcome` enum. No transitive impact: the helper imports `windows-sys` (already a transitive dependency of the agent for DPAPI), exposes safe Rust types, and is the only module that talks raw `ControlTraceW`. The helper is independent of ferrisetw — ferrisetw could be swapped without touching it.

### Layering analysis

The framing "ferrisetw + side-channel is a layering violation" was considered and rejected. ferrisetw and the helper module are **peer consumers** of the same Win32 ETW API surface, not nested abstractions. The bridging contract is the **agent-owned session name** (a constant the agent picks; for SPEC-005, expected to be something like `CG-Agent-KernelProcess`). That contract is stable by construction because the agent owns both endpoints — ferrisetw's `TraceBuilder::named(...)` and the helper's first argument. If ferrisetw ever changed how it represents sessions internally (it does not; the named session is an OS object), the contract would still hold because we always identify the session by name to the kernel.

The bound for the layering decision is: the helper module is the only place in the agent that talks raw ETW. All other agent code goes through ferrisetw. This keeps the unsafe / FFI surface localised and auditable.

### Empirical justification (spike numbers)

From [docs/spikes/2026-05-23-etw-process-events.md](../spikes/2026-05-23-etw-process-events.md):

- Default ferrisetw buffers (32 KB, OS-chosen min/max) absorbed ~1.1 k events/sec of background Kernel-Process activity on a developer machine with **zero loss** across 10 s.
- Under deliberately-induced buffer pressure (1 KB × 2 buffers, 80 ms in-callback sleep, three bursts of 200 short-lived processes), the side-channel reported **7649 lost events** in 25 s, climbing monotonically. Counter is falsifiable: it moves when reality moves.
- Force-kill confirmed Windows ETW contract: zombie persists; `stop_zombie` (the helper's STOP-by-name) reclaims cleanly on relaunch.

## Alternatives considered

### A1 — `windows-rs` / `windows-sys` raw bindings (no abstraction)

Pros: no third-party crate, no gaps to mitigate, full control over every API call, no risk of an upstream maintainer abandoning the project. Microsoft-owned bindings already in the agent's tree (for DPAPI).

Cons: writing real-time ETW consumers in raw Win32 is *substantial* work — `EventTraceProperties` layout with trailing wide-char buffers, manual `TdhGetEventInformation` schema resolution, per-event-class property parsing (PID → `TDH_INTYPE_UINT32`, `ImageName` → `TDH_INTYPE_UNICODESTRING`, etc.), and the safety story around `LogfileHeader` / `EVENT_RECORD` pointers in the callback. From the spike's ~80-line helper for *one* QUERY call: a hand-rolled ferrisetw equivalent would be order-of-magnitude larger, plus a TDH parser whose size scales with the number of event classes consumed and must handle variation between Windows builds (TDH schemas can differ in optional fields). Two gaps avoided at the cost of substantially more code, all of it `unsafe`.

Rejected. The cost-benefit is wrong: ferrisetw + two helper functions wins on every axis except "no transitive dependency."

### A2 — Hybrid: ferrisetw + upstream-contribution-first (block on PR acceptance)

Pros: cleanest long-term posture — if upstream accepts `events_lost()` and a release tag for `stop_if_exist`, the agent ships against a clean 1.3+ API with no side-channel.

Cons: blocks SPEC-005 on upstream cadence (the maintainer's velocity, review depth, release timing) for a question that can be answered with ~80 lines of side-channel today. Not a question of *if* we contribute upstream — we will (see Follow-up below) — but of whether we *block* on it. Slowest-now option.

Rejected as the **blocker** path; retained as the **follow-up** path. The side-channel is the shim that decouples our schedule from upstream's.

### A3 — ferrisetw 1.2.0 + raw-Win32 side-channel for the documented gaps (chosen)

Pros: smallest reasonable code surface; ferrisetw handles the hard parts (session lifecycle, TDH dispatch, schema parsing), the helper handles the two surfaced gaps with ~80 lines per gap; spike-empirically validated on all three gates including falsifiability of the lost-events signal; layering boundary is honest (peer use of Win32, agent owns the bridging name); the gaps and their mitigation are documented, not hidden; future migration to a clean 1.3+ API is a single-PR shim removal.

Cons: two helpers exist that conceptually shouldn't (would not need to exist if 1.2.0 were complete); two upstream-contribution commitments are now on the agent's plate (tracked, not optional — see Follow-up); a future ferrisetw breaking change to session naming would force a coordinated update (unlikely, since the session name is what we pass to the kernel; ferrisetw forwards it, it doesn't redefine it).

Chosen.

## Consequences

### Positive

- SPEC-005 unblocked: the consumer crate is settled, the session lifecycle is well-understood, and the lost-events signal is observable and falsifiable. The dispatch path SPEC-005 needs is the one ferrisetw's `user_trace` example demonstrates (Provider GUID `{22fb2cd6-0e7b-422b-a0c7-2fad1fd0e716}`, event_id `1` / `2`, parser fields `ProcessID` / `ParentProcessID` / `ImageName` / `CommandLine` / `ExitCode`).
- The unsafe / FFI surface in the agent is localised to one module (the helper) that is the only place outside ferrisetw and DPAPI touching Win32 directly.
- The agent gains a small, generic API (`events_lost`, `stop_zombie`) that is reusable for future ETW providers under the same telemetry family (file, network, registry, image-load).
- Future migration to a clean ferrisetw 1.3+ API, if upstream accepts the contributions, is a localised removal of the helper module and its callsites.

### Negative

- The agent ships two helpers it conceptually would not need with a complete upstream API. They are tested in the spike, documented here, and tracked for removal — but they exist.
- Two upstream-contribution commitments are added to the project's near-future workload (after SPEC-005 is `Accepted`). See the spike note's Follow-ups section, where these are the durable record.
- A non-Microsoft third-party crate (`ferrisetw`) is now a runtime dependency of the agent. The maintainer's velocity is good as of writing (last push 2025-10-20, 91 stars); a future change in maintenance posture could leave us holding the bag, in which case the fallback is A1 (raw bindings — costly but always reachable). Risk acknowledged, not mitigated further.

### Neutral

- The Phase 0 CI gate (validate `windows-latest` `runneradmin` can open the Kernel-Process provider) was **skipped by chat decision** and is carried forward as accepted risk in ADR-0010 (Agent privilege model & installation posture for MVP). The first execution of the SPEC-005 AC-001 marquee in CI is the free first-use validation; ADR-0010 specifies the fallback paths if that validation fails.
- `ferrisetw::trace::TraceError` not implementing `std::error::Error` is absorbed by an explicit `.map_err()` at the boundary in the agent. Cosmetic.

## Compliance

- All ETW consumption in the agent goes through ferrisetw, **except** the two operations covered by the helper module (`events_lost`, `stop_zombie`). New ETW-related code that needs a raw Win32 call must either extend the helper module (if it follows the same name-bridged pattern) or open a new ADR.
- The helper module's name argument must always equal the name passed to ferrisetw's `TraceBuilder::named(...)`. Constant-define the session name in one place (exact location settled by SPEC-005); do not duplicate the literal.
- The follow-up upstream-PR commitments live in the spike note ([docs/spikes/2026-05-23-etw-process-events.md](../spikes/2026-05-23-etw-process-events.md)) per the convention recorded in [docs/engineering-notes.md](../engineering-notes.md) Session 10 — follow-ups co-located with the document that generated them. The spike generated the gap evidence; the spike carries the upstream-PR commitments.
- If ferrisetw releases a 1.3+ that includes `events_lost()` and `stop_if_exist`, the agent migrates the side-channel callsites to the upstream API and the helper module shrinks or disappears. The migration is a follow-up SPEC amendment under SPEC-005 (or its successor that introduces the change), not a new ADR — the *decision* this ADR records is *which* crate; a fresh release of the same crate does not supersede that decision.
- This ADR does not pre-decide ETW consumption for non-Windows platforms (Linux audit, macOS Endpoint Security Framework). When those platforms land, they open their own ADRs.

## References

- [ADR-0001](0001-monorepo-layout.md) — Monorepo layout. The agent lives at `agent/cg-agent/`; ETW code goes under it.
- [ADR-0002](0002-language-per-component.md) — Language per component. The agent is Rust; this ADR picks a Rust crate.
- [docs/spikes/2026-05-23-etw-process-events.md](../spikes/2026-05-23-etw-process-events.md) — Phase 0 spike findings. Source of truth for the documented gaps and the empirical justification.
- [docs/engineering-notes.md](../engineering-notes.md) — Session 10 entry on follow-up co-location.
- [SPEC-005](../specs/SPEC-005-agent-process-telemetry-windows-etw.md) — Agent process telemetry (Windows ETW Kernel-Process). First consumer of this decision.
- ADR-0010 — Agent privilege model & installation posture for MVP. Carries the CI privilege assumption skipped from this spike's gates.
- `ferrisetw` 1.2.0 source — `src/trace.rs` (`TraceBuilder`, `Drop` semantics, no `stop_if_exist` in the published release), `src/query.rs` (the only "query" surface, exposing only `ProfileSource::sample_interval` and `max_pmc`).
- `ferrisetw` 1.2.0 source — **`src/native/evntrace.rs:59`** carries the maintainer comment *"ControlTraceW(EVENT_TRACE_CONTROL_QUERY) might tell us if the buffers are empty or not"*. Strongest documentary evidence that the missing `EventsLost` accessor is an upstream-acknowledged gap, not a misunderstanding on our side; preserved as a primary citation because its evidentiary weight is disproportionate to its size.
- Microsoft Learn — [`EVENT_TRACE_PROPERTIES`](https://learn.microsoft.com/en-us/windows/win32/api/evntrace/ns-evntrace-event_trace_properties), [`ControlTraceW`](https://learn.microsoft.com/en-us/windows/win32/api/evntrace/nf-evntrace-controltracew), [`StartTraceW`](https://learn.microsoft.com/en-us/windows/win32/api/evntrace/nf-evntrace-starttracew).
