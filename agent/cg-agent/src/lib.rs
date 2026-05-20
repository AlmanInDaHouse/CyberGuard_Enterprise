//! `cg-agent` library surface. Public types and the `run()` orchestrator
//! used by both `main.rs` and the integration harness under `tests/`.
//!
//! Module split per SPEC-001 §Behavior:
//!   - `config`   — TOML schema, validation, defaults.
//!   - `envelope` — Heartbeat envelope and identity sub-object types.
//!   - `transport`— HTTP client with retry + exponential backoff.
//!   - `shutdown` — Signal-driven graceful shutdown helper.
//!   - `errors`   — Domain error enums.

pub mod config;
pub mod envelope;
pub mod errors;
pub mod shutdown;
pub mod transport;

use crate::config::AgentConfig;
use crate::errors::AgentError;
use std::io::Write;

/// Initialise the structured-JSON tracing subscriber, writing to a
/// caller-supplied sink. In production `main.rs` passes
/// `std::io::stdout`; tests inject a thread-safe buffer for inspection.
pub fn init_logger_with_writer<W>(_level: &str, _writer: W) -> Result<(), AgentError>
where
    W: Write + Send + Clone + 'static,
{
    todo!("implemented in the heartbeat commit")
}

/// Emit a lifecycle log entry at INFO level. Used by the integration
/// harness (AC-009) and by `main.rs` for the "agent starting" /
/// "agent stopping" milestones documented in SPEC-001 §Observability.
pub fn log_lifecycle_event(_message: &str, _component: &str) {
    todo!("implemented in the heartbeat commit")
}

/// Run the agent heartbeat loop to completion.
///
/// Drives the state machine described in SPEC-001 §Behavior:
/// schedule the first heartbeat (sequence_number = 1) within 5 s,
/// continue on the absolute timeline anchored at the recorded
/// `start_time`, and on resolution of `shutdown_signal` send a
/// final heartbeat with `status = "going_offline"` (single
/// attempt) before returning.
///
/// `shutdown_signal` is a future that resolves when the agent
/// should stop. In production this is `tokio::signal::ctrl_c()`
/// mapped to `()`. Integration tests inject a `oneshot::Receiver`
/// to drive shutdown deterministically.
pub async fn run<F>(_config: AgentConfig, _shutdown_signal: F) -> Result<(), AgentError>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    todo!("implemented in the heartbeat commit")
}
