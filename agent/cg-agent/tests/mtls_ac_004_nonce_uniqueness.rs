//! SPEC-003 AC-004 — Across heartbeats the `nonce` values are all
//! distinct and each decodes to exactly 16 bytes. (The mock's nonce cache
//! rejects a replayed nonce with 409; the agent never reuses one, which
//! this asserts.)

mod common;

use std::collections::HashSet;
use std::time::Duration;
use tokio::sync::oneshot;

const AGENT_ID: &str = "01934abc-def0-7000-89ab-0000000000aa";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mtls_ac_004_nonce_uniqueness() {
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

    // Need at least two heartbeats to compare nonces (interval = 1 s).
    for _ in 0..120 {
        if mock.received_count() >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let received = mock.received();
    let _ = tx.send(());
    let _ = handle.await;

    assert!(
        received.len() >= 2,
        "expected >=2 signed heartbeats to compare nonces, got {}",
        received.len()
    );
    let nonces: Vec<String> = received
        .iter()
        .map(|e| e["nonce"].as_str().expect("nonce field").to_string())
        .collect();
    let unique: HashSet<&String> = nonces.iter().collect();
    assert_eq!(unique.len(), nonces.len(), "nonces must all be distinct");
    for n in &nonces {
        assert_eq!(
            common::base64url_decode(n).len(),
            16,
            "each nonce decodes to 16 bytes"
        );
    }
}
