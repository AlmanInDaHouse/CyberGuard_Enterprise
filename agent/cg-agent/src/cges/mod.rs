//! CGES (CyberGuard Event Stream) emission path.
//!
//! Renders `CapturedEvent` instances from the etw module into the CGES
//! wire shape per SPEC-005 §AC + ADR-0011 §3, and `NetworkEvent`
//! instances into the Network Activity shape of SPEC-019. The delivery loop
//! (`delivery.rs`) renders each event once, when its batch is formed,
//! and sends it inside the signed envelope's `body.events`.

mod emit;

pub use emit::{
    emit_process_activity, emit_process_activity_with_cache, render_authentication,
    render_network_activity, render_process_activity, CgesActor, CgesActorProcess,
    CgesAuthentication, CgesConnectionInfo, CgesEvent, CgesLogonSource, CgesLogonUser,
    CgesNetworkActivity, CgesNetworkEndpoint, CgesProcess, CgesProcessActivity,
};
