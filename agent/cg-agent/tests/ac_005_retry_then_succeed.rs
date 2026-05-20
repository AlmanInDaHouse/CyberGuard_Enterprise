//! SPEC-001 AC-005 — When the mock server returns HTTP 503 on the
//! first 2 attempts of a heartbeat and HTTP 200 on the 3rd, the agent
//! eventually delivers that heartbeat and the next interval still ticks.

mod common;

use std::time::Duration;
use tokio::sync::oneshot;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ac_005_retry_then_succeed() {
    let mock = common::MockServer::start().await;
    // First two attempts of the first heartbeat fail; third succeeds.
    // All subsequent calls succeed by default.
    mock.set_response_plan(vec![503, 503, 200]);

    let config = common::config_with_url(&mock.base_url, 1);

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        cg_agent::run(config, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });

    // Wait for at least 2 accepted heartbeats (the retried-then-accepted
    // one plus the next interval).
    for _ in 0..200 {
        if mock.received_count() >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let count = mock.received_count();
    let _ = shutdown_tx.send(());
    let _ = handle.await;

    assert!(
        count >= 2,
        "expected ≥2 accepted heartbeats after the retry recovery, got {count}"
    );
}
