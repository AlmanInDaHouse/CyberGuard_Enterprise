//! SPEC-003 AC-005 — When the server rejects a signed envelope (here the
//! mock answers every envelope 409, simulating a timestamp-skew or replay
//! rejection), the agent logs `warn` and continues to the next interval;
//! it does NOT exit. The run loop survives until graceful shutdown.

mod common;

use std::time::Duration;
use tokio::sync::oneshot;

const AGENT_ID: &str = "01934abc-def0-7000-89ab-0000000000aa";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mtls_ac_005_server_rejection_nonfatal() {
    let pki = common::generate_test_pki(AGENT_ID);
    // TLS is Normal; the server rejects every signed envelope with 409.
    let mock = common::TlsMockServer::start_rejecting(&pki, 409).await;
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

    // Let the agent attempt a few intervals — each rejected — then stop.
    tokio::time::sleep(Duration::from_millis(2500)).await;
    let _ = tx.send(());
    let outcome = handle.await.expect("run_secure task must not panic");

    assert!(
        outcome.is_ok(),
        "a server envelope rejection must be non-fatal (warn + continue), got {outcome:?}"
    );
    assert_eq!(
        mock.received_count(),
        0,
        "rejected envelopes must not be recorded as accepted"
    );
}
