//! CyberGuard agent binary — SPEC-001 (heartbeat).
//!
//! Scaffold only in this commit. The behaviour described by SPEC-001
//! (config loading, heartbeat loop, graceful shutdown) lands in a
//! subsequent commit alongside the integration harness that
//! validates each acceptance criterion.

fn main() {
    println!(
        "cg-agent v{} — scaffold; SPEC-001 behaviour lands in a follow-up commit.",
        env!("CARGO_PKG_VERSION")
    );
}
