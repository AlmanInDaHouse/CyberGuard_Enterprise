//! SPEC-002 AC-004 — On a 200 OK response carrying a fresh `agent_id`, a
//! valid PEM cert, and ISO 8601 timestamps, the agent writes all three
//! artifacts: `cert.pem`, `key.dat`, `identity.json`.

mod common;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn enroll_ac_004_persists_artifacts() {
    let mock = common::MockServer::start().await;
    let fixture = common::enrollment_fixture(&mock.base_url, "tok-ac-004");

    cg_agent::identity::ensure_identity(&fixture.config, &fixture.config_path)
        .await
        .expect("enrollment should succeed");

    assert!(
        fixture.cert_path.exists(),
        "cert.pem must exist after enroll"
    );
    assert!(fixture.key_path.exists(), "key.dat must exist after enroll");
    assert!(
        fixture.identity_path.exists(),
        "identity.json must exist after enroll"
    );
}
