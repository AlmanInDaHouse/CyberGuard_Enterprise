//! SPEC-002 AC-006 — When the mock returns 401, the agent fails
//! enrollment with exit code 3, the error renders the substring
//! `token rejected by server`, and no identity files are written.

mod common;

use cg_agent::errors::EnrollmentError;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn enroll_ac_006_token_rejected() {
    let mock = common::MockServer::start().await;
    mock.set_enroll_plan(vec![401]);
    let fixture = common::enrollment_fixture(&mock.base_url, "rejected-token");

    let err = cg_agent::identity::ensure_identity(&fixture.config, &fixture.config_path)
        .await
        .expect_err("a 401 must fail enrollment");

    assert_eq!(err.exit_code(), 3, "token rejection is exit code 3");
    assert!(
        matches!(err, EnrollmentError::Refused(_)),
        "expected Refused, got {err:?}"
    );
    assert!(
        format!("{err}").contains("token rejected by server"),
        "error must carry the documented substring; got: {err}"
    );
    assert!(
        fixture.no_artifacts_exist(),
        "no identity files may be written on rejection"
    );
}
