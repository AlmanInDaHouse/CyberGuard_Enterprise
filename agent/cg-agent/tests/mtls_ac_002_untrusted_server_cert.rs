//! SPEC-003 AC-002 — When the mock presents a server certificate that
//! does not chain to the configured trust anchor, the agent refuses the
//! connection and exits 6 with `server certificate verification failed`.

mod common;

use cg_agent::errors::{AgentError, TlsError};
use std::time::Duration;

const AGENT_ID: &str = "01934abc-def0-7000-89ab-0000000000aa";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mtls_ac_002_untrusted_server_cert() {
    let pki = common::generate_test_pki(AGENT_ID);
    let mock = common::TlsMockServer::start(&pki, common::TlsMockMode::UntrustedServerCert).await;
    let common::SecureFixture {
        config,
        identity,
        trust_anchor: _trust_anchor,
    } = common::secure_fixture(&mock.base_url, &pki);

    let result = tokio::time::timeout(
        Duration::from_secs(10),
        cg_agent::run_secure(config, identity, std::future::pending::<()>()),
    )
    .await
    .expect("run_secure must return on a fatal cert failure, not hang");

    let err = result.expect_err("an untrusted server certificate must fail the secure path");
    match err {
        AgentError::Tls(ref t) => {
            assert_eq!(t.exit_code(), 6, "untrusted server cert is exit code 6");
            assert!(
                matches!(
                    t,
                    TlsError::ServerCertUntrusted(_) | TlsError::ClientConfig(_)
                ),
                "expected ServerCertUntrusted, got {t:?}"
            );
        }
        other => panic!("expected AgentError::Tls, got {other:?}"),
    }
    assert!(
        format!("{err}").contains("server certificate verification failed"),
        "error must carry the documented substring; got: {err}"
    );
}
