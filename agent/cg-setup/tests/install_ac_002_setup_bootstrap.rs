//! install_ac_002's setup half on Windows (SPEC-024 §Data contracts §3 and its
//! Amendment 2026-11-09, items 2 and 3), through preflight's `assess` over a
//! temporary host, with the current user where the service's custody names
//! SYSTEM: an unsafe bootstrap file is code 23, an anchor that does not match
//! its fingerprints 25, plain HTTP and other invalid input 24, all before
//! anything changes; a valid one yields the configuration and the anchor to
//! stage. No line carries the token.
#![cfg(windows)]

mod fixtures;

use std::os::windows::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};

use cg_agent::service::anchor::check_anchor;
use cg_setup::codes;
use cg_setup::preflight::{assess, Plan, Request, Staged};
use cg_setup::Refusal;

use fixtures::{
    assert_clean, bootstrap_text, certificate, junction, open_directory_sddl, private_file_sddl,
    set, Certificate, Root, HOSTNAME, TOKEN,
};

fn request(bootstrap: &Path) -> Request {
    Request {
        bootstrap: Some(bootstrap.to_path_buf()),
        ..Request::default()
    }
}

/// A fresh host, an anchor of one certificate and a valid bootstrap file.
fn fresh() -> (Root, Certificate, PathBuf) {
    let root = Root::empty();
    let ca = certificate("ca.setup.test");
    let anchor = root.anchor(&[&ca]);
    let bootstrap = root.bootstrap(&bootstrap_text(
        anchor.to_str().expect("UTF-8"),
        &[&ca.fingerprint],
    ));
    (root, ca, bootstrap)
}

/// `assess` refuses with `code`, naming `naming`, and changes nothing.
fn refused(root: &Root, request: &Request, code: u8, naming: &str) -> Refusal {
    let before = root.tree();
    let refusal = match assess(&root.host, request) {
        Ok(plan) => panic!("{naming}: code {code} was expected: {plan:?}"),
        Err(refusal) => refusal,
    };
    assert_eq!(refusal.code, code, "{naming}: {}", refusal.line);
    assert!(refusal.line.starts_with("cg-setup: "), "{}", refusal.line);
    assert!(refusal.line.contains(naming), "{naming}: {}", refusal.line);
    assert_clean(&refusal.line);
    assert_eq!(root.tree(), before, "{naming}: nothing changed");
    refusal
}

fn staged(root: &Root, request: &Request) -> Staged {
    let before = root.tree();
    let plan = assess(&root.host, request).expect("a route that stages");
    assert_eq!(root.tree(), before, "assess changes nothing");
    match plan {
        Plan::Stage(staged) => staged,
        other => panic!("staging was expected: {other:?}"),
    }
}

// --- A valid bootstrap --------------------------------------------------------

#[test]
fn install_ac_002_setup_a_valid_bootstrap_yields_what_is_staged() {
    let (root, ca, bootstrap) = fresh();
    let staged = staged(&root, &request(&bootstrap));
    assert_eq!(staged.fingerprints, set(&[&ca.fingerprint]));
    cg_setup::staged::check_staged(&root.paths, &staged.config).expect("the adoption's checks");
    let document: toml::Value = staged.config.parse().expect("TOML");
    assert_eq!(document["agent"]["hostname"].as_str(), Some(HOSTNAME));
    assert_eq!(document["enrollment"]["token"].as_str(), Some(TOKEN));
    assert_eq!(
        check_anchor(&staged.anchor, &staged.fingerprints).expect("the staged anchor"),
        vec![ca.der.clone()]
    );
    let debug = format!("{staged:?}");
    assert_clean(&debug);
    assert!(!debug.contains("BEGIN CERTIFICATE"), "{debug}");
}

// --- Code 23: the bootstrap file's access -------------------------------------

#[test]
fn install_ac_002_setup_a_bootstrap_others_can_read_is_code_23() {
    let (root, ca, _) = fresh();
    let text = bootstrap_text(
        root.operator.join("server-ca.pem").to_str().expect("UTF-8"),
        &[&ca.fingerprint],
    );
    for (name, sddl) in [
        (
            "users.toml",
            format!("D:P(A;;FA;;;{})(A;;FA;;;BA)(A;;FR;;;BU)", fixtures::sid()),
        ),
        (
            "everyone.toml",
            format!("D:P(A;;FA;;;{})(A;;FR;;;WD)", fixtures::sid()),
        ),
        (
            "authenticated.toml",
            format!("D:P(A;;FA;;;{})(A;;0x120089;;;AU)", fixtures::sid()),
        ),
    ] {
        let path = root.bootstrap_with(name, &sddl, &text);
        let refusal = refused(&root, &request(&path), codes::BOOTSTRAP_ACCESS, name);
        assert!(
            refusal
                .line
                .contains("allows access to a principal other than"),
            "{}",
            refusal.line
        );
    }
    // The same text, protected for this user and Administrators alone, is taken.
    let path = root.bootstrap_with("private.toml", &private_file_sddl(), &text);
    assert!(matches!(
        assess(&root.host, &request(&path)),
        Ok(Plan::Stage(_))
    ));
}

