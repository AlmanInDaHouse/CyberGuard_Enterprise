//! Agent enrollment flow. See SPEC-002 §FR-003 to §FR-006, §Data
//! contracts, §Failure modes.

use crate::config::{AgentConfig, EnrollmentConfig};
use crate::errors::EnrollmentError;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;

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
/// response. Does NOT persist — that is `identity::persist_identity`.
pub async fn enroll(config: &AgentConfig) -> Result<EnrolledIdentity, EnrollmentError> {
    let enr = config
        .enrollment
        .as_ref()
        .ok_or(EnrollmentError::MissingToken)?;
    let token = match enr.token.as_deref() {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => return Err(EnrollmentError::MissingToken),
    };

    // FR-003: fresh Ed25519 keypair from OS entropy.
    let keypair = crate::crypto::AgentKeypair::generate()
        .map_err(|e| EnrollmentError::Persistence(format!("key generation failed: {e}")))?;
    let public_key = keypair.public_key_bytes();
    let fingerprint = crate::crypto::pubkey_fingerprint(&public_key);

    tracing::info!(
        agent.hostname = %config.agent.hostname,
        server.url = %config.server.url,
        enrollment.timeout_seconds = enr.timeout_seconds,
        "enrollment starting"
    );
    // The fingerprint is safe to log; the private key never is (NFR-002).
    tracing::info!(agent_pubkey_fingerprint = %fingerprint, "keypair generated");

    // FR-004: build the request envelope (raw pubkey, base64url-unpadded).
    let request = EnrollmentRequest {
        envelope_version: ENROLLMENT_ENVELOPE_VERSION.to_string(),
        enrollment_token: token,
        agent_pubkey: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(public_key),
        agent_hostname: config.agent.hostname.clone(),
        agent_platform: crate::detect_platform().to_string(),
        agent_version: env!("CARGO_PKG_VERSION").to_string(),
    };

    // FR-005: POST as JSON with the enrollment request timeout.
    let url = format!(
        "{}/v1/agents/enroll",
        config.server.url.trim_end_matches('/')
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(enr.timeout_seconds))
        .build()
        .map_err(|e| EnrollmentError::Persistence(format!("http client build failed: {e}")))?;

    let response = post_with_retry(&client, &url, &request, enr).await?;

    Ok(EnrolledIdentity {
        agent_id: response.agent_id,
        client_certificate_pem: response.client_certificate,
        issued_at: response.issued_at,
        expires_at: response.expires_at,
        secret_seed: keypair.secret_seed(),
        public_key,
    })
}

/// POST the enrollment request with the SPEC-002 retry policy: 5xx and
/// network errors are retried with exponential backoff up to
/// `max_retries` total attempts; 4xx responses are terminal (FR-009 to
/// FR-011). Returns the validated `EnrollmentResponse` on `200 OK`.
async fn post_with_retry(
    client: &reqwest::Client,
    url: &str,
    request: &EnrollmentRequest,
    enr: &EnrollmentConfig,
) -> Result<EnrollmentResponse, EnrollmentError> {
    let max = enr.max_retries.max(1);
    let mut backoff_ms = enr.backoff_initial_ms;
    let mut last_error = String::from("no attempts made");
    let mut attempt: u32 = 0;

    while attempt < max {
        attempt += 1;
        tracing::info!(attempt, server.url = %url, "enrollment request sent");

        match client.post(url).json(request).send().await {
            Ok(resp) => {
                let status = resp.status();
                let code = status.as_u16();
                if status.is_success() {
                    let body = resp.text().await.map_err(|e| {
                        EnrollmentError::Refused(format!("malformed server response: {e}"))
                    })?;
                    return parse_and_validate(&body);
                }
                match code {
                    401 | 403 => {
                        tracing::warn!(response_status = code, "enrollment failed: token rejected");
                        return Err(EnrollmentError::Refused(
                            "token rejected by server".to_string(),
                        ));
                    }
                    409 => {
                        tracing::warn!(
                            response_status = code,
                            "enrollment failed: token already used"
                        );
                        return Err(EnrollmentError::Refused("token already used".to_string()));
                    }
                    400..=499 => {
                        // Any other 4xx is terminal too (no retry helps).
                        return Err(EnrollmentError::Refused(format!(
                            "malformed server response (unexpected status {code})"
                        )));
                    }
                    _ => {
                        // 5xx — retryable.
                        last_error = format!("status {code}");
                    }
                }
            }
            Err(e) => {
                // Network-layer failure — retryable.
                last_error = e.to_string();
            }
        }

        if attempt >= max {
            break;
        }
        tracing::warn!(attempt, backoff_ms, error = %last_error, "enrollment retry");
        sleep(Duration::from_millis(backoff_ms)).await;
        backoff_ms = (backoff_ms as f64 * enr.backoff_factor) as u64;
    }

    tracing::warn!(attempts = attempt, last_error = %last_error, "enrollment failed: server unreachable");
    Err(EnrollmentError::Unreachable {
        attempts: attempt,
        last_error,
    })
}

/// Parse the `200 OK` body and validate it (FR-006): `agent_id` must be a
/// UUIDv7 and `client_certificate` must parse as PEM with a non-empty DER
/// body. Any mismatch is a terminal "malformed server response".
fn parse_and_validate(body: &str) -> Result<EnrollmentResponse, EnrollmentError> {
    let resp: EnrollmentResponse = serde_json::from_str(body)
        .map_err(|e| EnrollmentError::Refused(format!("malformed server response: {e}")))?;

    if !crate::config::is_uuidv7(&resp.agent_id) {
        return Err(EnrollmentError::Refused(
            "malformed server response (agent_id is not a UUIDv7)".to_string(),
        ));
    }

    let parsed = pem::parse(resp.client_certificate.as_bytes()).map_err(|e| {
        EnrollmentError::Refused(format!(
            "malformed server response (bad certificate PEM: {e})"
        ))
    })?;
    if parsed.contents().is_empty() {
        return Err(EnrollmentError::Refused(
            "malformed server response (empty certificate)".to_string(),
        ));
    }

    tracing::info!(
        agent_id = %resp.agent_id,
        expires_at = %resp.expires_at,
        "enrollment response received"
    );
    Ok(resp)
}
