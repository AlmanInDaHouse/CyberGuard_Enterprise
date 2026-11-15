//! `cg-setup`'s command line (SPEC-024 Amendment 2026-11-10, item 9): one
//! verb, `preflight`, `configure`, `reverse` or `purge` (the Amendment
//! 2026-11-15, point 3, `--confirm=` empty or `yes`, and the Amendment
//! 2026-11-16, item 2, `--action=` of digits), with the options of
//! the bundle variables, each at most once, an empty value as not given;
//! `--version`; any other command line is 40, its line quoting no value.
#![cfg(windows)]

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt as _;
use std::path::PathBuf;

use cg_setup::cli::{parse, Command};
use cg_setup::codes;
use cg_setup::preflight::Request;

fn args(text: &[&str]) -> Vec<OsString> {
    text.iter().map(OsString::from).collect()
}

fn refused(text: &[&str]) -> String {
    let refusal = parse(&args(text)).expect_err(&format!("{text:?} is refused"));
    assert_eq!(refusal.code, codes::OWN_STATE, "{text:?}: {}", refusal.line);
    assert!(
        refusal
            .line
            .starts_with("cg-setup: the command line is not one the setup's bundle forms: "),
        "{}",
        refusal.line
    );
    refusal.line
}

#[test]
fn package_cli_the_verbs_and_their_options() {
    assert_eq!(parse(&args(&["--version"])), Ok(Command::Version));
    assert_eq!(parse(&args(&["reverse"])), Ok(Command::Reverse));
    assert_eq!(
        parse(&args(&["preflight"])),
        Ok(Command::Preflight(Request::default()))
    );
    assert_eq!(
        parse(&args(&[
            "preflight",
            "--trust-anchors=sha256:aa",
            r"--bootstrap=C:\setup\bootstrap.toml",
            "--migrate-legacy=keep",
        ])),
        Ok(Command::Preflight(Request {
            bootstrap: Some(PathBuf::from(r"C:\setup\bootstrap.toml")),
            migrate_legacy: Some("keep".into()),
            trust_anchors: Some("sha256:aa".into()),
            ui: None,
        })),
        "in any order"
    );
    assert_eq!(
        parse(&args(&[
            "configure",
            "--migrate-legacy=remove",
            "--trust-anchors=sha256:bb"
        ])),
        Ok(Command::Configure(Request {
            bootstrap: None,
            migrate_legacy: Some("remove".into()),
            trust_anchors: Some("sha256:bb".into()),
            ui: None,
        }))
    );
    // A value may hold '=' and spaces: only the first '=' separates.
    assert_eq!(
        parse(&args(&["preflight", r"--bootstrap=C:\a b\c=d.toml"])),
        Ok(Command::Preflight(Request {
            bootstrap: Some(PathBuf::from(r"C:\a b\c=d.toml")),
            ..Request::default()
        }))
    );
}

/// An empty value is a variable not given: Burn expands one that is unset to
/// nothing.
#[test]
fn package_cli_an_empty_value_is_not_given() {
    assert_eq!(
        parse(&args(&[
            "preflight",
            "--bootstrap=",
            "--migrate-legacy=",
            "--trust-anchors="
        ])),
        Ok(Command::Preflight(Request::default()))
    );
    assert_eq!(
        parse(&args(&[
            "configure",
            "--migrate-legacy=",
            "--trust-anchors="
        ])),
        Ok(Command::Configure(Request::default()))
    );
}

/// Preflight's `--ui`, the setup's `WixBundleUILevel`, at most once, an empty
/// value not given; configure and reverse take none (SPEC-024 Amendment
/// 2026-11-13, point 1). Burn's `--ui="4"` reaches the setup as `--ui=4`.
#[test]
fn package_cli_preflight_takes_the_setups_window_once() {
    for value in ["4", "3", "2", "anything"] {
        let option = format!("--ui={value}");
        assert_eq!(
            parse(&args(&["preflight", &option])),
            Ok(Command::Preflight(Request {
                ui: Some(value.into()),
                ..Request::default()
            })),
            "{value}"
        );
    }
    assert_eq!(
        parse(&args(&[
            "preflight",
            "--ui=4",
            r"--bootstrap=C:\setup\bootstrap.toml",
        ])),
        Ok(Command::Preflight(Request {
            bootstrap: Some(PathBuf::from(r"C:\setup\bootstrap.toml")),
            ui: Some("4".into()),
            ..Request::default()
        }))
    );
    assert_eq!(
        parse(&args(&["preflight", "--ui="])),
        Ok(Command::Preflight(Request::default()))
    );
    assert!(Request::default().with_ui(Some("4")).full_window());
    assert!(!Request::default().with_ui(Some("3")).full_window());
    assert!(!Request::default().full_window());
    for text in [
        &["preflight", "--ui=4", "--ui=4"][..],
        &["preflight", "--ui=", "--ui="],
        &["preflight", "--UI=4"],
        &["preflight", "--ui"],
        &["configure", "--ui=4"],
        &["configure", "--ui="],
        &["reverse", "--ui=4"],
    ] {
        refused(text);
    }
}

