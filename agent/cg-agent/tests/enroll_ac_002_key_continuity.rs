//! SPEC-002 AC-002 — End-to-end identity continuity. The 32-byte public
//! key generated on first run equals all of: (a) the `agent_pubkey`
//! decoded from the enrollment request the mock received; (b) the key
//! re-derived after persistence + reload on a second run; and (c) the
//! `agent_pubkey_fingerprint` recorded in `identity.json`.

mod common;

use serde_json::Value;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn enroll_ac_002_key_continuity() {
    let mock = common::MockServer::start().await;
    let fixture = common::enrollment_fixture(&mock.base_url, "tok-ac-002");

    // First run: generate + enroll + persist.
    let first = cg_agent::identity::ensure_identity(&fixture.config, &fixture.config_path)
        .await
        .expect("first-run enrollment should succeed");
    let generated = first.keypair.public_key_bytes();

    // (a) The pubkey on the wire equals the generated key.
    let requests = mock.enroll_received();
    assert_eq!(requests.len(), 1, "exactly one enrollment request");
    let sent_b64 = requests[0]["agent_pubkey"]
        .as_str()
        .expect("agent_pubkey must be a string");
    let sent = common::base64url_decode(sent_b64);
    assert_eq!(
        sent.as_slice(),
        generated.as_slice(),
        "request pubkey must equal the generated pubkey"
    );

    // (b) A second run loads from disk and re-derives the same pubkey.
    mock.reset_enroll();
    let second = cg_agent::identity::ensure_identity(&fixture.config, &fixture.config_path)
        .await
        .expect("second-run load should succeed");
    assert_eq!(
        mock.enroll_received_count(),
        0,
        "second run must not re-enroll"
    );
    assert_eq!(
        second.keypair.public_key_bytes(),
        generated,
        "reloaded pubkey must equal the generated pubkey"
    );

    // (c) identity.json fingerprint equals sha256(pubkey).
    let raw = std::fs::read_to_string(&fixture.identity_path).expect("read identity.json");
    let identity_json: Value = serde_json::from_str(&raw).expect("identity.json is valid JSON");
    let fingerprint = identity_json["agent_pubkey_fingerprint"]
        .as_str()
        .expect("agent_pubkey_fingerprint must be a string");
    assert_eq!(
        fingerprint,
        cg_agent::crypto::pubkey_fingerprint(&generated),
        "identity.json fingerprint must match the generated pubkey"
    );
}
