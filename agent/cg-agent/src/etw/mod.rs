//! ETW capture path — Windows Kernel-Process events.
//!
//! Submodule organization:
//! - `types`: in-memory captured-event shape + activity discriminants +
//!   the session-start error.
//! - `dispatch`: the dispatch-callback logic, platform-independent.
//! - `ring`: bounded ring buffer with FIFO-drop + monotonic drop counter,
//!   and the throttled overflow warning.
//! - `uid`: `process.uid` recipe formatter per ADR-0011 §6.
//! - `cache`: `CreatedTimeCache` for Terminate retention.
//! - `session` (Windows-only): ETW session + dispatch callback.
//! - `session_stub` (non-Windows): no capture backend.
//! - `events_lost_impl`: side-channel helpers (events_lost, stop by name).
//! - `hygiene`: the 60 s cache sweep and `events_lost` poll.
//!
//! Module-level public surface is the union of submodule re-exports below.

mod cache;
mod dispatch;
mod events_lost_impl;
mod hygiene;
mod ring;
mod types;
mod uid;

#[cfg(target_os = "windows")]
mod session;
#[cfg(not(target_os = "windows"))]
mod session_stub;

#[cfg(target_os = "windows")]
pub use session::EtwSession;
#[cfg(not(target_os = "windows"))]
pub use session_stub::EtwSession;

/// The agent's ETW session name. One constant (ADR-0008 §Compliance):
/// ferrisetw's `named(...)` and the side-channel helpers both use it.
pub const SESSION_NAME: &str = "CGAgent-KernelProcess";

pub use cache::CreatedTimeCache;
pub use dispatch::{dispatch_record, filetime_to_unix_nanos, RawProcessRecord};
pub use events_lost_impl::events_lost;
pub use events_lost_impl::reclaim_zombie;
pub use events_lost_impl::stop_session;
#[cfg(target_os = "windows")]
pub use hygiene::process_is_alive;
pub use hygiene::{EventsLostMonitor, LostObservation, HYGIENE_INTERVAL};
pub use ring::{EventRing, OverflowWarning};
pub use types::{win32_from_os_error, ActivityId, CapturedEvent, OpenError};
pub use uid::format_process_uid;
