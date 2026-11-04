//! SPEC-024 install_ac_004 and install_ac_005, the custody check (its
//! Amendment 2026-11-04, items 3, 4 and 6), unelevated: an object is opened
//! without following a reparse point and checked through that handle — a
//! directory where one is expected, otherwise a regular file with one link;
//! not a reparse point; owned by one of the policy's owners; a protected
//! DACL, not null, whose allow entries name only the policy's principals.
//! What `custody` creates has its final security at creation, and the
//! token-hygiene replacement keeps the original's owner and DACL.
//!
//! The policies here name the current user where the service's name
//! SYSTEM (`custody_fixtures`); the service's own policies are asserted
//! against a file this user owns, which they refuse.
#![cfg(windows)]

mod custody_fixtures;

use std::path::Path;

use cg_agent::custody::{self, Kind, Policy};
use custody_fixtures::{
    default_owner_sid, normalized, owner_and_dacl, sddl, user, user_only, user_sid,
};

fn file(path: &Path, security: &str, bytes: &[u8]) {
    custody::create_new_file(path, security, bytes).expect("create the fixture");
}

/// Objects the policy allows pass; their bytes are read from the checked
/// handle; an absent one is reported absent.
#[test]
fn install_ac_004_an_object_the_policy_allows_passes() {
    let root = tempfile::tempdir().expect("tempdir");
    let dir = root.path().join("identity");
    custody::ensure_directory(&dir, &sddl(true, ""), &user_only()).expect("create");
    let path = dir.join("key.dat");
    file(&path, &sddl(false, ""), b"sealed");

    custody::check(&dir, Kind::Directory, &user_only()).expect("the directory");
    custody::check(&path, Kind::File, &user_only()).expect("the file");
    assert_eq!(
        custody::read_checked(&path, &user_only()).expect("read"),
        Some(b"sealed".to_vec())
    );
    // A deny entry may name anyone.
    let denied = dir.join("denied.dat");
    file(&denied, &sddl(false, "(D;;FA;;;BU)"), b"x");
    custody::check(&denied, Kind::File, &user_only()).expect("a deny entry is accepted");

    let absent = dir.join("absent.dat");
    assert!(!custody::check_if_exists(&absent, Kind::File, &user_only()).expect("absent"));
    assert_eq!(
        custody::read_checked(&absent, &user_only()).expect("absent"),
        None
    );
    let error = custody::check(&absent, Kind::File, &user_only()).expect_err("absent");
    assert!(error.contains("does not exist"), "{error}");
}

/// Each condition of item 3, refused with its reason and the path.
#[test]
fn install_ac_004_each_custody_condition_is_refused() {
    let root = tempfile::tempdir().expect("tempdir");
    let refused = |path: &Path, kind: Kind, policy: &Policy, reason: &str| {
        let error = custody::check(path, kind, policy)
            .expect_err(&format!("{} is refused", path.display()));
        assert!(
            error.contains(reason) && error.contains(&path.display().to_string()),
            "{} → {error}",
            path.display()
        );
        if kind == Kind::File {
            let read = custody::read_checked(path, policy).expect_err("not read");
            assert!(read.contains(reason), "{read}");
        }
    };

    let readable = root.path().join("readable.toml");
    file(&readable, &sddl(false, "(A;;FR;;;BU)"), b"token");
    refused(
        &readable,
        Kind::File,
        &user_only(),
        "allows access to a principal other than",
    );

    // A DACL given at creation is protected; one only inherited is not.
    let inheriting = root.path().join("inheriting");
    custody::ensure_directory(&inheriting, &sddl(true, ""), &user_only()).expect("dir");
    let unprotected = inheriting.join("unprotected.toml");
    std::fs::write(&unprotected, b"x").expect("an inherited DACL");
    assert!(
        !owner_and_dacl(&unprotected).contains("D:P"),
        "not protected"
    );
    refused(&unprotected, Kind::File, &user_only(), "is not protected");

    let null_dacl = root.path().join("null.toml");
    file(&null_dacl, "D:NO_ACCESS_CONTROL", b"x");
    refused(&null_dacl, Kind::File, &user_only(), "has no DACL");

    // The service's policies want SYSTEM as the owner; this user owns it.
    let mine = root.path().join("mine.toml");
    file(&mine, &sddl(false, "(A;;FA;;;SY)"), b"x");
    refused(
        &mine,
        Kind::File,
        &Policy::configuration(),
        "is not owned by SYSTEM",
    );
    refused(
        &mine,
        Kind::File,
        &Policy::system_only(),
        "is not owned by SYSTEM",
    );
    let other_owner = Policy {
        owners: vec![custody::Principal::System],
        ..user_only()
    };
    refused(&mine, Kind::File, &other_owner, "is not owned by SYSTEM");

    let linked = root.path().join("linked.dat");
    file(&linked, &sddl(false, ""), b"x");
    std::fs::hard_link(&linked, root.path().join("link.dat")).expect("hard link");
    refused(&linked, Kind::File, &user_only(), "is hard-linked");
    refused(
        &root.path().join("link.dat"),
        Kind::File,
        &user_only(),
        "is hard-linked",
    );

    let dir = root.path().join("dir");
    custody::ensure_directory(&dir, &sddl(true, ""), &user_only()).expect("dir");
    refused(&dir, Kind::File, &user_only(), "is not a regular file");
    let plain = root.path().join("plain.dat");
    file(&plain, &sddl(false, ""), b"x");
    refused(&plain, Kind::Directory, &user_only(), "is not a directory");

    // A junction is a reparse point; it is refused, not followed.
    let junction = root.path().join("junction");
    let status = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&junction)
        .arg(&dir)
        .stdout(std::process::Stdio::null())
        .status()
        .expect("run mklink");
    assert!(status.success(), "mklink /J");
    refused(
        &junction,
        Kind::Directory,
        &user_only(),
        "is a reparse point",
    );
}

