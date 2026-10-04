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
//! Pressure: the smallest buffer pool the API allows (4 KB × 2 buffers;
//! the spike's 1 KB is below the Win32 minimum), the process keyword
//! (`0x10`, as the agent subscribes), an 80 ms in-callback sleep (a
//! test-only callback; the agent has no sleep) and 3 bursts × 200
//! processes. The empirical reach of `events_lost > 0` is the
//! load-bearing assertion; the exact lost count varies with runner CPU
//! contention and is not deterministic.
//!
//! Windows-only, real ETW, elevated: in the elevated gate
//! (`cargo test -p cg-agent -- --ignored --test-threads=1`). The test
//! owns a standalone ferrisetw session (independent of the agent's),
//! reclaims a leftover of it first and stops it at the end, so no
//! session is left behind.

#![cfg(windows)]

use cg_agent::etw::{events_lost, stop_session};
use ferrisetw::provider::Provider;
use ferrisetw::schema_locator::SchemaLocator;
use ferrisetw::trace::{TraceProperties, TraceTrait, UserTrace};
use ferrisetw::EventRecord;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

const SESSION_NAME: &str = "CGAgent-AC009-LostTest";
const KERNEL_PROCESS_GUID: &str = "22fb2cd6-0e7b-422b-a0c7-2fad1fd0e716";
const WINEVENT_KEYWORD_PROCESS: u64 = 0x10;
const CALLBACK_SLEEP_MS: u64 = 80;
const PROCESSES_PER_BURST: usize = 200;
const BURSTS_BEFORE_FIRST_POLL: usize = 2;
const BURSTS_BEFORE_SECOND_POLL: usize = 1;

#[ignore = "real ETW, elevated gate: cargo test -p cg-agent -- --ignored --test-threads=1"]
#[test]
fn ac_009_events_lost_under_deliberate_etw_buffer_pressure() {
    // A session of this name left by an earlier run would make the start
    // fail with ERROR_ALREADY_EXISTS.
    let _ = stop_session(SESSION_NAME);

    let callback_invocations = Arc::new(AtomicU32::new(0));
    let callback_invocations_for_handler = Arc::clone(&callback_invocations);

    // The callback deliberately sleeps to induce dispatch backpressure
    // (the spike's key insight: any non-trivial callback work risks
    // kernel-side buffer overflow).
    let provider = Provider::by_guid(KERNEL_PROCESS_GUID)
        .any(WINEVENT_KEYWORD_PROCESS)
        .add_callback(move |_record: &EventRecord, _schema: &SchemaLocator| {
            callback_invocations_for_handler.fetch_add(1, Ordering::Relaxed);
            thread::sleep(Duration::from_millis(CALLBACK_SLEEP_MS));
        })
        .build();

    let trace = UserTrace::new()
        .named(String::from(SESSION_NAME))
        .set_trace_properties(TraceProperties {
            buffer_size: 4,
            min_buffer: 2,
            max_buffer: 2,
            flush_timer: Duration::from_secs(1),
            ..TraceProperties::default()
        })
        .enable(provider);

    // Start and pump on a dedicated thread (the agent's pattern): start()
    // registers the session, process_from_handle() delivers events to the
    // callback on this thread until the session is stopped.
    let (started_tx, started_rx) = mpsc::channel::<Result<(), String>>();
    let pump = thread::spawn(move || match trace.start() {
        Ok((trace_session, handle)) => {
            let _ = started_tx.send(Ok(()));
            let _ = UserTrace::process_from_handle(handle);
            drop(trace_session);
        }
        Err(e) => {
            let _ = started_tx.send(Err(format!("{e:?}")));
        }
    });
    started_rx
        .recv()
        .expect("the pump thread reports the start")
        .expect("AC-009: the test session must start (elevated?)");

    // Allow the session to initialize.
    thread::sleep(Duration::from_secs(1));

    for _burst in 0..BURSTS_BEFORE_FIRST_POLL {
        spawn_process_burst(PROCESSES_PER_BURST);
        thread::sleep(Duration::from_millis(500));
    }
    // Let the kernel surface its lost-events counter.
    thread::sleep(Duration::from_secs(2));

    // First poll: events_lost MUST be > 0 (ADR-0008 §Decision part 2).
    let first_lost = events_lost(SESSION_NAME);

    for _burst in 0..BURSTS_BEFORE_SECOND_POLL {
        spawn_process_burst(PROCESSES_PER_BURST);
        thread::sleep(Duration::from_millis(500));
    }
    thread::sleep(Duration::from_secs(2));
    let second_lost = events_lost(SESSION_NAME);

    // Stop the session (process_from_handle returns) before asserting, so
    // a failure leaves nothing behind.
    let _ = stop_session(SESSION_NAME);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !pump.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(50));
    }

    let first_lost =
        first_lost.expect("AC-009: events_lost helper MUST return Ok (not Err) under pressure");
    assert!(
        first_lost > 0,
        "AC-009: events_lost MUST be > 0 after deliberate pressure \
         (callback_invocations={}; first_lost={})",
        callback_invocations.load(Ordering::Relaxed),
        first_lost
    );
    let second_lost =
        second_lost.expect("AC-009: events_lost helper MUST return Ok on second poll");
    assert!(
        second_lost >= first_lost,
        "AC-009: events_lost MUST be monotonically non-decreasing under sustained pressure \
         (first_lost={}; second_lost={}); a decrease falsifies ADR-0008 §Empirical justification",
        first_lost,
        second_lost
    );
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
