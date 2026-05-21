//! SPEC-003 AC-009 — The inner `body` of the outer envelope equals the
//! SPEC-001 envelope verbatim: envelope_version "0.1.0", the four-field
//! agent block, sequence_number, sent_at, status, uptime_seconds.
//! (Backward-compatibility regression.)

mod common;

use std::time::Duration;
use tokio::sync::oneshot;

const AGENT_ID: &str = "01934abc-def0-7000-89ab-0000000000aa";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mtls_ac_009_inner_body_verbatim() {
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

    assert!(!received.is_empty(), "expected >=1 signed heartbeat");
    let body = &received[0]["body"];
    assert_eq!(
        body["envelope_version"].as_str(),
        Some("0.1.0"),
        "inner envelope_version unchanged"
    );
    assert_eq!(body["agent"]["agent_id"].as_str(), Some(AGENT_ID));
    assert!(body["agent"]["agent_version"].is_string());
    assert!(body["agent"]["agent_platform"].is_string());
    assert!(body["agent"]["agent_hostname"].is_string());
    assert!(body["sequence_number"].is_number());
    assert!(body["sent_at"].is_string());
    assert_eq!(body["status"].as_str(), Some("online"));
    assert!(body["uptime_seconds"].is_number());
}
