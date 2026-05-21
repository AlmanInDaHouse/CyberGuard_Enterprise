//! SPEC-002 AC-007 — When the mock returns 409, the agent fails
//! enrollment with exit code 3, the error renders the substring
//! `token already used`, and no identity files are written.

mod common;

use cg_agent::errors::EnrollmentError;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn enroll_ac_007_token_already_used() {
    let mock = common::MockServer::start().await;
    mock.set_enroll_plan(vec![409]);
    let fixture = common::enrollment_fixture(&mock.base_url, "used-token");

    let err = cg_agent::identity::ensure_identity(&fixture.config, &fixture.config_path)
        .await
        .expect_err("a 409 must fail enrollment");

    assert_eq!(err.exit_code(), 3, "token-already-used is exit code 3");
    assert!(
        matches!(err, EnrollmentError::Refused(_)),
        "expected Refused, got {err:?}"
    );
    assert!(
        format!("{err}").contains("token already used"),
        "error must carry the documented substring; got: {err}"
    );
    assert!(
        fixture.no_artifacts_exist(),
        "no identity files may be written on conflict"
    );
}