#[test]
fn install_ac_002_setup_a_bootstrap_that_inherits_access_for_others_is_code_23() {
    let (root, ca, _) = fresh();
    let open = root.operator.join("shared");
    let any = cg_agent::custody::Policy {
        owners: vec![],
        allowed: vec![],
        protected: false,
    };
    cg_agent::custody::ensure_directory(&open, &open_directory_sddl(), &any).expect("shared");
    let path = open.join("bootstrap.toml");
    std::fs::write(
        &path,
        bootstrap_text(
            root.operator.join("server-ca.pem").to_str().expect("UTF-8"),
            &[&ca.fingerprint],
        ),
    )
    .expect("a file that inherits its directory's entries");
    refused(
        &root,
        &request(&path),
        codes::BOOTSTRAP_ACCESS,
        "allows access to a principal other than",
    );
    // The same text, beside it in a directory that gives no one else access,
    // is taken: what refuses it is the inherited entries.
    let private = root.operator.join("private");
    cg_agent::custody::ensure_directory(&private, &fixtures::closed_directory_sddl(), &any)
        .expect("private");
    let taken = private.join("bootstrap.toml");
    std::fs::copy(&path, &taken).expect("a file that inherits only private entries");
    assert!(matches!(
        assess(&root.host, &request(&taken)),
        Ok(Plan::Stage(_))
    ));
}

#[test]
fn install_ac_002_setup_a_bootstrap_of_another_owner_is_code_23() {
    let (mut root, _, bootstrap) = fresh();
    // The file is the current user's; the policy admits SYSTEM alone.
    root.host.bootstrap.owners = vec![cg_agent::custody::Principal::System];
    refused(
        &root,
        &request(&bootstrap),
        codes::BOOTSTRAP_ACCESS,
        "is not owned by",
    );
}

#[test]
fn install_ac_002_setup_a_bootstrap_that_is_not_a_regular_local_file_is_code_23() {
    let (root, ca, bootstrap) = fresh();
    refused(
        &root,
        &request(&root.operator.join("absent.toml")),
        codes::BOOTSTRAP_ACCESS,
        "does not exist",
    );
    refused(
        &root,
        &request(Path::new("bootstrap.toml")),
        codes::BOOTSTRAP_ACCESS,
        "absolute",
    );
    let directory = root.operator.join("a-directory.toml");
    std::fs::create_dir(&directory).expect("a directory");
    refused(
        &root,
        &request(&directory),
        codes::BOOTSTRAP_ACCESS,
        "not a regular file",
    );
    // Another link to the file shares its owner and DACL: it is taken
    // (§Data contracts §3 asks for a regular local file, not one link).
    let linked = root.operator.join("linked.toml");
    std::fs::hard_link(&bootstrap, &linked).expect("a hard link");
    assert!(matches!(
        assess(&root.host, &request(&linked)),
        Ok(Plan::Stage(_))
    ));
    std::fs::remove_file(&linked).expect("unlink");

    // A junction is opened as itself, not followed.
    let target = root.operator.join("target");
    std::fs::create_dir(&target).expect("target");
    let reparse = root.operator.join("reparse.toml");
    junction(&reparse, &target);
    refused(
        &root,
        &request(&reparse),
        codes::BOOTSTRAP_ACCESS,
        "reparse point",
    );

    // Too large to be a bootstrap.
    let large = root.bootstrap_with(
        "large.toml",
        &private_file_sddl(),
        &format!(
            "{}\n# {}\n",
            bootstrap_text("C:\\x.pem", &[&ca.fingerprint]),
            "x".repeat(16 * 1024 * 1024)
        ),
    );
    refused(
        &root,
        &request(&large),
        codes::BOOTSTRAP_ACCESS,
        "larger than",
    );
}

#[test]
fn install_ac_002_setup_a_bootstrap_held_for_writing_is_code_23() {
    let (root, _, bootstrap) = fresh();
    let _writer = std::fs::OpenOptions::new()
        .write(true)
        .share_mode(7)
        .open(&bootstrap)
        .expect("held for writing");
    refused(
        &root,
        &request(&bootstrap),
        codes::BOOTSTRAP_ACCESS,
        "in use",
    );
}

