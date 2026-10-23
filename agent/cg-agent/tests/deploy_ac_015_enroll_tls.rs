//! deploy_ac_015, the agent's half (SPEC-023 §Operational §6; SPEC-002 and SPEC-003 by
//! scope): the agent refuses a configuration with `[enrollment]` and an `http://`
//! `server.url`, or without a trust anchor (exit code 2); it enrolls against a server whose
//! certificate its anchor issued, and fails with exit code 6, without the retries of a
//! network failure, against one another CA issued. Against a server that offers TLS 1.2
//! alone, or TLS 1.3 with only a cipher suite ADR-0004 does not allow, it does not enroll,
//! and ends as unreachable (exit code 4) after those retries.

mod common;

use cg_agent::errors::{ConfigError, EnrollmentError, TlsError};
use common::TlsMockMode;
use std::path::Path;
use std::process::Output;

/// The fixture's `max_retries`: the attempts a network failure gets.
const ATTEMPTS: usize = 3;

/// Write `agent.toml` into `dir` with `[enrollment]`, `server_url`, an https
/// `server.heartbeat_url` (beside which SPEC-003 Amendment 2026-05-22 (b) let
/// `server.url` be http), and the test CA as trust anchor when `with_anchor`.
fn write_config(dir: &Path, server_url: &str, with_anchor: bool) -> std::path::PathBuf {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let anchor = with_anchor.then(|| {
        let path = dir.join("server-ca.pem");
        std::fs::write(&path, &pki.trust_anchor_pem).expect("write trust anchor");
        path
    });
    let path = dir.join("agent.toml");
    let toml = common::enrollment_toml(server_url, "tok-deploy-015", anchor.as_deref(), dir)
        .replacen(
            "[server]
",
            "[server]
heartbeat_url = \"https://127.0.0.1:9\"
",
            1,
        );
    assert!(toml.contains("heartbeat_url"), "{toml}");
    std::fs::write(&path, toml).expect("write agent.toml");
    path
}

/// Run the real binary on `config` and return its output.
async fn run_binary(config: &Path) -> Output {
    tokio::time::timeout(
        std::time::Duration::from_secs(60),
        tokio::process::Command::new(env!("CARGO_BIN_EXE_cg-agent"))
            .arg("--config")
            .arg(config)
            .output(),
    )
    .await
    .expect("the agent must exit on its own")
    .expect("launch the agent binary")
}

#[tokio::test]
async fn deploy_ac_015_enrollment_with_an_http_server_url_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_config(dir.path(), "http://127.0.0.1:9", true);

    let err = cg_agent::config::load_from_path(&path).expect_err("http with [enrollment]");
    assert!(
        matches!(&err, ConfigError::Invalid(m) if m == "server.url must use https when [enrollment] is set"),
        "{err:?}"
    );

    let out = run_binary(&path).await;
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("server.url must use https when [enrollment] is set"),
        "{stderr}"
    );
}

#[tokio::test]
async fn deploy_ac_015_enrollment_without_a_trust_anchor_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_config(dir.path(), "https://127.0.0.1:9", false);

    let err = cg_agent::config::load_from_path(&path).expect_err("[enrollment] without anchor");
    assert!(
        matches!(&err, ConfigError::MissingKey(k) if k == "server.trust_anchor_path"),
        "{err:?}"
    );

    let out = run_binary(&path).await;
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("missing key 'server.trust_anchor_path'"),
        "{stderr}"
    );
}

