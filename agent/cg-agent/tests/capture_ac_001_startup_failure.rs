//! SPEC-017 capture_ac_001 — startup failure.
//!
//! A privilege failure of the ETW session start maps to exit code 9 with
//! the SPEC-005 AC-002 line; any other start failure maps to exit code 1
//! with `cg-agent: ETW session open failed: <code> <message>`; a build
//! without a capture backend continues startup. `EtwSession::open`
//! returns the start result itself: on unelevated Windows it is a
//! privilege failure; elevated, the session opens and `stop` leaves no
//! session of the agent's name behind.

use cg_agent::errors::{AgentError, EtwError, STDERR_INSUFFICIENT_PRIVILEGE};
use cg_agent::etw::{win32_from_os_error, EtwSession, OpenError};
use cg_agent::startup::handle_etw_open_result;

const AC_002_LINE: &str = "cg-agent: insufficient privilege to open \
                           Microsoft-Windows-Kernel-Process ETW session; \
                           run as elevated user or LocalSystem";

#[test]
fn capture_ac_001_privilege_failure_exits_9_with_the_ac_002_line() {
    assert_eq!(STDERR_INSUFFICIENT_PRIVILEGE, AC_002_LINE);
    for open_error in [OpenError::AccessDenied, OpenError::PrivilegeNotHeld] {
        let etw = EtwError::from(open_error.clone());
        assert_eq!(etw.exit_code(), 9, "{open_error:?} must exit 9");
        assert_eq!(etw.stderr_line(), AC_002_LINE);
        assert_eq!(AgentError::Etw(etw).exit_code(), 9);

        let abort = handle_etw_open_result(Err(open_error)).expect_err("privilege must abort");
        assert_eq!(abort.exit_code, 9);
        assert_eq!(abort.stderr_message, AC_002_LINE);
    }
}

#[test]
fn capture_ac_001_other_failure_exits_1_with_its_code() {
    // ERROR_GEN_FAILURE (31): any start failure that is not a privilege one.
    let open_error = OpenError::from_win32(31);
    assert!(matches!(open_error, OpenError::Failed { code: 31, .. }));
    assert!(!open_error.is_privilege());

    let etw = EtwError::from(open_error.clone());
    assert_eq!(etw.exit_code(), 1);
    let line = etw.stderr_line();
    assert!(
        line.starts_with("cg-agent: ETW session open failed: 31 "),
        "unexpected line: {line}"
    );
    assert!(
        !line.contains("os error"),
        "no duplicated code suffix: {line}"
    );
    assert_eq!(AgentError::Etw(etw).exit_code(), 1);

    let abort = handle_etw_open_result(Err(open_error)).expect_err("a start failure must abort");
    assert_eq!(abort.exit_code, 1);
    assert_eq!(abort.stderr_message, line);
}

#[test]
fn capture_ac_001_privilege_codes_are_classified() {
    assert_eq!(OpenError::from_win32(5), OpenError::AccessDenied);
    assert_eq!(OpenError::from_win32(1314), OpenError::PrivilegeNotHeld);
}

#[test]
fn capture_ac_001_hresult_carries_the_win32_code() {
    // ferrisetw reports StartTraceW failures as HRESULT_FROM_WIN32 values.
    assert_eq!(win32_from_os_error(0x8007_0005_u32 as i32), 5);
    assert_eq!(win32_from_os_error(0x8007_0522_u32 as i32), 1314);
    assert_eq!(win32_from_os_error(5), 5);
}

#[test]
fn capture_ac_001_no_backend_continues_startup() {
    assert_eq!(handle_etw_open_result(Err(OpenError::Unsupported)), Ok(()));
    assert_eq!(handle_etw_open_result(Ok(())), Ok(()));
}

/// Real ETW, either way: unelevated, `open` reports the privilege failure
/// (it no longer returns `Ok` before the start runs); elevated, the
/// session opens and `stop` leaves no session of the agent's name.
#[cfg(windows)]
#[test]
fn capture_ac_001_open_returns_the_start_result() {
    match EtwSession::open(16) {
        Ok(mut session) => {
            session.stop();
            // ERROR_WMI_INSTANCE_NOT_FOUND: no session of that name exists.
            assert_eq!(
                cg_agent::etw::events_lost(cg_agent::etw::SESSION_NAME),
                Err(4201),
                "elevated: stop must leave no session behind"
            );
        }
        Err(e) => assert!(
            e.is_privilege(),
            "unelevated: open must report the privilege failure, got {e:?}"
        ),
    }
}

#[cfg(not(windows))]
#[test]
fn capture_ac_001_open_reports_no_backend() {
    assert!(matches!(EtwSession::open(16), Err(OpenError::Unsupported)));
}
