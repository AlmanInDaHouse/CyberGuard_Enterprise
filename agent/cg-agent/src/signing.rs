//! Outer signed envelope (SPEC-003 §Data contracts). Wraps the SPEC-001
//! heartbeat envelope verbatim as `body`, adds a per-message nonce and
//! send timestamp, and an Ed25519 signature over the canonical
//! envelope-minus-signature (ADR-0004 §Message integrity).

use crate::crypto::AgentKeypair;
use crate::envelope::HeartbeatEnvelope;
use crate::errors::SigningError;
use base64::Engine as _;
use ed25519_dalek::Signer as _;
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

fn b64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Build and sign an outer envelope around `body`: generate a fresh
/// 16-byte `OsRng` nonce, set `sent_at`, canonicalize the
/// envelope-minus-signature (JCS), and sign it with the agent key.
/// `sequence_number` mirrors `body.sequence_number`.
///
/// The signed region is built by serializing the assembled envelope to a
/// JSON value, removing the (placeholder) `signature` field, and running
/// JCS over the remainder. The server verifies by recomputing the same
/// canonicalization over the received envelope minus its signature, so
/// the two byte strings are identical by construction.
pub fn seal_envelope(
    body: HeartbeatEnvelope,
    agent_id: &str,
    keypair: &AgentKeypair,
    sent_at: &str,
) -> Result<OuterEnvelope, SigningError> {
    let mut nonce_bytes = [0u8; NONCE_LEN];
    getrandom::getrandom(&mut nonce_bytes)
        .map_err(|e| SigningError::Sign(format!("nonce generation failed: {e}")))?;

    let mut envelope = OuterEnvelope {
        outer_envelope_version: OUTER_ENVELOPE_VERSION.to_string(),
        agent_id: agent_id.to_string(),
        sequence_number: body.sequence_number,
        nonce: b64url(&nonce_bytes),
        sent_at: sent_at.to_string(),
        body,
        signature: String::new(),
    };

    let mut value = serde_json::to_value(&envelope)
        .map_err(|e| SigningError::Canonical(format!("serialize envelope: {e}")))?;
    if let Some(obj) = value.as_object_mut() {
        obj.remove("signature");
    }
    let canonical = crate::canonical::canonical_bytes(&value)?;

    let signature = keypair.signing_key().sign(&canonical);
    envelope.signature = b64url(&signature.to_bytes());
    Ok(envelope)
}
