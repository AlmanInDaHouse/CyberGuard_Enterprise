//! SPEC-002 AC-005 — On a second run with the three identity files
//! present, the agent does NOT POST to `/v1/agents/enroll`, loads the
//! existing identity, and proceeds (returning the persisted `agent_id`).

mod common;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn enroll_ac_005_idempotent_second_run() {
    let mock = common::MockServer::start().await;
    let fixture = common::enrollment_fixture(&mock.base_url, "tok-ac-005");

    // First run enrolls.
    cg_agent::identity::ensure_identity(&fixture.config, &fixture.config_path)
        .await
        .expect("first-run enrollment should succeed");
    assert_eq!(mock.enroll_received_count(), 1, "first run enrolls once");

    // Second run must load the persisted identity without enrolling.
    mock.reset_enroll();
    let loaded = cg_agent::identity::ensure_identity(&fixture.config, &fixture.config_path)
        .await
        .expect("second-run load should succeed");

    assert_eq!(
        mock.enroll_received_count(),
        0,
        "second run must NOT POST to /v1/agents/enroll"
    );
    assert_eq!(
        loaded.agent_id,
        common::TEST_AGENT_ID,
        "loaded identity must carry the server-assigned agent_id"
    );
}
