//! SPEC-002 AC-010 *(Windows)* — After a successful enrollment, the
//! persisted `cert.pem`, `key.dat`, and `identity.json` have filesystem
//! ACLs that exclude all principals other than the owner and SYSTEM
//! (NFR-003). Verified by inspecting `icacls` output. `#[cfg(windows)]`.

mod common;

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn enroll_ac_010_owner_only_acls() {
    let mock = common::MockServer::start().await;
    let fixture = common::enrollment_fixture(&mock.base_url, "tok-ac-010");

    cg_agent::identity::ensure_identity(&fixture.config, &fixture.config_path)
        .await
        .expect("enrollment should succeed");

    assert_owner_only_acl(&fixture.cert_path);
    assert_owner_only_acl(&fixture.key_path);
    assert_owner_only_acl(&fixture.identity_path);
}

/// Assert that `icacls <path>` grants access to no principal other than
/// the current user and `NT AUTHORITY\SYSTEM` — in particular none of the
/// broad principals an inherited or default ACL would carry.
#[cfg(windows)]
fn assert_owner_only_acl(path: &std::path::Path) {
    let output = std::process::Command::new("icacls")
        .arg(path)
        .output()
        .expect("failed to run icacls");
    let text = String::from_utf8_lossy(&output.stdout);

    for forbidden in [
        "Everyone",
        "BUILTIN\\Users",
        "Authenticated Users",
        "BUILTIN\\Administrators",
    ] {
        assert!(
            !text.contains(forbidden),
            "ACL on {} unexpectedly grants `{forbidden}`:\n{text}",
            path.display()
        );
    }

    let user = std::env::var("USERNAME").unwrap_or_default();
    assert!(
        (!user.is_empty() && text.contains(&user)) || text.contains("NT AUTHORITY\\SYSTEM"),
        "ACL on {} grants neither the owner nor SYSTEM:\n{text}",
        path.display()
    );
}
