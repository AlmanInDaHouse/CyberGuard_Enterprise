//! SPEC-005 AC-004 — `process.created_time` integer-nanos UTC + cache
//! hit/miss for Terminate retention.
//!
//! Two tests with asymmetric setup:
//!
//! 1. Cache-hit nominal lifecycle: the agent's secure path with real ETW
//!    capture (Windows, elevated; in the elevated gate), a real probe
//!    process spawn + terminate, and the TLS mock capturing the agent's
//!    signed envelopes. Asserts the Terminate event's
//!    `process.created_time` matches the Launch event's byte-for-byte
//!    (cache populated at Launch, consulted at Terminate per §Operational
//!    §2).
//!
//! 2. Cache-miss synthetic injection: synthetic Terminate event with
//!    the cache empty for the Terminated PID. Asserts the emitted
//!    Terminate event has `process.created_time = null` (cache miss
//!    path). No real ETW, no probe process. Pattern matches AC-006's
//!    defensive-contract synthetic-injection approach.

mod common;

use cg_agent::cges::emit_process_activity_with_cache;
use cg_agent::etw::{ActivityId, CapturedEvent, CreatedTimeCache};
use serde_json::Value;

/// Cache-hit nominal lifecycle. Real ETW capture on the normal run path,
/// real probe, TLS mock. Requires Windows + elevation: run with
/// `cargo test -p cg-agent -- --ignored --test-threads=1`.
#[cfg(windows)]
#[ignore = "real ETW, elevated gate: cargo test -p cg-agent -- --ignored --test-threads=1"]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ac_004_cache_hit_terminate_matches_launch_byte_for_byte() {
    use std::process::Command;
    use std::time::Duration;

    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let mock = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let agent = common::start_secure_agent(&pki, &mock.base_url, 1, cg_agent::Capture::Platform);
    // The first heartbeat goes out once the ETW session is open.
    assert!(
        common::wait_until(Duration::from_secs(10), || mock.received_count() >= 1).await,
        "AC-004: the agent must open its ETW session and heartbeat (elevated?)"
    );

    // Spawn a probe that exits cleanly after a brief delay.
    let probe = Command::new("cmd.exe")
        .args(["/c", "ping -n 2 127.0.0.1 >NUL & exit 0"])
        .spawn()
        .expect("AC-004: probe spawn must succeed on Windows test runner");
    let probe_pid = u64::from(probe.id());

    let probe_events = |mock: &common::TlsMockServer| -> Vec<Value> {
        mock.received()
            .iter()
            .flat_map(common::envelope_events)
            .filter(|e| e.pointer("/process/pid").and_then(Value::as_u64) == Some(probe_pid))
            .collect()
    };
    // Wait for both Launch + Terminate to flow through the agent.
    common::wait_until(Duration::from_secs(15), || probe_events(&mock).len() >= 2).await;
    agent.stop().await.expect("AC-004: clean stop");

    let events = probe_events(&mock);
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
        .and_then(Value::as_str)
        .expect("AC-004: Launch MUST emit process.created_time as string-encoded nanos");
    let terminate_time = terminate
        .pointer("/process/created_time")
        .and_then(Value::as_str)
        .expect("AC-004: Terminate MUST emit process.created_time (cache hit) per §Operational §2");

    assert_eq!(
        launch_time, terminate_time,
        "AC-004: Terminate process.created_time MUST equal Launch's byte-for-byte (cache hit)"
    );
}

/// Cache-miss synthetic injection. No real ETW; synthetic Terminate event
/// with no prior Launch in the cache. Asserts created_time = null.
#[test]
fn ac_004_cache_miss_terminate_emits_null_created_time() {
    let cache = CreatedTimeCache::new();
    // Cache is empty. Synthetic Terminate event for an unknown PID.
    let terminate = CapturedEvent {
        pid: 99999,
        event_id: "synthetic-ac004-test".to_string(),
        activity_id: ActivityId::Terminate,
        image_file_name: String::from("\\Device\\HarddiskVolume2\\Windows\\System32\\cmd.exe"),
        parent_pid: 4,
        command_line: String::from("cmd.exe /c exit"),
        subject_user_sid: String::from("S-1-5-18"),
        etw_timestamp_nanos: 1716123612901000000,
        created_time_nanos: None,
        exit_status: Some(0),
    };

    let cached_created_time = cache.consult_and_purge(terminate.pid);
    assert!(
        cached_created_time.is_none(),
        "AC-004: cache-miss MUST return None for unknown PID"
    );

    let event = emit_process_activity_with_cache(&terminate, cached_created_time, "test-agent-id");
    let json: Value =
        serde_json::to_value(event).expect("AC-004: emitted event MUST be JSON-serialisable");
    let created_time_field = json.pointer("/process/created_time");
    assert!(
        matches!(created_time_field, Some(Value::Null)),
        "AC-004: cache-miss Terminate MUST emit process.created_time = null (not absent, not zero)"
    );
}
