//! In-memory captured-event shape + activity discriminants + raw ETW
//! open-error variants.
//!
//! `CapturedEvent` is the post-capture pre-emission representation of a
//! Kernel-Process Launch or Terminate event. It is produced by the ETW
//! dispatch callback (β3 session.rs on Windows; session_stub.rs returns
//! Err on non-Windows), traverses the bounded ring buffer (β1 ring.rs),
//! and is rendered to CGES JSON via the emit functions (β2 cges/emit.rs).
//! `EtwSession` is provided by session.rs/session_stub.rs (β3); the β2
//! uninhabited-enum stub previously here is removed.
//!
//! Fields are minimal and match exactly what the Phase 3.4 RED tests
//! prescribe (see SPEC-005 §AC AC-005/AC-006/AC-004/AC-008). The struct
//! is `Clone` because the ring buffer's snapshot_events accessor (test
//! API) needs to return owned copies.

use serde::{Deserialize, Serialize};

/// Discriminator between Launch (activity_id=1) and Terminate
/// (activity_id=2) Kernel-Process events. The integer values match the
/// CGES wire format per ADR-0011 §3 and are exposed via `as u64` cast
/// where serialization needs an integer. Deserialize via `TryFrom<u64>`
/// validates unknown discriminants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "u64", try_from = "u64")]
pub enum ActivityId {
    Launch = 1,
    Terminate = 2,
}

impl From<ActivityId> for u64 {
    fn from(activity_id: ActivityId) -> u64 {
        activity_id as u64
    }
}

impl TryFrom<u64> for ActivityId {
    type Error = String;
    fn try_from(value: u64) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(ActivityId::Launch),
            2 => Ok(ActivityId::Terminate),
            n => Err(format!("invalid activity_id: {n}")),
        }
    }
}

/// Post-capture in-memory event representation.
///
/// Eight fields per the Phase 3.4 RED test contract:
/// - `pid`: PID assigned by Windows at process creation.
/// - `activity_id`: Launch or Terminate.
/// - `image_file_name`: NT-style device path from ETW
///   (`\Device\HarddiskVolumeN\...`); translation to Win32 path happens
///   at emit time per SPEC-005 §Operational §3.
/// - `parent_pid`: kernel `ParentProcessID`; emitted with `name` absent
///   when the parent is unresolvable per ADR-0011 §5 + AC-007.
/// - `command_line`: ETW `CommandLine` field (empty for Terminate).
/// - `subject_user_sid`: SID string form (e.g., `S-1-5-18`).
/// - `etw_timestamp_nanos`: monotonic timestamp from ETW
///   (FILETIME-converted to UTC nanoseconds at capture per
///   SPEC-005 §Operational §1).
/// - `exit_status`: ETW `ExitStatus` for Terminate events; `None` for
///   Launch (serde-skipped at emit time per AC-005).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedEvent {
    pub pid: u32,
    /// Agent-generated UUID v4 unique per capture event. Retries reuse
    /// the same event_id for dedup at the ClickHouse ReplacingMergeTree
    /// merge stage per ADR-0009 §References. Per SPEC-005 §AC AC-001
    /// the event_id is part of the persisted row and round-trips
    /// agent → envelope → ingest → cges_events column.
    pub event_id: String,
    pub activity_id: ActivityId,
    pub image_file_name: String,
    pub parent_pid: u32,
    pub command_line: String,
    pub subject_user_sid: String,
    pub etw_timestamp_nanos: u64,
    pub exit_status: Option<i32>,
}

/// Raw ETW session-open error variants per ADR-0010 §Decision part 1.
///
/// Internal to the etw module. The agent-level error domain wraps these
/// via `EtwError` in `errors.rs`, which is what `handle_etw_open_result`
/// in `startup.rs` consumes. β3 will expand the variant set when the
/// real Win32 `StartTraceW` / `ControlTraceW` paths surface additional
/// failure modes (TraceNotFound, AlreadyExists, ControlFailed, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenError {
    /// Win32 `ERROR_PRIVILEGE_NOT_HELD` (1314). The process is not
    /// running with the SeSystemProfilePrivilege or equivalent required
    /// to open the Kernel-Process provider.
    PrivilegeNotHeld,
    /// Win32 `ERROR_ACCESS_DENIED` (5). Similar to PrivilegeNotHeld in
    /// effect; surfaces on certain hardened SKUs or under group policy.
    AccessDenied,
}
