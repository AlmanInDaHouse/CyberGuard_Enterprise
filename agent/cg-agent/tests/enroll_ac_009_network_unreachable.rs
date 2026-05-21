//! SPEC-002 AC-009 — When the server is unreachable at the network layer
//! for every retry, the agent exits with code 4, the error renders the
//! substring `server unreachable after`, and no identity files are
//! written.

mod common;

use cg_agent::errors::EnrollmentError;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn enroll_ac_009_network_unreachable() {
    // A URL whose port is closed: connection attempts are refused.
    let url = common::closed_port_url().await;
    let fixture = common::enrollment_fixture(&url, "tok-ac-009");

    let err = cg_agent::identity::ensure_identity(&fixture.config, &fixture.config_path)
        .await
        .expect_err("a refused connection must exhaust retries");

    assert_eq!(err.exit_code(), 4, "network unreachable is exit code 4");
    assert!(
        matches!(err, EnrollmentError::Unreachable { .. }),
        "expected Unreachable, got {err:?}"
    );
    assert!(
        format!("{err}").contains("server unreachable after"),
        "error must carry the documented substring; got: {err}"
    );
    assert!(
        fixture.no_artifacts_exist(),
        "no identity files may be written when the server is unreachable"
    );
}
