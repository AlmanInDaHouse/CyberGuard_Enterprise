# Spike — ETW Kernel-Process consumption via ferrisetw (Phase 0 of SPEC-005)

- **Date:** 2026-05-23
- **Session:** 10
- **Scope decided by:** D1 (re-decision authorising Option 1, ferrisetw + raw-Win32 side-channel) and D4 (CI privilege assumption)
- **Outcome:** all three local gates GREEN; CI gate **skipped by decision**, accepted as documented risk in ADR-0010 (forthcoming). Phase 1 (ADR drafting) authorised.

## Purpose

Validate, before any ADR commits decisions to a specific Rust ETW consumer crate, that:

1. The lost-event counter (`EVENT_TRACE_PROPERTIES.EventsLost`) is **reachable** through some viable mechanism — direct API on ferrisetw, or a raw-Win32 side-channel keyed on the agent-owned session name.
2. ETW session cleanup is reliable on **clean Drop**, and that a force-killed agent leaves a reclaimable zombie (consistent with the Windows ETW contract).
3. The chosen mechanism is **falsifiable**: an `EventsLost` counter that always returns `0` is indistinguishable from a placebo, so the spike must deliberately induce a lost condition and observe the counter move.

Three explicit gates, three explicit stop conditions if any failed.

## Method

