//! ETW capture path — Windows Kernel-Process events.
//!
//! Submodule organization (Phase 3.5):
//! - `types`: in-memory captured-event shape + activity discriminants.
//! - `ring`: bounded ring buffer with FIFO-drop + monotonic drop counter.
//! - `uid`: `process.uid` recipe formatter per ADR-0011 §6.
//! - `cache`: `CreatedTimeCache` for Terminate retention (β2, forthcoming).
//! - `session`: Windows-only ETW session + dispatch callback (β3, forthcoming).
//!
//! Module-level public surface is the union of submodule re-exports below.
//! Phase 3.4 tests import via `cg_agent::etw::{ActivityId, CapturedEvent, ...}`;
//! this module honors that contract.

mod ring;
mod types;
mod uid;

pub use ring::EventRing;
pub use types::{ActivityId, CapturedEvent};
pub use uid::format_process_uid;
