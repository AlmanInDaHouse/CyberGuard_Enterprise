//! The secure path's delivery loop (SPEC-017 §Operational §2–§4).
//!
//! One POST is in flight at a time and POSTs go out in order. A POST is
//! formed, when none is in flight, as soon as 1024 events are buffered,
//! a buffered event is 5000 ms old, or a heartbeat tick is due; it
//! carries at most 1024 events, rendered once (SPEC-017 §Data contracts),
//! and takes the next `sequence_number`. Heartbeat ticks follow SPEC-001
//! FR-011's absolute timeline; a tick sends a POST without events only
//! if no batch POST was formed since the previous tick.
//!
//! A retry is the same POST — same `sequence_number`, same events — with
//! a fresh `nonce`, `sent_at` and signature. A transient failure (a
//! connection error, a timeout, a 5xx) is retried with the SPEC-001
//! backoff: until delivered or rejected for a POST with events, up to
//! `max_retries` attempts for one without. A rejection (any other
//! non-2xx) is retried up to `max_retries` attempts in total; then the
//! batch is dropped, counted in the ring's dropped total and logged at
//! `error`. Fatal TLS and signing failures end the agent (exit 6, 7, 8).
//!
//! On shutdown the caller stops the capture, then `finish` makes one
//! attempt at a final `going_offline` POST carrying the batch in flight,
//! or else the next batch, or no events.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;

use crate::cges::{render_process_activity, CgesProcessActivity};
use crate::config::HeartbeatConfig;
use crate::crypto::AgentKeypair;
use crate::envelope::{build_envelope, AgentBlock, HeartbeatStatus};
use crate::errors::{AgentError, SigningError, TlsError};
use crate::etw::{EventRing, OverflowWarning};
use crate::paths::{DevicePathMap, UnresolvedPathLog};
use crate::tls::{SecureSender, SendResult};

/// A POST carries at most this many events (SPEC-005 NFR-005-002).
pub const MAX_BATCH_EVENTS: usize = 1024;

/// A buffered event older than this triggers a POST (SPEC-005
/// NFR-005-002).
pub const MAX_BATCH_LATENCY: Duration = Duration::from_millis(5000);

/// How often the loop re-checks the batch triggers while idle.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

const HEARTBEAT_PATH: &str = "/v1/agents/heartbeat";

/// One POST: its sequence number and its events, rendered once.
pub(crate) struct Batch {
    sequence: u64,
    events: Vec<CgesProcessActivity>,
}

/// The POST in flight and its retry state.
pub(crate) struct Pending {
    batch: Batch,
    transient_failures: u32,
    rejections: u32,
    backoff_ms: u64,
}

/// The result of one attempt at a POST.
enum Attempt {
    Delivered,
    Transient(String),
    Rejected(u16),
}

/// The delivery loop's state.
pub(crate) struct Delivery<'a> {
    sender: &'a SecureSender,
    agent_block: AgentBlock,
    agent_id: &'a str,
    keypair: &'a AgentKeypair,
    heartbeat: &'a HeartbeatConfig,
    start_time: Instant,
    ring: Option<Arc<EventRing>>,
    paths: DevicePathMap,
    unresolved: UnresolvedPathLog,
    overflow: OverflowWarning,
    sequence: u64,
}

