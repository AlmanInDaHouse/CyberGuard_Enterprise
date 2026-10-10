//! SPEC-020 auth_ac_011 — startup failures (§Operational §6).
//!
//! The mapping of a subscription failure to the agent's exit: Win32 error
//! 5 and 1314 give exit code 9 and the stderr line of §Operational §6;
//! another code gives exit code 1 and `cg-agent: Security log subscription
//! failed: <code> <message>`. Both reach the agent's exit through
//! `AgentError`.

use cg_agent::errors::{AgentError, LogonError, STDERR_LOGON_PRIVILEGE};

#[test]
fn auth_ac_011_privilege_failures_exit_9() {
    for code in [5u32, 1314] {
        let err = LogonError::from_win32(code);
        assert_eq!(err, LogonError::AccessDenied, "Win32 {code}");
        assert_eq!(err.exit_code(), 9);
        assert_eq!(
            err.stderr_line(),
            "cg-agent: insufficient privilege to read the Security log; run as elevated user"
        );
        assert_eq!(err.stderr_line(), STDERR_LOGON_PRIVILEGE);
        assert_eq!(AgentError::from(err).exit_code(), 9);
    }
}

#[test]
fn auth_ac_011_other_failures_exit_1_with_code_and_message() {
    // ERROR_EVT_CHANNEL_NOT_FOUND.
    let err = LogonError::from_win32(15007);
    let LogonError::Failed { code, message } = &err else {
        panic!("15007 is not a privilege failure: {err:?}");
    };
    assert_eq!(*code, 15007);
    assert!(!message.is_empty());
    assert!(!message.contains("os error"));
    assert_eq!(err.exit_code(), 1);
    assert_eq!(
        err.stderr_line(),
        format!("cg-agent: Security log subscription failed: 15007 {message}")
    );
    assert_eq!(AgentError::from(err).exit_code(), 1);
}
