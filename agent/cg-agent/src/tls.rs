//! TLS 1.3 mutual-auth client transport (SPEC-003 §FR-003–006, §NFR-004).
//!
//! Builds a rustls `ClientConfig` pinned to TLS 1.3 and the ADR-0004
//! cipher suites (`TLS_AES_256_GCM_SHA384`, `TLS_CHACHA20_POLY1305_SHA256`),
//! with a root store loaded from `server.trust_anchor_path` and client
//! authentication backed by the SPEC-002 identity (cert + Ed25519 key).
//!
//! The transport speaks HTTP/1.1 directly over a `tokio-rustls` stream
//! rather than through reqwest. That is deliberate: a fatal handshake
//! error must be classified as a server-cert failure (exit 6) vs. a
//! server-side client-cert rejection (exit 7) vs. a transient failure,
//! and only the raw `rustls::Error` carries that distinction cleanly.
//! The `ring` crypto provider matches the rest of the tree.

use crate::errors::TlsError;
use crate::identity::Identity;
use ed25519_dalek::pkcs8::EncodePrivateKey as _;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

/// Crypto provider restricted to the two ADR-0004 TLS 1.3 cipher suites.
/// `TLS_AES_128_GCM_SHA256` is excluded (SPEC-003 §FR-006).
fn restricted_provider() -> rustls::crypto::CryptoProvider {
    let base = rustls::crypto::ring::default_provider();
    let cipher_suites = base
        .cipher_suites
        .iter()
        .copied()
        .filter(|cs| {
            matches!(
                cs.suite(),
                rustls::CipherSuite::TLS13_AES_256_GCM_SHA384
                    | rustls::CipherSuite::TLS13_CHACHA20_POLY1305_SHA256
            )
        })
        .collect();
    rustls::crypto::CryptoProvider {
        cipher_suites,
        ..base
    }
}

/// Build the rustls `ClientConfig` for the secure heartbeat path.
/// `trust_anchor_pem` is the PEM contents of `server.trust_anchor_path`;
/// `identity` supplies the client certificate chain and signing key.
pub fn build_client_config(
    trust_anchor_pem: &[u8],
    identity: &Identity,
) -> Result<rustls::ClientConfig, TlsError> {
    let mut roots = rustls::RootCertStore::empty();
    let mut reader = std::io::BufReader::new(trust_anchor_pem);
    for entry in rustls_pemfile::certs(&mut reader) {
        let cert = entry.map_err(|e| TlsError::ClientConfig(format!("trust anchor PEM: {e}")))?;
        roots
            .add(cert)
            .map_err(|e| TlsError::ClientConfig(format!("add trust anchor: {e}")))?;
    }
    if roots.is_empty() {
        return Err(TlsError::ClientConfig(
            "trust anchor contains no certificates".to_string(),
        ));
    }

    let mut cert_reader = std::io::BufReader::new(identity.client_certificate_pem.as_bytes());
    let client_chain: Vec<rustls::pki_types::CertificateDer<'static>> =
        rustls_pemfile::certs(&mut cert_reader)
            .collect::<Result<_, _>>()
            .map_err(|e| TlsError::ClientConfig(format!("client cert PEM: {e}")))?;
    if client_chain.is_empty() {
        return Err(TlsError::ClientConfig(
            "client certificate PEM is empty".to_string(),
        ));
    }

    let pkcs8 = identity
        .keypair
        .signing_key()
        .to_pkcs8_der()
        .map_err(|e| TlsError::ClientConfig(format!("client key to pkcs8: {e}")))?;
    let key_der = rustls::pki_types::PrivateKeyDer::try_from(pkcs8.as_bytes().to_vec())
        .map_err(|e| TlsError::ClientConfig(format!("client key der: {e}")))?;

    rustls::ClientConfig::builder_with_provider(Arc::new(restricted_provider()))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| TlsError::ClientConfig(format!("tls versions: {e}")))?
        .with_root_certificates(roots)
        .with_client_auth_cert(client_chain, key_der)
        .map_err(|e| TlsError::ClientConfig(format!("client auth: {e}")))
}

/// Outcome of one secure send attempt.
pub enum SendResult {
    /// Got an HTTP response with this status (2xx accepted, else rejected).
    Status(u16),
    /// Fatal: the server certificate did not validate (exit 6).
    ServerCertFatal(String),
    /// Fatal: the server rejected our client certificate (exit 7).
    ClientCertFatal(String),
    /// Transient failure — retry per the backoff policy.
    Transient(String),
}

/// Sends one outer envelope over TLS 1.3 mTLS to a fixed host/port.
pub struct SecureSender {
    connector: TlsConnector,
    server_name: rustls::pki_types::ServerName<'static>,
    host: String,
    port: u16,
    timeout: Duration,
}

