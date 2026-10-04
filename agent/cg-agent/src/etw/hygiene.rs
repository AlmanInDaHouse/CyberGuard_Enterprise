//! Capture hygiene every 60 s (SPEC-017 §Operational §6): the cache
//! sweep of SPEC-005 NFR-005-006 and the `events_lost` poll of SPEC-005
//! §Failure modes.
//!
//! `EventsLostMonitor` is the platform-independent decision; on Windows
//! the session's dedicated hygiene thread (`HygieneThread`) runs the
//! sweep, with `process_is_alive` as its liveness probe, and the poll.

use std::time::Duration;

/// How often the hygiene work runs (SPEC-005 NFR-005-006).
pub const HYGIENE_INTERVAL: Duration = Duration::from_secs(60);

/// What one `events_lost` poll showed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LostObservation {
    /// No change since the previous poll (or the first poll, at zero).
    Unchanged,
    /// The kernel counter grew (logged at `warn`).
    Increased { total: u32, delta: u32 },
    /// The counter went backwards: never expected (logged at `error`).
    Decreased { current: u32, previous: u32 },
}

/// Tracks the session's `events_lost` counter across polls.
#[derive(Debug, Default)]
pub struct EventsLostMonitor {
    previous: u32,
}

impl EventsLostMonitor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one poll of the counter and log a change: an increase at
    /// `warn` with the new total, a decrease at `error` (it would falsify
    /// ADR-0008's spike findings).
    pub fn observe(&mut self, current: u32, session_name: &str) -> LostObservation {
        let previous = self.previous;
        self.previous = current;
        if current > previous {
            let delta = current - previous;
            tracing::warn!(
                target: "cg_agent::etw",
                events_lost = current,
                delta_since_last_poll = delta,
                session_name,
                "ETW reported lost events",
            );
            LostObservation::Increased {
                total: current,
                delta,
            }
        } else if current < previous {
            tracing::error!(
                target: "cg_agent::etw",
                events_lost_current = current,
                events_lost_previous = previous,
                session_name,
                "ETW events_lost decreased between polls",
            );
            LostObservation::Decreased { current, previous }
        } else {
            LostObservation::Unchanged
        }
    }
}

/// Whether a process with this PID exists. `OpenProcess` fails with
/// `ERROR_INVALID_PARAMETER` only when no such process exists; any other
/// failure (e.g. access denied to a protected process) means it does.
#[cfg(windows)]
pub fn process_is_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_INVALID_PARAMETER};
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    // SAFETY: OpenProcess takes no pointers; a non-null handle is ours to
    // close.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if !handle.is_null() {
        // SAFETY: `handle` was just returned by OpenProcess.
        unsafe { CloseHandle(handle) };
        return true;
    }
    // SAFETY: reads the calling thread's last-error value.
    unsafe { GetLastError() != ERROR_INVALID_PARAMETER }
}

/// The session's hygiene thread: every `HYGIENE_INTERVAL` it sweeps the
/// cache and polls `events_lost`, until stopped.
#[cfg(windows)]
pub(super) struct HygieneThread {
    stop_tx: std::sync::mpsc::Sender<()>,
    join: std::thread::JoinHandle<()>,
}

#[cfg(windows)]
impl HygieneThread {
    pub(super) fn spawn(
        cache: std::sync::Arc<super::cache::CreatedTimeCache>,
        session_name: &'static str,
    ) -> std::io::Result<Self> {
        use std::sync::mpsc::RecvTimeoutError;

        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
        let join = std::thread::Builder::new()
            .name("cg-etw-hygiene".to_string())
            .spawn(move || {
                let mut lost = EventsLostMonitor::new();
                loop {
                    match stop_rx.recv_timeout(HYGIENE_INTERVAL) {
                        Err(RecvTimeoutError::Timeout) => {}
                        // Stopped, or the session was dropped.
                        _ => return,
                    }
                    let started = std::time::Instant::now();
                    let swept = cache.len();
                    let evicted = cache.sweep(process_is_alive);
                    tracing::debug!(
                        target: "cg_agent::etw",
                        entries_swept = swept,
                        entries_evicted = evicted,
                        duration_ms = started.elapsed().as_millis() as u64,
                        "cache sweep complete",
                    );
                    match super::events_lost(session_name) {
                        Ok(current) => {
                            lost.observe(current, session_name);
                        }
                        Err(rc) => tracing::warn!(
                            target: "cg_agent::etw",
                            session_name,
                            win32_status = rc,
                            "events_lost query failed",
                        ),
                    }
                }
            })?;
        Ok(Self { stop_tx, join })
    }

    /// Wake the thread to stop, and wait for it.
    pub(super) fn stop(self) {
        let _ = self.stop_tx.send(());
        let _ = self.join.join();
    }
}
