//! SPEC-002 AC-001 — Given an `agent.toml` with a valid `enrollment.token`
//! and no persisted identity, the agent enrolls successfully against a mock
//! at `/v1/agents/enroll` within the timeout budget, then delivers at least
//! one heartbeat. Since SPEC-023 §Operational §6 the enrollment runs over
//! TLS, trusting the trust anchor, and the enrolled agent heartbeats over
//! mTLS with the certificate the server issued for its key (SPEC-002's
//! FR-013, the SPEC-001 heartbeat after enrollment, is removed).

mod common;

use std::time::Duration;
use tokio::sync::oneshot;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn enroll_ac_001_enroll_then_heartbeat() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let mock = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    let fixture = common::enrollment_fixture(&mock.base_url, "tok-ac-001", &pki);

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

    // The secure path with the enrolled identity: the mTLS mock trusts the
    // client certificate the enrollment mock issued, and verifies each
    // envelope under the key the agent generated.
    let heartbeat =
        common::TlsMockServer::start_for_key(&pki, identity.keypair.public_key_bytes()).await;
    let mut config = fixture.config.clone();
    config.agent.id = identity.agent_id.clone();
    config.server.heartbeat_url = Some(heartbeat.base_url.clone());
    config.heartbeat.interval_seconds = 1;

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        cg_agent::run_secure(config, identity, cg_agent::Capture::Off, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });

    let delivered =
        common::wait_until(Duration::from_secs(10), || heartbeat.received_count() >= 1).await;
    let _ = shutdown_tx.send(());
    let outcome = handle.await.expect("run_secure must not panic");

    assert!(
        delivered,
        "expected ≥1 heartbeat over mTLS after enrollment, got {}",
        heartbeat.received_count()
    );
    outcome.expect("the secure path ends cleanly at shutdown");
}
