//! TLS 1.3 mutual-auth client transport (SPEC-003 §FR-003–006, §NFR-004).
//!
//! Builds a rustls `ClientConfig` pinned to TLS 1.3 and the ADR-0004
//! cipher suites (`TLS_AES_256_GCM_SHA384`, `TLS_CHACHA20_POLY1305_SHA256`),
//! with a root store loaded from `server.trust_anchor_path` and client
//! authentication backed by the SPEC-002 identity (cert + Ed25519 key).
//! Enrollment's client takes the same versions, suites and roots, without
//! a client certificate (SPEC-023 §Operational §6).
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
use rustls::pki_types::pem::PemObject as _;
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

/// The root store of the trust anchor: its certificates, at least one.
fn anchor_roots(trust_anchor_pem: &[u8]) -> Result<rustls::RootCertStore, TlsError> {
    let mut roots = rustls::RootCertStore::empty();
    for cert in certificates_from_pem(trust_anchor_pem, "trust anchor")? {
        roots
            .add(cert)
            .map_err(|e| TlsError::ClientConfig(format!("add trust anchor: {e}")))?;
    }
    if roots.is_empty() {
        return Err(TlsError::ClientConfig(
            "trust anchor contains no certificates".to_string(),
        ));
    }
    Ok(roots)
}

/// Build the rustls `ClientConfig` for enrollment (SPEC-023 §Operational
/// §6): TLS 1.3 with the ADR-0004 cipher suites, as the heartbeat client,
/// trusting the roots of the trust anchor alone, and presenting no client
/// certificate (the agent has none before it enrolls).
pub fn build_enrollment_config(trust_anchor_pem: &[u8]) -> Result<rustls::ClientConfig, TlsError> {
    let roots = anchor_roots(trust_anchor_pem)?;
    Ok(
        rustls::ClientConfig::builder_with_provider(Arc::new(restricted_provider()))
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(|e| TlsError::ClientConfig(format!("tls versions: {e}")))?
            .with_root_certificates(roots)
            .with_no_client_auth(),
    )
}

/// Build the rustls `ClientConfig` for the secure heartbeat path.
/// `trust_anchor_pem` is the PEM contents of `server.trust_anchor_path`;
/// `identity` supplies the client certificate chain and signing key.
pub fn build_client_config(
    trust_anchor_pem: &[u8],
    identity: &Identity,
) -> Result<rustls::ClientConfig, TlsError> {
    let roots = anchor_roots(trust_anchor_pem)?;

    let client_chain =
        certificates_from_pem(identity.client_certificate_pem.as_bytes(), "client cert")?;
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

/// The certificates of a PEM document, in their order; sections of any
/// other kind are skipped. `what` names the document in the error.
fn certificates_from_pem(
    pem: &[u8],
    what: &str,
) -> Result<Vec<rustls::pki_types::CertificateDer<'static>>, TlsError> {
    rustls::pki_types::CertificateDer::pem_slice_iter(pem)
        .collect::<Result<_, _>>()
        .map_err(|e| TlsError::ClientConfig(format!("{what} PEM: {}", pem_error_text(&e))))
}

