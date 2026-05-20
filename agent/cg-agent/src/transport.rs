//! HTTP transport for the heartbeat. See SPEC-001 §FR-006 / §FR-007.

use crate::config::HeartbeatConfig;
use crate::envelope::HeartbeatEnvelope;
use crate::errors::TransportError;

/// HTTP client wrapper that POSTs heartbeat envelopes with retry +
/// exponential backoff per SPEC-001 §FR-007.
pub struct HeartbeatClient {
    _placeholder: (),
}

impl HeartbeatClient {
    pub fn new(_server_url: String, _cfg: HeartbeatConfig) -> Self {
        todo!("implemented in the heartbeat commit")
    }

    /// Send one heartbeat envelope. Applies the configured retry policy.
    /// Returns `Ok(())` on the first successful 2xx response; returns
    /// `TransportError::RetryExhausted` after exhausting `max_retries`.
    pub async fn send(&self, _envelope: &HeartbeatEnvelope) -> Result<(), TransportError> {
        todo!("implemented in the heartbeat commit")
    }
}
