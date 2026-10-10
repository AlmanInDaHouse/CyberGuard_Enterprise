//! `cg-agent` library surface. Public types and the `run()` orchestrator
//! used by both `main.rs` and the integration harness under `tests/`.
//!
//! Module split per SPEC-001 §Behavior:
//!   - `config`   — TOML schema, validation, defaults.
//!   - `envelope` — Heartbeat envelope and identity sub-object types.
//!   - `transport`— HTTP client with retry + exponential backoff.
//!   - `shutdown` — Signal-driven graceful shutdown helper.
//!   - `errors`   — Domain error enums.
//!   - `delivery` — The secure path's at-least-once delivery loop
//!     (SPEC-017).

pub mod canonical;
pub mod cges;
pub mod config;
pub mod crypto;
pub mod delivery;
pub mod enrollment;
pub mod envelope;
pub mod errors;
pub mod etw;
pub mod identity;
pub mod logon;
pub mod paths;
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

/// Where the secure path's events come from (SPEC-017 §Operational §1).
pub enum Capture {
    /// The platform capture backend. On Windows, the ETW session
    /// (Kernel-Process and Kernel-Network, the agent's own connections
    /// excluded) and then the Security-log subscription (SPEC-020), opened
    /// after the identity is loaded; a failed start of either ends the
    /// agent (exit code 9 for privilege, 1 otherwise). Elsewhere there is
    /// no backend: heartbeats only, and one `info` line.
    Platform,
    /// Events fed into this ring by the caller (the harness); no ETW.
    Ring(Arc<crate::etw::EventRing>),
    /// No events: heartbeats only.
    Off,
}

/// Run the secure path: TLS 1.3 mutual authentication presenting the
/// SPEC-002 `identity`, each POST wrapped in a signed outer envelope
/// (SPEC-003), and the events of `capture` delivered at least once by
/// the loop of SPEC-017 §Operational §2–§4 (`delivery.rs`). Every POST is
/// a heartbeat; ticks follow SPEC-001 FR-011.
///
/// On `shutdown_signal` it stops the capture (the ETW session and its
/// thread), then makes one attempt at a final `going_offline` POST
/// carrying the remaining events. A failed capture start, or a fatal
/// TLS or signing failure, returns an [`AgentError`] whose `exit_code()`
/// is 9 or 1 (SPEC-017), or 6/7/8 (SPEC-003 §Failure modes).
pub async fn run_secure<F>(
    config: AgentConfig,
    identity: crate::identity::Identity,
    capture: Capture,
    shutdown_signal: F,
) -> Result<(), AgentError>
where
    F: Future<Output = ()> + Send + 'static,
{
    use crate::errors::TlsError;

    let start_time = Instant::now();

    // The capture source: the identity is already loaded, so the ETW
    // session opens before anything is sent (SPEC-017 §Operational §1).
    let PlatformCapture {
        ring,
        mut session,
        mut logons,
        paths,
    } = match capture {
        Capture::Platform => open_platform_capture()?,
        Capture::Ring(ring) => PlatformCapture::events_only(Some(ring)),
        Capture::Off => PlatformCapture::events_only(None),
    };

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
    let timeout = Duration::from_secs(config.heartbeat.request_timeout_seconds);
    let sender =
        crate::tls::SecureSender::new(client_config, config.server.heartbeat_target(), timeout)?;

    let agent_block = AgentBlock {
        agent_id: identity.agent_id.clone(),
        agent_version: env!("CARGO_PKG_VERSION").to_string(),
        agent_platform: detect_platform().to_string(),
        agent_hostname: config.agent.hostname.clone(),
    };

    let mut delivery = crate::delivery::Delivery::new(
        &sender,
        agent_block,
        &identity.agent_id,
        &identity.keypair,
        &config.heartbeat,
        start_time,
        ring,
        paths,
    );
    let mut shutdown_signal = pin!(shutdown_signal);
    let in_flight = delivery.run_until(shutdown_signal.as_mut()).await?;

    tracing::info!(signal = "shutdown", "shutdown signal received");
    // Stop the logon thread (it closes its subscription), then the session,
    // and wait for both before the final POST, so the events they flushed
    // on the way out ride in it (SPEC-020 §Operational §6).
    if let Some(mut logons) = logons.take() {
        let _ = tokio::task::spawn_blocking(move || logons.stop()).await;
    }
    if let Some(mut session) = session.take() {
        let _ = tokio::task::spawn_blocking(move || session.stop()).await;
    }
    delivery.finish(in_flight).await;
    tracing::info!(
        uptime_seconds = start_time.elapsed().as_secs(),
        "agent stopping"
    );
    Ok(())
}

