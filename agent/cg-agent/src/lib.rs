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
pub mod crypto;
pub mod enrollment;
pub mod envelope;
pub mod errors;
pub mod identity;
pub mod secure_storage;
pub mod shutdown;
pub mod transport;

use crate::config::AgentConfig;
use crate::envelope::{build_envelope, AgentBlock, HeartbeatStatus};
use crate::errors::AgentError;
use crate::transport::HeartbeatClient;
use chrono::Utc;
use std::future::Future;
use std::io::Write;
use std::pin::pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::EnvFilter;

/// Initialise the structured-JSON tracing subscriber, writing to a
/// caller-supplied sink. In production `main.rs` passes
/// `std::io::stdout`; tests inject a thread-safe buffer for inspection.
pub fn init_logger_with_writer<W>(level: &str, writer: W) -> Result<(), AgentError>
where
    W: Write + Send + 'static,
{
    let filter = EnvFilter::try_new(level).unwrap_or_else(|_| EnvFilter::new("info"));
    let writer = SharedWriter(Arc::new(Mutex::new(writer)));
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_writer(writer)
        .with_env_filter(filter)
        .with_target(true)
        .finish();
    // `set_global_default` returns Err if a subscriber is already set
    // in this process; tests can set their own, so we tolerate that.
    let _ = tracing::subscriber::set_global_default(subscriber);
    Ok(())
}

struct SharedWriter<W: Write + Send + 'static>(Arc<Mutex<W>>);

impl<W: Write + Send + 'static> Clone for SharedWriter<W> {
    fn clone(&self) -> Self {
        // Arc::clone is independent of W: Clone.
        Self(self.0.clone())
    }
}

impl<W: Write + Send + 'static> Write for SharedWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("writer lock").write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.lock().expect("writer lock").flush()
    }
}

impl<'a, W: Write + Send + 'static> MakeWriter<'a> for SharedWriter<W> {
    type Writer = SharedWriter<W>;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Emit a lifecycle log entry at INFO level. Used by the integration
/// harness (AC-009) and by `main.rs` for the "agent starting" /
/// "agent stopping" milestones documented in SPEC-001 §Observability.
pub fn log_lifecycle_event(message: &str, component: &str) {
    tracing::info!(component = component, "{message}");
}

/// Run the agent heartbeat loop to completion.
///
/// Drives the state machine described in SPEC-001 §Behavior:
/// schedule the first heartbeat (sequence_number = 1) immediately,
/// continue on the absolute timeline anchored at the recorded
/// `start_time` (FR-011), and on resolution of `shutdown_signal`
/// send a final heartbeat with `status = "going_offline"` (single
/// attempt, no retry) before returning.
pub async fn run<F>(config: AgentConfig, shutdown_signal: F) -> Result<(), AgentError>
where
    F: Future<Output = ()> + Send + 'static,
{
    let start_time = Instant::now();
    let interval = Duration::from_secs(config.heartbeat.interval_seconds);

    let client = HeartbeatClient::new(config.server.url.clone(), config.heartbeat.clone());
    let agent_block = AgentBlock {
        agent_id: config.agent.id.clone(),
        agent_version: env!("CARGO_PKG_VERSION").to_string(),
        agent_platform: detect_platform().to_string(),
        agent_hostname: config.agent.hostname.clone(),
    };

    let mut shutdown_signal = pin!(shutdown_signal);
    let mut sequence: u64 = 0;

    loop {
        sequence += 1;
        // FR-011: tick N fires at start_time + (N − 1) × interval.
        let target = start_time + interval.saturating_mul((sequence - 1) as u32);
        let now = Instant::now();
        let until = target.saturating_duration_since(now);

        tokio::select! {
            _ = tokio::time::sleep(until) => {
                let envelope = build_envelope(
                    &agent_block,
                    sequence,
                    start_time,
                    Utc::now(),
                    HeartbeatStatus::Online,
                );
                match client.send(&envelope).await {
                    Ok(()) => {
                        tracing::info!(
                            sequence_number = envelope.sequence_number,
                            status = "online",
                            sent_at = %envelope.sent_at,
                            "heartbeat sent"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            sequence_number = envelope.sequence_number,
                            error = %e,
                            "heartbeat failed after retries"
                        );
                    }
                }
            }
            _ = &mut shutdown_signal => {
                tracing::info!(signal = "shutdown", "shutdown signal received");
                let final_envelope = build_envelope(
                    &agent_block,
                    sequence,
                    start_time,
                    Utc::now(),
                    HeartbeatStatus::GoingOffline,
                );
                let _ = client.send_single_attempt(&final_envelope).await;
                tracing::info!(uptime_seconds = start_time.elapsed().as_secs(), "agent stopping");
                return Ok(());
            }
        }
    }
}

/// The compile-time target platform string carried in the heartbeat
/// envelope (SPEC-001) and the enrollment request (SPEC-002 §FR-004,
/// AC-003). Public so the integration harness can assert the value the
/// enrollment request reports matches the host it runs on.
pub fn detect_platform() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "unknown"
    }
}
