//! The trust anchor's fingerprints (SPEC-024 §Data contracts §3): their form,
//! and the exact-set rule that ties a PEM file's certificates to them. The
//! migration and the adoption check `server-ca.pem` with it, and the setup
//! checks the anchor's source (Amendment 2026-11-09, items 2 and 3).

use std::collections::BTreeSet;

use rustls::pki_types::pem::PemObject as _;
use sha2::{Digest, Sha256};

/// `sha256:` and 64 lowercase hexadecimal digits (§Data contracts §3).
pub fn is_fingerprint(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digits| {
        digits.len() == 64
            && digits
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// One certificate's fingerprint, over its DER, as SPEC-023's export writes it.
pub fn fingerprint(der: &[u8]) -> String {
    let mut text = String::with_capacity(7 + 64);
    text.push_str("sha256:");
    for byte in Sha256::digest(der) {
        use std::fmt::Write;
        let _ = write!(text, "{byte:02x}");
    }
    text
}

/// The distinct certificates of the PEM file `name`, in their order: exactly
/// those of the fingerprints given (§Data contracts §3's exact-set rule).
pub fn check_certificates(
    bytes: &[u8],
    expected: &BTreeSet<String>,
    name: &str,
) -> Result<Vec<Vec<u8>>, String> {
    let mut distinct: Vec<Vec<u8>> = Vec::new();
    for certificate in rustls::pki_types::CertificateDer::pem_slice_iter(bytes) {
        let certificate = certificate.map_err(|_| format!("{name} cannot be read"))?;
        if !distinct.iter().any(|der| der == certificate.as_ref()) {
            distinct.push(certificate.as_ref().to_vec());
        }
    }
    let found: BTreeSet<String> = distinct.iter().map(|der| fingerprint(der)).collect();
    if found != *expected {
        return Err(format!(
            "the certificates of {name} are not exactly those of the fingerprints given"
        ));
    }
    Ok(distinct)
}

/// `check_certificates` for `server-ca.pem`.
pub fn check_anchor(bytes: &[u8], expected: &BTreeSet<String>) -> Result<Vec<Vec<u8>>, String> {
    check_certificates(bytes, expected, "server-ca.pem")
}
