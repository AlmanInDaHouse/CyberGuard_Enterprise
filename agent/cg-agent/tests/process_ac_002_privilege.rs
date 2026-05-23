//! SPEC-005 AC-002 — clean-fail on insufficient privilege.
//!
//! Asserts the agent's contract per ADR-0010 §Decision part 1: when the
//! ETW session open path returns `ERROR_PRIVILEGE_NOT_HELD` (1314) or
//! `ERROR_ACCESS_DENIED` (5), the agent exits with code 9 and writes
//! the exact stderr line specified by ADR-0010.
//!
//! Pure unit test — synthetic Err injection at the ETW open boundary,
//! CI-elevation-independent (does not require actually opening ETW).

use cg_agent::etw::OpenError;
use cg_agent::startup::handle_etw_open_result;

const EXPECTED_STDERR: &str = "cg-agent: insufficient privilege to open \
                               Microsoft-Windows-Kernel-Process ETW session; \
                               run as elevated user or LocalSystem";

#[test]
fn ac_002_privilege_not_held_aborts_with_exit_9() {
    let abort = handle_etw_open_result(Err(OpenError::PrivilegeNotHeld))
        .expect_err("PrivilegeNotHeld MUST cause startup abort");
    assert_eq!(abort.exit_code, 9, "AC-002: exit code 9 per ADR-0010 §1");
    assert_eq!(
        abort.stderr_message, EXPECTED_STDERR,
        "AC-002: stderr line MUST match ADR-0010 §1 verbatim"
    );
}

#[test]
fn ac_002_access_denied_aborts_with_exit_9() {
    let abort = handle_etw_open_result(Err(OpenError::AccessDenied))
        .expect_err("AccessDenied MUST cause startup abort");
    assert_eq!(abort.exit_code, 9, "AC-002: exit code 9 per ADR-0010 §1");
    assert_eq!(
        abort.stderr_message, EXPECTED_STDERR,
        "AC-002: stderr line MUST match ADR-0010 §1 verbatim"
    );
}

#[test]
fn ac_002_success_does_not_abort() {
    let result = handle_etw_open_result(Ok(()));
    assert!(
        result.is_ok(),
        "AC-002: successful ETW open MUST NOT trigger startup abort"
    );
}
