//! SPEC-002 AC-008 — When the mock returns 500 for every retry (within
//! `max_retries`), the agent exits with code 4, the error renders the
//! substring `server unreachable after`, and no identity files are
//! written.

mod common;

use cg_agent::errors::EnrollmentError;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn enroll_ac_008_server_5xx_exhausts_retries() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let mock = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    // Every enrollment attempt gets a 500 (default applies once the
    // empty finite plan is exhausted).
    mock.set_enroll_default(500);
    let fixture = common::enrollment_fixture(&mock.base_url, "tok-ac-008", &pki);

    let err = cg_agent::identity::ensure_identity(&fixture.config, &fixture.config_path)
        .await
        .expect_err("a 500 storm must exhaust retries");

    assert_eq!(err.exit_code(), 4, "retry exhaustion is exit code 4");
    assert!(
        matches!(err, EnrollmentError::Unreachable { .. }),
        "expected Unreachable, got {err:?}"
    );
    assert!(
        format!("{err}").contains("server unreachable after"),
        "error must carry the documented substring; got: {err}"
    );
    assert_eq!(
        mock.enroll_received_count(),
        3,
        "each of the max_retries attempts reached the server and got its 500"
    );
    assert!(
        fixture.no_artifacts_exist(),
        "no identity files may be written on retry exhaustion"
    );
}
