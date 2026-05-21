//! SPEC-003 AC-003 — A signed envelope the mock receives verifies:
//! recomputing JCS(outer minus signature) and checking the Ed25519
//! signature against the agent's enrolled public key succeeds. The mock
//! only records envelopes whose signature verifies, so a recorded
//! envelope is proof; we also assert the signature is a 64-byte Ed25519.

mod common;

use std::time::Duration;
use tokio::sync::oneshot;

const AGENT_ID: &str = "01934abc-def0-7000-89ab-0000000000aa";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mtls_ac_003_signature_verifies() {
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
        "the mock records an envelope only when its signature verifies"
    );
    let sig = received[0]["signature"]
        .as_str()
        .expect("signature field present");
    assert_eq!(
        common::base64url_decode(sig).len(),
        64,
        "Ed25519 signature decodes to 64 bytes"
    );
}
