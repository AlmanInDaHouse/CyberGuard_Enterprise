//! SPEC-003 AC-008 — When the mock rejects the client certificate at the
//! TLS layer (its verifier trusts a different root), the agent exits 7
//! with `server rejected client certificate`.

mod common;

use cg_agent::errors::{AgentError, TlsError};
use std::time::Duration;

const AGENT_ID: &str = "01934abc-def0-7000-89ab-0000000000aa";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mtls_ac_008_client_cert_rejected() {
    let pki = common::generate_test_pki(AGENT_ID);
    let mock = common::TlsMockServer::start(&pki, common::TlsMockMode::RejectClientCert).await;
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
    .expect("run_secure must return on client-cert rejection, not hang");

    let err = result.expect_err("a rejected client certificate must fail the secure path");
    match err {
        AgentError::Tls(ref t) => {
            assert_eq!(t.exit_code(), 7, "client-cert rejection is exit code 7");
            assert!(
                matches!(t, TlsError::ClientCertRejected(_)),
                "expected ClientCertRejected, got {t:?}"
            );
        }
        other => panic!("expected AgentError::Tls, got {other:?}"),
    }
    assert!(
        format!("{err}").contains("server rejected client certificate"),
        "error must carry the documented substring; got: {err}"
    );
}
