//! SPEC-024 install_ac_004 and install_ac_005, the custody cases debt #146
//! left untested (its Amendment 2026-11-04, items 3, 4 and 6), unelevated:
//! an ACE of a type the check does not read; a security descriptor that
//! cannot be built or that Windows refuses; a dangling file symlink where a
//! file is created or replaced; a replacement whose move fails. Each leaves
//! what was there as it was.
#![cfg(windows)]

mod custody_fixtures;

use std::path::Path;

use cg_agent::custody::{self, Kind};
use custody_fixtures::{owner_and_dacl, sddl, user_only};

fn file(path: &Path, security: &str, bytes: &[u8]) {
    custody::create_new_file(path, security, bytes).expect("create the fixture");
}

/// Item 3: "an entry of another type is refused". A conditional (callback)
/// allow entry, type 9, is not read as a plain allow entry; the check
/// refuses it. (NTFS stores an object ACE without GUIDs as a plain one, so
/// it would not exercise this.)
#[test]
fn install_ac_004_an_ace_of_another_type_is_refused() {
    let root = tempfile::tempdir().expect("tempdir");
    let path = root.path().join("callback-ace.toml");
    file(
        &path,
        &sddl(false, "(XA;;FR;;;BU;(Member_of {SID(BA)}))"),
        b"x",
    );
    assert!(
        owner_and_dacl(&path).contains("(XA;"),
        "the fixture holds it"
    );
    let error = custody::check(&path, Kind::File, &user_only()).expect_err("refused");
    assert!(error.contains("holds an ACE of type 9"), "{error}");
}

/// Item 4: "no byte of the identity is written under another security". A
/// descriptor that cannot be built creates nothing.
#[test]
fn install_ac_005_a_descriptor_that_cannot_be_built_creates_nothing() {
    let root = tempfile::tempdir().expect("tempdir");
    let path = root.path().join("key.dat");
    let error =
        custody::create_new_file(&path, "not a descriptor", b"sealed").expect_err("refused");
    assert!(error.contains("build a security descriptor"), "{error}");
    assert!(!path.exists(), "nothing created");

    let dir = root.path().join("identity");
    let error =
        custody::ensure_directory(&dir, "not a descriptor", &user_only()).expect_err("refused");
    assert!(error.contains("build a security descriptor"), "{error}");
    assert!(!dir.exists(), "nothing created");
}

/// Item 4: a descriptor that parses but that Windows refuses at creation
/// (owner SYSTEM, which neither this token nor an administrator's may
/// assign while SeRestorePrivilege is not enabled) creates nothing under
/// another security.
#[test]
fn install_ac_005_a_descriptor_windows_refuses_creates_nothing() {
    let root = tempfile::tempdir().expect("tempdir");
    let path = root.path().join("key.dat");
    let error =
        custody::create_new_file(&path, "O:SYD:P(A;;FA;;;SY)", b"sealed").expect_err("refused");
    assert!(error.contains("Win32 error 1307"), "{error}");
    assert!(!path.exists(), "nothing created");

    let dir = root.path().join("identity");
    let error = custody::ensure_directory(&dir, "O:SYD:P(A;OICI;FA;;;SY)", &user_only())
        .expect_err("refused");
    assert!(error.contains("Win32 error 1307"), "{error}");
    assert!(!dir.exists(), "nothing created");
}

/// A file symlink, or `None` when this token may not create one (no
/// Developer Mode, not elevated). rust-ci's Windows job runs as the
/// built-in Administrator, which can: there a missing privilege fails the
/// test instead of skipping it, so a green run there shows it ran.
fn symlink(link: &Path, target: &Path) -> Option<()> {
    match std::os::windows::fs::symlink_file(target, link) {
        Ok(()) => Some(()),
        Err(e) if e.raw_os_error() == Some(1314) => {
            assert!(
                std::env::var_os("GITHUB_ACTIONS").is_none(),
                "the CI runner must be able to create a symbolic link ({e})"
            );
            eprintln!("skipped: this token cannot create a symbolic link ({e})");
            None
        }
        Err(e) => panic!("symlink {}: {e}", link.display()),
    }
}

/// Items 3, 4 and 6: a file symlink is a reparse point. The check refuses
/// it. `CREATE_NEW` at a dangling link's name fails (Win32 error 80) and
/// creates nothing at its target. As a leftover hygiene temporary linked to
/// an existing file, it is deleted first, and its target keeps its bytes
/// ("deletes no reparse point's target").
#[test]
fn install_ac_005_a_file_symlink_is_never_followed() {
    let root = tempfile::tempdir().expect("tempdir");
    let absent = root.path().join("absent.dat");

    let link = root.path().join("key.dat");
    if symlink(&link, &absent).is_none() {
        return;
    }
    let error = custody::check(&link, Kind::File, &user_only()).expect_err("refused");
    assert!(error.contains("is a reparse point"), "{error}");
    let error =
        custody::create_new_file(&link, &sddl(false, ""), b"sealed").expect_err("not created");
    assert!(error.contains("Win32 error 80"), "{error}");
    assert!(!absent.exists(), "the link's target is not created");

    let target = root.path().join("target.dat");
    std::fs::write(&target, b"target").expect("target");
    let config = root.path().join("agent.toml");
    file(&config, &sddl(false, ""), b"[enrollment]\ntoken = \"t\"\n");
    let temporary = root.path().join("agent.toml.hygiene-tmp");
    symlink(&temporary, &target).expect("a second link");
    custody::replace_keeping_security(&config, &temporary, b"[enrollment]\n", &user_only())
        .expect("replaced");
    assert_eq!(std::fs::read(&config).expect("config"), b"[enrollment]\n");
    assert_eq!(
        std::fs::read(&target).expect("target"),
        b"target",
        "the link's target keeps its bytes"
    );
    assert!(
        !temporary.exists(),
        "the temporary was moved over the original"
    );
}

/// Item 6: when the move fails, the original keeps its bytes, owner and
/// DACL, and no temporary is left behind.
#[test]
fn install_ac_005_a_replacement_whose_move_fails_leaves_the_original() {
    let root = tempfile::tempdir().expect("tempdir");
    let config = root.path().join("agent.toml");
    file(&config, &sddl(false, ""), b"[enrollment]\ntoken = \"t\"\n");
    let before = owner_and_dacl(&config);
    let temporary = root.path().join("agent.toml.hygiene-tmp");

    // An open handle that does not share deletion blocks the move.
    use std::os::windows::fs::OpenOptionsExt;
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1) // FILE_SHARE_READ
        .open(&config)
        .expect("hold the original");
    let error =
        custody::replace_keeping_security(&config, &temporary, b"[enrollment]\n", &user_only())
            .expect_err("the move fails");
    assert!(error.contains("replace"), "{error}");
    drop(held);

    assert_eq!(
        std::fs::read(&config).expect("config"),
        b"[enrollment]\ntoken = \"t\"\n"
    );
    assert_eq!(owner_and_dacl(&config), before);
    assert!(!temporary.exists(), "no temporary is left");
}
