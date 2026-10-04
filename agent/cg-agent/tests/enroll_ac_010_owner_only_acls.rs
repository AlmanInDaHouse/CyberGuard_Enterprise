//! SPEC-002 AC-010 *(Windows)* — After a successful enrollment, the
//! persisted `cert.pem`, `key.dat`, and `identity.json` have filesystem
//! ACLs that exclude all principals other than the owner and SYSTEM
//! (NFR-003). `#[cfg(windows)]`.
//!
//! Checked by SID, not by name: principal names are localized
//! (`BUILTIN\Administradores` on a Spanish Windows), so the DACL is read
//! as SDDL with `icacls /save` and must be protected (no inherited
//! entries) and hold exactly two entries: the current user's SID and
//! SYSTEM (`SY`).

mod common;

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn enroll_ac_010_owner_only_acls() {
    let mock = common::MockServer::start().await;
    let fixture = common::enrollment_fixture(&mock.base_url, "tok-ac-010");

    cg_agent::identity::ensure_identity(&fixture.config, &fixture.config_path)
        .await
        .expect("enrollment should succeed");

    let user_sid = current_user_sid();
    assert_owner_only_dacl(&fixture.cert_path, &user_sid);
    assert_owner_only_dacl(&fixture.key_path, &user_sid);
    assert_owner_only_dacl(&fixture.identity_path, &user_sid);
}

/// The current user's SID, from `whoami /user /fo csv /nh`
/// (`"domain\user","S-1-5-21-..."`).
#[cfg(windows)]
fn current_user_sid() -> String {
    let output = std::process::Command::new("whoami")
        .args(["/user", "/fo", "csv", "/nh"])
        .output()
        .expect("failed to run whoami");
    let text = String::from_utf8_lossy(&output.stdout);
    text.trim()
        .rsplit(',')
        .next()
        .map(|field| field.trim_matches('"').to_string())
        .filter(|sid| sid.starts_with("S-1-"))
        .unwrap_or_else(|| panic!("no SID in whoami output: {text}"))
}

/// The DACL of `path` as SDDL (`D:...`), from `icacls <path> /save`.
#[cfg(windows)]
fn dacl_sddl(path: &std::path::Path) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let saved = dir.path().join("acl.txt");
    let status = std::process::Command::new("icacls")
        .arg(path)
        .arg("/save")
        .arg(&saved)
        .stdout(std::process::Stdio::null())
        .status()
        .expect("failed to run icacls /save");
    assert!(
        status.success(),
        "icacls /save failed on {}",
        path.display()
    );
    // icacls writes UTF-16LE: the file name, then the security descriptor.
    let bytes = std::fs::read(&saved).expect("read icacls output");
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    let text = String::from_utf16_lossy(&units);
    let sddl = text
        .lines()
        .map(|l| l.trim_start_matches('\u{feff}').trim())
        .find(|l| l.contains("D:"))
        .unwrap_or_else(|| panic!("no DACL in icacls /save output: {text}"));
    sddl[sddl.find("D:").expect("D: present")..].to_string()
}

/// Assert the DACL is protected and holds exactly two entries: one for
/// `user_sid` and one for SYSTEM.
#[cfg(windows)]
fn assert_owner_only_dacl(path: &std::path::Path, user_sid: &str) {
    let sddl = dacl_sddl(path);
    let flags = &sddl[2..sddl.find('(').unwrap_or(sddl.len())];
    assert!(
        flags.contains('P'),
        "DACL on {} is not protected (inherited entries possible): {sddl}",
        path.display()
    );
    // Each ACE is `(type;flags;rights;;;sid)`; the SID is the last field.
    let sids: Vec<&str> = sddl
        .split('(')
        .skip(1)
        .map(|ace| ace.trim_end_matches(')').rsplit(';').next().unwrap_or(""))
        .collect();
    let mut sorted = sids.clone();
    sorted.sort_unstable();
    let mut expected = vec!["SY", user_sid];
    expected.sort_unstable();
    assert_eq!(
        sorted,
        expected,
        "DACL on {} must hold exactly the current user and SYSTEM: {sddl}",
        path.display()
    );
}
