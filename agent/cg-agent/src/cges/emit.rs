//! CGES event emission — translates `CapturedEvent` to `CgesProcessActivity`.
//!
//! Two emission entry points:
//! - `emit_process_activity(&CapturedEvent)` — sugar for cache-less
//!   contexts (Launch events: created_time = etw_timestamp_nanos
//!   verbatim).
//! - `emit_process_activity_with_cache(&CapturedEvent, Option<u64>)` —
//!   primitive for Terminate events; the cache lookup result is passed
//!   in (None for cache-miss → JSON null; Some for cache-hit → integer
//!   nanos).
//!
//! Serialization conventions per the Phase 3.4 RED tests:
//! - `process.exit_code`: `#[serde(skip_serializing_if = "Option::is_none")]`
//!   → ABSENT in JSON output when None (AC-005 Launch contract).
//! - `process.created_time`: NO skip_serializing_if → serializes as
//!   JSON null when None (AC-004 cache-miss contract). Both fields are
//!   `Option<...>` semantically but render differently.

use serde::Serialize;

use crate::etw::{ActivityId, CapturedEvent};

/// CGES Process Activity event — wire shape per SPEC-005 §AC + OCSF
/// Process Activity (class_uid 1007). β3 will extend this with
/// parent_process resolution + image_file_name path translation;
/// β2 provides the core schema.
#[derive(Debug, Serialize)]
pub struct CgesProcessActivity {
    pub class_uid: u32,
    pub activity_id: ActivityId,
    pub process: CgesProcess,
}

#[derive(Debug, Serialize)]
pub struct CgesProcess {
    pub pid: u32,
    /// JSON null when None (cache-miss path); integer nanos when Some.
    pub created_time: Option<u64>,
    /// Absent in JSON output when None (Launch path); integer when Some.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

const CGES_PROCESS_ACTIVITY_CLASS_UID: u32 = 1007;

/// Emit a Process Activity event without consulting a cache.
///
/// For Launch events, `process.created_time` derives directly from the
/// ETW timestamp. For Terminate events without cache awareness, this
/// produces `process.created_time = null`; production code should use
/// `emit_process_activity_with_cache` for Terminate events.
pub fn emit_process_activity(event: &CapturedEvent) -> CgesProcessActivity {
    let cached_created_time = match event.activity_id {
        ActivityId::Launch => Some(event.etw_timestamp_nanos),
        ActivityId::Terminate => None,
    };
    emit_process_activity_with_cache(event, cached_created_time)
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
) -> CgesProcessActivity {
    CgesProcessActivity {
        class_uid: CGES_PROCESS_ACTIVITY_CLASS_UID,
        activity_id: event.activity_id,
        process: CgesProcess {
            pid: event.pid,
            created_time: cached_created_time,
            exit_code: event.exit_status,
        },
    }
}
