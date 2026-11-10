//! `setup.exe`'s exit codes (SPEC-024 Amendment 2026-11-09, item 7): the
//! operator's result. They are the setup's own, not the agent's.

/// The agent is operational.
pub const OPERATIONAL: u8 = 0;

// Refused before the MSI.
/// Not Windows on an x86-64 machine.
pub const PLATFORM: u8 = 20;
/// Not running as an elevated administrator.
pub const ELEVATION: u8 = 21;
/// A fresh host and no input.
pub const NO_INPUT: u8 = 22;
/// The bootstrap file cannot be used: absent, unreadable, not a regular local
/// file, or reachable by others.
pub const BOOTSTRAP_ACCESS: u8 = 23;
/// An input, or the configuration generated from it, is invalid.
pub const INVALID_INPUT: u8 = 24;
/// The trust anchor cannot be read or does not match its fingerprints.
pub const ANCHOR: u8 = 25;
/// A conflicting service.
pub const CONFLICTING_SERVICE: u8 = 26;
/// An untrusted path: a reparse point, or what fails the service's check.
pub const UNTRUSTED_PATH: u8 = 27;
/// A console layout, or a migration asked otherwise.
pub const CONSOLE_LAYOUT: u8 = 28;
/// A downgrade.
pub const DOWNGRADE: u8 = 29;
/// Already configured: an identity is there.
pub const ALREADY_CONFIGURED: u8 = 30;
/// The service was not proved stopped.
pub const NOT_STOPPED: u8 = 31;

/// The setup could not write or check its own state.
pub const OWN_STATE: u8 = 40;
// The MSI having committed and staying installed.
/// The SCM refused the start.
pub const START_REFUSED: u8 = 41;
/// The service stopped with a code.
pub const SERVICE_STOPPED: u8 = 42;
/// Pending at the timeout.
pub const PENDING: u8 = 43;
/// Running but not observable.
pub const NOT_OBSERVABLE: u8 = 44;
/// A restart is pending.
pub const RESTART_PENDING: u8 = 45;

/// A refusal: the exit code and the line the setup writes. No line carries
/// the token or quotes a line of an input file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub code: u8,
    pub line: String,
}

impl Refusal {
    /// `code` with the line `cg-setup: <text>`.
    pub fn new(code: u8, text: impl std::fmt::Display) -> Self {
        Refusal {
            code,
            line: format!("cg-setup: {text}"),
        }
    }
}
