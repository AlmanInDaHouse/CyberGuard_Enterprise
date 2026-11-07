//! SPEC-024 install_ac_013, the legacy migration's route (its Amendment
//! 2026-11-05, items 1, 2 and 6), unelevated over temporary roots: the start
//! arguments and the exact-set rule; items 2.1 to 2.6 in order, each refusal
//! with its code, its repair and the console installation's bytes and place
//! as item 2 leaves them; a start without arguments, which never migrates.
//!
//! The custody names the current user where the service's names SYSTEM and
//! real SYSTEM is the enrolling account (`migration_fixtures`); the service
//! and SYSTEM tests, `install_ac_013_migration`, run SYSTEM's. Failures are
//! injected through `Watch`, with no Cargo feature.
#![cfg(windows)]

mod common;
mod custody_fixtures;
mod migration_fixtures;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use cg_agent::custody::{self, Kind, Policy, Principal};
use cg_agent::secure_storage::default_store;
use cg_agent::service::migration::{
    parse_start_arguments, Choice, MigrationCustody, Point, Prepared, Request, Stop,
    DEFAULT_REFUSAL,
};
use custody_fixtures::{default_owner_sid, normalized, sid_bytes};
use migration_fixtures::{
    arguments_refusal, assert_in_place_line, assert_quotes_none, assert_same_tree, canonical,
    config_refusal, entries, fingerprints_of, forward, hold, identity_refusal, in_ci, junction,
    log_refusal, names, programdata_child_sddl, read_record, refused, security, set_dacl,
    set_owner, sid, test_custody, tls_refusal, tree, with_owner, Console, Script, Tree,
    ACCESS_DELETE, ACCESS_READ, ACCESS_WRITE, ARGUMENTS_LINE, DIRECTORIES, ENROLLING_SID,
    ENROLL_URL, FILES, HEARTBEAT_URL, HYGIENE, IDENTITY_FILES, IN_PLACE, LEGACY_AGENT_ID,
    LEGACY_HOSTNAME, LOG_LINE, RESTRICTED, SHARE_ALL, STAGED, TOKEN,
};

fn fingerprint_value(digit: char) -> String {
    format!("sha256:{}", digit.to_string().repeat(64))
}

fn fingerprint_argument(digit: char) -> String {
    format!("--trust-anchor-sha256={}", fingerprint_value(digit))
}

fn arguments(list: &[&str]) -> Vec<String> {
    list.iter().map(|argument| argument.to_string()).collect()
}

/// A refusal of item 2.2 or 2.3: the directories are closed, and the line
/// names them; no restriction began and each file is as it was.
fn assert_no_file_changed(console: &Console, before: &Tree, script: &Script, line: &str) {
    assert_in_place_line(line, &DIRECTORIES);
    assert!(script.restrictions().is_empty(), "{line}");
    console.assert_directories_closed();
    console.assert_console_in_place(before, &[]);
}

/// The DACL of a file that allows this user, the enrolling account and
/// `extra`.
fn identity_dacl(extra: &str) -> String {
    format!("D:P(A;;FA;;;{})(A;;FA;;;SY){extra}", sid())
}

/// What a case writes in a console's file.
type Bytes<'a> = &'a dyn Fn(&Console) -> Vec<u8>;
/// The fingerprints a case gives.
type Fingerprints<'a> = &'a dyn Fn(&Console) -> BTreeSet<String>;

/// The two lists name the same principals.
fn same_principals(list: &[Principal], expected: &[Principal]) -> bool {
    list.len() == expected.len() && expected.iter().all(|principal| list.contains(principal))
}

/// The policy's fields are those of `expected`.
fn assert_same_policy(policy: &Policy, expected: &Policy, what: &str) {
    assert!(same_principals(&policy.owners, &expected.owners), "{what}");
    assert!(
        same_principals(&policy.allowed, &expected.allowed),
        "{what}"
    );
    assert_eq!(policy.protected, expected.protected, "{what}");
}

/// What the staged tree holds came from the bytes `before` has, whatever the
/// console's files hold now.
fn assert_staged_from(console: &Console, before: &Tree) {
    for (name, path) in [
        ("cert.pem", &console.paths.cert),
        ("key.dat", &console.paths.key),
        ("identity.json", &console.paths.identity),
    ] {
        assert_eq!(
            std::fs::read(path).ok().as_ref(),
            before[name].bytes.as_ref(),
            "{name}"
        );
    }
    let staged = std::fs::read(&console.paths.trust_anchor).expect("read");
    assert_eq!(fingerprints_of(&staged), [console.fingerprint()].into());
    assert_eq!(console.generated_config().agent.id, LEGACY_AGENT_ID);
}

