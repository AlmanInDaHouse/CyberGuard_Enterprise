//! SPEC-002 AC-012 *(POSIX, parked)* — On non-Windows platforms the
//! persisted artifacts have mode `0600`. `#[cfg(unix)]` and `#[ignore]`d:
//! it is parked for the SPEC-003 Linux work and is not part of the
//! Windows-first MVP gate. Run with `cargo test -- --ignored` on a POSIX
//! host once the Linux SecureStore backend lands.

mod common;

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "parked for SPEC-003 Linux work; run with --ignored on a POSIX host"]
async fn enroll_ac_012_posix_mode_0600() {
    use std::os::unix::fs::PermissionsExt;

    let mock = common::MockServer::start().await;
    let fixture = common::enrollment_fixture(&mock.base_url, "tok-ac-012");

    cg_agent::identity::ensure_identity(&fixture.config, &fixture.config_path)
        .await
        .expect("enrollment should succeed");

    for path in [
        &fixture.cert_path,
        &fixture.key_path,
        &fixture.identity_path,
    ] {
        let mode = std::fs::metadata(path)
            .expect("artifact metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode,
            0o600,
            "{} should be mode 0600, got {mode:o}",
            path.display()
        );
    }
}
