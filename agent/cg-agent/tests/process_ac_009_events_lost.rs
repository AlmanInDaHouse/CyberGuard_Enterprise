//! SPEC-005 AC-009 — `events_lost` ETW buffer pressure via side-channel helper.
//!
//! Asserts the side-channel helper module's `events_lost(session_name)`
//! function per ADR-0008 §Decision part 2 returns a non-zero,
//! monotonically non-decreasing value under deliberately-induced ETW
//! buffer pressure, the principle of the Phase 0 spike (ADR-0008
//! §Empirical justification, docs/spikes/2026-05-23-etw-process-events.md):
//! pressure → loss → observable via `ControlTraceW(EVENT_TRACE_CONTROL_QUERY)`.
//!
//! The test owns a standalone ferrisetw session (independent of the
//! agent's) that receives every event of the Kernel-Process provider, with
//! no keyword filter: AC-009 measures the helper, not the agent's filter.
//! Pressure: the smallest buffer pool the API allows (4 KB × 2 buffers,
//! flush 1 s; the spike's 1 KB is below the Win32 minimum), an 80 ms
//! in-callback sleep (a test-only callback; the agent has no sleep), and
//! bursts of 200 short-lived processes, polling `events_lost` after each
//! burst until it is above 0 or 10 bursts / 90 s have passed. Then one
//! more burst and a second poll for monotonicity.
//!
//! Observed on 2026-10-04 (elevated gate on 3b78251), with the session
//! filtered to the process keyword `0x10`: `events_lost` stayed at 0 with
//! 216 callbacks — a slow consumer alone did not lose events at that
//! volume. Hence no keyword filter here. Whether this version passes is
//! known only from the elevated gate; on failure the message carries the
//! bursts, callbacks, every reading and `logman query` of the session.
//!
//! Windows-only, real ETW, elevated: in the elevated gate
//! (`cargo test -p cg-agent -- --ignored --test-threads=1`). It reclaims a
//! leftover session of its name first and stops its session before
//! asserting, so no session is left behind.

#![cfg(windows)]

use cg_agent::etw::{events_lost, stop_session};
use ferrisetw::provider::Provider;
use ferrisetw::schema_locator::SchemaLocator;
use ferrisetw::trace::{TraceProperties, TraceTrait, UserTrace};
use ferrisetw::EventRecord;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

const SESSION_NAME: &str = "CGAgent-AC009-LostTest";
const KERNEL_PROCESS_GUID: &str = "22fb2cd6-0e7b-422b-a0c7-2fad1fd0e716";
const CALLBACK_SLEEP_MS: u64 = 80;
const PROCESSES_PER_BURST: usize = 200;
const MAX_BURSTS: usize = 10;
const MAX_PRESSURE: Duration = Duration::from_secs(90);

#[ignore = "real ETW, elevated gate: cargo test -p cg-agent -- --ignored --test-threads=1"]
#[test]
fn ac_009_events_lost_under_deliberate_etw_buffer_pressure() {
    // A session of this name left by an earlier run would make the start
    // fail with ERROR_ALREADY_EXISTS.
    let _ = stop_session(SESSION_NAME);

    let callback_invocations = Arc::new(AtomicU32::new(0));
    let stopping = Arc::new(AtomicBool::new(false));
    let invocations_in_callback = Arc::clone(&callback_invocations);
    let stopping_in_callback = Arc::clone(&stopping);

    // Every event of the provider (no keyword filter). The callback sleeps
    // to induce dispatch backpressure, until the test starts to stop, so
    // the drain on the way out does not take minutes.
    let provider = Provider::by_guid(KERNEL_PROCESS_GUID)
        .add_callback(move |_record: &EventRecord, _schema: &SchemaLocator| {
            invocations_in_callback.fetch_add(1, Ordering::Relaxed);
            if !stopping_in_callback.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(CALLBACK_SLEEP_MS));
            }
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

    // Adaptive pressure: a burst, then a poll, until events_lost > 0 or
    // the budget runs out.
    let pressure_started = Instant::now();
    let mut bursts = 0usize;
    let mut readings: Vec<Result<u32, u32>> = Vec::new();
    let mut first_lost: Option<u32> = None;
    while bursts < MAX_BURSTS && pressure_started.elapsed() < MAX_PRESSURE {
        spawn_process_burst(PROCESSES_PER_BURST);
        bursts += 1;
        thread::sleep(Duration::from_millis(500));
        let reading = events_lost(SESSION_NAME);
        readings.push(reading);
        if let Ok(lost) = reading {
            if lost > 0 {
                first_lost = Some(lost);
                break;
            }
        }
    }

    // One more burst for the monotonicity check.
    let mut second_lost: Option<Result<u32, u32>> = None;
    if first_lost.is_some() {
        spawn_process_burst(PROCESSES_PER_BURST);
        bursts += 1;
        thread::sleep(Duration::from_secs(2));
        let reading = events_lost(SESSION_NAME);
        readings.push(reading);
        second_lost = Some(reading);
    }

    // The session as the OS reports it (buffer size and count, events and
    // buffers lost), before it is stopped.
    let logman = Command::new("logman")
        .args(["query", SESSION_NAME, "-ets"])
        .output()
        .map(|o| {
            format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            )
        })
        .unwrap_or_else(|e| format!("logman could not run: {e}"));

    // Stop the session (process_from_handle returns) before asserting, so
    // a failure leaves nothing behind.
    stopping.store(true, Ordering::Relaxed);
    let _ = stop_session(SESSION_NAME);
    let deadline = Instant::now() + Duration::from_secs(30);
    while !pump.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(50));
    }

    let diagnostics = format!(
        "bursts={bursts}; pressure_seconds={:.1}; callback_invocations={}; readings={readings:?}; \
         logman query {SESSION_NAME} -ets:\n{logman}",
        pressure_started.elapsed().as_secs_f64(),
        callback_invocations.load(Ordering::Relaxed),
    );

    assert!(
        readings.iter().all(Result::is_ok),
        "AC-009: events_lost helper MUST return Ok (not Err) under pressure; {diagnostics}"
    );
    let first_lost = first_lost.unwrap_or_else(|| {
        panic!(
            "AC-009: events_lost MUST be > 0 after deliberate pressure \
             (up to {MAX_BURSTS} bursts or {MAX_PRESSURE:?}); {diagnostics}"
        )
    });
    let second_lost = second_lost
        .expect("a second poll follows the first loss")
        .expect("checked Ok above");
    assert!(
        second_lost >= first_lost,
        "AC-009: events_lost MUST be monotonically non-decreasing under sustained pressure \
         (first_lost={first_lost}; second_lost={second_lost}); a decrease falsifies ADR-0008 \
         §Empirical justification; {diagnostics}"
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