/// The Rust files under `directory`.
fn rust_files(directory: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("read a source directory") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            rust_files(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

// --- The service's custody --------------------------------------------------

/// Items 2.1, 2.4, 2.5, 4 and 5: every test here names the current user in
/// SYSTEM's place, so what the service itself checks and writes is pinned
/// here: owner SYSTEM; SYSTEM and Administrators on the closed directories
/// and the shared files, SYSTEM alone on the identity's; SYSTEM or
/// Administrators as the owner item 2.2 accepts.
#[test]
fn install_ac_013_the_service_s_custody_is_system_s() {
    let custody = MigrationCustody::system();
    assert_eq!(custody.service, Principal::System);
    let both = [Principal::System, Principal::Administrators];
    assert!(same_principals(&custody.owners, &both), "{custody:?}");
    assert!(same_principals(&custody.trusted, &both), "{custody:?}");
    assert_same_policy(
        &custody.configuration,
        &Policy::configuration(),
        "agent.toml's policy",
    );
    assert_same_policy(&custody.identity, &Policy::system_only(), "identity\\");
    for (sddl, expected) in [
        (
            &custody.closed_directory_sddl,
            "O:SYD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)",
        ),
        (&custody.shared_file_sddl, "O:SYD:P(A;;FA;;;SY)(A;;FA;;;BA)"),
        (&custody.identity_directory_sddl, "O:SYD:P(A;OICI;FA;;;SY)"),
        (&custody.identity_file_sddl, "O:SYD:P(A;;FA;;;SY)"),
    ] {
        assert_eq!(
            canonical(&normalized(sddl)),
            canonical(&normalized(expected))
        );
    }
}

// --- Item 1: the start arguments -------------------------------------------

/// Item 1: a start without arguments asks for nothing.
#[test]
fn install_ac_013_no_start_argument_asks_for_no_migration() {
    assert_eq!(parse_start_arguments(&[]), Ok(None));
}

/// Item 1: the choice once and one or more fingerprints are a request, in
/// either order; nothing in the item orders them.
#[test]
fn install_ac_013_a_choice_and_its_fingerprints_are_a_request() {
    let (a, zero) = (fingerprint_argument('a'), fingerprint_argument('0'));
    assert_eq!(
        parse_start_arguments(&arguments(&["--migrate-legacy=remove", &a])),
        Ok(Some(Request {
            choice: Choice::Remove,
            fingerprints: BTreeSet::from([fingerprint_value('a')]),
        }))
    );
    let both = Request {
        choice: Choice::Keep,
        fingerprints: BTreeSet::from([fingerprint_value('a'), fingerprint_value('0')]),
    };
    assert_eq!(
        parse_start_arguments(&arguments(&["--migrate-legacy=keep", &a, &zero])),
        Ok(Some(both.clone()))
    );
    assert_eq!(
        parse_start_arguments(&arguments(&[&zero, "--migrate-legacy=keep", &a])),
        Ok(Some(both))
    );
    let mixed = format!("sha256:{}", "0123456789abcdef".repeat(4));
    assert_eq!(
        parse_start_arguments(&arguments(&[
            "--migrate-legacy=keep",
            &format!("--trust-anchor-sha256={mixed}"),
        ])),
        Ok(Some(Request {
            choice: Choice::Keep,
            fingerprints: BTreeSet::from([mixed]),
        }))
    );
}

/// Item 1: any other argument, either kind without the other and a choice
/// given twice are code 2's whole line, which never repeats a value.
#[test]
fn install_ac_013_any_other_start_argument_is_code_2_without_its_value() {
    let good = fingerprint_argument('a');
    let choice = "--migrate-legacy=remove";
    let upper = format!("--trust-anchor-sha256=sha256:{}", "A".repeat(64));
    let short = format!("--trust-anchor-sha256=sha256:{}", "a".repeat(63));
    let long = format!("--trust-anchor-sha256=sha256:{}", "a".repeat(65));
    let not_hex = format!("--trust-anchor-sha256=sha256:{}g", "a".repeat(63));
    let no_prefix = format!("--trust-anchor-sha256={}", "a".repeat(64));
    let other_prefix = format!("--trust-anchor-sha256=sha1:{}", "a".repeat(64));
    let upper_prefix = format!("--trust-anchor-sha256=SHA256:{}", "a".repeat(64));
    let spaced = format!("--trust-anchor-sha256=sha256:{} ", "a".repeat(64));
    let empty_fingerprint = "--trust-anchor-sha256=".to_string();
    let other_case_name = format!("--Trust-Anchor-Sha256=sha256:{}", "a".repeat(64));
    // The Amendment 2026-11-07 (second), item 5: these precede the route, so
    // a line says nothing of a console installation.
    for list in [
        vec!["--MIGRATE-LEGACY=remove", good.as_str()],
        vec!["--migrate-legacy=remove ", good.as_str()],
        vec!["-migrate-legacy=remove", good.as_str()],
        vec![choice, other_case_name.as_str()],
        vec![choice],
        vec!["--migrate-legacy=keep"],
        vec![good.as_str()],
        vec![good.as_str(), fingerprint_argument('b').as_str()],
        vec![choice, choice, good.as_str()],
        vec![choice, "--migrate-legacy=keep", good.as_str()],
        vec!["--migrate-legacy=purge", good.as_str()],
        vec!["--migrate-legacy=REMOVE", good.as_str()],
        vec!["--migrate-legacy=", good.as_str()],
        vec!["--migrate-legacy", good.as_str()],
        vec!["--migrate-legacy", "remove", good.as_str()],
        vec![choice, "--trust-anchor-sha256"],
        vec![choice, upper.as_str()],
        vec![choice, short.as_str()],
        vec![choice, long.as_str()],
        vec![choice, not_hex.as_str()],
        vec![choice, no_prefix.as_str()],
        vec![choice, other_prefix.as_str()],
        vec![choice, upper_prefix.as_str()],
        vec![choice, spaced.as_str()],
        vec![choice, empty_fingerprint.as_str()],
        vec![choice, good.as_str(), upper.as_str()],
        vec![choice, good.as_str(), "--verbose-option-s57"],
        vec![choice, good.as_str(), "--config=C:\\not-the-agent.toml"],
        vec![choice, good.as_str(), "positional-value-s57"],
        vec![choice, good.as_str(), ""],
        vec!["--some-unknown-switch-s57=its-value-s57"],
    ] {
        let line =
            parse_start_arguments(&arguments(&list)).expect_err(&format!("{list:?} is refused"));
        assert!(line.starts_with(ARGUMENTS_LINE), "{list:?}: {line}");
        assert!(!line.contains(IN_PLACE), "{list:?}: {line}");
        for argument in list {
            let value = argument
                .split_once('=')
                .map_or(argument, |(_, value)| value)
                .trim();
            let known = [
                "remove",
                "keep",
                "",
                "--migrate-legacy",
                "--trust-anchor-sha256",
            ];
            if !known.contains(&value) {
                assert!(!line.contains(value), "{argument} repeated: {line}");
            }
        }
    }
}

/// Item 1: migration arguments on a host that is neither a console layout
/// nor a migration in progress are code 2, before any change.
#[test]
fn install_ac_013_arguments_on_a_host_with_nothing_to_migrate_are_code_2() {
    let closed = test_custody().closed_directory_sddl;
    for console in [
        Console::bare(),
        Console::service_host(&closed),
        Console::service_host(&programdata_child_sddl()),
    ] {
        let before = console.whole();
        let script = Script::new();
        let request = console.request(Choice::Remove);
        let line = arguments_refusal(console.start(Some(&request), &script));
        let digits = console.fingerprint();
        assert!(!line.contains(&digits["sha256:".len()..]), "{line}");
        assert_eq!(script.seen(), [], "no step ran");
        assert_same_tree(&before, &console.whole(), "before any change");
    }
}

/// Item 1: arguments other than those a migration in progress recorded are
/// code 2, before any change.
#[test]
fn install_ac_013_arguments_other_than_the_recorded_are_code_2() {
    let console = Console::new();
    let request = console.request(Choice::Remove);
    console.crash_at(Point::Stage("agent.toml"), &request);
    let before = console.whole();
    let other_digest = fingerprint_value('c');
    let other_choice = console.request(Choice::Keep);
    let other_fingerprint = Request {
        choice: Choice::Remove,
        fingerprints: BTreeSet::from([other_digest.clone()]),
    };
    let more_fingerprints = Request {
        choice: Choice::Remove,
        fingerprints: BTreeSet::from([console.fingerprint(), other_digest.clone()]),
    };
    for other in [other_choice, other_fingerprint, more_fingerprints] {
        let line = arguments_refusal(console.start(Some(&other), &Script::new()));
        assert!(!line.contains(&other_digest["sha256:".len()..]), "{line}");
        assert_same_tree(&before, &console.whole(), "before any change");
    }

    // The exact set: a subset of what was recorded is other arguments too,
    // which would otherwise go on to item 2.3's code 6.
    let console = Console::new();
    let other = common::generate_test_pki(common::TEST_AGENT_ID);
    let both = format!("{}{}", console.pki.trust_anchor_pem, other.trust_anchor_pem);
    console.write("server-ca.pem", both.as_bytes());
    let request = Request {
        choice: Choice::Remove,
        fingerprints: fingerprints_of(both.as_bytes()),
    };
    assert_eq!(request.fingerprints.len(), 2);
    console.crash_at(Point::Stage("agent.toml"), &request);
    let before = console.whole();
    let fewer = console.request(Choice::Remove);
    arguments_refusal(console.start(Some(&fewer), &Script::new()));
    assert_same_tree(&before, &console.whole(), "before any change");
}

// --- Item 2: the order of the route -----------------------------------------

/// Item 2: 2.1 to 2.6 in order, the record before the staged files (item 3)
/// and the staged files in item 2.5's order; the start goes on after the
/// commit.
#[test]
fn install_ac_013_the_route_runs_in_order_and_commits() {
    let console = Console::new();
    let request = console.request(Choice::Remove);
    let script = Script::new();
    assert_eq!(
        console.start(Some(&request), &script),
        Ok(Prepared::Committed)
    );

    let restrictions = script.restrictions();
    assert_eq!(
        restrictions.iter().copied().collect::<BTreeSet<_>>(),
        FILES.into_iter().collect::<BTreeSet<_>>(),
        "each file is restricted once"
    );
    assert_eq!(restrictions.len(), FILES.len());
    let mut expected = vec![Point::CloseProductRoot, Point::CloseDataRoot];
    expected.extend(FILES.iter().copied().map(Point::Check));
    expected.extend(FILES.iter().copied().map(Point::Read));
    for name in &restrictions {
        expected.extend([Point::Restrict(name), Point::Restricted(name)]);
    }
    expected.extend([
        Point::CreateStaging,
        Point::WriteRecord,
        Point::Stage("agent.toml"),
        Point::Stage("server-ca.pem"),
        Point::Stage("identity"),
        Point::Stage("cert.pem"),
        Point::Stage("key.dat"),
        Point::Stage("identity.json"),
        Point::CheckStaged,
        Point::FirstRename,
        Point::SecondRename,
    ]);
    assert_eq!(script.seen(), expected);
    console.assert_committed();
}

/// Item 2: the lock comes before any change.
#[test]
fn install_ac_013_the_lock_is_taken_before_any_change() {
    let console = Console::new();
    let before = console.whole();
    let request = console.request(Choice::Keep);
    let script = Script::new();
    let mut taken = false;
    let result = console.start_with_lock(Some(&request), &script, &mut || {
        assert_same_tree(&before, &console.whole(), "the lock comes first");
        assert_eq!(script.seen(), [], "no step before the lock");
        taken = true;
        Ok(())
    });
    assert_eq!(result, Ok(Prepared::Committed));
    assert!(taken, "the lock was taken");
}

/// Item 2: code 10 while an elevated console agent runs, the lock's own
/// line unchanged, and nothing changes.
#[test]
fn install_ac_013_a_held_lock_is_code_10_unchanged() {
    let console = Console::new();
    let before = console.whole();
    let request = console.request(Choice::Remove);
    let held = Stop::Refused {
        code: 10,
        line: "cg-agent: another agent holds the instance lock".to_string(),
    };
    let script = Script::new();
    let result = console.start_with_lock(Some(&request), &script, &mut || Err(held.clone()));
    let Err(Stop::Refused { code, line }) = result else {
        panic!("refused: {result:?}");
    };
    assert_eq!(code, 10);
    assert!(
        line.starts_with("cg-agent: another agent holds the instance lock"),
        "the lock's line first: {line}"
    );
    assert_in_place_line(&line, &[]);
    assert_eq!(script.seen(), [], "no step ran");
    assert_same_tree(&before, &console.whole(), "nothing changes");
}

/// Item 1: "no privilege is enabled (ADR-0010 §Compliance)". The SCM target
/// reads the running service's token; here, nothing of the agent's source
/// calls what enables one. It passes against the stub by design: a guard.
#[test]
fn install_ac_013_no_source_enables_a_privilege() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&source, &mut files);
    assert!(
        files
            .iter()
            .any(|file| file.ends_with("service/migration.rs")),
        "the migration's source is among them"
    );
    for file in files {
        let text = std::fs::read_to_string(&file).expect("read a source file");
        for call in ["AdjustTokenPrivileges", "RtlAdjustPrivilege"] {
            assert!(!text.contains(call), "{}: {call}", file.display());
        }
    }
}

// --- Item 2.1: closing the directories --------------------------------------

/// Item 2.1: both directories get the closed DACL, which a later refusal
/// does not undo and which propagates to no child; the refusal names them.
#[test]
fn install_ac_013_closing_the_directories_stays_and_reaches_no_child() {
    let console = Console::new();
    console.write("stray.txt", b"not the agent's");
    let before = console.files();
    let request = console.request(Choice::Remove);
    let script = Script::new();
    let line = identity_refusal(console.start(Some(&request), &script));
    assert_eq!(
        script.seen()[..2],
        [Point::CloseProductRoot, Point::CloseDataRoot]
    );
    console.assert_directories_closed();
    assert_in_place_line(&line, &["CyberGuard", "agent"]);
    assert_same_tree(&before, &console.files(), "no child's DACL changed");
    assert!(
        security(&console.file("agent.toml")).contains(";BU)"),
        "agent.toml keeps its inherited entries"
    );
}

/// Item 2.1: a reparse point in either directory's place stops the start
/// and what it points to is not touched. Item 2.1 makes that code 13 with
/// the file log's reason, on a migrating start; a start that reads the data
/// root without following it (the amendment's preamble) sees no console
/// layout at all, which item 1 makes code 2. The text allows both.
#[test]
fn install_ac_013_a_reparse_point_for_a_directory_stops_the_start() {
    fn assert_stopped(result: Result<Prepared, Stop>) {
        let (code, line) = refused(result);
        match code {
            13 => {
                assert!(line.starts_with(LOG_LINE), "{line}");
                assert_in_place_line(&line, &[]);
            }
            2 => assert!(line.starts_with(ARGUMENTS_LINE), "{line}"),
            _ => panic!("{line}"),
        }
    }

    // The refusal's line reads no object through the reparse point either:
    // a file behind it that has item 2.4's security is not named.
    let console = Console::new();
    set_dacl(
        &console.file("agent.toml"),
        &console.custody.shared_file_sddl,
    );
    let target = console.outside("the-real-data-root");
    std::fs::rename(&console.paths.data_root, &target).expect("move the data root");
    junction(&console.paths.data_root, &target);
    let before = tree(&target);
    let request = console.request(Choice::Remove);
    let result = console.start(Some(&request), &Script::new());
    assert!(!format!("{result:?}").contains("agent.toml"), "{result:?}");
    assert_stopped(result);
    assert_same_tree(&before, &tree(&target), "not followed");

    let console = Console::new();
    set_dacl(
        &console.file("agent.toml"),
        &console.custody.shared_file_sddl,
    );
    let target = console.outside("the-real-product-root");
    std::fs::rename(&console.paths.product_root, &target).expect("move <P>");
    junction(&console.paths.product_root, &target);
    let before = tree(&target);
    let request = console.request(Choice::Remove);
    let result = console.start(Some(&request), &Script::new());
    assert!(!format!("{result:?}").contains("agent.toml"), "{result:?}");
    assert_stopped(result);
    assert_same_tree(&before, &tree(&target), "not followed");
}

/// Item 2.1: a DACL that lets anyone else change it or take ownership is
/// code 13 with item 2.2's repair, on either directory; that directory is
/// left as it was.
#[test]
fn install_ac_013_a_directory_others_can_change_is_code_13_with_the_repair() {
    let good = programdata_child_sddl();
    for extra in [
        "(A;OICI;WD;;;BU)",
        "(A;OICI;WO;;;BU)",
        "(A;OICI;FA;;;BU)",
        "(A;;WD;;;WD)",
    ] {
        let bad = format!("{good}{extra}");
        for (product, data, name, word) in [
            (bad.as_str(), good.as_str(), "CyberGuard", "CyberGuard"),
            (good.as_str(), bad.as_str(), "CyberGuard/agent", "agent"),
        ] {
            let console = Console::with_directories(product, data);
            let before = console.whole();
            let files = console.files();
            let request = console.request(Choice::Remove);
            let script = Script::new();
            let line = log_refusal(console.start(Some(&request), &script));
            assert!(line.to_lowercase().contains("restart"), "{line}");
            assert_in_place_line(&line, &[]);
            assert!(names(&line, word), "the directory refused: {line}");
            assert_eq!(
                console.whole()[name].security,
                before[name].security,
                "{extra} on {name}: not closed"
            );
            assert!(script.restrictions().is_empty(), "{line}");
            console.assert_console_in_place(&files, &[]);
        }
    }
}

