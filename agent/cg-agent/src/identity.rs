//! Load-or-enroll dispatcher and identity persistence.
//! See SPEC-002 §FR-001, §FR-007, §FR-008, §FR-014, §Behavior.

use crate::config::AgentConfig;
use crate::errors::EnrollmentError;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The on-disk `identity.json` (SPEC-002 §Data contracts).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedIdentity {
    pub agent_id: String,
    pub agent_pubkey_fingerprint: String,
    pub issued_at: String,
    pub expires_at: String,
}

/// A loaded, ready-to-use agent identity (in memory).
#[derive(Debug)]
pub struct Identity {
    pub agent_id: String,
    pub keypair: crate::crypto::AgentKeypair,
    pub client_certificate_pem: String,
}

/// Startup dispatcher (SPEC-002 §Behavior). The single entry point the
/// agent (and the integration harness) calls after `LoadConfig` +
/// `InitLogger`:
///
/// - `CheckIdentity` (FR-001): if both `cert_path` and `key_path` exist,
///   `LoadIdentity` (FR-008) and return the loaded `Identity`.
/// - otherwise `Enrolling`: `enroll` (FR-003–FR-006) → `PersistIdentity`
///   (FR-007) → token hygiene (FR-014), returning the fresh `Identity`.
///
/// `config_path` is the path to `agent.toml`, needed for FR-014 token
/// hygiene after a successful first run. Returns the resolved identity
/// or an `EnrollmentError` whose `exit_code()` drives the process exit.
pub async fn ensure_identity(
    _config: &AgentConfig,
    _config_path: &Path,
) -> Result<Identity, EnrollmentError> {
    todo!("implemented in the enrollment commit")
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
