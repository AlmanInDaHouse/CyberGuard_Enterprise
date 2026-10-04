//! Dispatch-callback logic, independent of the ETW plumbing.
//!
//! The Windows session parses each Kernel-Process `EventRecord` into a
//! `RawProcessRecord` and hands it to `dispatch_record`, which does the
//! rest of the callback's work (SPEC-017 §Operational §6, amending
//! SPEC-005 NFR-005-001 by scope): it converts the timestamp, generates
//! the UUIDv7 `event_id` (ADR-0009 §1), inserts into the cache on Launch
//! or consults and purges it on Terminate (SPEC-005 §Operational §2),
//! and enqueues. No I/O and no lock beyond the cache's and the ring's;
//! the one exception is the `error` line of a dropped anomaly.
//!
//! Keeping this platform-independent lets the harness drive it with
//! synthetic records on every platform (capture_ac_007).

use uuid::Uuid;

use super::cache::CreatedTimeCache;
use super::ring::EventRing;
use super::types::{ActivityId, CapturedEvent};

/// The fields the dispatch callback reads from one Kernel-Process record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawProcessRecord {
    pub activity_id: ActivityId,
    pub pid: u32,
    pub parent_pid: u32,
    /// ETW `ImageName`, in kernel device form.
    pub image_file_name: String,
    pub command_line: String,
    pub subject_user_sid: String,
    /// `EVENT_HEADER.TimeStamp`: FILETIME, 100 ns intervals since 1601 UTC.
    pub filetime_100ns: i64,
    /// ETW `ExitCode`; `None` for Launch.
    pub exit_status: Option<i32>,
}

/// Turn one record into a `CapturedEvent` and enqueue it.
///
/// The Terminate's `created_time_nanos` is resolved here, at dispatch,
/// so a PID reused before the next batch cannot change it (SPEC-017
/// capture_ac_007). A pre-1970 timestamp is a capture-time anomaly: the
/// event is logged at `error` and dropped (SPEC-005 §Operational §1).
pub fn dispatch_record(raw: RawProcessRecord, cache: &CreatedTimeCache, ring: &EventRing) {
    let Some(etw_timestamp_nanos) = filetime_to_unix_nanos(raw.filetime_100ns) else {
        tracing::error!(
            pid = raw.pid,
            activity_id = raw.activity_id as u64,
            filetime_100ns = raw.filetime_100ns,
            reason = "timestamp_before_unix_epoch",
            "captured event with a pre-1970 timestamp dropped"
        );
        return;
    };

    let created_time_nanos = match raw.activity_id {
        ActivityId::Launch => {
            cache.insert(raw.pid, etw_timestamp_nanos);
            Some(etw_timestamp_nanos)
        }
        ActivityId::Terminate => cache.consult_and_purge(raw.pid),
    };

    ring.enqueue_or_drop(CapturedEvent {
        pid: raw.pid,
        event_id: Uuid::now_v7().to_string(),
        activity_id: raw.activity_id,
        image_file_name: raw.image_file_name,
        parent_pid: raw.parent_pid,
        command_line: raw.command_line,
        subject_user_sid: raw.subject_user_sid,
        etw_timestamp_nanos,
        created_time_nanos,
        exit_status: raw.exit_status,
    });
}

/// Convert ETW FILETIME (100-nanosecond intervals since 1601-01-01 UTC)
/// to Unix nanoseconds (since 1970-01-01 UTC); `None` before 1970.
///
/// 11_644_473_600 seconds between the 1601 and 1970 epochs × 10_000_000
/// (100-ns intervals per second) = 116444736000000000. Per SPEC-005
/// §Operational §1; no timezone or clock is consulted.
pub fn filetime_to_unix_nanos(filetime_100ns: i64) -> Option<u64> {
    const FILETIME_TO_UNIX_100NS: i64 = 116_444_736_000_000_000;
    let unix_100ns = filetime_100ns.checked_sub(FILETIME_TO_UNIX_100NS)?;
    if unix_100ns < 0 {
        return None;
    }
    Some((unix_100ns as u64).saturating_mul(100))
}
