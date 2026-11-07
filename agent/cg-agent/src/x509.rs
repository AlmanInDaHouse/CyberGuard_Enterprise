//! The two fields of the agent's client certificate that installed mode
//! checks against its key and its `agent_id` (SPEC-024 Amendment
//! 2026-11-04, item 5): the Ed25519 public key of `subjectPublicKeyInfo`
//! and the subject's common name.
//!
//! A minimal DER reader over the certificate's structure (RFC 5280 §4.1),
//! with no new dependency: it walks `tbsCertificate` to `subject` and
//! `subjectPublicKeyInfo` and validates nothing else. The certificate's
//! signature and validity stay with TLS (SPEC-003), which also reads the
//! PEM document: a document its reader refuses is refused here (Amendment
//! 2026-11-07).

/// The fields installed mode compares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificateIdentity {
    pub ed25519_public_key: [u8; 32],
    pub common_name: String,
}

const SEQUENCE: u8 = 0x30;
const SET: u8 = 0x31;
const INTEGER: u8 = 0x02;
const BIT_STRING: u8 = 0x03;
const OID: u8 = 0x06;
const UTF8_STRING: u8 = 0x0c;
const PRINTABLE_STRING: u8 = 0x13;
const VERSION: u8 = 0xa0;
/// id-at-commonName, 2.5.4.3.
const COMMON_NAME: &[u8] = &[0x55, 0x04, 0x03];
/// id-Ed25519, 1.3.101.112 (RFC 8410).
const ED25519: &[u8] = &[0x2b, 0x65, 0x70];

/// Read the first `CERTIFICATE` block of `pem_text`, the end-entity
/// certificate TLS presents (SPEC-003), from a document TLS accepts.
pub fn certificate_identity(pem_text: &str) -> Result<CertificateIdentity, String> {
    let certificates = certificates_in(pem_text)?;
    let first = certificates
        .first()
        .ok_or_else(|| "no CERTIFICATE block".to_string())?;
    certificate_identity_der(first.as_ref())
}

/// The certificates of a PEM document, in their order, as TLS reads them
/// (`tls.rs`). The error names the condition, not the reader's message,
/// which can quote the document.
pub fn certificates_in(
    pem_text: &str,
) -> Result<Vec<rustls::pki_types::CertificateDer<'static>>, String> {
    use rustls::pki_types::pem::PemObject as _;
    rustls::pki_types::CertificateDer::pem_slice_iter(pem_text.as_bytes())
        .collect::<Result<_, _>>()
        .map_err(|_| "the certificate's PEM document cannot be read".to_string())
}

/// The same from the certificate's DER.
pub fn certificate_identity_der(der: &[u8]) -> Result<CertificateIdentity, String> {
    let mut outer = Reader(der);
    let certificate = outer.expect(SEQUENCE, "Certificate")?;
    if !outer.0.is_empty() {
        return Err("bytes after the Certificate".to_string());
    }
    let mut certificate = Reader(certificate);
    let mut tbs = Reader(certificate.expect(SEQUENCE, "tbsCertificate")?);
    if tbs.peek() == Some(VERSION) {
        tbs.next("version")?;
    }
    tbs.expect(INTEGER, "serialNumber")?;
    tbs.expect(SEQUENCE, "signature")?;
    tbs.expect(SEQUENCE, "issuer")?;
    tbs.expect(SEQUENCE, "validity")?;
    let subject = tbs.expect(SEQUENCE, "subject")?;
    let spki = tbs.expect(SEQUENCE, "subjectPublicKeyInfo")?;
    Ok(CertificateIdentity {
        common_name: common_name(subject)?,
        ed25519_public_key: ed25519_key(spki)?,
    })
}

/// The one common name of a `Name`; none or several is refused.
fn common_name(name: &[u8]) -> Result<String, String> {
    let mut found: Option<String> = None;
    let mut rdns = Reader(name);
    while !rdns.0.is_empty() {
        let mut attributes = Reader(rdns.expect(SET, "RelativeDistinguishedName")?);
        while !attributes.0.is_empty() {
            let mut attribute = Reader(attributes.expect(SEQUENCE, "AttributeTypeAndValue")?);
            let kind = attribute.expect(OID, "AttributeType")?;
            let (tag, value) = attribute.next("AttributeValue")?;
            if kind != COMMON_NAME {
                continue;
            }
            if tag != UTF8_STRING && tag != PRINTABLE_STRING {
                return Err(format!("common name of tag {tag:#04x}"));
            }
            let text =
                std::str::from_utf8(value).map_err(|_| "common name is not UTF-8".to_string())?;
            if found.replace(text.to_string()).is_some() {
                return Err("more than one common name".to_string());
            }
        }
    }
    found.ok_or_else(|| "no common name".to_string())
}

/// The Ed25519 key of a `SubjectPublicKeyInfo`; another algorithm is
/// refused.
fn ed25519_key(spki: &[u8]) -> Result<[u8; 32], String> {
    let mut spki = Reader(spki);
    let mut algorithm = Reader(spki.expect(SEQUENCE, "AlgorithmIdentifier")?);
    if algorithm.expect(OID, "algorithm")? != ED25519 || !algorithm.0.is_empty() {
        return Err("the public key is not Ed25519".to_string());
    }
    let bits = spki.expect(BIT_STRING, "subjectPublicKey")?;
    if !spki.0.is_empty() {
        return Err("bytes after the subjectPublicKey".to_string());
    }
    match bits {
        [0, key @ ..] if key.len() == 32 => {
            let mut out = [0u8; 32];
            out.copy_from_slice(key);
            Ok(out)
        }
        _ => Err("the Ed25519 public key is not 32 bytes".to_string()),
    }
}

/// A cursor over DER bytes. Only single-byte tags and definite lengths of
/// at most four bytes in their shortest form, which a certificate uses, are
/// read.
struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn peek(&self) -> Option<u8> {
        self.0.first().copied()
    }

    fn next(&mut self, what: &str) -> Result<(u8, &'a [u8]), String> {
        let truncated = || format!("{what}: truncated");
        let (&tag, rest) = self.0.split_first().ok_or_else(truncated)?;
        if tag & 0x1f == 0x1f {
            return Err(format!("{what}: multi-byte tag"));
        }
        let (&first, mut rest) = rest.split_first().ok_or_else(truncated)?;
        let length = if first < 0x80 {
            usize::from(first)
        } else {
            let count = usize::from(first & 0x7f);
            if count == 0 || count > 4 || rest.len() < count {
                return Err(format!("{what}: bad length"));
            }
            let (bytes, after) = rest.split_at(count);
            rest = after;
            let length = bytes.iter().fold(0usize, |n, &b| (n << 8) | usize::from(b));
            // DER: the shortest form, so no leading zero and no long form
            // for a length below 128.
            if bytes[0] == 0 || length < 0x80 {
                return Err(format!("{what}: length not in its shortest form"));
            }
            length
        };
        if rest.len() < length {
            return Err(truncated());
        }
        let (value, after) = rest.split_at(length);
        self.0 = after;
        Ok((tag, value))
    }

    fn expect(&mut self, tag: u8, what: &str) -> Result<&'a [u8], String> {
        let (found, value) = self.next(what)?;
        if found != tag {
            return Err(format!("{what}: tag {found:#04x}, expected {tag:#04x}"));
        }
        Ok(value)
    }
}
