//! SPEC-002 AC-001 — Given an `agent.toml` with a valid `enrollment.token`
//! and no persisted identity, the agent enrolls successfully against a mock
//! at `/v1/agents/enroll` within the timeout budget, then transitions into
//! the SPEC-001 heartbeat loop and delivers at least one heartbeat.

mod common;

use std::time::Duration;
use tokio::sync::oneshot;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn enroll_ac_001_enroll_then_heartbeat() {
    let mock = common::MockServer::start().await;
    let fixture = common::enrollment_fixture(&mock.base_url, "tok-ac-001");

    // Enrollment must complete within the configured timeout budget.
    let identity = tokio::time::timeout(
        Duration::from_secs(30),
        cg_agent::identity::ensure_identity(&fixture.config, &fixture.config_path),
    )
    .await
    .expect("enrollment exceeded the timeout budget")
    .expect("enrollment should succeed against the mock");

    assert_eq!(
        mock.enroll_received_count(),
        1,
        "exactly one POST to /v1/agents/enroll"
    );
    assert!(
        fixture.all_artifacts_exist(),
        "identity artifacts must be persisted before heartbeating"
    );

    // Transition into the SPEC-001 heartbeat loop using the resolved id.
    let mut hb_config = fixture.config.clone();
    hb_config.agent.id = identity.agent_id.clone();
    hb_config.heartbeat.interval_seconds = 1;

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        cg_agent::run(hb_config, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });

    for _ in 0..200 {
        if mock.received_count() >= 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let heartbeats = mock.received_count();
    let _ = shutdown_tx.send(());
    let _ = handle.await;

    assert!(
        heartbeats >= 1,
        "expected ≥1 heartbeat after enrollment, got {heartbeats}"
    );
}
