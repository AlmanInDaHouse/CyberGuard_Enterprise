//! SPEC-003 AC-006 — Against a mock that offers only TLS 1.2, the agent
//! refuses to negotiate and never transmits a signed envelope. The
//! sub-1.3 handshake is a non-fatal transport failure (warn + continue),
//! so the run loop survives until shutdown and the mock records nothing.

mod common;

use std::time::Duration;
use tokio::sync::oneshot;

const AGENT_ID: &str = "01934abc-def0-7000-89ab-0000000000aa";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mtls_ac_006_tls12_refused() {
    let pki = common::generate_test_pki(AGENT_ID);
    let mock = common::TlsMockServer::start(&pki, common::TlsMockMode::Tls12Only).await;
    let common::SecureFixture {
        config,
        identity,
        trust_anchor: _trust_anchor,
    } = common::secure_fixture(&mock.base_url, &pki);

    let (tx, rx) = oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        cg_agent::run_secure(config, identity, async move {
            let _ = rx.await;
        })
        .await
    });

    tokio::time::sleep(Duration::from_millis(2000)).await;
    let _ = tx.send(());
    let outcome = handle.await.expect("run_secure task must not panic");

    assert!(
        outcome.is_ok(),
        "a TLS 1.2-only server must be a non-fatal transport failure, got {outcome:?}"
    );
    assert_eq!(
        mock.received_count(),
        0,
        "no signed envelope may be transmitted over a sub-1.3 connection"
    );
}
