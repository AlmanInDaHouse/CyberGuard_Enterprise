//! CGES event emission — translates `CapturedEvent` to `CgesProcessActivity`.
//!
//! Renders the full SPEC-005 wire shape per the cges_events ClickHouse
//! table DDL (Phase 3.5.E γ), once per event, when its batch is formed
//! (SPEC-017 §Data contracts: a resent event is byte-identical). Three
//! emission entry points:
//! - `render_process_activity(&CapturedEvent, agent_id, &DevicePathMap)`
//!   — what the agent sends: the dispatch-resolved `created_time_nanos`
//!   and `image_file_name` translated to Win32 form when it is a device
//!   path the map resolves (SPEC-017 §Operational §5); a Terminate's base
//!   name passes through unchanged.
//! - `emit_process_activity(&CapturedEvent, agent_id)` — the same with
//!   no drive map (device paths stay verbatim; UNC still applies).
//! - `emit_process_activity_with_cache(&CapturedEvent, Option<u64>,
//!   agent_id)` — with the creation time passed in (None for a cache
//!   miss → JSON null; Some → string-encoded nanos).
//!
//! agent_id is a parameter (not on CapturedEvent) so the capture path
//! stays config-free per Phase 3.5.F Option (a); the delivery loop
//! passes the enrolled identity's agent_id.
//!
//! Serialization conventions per the Phase 3.4 RED tests + γ DDL:
//! - `process.exit_code`: `#[serde(skip_serializing_if = "Option::is_none")]`
//!   → ABSENT in JSON output when None (AC-005 Launch contract).
//! - `process.created_time`: NO skip_serializing_if → serializes as
//!   JSON null when None (AC-004 cache-miss contract).
//! - `process.parent_pid`: similar to created_time; JSON null when
//!   None (ETW reported 0; SPEC-017 §Data contracts).
//! - All other string fields always present; empty string acceptable.

use serde::{Deserialize, Serialize};

use crate::etw::{ActivityId, CapturedEvent};
use crate::paths::DevicePathMap;

/// CGES Process Activity event — wire shape per SPEC-005 §AC + OCSF
/// Process Activity (class_uid 1007). The full 15-field shape mirrors
/// the cges_events ClickHouse table DDL from Phase 3.5.E γ.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CgesProcessActivity {
    pub event_id: String,
    pub class_uid: u32,
    pub activity_id: ActivityId,
    pub process: CgesProcess,
    /// String-encoded integer nanoseconds since Unix epoch, UTC strict.
    /// Serialized as a JSON string (not integer) to avoid IEEE 754
    /// double-precision loss on the server side (u64 nanos > 2^53).
    pub time: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CgesProcess {
    pub pid: u32,
    pub uid: String,
    pub name: String,
    /// JSON null when None (cache-miss path); string-encoded integer
    /// nanos when Some. String to avoid IEEE 754 precision loss (> 2^53).
    pub created_time: Option<String>,
    /// Absent in JSON output when None (Launch path); integer when Some.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// JSON null when None (ETW reported ParentProcessID 0); integer when
    /// Some. Never a `parent_process` object (SPEC-017 §Data contracts).
    pub parent_pid: Option<u32>,
    pub command_line: String,
    pub subject_user_sid: String,
    pub image_file_name: String,
}

const CGES_PROCESS_ACTIVITY_CLASS_UID: u32 = 1007;

/// Render a Process Activity event as the agent sends it: the creation
/// time the dispatch resolved, and `process.image_file_name` in Win32
/// form when `paths` resolves it (verbatim otherwise). `process.name` is
/// the last segment of the rendered path.
pub fn render_process_activity(
    event: &CapturedEvent,
    agent_id: &str,
    paths: &DevicePathMap,
) -> CgesProcessActivity {
    build(
        event,
        event.created_time_nanos,
        paths.translate(&event.image_file_name),
        agent_id,
    )
}

/// Emit a Process Activity event with the creation time the dispatch
/// resolved (`event.created_time_nanos`) and no drive map.
pub fn emit_process_activity(event: &CapturedEvent, agent_id: &str) -> CgesProcessActivity {
    render_process_activity(event, agent_id, &DevicePathMap::empty())
}

/// Emit a Process Activity event with an explicit creation time:
/// - For Launch events: `Some(event.etw_timestamp_nanos)`.
/// - For Terminate events: the cache lookup result (`None` on a miss).
pub fn emit_process_activity_with_cache(
    event: &CapturedEvent,
    cached_created_time: Option<u64>,
    agent_id: &str,
) -> CgesProcessActivity {
    build(
        event,
        cached_created_time,
        DevicePathMap::empty().translate(&event.image_file_name),
        agent_id,
    )
}

fn build(
    event: &CapturedEvent,
    cached_created_time: Option<u64>,
    image_file_name: String,
    agent_id: &str,
) -> CgesProcessActivity {
    let parent_pid = if event.parent_pid == 0 {
        None
    } else {
        Some(event.parent_pid)
    };

    let process_name = derive_process_name(&image_file_name);
    // Cache-hit: use the Launch creation_time (stable across Launch +
    // Terminate per ADR-0011 §6 amendment 2026-05-28). Cache-miss:
    // fallback to this event's own ETW timestamp (best-effort; the
    // Terminate EventRecord carries the termination timestamp, not the
    // creation timestamp, so the uid is not correlatable with a Launch
    // that was never observed — consistent with created_time = null).
    let uid_nanos = cached_created_time.unwrap_or(event.etw_timestamp_nanos);
    let process_uid = crate::etw::format_process_uid(agent_id, event.pid, uid_nanos);

    CgesProcessActivity {
        event_id: event.event_id.clone(),
        class_uid: CGES_PROCESS_ACTIVITY_CLASS_UID,
        activity_id: event.activity_id,
        time: event.etw_timestamp_nanos.to_string(),
        process: CgesProcess {
            pid: event.pid,
            uid: process_uid,
            name: process_name,
            created_time: cached_created_time.map(|n| n.to_string()),
            exit_code: event.exit_status,
            parent_pid,
            command_line: event.command_line.clone(),
            subject_user_sid: event.subject_user_sid.clone(),
            image_file_name,
        },
    }
}

/// Extract the basename from an image path in either form (e.g.,
/// `\Device\HarddiskVolume2\Windows\System32\cmd.exe` or
/// `C:\Windows\System32\cmd.exe` → `cmd.exe`).
///
/// Per AC-006 the process_name must be non-empty; `EventRing::
/// enqueue_or_drop` filters events with an empty image_file_name BEFORE
/// they reach the ring, so this function may assume image_file_name is
/// non-empty.
fn derive_process_name(image_file_name: &str) -> String {
    image_file_name
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("")
        .to_string()
}