/// Item 2.1: a null DACL lets everyone change the directory, with no entry
/// that says so, and an entry of another type can too: each is code 13 with
/// item 2.2's repair, on either directory, which is left as it was.
#[test]
fn install_ac_013_a_null_dacl_or_an_entry_of_another_type_is_code_13() {
    let callback = format!(
        "{}(XA;;WD;;;WD;(@User.Title==\"PM\"))",
        programdata_child_sddl()
    );
    for dacl in ["D:NO_ACCESS_CONTROL", callback.as_str()] {
        for (product_root, word) in [(true, "CyberGuard"), (false, "agent")] {
            let console = Console::new();
            let path = if product_root {
                &console.paths.product_root
            } else {
                &console.paths.data_root
            };
            set_dacl(path, dacl);
            let set = security(path);
            let files = console.files();
            let request = console.request(Choice::Remove);
            let script = Script::new();
            let line = log_refusal(console.start(Some(&request), &script));
            assert!(line.to_lowercase().contains("restart"), "{line}");
            assert_in_place_line(&line, &[]);
            assert!(names(&line, word), "the directory refused: {line}");
            assert_eq!(security(path), set, "{dacl} on {word}: not closed");
            assert!(script.restrictions().is_empty(), "{line}");
            console.assert_console_in_place(&files, &[]);
        }
    }
}

/// Item 2.1: a directory owned by another account is code 13 with item
/// 2.2's repair. The owner here is this user or Administrators; the custody
/// accepts neither.
#[test]
fn install_ac_013_a_directory_of_another_account_is_code_13_with_the_repair() {
    let mut console = Console::new();
    console.custody.owners = vec![Principal::System];
    let before = console.whole();
    let request = console.request(Choice::Remove);
    let line = log_refusal(console.start(Some(&request), &Script::new()));
    assert!(line.to_lowercase().contains("restart"), "{line}");
    assert!(line.contains("Administrators"), "{line}");
    assert_in_place_line(&line, &[]);
    assert!(
        names(&line, "CyberGuard") || names(&line, "agent"),
        "the directory refused: {line}"
    );
    assert_same_tree(&before, &console.whole(), "nothing changes");
}

/// Item 2.1: an inherit-only entry applies to the directory's children, not
/// to the directory: `%ProgramData%`'s `CREATOR OWNER` entry, which its
/// children inherit, lets no one change them.
#[test]
fn install_ac_013_an_inherit_only_entry_lets_no_one_change_the_directory() {
    let sddl = format!("{}(A;OICIIO;GA;;;CO)", programdata_child_sddl());
    let console = Console::with_directories(&sddl, &sddl);
    console.migrate(Choice::Remove);
    console.assert_committed();
}

/// Item 2.1: "or stops with code 13 when it cannot".
#[test]
fn install_ac_013_a_directory_that_cannot_be_closed_is_code_13() {
    let console = Console::new();
    let before = console.whole();
    let request = console.request(Choice::Remove);
    let script = Script::new().fail(Point::CloseProductRoot);
    let line = log_refusal(console.start(Some(&request), &script));
    assert_in_place_line(&line, &[]);
    assert_eq!(script.seen(), [Point::CloseProductRoot]);
    assert_same_tree(&before, &console.whole(), "nothing changes");

    let console = Console::new();
    let before = console.whole();
    let files = console.files();
    let request = console.request(Choice::Remove);
    let script = Script::new().fail(Point::CloseDataRoot);
    let line = log_refusal(console.start(Some(&request), &script));
    assert_in_place_line(&line, &["CyberGuard"]);
    assert_eq!(
        script.seen(),
        [Point::CloseProductRoot, Point::CloseDataRoot]
    );
    assert_eq!(
        security(&console.paths.product_root),
        with_owner(&console.custody.closed_directory_sddl),
        "<P> stays closed"
    );
    assert_eq!(
        console.whole()["CyberGuard/agent"].security,
        before["CyberGuard/agent"].security
    );
    console.assert_console_in_place(&files, &[]);
}

// --- Item 2.2: what is there ------------------------------------------------

/// Item 2.2: `<P>` holds no entry but the data root; another is named,
/// neither read nor deleted, before any file changes.
#[test]
fn install_ac_013_another_entry_under_the_product_root_is_code_5() {
    for directory in [false, true] {
        let console = Console::new();
        let stray = console.paths.product_root.join("not-the-agents");
        if directory {
            std::fs::create_dir(&stray).expect("a stray directory");
        } else {
            std::fs::write(&stray, b"its content").expect("a stray file");
        }
        let before = console.files();
        let request = console.request(Choice::Remove);
        let script = Script::new();
        let line = identity_refusal(console.start(Some(&request), &script));
        assert!(line.contains("not-the-agents"), "{line}");
        assert!(!line.contains("its content"), "{line}");
        assert!(stray.exists(), "not deleted");
        assert_no_file_changed(&console, &before, &script, &line);
    }
}

/// Item 2.2: another entry in the data root is named, neither read nor
/// deleted, and refused with its repair.
#[test]
fn install_ac_013_another_entry_in_the_data_root_is_code_5_with_the_repair() {
    for directory in [false, true] {
        let console = Console::new();
        let stray = console.file("logs");
        if directory {
            std::fs::create_dir(&stray).expect("a stray directory");
        } else {
            std::fs::write(&stray, b"its content").expect("a stray file");
        }
        let before = console.files();
        let request = console.request(Choice::Remove);
        let script = Script::new();
        let line = identity_refusal(console.start(Some(&request), &script));
        assert!(line.contains("logs"), "{line}");
        assert!(!line.contains("its content"), "{line}");
        let repair = line.to_lowercase();
        assert!(
            repair.contains("administrator") && repair.contains("removes"),
            "{line}"
        );
        assert!(stray.exists(), "not deleted");
        assert_no_file_changed(&console, &before, &script, &line);
    }
}

/// Item 2.2: each of the five files must be there; any identity file left
/// is still a console layout.
#[test]
fn install_ac_013_a_missing_file_is_code_5() {
    for name in FILES {
        let console = Console::new();
        std::fs::remove_file(console.file(name)).expect("remove");
        let before = console.files();
        let request = console.request(Choice::Remove);
        let script = Script::new();
        let line = identity_refusal(console.start(Some(&request), &script));
        assert!(line.contains(name), "{line}");
        assert_no_file_changed(&console, &before, &script, &line);
    }
}

/// Item 2.2: each file is a regular file; a directory or a reparse point
/// in its place is refused by name and not followed.
#[test]
fn install_ac_013_something_else_in_a_file_s_place_is_code_5() {
    for name in FILES {
        let console = Console::new();
        std::fs::remove_file(console.file(name)).expect("remove");
        std::fs::create_dir(console.file(name)).expect("a directory of that name");
        let before = console.files();
        let request = console.request(Choice::Remove);
        let script = Script::new();
        let line = identity_refusal(console.start(Some(&request), &script));
        assert!(line.contains(name), "{line}");
        assert_no_file_changed(&console, &before, &script, &line);

        let console = Console::new();
        let target = console.outside("target");
        std::fs::create_dir(&target).expect("a target");
        std::fs::remove_file(console.file(name)).expect("remove");
        junction(&console.file(name), &target);
        let before = console.files();
        let request = console.request(Choice::Remove);
        let script = Script::new();
        let line = identity_refusal(console.start(Some(&request), &script));
        assert!(line.contains(name), "{line}");
        assert_no_file_changed(&console, &before, &script, &line);
    }
}

/// Item 2.2 and the amendment's preamble: a symbolic link to the real file
/// is refused by name, and what it points to is not read. Following it
/// would commit, so this tells the two apart where a junction cannot: a
/// junction to a file does not open. Creating one needs a privilege that an
/// unelevated token here does not hold, which rust-ci's Windows job does.
#[test]
fn install_ac_013_a_symbolic_link_in_a_file_s_place_is_not_followed() {
    for name in FILES {
        let console = Console::new();
        let target = console.outside(name);
        std::fs::rename(console.file(name), &target).expect("move the file");
        match std::os::windows::fs::symlink_file(&target, console.file(name)) {
            Ok(()) => {}
            // ERROR_PRIVILEGE_NOT_HELD.
            Err(error) if error.raw_os_error() == Some(1314) && !in_ci() => {
                eprintln!("this token cannot create a symbolic link");
                return;
            }
            Err(error) => panic!("a symbolic link: {error}"),
        }
        let outside = tree(&target);
        let before = console.files();
        let request = console.request(Choice::Remove);
        let script = Script::new();
        let line = identity_refusal(console.start(Some(&request), &script));
        assert!(line.contains(name), "{line}");
        assert_no_file_changed(&console, &before, &script, &line);
        assert_same_tree(&outside, &tree(&target), "not followed");
    }
}

/// Item 2.2: "owned by SYSTEM or Administrators" for a file, and "a ...
/// file owned by another account ... is refused with the repair", before
/// any file changes; the leftover temporary's repair is a deletion. Item
/// 2.1 checks the directories against the same owners, so this needs a token
/// with two owners to give: an elevated one, as in rust-ci's Windows job,
/// whose default owner is Administrators and not its user.
#[test]
fn install_ac_013_a_file_of_another_account_is_code_5_with_the_repair() {
    let owner = default_owner_sid();
    if owner == sid() {
        assert!(!in_ci(), "this job's token has one owner to give");
        eprintln!("an unelevated token has one owner to give");
        return;
    }
    let cases = FILES.map(|name| (name, "restart"));
    for (name, repair) in cases.into_iter().chain([(HYGIENE, "deletes")]) {
        let mut console = Console::new();
        console.write(HYGIENE, b"a consumed token");
        console.custody.owners = vec![Principal::Sid(sid_bytes(&owner))];
        set_owner(&console.file(name), sid());
        let before = console.files();
        let request = console.request(Choice::Remove);
        let script = Script::new();
        let line = identity_refusal(console.start(Some(&request), &script));
        assert!(line.contains(name), "{line}");
        assert!(line.to_lowercase().contains(repair), "{line}");
        assert_no_file_changed(&console, &before, &script, &line);
    }
}

/// Item 2.2: a file with another link is refused with its repair, before
/// any file changes.
#[test]
fn install_ac_013_a_hard_linked_file_is_code_5_with_the_repair() {
    for name in FILES {
        let console = Console::new();
        std::fs::hard_link(console.file(name), console.outside("another-name")).expect("link");
        let before = console.files();
        let request = console.request(Choice::Remove);
        let script = Script::new();
        let line = identity_refusal(console.start(Some(&request), &script));
        assert!(line.contains(name), "{line}");
        assert!(line.contains("other names"), "{line}");
        assert_no_file_changed(&console, &before, &script, &line);
    }
}

