//! `process.uid` recipe formatter per ADR-0011 §6.
//!
//! The recipe is `<agent_id>:<pid_decimal>:<created_time_unix_nanos_utc>`.
//! Length bounds: 58 chars at `pid=1`, 67 chars at `pid=u32::MAX`. The
//! agent_id is a UUIDv7 (36 chars); the pid is 1-10 decimal digits; the
//! created_time is up to 20 decimal digits (Unix nanoseconds for dates
//! up to year ~2262). Total: 36 + 1 + 1..=10 + 1 + 19..=20 = 58..=67.

/// Format the `process.uid` field for a CGES event.
///
/// Pure function — no allocation hot path (single `format!` heap
/// allocation per call). Phase 4+ optimization if profiling demands.
pub fn format_process_uid(agent_id: &str, pid: u32, created_time_nanos: u64) -> String {
    format!("{agent_id}:{pid}:{created_time_nanos}")
}
