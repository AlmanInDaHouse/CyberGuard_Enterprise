//! SPEC-001 AC-006 — When the mock server is unreachable for the
//! entire `max_retries` window of a heartbeat, the agent continues
//! to the next interval (does NOT exit).

mod common;

use std::time::Duration;
use tokio::sync::oneshot;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ac_006_continues_after_retry_exhaustion() {
    let mock = common::MockServer::start().await;
    // Reject the first heartbeat's attempts (max_retries = 3 in
    // common::config_with_url, so 3 × 503 exhausts it). Subsequent
    // requests succeed.
    mock.set_response_plan(vec![503, 503, 503]);

    let config = common::config_with_url(&mock.base_url, 1);

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        cg_agent::run(config, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });

    // Wait long enough for: first heartbeat retries exhausted (~0.4 s
    // of backoffs at the test cadence) + next interval tick (1 s).
    // Total ~2 s headroom.
    for _ in 0..150 {
        if mock.received_count() >= 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let count = mock.received_count();
    let _ = shutdown_tx.send(());
    let result = handle.await;

    assert!(
        count >= 1,
        "expected agent to continue and deliver the next heartbeat after retry exhaustion, got {count}"
    );
    assert!(
        result.is_ok(),
        "agent panicked instead of continuing after retry exhaustion"
    );
}
