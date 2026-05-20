//! SPEC-001 AC-004 — Heartbeat envelopes received by the mock server
//! have `sequence_number` values that are monotonically increasing by
//! exactly 1, starting at 1.

mod common;

use std::time::Duration;
use tokio::sync::oneshot;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ac_004_sequence_monotonic() {
    let mock = common::MockServer::start().await;
    let config = common::config_with_url(&mock.base_url, 1);

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        cg_agent::run(config, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });

    // Wait for at least 3 heartbeats.
    for _ in 0..100 {
        if mock.received_count() >= 3 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let received = mock.received();
    let _ = shutdown_tx.send(());
    let _ = handle.await;

    assert!(
        received.len() >= 3,
        "expected ≥3 heartbeats, got {}",
        received.len()
    );
    for (idx, env) in received.iter().take(3).enumerate() {
        let expected = (idx as u64) + 1;
        let got = env["sequence_number"]
            .as_u64()
            .expect("sequence_number is u64");
        assert_eq!(
            got,
            expected,
            "heartbeat #{} expected sequence_number {}, got {}",
            idx + 1,
            expected,
            got
        );
    }
}
