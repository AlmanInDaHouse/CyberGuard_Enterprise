//! Graceful shutdown helper. See SPEC-001 §FR-008 and §Behavior /
//! ShuttingDown.

/// Resolves when the process should shut down. In production this is
/// `tokio::signal::ctrl_c()` (which on Windows also catches
/// `Ctrl+Break`). Integration tests inject their own future instead
/// of calling this directly.
pub async fn wait_for_shutdown() {
    todo!("implemented in the heartbeat commit")
}
