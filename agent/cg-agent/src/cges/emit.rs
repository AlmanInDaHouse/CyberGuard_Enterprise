//! CGES event emission — translates `CapturedEvent` to `CgesProcessActivity`.
//!
//! Renders the full SPEC-005 wire shape per the cges_events ClickHouse
//! table DDL (Phase 3.5.E γ). Two emission entry points:
//! - `emit_process_activity(&CapturedEvent, agent_id)` — sugar for
//!   cache-less contexts (Launch events: created_time =
//!   etw_timestamp_nanos verbatim).
//! - `emit_process_activity_with_cache(&CapturedEvent, Option<u64>,
//!   agent_id)` — primitive for Terminate events; the cache lookup
//!   result is passed in (None for cache-miss → JSON null; Some for
//!   cache-hit → integer nanos).
//!
//! agent_id is a parameter (not on CapturedEvent) so the capture path
//! stays config-free per Phase 3.5.F Option (a). The ring-drain task in
//! `lib.rs::run_test_mode` passes `&config.agent.id`.
//!
//! Serialization conventions per the Phase 3.4 RED tests + γ DDL:
//! - `process.exit_code`: `#[serde(skip_serializing_if = "Option::is_none")]`
//!   → ABSENT in JSON output when None (AC-005 Launch contract).
//! - `process.created_time`: NO skip_serializing_if → serializes as
//!   JSON null when None (AC-004 cache-miss contract).
//! - `process.parent_pid`: similar to created_time; JSON null when
//!   None (AC-007 PPID unresolvable contract).
//! - All other string fields always present; empty string acceptable.

use serde::{Deserialize, Serialize};

use crate::etw::{ActivityId, CapturedEvent};

/// CGES Process Activity event — wire shape per SPEC-005 §AC + OCSF
/// Process Activity (class_uid 1007). The full 15-field shape mirrors
/// the cges_events ClickHouse table DDL from Phase 3.5.E γ.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CgesProcessActivity {
    pub event_id: String,
    pub class_uid: u32,
    pub activity_id: ActivityId,
    pub process: CgesProcess,
    pub time: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CgesProcess {
    pub pid: u32,
    pub uid: String,
    pub name: String,
    /// JSON null when None (cache-miss path); integer nanos when Some.
    pub created_time: Option<u64>,
    /// Absent in JSON output when None (Launch path); integer when Some.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// JSON null when None (PPID unresolvable per AC-007); integer when Some.
    pub parent_pid: Option<u32>,
    pub command_line: String,
    pub subject_user_sid: String,
    pub image_file_name: String,
}

const CGES_PROCESS_ACTIVITY_CLASS_UID: u32 = 1007;

/// Emit a Process Activity event without consulting a cache.
///
/// For Launch events, `process.created_time` derives directly from the
/// ETW timestamp. For Terminate events without cache awareness, this
/// produces `process.created_time = null`. Production code should use
/// `emit_process_activity_with_cache` for Terminate events.
pub fn emit_process_activity(event: &CapturedEvent, agent_id: &str) -> CgesProcessActivity {
    let cached_created_time = match event.activity_id {
        ActivityId::Launch => Some(event.etw_timestamp_nanos),
        ActivityId::Terminate => None,
    };
    emit_process_activity_with_cache(event, cached_created_time, agent_id)
}

/// Emit a Process Activity event with explicit cache lookup result.
///
/// Primitive emission entry point. The caller (β3 dispatch callback)
/// passes:
/// - For Launch events: `Some(event.etw_timestamp_nanos)` (always populated).
/// - For Terminate events: `cache.consult_and_purge(event.pid)` (Option result).
pub fn emit_process_activity_with_cache(
    event: &CapturedEvent,
    cached_created_time: Option<u64>,
    agent_id: &str,
) -> CgesProcessActivity {
    let parent_pid = if event.parent_pid == 0 {
        None
    } else {
        Some(event.parent_pid)
    };

    let process_name = derive_process_name(&event.image_file_name);
    let process_uid =
        crate::etw::format_process_uid(agent_id, event.pid, event.etw_timestamp_nanos);

    CgesProcessActivity {
        event_id: event.event_id.clone(),
        class_uid: CGES_PROCESS_ACTIVITY_CLASS_UID,
        activity_id: event.activity_id,
        time: event.etw_timestamp_nanos,
        process: CgesProcess {
            pid: event.pid,
            uid: process_uid,
            name: process_name,
            created_time: cached_created_time,
            exit_code: event.exit_status,
            parent_pid,
            command_line: event.command_line.clone(),
            subject_user_sid: event.subject_user_sid.clone(),
            image_file_name: event.image_file_name.clone(),
        },
    }
}

/// Extract the basename from an NT device path (e.g.,
/// `\Device\HarddiskVolume2\Windows\System32\cmd.exe` → `cmd.exe`).
///
/// Per AC-006 the process_name must be non-empty; β3 dispatch callback
/// is responsible for filtering events with empty image_file_name BEFORE
/// they reach the ring (via `EventRing::enqueue_or_drop`), so this
/// function may assume image_file_name is non-empty. Returns empty
/// string if no separator found (degenerate case).
fn derive_process_name(image_file_name: &str) -> String {
    image_file_name
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("")
        .to_string()
}
