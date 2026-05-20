//! HTTP transport for the heartbeat. See SPEC-001 §FR-006 / §FR-007.

use crate::config::HeartbeatConfig;
use crate::envelope::HeartbeatEnvelope;
use crate::errors::TransportError;
use reqwest::Client;
use std::time::Duration;
use tokio::time::sleep;

/// HTTP client wrapper that POSTs heartbeat envelopes with retry +
/// exponential backoff per SPEC-001 §FR-007.
pub struct HeartbeatClient {
    client: Client,
    url: String,
    config: HeartbeatConfig,
}

impl HeartbeatClient {
    pub fn new(server_url: String, cfg: HeartbeatConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(cfg.request_timeout_seconds))
            .build()
            .expect("reqwest client should build with valid timeout");
        let url = format!("{}/v1/agents/heartbeat", server_url.trim_end_matches('/'));
        Self {
            client,
            url,
            config: cfg,
        }
    }

    /// Send one heartbeat envelope with the configured retry policy
    /// (FR-007). Returns `Ok(())` on the first 2xx response; returns
    /// `TransportError::RetryExhausted` after `max_retries` attempts.
    pub async fn send(&self, envelope: &HeartbeatEnvelope) -> Result<(), TransportError> {
        let mut attempt: u32 = 0;
        let mut backoff_ms = self.config.backoff_initial_ms;
        let mut last_error = String::from("no attempts made");

        while attempt < self.config.max_retries {
            attempt += 1;
            match self.client.post(&self.url).json(envelope).send().await {
                Ok(resp) if resp.status().is_success() => return Ok(()),
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    last_error = format!("status {status}");
                    tracing::warn!(
                        sequence_number = envelope.sequence_number,
                        attempt = attempt,
                        backoff_ms = backoff_ms,
                        error = %last_error,
                        "heartbeat retry"
                    );
                }
                Err(e) => {
                    last_error = e.to_string();
                    tracing::warn!(
                        sequence_number = envelope.sequence_number,
                        attempt = attempt,
                        backoff_ms = backoff_ms,
                        error = %last_error,
                        "heartbeat retry"
                    );
                }
            }
            if attempt >= self.config.max_retries {
                break;
            }
            sleep(Duration::from_millis(backoff_ms)).await;
            let next = (backoff_ms as f64 * self.config.backoff_factor) as u64;
            backoff_ms = next.min(self.config.backoff_max_ms);
        }

        Err(TransportError::RetryExhausted {
            attempts: attempt,
            last_error,
        })
    }

    /// Single attempt without retry — used for the final
    /// "going_offline" heartbeat at graceful shutdown (FR-008).
    pub async fn send_single_attempt(
        &self,
        envelope: &HeartbeatEnvelope,
    ) -> Result<(), TransportError> {
        match self.client.post(&self.url).json(envelope).send().await {
            Ok(resp) if resp.status().is_success() => Ok(()),
            Ok(resp) => Err(TransportError::BadStatus {
                status: resp.status().as_u16(),
            }),
            Err(e) => Err(TransportError::Network(e.to_string())),
        }
    }
}
