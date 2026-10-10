//! Dispatch-callback logic, independent of the ETW plumbing.
//!
//! Network records (SPEC-019) go through `dispatch_network_record`, which
//! decodes them (`network.rs`), drops the agent's own connections, looks
//! the process's creation time up and enqueues a `NetworkEvent`.
//!
//! The Windows session parses each Kernel-Process `EventRecord` into a
//! `RawProcessRecord` and hands it to `dispatch_record`, which does the
//! rest of the callback's work (SPEC-017 §Operational §6, amending
//! SPEC-005 NFR-005-001 by scope): it converts the timestamp, generates
//! the UUIDv7 `event_id` (ADR-0009 §1), inserts into the cache on Launch
//! or consults and purges it on Terminate (SPEC-005 §Operational §2),
//! and enqueues. No I/O and no lock of its own beyond the cache's and the
//! ring's (the locks inside ferrisetw and the uuid crate are taken on both
//! paths alike, SPEC-019 Amendment 2026-10-10); the one exception is the
//! `error` line of a dropped anomaly.
//!
//! Keeping this platform-independent lets the harness drive it with
//! synthetic records on every platform (capture_ac_007).

use std::sync::atomic::{AtomicU64, Ordering};

use uuid::Uuid;

use super::cache::CreatedTimeCache;
use super::network::{connection_direction, decode_connection, decode_pid};
use super::ring::EventRing;
use super::types::{ActivityId, CapturedEvent, NetworkEvent};

/// The fields the dispatch callback reads from one Kernel-Process record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawProcessRecord {
    pub activity_id: ActivityId,
    pub pid: u32,
    pub parent_pid: u32,
    /// ETW `ImageName`: on Launch (ProcessStart) the kernel device path
    /// (`\Device\HarddiskVolumeN\...`); on Terminate (ProcessStop) only
    /// the image's base name.
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

/// The raw properties the dispatch callback reads from one Kernel-Network
/// record (SPEC-019 §Operational §2): the bytes of each property as the
/// payload carries them, empty when the property could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawNetworkRecord {
    pub event_id: u16,
    pub pid: Vec<u8>,
    pub saddr: Vec<u8>,
    pub daddr: Vec<u8>,
    pub sport: Vec<u8>,
    pub dport: Vec<u8>,
    /// `EVENT_HEADER.TimeStamp`: FILETIME, 100 ns intervals since 1601 UTC.
    pub filetime_100ns: i64,
}

/// The first discarded network record, kept to say why records are
/// discarded: its event id and the byte length of each property (`PID`,
/// `saddr`, `daddr`, `sport`, `dport`; 0 when not read, capped at 255).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscardSample {
    pub event_id: u16,
    pub field_lengths: [u8; 5],
}

/// The session's count of discarded network records (SPEC-019
/// §Operational §1 and §2): records of another event id, and records
/// whose fields cannot be decoded. Lock-free: two atomics, the second
/// holding the first discard, packed.
#[derive(Debug, Default)]
pub struct NetworkDiscards {
    total: AtomicU64,
    first: AtomicU64,
}

impl NetworkDiscards {
    const PRESENT: u64 = 1 << 63;

    pub fn new() -> Self {
        Self::default()
    }

    /// Records discarded so far.
    pub fn total(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }

    /// The first record discarded, if any.
    pub fn first(&self) -> Option<DiscardSample> {
        let packed = self.first.load(Ordering::Relaxed);
        if packed & Self::PRESENT == 0 {
            return None;
        }
        let mut field_lengths = [0u8; 5];
        for (i, len) in field_lengths.iter_mut().enumerate() {
            *len = (packed >> (8 * i)) as u8;
        }
        Some(DiscardSample {
            event_id: (packed >> 40) as u16,
            field_lengths,
        })
    }

    /// Count one discarded record. The first is also kept.
    pub fn record(&self, event_id: u16, field_lengths: [usize; 5]) {
        self.total.fetch_add(1, Ordering::Relaxed);
        let mut packed = Self::PRESENT | (u64::from(event_id) << 40);
        for (i, len) in field_lengths.iter().enumerate() {
            packed |= ((*len).min(255) as u64) << (8 * i);
        }
        let _ = self
            .first
            .compare_exchange(0, packed, Ordering::Relaxed, Ordering::Relaxed);
    }
}

/// Turn one Kernel-Network record into a `NetworkEvent` and enqueue it
/// (SPEC-019 §Operational §1–§4).
///
/// - A record of an event id other than 12, 15, 28 or 31, or whose fields
///   cannot be decoded, is dropped and counted in `discards`.
/// - A connection record whose `PID` is `excluded_pid` (the agent's own)
///   is dropped before its other fields are decoded, counted nowhere and
///   not logged (ADR-0018 §8; SPEC-019 §Operational §4 as amended). A
///   record of another event id is a discard whatever its `PID`.
/// - A pre-1970 timestamp is logged at `error` and dropped, as for a
///   process record.
/// - The creation time of the record's process is looked up in the cache
///   now, without removing it, so a PID reused before the batch is formed
///   cannot change the uid (§Operational §3).
///
/// No I/O and no lock of its own beyond the cache's and the ring's; the
/// one exception is the `error` line of a dropped anomaly.
pub fn dispatch_network_record(
    raw: RawNetworkRecord,
    excluded_pid: Option<u32>,
    cache: &CreatedTimeCache,
    ring: &EventRing,
    discards: &NetworkDiscards,
) {
    let lengths = [
        raw.pid.len(),
        raw.saddr.len(),
        raw.daddr.len(),
        raw.sport.len(),
        raw.dport.len(),
    ];
    if connection_direction(raw.event_id).is_none() {
        discards.record(raw.event_id, lengths);
        return;
    }
    if excluded_pid.is_some() && decode_pid(&raw.pid) == excluded_pid {
        return;
    }
    let Some(connection) = decode_connection(
        raw.event_id,
        &raw.pid,
        &raw.saddr,
        &raw.daddr,
        &raw.sport,
        &raw.dport,
    ) else {
        discards.record(raw.event_id, lengths);
        return;
    };
    let Some(etw_timestamp_nanos) = filetime_to_unix_nanos(raw.filetime_100ns) else {
        tracing::error!(
            pid = connection.pid,
            event_id = raw.event_id,
            filetime_100ns = raw.filetime_100ns,
            reason = "timestamp_before_unix_epoch",
            "captured network event with a pre-1970 timestamp dropped"
        );
        return;
    };

    ring.enqueue_or_drop(NetworkEvent {
        event_id: Uuid::now_v7().to_string(),
        pid: connection.pid,
        direction: connection.direction,
        src: connection.src,
        dst: connection.dst,
        etw_timestamp_nanos,
        created_time_nanos: cache.created_time(connection.pid),
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