#[tokio::test]
async fn deploy_ac_015_an_empty_trust_anchor_path_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_config(dir.path(), "https://127.0.0.1:9", false);
    let toml = std::fs::read_to_string(&path)
        .expect("read agent.toml")
        .replacen(
            "[server]
",
            "[server]
trust_anchor_path = ''
",
            1,
        );
    std::fs::write(&path, toml).expect("write agent.toml");

    let err = cg_agent::config::load_from_path(&path).expect_err("an empty anchor path");
    assert!(
        matches!(&err, ConfigError::Invalid(m) if m == "server.trust_anchor_path must be non-empty"),
        "{err:?}"
    );

    let out = run_binary(&path).await;
    assert_eq!(out.status.code(), Some(2), "{out:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deploy_ac_015_enrolls_against_a_certificate_its_anchor_issued() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let mock = common::TlsEnrollMock::start(&pki, TlsMockMode::Normal).await;
    let fixture = common::enrollment_fixture(&mock.base_url, "tok-deploy-015", &pki);

    let identity = cg_agent::identity::ensure_identity(&fixture.config, &fixture.config_path)
        .await
        .expect("enrollment over TLS against the anchor's CA");

    assert_eq!(identity.agent_id, common::TEST_AGENT_ID);
    assert_eq!(mock.enroll_received_count(), 1);
    assert_eq!(mock.connections(), 1);
    assert!(fixture.all_artifacts_exist());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deploy_ac_015_another_ca_ends_enrollment_with_exit_6_without_retries() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let mock = common::TlsEnrollMock::start(&pki, TlsMockMode::UntrustedServerCert).await;
    let fixture = common::enrollment_fixture(&mock.base_url, "tok-deploy-015", &pki);

    let err = cg_agent::identity::ensure_identity(&fixture.config, &fixture.config_path)
        .await
        .expect_err("a server certificate another CA issued");

    assert_eq!(err.exit_code(), 6, "{err:?}");
    assert!(
        matches!(err, EnrollmentError::Tls(TlsError::ServerCertUntrusted(_))),
        "{err:?}"
    );
    assert_eq!(mock.connections(), 1, "no retry after the first handshake");
    assert_eq!(mock.enroll_received_count(), 0, "no request crossed");
    assert!(fixture.no_artifacts_exist());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deploy_ac_015_the_binary_exits_6_against_another_ca() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let mock = common::TlsEnrollMock::start(&pki, TlsMockMode::UntrustedServerCert).await;
    let fixture = common::enrollment_fixture(&mock.base_url, "tok-deploy-015", &pki);

    let out = run_binary(&fixture.config_path).await;
    assert_eq!(out.status.code(), Some(6), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("cg-agent: tls: server certificate verification failed: "),
        "{stderr}"
    );
    assert_eq!(mock.connections(), 1, "no retry after the first handshake");
    assert!(fixture.no_artifacts_exist());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deploy_ac_015_a_tls12_only_server_ends_as_unreachable_after_the_retries() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let mock = common::TlsEnrollMock::start(&pki, TlsMockMode::Tls12Only).await;
    let fixture = common::enrollment_fixture(&mock.base_url, "tok-deploy-015", &pki);

    let err = cg_agent::identity::ensure_identity(&fixture.config, &fixture.config_path)
        .await
        .expect_err("a TLS 1.2-only server");

    assert_eq!(err.exit_code(), 4, "{err:?}");
    assert!(
        matches!(err, EnrollmentError::Unreachable { attempts, .. } if attempts as usize == ATTEMPTS),
        "{err:?}"
    );
    assert_eq!(mock.connections(), ATTEMPTS, "each attempt a new handshake");
    assert_eq!(mock.enroll_received_count(), 0);
    assert!(fixture.no_artifacts_exist());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deploy_ac_015_a_server_with_only_aes_128_ends_as_unreachable_after_the_retries() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let mock = common::TlsEnrollMock::start(&pki, TlsMockMode::Aes128Only).await;
    let fixture = common::enrollment_fixture(&mock.base_url, "tok-deploy-015", &pki);

    let err = cg_agent::identity::ensure_identity(&fixture.config, &fixture.config_path)
        .await
        .expect_err("a server offering only TLS_AES_128_GCM_SHA256");

    assert_eq!(err.exit_code(), 4, "{err:?}");
    assert!(
        matches!(err, EnrollmentError::Unreachable { .. }),
        "{err:?}"
    );
    assert_eq!(mock.connections(), ATTEMPTS, "each attempt a new handshake");
    assert_eq!(mock.enroll_received_count(), 0);
}
