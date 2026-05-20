//! SPEC-001 AC-002 — Given a valid config and a reachable mock server,
//! the agent sends its first heartbeat within 5 seconds of process start.

mod common;

use std::time::{Duration, Instant};
use tokio::sync::oneshot;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ac_002_first_heartbeat_within_5s() {
    let mock = common::MockServer::start().await;
    let config = common::config_with_url(&mock.base_url, 30);

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let started = Instant::now();
    let handle = tokio::spawn(async move {
        cg_agent::run(config, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });

    // Wait up to 5 s for the first heartbeat to land on the mock.
    let deadline = started + Duration::from_secs(5);
    while Instant::now() < deadline && mock.received_count() == 0 {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let count = mock.received_count();
    let elapsed = started.elapsed();

    let _ = shutdown_tx.send(());
    let _ = handle.await;

    assert!(
        count >= 1,
        "expected at least one heartbeat within 5s; got {count} (elapsed {elapsed:?})"
    );
}
