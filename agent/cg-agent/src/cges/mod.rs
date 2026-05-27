//! CGES (CyberGuard Event Stream) emission path.
//!
//! Renders `CapturedEvent` instances from the etw module into the CGES
//! wire shape per SPEC-005 §AC + ADR-0011 §3. The emitted shape is
//! consumed by the envelope-construction path (β3 forthcoming) and
//! ultimately POSTed via the existing transport layer.

mod emit;

pub use emit::{
    emit_process_activity, emit_process_activity_with_cache, CgesProcess, CgesProcessActivity,
};
