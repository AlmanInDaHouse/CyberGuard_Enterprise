//! Outer signed envelope (SPEC-003 §Data contracts). Wraps the SPEC-001
//! heartbeat envelope verbatim as `body`, adds a per-message nonce and
//! send timestamp, and an Ed25519 signature over the canonical
//! envelope-minus-signature (ADR-0004 §Message integrity).

use crate::crypto::AgentKeypair;
use crate::envelope::HeartbeatEnvelope;
use crate::errors::SigningError;
use serde::{Deserialize, Serialize};

/// Outer envelope version constant (SPEC-003 §Data contracts).
pub const OUTER_ENVELOPE_VERSION: &str = "0.1.0";

/// Per-message nonce length in bytes — 16 random bytes from `OsRng`
/// (SPEC-003 §FR-008).
pub const NONCE_LEN: usize = 16;

/// The outer signed envelope as serialized on the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OuterEnvelope {
    pub outer_envelope_version: String,
    pub agent_id: String,
    pub sequence_number: u64,
    /// base64url-unpadded, 16 random bytes.
    pub nonce: String,
    pub sent_at: String,
    /// The SPEC-001 heartbeat envelope, verbatim.
    pub body: HeartbeatEnvelope,
    /// base64url-unpadded Ed25519 signature over the canonical
    /// envelope-minus-signature.
    pub signature: String,
}

/// Build and sign an outer envelope around `body`: generate a fresh
/// 16-byte `OsRng` nonce, set `sent_at`, canonicalize the
/// envelope-minus-signature (JCS), and sign it with the agent key.
/// `sequence_number` mirrors `body.sequence_number`.
pub fn seal_envelope(
    _body: HeartbeatEnvelope,
    _agent_id: &str,
    _keypair: &AgentKeypair,
    _sent_at: &str,
) -> Result<OuterEnvelope, SigningError> {
    todo!("implemented in the SPEC-003 implementation commit")
}
