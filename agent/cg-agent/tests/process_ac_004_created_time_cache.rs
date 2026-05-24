//! SPEC-005 AC-004 — `process.created_time` integer-nanos UTC + cache
//! hit/miss for Terminate retention.
//!
//! Two tests with asymmetric setup:
//!
//! 1. Cache-hit nominal lifecycle: real ETW capture on the Windows test
//!    runner (no testcontainers; Kernel-Process is a Windows kernel
//!    facility), real probe process spawn + terminate, the existing
//!    common::MockServer captures the agent's POSTed outer signed
//!    envelopes for inspection. Asserts the Terminate event's
//!    `process.created_time` matches the Launch event's
//!    `process.created_time` byte-for-byte (cache populated at Launch,
//!    consulted at Terminate per §Operational §2).
//!
//! 2. Cache-miss synthetic injection: synthetic Terminate event with
//!    the cache empty for the Terminated PID. Asserts the emitted
//!    Terminate event has `process.created_time = null` (cache miss
//!    path). No real ETW, no probe process. Pattern matches AC-006's
//!    defensive-contract synthetic-injection approach.
//!
//! Reuses common::MockServer (existing SPEC-003 helper) for the
//! cache-hit test's envelope capture; introduces common::TestAgentHandle
//! and common::start_test_agent (new scaffolding) for the in-process
//! agent control.

mod common;

use cg_agent::cges::emit_process_activity_with_cache;
use cg_agent::etw::{ActivityId, CapturedEvent, CreatedTimeCache, EtwSession};
use serde_json::Value;
use std::process::Command;
use std::time::Duration;

/// Cache-hit nominal lifecycle. Real ETW capture, real probe, existing
/// common::MockServer captures envelope POSTs. Requires Windows + ETW
/// Kernel-Process open privilege.
#[cfg_attr(not(target_os = "windows"), ignore)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ac_004_cache_hit_terminate_matches_launch_byte_for_byte() {
    let mock = common::MockServer::start().await;
    let agent = common::start_test_agent(&mock.base_url, common::TEST_AGENT_ID).await;

    // Spawn a probe that exits cleanly after a brief delay.
    let probe = Command::new("cmd.exe")
        .args(["/c", "ping -n 2 127.0.0.1 >NUL & exit 0"])
        .spawn()
        .expect("AC-004: probe spawn must succeed on Windows test runner");
    let probe_pid = probe.id();

    // Wait for both Launch + Terminate to flow through the agent.
    tokio::time::sleep(Duration::from_secs(8)).await;

    let envelopes = mock.received();
    let events: Vec<&Value> = envelopes
        .iter()
        .flat_map(|env| {
            env.pointer("/events")
                .and_then(Value::as_array)
                .map(|a| a.iter())
                .into_iter()
                .flatten()
        })
        .filter(|e| e.pointer("/process/pid").and_then(Value::as_u64) == Some(probe_pid as u64))
        .collect();

    let launch = events
        .iter()
        .find(|e| e.pointer("/activity_id").and_then(Value::as_u64) == Some(1))
        .expect("AC-004: Launch event for probe MUST be captured + posted");
    let terminate = events
        .iter()
        .find(|e| e.pointer("/activity_id").and_then(Value::as_u64) == Some(2))
        .expect("AC-004: Terminate event for probe MUST be captured + posted");

    let launch_time = launch
        .pointer("/process/created_time")
        .and_then(Value::as_u64)
        .expect("AC-004: Launch MUST emit process.created_time as integer nanos");
    let terminate_time = terminate
        .pointer("/process/created_time")
        .and_then(Value::as_u64)
        .expect("AC-004: Terminate MUST emit process.created_time (cache hit) per §Operational §2");

    assert_eq!(
        launch_time, terminate_time,
        "AC-004: Terminate process.created_time MUST equal Launch's byte-for-byte (cache hit)"
    );

    agent.shutdown().await;
}

/// Cache-miss synthetic injection. No real ETW; synthetic Terminate event
/// with no prior Launch in the cache. Asserts created_time = null.
#[test]
fn ac_004_cache_miss_terminate_emits_null_created_time() {
    let cache = CreatedTimeCache::new();
    // Cache is empty. Synthetic Terminate event for an unknown PID.
    let terminate = CapturedEvent {
        pid: 99999,
        activity_id: ActivityId::Terminate,
        image_file_name: String::from("\\Device\\HarddiskVolume2\\Windows\\System32\\cmd.exe"),
        parent_pid: 4,
        command_line: String::from("cmd.exe /c exit"),
        subject_user_sid: String::from("S-1-5-18"),
        etw_timestamp_nanos: 1716123612901000000,
        exit_status: Some(0),
    };

    let cached_created_time = cache.consult_and_purge(terminate.pid);
    assert!(
        cached_created_time.is_none(),
        "AC-004: cache-miss MUST return None for unknown PID"
    );

    let event = emit_process_activity_with_cache(&terminate, cached_created_time);
    let json: Value =
        serde_json::to_value(event).expect("AC-004: emitted event MUST be JSON-serialisable");
    let created_time_field = json.pointer("/process/created_time");
    assert!(
        matches!(created_time_field, Some(Value::Null)),
        "AC-004: cache-miss Terminate MUST emit process.created_time = null (not absent, not zero)"
    );
}

// Placeholder reference to ensure EtwSession is exercised at compile time.
// Phase 3.5 implementation must export this for AC-001 marquee + AC-009.
#[allow(dead_code)]
fn _ensure_etw_session_import() -> Option<EtwSession> {
    None
}
