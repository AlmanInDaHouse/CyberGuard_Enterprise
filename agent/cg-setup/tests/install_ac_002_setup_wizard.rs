//! install_ac_002's input through the wizard, on every platform (SPEC-024
//! Amendment 2026-11-13, *Tests*): the wizard's fields and a bootstrap file
//! of the same values give the same staged configuration, anchor and
//! fingerprints; each refusal of a field names the field and never the
//! token; the signal's values; and item 3's states with the setup's full
//! window and without it, then read again after the wizard (points 1 to 3).

mod fixtures;

use std::path::Path;

use cg_agent::service::anchor::check_certificates;
use cg_agent::service::InstalledPaths;
use cg_setup::codes;
use cg_setup::inputs::parse_bootstrap;
use cg_setup::staged::{stage_inputs, staged_config};
use cg_setup::wizard::{
    asks_again, full_window, route, route_again, Again, Asked, Choice, Closed, Fields, Kind, Offer,
    Route,
};
use cg_setup::Refusal;
use zeroize::Zeroizing;

use fixtures::{
    assert_clean, bootstrap_text, certificate, ENROLL_URL, HEARTBEAT_URL, HOSTNAME, TOKEN,
};

const SOURCE: &str = "C:\\operator\\server-ca.pem";
const AGENT_ID: &str = "0193f0a0-0000-7000-8000-000000000000";
const CONSOLE: &str = "a console installation's layout";

fn program_data() -> &'static Path {
    if cfg!(windows) {
        Path::new("C:\\ProgramData")
    } else {
        Path::new("/ProgramData")
    }
}

/// The fields a bootstrap file of `bootstrap_text(SOURCE, fingerprints)`
/// holds, the fingerprints typed one per line as `lines` joins them.
fn fields(fingerprints: &[&str], lines: &str) -> Fields {
    Fields {
        server_url: ENROLL_URL.to_string(),
        heartbeat_url: HEARTBEAT_URL.to_string(),
        anchor_path: SOURCE.to_string(),
        fingerprints: fingerprints.join(lines),
        token: Zeroizing::new(TOKEN.to_string()),
    }
}

// --- The fields and the bootstrap file --------------------------------------

#[test]
fn install_ac_002_wizard_fields_and_a_bootstrap_file_stage_alike() {
    let first = certificate("first.example.test");
    let second = certificate("second.example.test");
    let fingerprints = [first.fingerprint.as_str(), second.fingerprint.as_str()];
    let source: Vec<u8> = [first.pem.as_bytes(), second.pem.as_bytes()].concat();
    let paths = InstalledPaths::under(program_data());

    let from_file = parse_bootstrap(&bootstrap_text(SOURCE, &fingerprints)).expect("the file");
    // Lines as an edit control returns them, a line left empty included.
    for lines in ["\n", "\r\n", "\r\n\r\n", "\n\n"] {
        let typed = fields(&fingerprints, lines);
        let mut trailing = fields(&fingerprints, lines);
        trailing.fingerprints.push_str("\r\n");
        for fields in [typed, trailing] {
            let from_fields = fields.inputs().expect("the fields");
            assert_eq!(from_fields.server_url, from_file.server_url);
            assert_eq!(from_fields.heartbeat_url, from_file.heartbeat_url);
            assert_eq!(from_fields.anchor_source, from_file.anchor_source);
            assert_eq!(from_fields.token(), from_file.token());
            assert_eq!(from_fields.fingerprints, from_file.fingerprints);

            assert_eq!(
                *staged_config(&paths, &from_fields, AGENT_ID, HOSTNAME),
                *staged_config(&paths, &from_file, AGENT_ID, HOSTNAME),
                "{lines:?}"
            );
            let staged = |inputs: &cg_setup::inputs::Inputs| {
                let certificates = check_certificates(&source, &inputs.fingerprints, SOURCE)
                    .expect("the anchor matches");
                stage_inputs(&paths, inputs, &certificates, AGENT_ID, HOSTNAME)
                    .expect("the generated configuration passes")
            };
            let (a, b) = (staged(&from_fields), staged(&from_file));
            assert_eq!(*a.config, *b.config);
            assert_eq!(a.anchor, b.anchor);
            assert_eq!(a.fingerprints, b.fingerprints);
            assert!(a.config.contains(TOKEN), "the token is staged");
        }
    }
}