/// Creation: `ensure_directory` creates with its security, or checks what
/// is there; `create_new_file` never writes over a file.
#[test]
fn install_ac_005_what_custody_creates_has_its_final_security() {
    let root = tempfile::tempdir().expect("tempdir");
    let dir = root.path().join("identity");
    custody::ensure_directory(&dir, &sddl(true, ""), &user_only()).expect("create");
    let expected_dir = normalized(&format!(
        "O:{}D:P(A;OICI;FA;;;{})",
        default_owner_sid(),
        user_sid()
    ));
    assert_eq!(owner_and_dacl(&dir), expected_dir, "no inherited entry");
    custody::ensure_directory(&dir, &sddl(true, ""), &user_only()).expect("checked");

    let path = dir.join("cert.pem");
    file(&path, &sddl(false, ""), b"first");
    assert_eq!(
        owner_and_dacl(&path),
        normalized(&format!(
            "O:{}D:P(A;;FA;;;{})",
            default_owner_sid(),
            user_sid()
        ))
    );
    let error = custody::create_new_file(&path, &sddl(false, ""), b"second")
        .expect_err("never over an existing file");
    assert!(
        error.contains("Win32 error 80"),
        "ERROR_FILE_EXISTS: {error}"
    );
    assert_eq!(std::fs::read(&path).expect("read"), b"first");

    let broad = root.path().join("broad");
    custody::ensure_directory(&broad, &sddl(true, "(A;OICI;FR;;;BU)"), &user_only())
        .expect("created as asked");
    let error = custody::ensure_directory(&broad, &sddl(true, ""), &user_only())
        .expect_err("an existing directory is checked");
    assert!(error.contains("allows access"), "{error}");
}

/// Item 6: the replacement keeps the original's owner and DACL, through a
/// new temporary file; a leftover temporary file is deleted first.
#[test]
fn install_ac_005_the_replacement_keeps_owner_and_dacl() {
    let root = tempfile::tempdir().expect("tempdir");
    let path = root.path().join("agent.toml");
    file(&path, &sddl(false, "(A;;FR;;;SY)"), b"token = \"t\"\n");
    let before = owner_and_dacl(&path);
    let temporary = root.path().join("agent.toml.hygiene-tmp");
    std::fs::write(&temporary, b"leftover").expect("a leftover temporary file");

    let policy = Policy {
        allowed: vec![user(), custody::Principal::System],
        ..user_only()
    };
    // A file the policy refuses is not copied.
    let broad = root.path().join("broad.toml");
    file(&broad, &sddl(false, "(A;;FR;;;BU)"), b"token");
    let error = custody::replace_keeping_security(&broad, &temporary, b"x", &policy)
        .expect_err("a file others can read is not replaced");
    assert!(error.contains("allows access"), "{error}");
    assert_eq!(std::fs::read(&broad).expect("read"), b"token");

    custody::replace_keeping_security(&path, &temporary, b"rewritten\n", &policy).expect("replace");
    assert_eq!(std::fs::read(&path).expect("read"), b"rewritten\n");
    assert_eq!(owner_and_dacl(&path), before, "owner and DACL kept");
    assert!(!temporary.exists(), "the temporary file is gone");
    custody::check(&path, Kind::File, &policy).expect("still passes");
}

/// A policy's SID that is not a whole SID matches nothing, and is never
/// read past its end.
#[test]
fn install_ac_004_a_malformed_policy_sid_matches_nothing() {
    let root = tempfile::tempdir().expect("tempdir");
    let path = root.path().join("key.dat");
    file(&path, &sddl(false, ""), b"x");
    for bytes in [vec![], vec![1, 5], vec![1, 5, 0, 0, 0, 0, 0, 5, 21]] {
        let policy = Policy {
            owners: vec![custody::Principal::Sid(bytes.clone())],
            ..user_only()
        };
        let error = custody::check(&path, Kind::File, &policy).expect_err("no owner matches");
        assert!(error.contains("is not owned by"), "{bytes:?}: {error}");
    }
}
