//! SPEC-002 AC-010 *(Windows)* — After a successful enrollment, the
//! persisted `cert.pem`, `key.dat`, and `identity.json` have filesystem
//! ACLs that exclude all principals other than the owner and SYSTEM, each
//! with full control (NFR-003). `#[cfg(windows)]`.
//!
//! The DACL is read as SDDL with `icacls /save`, and the SID of every
//! entry is normalized to its full form with `ConvertStringSidToSidW` +
//! `ConvertSidToStringSidW`, which understand the SDDL aliases (`SY`, and
//! `LA` for a RID-500 account) and never depend on localized names or on
//! the shell that launched the tests. The DACL must be protected (`P`)
//! and hold exactly two entries, the current user's SID and S-1-5-18,
//! each an allow entry (`A`), not inherited (no `ID`), with full control
//! (`FA`). The same code runs locally and on the CI runner.
//!
//! A second test gives the artifacts an explicit entry for another
//! principal (BUILTIN\Users, S-1-5-32-545) before they are persisted: the
//! hardening must remove it, not only the inherited entries.

mod common;

/// The current user's SID, from `whoami /user /fo csv /nh`
/// (`"domain\user","S-1-5-21-..."`), normalized.
#[cfg(windows)]
fn current_user_sid() -> String {
    let output = std::process::Command::new("whoami")
        .args(["/user", "/fo", "csv", "/nh"])
        .output()
        .expect("failed to run whoami");
    let text = String::from_utf8_lossy(&output.stdout);
    let sid = text
        .trim()
        .rsplit(',')
        .next()
        .map(|field| field.trim_matches('"').to_string())
        .filter(|sid| sid.starts_with("S-1-"))
        .unwrap_or_else(|| panic!("no SID in whoami output: {text}"));
    full_sid(&sid)
}

/// A SID string — full (`S-1-...`) or an SDDL alias (`SY`, `LA`, `BU`) —
/// in its full form, through the Win32 conversions.
#[cfg(windows)]
fn full_sid(sid: &str) -> String {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSidToSidW,
    };
    use windows_sys::Win32::Security::PSID;

    let wide: Vec<u16> = sid.encode_utf16().chain(std::iter::once(0)).collect();
    let mut binary: PSID = std::ptr::null_mut();
    // SAFETY: `wide` is NUL-terminated; `binary` receives a LocalAlloc'd
    // SID freed below.
    if unsafe { ConvertStringSidToSidW(wide.as_ptr(), &mut binary) } == 0 {
        panic!(
            "ConvertStringSidToSidW({sid}): {}",
            std::io::Error::last_os_error()
        );
    }
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: `binary` is a valid SID; `text` receives a LocalAlloc'd,
    // NUL-terminated string freed below.
    let ok = unsafe { ConvertSidToStringSidW(binary, &mut text) };
    let error = std::io::Error::last_os_error();
    // SAFETY: allocated by ConvertStringSidToSidW.
    unsafe { LocalFree(binary.cast()) };
    if ok == 0 {
        panic!("ConvertSidToStringSidW({sid}): {error}");
    }
    // SAFETY: `text` is NUL-terminated; it is read before being freed.
    let full = unsafe {
        let len = (0..).take_while(|&i| *text.add(i) != 0).count();
        let full = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
        LocalFree(text.cast());
        full
    };
    full
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
    let line = text
        .lines()
        .map(|l| l.trim_start_matches('\u{feff}').trim())
        .find(|l| l.contains("D:"))
        .unwrap_or_else(|| panic!("no DACL in icacls /save output: {text}"));
    line[line.find("D:").expect("D: present")..].to_string()
}

/// One DACL entry, its SID in full form.
#[cfg(windows)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    ace_type: String,
    flags: String,
    rights: String,
    sid: String,
}

/// The DACL of `path`: whether it is protected, and its entries.
#[cfg(windows)]
fn read_dacl(path: &std::path::Path) -> (bool, Vec<Entry>, String) {
    let sddl = dacl_sddl(path);
    let body = &sddl[2..];
    let first_ace = body.find('(').unwrap_or(body.len());
    let protected = body[..first_ace].contains('P');
    let entries = body[first_ace..]
        .split('(')
        .filter(|ace| !ace.is_empty())
        .map(|ace| {
            // (type;flags;rights;object_guid;inherit_object_guid;sid)
            let fields: Vec<&str> = ace.trim_end_matches(')').split(';').collect();
            assert!(fields.len() >= 6, "unexpected ACE ({ace} in {sddl})");
            Entry {
                ace_type: fields[0].to_string(),
                flags: fields[1].to_string(),
                rights: fields[2].to_string(),
                sid: full_sid(fields[5]),
            }
        })
        .collect();
    (protected, entries, sddl)
}