/// Item 2.2: an identity file's DACL allows SYSTEM alone, SYSTEM and one
/// other SID, the same in each file that has one, or SYSTEM and
/// Administrators. Anything else is refused, and item 6 is its only route.
#[test]
fn install_ac_013_an_identity_file_that_allows_more_is_code_5() {
    let user = sid();
    let two_others = identity_dacl("(A;;FR;;;BU)");
    let with_administrators = identity_dacl("(A;;FR;;;BA)");
    let another_account = format!("D:P(A;;FA;;;{user})(A;;FA;;;LS)");
    let not_the_service = "D:P(A;;FA;;;SY)".to_string();
    let everyone = format!("D:P(A;;FA;;;{user})(A;;FR;;;WD)(A;;FA;;;SY)");
    let no_dacl = "D:NO_ACCESS_CONTROL".to_string();
    for name in IDENTITY_FILES {
        for dacl in [
            &two_others,
            &with_administrators,
            &another_account,
            &not_the_service,
            &everyone,
            &no_dacl,
        ] {
            let console = Console::new();
            set_dacl(&console.file(name), dacl);
            let before = console.files();
            let request = console.request(Choice::Remove);
            let script = Script::new();
            let line = identity_refusal(console.start(Some(&request), &script));
            // Of two files that name different accounts either can be named.
            assert!(
                IDENTITY_FILES.iter().any(|file| line.contains(file)),
                "{dacl}: {line}"
            );
            if *dacl != another_account {
                assert!(line.contains(name), "{dacl}: {line}");
            }
            assert_no_file_changed(&console, &before, &script, &line);
        }
    }
}

/// Item 2.2: the DACLs it accepts, and the enrolling account each gives
/// (the amendment's preamble, item 2.5): none when no file names one, and
/// the same other SID however the files mix the accepted shapes.
#[test]
fn install_ac_013_the_identity_dacls_accepted_and_the_enrolling_account() {
    let user = sid();
    let alone = format!("D:P(A;;FA;;;{user})");
    let administrators = format!("D:P(A;;FA;;;{user})(A;;FA;;;BA)");
    let console_dacl = identity_dacl("");
    for (dacls, enrolling) in [
        ([&alone, &alone, &alone], None),
        ([&administrators, &administrators, &administrators], None),
        ([&alone, &administrators, &alone], None),
        (
            [&console_dacl, &console_dacl, &console_dacl],
            Some(ENROLLING_SID),
        ),
        (
            [&console_dacl, &administrators, &alone],
            Some(ENROLLING_SID),
        ),
        ([&alone, &alone, &console_dacl], Some(ENROLLING_SID)),
    ] {
        let console = Console::new();
        for (name, dacl) in IDENTITY_FILES.into_iter().zip(dacls) {
            set_dacl(&console.file(name), dacl);
        }
        let request = console.request(Choice::Keep);
        console.crash_at(Point::Stage("agent.toml"), &request);
        assert_eq!(
            console.staged_record().enrolling_account.as_deref(),
            enrolling,
            "{dacls:?}"
        );
    }
}

/// Item 2.2: the leftover temporary is optional, never read or adopted, and
/// item 2.4 treats it as a copy: a temporary that holds what no file should
/// goes through.
#[test]
fn install_ac_013_a_leftover_temporary_is_a_copy_and_is_never_read() {
    let console = Console::new();
    let bytes = format!("this is = not toml\ntoken = \"{TOKEN}\"\n");
    console.write(HYGIENE, bytes.as_bytes());
    let request = console.request(Choice::Remove);
    let script = Script::new();
    assert_eq!(
        console.start(Some(&request), &script),
        Ok(Prepared::Committed)
    );
    assert!(script.saw(Point::Restrict(HYGIENE)), "restricted as a copy");
    console.assert_committed();
    assert_eq!(
        std::fs::read(console.legacy(HYGIENE)).expect("a copy"),
        bytes.as_bytes(),
        "it moves with the legacy directory"
    );
    assert_eq!(
        security(&console.legacy(HYGIENE)),
        with_owner(&console.custody.shared_file_sddl)
    );
    assert!(
        console.record().files.contains_key(HYGIENE),
        "item 5 matches its file ID"
    );
}

/// Item 2.2: a leftover temporary that fails the checks is refused with its
/// repair, a deletion by an administrator.
#[test]
fn install_ac_013_a_leftover_temporary_that_fails_the_checks_is_code_5_with_the_repair() {
    let linked = Console::new();
    linked.write(HYGIENE, b"x");
    std::fs::hard_link(linked.file(HYGIENE), linked.outside("another-name")).expect("link");
    let directory = Console::new();
    std::fs::create_dir(directory.file(HYGIENE)).expect("a directory of that name");
    for console in [linked, directory] {
        let before = console.files();
        let request = console.request(Choice::Remove);
        let script = Script::new();
        let line = identity_refusal(console.start(Some(&request), &script));
        assert!(line.contains(HYGIENE), "{line}");
        assert!(line.contains("deletes"), "{line}");
        assert_no_file_changed(&console, &before, &script, &line);
    }
}

/// Item 2.2: the data root's lookup is not case-sensitive, so a name in
/// another ASCII case is that file, as far as item 5, which finds it in the
/// legacy directory under that spelling and reaches it.
#[test]
fn install_ac_013_a_name_in_another_ascii_case_is_that_file() {
    let console = Console::new();
    std::fs::rename(console.file("agent.toml"), console.file("Agent.TOML")).expect("rename");
    console.migrate(Choice::Remove);
    console.assert_committed();
    let pass = console.copies(&Script::new()).expect("the pass runs");
    assert_eq!(pass.not_reached, Vec::<String>::new(), "{pass:?}");
    assert!(pass.directory_removed, "{pass:?}");
}

/// Item 2.2: "with case-sensitive lookup off": a data root that has it on
/// can hold two names for one of those files, and is refused. A Windows
/// without the feature cannot build the fixture, and has no such directory;
/// in CI that fails, so that a green run says the test ran.
#[test]
fn install_ac_013_a_case_sensitive_data_root_is_code_5() {
    let Some(console) = Console::case_sensitive() else {
        assert!(!in_ci(), "fsutil cannot make a directory case-sensitive");
        eprintln!("fsutil cannot make a directory case-sensitive here");
        return;
    };
    let before = console.files();
    let request = console.request(Choice::Remove);
    let script = Script::new();
    let line = identity_refusal(console.start(Some(&request), &script));
    assert_no_file_changed(&console, &before, &script, &line);
}

// --- Item 2.3: reading each file once ---------------------------------------

/// Item 2.3: a file held open for writing or deletion, or without sharing
/// read access, fails the open: code 5, the file named.
#[test]
fn install_ac_013_a_file_held_open_is_code_5_by_name() {
    for name in FILES {
        for (access, share) in [
            (ACCESS_WRITE, SHARE_ALL),
            (ACCESS_DELETE, SHARE_ALL),
            (ACCESS_READ, 0),
        ] {
            let console = Console::new();
            let before = console.files();
            let request = console.request(Choice::Remove);
            let script = Script::new();
            let held = hold(&console.file(name), access, share);
            let result = console.start(Some(&request), &script);
            drop(held);
            let line = identity_refusal(result);
            assert!(line.contains(name), "{line}");
            assert_no_file_changed(&console, &before, &script, &line);
        }
    }
}

/// Item 2.3: "sharing read access alone": a reader that shares everything
/// is neither writing nor deleting, so the read goes through and the route
/// reaches the commit. Its handle under the data root can still fail the
/// first rename (item 2.6), with the console in place.
#[test]
fn install_ac_013_a_file_a_reader_holds_open_is_read() {
    for name in FILES {
        let console = Console::new();
        let request = console.request(Choice::Remove);
        let script = Script::new();
        let held = hold(&console.file(name), ACCESS_READ, SHARE_ALL);
        let result = console.start(Some(&request), &script);
        drop(held);
        assert!(script.saw(Point::FirstRename), "{name}: {result:?}");
        if result != Ok(Prepared::Committed) {
            let line = identity_refusal(result);
            assert_in_place_line(&line, &RESTRICTED);
        }
    }
}

/// Item 2.3: the leftover temporary is the one file not read, so a handle
/// on it does not stop the route there. That handle under the data root can
/// still fail the first rename (item 2.6), with the console in place.
#[test]
fn install_ac_013_a_leftover_temporary_held_open_is_not_refused_at_the_read() {
    let console = Console::new();
    console.write(HYGIENE, b"x");
    let request = console.request(Choice::Remove);
    let script = Script::new();
    let held = hold(&console.file(HYGIENE), ACCESS_WRITE, SHARE_ALL);
    let result = console.start(Some(&request), &script);
    drop(held);
    assert!(script.saw(Point::FirstRename), "{result:?}");
    if result != Ok(Prepared::Committed) {
        let line = identity_refusal(result);
        assert_in_place_line(&line, &RESTRICTED);
    }
}

/// Item 2.3: `agent.toml` is a valid configuration with `[enrollment]`,
/// HTTPS URLs, the four console paths and no `setup` table: otherwise code
/// 2, read before any file changes, the file's text quoted nowhere: not the
/// value that fails, which is what a careless reason would repeat.
#[test]
fn install_ac_013_a_configuration_the_route_cannot_take_is_code_2() {
    let probe = Console::bare();
    let root = forward(&probe.paths.data_root);
    let text = probe.config_text.clone();
    let swap = |from: &str, to: &str| {
        assert!(text.contains(from), "{from}");
        text.replace(from, to)
    };
    let heartbeat_line = format!("heartbeat_url = \"{HEARTBEAT_URL}\"\n");
    let anchor = format!("\"{root}/server-ca.pem\"");
    let cert = format!("\"{root}/cert.pem\"");
    let key = format!("\"{root}/key.dat\"");
    let identity = format!("\"{root}/identity.json\"");
    let enrollment_at = text.find("[enrollment]").expect("[enrollment]");
    let tls_at = text.find("[tls]").expect("[tls]");
    let without_enrollment = format!("{}{}", &text[..enrollment_at], &text[tls_at..]);
    let verbatim = format!("\"//?/{root}/key.dat\"");
    let anchor_line = format!("trust_anchor_path = {anchor}\n");
    for (mutated, word, value) in [
        (
            format!("{text}\nthis is not toml = = \"{TOKEN}\"\n"),
            "",
            "this is not toml",
        ),
        (without_enrollment, "", ""),
        (
            swap(ENROLL_URL, "http://enroll.example.test:8444"),
            "https",
            "enroll.example.test",
        ),
        (
            swap(HEARTBEAT_URL, "http://ingest.example.test:8443"),
            "https",
            "ingest.example.test",
        ),
        (swap(&heartbeat_line, ""), "server.heartbeat_url", ""),
        (swap(&anchor_line, ""), "", ""),
        (
            swap(&anchor, &format!("\"{root}/other-anchor.pem\"")),
            "",
            "other-anchor.pem",
        ),
        (
            swap(&cert, &format!("\"{root}/identity/cert.pem\"")),
            "",
            "identity/cert.pem",
        ),
        (swap(&cert, &key), "", "agent/key.dat"),
        (swap(&key, "\"key.dat\""), "", ""),
        (swap(&key, &verbatim), "", "//?/"),
        (
            swap(&identity, &format!("\"{root}/../agent/identity.json\"")),
            "",
            "/../",
        ),
        (
            format!("{text}\n[setup]\ntrust_anchor_sha256 = [\"sha256:0\"]\n"),
            "setup",
            "sha256:0",
        ),
    ] {
        let console = Console::new();
        // The paths name the probe's root: this console's are its own.
        let mutated = mutated.replace(&root, &forward(&console.paths.data_root));
        console.write("agent.toml", mutated.as_bytes());
        let before = console.files();
        let request = console.request(Choice::Remove);
        let script = Script::new();
        let line = config_refusal(
            console.start(Some(&request), &script),
            &console.paths.config,
        );
        assert!(line.to_lowercase().contains(word), "{word}: {line}");
        if !value.is_empty() {
            assert_quotes_none(&line, &[value]);
        }
        assert_no_file_changed(&console, &before, &script, &line);
    }
}