#[test]
fn install_ac_002_setup_a_remote_bootstrap_is_code_23() {
    use cg_agent::custody::is_remote_nt_path;
    for remote in [
        "\\Device\\Mup\\server\\share\\bootstrap.toml",
        "\\device\\mup\\localhost\\C$\\bootstrap.toml",
        "\\Device\\LanmanRedirector\\server\\share\\bootstrap.toml",
    ] {
        assert!(is_remote_nt_path(remote), "{remote}");
    }
    for local in [
        "\\Device\\HarddiskVolume3\\Users\\op\\bootstrap.toml",
        "\\Device\\HarddiskVolumeShadowCopy1\\x.toml",
        "\\Device\\Mupx\\x.toml",
    ] {
        assert!(!is_remote_nt_path(local), "{local}");
    }
    // Through the loopback administrative share: refused whether the open
    // succeeds (a remote volume) or not (a filtered token has no access).
    let (root, _, bootstrap) = fresh();
    let text = bootstrap.to_str().expect("UTF-8");
    let (drive, rest) = text.split_once(":\\").expect("a drive path");
    let unc = PathBuf::from(format!("\\\\localhost\\{drive}$\\{rest}"));
    let refusal = assess(&root.host, &request(&unc)).expect_err("a remote file");
    assert_eq!(refusal.code, codes::BOOTSTRAP_ACCESS, "{}", refusal.line);
    assert_clean(&refusal.line);
    // The runner's built-in Administrator opens the share, so there the
    // refusal is the volume's; a filtered token's open is denied before it.
    if fixtures::in_ci() {
        assert!(
            refusal.line.contains("is not on a local volume"),
            "{}",
            refusal.line
        );
    }
}

// --- Code 24: invalid input -----------------------------------------------------

#[test]
fn install_ac_002_setup_invalid_input_is_code_24() {
    let (root, ca, _) = fresh();
    let anchor = root.operator.join("server-ca.pem");
    let valid = bootstrap_text(anchor.to_str().expect("UTF-8"), &[&ca.fingerprint]);
    for (name, text, naming) in [
        (
            "key.toml",
            format!("[agent]\nid = \"01934abc-def0-7000-89ab-0000000000bb\"\n{valid}"),
            "agent",
        ),
        (
            "http.toml",
            valid.replacen("https://enroll", "http://enroll", 1),
            "https",
        ),
        (
            "heartbeat.toml",
            valid.replacen("https://ingest", "http://ingest", 1),
            "server.heartbeat_url",
        ),
        (
            "relative.toml",
            bootstrap_text("server-ca.pem", &[&ca.fingerprint]),
            "absolute",
        ),
        ("binary.toml", "\u{0}\u{0}".to_string(), "TOML"),
    ] {
        let path = root.bootstrap_with(name, &private_file_sddl(), &text);
        refused(&root, &request(&path), codes::INVALID_INPUT, naming);
    }
    let path = root.operator.join("latin1.toml");
    cg_agent::custody::create_new_file(&path, &private_file_sddl(), &[0xff, 0xfe, 0x41])
        .expect("not UTF-8");
    refused(&root, &request(&path), codes::INVALID_INPUT, "UTF-8");
}

// --- Code 25: the trust anchor ----------------------------------------------------

#[test]
fn install_ac_002_setup_an_anchor_that_does_not_match_is_code_25() {
    let root = Root::empty();
    let (ca, other) = (
        certificate("ca.setup.test"),
        certificate("other.setup.test"),
    );
    let cases: [(&str, Vec<&Certificate>, Vec<&str>); 4] = [
        ("wrong.toml", vec![&other], vec![&ca.fingerprint]),
        ("extra-root.toml", vec![&ca, &other], vec![&ca.fingerprint]),
        (
            "extra-fingerprint.toml",
            vec![&ca],
            vec![&ca.fingerprint, &other.fingerprint],
        ),
        ("empty.toml", vec![], vec![&ca.fingerprint]),
    ];
    for (name, certificates, fingerprints) in cases {
        let anchor = root.operator.join(format!("{name}.pem"));
        let pem: String = certificates.iter().map(|c| c.pem.as_str()).collect();
        std::fs::write(&anchor, pem).expect("anchor");
        let path = root.bootstrap_with(
            name,
            &private_file_sddl(),
            &bootstrap_text(anchor.to_str().expect("UTF-8"), &fingerprints),
        );
        refused(
            &root,
            &request(&path),
            codes::ANCHOR,
            "not exactly those of the fingerprints given",
        );
    }
    for (name, source, naming) in [
        ("absent.toml", root.operator.join("absent.pem"), "open"),
        (
            "directory.toml",
            root.operator.clone(),
            "is not a regular file",
        ),
    ] {
        let path = root.bootstrap_with(
            name,
            &private_file_sddl(),
            &bootstrap_text(source.to_str().expect("UTF-8"), &[&ca.fingerprint]),
        );
        refused(&root, &request(&path), codes::ANCHOR, naming);
    }
}