/// A field's refusal names its key, and no reason holds the token, wherever
/// it was typed.
#[test]
fn install_ac_002_wizard_each_refused_field_names_the_field_and_never_the_token() {
    let fingerprint = certificate("refused.example.test").fingerprint;
    let paths = InstalledPaths::under(program_data());
    let reason = |change: &dyn Fn(&mut Fields)| -> String {
        let mut fields = fields(&[&fingerprint], "\n");
        change(&mut fields);
        match fields.inputs() {
            Err(reason) => reason,
            // A value the generated configuration's checks refuse.
            Ok(inputs) => {
                let config = staged_config(&paths, &inputs, AGENT_ID, HOSTNAME);
                cg_setup::staged::check_staged(&paths, &config).expect_err("a refused field")
            }
        }
    };
    type Change = Box<dyn Fn(&mut Fields)>;
    let cases: Vec<(&str, Change)> = vec![
        (
            "enrollment.token",
            Box::new(|f| f.token = Zeroizing::new(String::new())),
        ),
        (
            "setup.trust_anchor_sha256",
            Box::new(|f| f.fingerprints.clear()),
        ),
        (
            "setup.trust_anchor_sha256",
            Box::new(|f| f.fingerprints = "\r\n\r\n".to_string()),
        ),
        (
            "setup.trust_anchor_sha256",
            Box::new(|f| f.fingerprints = TOKEN.to_string()),
        ),
        (
            "setup.trust_anchor_sha256",
            Box::new(|f| f.fingerprints = format!("{} ", f.fingerprints)),
        ),
        (
            "setup.trust_anchor_sha256",
            Box::new(|f| f.fingerprints = f.fingerprints.to_uppercase()),
        ),
        (
            "server.url",
            Box::new(|f| f.server_url = "http://enroll.example.test:8444".to_string()),
        ),
        ("server.url", Box::new(|f| f.server_url = TOKEN.to_string())),
        ("server.url", Box::new(|f| f.server_url.clear())),
        (
            "server.heartbeat_url",
            Box::new(|f| f.heartbeat_url = "http://ingest.example.test:8443".to_string()),
        ),
        (
            "server.heartbeat_url",
            Box::new(|f| f.heartbeat_url = TOKEN.to_string()),
        ),
    ];
    for (field, change) in cases {
        let reason = reason(change.as_ref());
        assert!(reason.contains(field), "{field}: {reason}");
        assert_clean(&reason);
    }
}

#[test]
fn install_ac_002_wizard_fields_debug_writes_no_token() {
    let fields = fields(&["sha256:00"], "\n");
    let debug = format!("{fields:?}");
    assert!(debug.contains("<redacted>"), "{debug}");
    assert_clean(&debug);
}

#[test]
fn install_ac_002_wizard_the_token_is_cleared() {
    let mut fields = fields(&["sha256:00"], "\n");
    fields.clear_token();
    assert!(fields.token.is_empty());
}

// --- The signal --------------------------------------------------------------

#[test]
fn install_ac_002_wizard_only_the_full_window_lets_it_show() {
    assert!(full_window(Some("4")));
    for value in [
        None,
        Some(""),
        Some("2"),
        Some("3"),
        Some("04"),
        Some("4 "),
        Some("full"),
    ] {
        assert!(!full_window(value), "{value:?}");
    }
}

// --- Item 3 with the wizard --------------------------------------------------

fn asked(bootstrap: bool, migration: bool, full_window: bool) -> Asked {
    Asked {
        bootstrap,
        migration,
        full_window,
    }
}

#[test]
fn install_ac_002_wizard_shows_on_a_fresh_or_pending_host_with_the_full_window_alone() {
    let untrusted = Kind::Untrusted("C:\\x is a reparse point".to_string());
    for window in [false, true] {
        let none = asked(false, false, window);
        let bootstrap = asked(true, false, window);
        let migration = asked(false, true, window);
        let both = asked(true, true, window);
        let expected: Vec<(&Kind, Asked, Result<Route, u8>)> = vec![
            (&untrusted, none, Err(codes::UNTRUSTED_PATH)),
            (&untrusted, bootstrap, Err(codes::UNTRUSTED_PATH)),
            (&untrusted, migration, Err(codes::UNTRUSTED_PATH)),
            (&Kind::MigrationInProgress, none, Err(codes::CONSOLE_LAYOUT)),
            (
                &Kind::MigrationInProgress,
                bootstrap,
                Err(codes::CONSOLE_LAYOUT),
            ),
            (&Kind::MigrationInProgress, both, Err(codes::CONSOLE_LAYOUT)),
            (&Kind::MigrationInProgress, migration, Ok(Route::Migrate)),
            (&Kind::ConsoleLayout, none, Err(codes::CONSOLE_LAYOUT)),
            (&Kind::ConsoleLayout, bootstrap, Err(codes::CONSOLE_LAYOUT)),
            (&Kind::ConsoleLayout, migration, Ok(Route::Migrate)),
            (&Kind::Enrolled, none, Ok(Route::Start)),
            (&Kind::Enrolled, bootstrap, Err(codes::ALREADY_CONFIGURED)),
            (&Kind::Enrolled, migration, Err(codes::CONSOLE_LAYOUT)),
            (
                &Kind::Pending,
                none,
                Ok(if window {
                    Route::Ask(Offer::Pending)
                } else {
                    Route::Start
                }),
            ),
            (&Kind::Pending, bootstrap, Ok(Route::Stage)),
            (&Kind::Pending, migration, Err(codes::CONSOLE_LAYOUT)),
            (
                &Kind::Fresh,
                none,
                if window {
                    Ok(Route::Ask(Offer::Fresh))
                } else {
                    Err(codes::NO_INPUT)
                },
            ),
            (&Kind::Fresh, bootstrap, Ok(Route::Stage)),
            (&Kind::Fresh, migration, Err(codes::CONSOLE_LAYOUT)),
            (&Kind::Fresh, both, Err(codes::CONSOLE_LAYOUT)),
        ];
        for (kind, asked, outcome) in expected {
            let got = route(kind, asked, CONSOLE);
            check(&got, &outcome, &format!("{kind:?} {asked:?}"));
        }
    }
}

