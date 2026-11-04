//! SPEC-024 install_ac_005 (its Amendment 2026-11-04, items 3 to 6),
//! unelevated: installed mode writes `identity\` and its three files each
//! with its final security at creation, `identity.json` last, never over an
//! existing file; a write that stops leaves a set the next start refuses;
//! a partial, inconsistent or insecure set fails closed with code 5 and
//! names the repair; a certificate in the server's answer for another key
//! or id is code 3; the token's rewrite keeps the original's owner and DACL.
//!
//! The custody names the current user where the service's names SYSTEM
//! (`custody_fixtures`); the service and SYSTEM tests run SYSTEM's.
#![cfg(windows)]

mod common;
mod custody_fixtures;

use std::path::PathBuf;

use cg_agent::config::AgentConfig;
use cg_agent::custody::{self, Kind};
use cg_agent::enrollment::EnrolledIdentity;
use cg_agent::identity::installed::{self, InstalledCustody};
use custody_fixtures::{default_owner_sid, normalized, owner_and_dacl, sddl, user_only, user_sid};

/// A root with `agent.toml` (token included) whose identity paths are in
/// `identity\`, and the custody that names the current user.
struct Root {
    _dir: tempfile::TempDir,
    config_path: PathBuf,
    config: AgentConfig,
    custody: InstalledCustody,
}

fn root() -> Root {
    let dir = tempfile::tempdir().expect("tempdir");
    let identity_dir = dir.path().join("identity");
    let anchor = dir.path().join("server-ca.pem");
    let text = common::enrollment_toml(
        "https://127.0.0.1:9",
        "install-ac-005-token",
        Some(&anchor),
        &identity_dir,
    );
    let config_path = dir.path().join("agent.toml");
    custody::create_new_file(&config_path, &sddl(false, "(A;;FR;;;SY)"), text.as_bytes())
        .expect("agent.toml");
    let config = cg_agent::config::load_from_str(&text).expect("config");
    let custody = InstalledCustody {
        identity_dir,
        hygiene_temporary: dir.path().join("agent.toml.hygiene-tmp"),
        policy: user_only(),
        config_policy: custody::Policy {
            allowed: vec![custody_fixtures::user(), custody::Principal::System],
            ..user_only()
        },
        directory_sddl: sddl(true, ""),
        file_sddl: sddl(false, ""),
    };
    Root {
        _dir: dir,
        config_path,
        config,
        custody,
    }
}

fn enrolled(pki: &common::TlsTestPki, certificate_pem: &str) -> EnrolledIdentity {
    EnrolledIdentity {
        agent_id: pki.agent_id.clone(),
        client_certificate_pem: certificate_pem.to_string(),
        issued_at: "2026-11-04T00:00:00Z".to_string(),
        expires_at: "2027-11-04T00:00:00Z".to_string(),
        secret_seed: zeroize::Zeroizing::new(pki.agent_seed),
        public_key: pki.agent_pubkey,
    }
}

/// `identity` with `agent_id` in place of its own.
fn named(mut identity: EnrolledIdentity, agent_id: &str) -> EnrolledIdentity {
    identity.agent_id = agent_id.to_string();
    identity
}

fn files(root: &Root) -> [PathBuf; 3] {
    let enr = root.config.enrollment.as_ref().expect("[enrollment]");
    [&enr.cert_path, &enr.key_path, &enr.identity_path].map(PathBuf::from)
}

fn code_5_containing(error: cg_agent::errors::EnrollmentError, reason: &str) -> String {
    assert_eq!(error.exit_code(), 5, "{error}");
    let line = error.stderr_line();
    assert!(
        line.starts_with("cg-agent: identity error: ") && line.contains(reason),
        "{line}"
    );
    assert!(!line.contains("install-ac-005-token"), "no token: {line}");
    assert!(!line.contains("BEGIN"), "no PEM: {line}");
    line
}

