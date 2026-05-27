//! SPEC-005 AC-005 — `process.exit_code` conditional emission contract.
//!
//! Asserts the stricter-than-schema agent normative per ADR-0011 §4
//! amendment (b): Launch events MUST NOT emit the `process.exit_code`
//! field (absent, not null); Terminate events MUST emit it as signed
//! int32 matching the ETW `ExitStatus`. Three pinned values for
//! Terminate: 0 (clean exit), 1 (nominal non-zero), and -1073741819
//! (the signed-int32 interpretation of NTSTATUS `0xC0000005`
//! `STATUS_ACCESS_VIOLATION` per ADR-0011 §4 amendment (b)).
//!
//! Pure unit test — synthetic CapturedEvent → emit_process_activity
//! formatter → JSON inspection. No ETW, no testcontainers.

use cg_agent::cges::emit_process_activity;
use cg_agent::etw::{ActivityId, CapturedEvent};
use serde_json::Value;

fn synthetic_event(activity_id: ActivityId, exit_status: Option<i32>) -> CapturedEvent {
    CapturedEvent {
        pid: 7144,
        event_id: "synthetic-ac005-test".to_string(),
        activity_id,
        image_file_name: String::from("\\Device\\HarddiskVolume2\\Windows\\System32\\cmd.exe"),
        parent_pid: 4,
        command_line: String::from("cmd.exe /c exit"),
        subject_user_sid: String::from("S-1-5-18"),
        etw_timestamp_nanos: 1716123612901000000,
        exit_status,
    }
}

#[test]
fn ac_005_launch_omits_exit_code_field() {
    let event = synthetic_event(ActivityId::Launch, None);
    let json: Value = serde_json::to_value(emit_process_activity(&event, "test-agent-id"))
        .expect("emit_process_activity output must be JSON-serialisable");

    let process = json
        .pointer("/process")
        .expect("emitted event MUST have /process object");
    assert!(
        process.get("exit_code").is_none(),
        "AC-005: Launch event MUST NOT have process.exit_code (absent, not null)"
    );
}

#[test]
fn ac_005_terminate_emits_exit_code_zero() {
    let event = synthetic_event(ActivityId::Terminate, Some(0));
    let json: Value = serde_json::to_value(emit_process_activity(&event, "test-agent-id")).unwrap();
    let exit_code = json
        .pointer("/process/exit_code")
        .expect("Terminate event MUST emit process.exit_code");
    assert_eq!(exit_code.as_i64(), Some(0));
}

#[test]
fn ac_005_terminate_emits_exit_code_one() {
    let event = synthetic_event(ActivityId::Terminate, Some(1));
    let json: Value = serde_json::to_value(emit_process_activity(&event, "test-agent-id")).unwrap();
    let exit_code = json.pointer("/process/exit_code").unwrap();
    assert_eq!(exit_code.as_i64(), Some(1));
}

#[test]
fn ac_005_terminate_emits_ntstatus_as_negative_int32() {
    let event = synthetic_event(ActivityId::Terminate, Some(-1073741819));
    let json: Value = serde_json::to_value(emit_process_activity(&event, "test-agent-id")).unwrap();
    let exit_code = json.pointer("/process/exit_code").unwrap();
    assert_eq!(
        exit_code.as_i64(),
        Some(-1073741819),
        "AC-005: 0xC0000005 STATUS_ACCESS_VIOLATION as signed int32 per ADR-0011 §4 amendment (b)"
    );
}
