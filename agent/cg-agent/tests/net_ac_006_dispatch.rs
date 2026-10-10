//! SPEC-019 net_ac_006 — dispatch.
//!
//! Synthetic network records through the dispatch logic, without ETW: the
//! event has a UUIDv7 `event_id` and the converted `time`; it carries the
//! ADR-0011 §6 uid when a Launch of its PID was dispatched before it, and
//! none otherwise; the cache entry is still there afterwards, so a later
//! Terminate of that PID finds its creation time; a record with the
//! excluded PID is not enqueued and does not change the dropped total; a
//! network event is enqueued although it has no image name, and a process
//! event with an empty image name is still dropped. Records the dispatch
//! cannot use are counted as discards (§Operational §1 and §2), and the
//! hygiene pass reports an increase.

use cg_agent::cges::{emit_process_activity, render_network_activity};
use cg_agent::etw::{
    dispatch_network_record, dispatch_record, filetime_to_unix_nanos, ActivityId, CapturedEvent,
    CreatedTimeCache, Direction, DiscardMonitor, EventRing, NetworkDiscards, RawNetworkRecord,
    RawProcessRecord, RingEvent,
};

const AGENT_ID: &str = "01934abc-def0-7000-89ab-000000000006";

/// 2026-10-10T00:00:00Z as FILETIME (100 ns ticks since 1601).
const FILETIME_BASE: i64 = 116_444_736_000_000_000 + 17_915_904_000_000_000;

fn launch(pid: u32, filetime: i64) -> RawProcessRecord {
    RawProcessRecord {
        activity_id: ActivityId::Launch,
        pid,
        parent_pid: 4,
        image_file_name: format!("\\Device\\HarddiskVolume3\\probe\\p{pid}.exe"),
        command_line: String::new(),
        subject_user_sid: String::new(),
        filetime_100ns: filetime,
        exit_status: None,
    }
}

fn terminate(pid: u32, filetime: i64) -> RawProcessRecord {
    RawProcessRecord {
        activity_id: ActivityId::Terminate,
        exit_status: Some(0),
        ..launch(pid, filetime)
    }
}

/// An outbound IPv4 connection of `pid`, from 192.0.2.10:49213 to
/// 198.51.100.7:443, encoded as the decoding expects (`network.rs`).
fn connect(pid: u32, filetime: i64) -> RawNetworkRecord {
    RawNetworkRecord {
        event_id: 12,
        pid: pid.to_le_bytes().to_vec(),
        saddr: vec![192, 0, 2, 10],
        daddr: vec![198, 51, 100, 7],
        sport: 49213u16.to_be_bytes().to_vec(),
        dport: 443u16.to_be_bytes().to_vec(),
        filetime_100ns: filetime,
    }
}

fn drained(ring: &EventRing) -> Vec<RingEvent> {
    ring.drain_events()
}

#[test]
fn net_ac_006_event_id_time_and_uid_from_a_dispatched_launch() {
    let ring = EventRing::new(16);
    let cache = CreatedTimeCache::new();
    let discards = NetworkDiscards::new();

    dispatch_record(launch(4321, FILETIME_BASE), &cache, &ring);
    dispatch_network_record(
        connect(4321, FILETIME_BASE + 10),
        None,
        &cache,
        &ring,
        &discards,
    );
    dispatch_network_record(
        connect(999, FILETIME_BASE + 20),
        None,
        &cache,
        &ring,
        &discards,
    );

    let events = drained(&ring);
    assert_eq!(events.len(), 3);
    let launch_event = events[0].as_process().expect("the Launch");
    let with_uid = events[1].as_network().expect("the first connection");
    let without_uid = events[2].as_network().expect("the second connection");

    let id = uuid::Uuid::parse_str(&with_uid.event_id).expect("event_id is a UUID");
    assert_eq!(id.get_version_num(), 7);
    assert_ne!(with_uid.event_id, without_uid.event_id);
    assert_eq!(
        with_uid.etw_timestamp_nanos,
        filetime_to_unix_nanos(FILETIME_BASE + 10).unwrap()
    );
    assert_eq!(with_uid.direction, Direction::Outbound);

    // The uid is the one of the process's Launch (ADR-0011 §6).
    let rendered = render_network_activity(with_uid, AGENT_ID);
    let launch_rendered = emit_process_activity(launch_event, AGENT_ID);
    assert_eq!(
        rendered.actor.process.uid.as_deref(),
        Some(launch_rendered.process.uid.as_str())
    );
    assert_eq!(rendered.time, with_uid.etw_timestamp_nanos.to_string());

    // No Launch of PID 999 was dispatched: no uid, and no `uid` member.
    assert_eq!(without_uid.created_time_nanos, None);
    let json = serde_json::to_value(render_network_activity(without_uid, AGENT_ID)).unwrap();
    assert_eq!(json.pointer("/actor/process/pid").unwrap(), 999);
    assert!(json.pointer("/actor/process/uid").is_none());
}