/// Item 2.3: a console path is the data root's file of its name, with
/// either slash and equal but for ASCII case.
#[test]
fn install_ac_013_a_console_path_may_use_either_slash_and_any_ascii_case() {
    let console = Console::new();
    let root = forward(&console.paths.data_root);
    let mut text = console.config_text.clone();
    for (name, spelled) in [
        (
            "server-ca.pem",
            console.file("server-ca.pem").display().to_string(),
        ),
        ("cert.pem", format!("{root}/CERT.PEM").to_ascii_uppercase()),
        (
            "key.dat",
            console
                .file("key.dat")
                .display()
                .to_string()
                .to_ascii_lowercase(),
        ),
        ("identity.json", format!("{root}\\identity.json")),
    ] {
        let from = format!("\"{root}/{name}\"");
        assert!(text.contains(&from), "{from}");
        // A literal string: a backslash is no escape there.
        text = text.replace(&from, &format!("'{spelled}'"));
    }
    console.write("agent.toml", text.as_bytes());
    console.migrate(Choice::Remove);
    console.assert_committed();
}

/// Item 2.3: the distinct certificates of `server-ca.pem` are exactly those
/// of the fingerprints given: otherwise code 6, the value repeated nowhere.
#[test]
fn install_ac_013_a_trust_anchor_other_than_the_fingerprints_is_code_6() {
    let other = common::generate_test_pki(common::TEST_AGENT_ID);
    let other_fingerprint = fingerprints_of(other.trust_anchor_pem.as_bytes())
        .pop_first()
        .expect("a certificate");
    let cases: [(Bytes, Fingerprints); 6] = [
        // Another certificate than the one given.
        (&|c| c.pki.trust_anchor_pem.clone().into_bytes(), &|_| {
            BTreeSet::from([other_fingerprint.clone()])
        }),
        // A fingerprint with no certificate.
        (&|c| c.pki.trust_anchor_pem.clone().into_bytes(), &|c| {
            BTreeSet::from([c.fingerprint(), other_fingerprint.clone()])
        }),
        // A certificate with no fingerprint.
        (
            &|c| format!("{}{}", c.pki.trust_anchor_pem, other.trust_anchor_pem).into_bytes(),
            &|c| BTreeSet::from([c.fingerprint()]),
        ),
        // No certificate.
        (&|_| b"not a pem document".to_vec(), &|c| {
            BTreeSet::from([c.fingerprint()])
        }),
        (&|_| Vec::new(), &|c| BTreeSet::from([c.fingerprint()])),
        // A later section TLS refuses.
        (
            &|c| {
                format!(
                    "{}-----BEGIN CERTIFICATE-----\nnot base64 !!\n-----END CERTIFICATE-----\n",
                    c.pki.trust_anchor_pem
                )
                .into_bytes()
            },
            &|c| BTreeSet::from([c.fingerprint()]),
        ),
    ];
    for (anchor, fingerprints) in cases {
        let console = Console::new();
        console.write("server-ca.pem", &anchor(&console));
        let before = console.files();
        let request = Request {
            choice: Choice::Remove,
            fingerprints: fingerprints(&console),
        };
        let script = Script::new();
        let line = tls_refusal(console.start(Some(&request), &script));
        let found = console.fingerprint();
        for digits in request
            .fingerprints
            .iter()
            .chain([&found, &other_fingerprint])
        {
            assert_quotes_none(&line, &[&digits["sha256:".len()..]]);
        }
        assert_quotes_none(&line, &["not a pem document", "not base64"]);
        assert_no_file_changed(&console, &before, &script, &line);
    }
}

/// Item 2.3: "the distinct certificates": a certificate twice is one; two
/// certificates need both fingerprints. Item 2.5: the staged anchor holds
/// the verified certificates and nothing else of the file.
#[test]
fn install_ac_013_the_staged_anchor_holds_the_verified_certificates() {
    let console = Console::new();
    let anchor = console.pki.trust_anchor_pem.clone();
    console.write(
        "server-ca.pem",
        format!("a note of the administrator's\n{anchor}{anchor}").as_bytes(),
    );
    console.migrate(Choice::Keep);
    let staged = std::fs::read(&console.paths.trust_anchor).expect("read");
    assert_eq!(fingerprints_of(&staged), [console.fingerprint()].into());
    assert!(
        !String::from_utf8_lossy(&staged).contains("a note"),
        "from the verified certificates"
    );

    let console = Console::new();
    let other = common::generate_test_pki(common::TEST_AGENT_ID);
    let both = format!("{}{}", console.pki.trust_anchor_pem, other.trust_anchor_pem);
    console.write("server-ca.pem", both.as_bytes());
    let given = fingerprints_of(both.as_bytes());
    assert_eq!(given.len(), 2);
    let request = Request {
        choice: Choice::Keep,
        fingerprints: given.clone(),
    };
    assert_eq!(
        console.start(Some(&request), &Script::new()),
        Ok(Prepared::Committed)
    );
    let staged = std::fs::read(&console.paths.trust_anchor).expect("read");
    assert_eq!(fingerprints_of(&staged), given);
    let recorded: BTreeSet<String> = console.record().fingerprints.into_iter().collect();
    assert_eq!(recorded, given, "item 2.5: the fingerprints given");
}

/// Item 2.3: the identity that fails its checks is code 5 before any file
/// changes: a key that does not unseal to a seed of this fingerprint, a
/// `cert.pem` that is not one certificate of that key and `agent_id`, an
/// `identity.json` that does not parse, has expired or names another agent.
/// The line quotes no value of those files.
#[test]
fn install_ac_013_an_identity_that_fails_its_checks_is_code_5() {
    let other_key = common::generate_test_pki(common::TEST_AGENT_ID);
    let other_id = common::generate_test_pki("01934abc-def0-7000-89ab-0000000000aa");
    let store = default_store();
    let identity_with = |console: &Console, from: &str, to: &str| {
        let text = std::fs::read_to_string(console.file("identity.json")).expect("read");
        assert!(text.contains(from), "{from}");
        text.replace(from, to).into_bytes()
    };
    let cases: [(&str, Bytes); 15] = [
        ("key.dat", &|_| b"not sealed by DPAPI".to_vec()),
        ("key.dat", &|_| Vec::new()),
        ("key.dat", &|_| store.seal(&[7u8; 16]).expect("seal")),
        ("key.dat", &|_| {
            store.seal(&other_key.agent_seed).expect("seal")
        }),
        ("identity.json", &|_| b"{\"agent_id\": \"01934".to_vec()),
        ("identity.json", &|c| {
            identity_with(c, &c.key_fingerprint(), &"0".repeat(64))
        }),
        ("identity.json", &|c| {
            identity_with(c, "2036-11-01T00:00:00Z", "soon")
        }),
        ("identity.json", &|c| {
            identity_with(c, "2036-11-01T00:00:00Z", "2020-01-01T00:00:00Z")
        }),
        // The key's fingerprint is right: only the common name tells.
        ("identity.json", &|c| {
            identity_with(c, &c.pki.agent_id, "01934abc-def0-7000-89ab-0000000000aa")
        }),
        ("cert.pem", &|_| b"not a certificate".to_vec()),
        ("cert.pem", &|_| Vec::new()),
        ("cert.pem", &|c| {
            format!("{0}{0}", c.pki.client_cert_pem).into_bytes()
        }),
        ("cert.pem", &|c| {
            format!("{}{}", c.pki.client_cert_pem, other_key.client_cert_pem).into_bytes()
        }),
        ("cert.pem", &|_| {
            other_key.client_cert_pem.clone().into_bytes()
        }),
        ("cert.pem", &|_| {
            other_id.client_cert_pem.clone().into_bytes()
        }),
    ];
    for (name, bytes) in cases {
        let console = Console::new();
        console.write(name, &bytes(&console));
        let before = console.files();
        let request = console.request(Choice::Remove);
        let script = Script::new();
        let line = identity_refusal(console.start(Some(&request), &script));
        assert_quotes_none(
            &line,
            &[
                &console.pki.agent_id,
                &console.key_fingerprint(),
                "01934abc-def0-7000-89ab-0000000000aa",
                &"0".repeat(64),
                "soon",
                "2036-11-01",
                "2020-01-01",
            ],
        );
        assert_no_file_changed(&console, &before, &script, &line);
    }
}

/// Item 2.3: "everything after is checked and written from those bytes":
/// what a file holds after the read reaches neither the check nor the
/// staged tree. The handles of the read may stay open as far as item 2.6,
/// in which case the rewrite cannot happen, which proves as much.
#[test]
fn install_ac_013_what_is_staged_comes_from_the_bytes_read() {
    let console = Console::new();
    let before = console.files();
    let paths: Vec<_> = FILES.iter().map(|name| console.file(name)).collect();
    let script = Script::new().hook(Point::CreateStaging, move || {
        for path in &paths {
            let _ = std::fs::write(path, b"written after the read");
        }
    });
    let request = console.request(Choice::Keep);
    assert_eq!(
        console.start(Some(&request), &script),
        Ok(Prepared::Committed)
    );
    console.assert_committed();
    assert_staged_from(&console, &before);
}

/// Item 2.3: `expires_at` is in the future: `Environment.now` at the
/// instant itself is not before it.
#[test]
fn install_ac_013_an_identity_that_expires_now_is_code_5() {
    let mut console = Console::new();
    console.now = "2036-11-01T00:00:00Z".parse().expect("a time");
    let before = console.files();
    let request = console.request(Choice::Remove);
    let script = Script::new();
    let line = identity_refusal(console.start(Some(&request), &script));
    assert_no_file_changed(&console, &before, &script, &line);
}

// --- Item 2.4: restricting in place -----------------------------------------

