//! `cg-agent` library surface. Public types and the `run()` orchestrator
//! used by both `main.rs` and the integration harness under `tests/`.
//!
//! Module split per SPEC-001 §Behavior:
//!   - `config`   — TOML schema, validation, defaults.
//!   - `envelope` — Heartbeat envelope and identity sub-object types.
//!   - `transport`— HTTP client with retry + exponential backoff.
//!   - `shutdown` — Signal-driven graceful shutdown helper.
//!   - `errors`   — Domain error enums.

pub mod canonical;
pub mod cges;
pub mod config;
pub mod crypto;
pub mod enrollment;
pub mod envelope;
pub mod errors;
pub mod etw;
pub mod identity;
pub mod secure_storage;
pub mod shutdown;
pub mod signing;
pub mod startup;
pub mod tls;
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

/// Run the SPEC-003 secure heartbeat loop: TLS 1.3 mutual authentication
/// presenting the SPEC-002 `identity`, with each heartbeat wrapped in a
/// signed outer envelope (nonce + timestamp + Ed25519 signature). Mirrors
/// [`run`]'s scheduling and graceful-shutdown semantics over a TLS-only
/// transport. A fatal TLS or signature failure returns an [`AgentError`]
/// whose `exit_code()` is 6/7/8 per SPEC-003 §Failure modes; a server
/// rejection of a signed envelope is non-fatal (logged, next interval).
pub async fn run_secure<F>(
    config: AgentConfig,
    identity: crate::identity::Identity,
    shutdown_signal: F,
) -> Result<(), AgentError>
where
    F: Future<Output = ()> + Send + 'static,
{
    use crate::errors::TlsError;

    let start_time = Instant::now();
    let interval = Duration::from_secs(config.heartbeat.interval_seconds);
    let timeout = Duration::from_secs(config.heartbeat.request_timeout_seconds);

    // Build the TLS client config from the trust anchor + SPEC-002 identity.
    let trust_anchor_path = config.server.trust_anchor_path.as_ref().ok_or_else(|| {
        TlsError::ClientConfig("secure path requires server.trust_anchor_path".to_string())
    })?;
    let trust_anchor_pem = std::fs::read(trust_anchor_path).map_err(|e| {
        TlsError::ClientConfig(format!("read trust anchor {trust_anchor_path}: {e}"))
    })?;
    let client_config = crate::tls::build_client_config(&trust_anchor_pem, &identity)?;
    // SPEC-003 Amendment 2026-05-22: heartbeat connects to heartbeat_url
    // when set (enroll/heartbeat on different ports), else server.url.
    let sender =
        crate::tls::SecureSender::new(client_config, config.server.heartbeat_target(), timeout)?;

    let agent_block = AgentBlock {
        agent_id: identity.agent_id.clone(),
        agent_version: env!("CARGO_PKG_VERSION").to_string(),
        agent_platform: detect_platform().to_string(),
        agent_hostname: config.agent.hostname.clone(),
    };
    let agent_id = identity.agent_id.clone();
    let keypair = &identity.keypair;
    const HEARTBEAT_PATH: &str = "/v1/agents/heartbeat";

    let mut shutdown_signal = pin!(shutdown_signal);
    let mut sequence: u64 = 0;

    loop {
        sequence += 1;
        let target = start_time + interval.saturating_mul((sequence - 1) as u32);
        let until = target.saturating_duration_since(Instant::now());

        tokio::select! {
            _ = tokio::time::sleep(until) => {
                send_secure_with_retry(
                    &sender, HEARTBEAT_PATH, &agent_block, sequence, start_time,
                    &agent_id, keypair, &config.heartbeat, HeartbeatStatus::Online,
                ).await?;
            }
            _ = &mut shutdown_signal => {
                tracing::info!(signal = "shutdown", "shutdown signal received");
                send_secure_once(
                    &sender, HEARTBEAT_PATH, &agent_block, sequence, start_time,
                    &agent_id, keypair, HeartbeatStatus::GoingOffline,
                ).await;
                tracing::info!(uptime_seconds = start_time.elapsed().as_secs(), "agent stopping");
                return Ok(());
            }
        }
    }
}

/// Seal and send one heartbeat over TLS with the SPEC-001 retry policy.
/// Returns `Ok(())` on delivery, on a non-fatal server rejection, and on
/// transient-failure exhaustion (all "warn, next interval"). Returns
/// `Err` only on a fatal TLS (exit 6/7) or signing (exit 8) failure.
#[allow(clippy::too_many_arguments)]
async fn send_secure_with_retry(
    sender: &crate::tls::SecureSender,
    path: &str,
    agent_block: &AgentBlock,
    sequence: u64,
    start_time: Instant,
    agent_id: &str,
    keypair: &crate::crypto::AgentKeypair,
    heartbeat: &crate::config::HeartbeatConfig,
    status: HeartbeatStatus,
) -> Result<(), AgentError> {
    use crate::errors::{SigningError, TlsError};
    use crate::tls::SendResult;

    let mut attempt: u32 = 0;
    let mut backoff_ms = heartbeat.backoff_initial_ms;

    loop {
        attempt += 1;
        let inner = build_envelope(agent_block, sequence, start_time, Utc::now(), status);
        let sent_at = inner.sent_at.clone();
        let outer = crate::signing::seal_envelope(inner, agent_id, keypair, &sent_at)?;
        let bytes = serde_json::to_vec(&outer)
            .map_err(|e| SigningError::Canonical(format!("serialize outer envelope: {e}")))?;

        match sender.send(path, &bytes).await {
            SendResult::Status(s) if (200..300).contains(&s) => {
                tracing::info!(
                    sequence_number = sequence,
                    status = ?status,
                    sent_at = %sent_at,
                    response_status = s,
                    "signed heartbeat sent"
                );
                return Ok(());
            }
            SendResult::Status(s) => {
                tracing::warn!(
                    sequence_number = sequence,
                    response_status = s,
                    "signed envelope rejected by server"
                );
                return Ok(());
            }
            SendResult::ServerCertFatal(m) => {
                return Err(TlsError::ServerCertUntrusted(m).into());
            }
            SendResult::ClientCertFatal(m) => {
                return Err(TlsError::ClientCertRejected(m).into());
            }
            SendResult::Transient(m) => {
                if attempt >= heartbeat.max_retries {
                    tracing::warn!(
                        sequence_number = sequence,
                        attempts = attempt,
                        error = %m,
                        "secure heartbeat failed after retries"
                    );
                    return Ok(());
                }
                tracing::warn!(
                    sequence_number = sequence,
                    attempt,
                    backoff_ms,
                    error = %m,
                    "secure heartbeat retry"
                );
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                backoff_ms = ((backoff_ms as f64) * heartbeat.backoff_factor) as u64;
                backoff_ms = backoff_ms.min(heartbeat.backoff_max_ms);
            }
        }
    }
}

/// Best-effort single send for the graceful-shutdown final heartbeat.
#[allow(clippy::too_many_arguments)]
async fn send_secure_once(
    sender: &crate::tls::SecureSender,
    path: &str,
    agent_block: &AgentBlock,
    sequence: u64,
    start_time: Instant,
    agent_id: &str,
    keypair: &crate::crypto::AgentKeypair,
    status: HeartbeatStatus,
) {
    let inner = build_envelope(agent_block, sequence, start_time, Utc::now(), status);
    let sent_at = inner.sent_at.clone();
    if let Ok(outer) = crate::signing::seal_envelope(inner, agent_id, keypair, &sent_at) {
        if let Ok(bytes) = serde_json::to_vec(&outer) {
            let _ = sender.send(path, &bytes).await;
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