#[test]
fn net_ac_006_the_lookup_leaves_the_entry_for_the_terminate() {
    let ring = EventRing::new(16);
    let cache = CreatedTimeCache::new();
    let discards = NetworkDiscards::new();

    dispatch_record(launch(4321, FILETIME_BASE), &cache, &ring);
    dispatch_network_record(
        connect(4321, FILETIME_BASE + 10),
        None,
        &cache,
        &ring,
        &discards,
    );
    assert_eq!(cache.len(), 1, "the network lookup does not purge");
    dispatch_record(terminate(4321, FILETIME_BASE + 50), &cache, &ring);

    let events = drained(&ring);
    let terminate_event = events[2].as_process().expect("the Terminate");
    assert_eq!(
        terminate_event.created_time_nanos,
        Some(filetime_to_unix_nanos(FILETIME_BASE).unwrap()),
        "the Terminate still finds the Launch's creation time"
    );
    assert!(cache.is_empty(), "the Terminate purges as before");
}

#[test]
fn net_ac_006_the_excluded_pid_is_not_enqueued_nor_counted() {
    let ring = EventRing::new(16);
    let cache = CreatedTimeCache::new();
    let discards = NetworkDiscards::new();

    dispatch_network_record(
        connect(777, FILETIME_BASE),
        Some(777),
        &cache,
        &ring,
        &discards,
    );
    assert!(ring.is_empty());
    assert_eq!(ring.events_dropped_total(), 0);
    assert_eq!(discards.total(), 0);

    // Another PID with the same exclusion is reported.
    dispatch_network_record(
        connect(778, FILETIME_BASE),
        Some(777),
        &cache,
        &ring,
        &discards,
    );
    assert_eq!(ring.len(), 1);
}

#[test]
fn net_ac_006_the_empty_name_rule_applies_to_process_events_only() {
    let ring = EventRing::new(16);
    let cache = CreatedTimeCache::new();
    let discards = NetworkDiscards::new();

    // A network event has no image name and is enqueued.
    dispatch_network_record(connect(4321, FILETIME_BASE), None, &cache, &ring, &discards);
    assert_eq!(ring.len(), 1);

    // A process event with an empty image name is still dropped (SPEC-005 AC-006).
    ring.enqueue_or_drop(CapturedEvent {
        pid: 4321,
        event_id: uuid::Uuid::now_v7().to_string(),
        activity_id: ActivityId::Launch,
        image_file_name: String::new(),
        parent_pid: 4,
        command_line: String::new(),
        subject_user_sid: String::new(),
        etw_timestamp_nanos: 1,
        created_time_nanos: Some(1),
        exit_status: None,
    });
    assert_eq!(ring.len(), 1);
}

#[test]
fn net_ac_006_unusable_records_are_counted_as_discards() {
    let ring = EventRing::new(16);
    let cache = CreatedTimeCache::new();
    let discards = NetworkDiscards::new();

    // Another event id: data sent.
    let mut sent = connect(4321, FILETIME_BASE);
    sent.event_id = 10;
    dispatch_network_record(sent, None, &cache, &ring, &discards);
    // A field that could not be read.
    let mut unreadable = connect(4321, FILETIME_BASE);
    unreadable.sport = Vec::new();
    dispatch_network_record(unreadable, None, &cache, &ring, &discards);

    assert!(ring.is_empty());
    assert_eq!(ring.events_dropped_total(), 0, "a discard is not a drop");
    assert_eq!(discards.total(), 2);
    let first = discards.first().expect("the first discard is kept");
    assert_eq!(first.event_id, 10);
    assert_eq!(first.field_lengths, [4, 4, 4, 2, 2]);

    // A pre-1970 timestamp is dropped and logged, not counted as a discard.
    dispatch_network_record(connect(4321, 0), None, &cache, &ring, &discards);
    assert!(ring.is_empty());
    assert_eq!(discards.total(), 2);
}

#[test]
fn net_ac_006_the_hygiene_pass_reports_an_increase() {
    let discards = NetworkDiscards::new();
    let mut monitor = DiscardMonitor::new();
    assert_eq!(monitor.observe(&discards, "test"), 0);
    discards.record(10, [0; 5]);
    discards.record(11, [0; 5]);
    assert_eq!(monitor.observe(&discards, "test"), 2);
    assert_eq!(
        monitor.observe(&discards, "test"),
        0,
        "no increase, no report"
    );
}
