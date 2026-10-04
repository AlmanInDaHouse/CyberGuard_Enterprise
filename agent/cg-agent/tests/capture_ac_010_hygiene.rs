//! SPEC-017 capture_ac_010 — capture hygiene.
//!
//! `event_id` is a UUIDv7 generated at dispatch; a ring overflow produces
//! one `warn` with the dropped total, at most once per 60 s; the cache
//! sweep evicts the entries of processes that are gone (SPEC-005
//! NFR-005-006); an `events_lost` increase produces its `warn`, a
//! decrease an `error`.

use cg_agent::etw::{
    dispatch_record, ActivityId, CreatedTimeCache, EventRing, EventsLostMonitor, LostObservation,
    OverflowWarning, RawProcessRecord, HYGIENE_INTERVAL,
};
use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::subscriber::with_default;
use tracing_subscriber::fmt::MakeWriter;

#[derive(Clone)]
struct BufferWriter {
    buf: Arc<Mutex<Vec<u8>>>,
}

impl io::Write for BufferWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buf.lock().unwrap().extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for BufferWriter {
    type Writer = BufferWriter;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Run `f` with a JSON subscriber at `WARN` capturing into a buffer;
/// return the captured log lines.
fn capture_warn_logs(f: impl FnOnce()) -> Vec<String> {
    let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
    let subscriber = tracing_subscriber::fmt()
        .with_writer(BufferWriter {
            buf: Arc::clone(&buf),
        })
        .with_max_level(tracing::Level::WARN)
        .json()
        .finish();
    with_default(subscriber, f);
    let bytes = buf.lock().unwrap().clone();
    String::from_utf8_lossy(&bytes)
        .lines()
        .map(str::to_string)
        .collect()
}

fn launch(pid: u32) -> RawProcessRecord {
    RawProcessRecord {
        activity_id: ActivityId::Launch,
        pid,
        parent_pid: 4,
        image_file_name: format!("\\Device\\HarddiskVolume3\\probe_{pid}.exe"),
        command_line: String::new(),
        subject_user_sid: String::new(),
        // 2026-10-04T00:00:00Z as FILETIME.
        filetime_100ns: 116_444_736_000_000_000 + 17_910_720_000_000_000,
        exit_status: None,
    }
}

#[test]
fn capture_ac_010_event_id_is_a_uuidv7() {
    let ring = EventRing::new(16);
    let cache = CreatedTimeCache::new();
    dispatch_record(launch(10), &cache, &ring);
    dispatch_record(launch(11), &cache, &ring);

    let events = ring.drain_events();
    for event in &events {
        let id = uuid::Uuid::parse_str(&event.event_id).expect("event_id is a UUID");
        assert_eq!(
            id.get_version_num(),
            7,
            "event_id {} is not v7",
            event.event_id
        );
    }
    assert_ne!(events[0].event_id, events[1].event_id);
}

#[test]
fn capture_ac_010_ring_overflow_warns_with_a_60_s_throttle() {
    let ring = EventRing::new_for_test(2);
    let cache = CreatedTimeCache::new();
    let mut warning = OverflowWarning::new();
    let t0 = Instant::now();

    let lines = capture_warn_logs(|| {
        // Nothing dropped yet: no warning.
        assert!(!warning.check(&ring, t0));

        for pid in 0..3 {
            dispatch_record(launch(100 + pid), &cache, &ring);
        }
        assert_eq!(ring.events_dropped_total(), 1);
        assert!(warning.check(&ring, t0), "the first drop warns");

        dispatch_record(launch(200), &cache, &ring);
        assert_eq!(ring.events_dropped_total(), 2);
        assert!(
            !warning.check(&ring, t0 + Duration::from_secs(10)),
            "a second drop within 60 s does not warn"
        );
        assert!(
            warning.check(&ring, t0 + Duration::from_secs(61)),
            "after 60 s the new drops warn"
        );
        assert!(
            !warning.check(&ring, t0 + Duration::from_secs(200)),
            "no new drop, no warning"
        );
    });

    assert_eq!(lines.len(), 2, "exactly two warnings: {lines:?}");
    assert!(
        lines[0].contains("\"events_dropped_total\":1"),
        "{}",
        lines[0]
    );
    assert!(
        lines[1].contains("\"events_dropped_total\":2"),
        "{}",
        lines[1]
    );
    assert!(lines.iter().all(|l| l.contains("\"level\":\"WARN\"")));
}

#[test]
fn capture_ac_010_hygiene_runs_every_60_s() {
    assert_eq!(HYGIENE_INTERVAL, Duration::from_secs(60));
}

#[test]
fn capture_ac_010_sweep_evicts_entries_of_processes_that_are_gone() {
    let cache = CreatedTimeCache::new();
    cache.insert(1, 100);
    cache.insert(2, 200);
    cache.insert(3, 300);

    let evicted = cache.sweep(|pid| pid != 2);

    assert_eq!(evicted, 1);
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.consult_and_purge(2), None, "pid 2 was evicted");
    assert_eq!(cache.consult_and_purge(1), Some(100));
}

#[test]
fn capture_ac_010_sweep_keeps_an_entry_replaced_during_the_sweep() {
    let cache = CreatedTimeCache::new();
    cache.insert(5, 500);

    // The process is gone, but a new Launch reuses its PID meanwhile.
    let evicted = cache.sweep(|pid| {
        cache.insert(pid, 900);
        false
    });

    assert_eq!(evicted, 0);
    assert_eq!(cache.consult_and_purge(5), Some(900));
}

#[test]
fn capture_ac_010_events_lost_increase_warns_and_decrease_errors() {
    let mut monitor = EventsLostMonitor::new();
    let mut observations = Vec::new();
    let lines = capture_warn_logs(|| {
        observations.push(monitor.observe(0, "CGAgent-KernelProcess"));
        observations.push(monitor.observe(7, "CGAgent-KernelProcess"));
        observations.push(monitor.observe(7, "CGAgent-KernelProcess"));
        observations.push(monitor.observe(3, "CGAgent-KernelProcess"));
    });

    assert_eq!(
        observations,
        vec![
            LostObservation::Unchanged,
            LostObservation::Increased { total: 7, delta: 7 },
            LostObservation::Unchanged,
            LostObservation::Decreased {
                current: 3,
                previous: 7
            },
        ]
    );
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[0].contains("\"level\":\"WARN\""), "{}", lines[0]);
    assert!(lines[0].contains("\"events_lost\":7"), "{}", lines[0]);
    assert!(lines[1].contains("\"level\":\"ERROR\""), "{}", lines[1]);
}

#[cfg(windows)]
#[test]
fn capture_ac_010_process_liveness_probe() {
    use cg_agent::etw::process_is_alive;

    assert!(
        process_is_alive(std::process::id()),
        "this process is alive"
    );
    // PID 4 is the System process: access is denied, yet it exists.
    assert!(process_is_alive(4));
    // No process has this PID (PIDs are multiples of 4, far below this).
    assert!(!process_is_alive(0xFFFF_FFF0));
}
