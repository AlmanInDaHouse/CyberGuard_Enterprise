//! SPEC-005 AC-003 — `process.uid` byte-level recipe pinned by fixture.
//!
//! Asserts the formatter produces byte-for-byte the recipe
//! `<agent_id>:<pid_decimal>:<created_time_unix_nanos_utc>` per ADR-0011 §6,
//! including the two boundary assertions at `pid=1` (length 58) and
//! `pid=4294967295` (length 67) that pin the 58–67 character bound.
//!
//! Pure unit test — no testcontainers, no ETW, no ingest, no async runtime.

use cg_agent::etw::format_process_uid;

const AGENT_ID: &str = "01934abc-def0-7000-89ab-000000000001";
const CREATED_TIME_NANOS: u64 = 1716123612901000000;

#[test]
fn ac_003_nominal_fixture() {
    let actual = format_process_uid(AGENT_ID, 7144, CREATED_TIME_NANOS);
    let expected = "01934abc-def0-7000-89ab-000000000001:7144:1716123612901000000";
    assert_eq!(actual.as_str(), expected);
    assert_eq!(actual.len(), 61, "nominal fixture length");
}

#[test]
fn ac_003_lower_bound_pid_1() {
    let actual = format_process_uid(AGENT_ID, 1, CREATED_TIME_NANOS);
    assert_eq!(actual.len(), 58, "pid=1 length anchors ADR-0011 §6 lower bound");
    assert!(actual.ends_with(":1:1716123612901000000"));
}

#[test]
fn ac_003_upper_bound_pid_u32_max() {
    let actual = format_process_uid(AGENT_ID, 4_294_967_295, CREATED_TIME_NANOS);
    assert_eq!(actual.len(), 67, "pid=u32::MAX length anchors ADR-0011 §6 upper bound");
    assert!(actual.ends_with(":4294967295:1716123612901000000"));
}