/// Item 2.4: each file gets the shared DACL, in place, and stays so after a
/// refusal; its owner stays this process's where SYSTEM cannot be given.
/// The refusal names each file it restricted.
#[test]
fn install_ac_013_restriction_leaves_each_file_for_system_and_administrators() {
    let console = Console::new();
    let before = console.files();
    let request = console.request(Choice::Remove);
    let script = Script::new().fail(Point::CreateStaging);
    let line = identity_refusal(console.start(Some(&request), &script));
    assert_in_place_line(&line, &RESTRICTED);
    console.assert_console_in_place(&before, &FILES);
}

/// Item 2.4: a file the service cannot restrict refuses the migration, by
/// name: what it restricted so far stays restricted and is named, the rest
/// is as it was. The leftover temporary is one of them.
#[test]
fn install_ac_013_a_file_that_cannot_be_restricted_is_code_5() {
    for name in FILES.into_iter().chain([HYGIENE]) {
        let console = Console::new();
        console.write(HYGIENE, b"a consumed token");
        let before = console.files();
        let request = console.request(Choice::Remove);
        let script = Script::new().fail(Point::Restrict(name));
        let line = identity_refusal(console.start(Some(&request), &script));
        assert!(line.contains(name), "{line}");
        let restrictions = script.restrictions();
        assert_eq!(restrictions.last(), Some(&name), "it stops there");
        let earlier = &restrictions[..restrictions.len() - 1];
        assert_in_place_line(&line, &DIRECTORIES);
        assert_in_place_line(&line, earlier);
        console.assert_console_in_place(&before, earlier);
        assert!(!script.saw(Point::CreateStaging), "{line}");
    }
}

/// Item 2.4: the link count is read through the handle before the change
/// and again after it: a link made after the checks, at either moment,
/// refuses the migration, with item 2.2's repair.
#[test]
fn install_ac_013_a_link_made_around_a_restriction_is_code_5_with_the_repair() {
    for name in FILES {
        for point in [Point::Restrict(name), Point::Restricted(name)] {
            let console = Console::new();
            let before = console.files();
            let file = console.file(name);
            let link = console.outside("another-name");
            let script = Script::new().hook(point, move || {
                std::fs::hard_link(&file, &link).expect("link");
            });
            let request = console.request(Choice::Remove);
            let line = identity_refusal(console.start(Some(&request), &script));
            assert!(line.contains(name), "{line}");
            assert!(line.contains("other names"), "{line}");
            assert_in_place_line(&line, &DIRECTORIES);
            if point == Point::Restricted(name) {
                assert_in_place_line(&line, &[name]);
            }
            assert!(!script.saw(Point::CreateStaging), "{line}");
            assert_eq!(console.files()[name].bytes, before[name].bytes);
        }
    }
}

/// The Amendment 2026-11-07 (second): a file above the read's limit is
/// refused by name, unread.
#[test]
fn install_ac_013_a_file_above_the_read_limit_is_code_5() {
    let console = Console::new();
    let before = console.files();
    let file = console.file("identity.json");
    std::fs::OpenOptions::new()
        .write(true)
        .open(&file)
        .expect("open")
        .set_len(16 * 1024 * 1024 + 1)
        .expect("a sparse tail");
    let script = Script::new();
    let request = console.request(Choice::Remove);
    let line = identity_refusal(console.start(Some(&request), &script));
    assert!(line.contains("identity.json"), "{line}");
    assert!(line.contains("larger than"), "{line}");
    assert!(script.restrictions().is_empty(), "{line}");
    assert_eq!(
        console.files()["agent.toml"].bytes,
        before["agent.toml"].bytes
    );
}

/// The Amendment 2026-11-07 (second): an entry that appears after item 2.2,
/// as one a file transaction created before item 2.1 can, is refused
/// just before the first rename, with item 2.2's repair.
#[test]
fn install_ac_013_an_entry_that_appears_after_the_check_stops_the_first_rename() {
    for in_data_root in [true, false] {
        let console = Console::new();
        let directory = if in_data_root {
            console.paths.data_root.clone()
        } else {
            console.paths.product_root.clone()
        };
        let stray = directory.join("late-entry");
        let planted = stray.clone();
        let script = Script::new().hook(Point::FirstRename, move || {
            std::fs::write(&planted, b"late").expect("plant");
        });
        let request = console.request(Choice::Remove);
        let line = identity_refusal(console.start(Some(&request), &script));
        assert!(line.contains("late-entry"), "{line}");
        assert!(line.contains("removes it"), "{line}");
        assert_in_place_line(&line, &FILES);
        assert!(console.file("key.dat").is_file(), "the data root stays");
        assert!(!console.paths.legacy().exists(), "no rename ran");
        assert_eq!(std::fs::read(&stray).expect("left"), b"late");
    }
}

/// The same for a name the data root may hold: a leftover temporary that
/// appears after the check gets its own repair, and one file swapped for
/// another is told from the checked set.
#[test]
fn install_ac_013_a_late_temporary_gets_its_repair_and_a_swap_is_seen() {
    let console = Console::new();
    let file = console.file("agent.toml.hygiene-tmp");
    let script = Script::new().hook(Point::FirstRename, move || {
        std::fs::write(&file, b"late").expect("plant");
    });
    let request = console.request(Choice::Remove);
    let line = identity_refusal(console.start(Some(&request), &script));
    assert!(line.contains("agent.toml.hygiene-tmp"), "{line}");
    assert!(line.contains("deletes it"), "{line}");
    assert!(!console.paths.legacy().exists(), "no rename ran");

    let console = Console::new();
    console.write("agent.toml.hygiene-tmp", b"a consumed token");
    let (temporary, late) = (
        console.file("agent.toml.hygiene-tmp"),
        console.paths.data_root.join("late-entry"),
    );
    let script = Script::new().hook(Point::FirstRename, move || {
        std::fs::remove_file(&temporary).expect("remove");
        std::fs::write(&late, b"late").expect("plant");
    });
    let request = console.request(Choice::Remove);
    let line = identity_refusal(console.start(Some(&request), &script));
    assert!(line.contains("late-entry"), "{line}");
    assert!(!console.paths.legacy().exists(), "no rename ran");

    // A file the data root requires that has gone stops the start as one
    // missing at item 2.2 does, with item 6's route.
    let console = Console::new();
    let required = console.file("server-ca.pem");
    let script = Script::new().hook(Point::FirstRename, move || {
        std::fs::remove_file(&required).expect("remove");
    });
    let request = console.request(Choice::Remove);
    let line = identity_refusal(console.start(Some(&request), &script));
    assert!(line.contains("server-ca.pem is missing"), "{line}");
    assert!(line.contains("installs anew"), "{line}");
    assert!(!console.paths.legacy().exists(), "no rename ran");

    // A leftover temporary that has gone stops nothing.
    let console = Console::new();
    console.write("agent.toml.hygiene-tmp", b"a consumed token");
    let temporary = console.file("agent.toml.hygiene-tmp");
    let script = Script::new().hook(Point::FirstRename, move || {
        std::fs::remove_file(&temporary).expect("remove");
    });
    let request = console.request(Choice::Remove);
    assert_eq!(
        console.start(Some(&request), &script),
        Ok(Prepared::Committed)
    );
}

/// Item 2.6: anything held open under the data root fails the first rename
/// at once, whatever that handle shares; the line says so, in place of a
/// bare access-denied.
#[test]
fn install_ac_013_a_file_held_under_the_data_root_stops_the_first_rename() {
    use std::os::windows::fs::OpenOptionsExt as _;
    let console = Console::new();
    let held = std::sync::Arc::new(std::sync::Mutex::new(None));
    let slot = std::sync::Arc::clone(&held);
    let file = console.file("agent.toml");
    let script = Script::new().hook(Point::FirstRename, move || {
        let reader = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(7)
            .open(&file)
            .expect("a reader");
        *slot.lock().expect("slot") = Some(reader);
    });
    let request = console.request(Choice::Remove);
    let line = identity_refusal(console.start(Some(&request), &script));
    drop(held);
    assert!(line.contains("is in use by another process"), "{line}");
    assert_in_place_line(&line, &FILES);
    assert!(!console.paths.legacy().exists(), "no rename ran");
}

/// The amendment's preamble: an operation that Windows makes wait is
/// cancelled after 30 seconds; the service's environment says so.
#[test]
fn install_ac_013_the_service_waits_30_seconds() {
    assert_eq!(
        cg_agent::service::migration::OPERATION_WAIT,
        std::time::Duration::from_secs(30)
    );
}

/// Item 2.4: restriction goes through a handle whose file ID is the one
/// checked: a file replaced after its check is neither restricted nor
/// used. A start may answer in three ways, all of them that: a handle it
/// holds rules the swap out, and nothing happened to refuse; a handle it
/// holds lets the swap through and restricts the object checked, wherever
/// it went, and migrates from the bytes it read; or it opens the name
/// again, finds another file ID and refuses.
#[test]
fn install_ac_013_a_file_replaced_after_its_check_is_not_restricted() {
    for name in FILES {
        let console = Console::new();
        let before = console.files();
        let file = console.file(name);
        let aside = console.outside("aside");
        let moved_to = aside.clone();
        let swapped = Arc::new(Mutex::new(None));
        let told = Arc::clone(&swapped);
        let script = Script::new().hook(Point::Restrict(name), move || {
            let moved = std::fs::rename(&file, &moved_to).is_ok();
            if moved {
                std::fs::write(&file, b"a new file of that name").expect("write");
            }
            *told.lock().expect("told") = Some(moved);
        });
        let request = console.request(Choice::Remove);
        let result = console.start(Some(&request), &script);
        let moved = *swapped.lock().expect("swapped");
        let shared = with_owner(&console.custody.shared_file_sddl);
        match (moved, result) {
            (Some(false), result) => assert_eq!(result, Ok(Prepared::Committed)),
            (Some(true), Ok(Prepared::Committed)) => {
                console.assert_committed();
                assert_eq!(security(&aside), shared, "the object checked");
                assert_ne!(
                    security(&console.legacy(name)),
                    shared,
                    "the new file is not restricted"
                );
                assert_eq!(
                    std::fs::read(console.legacy(name)).expect("read"),
                    b"a new file of that name"
                );
                assert_staged_from(&console, &before);
            }
            (_, result) => {
                let line = identity_refusal(result);
                assert!(line.contains(name), "{line}");
                assert_ne!(
                    security(&console.file(name)),
                    shared,
                    "the new file is not restricted"
                );
                assert_eq!(
                    std::fs::read(console.file(name)).expect("read"),
                    b"a new file of that name"
                );
                assert!(!script.saw(Point::CreateStaging), "{line}");
            }
        }
    }
}

// --- Item 2.5: recording and staging ----------------------------------------

