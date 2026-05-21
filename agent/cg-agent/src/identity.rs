//! Load-or-enroll dispatcher and identity persistence.
//! See SPEC-002 §FR-001, §FR-007, §FR-008, §FR-014, §Behavior.

use crate::config::AgentConfig;
use crate::errors::EnrollmentError;
use serde::{Deserialize, Serialize};

/// The on-disk `identity.json` (SPEC-002 §Data contracts).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedIdentity {
    pub agent_id: String,
    pub agent_pubkey_fingerprint: String,
    pub issued_at: String,
    pub expires_at: String,
}

/// A loaded, ready-to-use agent identity (in memory).
pub struct Identity {
    pub agent_id: String,
    pub keypair: crate::crypto::AgentKeypair,
    pub client_certificate_pem: String,
}

/// CheckIdentity (FR-001): both cert and key files present and
/// readable ⇒ already enrolled.
pub fn is_enrolled(_config: &AgentConfig) -> bool {
    todo!("implemented in the enrollment commit")
}

/// LoadIdentity (FR-008): read cert + decrypt key + cross-check the
/// pubkey fingerprint against identity.json.
pub fn load_identity(_config: &AgentConfig) -> Result<Identity, EnrollmentError> {
    todo!("implemented in the enrollment commit")
}

/// PersistIdentity (FR-007): write cert.pem, key.dat (sealed),
/// identity.json with owner-only ACLs.
pub fn persist_identity(
    _config: &AgentConfig,
    _enrolled: &crate::enrollment::EnrolledIdentity,
) -> Result<(), EnrollmentError> {
    todo!("implemented in the enrollment commit")
}

/// Token hygiene (FR-014): atomically rewrite agent.toml dropping
/// `enrollment.token`. Best-effort; logs warn on failure.
pub fn hygienize_token(_config_path: &std::path::Path) {
    todo!("implemented in the enrollment commit")
}
