//! ETW capture path — Windows Kernel-Process and Kernel-Network events.
//!
//! Submodule organization:
//! - `types`: in-memory captured-event shapes (process, network, and the
//!   ring's `RingEvent` of both) + activity discriminants + the
//!   session-start error.
//! - `dispatch`: the dispatch-callback logic, platform-independent.
//! - `network`: decoding of a Kernel-Network connection record (pure).
//! - `ring`: bounded ring buffer with FIFO-drop + monotonic drop counter,
//!   and the throttled overflow warning.
//! - `uid`: `process.uid` recipe formatter per ADR-0011 §6.
//! - `cache`: `CreatedTimeCache` for Terminate retention.
//! - `session` (Windows-only): ETW session + dispatch callbacks.
//! - `session_stub` (non-Windows): no capture backend.
//! - `events_lost_impl`: side-channel helpers (events_lost, stop by name).
//! - `hygiene`: the 60 s cache sweep, `events_lost` poll and discard
//!   check.
//!
//! Module-level public surface is the union of submodule re-exports below.

mod cache;
mod dispatch;
mod events_lost_impl;
mod hygiene;
mod network;
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
pub use dispatch::{
    dispatch_network_record, dispatch_record, filetime_to_unix_nanos, DiscardSample,
    NetworkDiscards, RawNetworkRecord, RawProcessRecord,
};
pub use events_lost_impl::events_lost;
pub use events_lost_impl::reclaim_zombie;
pub use events_lost_impl::stop_session;
#[cfg(target_os = "windows")]
pub use hygiene::process_is_alive;
pub use hygiene::{DiscardMonitor, EventsLostMonitor, LostObservation, HYGIENE_INTERVAL};
pub use network::{
    connection_direction, decode_connection, decode_pid, DecodedConnection, CONNECTION_EVENT_IDS,
    TCP_ACCEPT_V4, TCP_ACCEPT_V6, TCP_CONNECT_V4, TCP_CONNECT_V6,
};
pub use ring::{EventRing, OverflowWarning};
pub(crate) use types::win32_message;
pub use types::{
    win32_from_os_error, ActivityId, CapturedEvent, Direction, NetworkEvent, OpenError, RingEvent,
};
pub use uid::format_process_uid;
