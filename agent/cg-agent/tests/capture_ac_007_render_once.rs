//! SPEC-017 capture_ac_007 — render once.
//!
//! The Terminate's `created_time` is resolved at dispatch (the cache is
//! consulted and purged there, SPEC-017 §Operational §6), so a PID reused
//! before the next batch does not change it; a rendered event is
//! byte-identical every time it is rendered (SPEC-017 §Data contracts).
//! Drives the platform-independent dispatch logic with synthetic records.

use cg_agent::cges::emit_process_activity;
use cg_agent::etw::{dispatch_record, ActivityId, CreatedTimeCache, EventRing, RawProcessRecord};

const AGENT_ID: &str = "01934abc-def0-7000-89ab-000000000099";

/// FILETIME of 1970-01-01T00:00:00Z, in 100 ns intervals since 1601.
const FILETIME_UNIX_EPOCH: i64 = 116_444_736_000_000_000;

/// 2026-10-04T00:00:00Z in Unix nanoseconds (a multiple of 100).
const LAUNCH_NS: u64 = 1_791_072_000_000_000_000;

fn filetime(unix_nanos: u64) -> i64 {
    FILETIME_UNIX_EPOCH + (unix_nanos / 100) as i64
}

fn record(
    activity_id: ActivityId,
    pid: u32,
    unix_nanos: u64,
    exit_status: Option<i32>,
) -> RawProcessRecord {
    RawProcessRecord {
        activity_id,
        pid,
        parent_pid: 4,
        image_file_name: String::from("\\Device\\HarddiskVolume3\\Windows\\System32\\cmd.exe"),
        command_line: String::new(),
        subject_user_sid: String::new(),
        filetime_100ns: filetime(unix_nanos),
        exit_status,
    }
}

#[test]
fn capture_ac_007_terminate_keeps_the_created_time_resolved_at_dispatch() {
    let ring = EventRing::new(16);
    let cache = CreatedTimeCache::new();
    let terminate_ns = LAUNCH_NS + 1_000_000_000;
    let reused_ns = LAUNCH_NS + 2_000_000_000;

    dispatch_record(
        record(ActivityId::Launch, 4242, LAUNCH_NS, None),
        &cache,
        &ring,
    );
    dispatch_record(
        record(ActivityId::Terminate, 4242, terminate_ns, Some(0)),
        &cache,
        &ring,
    );
    // The PID is reused before the batch is formed.
    dispatch_record(
        record(ActivityId::Launch, 4242, reused_ns, None),
        &cache,
        &ring,
    );

    let events = ring.drain_events();
    assert_eq!(events.len(), 3);
    let rendered: Vec<_> = events
        .iter()
        .map(|e| emit_process_activity(e, AGENT_ID))
        .collect();

    assert_eq!(
        rendered[1].process.created_time,
        Some(LAUNCH_NS.to_string()),
        "the Terminate keeps the Launch's created_time, not the reused PID's"
    );
    assert_eq!(
        rendered[1].process.uid, rendered[0].process.uid,
        "Launch and Terminate of one process share process.uid"
    );
    assert_eq!(rendered[1].time, terminate_ns.to_string());
    assert_eq!(
        rendered[2].process.created_time,
        Some(reused_ns.to_string())
    );
    assert_ne!(rendered[2].process.uid, rendered[0].process.uid);
    assert_eq!(cache.len(), 1, "only the reused process remains cached");
}

#[test]
fn capture_ac_007_terminate_without_a_launch_is_a_cache_miss() {
    let ring = EventRing::new(16);
    let cache = CreatedTimeCache::new();

    dispatch_record(
        record(ActivityId::Terminate, 777, LAUNCH_NS, Some(1)),
        &cache,
        &ring,
    );

    let events = ring.drain_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].created_time_nanos, None);
    let json = serde_json::to_value(emit_process_activity(&events[0], AGENT_ID)).unwrap();
    assert!(json.pointer("/process/created_time").unwrap().is_null());
}

#[test]
fn capture_ac_007_rendering_is_byte_identical() {
    let ring = EventRing::new(16);
    let cache = CreatedTimeCache::new();
    dispatch_record(
        record(ActivityId::Launch, 4242, LAUNCH_NS, None),
        &cache,
        &ring,
    );
    let event = ring.drain_events().remove(0);

    let first = serde_json::to_vec(&emit_process_activity(&event, AGENT_ID)).unwrap();
    let second = serde_json::to_vec(&emit_process_activity(&event, AGENT_ID)).unwrap();
    assert_eq!(first, second);
}

#[test]
fn capture_ac_007_pre_1970_timestamp_is_dropped() {
    let ring = EventRing::new(16);
    let cache = CreatedTimeCache::new();
    let mut raw = record(ActivityId::Launch, 4242, 0, None);
    raw.filetime_100ns = FILETIME_UNIX_EPOCH - 1;

    dispatch_record(raw, &cache, &ring);

    assert!(
        ring.is_empty(),
        "SPEC-005 §Operational §1: logged and dropped"
    );
    assert!(cache.is_empty());
}