/// Item 4: the directory and each file carry their final security, with
/// no inherited entry, and the set loads back as consistent (item 5).
#[test]
fn install_ac_005_the_identity_is_written_with_its_final_security() {
    let root = root();
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    installed::check(&root.config, &root.custody).expect("absent: enrollment may run");
    installed::persist(
        &root.config,
        &enrolled(&pki, &pki.client_cert_pem),
        &root.custody,
    )
    .expect("persist");

    let user = user_sid();
    let owner = default_owner_sid();
    assert_eq!(
        owner_and_dacl(&root.custody.identity_dir),
        normalized(&format!("O:{owner}D:P(A;OICI;FA;;;{user})")),
        "identity\\: its own protected DACL, nothing inherited from the root"
    );
    for path in files(&root) {
        assert_eq!(
            owner_and_dacl(&path),
            normalized(&format!("O:{owner}D:P(A;;FA;;;{user})")),
            "{}",
            path.display()
        );
    }
    installed::check(&root.config, &root.custody).expect("a complete set in custody");
    let identity = installed::load(&root.config).expect("consistent");
    assert_eq!(identity.agent_id, pki.agent_id);
    assert_eq!(identity.keypair.public_key_bytes(), pki.agent_pubkey);
}

/// Item 4: nothing is written over an existing file.
#[test]
fn install_ac_005_persist_never_overwrites_a_file() {
    let root = root();
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let [cert, key, identity] = files(&root);
    custody::ensure_directory(&root.custody.identity_dir, &sddl(true, ""), &user_only())
        .expect("dir");
    custody::create_new_file(&cert, &sddl(false, ""), b"the valid certificate").expect("cert");

    let error = installed::persist(
        &root.config,
        &enrolled(&pki, &pki.client_cert_pem),
        &root.custody,
    )
    .expect_err("cert.pem exists");
    code_5_containing(error, "cert.pem");
    assert_eq!(
        std::fs::read(&cert).expect("read"),
        b"the valid certificate"
    );
    assert!(
        !key.exists() && !identity.exists(),
        "nothing after the refusal"
    );
}

/// Item 4: the set is absent only when `identity\` is. A write that stops
/// once `identity\` exists, before any file, leaves it empty: the next
/// start refuses it as incomplete (code 5), with the repair, rather than
/// enrolling again.
#[test]
fn install_ac_005_an_empty_identity_directory_is_incomplete() {
    let root = root();
    installed::check(&root.config, &root.custody).expect("absent");
    custody::ensure_directory(&root.custody.identity_dir, &sddl(true, ""), &user_only())
        .expect("dir");
    let error = installed::check(&root.config, &root.custody).expect_err("empty identity\\");
    let line = code_5_containing(
        error,
        "incomplete identity: the certificate and the key and identity.json missing",
    );
    assert!(line.contains("remove the identity directory"), "{line}");
}

/// Item 4, an injected failure: a write that stops after `cert.pem` leaves
/// it with its final security and no `identity.json`; the next start's
/// check refuses the incomplete set (code 5) before reading it.
#[test]
fn install_ac_005_a_write_that_stops_leaves_a_refused_set() {
    let root = root();
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let [cert, key, identity] = files(&root);
    custody::ensure_directory(&root.custody.identity_dir, &sddl(true, ""), &user_only())
        .expect("dir");
    // key.dat cannot be created: a directory holds its name.
    std::fs::create_dir(&key).expect("block key.dat");

    let error = installed::persist(
        &root.config,
        &enrolled(&pki, &pki.client_cert_pem),
        &root.custody,
    )
    .expect_err("key.dat cannot be written");
    code_5_containing(error, "key.dat");
    custody::check(&cert, Kind::File, &user_only()).expect("cert.pem has its final security");
    assert!(!identity.exists(), "identity.json is last");

    std::fs::remove_dir(&key).expect("unblock");
    let error = installed::check(&root.config, &root.custody).expect_err("incomplete");
    code_5_containing(error, "incomplete identity");
}