impl<'a> Delivery<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        sender: &'a SecureSender,
        agent_block: AgentBlock,
        agent_id: &'a str,
        keypair: &'a AgentKeypair,
        heartbeat: &'a HeartbeatConfig,
        start_time: Instant,
        ring: Option<Arc<EventRing>>,
        paths: DevicePathMap,
    ) -> Self {
        Self {
            sender,
            agent_block,
            agent_id,
            keypair,
            heartbeat,
            start_time,
            ring,
            paths,
            unresolved: UnresolvedPathLog::new(),
            overflow: OverflowWarning::new(),
            sequence: 0,
        }
    }

    /// Run until `shutdown` resolves. Returns the POST still in flight
    /// at that moment, for `finish`; an `Err` only on a fatal TLS or
    /// signing failure.
    pub(crate) async fn run_until<F>(
        &mut self,
        mut shutdown: Pin<&mut F>,
    ) -> Result<Option<Pending>, AgentError>
    where
        F: Future<Output = ()>,
    {
        let interval = Duration::from_secs(self.heartbeat.interval_seconds);
        // Index of the next heartbeat tick: start_time + k · interval.
        let mut next_tick: u32 = 0;
        let mut heartbeat_due = false;
        let mut batch_post_since_tick = false;
        let mut in_flight: Option<Pending> = None;

        loop {
            let now = Instant::now();
            let tick_at = self.start_time + interval.saturating_mul(next_tick);
            if now >= tick_at {
                if !batch_post_since_tick {
                    heartbeat_due = true;
                }
                batch_post_since_tick = false;
                // Missed ticks collapse into this one (FR-011: the
                // schedule never shifts).
                while self.start_time + interval.saturating_mul(next_tick) <= now {
                    next_tick = next_tick.saturating_add(1);
                }
            }
            if let Some(ring) = &self.ring {
                self.overflow.check(ring, now);
            }

            if in_flight.is_none() {
                let batch_due = self.batch_due(now);
                if heartbeat_due || batch_due {
                    if !heartbeat_due {
                        batch_post_since_tick = true;
                    }
                    heartbeat_due = false;
                    in_flight = Some(Pending {
                        batch: self.form_batch(),
                        transient_failures: 0,
                        rejections: 0,
                        backoff_ms: self.heartbeat.backoff_initial_ms,
                    });
                }
            }

            let Some(pending) = in_flight.as_mut() else {
                let tick_at = self.start_time + interval.saturating_mul(next_tick);
                let wake = tick_at.min(Instant::now() + POLL_INTERVAL);
                tokio::select! {
                    _ = tokio::time::sleep_until(wake.into()) => {}
                    _ = &mut shutdown => return Ok(None),
                }
                continue;
            };

            let attempt = tokio::select! {
                attempt = self.attempt(&pending.batch, HeartbeatStatus::Online) => Some(attempt),
                _ = &mut shutdown => None,
            };
            let Some(attempt) = attempt else {
                return Ok(in_flight);
            };

            let retry = match attempt? {
                Attempt::Delivered => false,
                Attempt::Transient(error) => {
                    pending.transient_failures += 1;
                    if pending.batch.events.is_empty()
                        && pending.transient_failures >= self.heartbeat.max_retries
                    {
                        tracing::warn!(
                            sequence_number = pending.batch.sequence,
                            attempts = pending.transient_failures,
                            error = %error,
                            "secure heartbeat failed after retries"
                        );
                        false
                    } else {
                        tracing::warn!(
                            sequence_number = pending.batch.sequence,
                            events_count = pending.batch.events.len(),
                            attempt = pending.transient_failures,
                            backoff_ms = pending.backoff_ms,
                            error = %error,
                            "secure heartbeat retry"
                        );
                        true
                    }
                }
                Attempt::Rejected(status) => {
                    pending.rejections += 1;
                    tracing::warn!(
                        sequence_number = pending.batch.sequence,
                        response_status = status,
                        attempt = pending.rejections,
                        "signed envelope rejected by server"
                    );
                    if pending.rejections >= self.heartbeat.max_retries {
                        self.drop_rejected(&pending.batch, status);
                        false
                    } else {
                        true
                    }
                }
            };

            if !retry {
                in_flight = None;
                continue;
            }
            let backoff = Duration::from_millis(pending.backoff_ms);
            pending.backoff_ms = next_backoff(pending.backoff_ms, self.heartbeat);
            let stopped = tokio::select! {
                _ = tokio::time::sleep(backoff) => false,
                _ = &mut shutdown => true,
            };
            if stopped {
                return Ok(in_flight);
            }
        }
    }

    /// The final `going_offline` POST, one attempt (SPEC-017
    /// §Operational §4): the batch in flight, or else the next batch, or
    /// no events. Events left undelivered are counted at `warn`.
    pub(crate) async fn finish(&mut self, in_flight: Option<Pending>) {
        let batch = match in_flight {
            Some(pending) => pending.batch,
            None => self.form_batch(),
        };
        let delivered = matches!(
            self.attempt(&batch, HeartbeatStatus::GoingOffline).await,
            Ok(Attempt::Delivered)
        );
        let left_in_ring = self.ring.as_ref().map_or(0, |ring| ring.len());
        let undelivered = left_in_ring + if delivered { 0 } else { batch.events.len() };
        if undelivered > 0 {
            tracing::warn!(
                events_undelivered = undelivered,
                "events not delivered before shutdown are lost"
            );
        }
    }

    /// A batch trigger holds: 1024 events buffered, or the oldest is
    /// 5000 ms old.
    fn batch_due(&self, now: Instant) -> bool {
        let Some(ring) = &self.ring else {
            return false;
        };
        if ring.len() >= MAX_BATCH_EVENTS {
            return true;
        }
        ring.oldest_enqueued_at()
            .is_some_and(|at| now.saturating_duration_since(at) >= MAX_BATCH_LATENCY)
    }

    /// Drain up to 1024 events, render them once, and take the next
    /// sequence number.
    fn form_batch(&mut self) -> Batch {
        let captured = self
            .ring
            .as_ref()
            .map(|ring| ring.drain_up_to(MAX_BATCH_EVENTS))
            .unwrap_or_default();
        let events = captured
            .iter()
            .map(|event| {
                let rendered = render_process_activity(event, self.agent_id, &self.paths);
                self.unresolved
                    .note(&event.image_file_name, &rendered.process.image_file_name);
                rendered
            })
            .collect();
        self.sequence += 1;
        Batch {
            sequence: self.sequence,
            events,
        }
    }

    /// Seal and send one attempt at `batch` with `status`: a fresh
    /// envelope (`sent_at`, `uptime_seconds`), nonce and signature around
    /// the same events.
    async fn attempt(&self, batch: &Batch, status: HeartbeatStatus) -> Result<Attempt, AgentError> {
        let mut inner = build_envelope(
            &self.agent_block,
            batch.sequence,
            self.start_time,
            Utc::now(),
            status,
        );
        inner.events = batch.events.clone();
        let sent_at = inner.sent_at.clone();
        let outer = crate::signing::seal_envelope(inner, self.agent_id, self.keypair, &sent_at)?;
        let bytes = serde_json::to_vec(&outer)
            .map_err(|e| SigningError::Canonical(format!("serialize outer envelope: {e}")))?;

        let timeout = Duration::from_secs(self.heartbeat.request_timeout_seconds);
        let result =
            match tokio::time::timeout(timeout, self.sender.send(HEARTBEAT_PATH, &bytes)).await {
                Ok(result) => result,
                Err(_) => SendResult::Transient("request timed out".to_string()),
            };
        Ok(match result {
            SendResult::Status(s) if (200..300).contains(&s) => {
                tracing::info!(
                    sequence_number = batch.sequence,
                    status = ?status,
                    sent_at = %sent_at,
                    events_count = batch.events.len(),
                    body_size_bytes = bytes.len(),
                    response_status = s,
                    "signed heartbeat sent"
                );
                Attempt::Delivered
            }
            SendResult::Status(s) if (500..600).contains(&s) => {
                Attempt::Transient(format!("server status {s}"))
            }
            SendResult::Status(s) => Attempt::Rejected(s),
            SendResult::Transient(m) => Attempt::Transient(m),
            SendResult::ServerCertFatal(m) => return Err(TlsError::ServerCertUntrusted(m).into()),
            SendResult::ClientCertFatal(m) => return Err(TlsError::ClientCertRejected(m).into()),
        })
    }

    /// Drop a batch after its last allowed rejection: its events join
    /// the dropped total and one `error` line records it.
    fn drop_rejected(&self, batch: &Batch, status: u16) {
        let count = batch.events.len();
        if let Some(ring) = &self.ring {
            ring.add_dropped(count as u64);
        }
        tracing::error!(
            sequence_number = batch.sequence,
            response_status = status,
            events_dropped = count,
            attempts = self.heartbeat.max_retries,
            "batch dropped after repeated rejections"
        );
    }
}

/// SPEC-001 FR-007 backoff: × `backoff_factor`, capped at
/// `backoff_max_ms`.
fn next_backoff(current_ms: u64, heartbeat: &HeartbeatConfig) -> u64 {
    (((current_ms as f64) * heartbeat.backoff_factor) as u64).min(heartbeat.backoff_max_ms)
}
