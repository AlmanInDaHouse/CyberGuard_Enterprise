//! SPEC-003 AC-007 — On every handshake the mock (which requires client
//! auth) receives the agent's SPEC-002 client certificate; a delivered
//! heartbeat therefore proves mutual TLS succeeded. The certificate's CN
//! equals the outer envelope `agent_id` (ADR-0004 §Server validation
//! order step 2).

mod common;

use std::time::Duration;
use tokio::sync::oneshot;

const AGENT_ID: &str = "01934abc-def0-7000-89ab-0000000000aa";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mtls_ac_007_client_cert_presented() {
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
    let received = mock.received();
    let _ = tx.send(());
    let _ = handle.await;

    assert!(
        !received.is_empty(),
        "the client-auth-requiring mock accepts only when the client cert is presented"
    );
    assert_eq!(
        received[0]["agent_id"].as_str(),
        Some(AGENT_ID),
        "outer agent_id must equal the client-certificate CN"
    );
}
