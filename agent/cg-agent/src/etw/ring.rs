//! Bounded ring buffer for captured events with FIFO-drop on overflow.
//!
//! Per SPEC-005 §Operational §2 + ADR-0009 §Decision part 3:
//! - The constructor `new(capacity)` accepts any non-zero capacity. The
//!   agent passes the NFR-005-002 production size of 65536. AC-008 uses
//!   `new_for_test(n)` (alias of `new`, kept as a named function so the
//!   call site documents the test-only intent).
//! - On `enqueue_or_drop` against a full ring, the oldest event is
//!   dropped (FIFO) and `events_dropped_total` is incremented.
//! - The dispatch callback path's only concern is "do nothing but
//!   enqueue" per ADR-0009; this implementation honors that — the lock
//!   acquisition is the only operation between event arrival and ring
//!   insertion in the nominal path, and nothing logs while the lock is
//!   held (SPEC-017 §Operational §6).
//!
//! Storage:
//! - `Mutex<VecDeque<(Instant, CapturedEvent)>>` for the events, each
//!   with the instant it was enqueued (the batching latency trigger of
//!   SPEC-017 §Operational §2 reads the oldest one).
//! - `AtomicU64` for the drop counter (monotonic; no lock acquisition
//!   needed for read, per AC-008 assertion iii).
//!
//! AC-006 (strict normative + log-and-drop): the empty-`image_file_name`
//! filter lives INSIDE `enqueue_or_drop` because the AC-006 test
//! exercises the contract by calling only `ring.enqueue_or_drop(...)`.
//! When the captured event would render to an empty `process.name`, the
//! method emits an error-level structured log identifying the dropped
//! event by PID and reason (`image_file_name_empty`) and returns without
//! enqueueing.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::types::CapturedEvent;

/// Bounded ring buffer for captured events.
pub struct EventRing {
    capacity: usize,
    events: Mutex<VecDeque<(Instant, CapturedEvent)>>,
    dropped_total: AtomicU64,
}

impl EventRing {
    /// Construct a ring with the specified capacity. The agent passes the
    /// NFR-005-002 size (65536).
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            events: Mutex::new(VecDeque::with_capacity(capacity)),
            dropped_total: AtomicU64::new(0),
        }
    }

    /// Test-only alias for `new(capacity)`. Kept as a named function so
    /// AC-008's call site documents the test-only bypass of the
    /// NFR-005-002 production size.
    pub fn new_for_test(capacity: usize) -> Self {
        Self::new(capacity)
    }

    /// AC-006 filter + FIFO-drop enqueue path.
    ///
    /// If `event.image_file_name` is empty (would render `process.name`
    /// empty in the envelope per ADR-0011 §5), emit an error-level log
    /// and return without enqueueing. Otherwise enqueue at the back; on
    /// overflow, drop the oldest event (FIFO) and increment
    /// `events_dropped_total`.
    pub fn enqueue_or_drop(&self, event: CapturedEvent) {
        if event.image_file_name.is_empty() {
            tracing::error!(
                pid = event.pid,
                reason = "image_file_name_empty",
                "captured event with empty process.name dropped"
            );
            return;
        }

        let now = Instant::now();
        let mut events = self.events.lock().expect("ring mutex poisoned");
        if events.len() >= self.capacity {
            events.pop_front(); // FIFO drop of oldest
            self.dropped_total.fetch_add(1, Ordering::Relaxed);
        }
        events.push_back((now, event));
    }

    /// Returns the monotonically-increasing count of events dropped by
    /// the ring across overflow cycles. Per AC-008 assertion iii: never
    /// resets within an agent lifetime.
    pub fn events_dropped_total(&self) -> u64 {
        self.dropped_total.load(Ordering::Relaxed)
    }

    /// Test-only accessor: returns a snapshot of currently retained
    /// events (cloned). Used by AC-008 to verify FIFO drop semantics.
    /// Production code drains instead.
    pub fn snapshot_events(&self) -> Vec<CapturedEvent> {
        let events = self.events.lock().expect("ring mutex poisoned");
        events.iter().map(|(_, event)| event.clone()).collect()
    }

    /// Remove and return all currently retained events in FIFO order.
    /// The drop counter (events_dropped_total) is monotonic and is NOT
    /// reset by drain.
    pub fn drain_events(&self) -> Vec<CapturedEvent> {
        self.drain_up_to(usize::MAX)
    }

    /// Remove and return up to `max` of the oldest retained events, in
    /// FIFO order. Logs after the lock is released.
    pub fn drain_up_to(&self, max: usize) -> Vec<CapturedEvent> {
        let (drained, remaining) = {
            let mut events = self.events.lock().expect("ring mutex poisoned");
            let n = max.min(events.len());
            let drained: Vec<CapturedEvent> = events.drain(..n).map(|(_, event)| event).collect();
            (drained, events.len())
        };
        if !drained.is_empty() {
            tracing::debug!(
                target: "cg_agent::etw::ring",
                drained_count = drained.len(),
                remaining,
                dropped_total = self.events_dropped_total(),
                "ring drained",
            );
        }
        drained
    }

    /// The instant the oldest retained event was enqueued, if any.
    pub fn oldest_enqueued_at(&self) -> Option<Instant> {
        let events = self.events.lock().expect("ring mutex poisoned");
        events.front().map(|(at, _)| *at)
    }

    /// Current number of events retained in the ring (not the capacity).
    /// Used by AC-006 to verify the log-and-drop path did NOT enqueue.
    pub fn len(&self) -> usize {
        let events = self.events.lock().expect("ring mutex poisoned");
        events.len()
    }

    /// Returns true when the ring has zero events retained.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The throttled overflow warning of SPEC-017 §Operational §6 (SPEC-005
/// §Failure modes): when the dropped total has grown, one `warn` line
/// with the total, at most once per `INTERVAL`. Checked by the delivery
/// loop, never by the dispatch callback.
#[derive(Debug, Default)]
pub struct OverflowWarning {
    reported_total: u64,
    last_warn: Option<Instant>,
}

impl OverflowWarning {
    /// At most one overflow warning per this interval.
    pub const INTERVAL: Duration = Duration::from_secs(60);

    pub fn new() -> Self {
        Self::default()
    }

    /// Warn if `ring`'s dropped total grew since the last warning and the
    /// last warning is at least `INTERVAL` before `now`. Returns whether
    /// it warned.
    pub fn check(&mut self, ring: &EventRing, now: Instant) -> bool {
        let total = ring.events_dropped_total();
        if total <= self.reported_total {
            return false;
        }
        if let Some(last) = self.last_warn {
            if now.saturating_duration_since(last) < Self::INTERVAL {
                return false;
            }
        }
        tracing::warn!(
            target: "cg_agent::etw::ring",
            events_dropped_total = total,
            delta_since_last_log = total - self.reported_total,
            ring_size_current = ring.len(),
            "captured events dropped",
        );
        self.reported_total = total;
        self.last_warn = Some(now);
        true
    }
}