impl SecureSender {
    /// `server_url` must be `https://host[:port]`. The host is used for
    /// both the TCP connection and TLS server-name verification.
    pub fn new(
        config: rustls::ClientConfig,
        server_url: &str,
        timeout: Duration,
    ) -> Result<Self, TlsError> {
        let after = server_url.strip_prefix("https://").ok_or_else(|| {
            TlsError::ClientConfig(format!(
                "secure path requires an https:// URL, got {server_url}"
            ))
        })?;
        let authority = match after.find('/') {
            Some(i) => &after[..i],
            None => after,
        };
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse::<u16>().unwrap_or(443)),
            None => (authority.to_string(), 443),
        };
        let server_name = rustls::pki_types::ServerName::try_from(host.clone())
            .map_err(|e| TlsError::ClientConfig(format!("invalid server name {host}: {e}")))?;
        Ok(Self {
            connector: TlsConnector::from(Arc::new(config)),
            server_name,
            host,
            port,
            timeout,
        })
    }

    pub async fn send(&self, path: &str, body: &[u8]) -> SendResult {
        let tcp = match tokio::time::timeout(
            self.timeout,
            TcpStream::connect((self.host.as_str(), self.port)),
        )
        .await
        {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => return SendResult::Transient(format!("connect: {e}")),
            Err(_) => return SendResult::Transient("connect timed out".to_string()),
        };

        let tls = match self.connector.connect(self.server_name.clone(), tcp).await {
            Ok(s) => s,
            Err(e) => return classify_handshake_error(&e),
        };

        // The server cert was validated during the handshake above, so any
        // failure now is about *our* identity. In TLS 1.3 the client
        // finishes its half of the handshake before the server validates
        // the client certificate, so a client-cert rejection surfaces here
        // — as a cert alert, or (less cleanly) as the server tearing down
        // the connection with no HTTP response. Both mean the same thing.
        match self.exchange(tls, path, body).await {
            Ok(status) => SendResult::Status(status),
            Err(ExchangeFail::Refused(m)) => SendResult::ClientCertFatal(m),
            Err(ExchangeFail::Transient(m)) => SendResult::Transient(m),
        }
    }

    async fn exchange(
        &self,
        mut tls: tokio_rustls::client::TlsStream<TcpStream>,
        path: &str,
        body: &[u8],
    ) -> Result<u16, ExchangeFail> {
        let request = format!(
            "POST {path} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.host,
            body.len()
        );
        for chunk in [request.as_bytes(), body] {
            tls.write_all(chunk)
                .await
                .map_err(|e| ExchangeFail::Transient(format!("write: {e}")))?;
        }
        tls.flush()
            .await
            .map_err(|e| ExchangeFail::Transient(format!("flush: {e}")))?;

        let mut buf = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            match tls.read(&mut chunk).await {
                Ok(0) => break,
                Ok(n) => {
                    buf.extend_from_slice(&chunk[..n]);
                    if buf.windows(2).any(|w| w == b"\r\n") {
                        break;
                    }
                }
                // Nothing received yet and the connection failed: the
                // server refused the authenticated session (client-cert
                // rejection). Bytes already in flight => treat as transient.
                Err(e) if buf.is_empty() => {
                    return Err(ExchangeFail::Refused(format!(
                        "server refused the authenticated session: {e}"
                    )));
                }
                Err(e) => return Err(ExchangeFail::Transient(format!("read: {e}"))),
            }
        }

        if buf.is_empty() {
            return Err(ExchangeFail::Refused(
                "server closed the authenticated connection without an HTTP response".to_string(),
            ));
        }
        match parse_status(&buf) {
            Some(status) => Ok(status),
            None => Err(ExchangeFail::Transient(
                "incomplete HTTP response".to_string(),
            )),
        }
    }
}

/// Why a post-handshake HTTP exchange failed.
enum ExchangeFail {
    /// The server refused the mutually-authenticated session (client-cert
    /// rejection) — fatal, exit 7.
    Refused(String),
    /// A mid-exchange I/O failure — transient, retry.
    Transient(String),
}

/// Classify a `tokio-rustls` handshake `io::Error` (from `connect`) into
/// the SPEC-003 failure buckets. Only the underlying `rustls::Error`
/// distinguishes a server-cert validation failure (we reject the server)
/// from a server-sent client-cert rejection alert; a protocol-version or
/// plain I/O failure is transient.
fn classify_handshake_error(e: &std::io::Error) -> SendResult {
    let Some(rustls_err) = e
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<rustls::Error>())
    else {
        return SendResult::Transient(e.to_string());
    };

    use rustls::AlertDescription as Ad;
    match rustls_err {
        rustls::Error::InvalidCertificate(_) => SendResult::ServerCertFatal(rustls_err.to_string()),
        rustls::Error::AlertReceived(
            Ad::AccessDenied
            | Ad::BadCertificate
            | Ad::CertificateExpired
            | Ad::CertificateRevoked
            | Ad::CertificateUnknown
            | Ad::UnknownCA
            | Ad::CertificateRequired
            | Ad::UnsupportedCertificate,
        ) => SendResult::ClientCertFatal(rustls_err.to_string()),
        other => SendResult::Transient(other.to_string()),
    }
}

fn parse_status(buf: &[u8]) -> Option<u16> {
    let text = String::from_utf8_lossy(buf);
    let line = text.lines().next()?;
    let mut parts = line.split_whitespace();
    let _http = parts.next()?;
    parts.next()?.parse().ok()
}
