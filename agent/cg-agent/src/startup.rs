//! Agent startup orchestration helpers.
//!
//! Currently houses ETW open-result handling per ADR-0010 §Decision
//! part 1 + SPEC-005 §AC AC-002. Future additions (Phase 4+) may
//! include other privilege-check / capability-probe orchestration as
//! the agent's startup surface grows.

use crate::etw::OpenError;

/// Exact stderr line emitted on insufficient ETW privilege per
/// ADR-0010 §1. Test AC-002 references this verbatim via the
/// `StartupAbort.stderr_message` field. The Rust line-continuation
/// `\` syntax collapses each `\` + newline + leading whitespace to a
/// single character boundary, producing a single-line string.
const STDERR_INSUFFICIENT_PRIVILEGE: &str = "cg-agent: insufficient privilege to open \
                                              Microsoft-Windows-Kernel-Process ETW session; \
                                              run as elevated user or LocalSystem";

/// Result of a failed startup check: the exit code the process must
/// exit with + the exact stderr line to emit before exit.
///
/// Returned as the `Err` branch from `handle_etw_open_result`. Test
/// AC-002 inspects both fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupAbort {
    pub exit_code: i32,
    pub stderr_message: String,
}

/// Map an ETW session-open result to a startup action.
///
/// On `Ok(())`: returns `Ok(())` (continue startup).
/// On `Err(OpenError::PrivilegeNotHeld | AccessDenied)`: returns
/// `Err(StartupAbort { exit_code: 9, stderr_message: ... })` per
/// ADR-0010 §Decision part 1.
///
/// Exit code 9 is the SPEC-005-introduced code for ETW-specific
/// privilege failures; codes 1-8 are inherited per `errors.rs`.
pub fn handle_etw_open_result(result: Result<(), OpenError>) -> Result<(), StartupAbort> {
    match result {
        Ok(()) => Ok(()),
        Err(OpenError::PrivilegeNotHeld | OpenError::AccessDenied) => Err(StartupAbort {
            exit_code: 9,
            stderr_message: STDERR_INSUFFICIENT_PRIVILEGE.to_string(),
        }),
    }
}
