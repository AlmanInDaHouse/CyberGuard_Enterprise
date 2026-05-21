//! SPEC-002 AC-011 — After a successful first run, `agent.toml` no longer
//! contains the `enrollment.token` value (FR-014 hygiene). A second run
//! started from the hygienized config, with the persisted identity
//! present, proceeds without enrollment and without error.

mod common;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn enroll_ac_011_token_hygiene() {
    let mock = common::MockServer::start().await;
    let secret = "tok-ac-011-secret-value";
    let fixture = common::enrollment_fixture(&mock.base_url, secret);

    // Sanity: the token is on disk before the first run.
    let before = std::fs::read_to_string(&fixture.config_path).expect("read agent.toml");
    assert!(
        before.contains(secret),
        "fixture should write the token before the first run"
    );

    // First run enrolls and hygienizes the on-disk token (FR-014).
    cg_agent::identity::ensure_identity(&fixture.config, &fixture.config_path)
        .await
        .expect("first-run enrollment should succeed");

    let after = std::fs::read_to_string(&fixture.config_path).expect("read agent.toml");
    assert!(
        !after.contains(secret),
        "enrollment.token must be hygienized from agent.toml after first run"
    );

    // Second run: reload the hygienized config; identity present ⇒ load,
    // not enroll, and no error.
    let reloaded =
        cg_agent::config::load_from_path(&fixture.config_path).expect("hygienized config is valid");
    mock.reset_enroll();
    cg_agent::identity::ensure_identity(&reloaded, &fixture.config_path)
        .await
        .expect("second run must load the persisted identity without error");

    assert_eq!(
        mock.enroll_received_count(),
        0,
        "second run must not re-enroll"
    );
}
