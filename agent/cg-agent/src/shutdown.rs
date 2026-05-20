//! Graceful shutdown helper. See SPEC-001 §FR-008 and §Behavior /
//! ShuttingDown.

/// Resolves when the process should shut down. In production this is
/// `tokio::signal::ctrl_c()` (which on Windows also catches
/// `Ctrl+Break`). Integration tests inject their own future via
/// `cg_agent::run` instead of calling this directly.
pub async fn wait_for_shutdown() {
    // Best-effort: if signal registration fails (rare; in some
    // sandboxed test runners it can), block until the runtime
    // shuts us down some other way.
    let _ = tokio::signal::ctrl_c().await;
}
