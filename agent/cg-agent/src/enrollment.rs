//! Agent enrollment flow. See SPEC-002 §FR-003 to §FR-006, §Data
//! contracts, §Failure modes.

use crate::config::AgentConfig;
use crate::errors::EnrollmentError;
use serde::{Deserialize, Serialize};

/// Envelope version constant for enrollment request/response.
pub const ENROLLMENT_ENVELOPE_VERSION: &str = "0.1.0";

/// Enrollment request envelope (SPEC-002 §Data contracts).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnrollmentRequest {
    pub envelope_version: String,
    pub enrollment_token: String,
    /// base64url-unpadded 32-byte Ed25519 public key.
    pub agent_pubkey: String,
    pub agent_hostname: String,
    pub agent_platform: String,
    pub agent_version: String,
}

/// Enrollment response envelope (SPEC-002 §Data contracts).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnrollmentResponse {
    pub envelope_version: String,
    pub agent_id: String,
    pub client_certificate: String,
    pub issued_at: String,
    pub expires_at: String,
}

/// Outcome of a successful enrollment, ready to persist.
pub struct EnrolledIdentity {
    pub agent_id: String,
    pub client_certificate_pem: String,
    pub issued_at: String,
    pub expires_at: String,
    /// 32-byte Ed25519 secret seed, zeroed on drop.
    pub secret_seed: zeroize::Zeroizing<[u8; crate::crypto::KEY_LEN]>,
    pub public_key: [u8; crate::crypto::KEY_LEN],
}

/// Run the enrollment exchange against `{server.url}/v1/agents/enroll`:
/// generate a keypair, POST the request with the retry policy
/// (5xx / network retried; 4xx terminal), parse and validate the
/// response. Does NOT persist — that is `identity::persist`.
pub async fn enroll(_config: &AgentConfig) -> Result<EnrolledIdentity, EnrollmentError> {
    todo!("implemented in the enrollment commit")
}
