//! SPEC-002 AC-003 — The enrollment request the mock receives contains a
//! non-empty `enrollment_token` matching the configured value, a
//! base64url-decoded `agent_pubkey` of exactly 32 bytes, the configured
//! `agent_hostname`, the runtime `agent_platform`, and the crate
//! `agent_version`.

mod common;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn enroll_ac_003_request_contents() {
    let mock = common::MockServer::start().await;
    let token = "tok-ac-003-opaque";
    let fixture = common::enrollment_fixture(&mock.base_url, token);

    cg_agent::identity::ensure_identity(&fixture.config, &fixture.config_path)
        .await
        .expect("enrollment should succeed");

    let requests = mock.enroll_received();
    assert_eq!(requests.len(), 1, "exactly one enrollment request");
    let req = &requests[0];

    assert_eq!(
        req["enrollment_token"].as_str(),
        Some(token),
        "enrollment_token must match the configured value"
    );

    let pubkey = common::base64url_decode(
        req["agent_pubkey"]
            .as_str()
            .expect("agent_pubkey must be a string"),
    );
    assert_eq!(pubkey.len(), 32, "agent_pubkey must decode to 32 bytes");

    assert_eq!(
        req["agent_hostname"].as_str(),
        Some("FIN-PC-014"),
        "agent_hostname must match config"
    );
    assert_eq!(
        req["agent_platform"].as_str(),
        Some(cg_agent::detect_platform()),
        "agent_platform must be the runtime platform"
    );
    assert_eq!(
        req["agent_version"].as_str(),
        Some(env!("CARGO_PKG_VERSION")),
        "agent_version must be the crate version"
    );
}