/// What the secure path captures from: the ring, and the sources that
/// fill it on Windows.
struct PlatformCapture {
    ring: Option<Arc<crate::etw::EventRing>>,
    session: Option<crate::etw::EtwSession>,
    logons: Option<LogonSource>,
    paths: crate::paths::DevicePathMap,
}

#[cfg(windows)]
type LogonSource = crate::logon::LogonSubscription;

/// No logon source off Windows (ADR-0002 Rule 2): a type with no value.
#[cfg(not(windows))]
enum LogonSource {}

#[cfg(not(windows))]
impl LogonSource {
    fn stop(&mut self) {
        match *self {}
    }
}

impl PlatformCapture {
    /// A ring the caller fills, or none: no capture source.
    fn events_only(ring: Option<Arc<crate::etw::EventRing>>) -> Self {
        Self {
            ring,
            session: None,
            logons: None,
            paths: crate::paths::DevicePathMap::empty(),
        }
    }
}

/// Open the platform capture: on Windows the ETW session, then the
/// Security-log subscription into the same ring (SPEC-020 §Operational
/// §6), with the device-path map; on a build without a backend, nothing
/// (one `info`).
fn open_platform_capture() -> Result<PlatformCapture, AgentError> {
    use crate::etw::{EtwSession, OpenError};

    // The agent's own connections are not reported (SPEC-019
    // §Operational §4): the capture excludes this process's id.
    match EtwSession::open_excluding(RING_CAPACITY, std::process::id()) {
        Ok(session) => {
            let ring = Arc::clone(&session.ring);
            let logons = open_logons(&ring, session)?;
            Ok(PlatformCapture {
                ring: Some(ring),
                session: Some(logons.0),
                logons: logons.1,
                paths: device_path_map(),
            })
        }
        Err(OpenError::Unsupported) => {
            tracing::info!(
                platform = detect_platform(),
                "process capture is not available on this platform; sending heartbeats only"
            );
            Ok(PlatformCapture::events_only(None))
        }
        Err(e) => Err(AgentError::Etw(e.into())),
    }
}

/// Open the logon subscription beside an open session; when it cannot
/// open, stop the session and fail (SPEC-020 §Operational §6).
#[cfg(windows)]
fn open_logons(
    ring: &Arc<crate::etw::EventRing>,
    mut session: crate::etw::EtwSession,
) -> Result<(crate::etw::EtwSession, Option<LogonSource>), AgentError> {
    match crate::logon::LogonSubscription::open(Arc::clone(ring)) {
        Ok(logons) => Ok((session, Some(logons))),
        Err(e) => {
            session.stop();
            Err(AgentError::Logon(e))
        }
    }
}

#[cfg(not(windows))]
fn open_logons(
    _ring: &Arc<crate::etw::EventRing>,
    session: crate::etw::EtwSession,
) -> Result<(crate::etw::EtwSession, Option<LogonSource>), AgentError> {
    Ok((session, None))
}

/// The ring capacity (SPEC-005 NFR-005-002).
const RING_CAPACITY: usize = 65536;

/// The device-prefix → drive map, built once at startup (SPEC-017
/// §Operational §5). Empty off Windows, where there is no capture.
fn device_path_map() -> crate::paths::DevicePathMap {
    #[cfg(windows)]
    let map = crate::paths::DevicePathMap::from_system();
    #[cfg(not(windows))]
    let map = crate::paths::DevicePathMap::empty();
    tracing::info!(drive_prefixes = map.len(), "device path map built");
    map
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
