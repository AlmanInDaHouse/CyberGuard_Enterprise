//! SPEC-001 AC-007 — When the agent receives the shutdown signal
//! (SIGINT / Ctrl+C in production, an injected future in tests), it
//! sends one final heartbeat with `status = "going_offline"` and
//! exits within 2 seconds.

mod common;

use std::time::{Duration, Instant};
use tokio::sync::oneshot;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ac_007_shutdown_final_heartbeat() {
    let mock = common::MockServer::start().await;
    let config = common::config_with_url(&mock.base_url, 30);

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        cg_agent::run(config, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });

    // Wait for the initial heartbeat (online).
    for _ in 0..100 {
        if mock.received_count() >= 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let trigger = Instant::now();
    let _ = shutdown_tx.send(());

    let _ = tokio::time::timeout(Duration::from_secs(3), handle).await;
    let total_elapsed = trigger.elapsed();
    let received = mock.received();

    assert!(
        received.iter().any(|e| e["status"] == "going_offline"),
        "no heartbeat carried status=\"going_offline\"; received: {:?}",
        received
            .iter()
            .map(|e| e["status"].clone())
            .collect::<Vec<_>>()
    );
    assert!(
        total_elapsed < Duration::from_secs(2),
        "graceful shutdown took {:?}, exceeds the 2 s bound",
        total_elapsed
    );
}
