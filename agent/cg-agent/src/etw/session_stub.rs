//! Non-Windows stub for `EtwSession`.
//!
//! Debug builds: provides a stub `EtwSession::open()` that returns
//! `Err(OpenError::AccessDenied)`. The agent's startup path treats this
//! identically to a real ETW open failure on Windows. Linux developers
//! can build + run the agent for non-ETW code paths without it crashing
//! at the secure_storage hard-stop (debug builds tolerate non-Windows
//! per `secure_storage.rs:14` `cfg(all(not(windows), not(debug_assertions)))`).
//!
//! Release builds: compile_error per the `secure_storage.rs` precedent.
//! Production deployments on non-Windows are not supported.

#[cfg(all(not(windows), not(debug_assertions)))]
compile_error!(
    "cg-agent has no production ETW backend on non-Windows targets. \
     SPEC-005 explicitly scopes ETW capture to Windows per ADR-0011 \
     Out-of-scope (Linux/macOS deferred). To build for Linux/macOS, \
     use the debug profile (which stubs ETW out)."
);

use std::sync::Arc;

use super::cache::CreatedTimeCache;
use super::ring::EventRing;
use super::types::OpenError;

/// Non-Windows stub for EtwSession.
///
/// `open()` always returns Err(OpenError::AccessDenied). The ring + cache
/// fields exist so callers (lib.rs `run_test_mode`) compile against the
/// same struct shape as the Windows impl, even though they are never
/// populated on non-Windows.
pub struct EtwSession {
    pub ring: Arc<EventRing>,
    pub cache: Arc<CreatedTimeCache>,
}

impl EtwSession {
    pub fn open(_ring_capacity: usize) -> Result<Self, OpenError> {
        Err(OpenError::AccessDenied)
    }
}
