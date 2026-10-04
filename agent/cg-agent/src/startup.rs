//! Agent startup orchestration helpers.
//!
//! Houses ETW session-start result handling per ADR-0010 §Decision
//! part 1, SPEC-005 §AC AC-002 and SPEC-017 §Operational §1.

use crate::errors::EtwError;
use crate::etw::OpenError;

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

/// Map an ETW session-start result to a startup action.
///
/// - `Ok(())`: continue startup with capture.
/// - `Err(OpenError::PrivilegeNotHeld | AccessDenied)`: abort with exit
///   code 9 and the ADR-0010 §1 line (SPEC-005 AC-002).
/// - `Err(OpenError::Failed { .. })`: abort with exit code 1 and
///   `cg-agent: ETW session open failed: <code> <message>`.
/// - `Err(OpenError::Unsupported)`: continue startup without capture —
///   a build with no capture backend sends heartbeats only (SPEC-017
///   §Operational §1).
pub fn handle_etw_open_result(result: Result<(), OpenError>) -> Result<(), StartupAbort> {
    match result {
        Ok(()) | Err(OpenError::Unsupported) => Ok(()),
        Err(e) => {
            let err = EtwError::from(e);
            Err(StartupAbort {
                exit_code: i32::from(err.exit_code()),
                stderr_message: err.stderr_line(),
            })
        }
    }
}