/// Items 2.5 and 3: the staging directory and `migration.json`, created
/// before any staged file, with their final security and the record's
/// fields, the enrolling account read before item 2.4 changed the DACLs.
#[test]
fn install_ac_013_the_record_is_written_before_the_staged_files() {
    let console = Console::new();
    console.write(HYGIENE, b"x");
    let request = console.request(Choice::Keep);
    console.crash_at(Point::Stage("agent.toml"), &request);
    assert_eq!(entries(&console.paths.staging()), ["migration.json"]);
    assert_eq!(
        security(&console.paths.staging()),
        with_owner(&console.custody.closed_directory_sddl)
    );
    assert_eq!(
        security(&console.staged("migration.json")),
        with_owner(&console.custody.shared_file_sddl)
    );
    custody::check(
        &console.staged("migration.json"),
        Kind::File,
        &console.custody.configuration,
    )
    .expect("agent.toml's policy");

    let record = console.staged_record();
    assert_eq!(record.choice, Choice::Keep);
    assert_eq!(record.fingerprints, [console.fingerprint()]);
    assert_eq!(record.agent_id, console.pki.agent_id);
    assert_eq!(record.key_fingerprint, console.key_fingerprint());
    assert_eq!(record.enrolling_account.as_deref(), Some(ENROLLING_SID));
    assert!(!record.reversal_pending);
    assert_eq!(record.copies_not_reached, Vec::<String>::new());
    let mut checked: Vec<&str> = FILES.to_vec();
    checked.push(HYGIENE);
    assert_eq!(
        record
            .files
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        checked.into_iter().collect::<BTreeSet<_>>(),
        "each checked file's ID"
    );
    assert!(record.files.values().all(|id| !id.is_empty()));
}

/// Item 2.5: the generated `agent.toml`: the fixed paths, the legacy
/// `agent.id`, the hostname Windows reports, no token, each of the legacy's
/// other settings; it passes the service's own check.
fn assert_generated_configuration(console: &Console) {
    let config = console.generated_config();

    cg_agent::service::check_installed_config(&console.paths, &console.paths.config, &config)
        .expect("the fixed paths and both URLs");
    assert_eq!(config.agent.id, LEGACY_AGENT_ID, "the legacy agent.id");
    assert_ne!(config.agent.hostname, LEGACY_HOSTNAME);
    let output = std::process::Command::new("hostname")
        .output()
        .expect("run hostname");
    let hostname = String::from_utf8_lossy(&output.stdout);
    assert!(
        config.agent.hostname.eq_ignore_ascii_case(hostname.trim()),
        "{} is not {hostname}",
        config.agent.hostname
    );
    assert_eq!(config.server.url, ENROLL_URL);
    assert_eq!(config.server.heartbeat_url.as_deref(), Some(HEARTBEAT_URL));
    assert_eq!(config.heartbeat.interval_seconds, 17);
    assert_eq!(config.heartbeat.request_timeout_seconds, 11);
    assert_eq!(config.heartbeat.max_retries, 7);
    assert_eq!(config.heartbeat.backoff_initial_ms, 130);
    assert_eq!(config.heartbeat.backoff_factor, 1.5);
    assert_eq!(config.heartbeat.backoff_max_ms, 1700);
    assert_eq!(config.log.level, "debug");
    let enrollment = config.enrollment.as_ref().expect("[enrollment]");
    assert_eq!(enrollment.timeout_seconds, 23);
    assert_eq!(enrollment.max_retries, 9);
    assert_eq!(enrollment.backoff_initial_ms, 270);
    assert_eq!(enrollment.backoff_factor, 2.5);
    assert_eq!(enrollment.token, None, "no enrollment.token");
    let tls = config.tls.as_ref().expect("[tls]");
    assert_eq!(tls.minimum_version, "1.3");
    let envelope = config.envelope.as_ref().expect("[envelope]");
    assert_eq!(envelope.canonical_form, "JCS");
    let text = std::fs::read_to_string(&console.paths.config).expect("read");
    assert!(!text.contains(TOKEN), "{text}");
    assert!(!text.contains("[setup]"), "{text}");
    assert_eq!(
        security(&console.paths.config),
        with_owner(&console.custody.shared_file_sddl)
    );
    assert_eq!(
        security(&console.paths.trust_anchor),
        with_owner(&console.custody.shared_file_sddl)
    );
}

/// Item 2.5: the configuration generated from one that still holds its
/// token, as a console whose hygiene failed leaves it.
#[test]
fn install_ac_013_the_generated_configuration() {
    let console = Console::new();
    console.migrate(Choice::Remove);
    assert_generated_configuration(&console);
}

/// Items 2.3 and 2.5 on the usual host: FR-014 took the token out of
/// `agent.toml` after enrollment, with the console's own rewrite. The route
/// neither needs the token nor reads it, and what it generates is the same.
#[test]
fn install_ac_013_a_configuration_without_its_token_migrates_alike() {
    let console = Console::new();
    cg_agent::identity::hygienize_token(&console.file("agent.toml"));
    let text = std::fs::read_to_string(console.file("agent.toml")).expect("read");
    assert!(!text.contains(TOKEN), "the hygiene ran");
    let enrollment = cg_agent::config::load_from_str(&text)
        .expect("the hygienized configuration")
        .enrollment
        .expect("[enrollment]");
    assert_eq!(enrollment.token, None);

    console.migrate(Choice::Remove);
    console.assert_committed();
    assert_generated_configuration(&console);
}

/// Item 2.5: `identity\` and its files with their final security; `key.dat`
/// sealed bytes unchanged, the others written from the bytes read; the
/// migrated identity loads with the legacy `agent_id` and key.
#[test]
fn install_ac_013_the_identity_moves_with_its_bytes_and_final_security() {
    let console = Console::new();
    let before = console.files();
    console.migrate(Choice::Keep);
    for (name, path) in [
        ("cert.pem", &console.paths.cert),
        ("key.dat", &console.paths.key),
        ("identity.json", &console.paths.identity),
    ] {
        assert_eq!(
            std::fs::read(path).ok().as_ref(),
            before[name].bytes.as_ref(),
            "{name}"
        );
        assert_eq!(
            security(path),
            with_owner(&console.custody.identity_file_sddl),
            "{name}"
        );
    }
    assert_eq!(
        security(&console.paths.identity_dir),
        with_owner(&console.custody.identity_directory_sddl)
    );
    let identity = console.load_identity();
    assert_eq!(identity.agent_id, console.pki.agent_id);
    assert_eq!(
        identity.keypair.public_key_bytes(),
        console.pki.agent_pubkey
    );
    assert_eq!(identity.client_certificate_pem, console.pki.client_cert_pem);
}

/// Item 2.5: "each object new, with its final security": each staged object
/// already has it when the next one's point is reached, so none is
/// restricted after it is written.
#[test]
fn install_ac_013_each_staged_object_has_its_final_security_when_created() {
    let console = Console::new();
    let custody = &console.custody;
    let shared = with_owner(&custody.shared_file_sddl);
    let identity_file = with_owner(&custody.identity_file_sddl);
    let mut script = Script::new();
    for (point, staged, expected) in [
        (Point::Stage("server-ca.pem"), "agent.toml", shared.clone()),
        (Point::Stage("identity"), "server-ca.pem", shared.clone()),
        (
            Point::Stage("cert.pem"),
            "identity",
            with_owner(&custody.identity_directory_sddl),
        ),
        (
            Point::Stage("key.dat"),
            "identity/cert.pem",
            identity_file.clone(),
        ),
        (
            Point::Stage("identity.json"),
            "identity/key.dat",
            identity_file.clone(),
        ),
        (
            Point::CheckStaged,
            "identity/identity.json",
            identity_file.clone(),
        ),
    ] {
        let path = console.staged(staged);
        script = script.hook(point, move || {
            assert_eq!(security(&path), expected, "{}", path.display());
        });
    }
    let request = console.request(Choice::Remove);
    assert_eq!(
        console.start(Some(&request), &script),
        Ok(Prepared::Committed)
    );
    assert!(script.saw(Point::CheckStaged));
}

/// Item 2.5: "It creates the staging directory, new" and "each object new":
/// on a first attempt, what stands at one of those names is neither adopted
/// nor written over. The start stops before the commit and it stays. (Item
/// 2.5 gives such a failure no code; code 5 is a decision the tests take
/// with the implementation: item 2.2's default, and neither a configuration,
/// a trust anchor nor the file log.)
#[test]
fn install_ac_013_an_object_already_at_a_staged_name_stops_the_route() {
    let console = Console::new();
    let before = console.files();
    let staging = console.paths.staging();
    let planted = staging.join("planted.txt");
    let kept = planted.clone();
    let script = Script::new().hook(Point::CreateStaging, move || {
        std::fs::create_dir(&staging).expect("a directory of that name");
        std::fs::write(&planted, b"not the service's").expect("a file in it");
    });
    let request = console.request(Choice::Remove);
    let line = identity_refusal(console.start(Some(&request), &script));
    assert_in_place_line(&line, &RESTRICTED);
    assert!(!script.saw(Point::FirstRename), "{line}");
    assert_eq!(std::fs::read(&kept).expect("left"), b"not the service's");
    console.assert_console_in_place(&before, &FILES);

    for (point, name) in [
        (Point::Stage("agent.toml"), "agent.toml"),
        (Point::Stage("server-ca.pem"), "server-ca.pem"),
        (Point::Stage("identity"), "identity"),
        (Point::Stage("cert.pem"), "identity/cert.pem"),
        (Point::Stage("key.dat"), "identity/key.dat"),
        (Point::Stage("identity.json"), "identity/identity.json"),
    ] {
        let console = Console::new();
        let before = console.files();
        let path = console.staged(name);
        let planted = path.clone();
        let script = Script::new().hook(point, move || {
            if planted.ends_with("identity") {
                std::fs::create_dir(&planted).expect("a directory of that name");
            } else {
                std::fs::write(&planted, b"not the service's").expect("a file of that name");
            }
        });
        let request = console.request(Choice::Remove);
        let line = identity_refusal(console.start(Some(&request), &script));
        assert_in_place_line(&line, &RESTRICTED);
        assert!(!script.saw(Point::FirstRename), "{name}: {line}");
        if name != "identity" {
            assert_eq!(
                std::fs::read(&path).expect("left"),
                b"not the service's",
                "{name}"
            );
        }
        console.assert_console_in_place(&before, &FILES);
    }
}

/// Item 2.5, a failure at each step (code 5, as item 4 stops on a staged
/// tree that does not pass; the item gives none, and that is a decision
/// taken with the implementation): the console installation is in place
/// with its files restricted, and asking again migrates it.
#[test]
fn install_ac_013_a_failure_while_staging_leaves_the_console_in_place() {
    for point in [
        Point::CreateStaging,
        Point::WriteRecord,
        Point::Stage("agent.toml"),
        Point::Stage("server-ca.pem"),
        Point::Stage("identity"),
        Point::Stage("cert.pem"),
        Point::Stage("key.dat"),
        Point::Stage("identity.json"),
        Point::CheckStaged,
    ] {
        let console = Console::new();
        let before = console.files();
        let request = console.request(Choice::Remove);
        let script = Script::new().fail(point);
        let line = identity_refusal(console.start(Some(&request), &script));
        assert_in_place_line(&line, &RESTRICTED);
        assert!(!script.saw(Point::FirstRename), "{point:?}");
        console.assert_console_in_place(&before, &FILES);

        assert_eq!(
            console.start(Some(&request), &Script::new()),
            Ok(Prepared::Committed),
            "{point:?}: asked again"
        );
        console.assert_committed();
    }
}