- Crate under test: [`ferrisetw 1.2.0`](https://crates.io/crates/ferrisetw) (published 2024-06-27; repo `n4r1b/ferrisetw`, default branch `master`, last push 2025-10-20 — actively maintained, 91 stars).
- Provider: **`Microsoft-Windows-Kernel-Process`** (GUID `{22fb2cd6-0e7b-422b-a0c7-2fad1fd0e716}`). Manifest provider; opcode `1` = ProcessStart, opcode `2` = ProcessStop.
- Spike harness: throwaway Rust crate at `c:\tmp\etw-spike\` (not committed). Three CLI subcommands: `run` (open session, log events, poll `EventsLost` every 2 s), `run --induce-loss` (1 KB × 2 buffers, 80 ms in-callback sleep), `query <session-name>` (read `EventsLost` from a session by name without stopping it).
- Side-channel: raw `ControlTraceW(NULL, name, &props, EVENT_TRACE_CONTROL_QUERY)` for `EventsLost`, and `ControlTraceW(NULL, name, &props, EVENT_TRACE_CONTROL_STOP)` for zombie reclaim. Both via `windows-sys` 0.59; bridging contract is the agent-owned session name.
- Host: Windows 11 Home 10.0.26200.8457, elevated PowerShell (`IsInRole(Administrator) = True`).

## Findings against the three gates

### REQ-A.1 — Clean Drop releases the session ✅ GREEN

`run --seconds 10` against the default ferrisetw buffers (32 KB):

```text
starts    : 12
ends      : 12
other     : 13432
EventsLost: 0
dropping trace... (Drop should STOP the session)
```

`logman query -ets | Select-String 'CG-Agent-KernelProcess-Spike'` after exit returned empty. Ferrisetw's `non_consuming_stop` (called from the `Drop` impl on `UserTrace`) invokes `close_trace` followed by `ControlTrace(EVENT_TRACE_CONTROL_STOP)`, which is what the OS needs to release the named session object.

### REQ-A.2 — Force-kill leaves a zombie; relaunch reclaims it ✅ GREEN

Sequence:

1. Started the spike under `Start-Process -PassThru`.
2. After 3 s, `Stop-Process -Id $p.Id -Force` — Windows `TerminateProcess`, no Drop runs.
3. `logman query -ets` → `CG-Agent-KernelProcess-Spike  Seguimiento  Activo` (zombie alive, as expected by the Windows ETW contract: sessions are OS-scoped, not process-scoped).
4. `etw-spike.exe query CG-Agent-KernelProcess-Spike` → `EventsLost: 0` — proves the raw-Win32 side-channel reads zombie sessions too, not just live ones.
5. `etw-spike.exe run --seconds 10` → first stdout line: `reclaim   : pre-existing session stopped (was a zombie)`, then ran normally; final `logman query -ets` clean.

This is a meaningful result for the agent's lifecycle design: the agent **must** reclaim its own zombie on startup before opening a fresh session, otherwise `StartTraceW` will return `ERROR_ALREADY_EXISTS` and the agent will never recover from its own crash. The reclaim mechanism is not optional.

### REQ-B — Side-channel falsifiability under induced pressure ✅ GREEN

`run --induce-loss --seconds 25` with 1 KB buffers (the smallest the OS accepts), `min=1 max=2`, and an 80 ms `std::thread::sleep` inside the callback. In a second non-elevated shell, three bursts of `1..200 | ForEach-Object { Start-Process cmd /c exit }` during the window.

`EventsLost` trajectory across the 25 s window:

```text
[+  2.0s] starts=0 ends=0 other=24  lost=38
[+  4.0s] starts=0 ends=0 other=49  lost=346
[+  6.0s] starts=2 ends=0 other=72  lost=875
[+  8.0s] starts=2 ends=0 other=97  lost=1389
[+ 10.0s] starts=2 ends=0 other=122 lost=2097
[+ 12.0s] starts=2 ends=0 other=147 lost=2175
[+ 14.0s] starts=2 ends=0 other=172 lost=2549
[+ 16.1s] starts=2 ends=0 other=197 lost=2583
[+ 18.1s] starts=2 ends=0 other=222 lost=2732
[+ 20.1s] starts=2 ends=0 other=247 lost=2856
[+ 22.1s] starts=2 ends=0 other=272 lost=3478
[+ 24.1s] starts=2 ends=0 other=297 lost=7245
EventsLost (final): 7649
```

The counter is **monotonic** (only ever increases — useful: the agent can sample periodically and report deltas) and clearly correlates with the bursts (notice the jump from `lost=3478` to `lost=7245` between +22 s and +24 s, the third burst). Only `starts=2` events reached the callback because the deliberate 80 ms sleep starved the dispatcher; this is the empirical justification for the SPEC-005 hard NFR that the dispatch callback must do nothing but enqueue to a ring buffer.

## Side observations (carried forward into SPEC-005)

Findings outside the three gates that the SPEC drafting must absorb:

- **Background baseline ≈ 1.1 k events/sec.** REQ-A.1 saw `other=13432` over 10 s on an idle developer machine running Docker Desktop. Useful real-world data point for the buffer-sizing NFR — 32 KB defaults handle this trivially with zero loss, but the SPEC should specify expected upper bounds and the agent's response to sustained exceedance.
- **Dispatch-callback latency NFR — empirical justification.** 80 ms callback + 1 KB × 2 buffers + ~600 short-lived processes → 7649 lost in 25 s, only 2 `event_id=1` records reached the callback. The SPEC NFR ("the dispatch callback does nothing but enqueue to the agent's ring buffer; any per-event work happens in a separate worker") is **load-bearing**, not aspirational, and this number is its evidence.
- **`EventsLost` is monotonic across the session lifetime.** The SPEC should specify `cg_events_lost_total` as a delta-computed counter (sample, subtract last reading, emit delta), not as the raw OS counter.
- **No process rundown by default.** REQ-A.2 saw `ends=14 > starts=12` after the reclaim run, because some pre-existing processes terminated during our window without us having seen their original launch. SPEC-005 must declare explicitly: **Launch events are only captured for processes that start after the agent's session opens; pre-existing processes are not enumerated.** ETW has rundown providers that could populate this, but they're out of scope for v0.1.
- **Kernel-Process emits many more opcodes than 1/2.** The `other` counters in every run are large (13432 / 6656 / 308). The provider emits ThreadStart, ImageLoad, and others under the same manifest. SPEC-005's parsing must filter strictly to `event_id == 1 || event_id == 2`; everything else is discarded.

## Gaps in ferrisetw 1.2.0 (documented here as the source of truth for ADR-0008)

Two distinct gaps surfaced. Both are in the published 1.2.0 release; both are addressed in unreleased `master`; both are recovered via the same raw-Win32 side-channel pattern.

1. **No `EventsLost` accessor.** Confirmed by direct source inspection: `src/query.rs` exposes only `ProfileSource::sample_interval` and `max_pmc`; `grep -nE "lost|EventsLost|MISSED"` over the entire `src/` tree returns zero hits; `src/native/evntrace.rs:59` carries a maintainer comment — *"ControlTraceW(EVENT_TRACE_CONTROL_QUERY) might tell us if the buffers are empty or not"* — acknowledging the API gap.
2. **No `stop_if_exist` method on `TraceBuilder`.** The `master` branch added `pub fn stop_if_exist(b: bool) -> Self` and a default of `true` in `UserTrace::new()`, but neither is in the 1.2.0 release. Confirmed by reading the cached registry source at `~/.cargo/registry/src/index.crates.io-…/ferrisetw-1.2.0/src/trace.rs:259-275` and by the build error `no method named 'stop_if_exist' found for struct 'TraceBuilder<T>'` when called. Without this, `start_and_process` returns `Err(EvntraceNativeError::AlreadyExist)` on a name collision and the agent cannot recover from its own zombie.

Both gaps are recovered by the same agent-local helper module: `etw::session` (or whatever name ADR-0008 settles) exposing `events_lost(session_name) -> u32` and `stop_zombie(session_name) -> StopOutcome` via `windows-sys`. Total surface in the spike: ~80 lines including `#[repr(C)]` blob plumbing.

A third, lesser gap worth noting in ADR-0008: **`ferrisetw::trace::TraceError` does not impl `std::error::Error`** in 1.2.0, so it cannot be `?`-chained into `anyhow::Error` without an explicit `.map_err()`. Cosmetic; the agent's existing error handling pattern (`thiserror` for domain errors) absorbs it cleanly.

## Decision status

D1 ratified as Option 1 (ferrisetw + raw-Win32 side-channel). The spike empirically supports this: ferrisetw's session-lifetime + parsing path works well, the side-channel for `EventsLost` is non-placebo, and the second side-channel for zombie reclaim follows the exact same pattern. Both gaps are localised to one module in the agent.

D4's CI privilege assumption (windows-latest `runneradmin` can open Kernel-Process) is **not** validated by this spike — see "CI gate skipped" below.

## CI gate skipped — accepted risk

The original Phase 0 plan included a CI validation step: run a minimised version of the spike on a GitHub Actions `windows-latest` runner via `workflow_dispatch` on a throwaway branch, confirming `runneradmin` has the privilege bits to open the Kernel-Process provider.

Decision: **skip**, accepted risk to be carried in ADR-0010 (privilege model). Rationale:

- Local elevated behaviour matched the Windows ETW contract on every observable axis (clean Drop, force-kill zombie, side-channel correctness).
- The CI delta is exactly one question: "does `runneradmin` carry the same privilege bits as a locally-elevated user?". That question gets answered for free the first time SPEC-005's AC-001 marquee runs in CI during Phase 5, at zero incremental cost.
- Pre-validating on a throwaway branch before AC-001 exists adds ceremony for no extra signal — the AC-001 harness *is* the validation.

ADR-0010 must document this with its two fallback paths: (a) escalate runner privileges via a SYSTEM trampoline (e.g. `psexec /s`); (b) move the marquee AC out of CI with honest documentation. The choice between (a) and (b) is deferred until/unless the assumption is contradicted in Phase 5.

## Follow-ups

Co-located with the document that generated them, per the convention adopted in Session 10 (recorded in [engineering-notes.md](../engineering-notes.md)).

- **Upstream PR to ferrisetw exposing `events_lost()` on the session handle.** Owner: Claude Code (with Manuel's review). Trigger: after SPEC-005 is `Accepted`. Outcome semantics: if accepted upstream and released, the agent migrates the side-channel to the native API via a follow-up SPEC-005 amendment and this entry closes. If rejected or stalled > 90 days from PR opening, the side-channel becomes permanent and this entry closes with that reason recorded.
- **Upstream PR to ferrisetw publishing `stop_if_exist` in a 1.3.0 release.** Same trigger and same outcome semantics as the `events_lost` follow-up. The method already exists on `master`; the upstream commitment is "request a release tag", not "request new code". May be a single combined PR with the `events_lost` work, at Claude Code's discretion when drafting.

## References

- `ferrisetw` 1.2.0 source — `src/trace.rs`, `src/query.rs`, `src/native/evntrace.rs`.
- Microsoft Learn — [Event Tracing: Specifying advanced session configuration](https://learn.microsoft.com/en-us/windows/win32/api/evntrace/ns-evntrace-event_trace_properties), [ControlTraceW](https://learn.microsoft.com/en-us/windows/win32/api/evntrace/nf-evntrace-controltracew).
- Spike harness sources — `c:\tmp\etw-spike\src\{main.rs, etw_query.rs}`. Throwaway; not committed.
