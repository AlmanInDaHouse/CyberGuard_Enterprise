//! `cg-setup`'s command line (SPEC-024 Amendment 2026-11-10, item 9): one
//! verb, `preflight`, `configure` or `reverse`, with the options of the
//! bundle variables, each at most once, an empty value as not given;
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