/// Item 2.5: "it checks the staged tree as a start checks the data root,
/// and that the staged identity loads with the legacy `agent_id` and key
/// fingerprint": each is seen at the check, before any rename. A whole
/// identity of another key or agent is consistent in itself, so only the
/// comparison with the legacy one refuses it.
#[test]
fn install_ac_013_the_staged_tree_is_checked_before_the_commit() {
    type Spoil = fn(&Console) -> Box<dyn FnMut() + Send>;
    fn written(writes: Vec<(PathBuf, Vec<u8>)>) -> Box<dyn FnMut() + Send> {
        Box::new(move || {
            for (path, bytes) in &writes {
                std::fs::write(path, bytes).expect("write");
            }
        })
    }
    let spoils: [Spoil; 8] = [
        // agent.toml readable by others.
        |c| {
            let path = c.staged("agent.toml");
            let dacl = format!("D:P(A;;FA;;;{})(A;;FR;;;BU)", sid());
            Box::new(move || set_dacl(&path, &dacl))
        },
        // server-ca.pem with another link.
        |c| {
            let path = c.staged("server-ca.pem");
            let link = c.outside("another-name");
            Box::new(move || std::fs::hard_link(&path, &link).expect("link"))
        },
        // identity\ allowing Administrators.
        |c| {
            let path = c.staged("identity");
            let dacl = format!("D:P(A;OICI;FA;;;{})(A;OICI;FA;;;BA)", sid());
            Box::new(move || set_dacl(&path, &dacl))
        },
        // A key that does not unseal.
        |c| {
            let path = c.staged("identity/key.dat");
            Box::new(move || std::fs::write(&path, b"not sealed").expect("write"))
        },
        // No certificate.
        |c| {
            let path = c.staged("identity/cert.pem");
            Box::new(move || std::fs::remove_file(&path).expect("remove"))
        },
        // Another agent_id.
        |c| {
            let path = c.staged("identity/identity.json");
            let id = c.pki.agent_id.clone();
            Box::new(move || {
                let text = std::fs::read_to_string(&path).expect("read");
                assert!(text.contains(&id));
                let other = text.replace(&id, "01934abc-def0-7000-89ab-0000000000bb");
                std::fs::write(&path, other).expect("write");
            })
        },
        // The whole identity of another key, for the same agent.
        |c| written(c.staged_identity_of(&Console::new())),
        // The whole identity of another agent.
        |c| {
            let other = Console::for_agent("01934abc-def0-7000-89ab-0000000000bb");
            written(c.staged_identity_of(&other))
        },
    ];
    for spoil in spoils {
        let console = Console::new();
        let before = console.files();
        let script = Script::new().hook(Point::CheckStaged, spoil(&console));
        let request = console.request(Choice::Remove);
        let line = identity_refusal(console.start(Some(&request), &script));
        assert_in_place_line(&line, &RESTRICTED);
        assert!(!script.saw(Point::FirstRename), "{line}");
        console.assert_console_in_place(&before, &FILES);
    }
}

// --- Item 2.6: the commit ---------------------------------------------------

/// Item 2.6: a failed first rename stops the start (code 5), the console
/// installation in place; the migration can be asked for again.
#[test]
fn install_ac_013_a_failed_first_rename_is_code_5() {
    let console = Console::new();
    let before = console.files();
    let request = console.request(Choice::Remove);
    let script = Script::new().fail(Point::FirstRename);
    let line = identity_refusal(console.start(Some(&request), &script));
    assert_in_place_line(&line, &RESTRICTED);
    assert!(!script.saw(Point::SecondRename), "{line}");
    console.assert_console_in_place(&before, &FILES);

    assert_eq!(
        console.start(Some(&request), &Script::new()),
        Ok(Prepared::Committed)
    );
    console.assert_committed();
}

/// Item 2.6: a failed second rename is reversed: recorded, renamed back,
/// the mark cleared, code 5, the console in place. Each rewrite of the
/// record keeps its owner and DACL and leaves no temporary (item 3), and
/// what it held: the migration asked for again still has the enrolling
/// account.
#[test]
fn install_ac_013_a_failed_second_rename_is_reversed() {
    let console = Console::new();
    let before = console.files();
    let request = console.request(Choice::Remove);
    let record = console.staged("migration.json");
    let shared = with_owner(&console.custody.shared_file_sddl);
    let at_rename_back = record.clone();
    let expected = shared.clone();
    let staging = console.paths.staging();
    let legacy = console.paths.legacy();
    let data_root = console.paths.data_root.clone();
    let script = Script::new()
        .fail(Point::SecondRename)
        .hook(Point::RenameBack, move || {
            assert!(
                read_record(&at_rename_back).reversal_pending,
                "recorded before it reverses"
            );
            assert_eq!(security(&at_rename_back), expected, "its owner and DACL");
            assert_eq!(entries(&staging), STAGED, "no temporary");
            assert!(
                legacy.is_dir() && !data_root.exists(),
                "the first rename ran"
            );
        });
    let line = identity_refusal(console.start(Some(&request), &script));
    assert_in_place_line(&line, &RESTRICTED);
    let seen = script.seen();
    assert_eq!(
        seen[seen.len() - 4..],
        [
            Point::SecondRename,
            Point::RecordReversal,
            Point::RenameBack,
            Point::ClearReversal,
        ]
    );
    console.assert_console_in_place(&before, &FILES);
    assert!(
        !read_record(&record).reversal_pending,
        "the mark is cleared"
    );
    assert_eq!(security(&record), shared, "its owner and DACL");
    assert_eq!(entries(&console.paths.staging()), STAGED, "no temporary");
    assert_eq!(
        console.start(Some(&request), &Script::new()),
        Ok(Prepared::Committed)
    );
    assert_eq!(
        console.record().enrolling_account.as_deref(),
        Some(ENROLLING_SID),
        "the two rewrites kept it"
    );
}

/// Item 2.6 and the paragraph after it: when the rename back fails, the
/// line says where the data root is and that the next start, with or
/// without arguments, puts it back; the record keeps the mark.
#[test]
fn install_ac_013_a_failed_rename_back_says_where_the_data_root_is() {
    let console = Console::new();
    let request = console.request(Choice::Remove);
    let script = Script::new()
        .fail(Point::SecondRename)
        .fail(Point::RenameBack);
    let (code, line) = refused(console.start(Some(&request), &script));
    assert_eq!(code, 5, "{line}");
    assert!(
        line.starts_with(migration_fixtures::IDENTITY_LINE),
        "{line}"
    );
    assert!(line.contains("agent.legacy"), "{line}");
    assert!(line.contains("next start"), "{line}");
    assert!(line.contains("arguments"), "with or without them: {line}");
    assert!(!line.contains(IN_PLACE), "{line}");
    assert!(!console.paths.data_root.exists(), "not put back");
    assert!(console.paths.legacy().is_dir());
    assert!(console.staged_record().reversal_pending, "the mark stays");
    assert!(!script.saw(Point::ClearReversal), "{line}");
}

/// Item 2.6: the commit: the data root is the staged tree, under the closed
/// DACL, the console's files are in the legacy directory, with their bytes.
#[test]
fn install_ac_013_the_commit_moves_the_console_aside_with_its_bytes() {
    let console = Console::new();
    let before = console.files();
    console.migrate(Choice::Keep);
    console.assert_committed();
    assert_eq!(
        security(&console.paths.data_root),
        with_owner(&console.custody.closed_directory_sddl)
    );
    assert_eq!(
        entries(&console.paths.product_root),
        ["agent", "agent.legacy"]
    );
    for name in FILES {
        assert_eq!(
            std::fs::read(console.legacy(name)).ok().as_ref(),
            before[name].bytes.as_ref(),
            "{name}"
        );
    }
    let record = console.record();
    assert_eq!(record.choice, Choice::Keep);
    assert!(!record.reversal_pending);
}

// --- Item 6: the default refusal --------------------------------------------

/// Item 6: without arguments, a console layout whose data directories pass
/// is left to the Amendment 2026-11-04's checks: nothing migrates, nothing
/// changes, and no lock is taken here.
#[test]
fn install_ac_013_a_start_without_arguments_never_migrates() {
    let console = Console::closed();
    let before = console.whole();
    let script = Script::new();
    let mut locks = 0;
    let result = console.start_with_lock(None, &script, &mut || {
        locks += 1;
        Ok(())
    });
    assert_eq!(result, Ok(Prepared::ConsoleLayout));
    assert_eq!(locks, 0, "nothing is in progress");
    assert_eq!(script.seen(), [], "no step ran");
    assert_same_tree(&before, &console.whole(), "nothing changes");
}

/// Item 6: R17's outcome, code 13 at the data directories, adds the default
/// refusal's reason, whichever of the identity files the data root holds.
#[test]
fn install_ac_013_the_default_refusal_is_code_13_on_an_open_data_directory() {
    let console = Console::new();
    let before = console.whole();
    let script = Script::new();
    let line = log_refusal(console.start(None, &script));
    assert!(line.ends_with(DEFAULT_REFUSAL), "{line}");
    assert_eq!(script.seen(), [], "no step ran");
    assert_same_tree(&before, &console.whole(), "nothing changes");

    for kept in IDENTITY_FILES {
        let console = Console::new();
        for name in IDENTITY_FILES.into_iter().filter(|name| *name != kept) {
            std::fs::remove_file(console.file(name)).expect("remove");
        }
        let line = log_refusal(console.start(None, &Script::new()));
        assert!(line.ends_with(DEFAULT_REFUSAL), "{kept} alone: {line}");
    }
}

/// Items 4 and 6: no console layout, no default refusal: an open data
/// directory keeps the Amendment 2026-11-04's code 13, and a data root
/// whose identity is in `identity\` starts.
#[test]
fn install_ac_013_a_host_without_a_console_layout_keeps_its_outcome() {
    let open = Console::service_host(&programdata_child_sddl());
    let line = log_refusal(open.start(None, &Script::new()));
    assert!(line.starts_with(LOG_LINE), "{line}");
    assert!(!line.contains(DEFAULT_REFUSAL), "{line}");

    let closed = Console::service_host(&test_custody().closed_directory_sddl);
    let before = closed.whole();
    assert_eq!(closed.start(None, &Script::new()), Ok(Prepared::Plain));
    assert_same_tree(&before, &closed.whole(), "nothing changes");

    assert_eq!(
        Console::bare().start(None, &Script::new()),
        Ok(Prepared::Plain),
        "none of the three directories"
    );
}

/// Item 2.2's refusal is repaired and the migration started again: item
/// 2.1's closure is accepted on the way through.
#[test]
fn install_ac_013_a_repaired_host_migrates_when_asked_again() {
    let console = Console::new();
    console.write("stray.txt", b"x");
    let request = console.request(Choice::Remove);
    identity_refusal(console.start(Some(&request), &Script::new()));
    std::fs::remove_file(console.file("stray.txt")).expect("the repair");
    assert_eq!(
        console.start(Some(&request), &Script::new()),
        Ok(Prepared::Committed)
    );
    console.assert_committed();
}
