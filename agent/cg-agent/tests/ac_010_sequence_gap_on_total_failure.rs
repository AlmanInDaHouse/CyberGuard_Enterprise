//! SPEC-001 AC-010 — When the mock server rejects every retry of
//! heartbeat N (so heartbeat N is never accepted), the next
//! scheduling tick still arrives and the next received heartbeat
//! carries `sequence_number = N + 1`. Sequence gaps observed at
//! the server indicate undelivered heartbeats, not retries.

mod common;

use std::time::Duration;
use tokio::sync::oneshot;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ac_010_sequence_gap_on_total_failure() {
    let mock = common::MockServer::start().await;
    // Reject the first heartbeat's full retry budget (max_retries = 3
    // → 3 rejections), then accept everything from there onward.
    mock.set_response_plan(vec![503, 503, 503]);

    let config = common::config_with_url(&mock.base_url, 1);

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        cg_agent::run(config, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });

    // Wait for the next-tick heartbeat to land.
    for _ in 0..200 {
        if mock.received_count() >= 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let received = mock.received();
    let _ = shutdown_tx.send(());
    let _ = handle.await;

    assert!(
        !received.is_empty(),
        "expected at least one accepted heartbeat after the dropped one"
    );
    let first_accepted = received[0]["sequence_number"]
        .as_u64()
        .expect("sequence_number is u64");
    assert_eq!(
        first_accepted, 2,
        "after heartbeat #1 was dropped, first accepted must be sequence_number=2 (got {first_accepted})"
    );
}