#[test]
fn install_ac_002_wizard_the_second_reading_takes_the_state_as_a_bootstrap_would() {
    let untrusted = Kind::Untrusted("C:\\x is a reparse point".to_string());
    let expected: Vec<(&Kind, Choice, Result<Again, u8>)> = vec![
        (&untrusted, Choice::Input, Err(codes::UNTRUSTED_PATH)),
        (&untrusted, Choice::Keep, Err(codes::UNTRUSTED_PATH)),
        (
            &Kind::MigrationInProgress,
            Choice::Input,
            Err(codes::CONSOLE_LAYOUT),
        ),
        (
            &Kind::MigrationInProgress,
            Choice::Keep,
            Err(codes::CONSOLE_LAYOUT),
        ),
        (
            &Kind::ConsoleLayout,
            Choice::Input,
            Err(codes::CONSOLE_LAYOUT),
        ),
        (
            &Kind::ConsoleLayout,
            Choice::Keep,
            Err(codes::CONSOLE_LAYOUT),
        ),
        (
            &Kind::Enrolled,
            Choice::Input,
            Err(codes::ALREADY_CONFIGURED),
        ),
        (
            &Kind::Enrolled,
            Choice::Keep,
            Err(codes::ALREADY_CONFIGURED),
        ),
        (&Kind::Pending, Choice::Input, Ok(Again::Stage)),
        (&Kind::Pending, Choice::Keep, Ok(Again::Start)),
        (&Kind::Fresh, Choice::Input, Ok(Again::Stage)),
        (&Kind::Fresh, Choice::Keep, Err(codes::NO_INPUT)),
    ];
    for (kind, choice, outcome) in expected {
        let got = route_again(kind, choice, CONSOLE);
        check(&got, &outcome, &format!("{kind:?} {choice:?}"));
    }
}

#[test]
fn install_ac_002_wizard_a_closed_wizard_is_22_and_a_field_or_anchor_asks_again() {
    for closed in [Closed::Cancelled, Closed::TimedOut, Closed::StarterExited] {
        let refusal = closed.refusal();
        assert_eq!(refusal.code, codes::NO_INPUT, "{closed:?}");
        assert!(
            refusal.line.contains("nothing was changed"),
            "{}",
            refusal.line
        );
    }
    for code in [codes::INVALID_INPUT, codes::ANCHOR] {
        assert!(asks_again(&Refusal::new(code, "x")), "{code}");
    }
    for code in [
        codes::NO_INPUT,
        codes::BOOTSTRAP_ACCESS,
        codes::CONFLICTING_SERVICE,
        codes::UNTRUSTED_PATH,
        codes::CONSOLE_LAYOUT,
        codes::DOWNGRADE,
        codes::ALREADY_CONFIGURED,
        codes::OWN_STATE,
    ] {
        assert!(!asks_again(&Refusal::new(code, "x")), "{code}");
    }
}

fn check<T: std::fmt::Debug + PartialEq>(
    got: &Result<T, Refusal>,
    want: &Result<T, u8>,
    what: &str,
) {
    match (got, want) {
        (Ok(got), Ok(want)) => assert_eq!(got, want, "{what}"),
        (Err(refusal), Err(code)) => {
            assert_eq!(refusal.code, *code, "{what}: {}", refusal.line);
            assert!(refusal.line.starts_with("cg-setup: "), "{}", refusal.line);
        }
        _ => panic!("{what}: {got:?}, expected {want:?}"),
    }
}