/// Assert the DACL is protected and holds exactly two entries, the
/// current user's SID and SYSTEM, each allow, not inherited, full control.
#[cfg(windows)]
fn assert_owner_only_dacl(path: &std::path::Path, user_sid: &str) {
    let (protected, entries, sddl) = read_dacl(path);
    assert!(
        protected,
        "DACL on {} is not protected (inherited entries possible): {sddl}",
        path.display()
    );
    let mut sids: Vec<&str> = entries.iter().map(|e| e.sid.as_str()).collect();
    sids.sort_unstable();
    let mut expected = vec!["S-1-5-18", user_sid];
    expected.sort_unstable();
    assert_eq!(
        sids,
        expected,
        "DACL on {} must hold exactly the current user and SYSTEM: {sddl} -> {entries:?}",
        path.display()
    );
    for entry in &entries {
        assert!(
            entry.ace_type == "A" && !entry.flags.contains("ID") && entry.rights == "FA",
            "every entry on {} must be allow, not inherited, full control: {sddl} -> {entries:?}",
            path.display()
        );
    }
}

/// The SDDL aliases the DACL may carry normalize to full SIDs on every
/// machine, so the reader's alias path runs locally too.
#[cfg(windows)]
#[test]
fn enroll_ac_010_sddl_aliases_normalize_to_full_sids() {
    assert_eq!(full_sid("SY"), "S-1-5-18");
    assert_eq!(full_sid("BU"), "S-1-5-32-545");
    assert_eq!(full_sid("S-1-5-18"), "S-1-5-18");
    let local_admin = full_sid("LA");
    assert!(
        local_admin.starts_with("S-1-5-21-") && local_admin.ends_with("-500"),
        "LA is the local RID-500 account: {local_admin}"
    );
}

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

/// An explicit entry for another principal on the artifacts before they
/// are persisted (BUILTIN\Users, read) is gone afterwards: the hardening
/// replaces the whole DACL, not only the inherited entries.
#[cfg(windows)]
#[test]
fn enroll_ac_010_hardening_removes_other_explicit_entries() {
    const USERS_SID: &str = "S-1-5-32-545";
    let fixture = common::enrollment_fixture("http://127.0.0.1:9", "tok-ac-010-explicit");
    let paths = [
        &fixture.cert_path,
        &fixture.key_path,
        &fixture.identity_path,
    ];
    for path in paths {
        std::fs::write(path, b"placeholder").expect("pre-create artifact");
        let status = std::process::Command::new("icacls")
            .arg(path)
            .arg("/grant")
            .arg(format!("*{USERS_SID}:(R)"))
            .stdout(std::process::Stdio::null())
            .status()
            .expect("failed to run icacls");
        assert!(
            status.success(),
            "icacls /grant failed on {}",
            path.display()
        );
        let (_, before, sddl) = read_dacl(path);
        assert!(
            before
                .iter()
                .any(|e| e.sid == USERS_SID && !e.flags.contains("ID")),
            "setup: an explicit Users entry on {}: {sddl}",
            path.display()
        );
    }

    let keypair = cg_agent::crypto::AgentKeypair::from_secret_bytes(&[7u8; 32]);
    let enrolled = cg_agent::enrollment::EnrolledIdentity {
        agent_id: common::TEST_AGENT_ID.to_string(),
        client_certificate_pem: common::TEST_CLIENT_CERT_PEM.to_string(),
        issued_at: "2026-10-04T00:00:00.000Z".to_string(),
        expires_at: "2027-01-02T00:00:00.000Z".to_string(),
        secret_seed: keypair.secret_seed(),
        public_key: keypair.public_key_bytes(),
    };
    cg_agent::identity::persist_identity(&fixture.config, &enrolled)
        .expect("persist and harden the artifacts");

    let user_sid = current_user_sid();
    for path in paths {
        assert_owner_only_dacl(path, &user_sid);
    }
}
