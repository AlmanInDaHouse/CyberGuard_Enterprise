//! SPEC-024 Amendment 2026-11-04, item 5: the agent reads its client
//! certificate's Ed25519 public key and subject common name with a DER
//! reader of its own, and `check_consistency` compares them with the key
//! and the `agent_id`. Any other certificate shape is an error, never a
//! panic. Portable.

mod common;

use cg_agent::identity::check_consistency;
use cg_agent::x509::{certificate_identity, certificate_identity_der};

#[test]
fn install_ac_005_the_certificate_names_its_key_and_agent() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let identity = certificate_identity(&pki.client_cert_pem).expect("read");
    assert_eq!(identity.common_name, common::TEST_AGENT_ID);
    assert_eq!(identity.ed25519_public_key, pki.agent_pubkey);

    check_consistency(
        &pki.client_cert_pem,
        &pki.agent_pubkey,
        common::TEST_AGENT_ID,
    )
    .expect("consistent");
    let other = common::generate_test_pki(common::TEST_AGENT_ID);
    let error = check_consistency(
        &pki.client_cert_pem,
        &other.agent_pubkey,
        common::TEST_AGENT_ID,
    )
    .expect_err("another key");
    assert_eq!(error, "the certificate's public key is not the agent's key");
    let error = check_consistency(
        &pki.client_cert_pem,
        &pki.agent_pubkey,
        "01934abc-def0-7000-89ab-0000000000aa",
    )
    .expect_err("another id");
    assert_eq!(error, "the certificate's common name is not the agent's id");
}

/// SPEC-024 Amendment 2026-11-07: a document the reader TLS uses refuses is
/// refused here too, whatever its first block holds, and what that reader
/// accepts stays accepted, read from the first block. The error names the
/// condition and quotes nothing of the document.
#[test]
fn install_ac_005_a_document_tls_refuses_is_refused() {
    use rustls::pki_types::pem::PemObject as _;
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let good = &pki.client_cert_pem;
    let tls_accepts = |document: &str| {
        rustls::pki_types::CertificateDer::pem_slice_iter(document.as_bytes())
            .collect::<Result<Vec<_>, _>>()
            .is_ok()
    };
    for later in [
        "-----BEGIN CERTIFICATE-----\n!!not base64!!\n-----END CERTIFICATE-----\n",
        "-----BEGIN PRIVATE KEY-----\n!!not base64!!\n-----END PRIVATE KEY-----\n",
        "-----BEGIN CERTIFICATE-----\nMIIB\n",
        "-----BEGIN FOO-----\nAAAA\n",
    ] {
        let document = format!("{good}{later}");
        assert!(!tls_accepts(&document), "TLS refuses: {later}");
        let error = certificate_identity(&document).expect_err(later);
        assert_eq!(error, "the certificate's PEM document cannot be read");
    }
    for later in [
        "-----BEGIN FOO-----\n!!not base64!!\n-----END FOO-----\n",
        good.as_str(),
    ] {
        let document = format!("{good}{later}");
        assert!(tls_accepts(&document), "TLS accepts: {later}");
        let identity = certificate_identity(&document).expect(later);
        assert_eq!(identity.ed25519_public_key, pki.agent_pubkey);
    }
}

/// SPEC-024 Amendment 2026-11-07: the consistency check reads the document
/// as TLS reads it: a later section TLS refuses fails it, though the first
/// block is good, and the error quotes nothing of the document.
#[test]
fn install_ac_005_a_later_section_tls_refuses_fails_the_consistency_check() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    for later in [
        "-----BEGIN CERTIFICATE-----\nnot base64 !!\n-----END CERTIFICATE-----\n",
        "-----BEGIN CERTIFICATE-----\nAAAA\n",
    ] {
        let document = format!("{}{later}", pki.client_cert_pem);
        let error = check_consistency(&document, &pki.agent_pubkey, common::TEST_AGENT_ID)
            .expect_err("a section TLS refuses");
        assert!(!error.contains("not base64"), "{error}");
    }
}

/// A certificate for another algorithm, a PEM without a certificate and
/// every truncation of a valid DER are errors.
#[test]
fn install_ac_005_other_shapes_are_errors() {
    let key = rcgen::KeyPair::generate().expect("an ECDSA key");
    let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).expect("params");
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, common::TEST_AGENT_ID);
    let ecdsa = params.self_signed(&key).expect("self-signed");
    let error = certificate_identity(&ecdsa.pem()).expect_err("not Ed25519");
    assert_eq!(error, "the public key is not Ed25519");

    let ed25519 = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519).expect("an Ed25519 key");
    let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).expect("params");
    params.distinguished_name = rcgen::DistinguishedName::new();
    let no_cn = params.self_signed(&ed25519).expect("self-signed");
    assert_eq!(
        certificate_identity(&no_cn.pem()).expect_err("no CN"),
        "no common name"
    );

    // Two common names: neither is taken as the agent's.
    let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).expect("params");
    params.distinguished_name = rcgen::DistinguishedName::new();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, common::TEST_AGENT_ID);
    params.distinguished_name.push(
        rcgen::DnType::CustomDnType(vec![2, 5, 4, 3]),
        "01934abc-def0-7000-89ab-0000000000aa",
    );
    let two = params.self_signed(&ed25519).expect("self-signed");
    assert_eq!(
        certificate_identity(&two.pem()).expect_err("two CNs"),
        "more than one common name"
    );

    assert!(certificate_identity("not a PEM").is_err());
    let key_only = "-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n";
    assert_eq!(
        certificate_identity(key_only).expect_err("no certificate"),
        "no CERTIFICATE block"
    );

    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let der = pem::parse(&pki.client_cert_pem)
        .expect("pem")
        .into_contents();
    certificate_identity_der(&der).expect("the whole DER");
    for length in 0..der.len() {
        assert!(
            certificate_identity_der(&der[..length]).is_err(),
            "a DER cut at {length} bytes"
        );
    }
    // DER lengths in their shortest form only.
    let error = certificate_identity_der(&[0x30, 0x81, 0x03, 0x02, 0x01, 0x00])
        .expect_err("a long form for 3");
    assert!(error.contains("shortest form"), "{error}");
    let error = certificate_identity_der(&[0x30, 0x82, 0x00, 0x83]).expect_err("a leading zero");
    assert!(error.contains("shortest form"), "{error}");

    let mut longer = der.clone();
    longer.push(0);
    assert!(certificate_identity_der(&longer).is_err(), "bytes after it");
}

/// Debt #146: the reader runs over a certificate the ingest server's own
/// `issueClientCert` issued (`custody_fixtures/issue_client_cert.mts`, for
/// the RFC 8032 test-vector 1 key and a fixed agent id).
#[test]
fn install_ac_005_a_server_issued_certificate_is_read() {
    let pem = include_str!("custody_fixtures/server_issued_client_cert.txt");
    let identity = certificate_identity(pem).expect("read");
    assert_eq!(identity.common_name, "01934abc-def0-7000-89ab-000000000146");
    let mut key = [0u8; 32];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(
            &"d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"[2 * i..2 * i + 2],
            16,
        )
        .expect("hex");
    }
    assert_eq!(identity.ed25519_public_key, key);
    check_consistency(pem, &key, "01934abc-def0-7000-89ab-000000000146").expect("consistent");
}
