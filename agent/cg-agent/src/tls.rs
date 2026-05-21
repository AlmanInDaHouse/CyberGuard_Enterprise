//! TLS 1.3 mutual-auth client transport (SPEC-003 §FR-003–006, §NFR-004).
//!
//! Builds a rustls `ClientConfig` pinned to TLS 1.3 and the ADR-0004
//! cipher suites (`TLS_AES_256_GCM_SHA384`, `TLS_CHACHA20_POLY1305_SHA256`),
//! with a root store loaded from `server.trust_anchor_path` and client
//! authentication backed by the SPEC-002 identity (cert + Ed25519 key).
//!
//! The `ring` crypto provider is used, matching the provider reqwest
//! already pulls. The resulting `ClientConfig` is handed to reqwest via
//! `use_preconfigured_tls`, so HTTP/JSON stays on reqwest while the TLS
//! parameters are fully controlled here.

use crate::errors::TlsError;
use crate::identity::Identity;

/// Build the rustls `ClientConfig` for the secure heartbeat path.
/// `trust_anchor_pem` is the PEM contents of `server.trust_anchor_path`;
/// `identity` supplies the client certificate chain and signing key.
pub fn build_client_config(
    _trust_anchor_pem: &[u8],
    _identity: &Identity,
) -> Result<rustls::ClientConfig, TlsError> {
    todo!("implemented in the SPEC-003 implementation commit")
}
