//! ETW capture path — Windows Kernel-Process events.
//!
//! Submodule organization (Phase 3.5):
//! - `types`: in-memory captured-event shape + activity discriminants +
//!   raw open-error variants + EtwSession type marker.
//! - `ring`: bounded ring buffer with FIFO-drop + monotonic drop counter.
//! - `uid`: `process.uid` recipe formatter per ADR-0011 §6.
//! - `cache`: `CreatedTimeCache` for Terminate retention (β2).
//! - `session`: Windows-only ETW session + dispatch callback (β3, forthcoming).
//!
//! Module-level public surface is the union of submodule re-exports below.
//! Phase 3.4 tests import via `cg_agent::etw::{ActivityId, CapturedEvent, ...}`;
//! this module honors that contract.

mod cache;
mod ring;
mod types;
mod uid;

pub use cache::CreatedTimeCache;
pub use ring::EventRing;
pub use types::{ActivityId, CapturedEvent, EtwSession, OpenError};
pub use uid::format_process_uid;
