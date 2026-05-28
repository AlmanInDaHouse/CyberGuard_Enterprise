//! ETW capture path — Windows Kernel-Process events.
//!
//! Submodule organization (Phase 3.5 complete after β3):
//! - `types`: in-memory captured-event shape + activity discriminants +
//!   raw open-error variants.
//! - `ring`: bounded ring buffer with FIFO-drop + monotonic drop counter.
//! - `uid`: `process.uid` recipe formatter per ADR-0011 §6.
//! - `cache`: `CreatedTimeCache` for Terminate retention.
//! - `session` (Windows-only): ETW session + dispatch callback.
//! - `session_stub` (non-Windows): debug-build stub returning Err.
//! - `events_lost_impl`: side-channel events_lost helper.
//!
//! Module-level public surface is the union of submodule re-exports below.

mod cache;
mod events_lost_impl;
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

pub use cache::CreatedTimeCache;
pub use events_lost_impl::events_lost;
pub use events_lost_impl::reclaim_zombie;
pub use ring::EventRing;
pub use types::{ActivityId, CapturedEvent, OpenError};
pub use uid::format_process_uid;
