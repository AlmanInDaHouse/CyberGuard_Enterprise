//! SPEC-003 AC-001 — Given a valid identity and a trust anchor matching
//! the mock's server cert, the agent completes a TLS 1.3 mutual handshake
//! and delivers its first signed heartbeat, which the mock accepts.

mod common;

use std::time::Duration;
use tokio::sync::oneshot;

const AGENT_ID: &str = "01934abc-def0-7000-89ab-0000000000aa";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mtls_ac_001_handshake_and_first_heartbeat() {
    let pki = common::generate_test_pki(AGENT_ID);
    let mock = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
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

    for _ in 0..100 {
        if mock.received_count() >= 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let count = mock.received_count();
    let _ = tx.send(());
    let _ = handle.await;

    assert!(
        count >= 1,
        "expected >=1 signed heartbeat delivered over mTLS 1.3, got {count}"
    );
}