/// Item 5: a set whose certificate is for another key, or names another
/// agent, or whose `identity.json` does not parse, is code 5 with the
/// repair; nothing is changed.
#[test]
fn install_ac_005_an_inconsistent_set_fails_closed() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let other_key = common::generate_test_pki(common::TEST_AGENT_ID);
    let other_id = common::generate_test_pki("01934abc-def0-7000-89ab-0000000000aa");

    for (identity, reason) in [
        (
            // This agent's key and id, the certificate of another key.
            enrolled(&pki, &other_key.client_cert_pem),
            "the certificate's public key is not the agent's key",
        ),
        (
            // The certificate's key, but this agent's id.
            named(
                enrolled(&other_id, &other_id.client_cert_pem),
                &pki.agent_id,
            ),
            "the certificate's common name is not the agent's id",
        ),
    ] {
        let root = root();
        installed::persist(&root.config, &identity, &root.custody)
            .expect("persist writes what it is given");
        let before: Vec<Vec<u8>> = files(&root)
            .iter()
            .map(|p| std::fs::read(p).unwrap())
            .collect();
        installed::check(&root.config, &root.custody).expect("complete and in custody");
        let error = installed::load(&root.config).expect_err("inconsistent");
        let line = code_5_containing(error, reason);
        assert!(
            line.contains("restore the identity, or remove the identity directory"),
            "the repair: {line}"
        );
        let after: Vec<Vec<u8>> = files(&root)
            .iter()
            .map(|p| std::fs::read(p).unwrap())
            .collect();
        assert_eq!(before, after, "nothing is changed");
    }

    let root = root();
    let [cert, key, identity] = files(&root);
    installed::persist(
        &root.config,
        &enrolled(&pki, &pki.client_cert_pem),
        &root.custody,
    )
    .expect("persist");
    std::fs::remove_file(&identity).expect("remove");
    custody::create_new_file(&identity, &sddl(false, ""), b"{\"agent_id\": \"01934").expect("torn");
    assert!(cert.exists() && key.exists());
    let error = installed::load(&root.config).expect_err("a torn identity.json");
    code_5_containing(error, "identity.json corrupted");
}

/// Item 3: an insecure `identity\` or file is refused (code 5) before any
/// is read.
#[test]
fn install_ac_005_an_insecure_set_fails_closed() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let root = root();
    installed::persist(
        &root.config,
        &enrolled(&pki, &pki.client_cert_pem),
        &root.custody,
    )
    .expect("persist");
    let [_, key, _] = files(&root);
    std::fs::remove_file(&key).expect("remove");
    custody::create_new_file(&key, &sddl(false, "(A;;FR;;;BU)"), b"sealed").expect("readable");
    let error = installed::check(&root.config, &root.custody).expect_err("readable key.dat");
    code_5_containing(error, "allows access to a principal other than");

    let broad = self::root();
    custody::ensure_directory(
        &broad.custody.identity_dir,
        &sddl(true, "(A;OICI;FR;;;BU)"),
        &user_only(),
    )
    .expect("a broad identity\\");
    let error = installed::check(&broad.config, &broad.custody).expect_err("broad identity\\");
    code_5_containing(error, "allows access to a principal other than");
    assert!(
        installed::persist(
            &broad.config,
            &enrolled(&pki, &pki.client_cert_pem),
            &broad.custody
        )
        .is_err(),
        "nothing is written into it"
    );
    assert!(files(&broad).iter().all(|p| !p.exists()));
}

/// Item 5 at enrollment: a certificate in the answer for another key or id
/// is a malformed answer, code 3.
#[test]
fn install_ac_005_a_certificate_for_another_key_in_the_answer_is_code_3() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let other_key = common::generate_test_pki(common::TEST_AGENT_ID);
    let other_id = common::generate_test_pki("01934abc-def0-7000-89ab-0000000000aa");
    installed::check_issued(&enrolled(&pki, &pki.client_cert_pem)).expect("matches");
    for answer in [
        enrolled(&pki, &other_key.client_cert_pem),
        named(
            enrolled(&other_id, &other_id.client_cert_pem),
            &pki.agent_id,
        ),
    ] {
        let error = installed::check_issued(&answer).expect_err("another key or id");
        assert_eq!(error.exit_code(), 3, "{error}");
        assert!(
            error.stderr_line().contains("malformed response"),
            "{error}"
        );
    }
}

/// Item 6: FR-014's rewrite drops the token and keeps the original's owner
/// and DACL.
#[test]
fn install_ac_005_token_hygiene_keeps_owner_and_dacl() {
    let root = root();
    let before = owner_and_dacl(&root.config_path);
    installed::hygienize_token(&root.config_path, &root.custody);
    let text = std::fs::read_to_string(&root.config_path).expect("read");
    assert!(!text.contains("install-ac-005-token"), "{text}");
    cg_agent::config::load_from_str(&text).expect("still a valid configuration");
    assert_eq!(owner_and_dacl(&root.config_path), before);
    assert!(!root.custody.hygiene_temporary.exists());
}