/// A PEM error's text: a missing section end, an illegal section start and a
/// base64 failure as rustls-pemfile worded them, a marker as text rather than
/// the bytes the pki-types `Display` prints; any other kind as pki-types words
/// it (debt #111).
fn pem_error_text(e: &rustls::pki_types::pem::Error) -> String {
    use rustls::pki_types::pem::Error;
    match e {
        Error::MissingSectionEnd { end_marker } => {
            format!(
                "section end {:?} missing",
                String::from_utf8_lossy(end_marker)
            )
        }
        Error::IllegalSectionStart { line } => {
            format!("illegal section start: {:?}", String::from_utf8_lossy(line))
        }
        Error::Base64Decode(err) => err.clone(),
        other => other.to_string(),
    }
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

/// The reason a failed request did not trust the server's certificate, when
/// that is why it failed: a `rustls::Error::InvalidCertificate` in the
/// error's source chain, directly or inside `io::Error`s, however nested
/// (reqwest's connector wraps tokio-rustls's `io::Error` in another, whose
/// `source` skips both): an untrusted chain, an expired certificate, a name
/// mismatch. `None` for any other failure (SPEC-023 §Operational §6).
pub fn untrusted_server_cert(err: &(dyn std::error::Error + 'static)) -> Option<String> {
    let mut current = Some(err);
    while let Some(e) = current {
        if let Some(r @ rustls::Error::InvalidCertificate(_)) = rustls_error_in(e) {
            return Some(r.to_string());
        }
        current = e.source();
    }
    None
}

/// The `rustls::Error` that `e` is, or that the `io::Error` `e` carries,
/// through any `io::Error`s it nests.
fn rustls_error_in<'a>(e: &'a (dyn std::error::Error + 'static)) -> Option<&'a rustls::Error> {
    if let Some(r) = e.downcast_ref::<rustls::Error>() {
        return Some(r);
    }
    let mut io = e.downcast_ref::<std::io::Error>()?;
    loop {
        let inner = io.get_ref()?;
        if let Some(r) = inner.downcast_ref::<rustls::Error>() {
            return Some(r);
        }
        io = inner.downcast_ref::<std::io::Error>()?;
    }
}

fn parse_status(buf: &[u8]) -> Option<u16> {
    let text = String::from_utf8_lossy(buf);
    let line = text.lines().next()?;
    let mut parts = line.split_whitespace();
    let _http = parts.next()?;
    parts.next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::{certificates_from_pem, untrusted_server_cert};
    use crate::errors::TlsError;

    /// An error as reqwest's connector returns a failed handshake: an
    /// `io::Error` of kind Other around tokio-rustls's, around `err`.
    fn wrapped(err: rustls::Error) -> std::io::Error {
        std::io::Error::other(std::io::Error::new(std::io::ErrorKind::InvalidData, err))
    }

    #[test]
    fn an_invalid_certificate_is_found_through_nested_io_errors() {
        let err = wrapped(rustls::Error::InvalidCertificate(
            rustls::CertificateError::UnknownIssuer,
        ));
        let reason = untrusted_server_cert(&err).expect("an untrusted server certificate");
        assert!(reason.contains("UnknownIssuer"), "{reason}");
    }

    #[test]
    fn other_handshake_failures_are_not_an_untrusted_certificate() {
        for err in [
            rustls::Error::AlertReceived(rustls::AlertDescription::ProtocolVersion),
            rustls::Error::AlertReceived(rustls::AlertDescription::HandshakeFailure),
            rustls::Error::PeerIncompatible(rustls::PeerIncompatible::Tls12NotOffered),
        ] {
            assert_eq!(untrusted_server_cert(&wrapped(err.clone())), None, "{err}");
        }
        let reset = std::io::Error::from(std::io::ErrorKind::ConnectionReset);
        assert_eq!(untrusted_server_cert(&reset), None);
    }

    fn self_signed(cn: &str) -> rcgen::Certificate {
        let key = rcgen::KeyPair::generate().expect("key");
        let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).expect("params");
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, cn);
        params.self_signed(&key).expect("self-signed")
    }

    #[test]
    fn every_certificate_in_order() {
        let (a, b) = (self_signed("a"), self_signed("b"));
        let pem = format!("{}{}", a.pem(), b.pem());
        let certs = certificates_from_pem(pem.as_bytes(), "trust anchor").expect("parsed");
        assert_eq!(certs.len(), 2);
        assert_eq!(certs[0].as_ref(), a.der().as_ref());
        assert_eq!(certs[1].as_ref(), b.der().as_ref());
    }

    #[test]
    fn other_sections_and_text_are_skipped() {
        let a = self_signed("a");
        let key = rcgen::KeyPair::generate().expect("key");
        let pem = format!("issued for the test\n{}{}", key.serialize_pem(), a.pem());
        let certs = certificates_from_pem(pem.as_bytes(), "trust anchor").expect("parsed");
        assert_eq!(certs.len(), 1);
        assert_eq!(certs[0].as_ref(), a.der().as_ref());
        let only_key = key.serialize_pem();
        let none = certificates_from_pem(only_key.as_bytes(), "trust anchor").expect("parsed");
        assert!(none.is_empty());
    }

    fn pem_error(pem: &str) -> String {
        match certificates_from_pem(pem.as_bytes(), "client cert") {
            Err(TlsError::ClientConfig(msg)) => msg,
            other => panic!("expected a PEM error for {pem:?}, got {other:?}"),
        }
    }

    #[test]
    fn a_malformed_certificate_is_an_error_naming_the_document() {
        assert_eq!(
            pem_error("-----BEGIN CERTIFICATE-----\nMIIB\n"),
            "client cert PEM: section end \"CERTIFICATE\" missing"
        );
        let base64 = pem_error("-----BEGIN CERTIFICATE-----\n!!!!\n-----END CERTIFICATE-----\n");
        let detail = base64
            .strip_prefix("client cert PEM: ")
            .expect("the document named");
        assert!(!detail.is_empty(), "{base64}");
        assert!(!detail.starts_with("base64 decode error"), "{base64}");
        assert!(!detail.contains('['), "a marker printed as bytes: {base64}");
    }
}