/// Any other command line is 40.
#[test]
fn package_cli_any_other_command_line_is_40() {
    for text in [
        &[][..],
        &["--version", "preflight"],
        &["-version"],
        &["reverse", "--bootstrap=x"],
        &["preflight", "reverse"],
        &["preflight", "--bootstrap"],
        &["preflight", "--bootstrap=a", "--bootstrap=b"],
        &["preflight", "--bootstrap=", "--bootstrap="],
        &["preflight", "--Bootstrap=a"],
        &["preflight", "--purge=yes"],
        &["configure", "--bootstrap=a"],
        &["configure", "--trust-anchors=a", "--trust-anchors=a"],
        &["install"],
        &["PREFLIGHT"],
        &["--help"],
    ] {
        refused(text);
    }
}

/// A refusal names an option, never a value: a misplaced value could be a
/// token.
#[test]
fn package_cli_a_refusal_quotes_no_value() {
    for text in [
        &[
            "preflight",
            "--bootstrap=SECRET-ONE",
            "--bootstrap=SECRET-TWO",
        ][..],
        &["preflight", "--token=SECRET-ONE"],
        &["preflight", "SECRET-ONE"],
        &["SECRET-ONE"],
        &["reverse", "SECRET-ONE"],
        &["configure", "--bootstrap=SECRET-ONE"],
    ] {
        let line = refused(text);
        assert!(!line.contains("SECRET"), "{text:?}: {line}");
        assert!(!line.contains("token"), "{text:?}: {line}");
    }
}

/// An argument that is not Unicode is 40, not a lossy reading.
#[test]
fn package_cli_an_argument_that_is_not_unicode_is_40() {
    let lone_surrogate = OsString::from_wide(&[0x2D, 0x2D, 0xD800]);
    for arguments in [
        vec![OsString::from("preflight"), lone_surrogate.clone()],
        vec![lone_surrogate],
    ] {
        let refusal = parse(&arguments).expect_err("40");
        assert_eq!(refusal.code, codes::OWN_STATE, "{}", refusal.line);
        assert!(refusal.line.contains("not Unicode"), "{}", refusal.line);
    }
}

// --- purge (SPEC-024 Amendment 2026-11-15, point 3) ----------------------------

/// `--confirm=` at most once; absent or empty is not given, `yes` confirms.
#[test]
fn package_cli_purge_takes_confirm_once() {
    assert_eq!(
        parse(&args(&["purge"])),
        Ok(Command::Purge { confirmed: false })
    );
    assert_eq!(
        parse(&args(&["purge", "--confirm="])),
        Ok(Command::Purge { confirmed: false })
    );
    assert_eq!(
        parse(&args(&["purge", "--confirm=yes"])),
        Ok(Command::Purge { confirmed: true })
    );
}

/// Another value, the option twice, an unknown option or one without `=` is
/// 40, and the line quotes no value.
#[test]
fn package_cli_purge_refuses_any_other_command_line() {
    for value in ["no", "YES", "yes ", "\"yes\"", "1"] {
        let option = format!("--confirm={value}");
        let line = refused(&["purge", &option]);
        assert!(line.contains("--confirm="), "{line}");
        if !value.is_empty() {
            assert!(!line.contains(&option), "{line}");
        }
    }
    refused(&["purge", "--confirm=yes", "--confirm=yes"]);
    refused(&["purge", "--confirm=", "--confirm=yes"]);
    refused(&["purge", r"--bootstrap=C:\x"]);
    refused(&["purge", "--confirm"]);
    refused(&["purge", "yes"]);
}

/// `--action=`, the setup's `WixBundleAction` (SPEC-024 Amendment
/// 2026-11-16, item 2), at most once, absent or empty not given, of digits:
/// `yes` confirms with Burn's uninstallation, 4, and with no action; with
/// any other action, an installation's 6 and a repair's 8 included, it is not
/// given. Another value is 40, its line quoting none.
#[test]
fn package_cli_purge_confirms_only_with_the_uninstallation() {
    let purge = |options: &[&str]| {
        let mut arguments = vec!["purge"];
        arguments.extend_from_slice(options);
        parse(&args(&arguments))
    };
    for (options, confirmed) in [
        (&["--confirm=yes", "--action=4"][..], true),
        (&["--action=4", "--confirm=yes"], true),
        (&["--confirm=yes", "--action=04"], true),
        (&["--confirm=yes", "--action="], true),
        (&["--confirm=yes", "--action=6"], false),
        (&["--confirm=yes", "--action=8"], false),
        (&["--confirm=yes", "--action=3"], false),
        (&["--confirm=yes", "--action=0"], false),
        (
            &["--confirm=yes", "--action=99999999999999999999999"],
            false,
        ),
        (&["--confirm=", "--action=4"], false),
        (&["--action=4"], false),
    ] {
        assert_eq!(
            purge(options),
            Ok(Command::Purge { confirmed }),
            "{options:?}"
        );
    }
    for value in ["x", "-4", "4 ", " 4", "4.0", "uninstall"] {
        let option = format!("--action={value}");
        let line = refused(&["purge", "--confirm=yes", &option]);
        assert!(line.contains("--action="), "{line}");
        assert!(!line.contains(&option), "{line}");
    }
    refused(&["purge", "--action=4", "--action=4"]);
    refused(&["purge", "--action=", "--action=6"]);
}
