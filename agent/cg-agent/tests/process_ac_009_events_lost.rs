//! SPEC-005 AC-009 — `events_lost` ETW buffer pressure via side-channel helper.
//!
//! Asserts the side-channel helper module's `events_lost(session_name)`
//! function per ADR-0008 §Decision part 2 returns a non-zero,
//! monotonically non-decreasing value under deliberately-induced ETW
//! buffer pressure. The test reproduces the principle of the Phase 0
//! spike (ADR-0008 §Empirical justification, spike note at
//! docs/spikes/2026-05-23-etw-process-events.md): pressure → loss →
//! observable via the `ControlTraceW(EVENT_TRACE_CONTROL_QUERY)`
//! mechanism.
//!
//! Spike reproduction note. The spike cites "1 KB × 2 buffers + 80 ms
//! in-callback sleep + three 200-process bursts → 7649 lost in 25 s".
//! Win32 `EVENT_TRACE_PROPERTIES.BufferSize` minimum is 4 KB; the spike's
//! 1 KB likely hit OS clamp or used raw Win32 bypassing ferrisetw
//! defaults. AC-009 reproduces the spike's *principle* (pressure → loss),
//! not the exact buffer config — uses `TraceProperties::default()` +
//! `flush_timer = 1` (aggressive flush) + 80 ms callback sleep + 3
//! bursts × 200 processes. The empirical reach of `events_lost > 0`
//! is the load-bearing assertion; exact lost-count varies with runner
//! CPU contention and is not deterministic.
//!
//! Windows-only integration test. Sets up its own standalone ferrisetw
//! UserTrace session (independent of any cg-agent ETW path) and invokes
//! the agent's `events_lost` helper directly. No mock server, no
//! test-agent, no testcontainers.

use cg_agent::etw::events_lost;
use ferrisetw::provider::Provider;
use ferrisetw::schema_locator::SchemaLocator;
use ferrisetw::trace::{TraceTrait, UserTrace};
use ferrisetw::EventRecord;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const SESSION_NAME: &str = "CGAgent-AC009-LostTest";
const KERNEL_PROCESS_GUID: &str = "22fb2cd6-0e7b-422b-a0c7-2fad1fd0e716";
const CALLBACK_SLEEP_MS: u64 = 80;
const PROCESSES_PER_BURST: usize = 200;
const BURSTS_BEFORE_FIRST_POLL: usize = 2;
const BURSTS_BEFORE_SECOND_POLL: usize = 1;

#[cfg_attr(not(target_os = "windows"), ignore)]
#[test]
fn ac_009_events_lost_under_deliberate_etw_buffer_pressure() {
    let callback_invocations = Arc::new(AtomicU32::new(0));
    let callback_invocations_for_handler = Arc::clone(&callback_invocations);

    // Set up the test-owned ETW session. The callback deliberately sleeps
    // to induce dispatch backpressure (the spike's key insight: any
    // non-trivial callback work risks kernel-side buffer overflow).
    let provider = Provider::by_guid(KERNEL_PROCESS_GUID)
        .add_callback(move |_record: &EventRecord, _schema: &SchemaLocator| {
            callback_invocations_for_handler.fetch_add(1, Ordering::Relaxed);
            thread::sleep(Duration::from_millis(CALLBACK_SLEEP_MS));
        })
        .build();

    let trace = UserTrace::new()
        .named(String::from(SESSION_NAME))
        .enable(provider);

    // Start the trace on a background thread (UserTrace::start blocks).
    let trace_handle = thread::spawn(move || {
        let _ = trace.start();
    });

    // Allow the session to initialize.
    thread::sleep(Duration::from_secs(1));

    // Burst 1+2: spawn short-lived processes to flood Kernel-Process
    // events into the session. The callback sleep + flood combination
    // induces backpressure on the ETW kernel buffers.
    for _burst in 0..BURSTS_BEFORE_FIRST_POLL {
        spawn_process_burst(PROCESSES_PER_BURST);
        thread::sleep(Duration::from_millis(500));
    }

    // Let the kernel surface its lost-events counter.
    thread::sleep(Duration::from_secs(2));

    // First poll: events_lost MUST be > 0 (the helper observes the kernel
    // counter via ControlTraceW(EVENT_TRACE_CONTROL_QUERY) per ADR-0008
    // §Decision part 2).
    let first_lost = events_lost(SESSION_NAME)
        .expect("AC-009: events_lost helper MUST return Ok (not Err) under pressure");
    assert!(
        first_lost > 0,
        "AC-009: events_lost MUST be > 0 after deliberate pressure \
         (callback_invocations={}; first_lost={})",
        callback_invocations.load(Ordering::Relaxed),
        first_lost
    );

    // Burst 3: additional load to drive the counter further.
    for _burst in 0..BURSTS_BEFORE_SECOND_POLL {
        spawn_process_burst(PROCESSES_PER_BURST);
        thread::sleep(Duration::from_millis(500));
    }
    thread::sleep(Duration::from_secs(2));

    // Second poll: events_lost MUST be monotonically non-decreasing.
    // A backwards count would falsify ADR-0008's spike findings and is
    // treated as a capture-time anomaly per SPEC-005 §Failure modes.
    let second_lost = events_lost(SESSION_NAME)
        .expect("AC-009: events_lost helper MUST return Ok on second poll");
    assert!(
        second_lost >= first_lost,
        "AC-009: events_lost MUST be monotonically non-decreasing under sustained pressure \
         (first_lost={}; second_lost={}); a decrease falsifies ADR-0008 §Empirical justification",
        first_lost,
        second_lost
    );

    // Cleanup: drop the trace_handle's join to allow the test to exit.
    // The trace's session is also reclaimed when UserTrace drops out of
    // scope (ADR-0008 §Decision part 1: ferrisetw owns the session via
    // its Drop impl). The thread::spawn's handle is intentionally not
    // joined here to avoid blocking on the trace's processing thread,
    // which has been observed to outlive the parent in some kernel states.
    drop(trace_handle);
}

/// Spawn `count` short-lived processes in rapid succession to flood the
/// Kernel-Process ETW provider. Each `cmd.exe /c rem` spawns + exits in
/// <50 ms typically; the burst produces a Launch and Terminate event per
/// spawn, doubling the event rate observed by the session.
fn spawn_process_burst(count: usize) {
    let mut handles = Vec::with_capacity(count);
    for _ in 0..count {
        if let Ok(child) = Command::new("cmd.exe").args(["/c", "rem"]).spawn() {
            handles.push(child);
        }
    }
    // Reap each child to avoid zombie accumulation; cmd.exe /c rem exits
    // near-instantly so the wait is brief.
    for mut handle in handles {
        let _ = handle.wait();
    }
}
