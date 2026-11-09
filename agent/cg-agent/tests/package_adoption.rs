//! The service's adoption of the staged configuration (SPEC-024 Amendment
//! 2026-11-09, item 6): a precondition of the package's criteria
//! (install_ac_001, install_ac_011), not those criteria. Unelevated over
//! temporary roots, with the current user in SYSTEM's place
//! (`migration_fixtures`): the start arguments, each refusal and its code with
//! the host unchanged, a failure and a crash injected at each step, and what
//! a retry finds. The service and SYSTEM tests are `package_adoption_service`.
//!
//! An interruption is `Injected::Crash`; the next start is `prepare` without
//! arguments, which is what a restart by the SCM runs.
#![cfg(windows)]

mod common;
mod custody_fixtures;
mod migration_fixtures;

use std::collections::BTreeSet;
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use cg_agent::custody::{self, Kind, Principal};
use cg_agent::service::adoption::adopt;
use cg_agent::service::migration::{parse_start, Choice, Point, Prepared, Request, Start, Stop};

use migration_fixtures::{
    assert_clean, assert_quotes_none, assert_same_tree, batch_oplock, entries, junction, security,
    set_dacl, sid, with_owner, Console, Script, ARGUMENTS_LINE, ENROLL_URL, HEARTBEAT_URL,
    IDENTITY_LINE, IN_PLACE, LOG_LINE, SHARE_ALL, TOKEN,
};

const ADOPT: &str = "--adopt-staged-config";
const CONFIG: &str = "agent.toml";
const ANCHOR: &str = "server-ca.pem";
const STAGED_CONFIG: &str = "agent.toml.setup";
const STAGED_ANCHOR: &str = "server-ca.pem.setup";
/// The line of a staged anchor refused (code 6).
const ANCHOR_LINE: &str = "cg-agent: tls: client configuration failed: adoption: ";
/// What the setup leaves in the data root, and the service leaves alone.
const ATTEMPT_RECORD: &str = "setup-attempt.json";

/// What a pending host's earlier files hold.
const EARLIER_CONFIG: &[u8] = b"earlier agent.toml, with its own token";
const EARLIER_ANCHOR: &[u8] = b"earlier server-ca.pem";

/// Each point of a whole adoption, in order.
fn steps() -> Vec<Point> {
    vec![
        Point::AdoptRead(STAGED_CONFIG),
        Point::AdoptRead(STAGED_ANCHOR),
        Point::CloseProductRoot,
        Point::CloseDataRoot,
        Point::AdoptRestrict(STAGED_CONFIG),
        Point::AdoptRestricted(STAGED_CONFIG),
        Point::AdoptRestrict(STAGED_ANCHOR),
        Point::AdoptRestricted(STAGED_ANCHOR),
        Point::AdoptNames,
        Point::AdoptDelete(CONFIG),
        Point::AdoptDelete(ANCHOR),
        Point::AdoptRename(ANCHOR),
        Point::AdoptRename(CONFIG),
    ]
}

/// A DACL that passes the data directories check and is not the closed one:
/// closing them changes it, so a test can tell that it ran.
fn before_closing() -> String {
    format!("D:P(A;OICI;FA;;;{})", sid())
}

/// A host with its directories, which adoption will close.
fn host() -> Console {
    let console = Console::bare();
    let sddl = before_closing();
    console.make_directories(&sddl, &sddl);
    console
}

/// The staged configuration's text.
fn staged_toml(console: &Console) -> String {
    let paths = &console.paths;
    format!(
        "[server]\n\
         url = \"{ENROLL_URL}\"\n\
         heartbeat_url = \"{HEARTBEAT_URL}\"\n\
         trust_anchor_path = '{}'\n\
         \n\
         [agent]\n\
         id = \"01934abc-def0-7000-89ab-000000000007\"\n\
         hostname = \"TEST-PC\"\n\
         \n\
         [enrollment]\n\
         token = \"{TOKEN}\"\n\
         cert_path = '{}'\n\
         key_path = '{}'\n\
         identity_path = '{}'\n",
        paths.trust_anchor.display(),
        paths.cert.display(),
        paths.key.display(),
        paths.identity.display(),
    )
}

/// A security that passes the staged files' check and is not what step 5 sets.
fn staged_security() -> String {
    format!("D:P(A;;FA;;;{})", sid())
}

fn write_staged(console: &Console, name: &str, bytes: &[u8]) -> PathBuf {
    let path = console.paths.data_root.join(name);
    custody::create_new_file(&path, &staged_security(), bytes).expect("a staged file");
    path
}

fn stage_config(console: &Console, text: &str) -> PathBuf {
    write_staged(console, STAGED_CONFIG, text.as_bytes())
}

fn stage_anchor(console: &Console) -> PathBuf {
    write_staged(
        console,
        STAGED_ANCHOR,
        console.pki.trust_anchor_pem.as_bytes(),
    )
}

fn stage(console: &Console) {
    stage_config(console, &staged_toml(console));
    stage_anchor(console);
}

/// A host that has staged files beside `agent.toml` and `server-ca.pem`.
fn pending() -> Console {
    let console = host();
    for (path, bytes) in [
        (&console.paths.config, EARLIER_CONFIG),
        (&console.paths.trust_anchor, EARLIER_ANCHOR),
    ] {
        std::fs::write(path, bytes).expect("an earlier file");
    }
    stage(&console);
    console
}

fn fresh_and_staged() -> Console {
    let console = host();
    stage(&console);
    console
}

fn fingerprints(console: &Console) -> BTreeSet<String> {
    BTreeSet::from([console.fingerprint()])
}

fn adopt_with(
    console: &Console,
    script: &Script,
    lock: &mut dyn FnMut() -> Result<(), Stop>,
) -> Result<(), Stop> {
    adopt(&console.environment(script), &fingerprints(console), lock)
}

fn run(console: &Console, script: &Script) -> Result<(), Stop> {
    adopt_with(console, script, &mut || Ok(()))
}

/// The code and line of a refusal, which never carries a token, a SID or a PEM.
fn refusal(result: Result<(), Stop>) -> (u8, String) {
    match result {
        Err(Stop::Refused { code, line }) => {
            assert_clean(&line);
            (code, line)
        }
        other => panic!("a refusal was expected: {other:?}"),
    }
}

/// Refused with `code` and `prefix`, the whole host as it was.
fn assert_refused_unchanged(console: &Console, code: u8, prefix: &str) -> String {
    let before = console.whole();
    let (actual, line) = refusal(run(console, &Script::new()));
    assert_eq!(actual, code, "{line}");
    assert!(line.starts_with(prefix), "{line}");
    assert_same_tree(&before, &console.whole(), "the refusal");
    line
}

fn staged_config_line(console: &Console) -> String {
    format!(
        "cg-agent: invalid config {}: ",
        console.paths.data_root.join(STAGED_CONFIG).display()
    )
}

fn staged(console: &Console, name: &str) -> PathBuf {
    console.paths.data_root.join(name)
}

fn bytes_of(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

// --- The start arguments -----------------------------------------------------

fn start_of(arguments: &[&str]) -> Result<Start, String> {
    let owned: Vec<String> = arguments.iter().map(|text| text.to_string()).collect();
    parse_start(&owned)
}

fn fingerprint_of(digit: char) -> String {
    format!("sha256:{}", digit.to_string().repeat(64))
}

#[test]
fn package_adoption_no_arguments_is_a_plain_start() {
    assert_eq!(start_of(&[]), Ok(Start::Plain));
}

#[test]
fn package_adoption_a_migrations_arguments_stay_a_migrations() {
    let first = fingerprint_of('a');
    let trust = format!("--trust-anchor-sha256={first}");
    assert_eq!(
        start_of(&["--migrate-legacy=keep", &trust]),
        Ok(Start::Migrate(Request {
            choice: Choice::Keep,
            fingerprints: BTreeSet::from([first]),
        }))
    );
}

#[test]
fn package_adoption_takes_the_flag_once_and_each_fingerprint() {
    let (first, second) = (fingerprint_of('a'), fingerprint_of('b'));
    let (one, two) = (
        format!("--trust-anchor-sha256={first}"),
        format!("--trust-anchor-sha256={second}"),
    );
    assert_eq!(
        start_of(&[ADOPT, &one]),
        Ok(Start::Adopt(BTreeSet::from([first.clone()])))
    );
    assert_eq!(
        start_of(&[&two, ADOPT, &one]),
        Ok(Start::Adopt(BTreeSet::from([first.clone(), second]))),
        "in any order"
    );
    assert_eq!(
        start_of(&[ADOPT, &one, &one]),
        Ok(Start::Adopt(BTreeSet::from([first]))),
        "the same fingerprint twice is one, as the migration's"
    );
}

#[test]
fn package_adoption_refuses_arguments_of_any_other_form() {
    let good = format!("--trust-anchor-sha256={}", fingerprint_of('a'));
    let good = good.as_str();
    let upper = format!("--trust-anchor-sha256=sha256:{}", "A".repeat(64));
    let short = format!("--trust-anchor-sha256=sha256:{}", "a".repeat(63));
    let bare = format!("--trust-anchor-sha256={}", "a".repeat(64));
    let cases: Vec<Vec<&str>> = vec![
        vec![ADOPT],
        vec![ADOPT, ADOPT, good],
        vec![ADOPT, good, "--migrate-legacy=keep"],
        vec!["--migrate-legacy=remove", ADOPT, good],
        vec![ADOPT, good, "s3cret-value"],
        vec![ADOPT, upper.as_str()],
        vec![ADOPT, short.as_str()],
        vec![ADOPT, bare.as_str()],
        vec!["--adopt-staged-config=1", good],
    ];
    for case in cases {
        let line = start_of(&case).expect_err("refused");
        assert!(line.starts_with(ARGUMENTS_LINE), "{case:?}: {line}");
        assert_quotes_none(&line, &["s3cret-value", "AAAA", "aaaa"]);
    }
}

// --- Steps 1 to 3: before the staged files are read ---------------------------

fn lock_counted(console: &Console) -> (u8, String, u32) {
    let mut calls = 0;
    let result = adopt_with(console, &Script::new(), &mut || {
        calls += 1;
        Ok(())
    });
    let (code, line) = refusal(result);
    (code, line, calls)
}

/// Step 1: before any change, and before the lock.
#[test]
fn package_adoption_a_console_layout_is_code_2_before_the_lock() {
    for name in ["cert.pem", "key.dat", "identity.json", "KEY.DAT"] {
        let console = fresh_and_staged();
        std::fs::write(console.paths.data_root.join(name), b"x").expect("a console file");
        let before = console.whole();
        let (code, line, calls) = lock_counted(&console);
        assert_eq!((code, calls), (2, 0), "{name}: {line}");
        assert!(line.starts_with(ARGUMENTS_LINE), "{line}");
        assert_same_tree(&before, &console.whole(), name);
    }
}

#[test]
fn package_adoption_a_migration_in_progress_is_code_2_before_the_lock() {
    let console = fresh_and_staged();
    std::fs::create_dir(console.paths.staging()).expect("a staging directory");
    let before = console.whole();
    let (code, line, calls) = lock_counted(&console);
    assert_eq!((code, calls), (2, 0), "{line}");
    assert!(line.starts_with(ARGUMENTS_LINE), "{line}");
    assert_same_tree(&before, &console.whole(), "staging");

    let console = Console::bare();
    let sddl = before_closing();
    console.make_directories(&sddl, &sddl);
    std::fs::remove_dir(&console.paths.data_root).expect("no data root");
    std::fs::create_dir(console.paths.legacy()).expect("a legacy directory");
    let (code, line, calls) = lock_counted(&console);
    assert_eq!((code, calls), (2, 0), "{line}");
    assert!(line.starts_with(ARGUMENTS_LINE), "{line}");
}

/// A data root that cannot be listed may be a console layout: nothing is
/// assumed of it, and nothing is taken.
#[test]
fn package_adoption_a_data_root_that_cannot_be_listed_is_code_13_before_the_lock() {
    let console = fresh_and_staged();
    set_dacl(&console.paths.data_root, "D:P");
    let (code, line, calls) = lock_counted(&console);
    set_dacl(&console.paths.data_root, &before_closing());
    assert_eq!((code, calls), (13, 0), "{line}");
    assert!(line.starts_with(LOG_LINE), "{line}");
}

/// The first that applies: a migration is read from `<P>`'s listing alone.
#[test]
fn package_adoption_a_migration_comes_before_a_data_root_that_cannot_be_listed() {
    let console = fresh_and_staged();
    std::fs::create_dir(console.paths.staging()).expect("a staging directory");
    set_dacl(&console.paths.data_root, "D:P");
    let (code, line, calls) = lock_counted(&console);
    set_dacl(&console.paths.data_root, &before_closing());
    assert_eq!((code, calls), (2, 0), "{line}");
    assert!(line.starts_with(ARGUMENTS_LINE), "{line}");
}

/// The Amendment 2026-11-07 (second), item 5: beside a console layout with a
/// staging directory that counts, a refusal says what is in place.
#[test]
fn package_adoption_a_refusal_beside_a_migration_says_the_console_is_in_place() {
    let console = Console::new();
    console.counting_directory(&console.paths.staging());
    let before = console.whole();
    let (code, line, calls) = lock_counted(&console);
    assert_eq!((code, calls), (2, 0), "{line}");
    assert!(line.starts_with(ARGUMENTS_LINE), "{line}");
    assert!(line.contains(IN_PLACE), "{line}");
    assert_same_tree(&before, &console.whole(), "the refusal");

    // Without a staging directory that counts, nothing to say in place.
    let console = fresh_and_staged();
    std::fs::write(console.paths.data_root.join("cert.pem"), b"x").expect("a console file");
    let (code, line, _) = lock_counted(&console);
    assert_eq!(code, 2, "{line}");
    assert!(!line.contains(IN_PLACE), "{line}");
}

/// Step 1 is read again under the lock: a console agent that wrote its identity
/// before releasing the lock is not overwritten.
#[test]
fn package_adoption_names_are_read_again_under_the_lock() {
    let console = pending();
    let planted = console.paths.data_root.join("cert.pem");
    let result = adopt_with(&console, &Script::new(), &mut || {
        std::fs::write(&planted, b"x").expect("a console file");
        Ok(())
    });
    let (code, line) = refusal(result);
    assert_eq!(code, 2, "{line}");
    assert!(line.starts_with(ARGUMENTS_LINE), "{line}");
    assert_eq!(bytes_of(&console.paths.config), EARLIER_CONFIG);
}

/// Step 2: code 10 while another instance runs, the host unchanged.
#[test]
fn package_adoption_takes_the_lock_before_anything_changes() {
    let console = pending();
    let before = console.whole();
    let result = adopt_with(&console, &Script::new(), &mut || {
        Err(Stop::Refused {
            code: 10,
            line: "cg-agent: another agent holds the instance lock".to_string(),
        })
    });
    let (code, line) = refusal(result);
    assert_eq!(code, 10, "{line}");
    assert_same_tree(&before, &console.whole(), "the lock's refusal");
}

/// Step 3: a name `identity` is code 5, whatever it is.
#[test]
fn package_adoption_a_name_identity_is_code_5() {
    type Plant = fn(&Console);
    let plants: [Plant; 3] = [
        |c| std::fs::create_dir(&c.paths.identity_dir).expect("a directory"),
        |c| std::fs::write(&c.paths.identity_dir, b"x").expect("a file"),
        |c| std::fs::create_dir(c.paths.data_root.join("IDENTITY")).expect("in another case"),
    ];
    for plant in plants {
        let console = pending();
        plant(&console);
        let before = console.whole();
        let (code, line, calls) = lock_counted(&console);
        assert_eq!((code, calls), (5, 1), "{line}");
        assert!(line.starts_with(IDENTITY_LINE), "{line}");
        assert_same_tree(&before, &console.whole(), "identity");
    }
}

/// Step 3's two refusals, in its order.
#[test]
fn package_adoption_a_name_identity_comes_before_no_staged_configuration() {
    let console = host();
    std::fs::create_dir(&console.paths.identity_dir).expect("a directory");
    assert_refused_unchanged(&console, 5, IDENTITY_LINE);
}

#[test]
fn package_adoption_no_staged_configuration_is_code_2() {
    let console = host();
    let prefix = staged_config_line(&console);
    assert_refused_unchanged(&console, 2, &prefix);

    let console = host();
    stage_anchor(&console);
    let prefix = staged_config_line(&console);
    assert_refused_unchanged(&console, 2, &prefix);

    // No data root, and no <P>: nothing staged either.
    let console = Console::bare();
    let prefix = staged_config_line(&console);
    assert_refused_unchanged(&console, 2, &prefix);

    let console = Console::bare();
    std::fs::create_dir(&console.paths.product_root).expect("<P>");
    std::fs::write(&console.paths.data_root, b"x").expect("a file at the data root");
    let prefix = staged_config_line(&console);
    assert_refused_unchanged(&console, 2, &prefix);
}

/// The Amendment 2026-11-05, item 4: on every start, a combination no
/// migration leaves is code 5 and nothing is changed; a legacy directory that
/// does not count (a name an ordinary user created) is as if absent.
#[test]
fn package_adoption_a_legacy_directory_that_counts_is_code_5_before_any_change() {
    let console = pending();
    console.counting_directory(&console.paths.legacy());
    assert_refused_unchanged(&console, 5, IDENTITY_LINE);

    let console = pending();
    std::fs::create_dir(console.paths.legacy()).expect("a name that does not count");
    assert_eq!(run(&console, &Script::new()), Ok(()));
    assert_adopted(&console, staged_toml(&console).as_bytes());
}

/// The Amendment 2026-11-05, item 2.1's condition, met before any read.
#[test]
fn package_adoption_a_reparse_point_is_code_13_and_not_followed() {
    let behind = fresh_and_staged();
    let console = Console::bare();
    junction(&console.paths.product_root, &behind.paths.product_root);
    let untouched = behind.whole();
    let line = assert_refused_unchanged(&console, 13, LOG_LINE);
    assert!(line.contains("reparse point"), "{line}");
    assert_same_tree(&untouched, &behind.whole(), "what lies behind <P>");

    let behind = fresh_and_staged();
    let console = Console::bare();
    std::fs::create_dir(&console.paths.product_root).expect("<P>");
    set_dacl(&console.paths.product_root, &before_closing());
    junction(&console.paths.data_root, &behind.paths.data_root);
    let untouched = behind.whole();
    let line = assert_refused_unchanged(&console, 13, LOG_LINE);
    assert!(line.contains("reparse point"), "{line}");
    assert_same_tree(
        &untouched,
        &behind.whole(),
        "what lies behind the data root",
    );
}

// --- Step 4: the staged files -------------------------------------------------

type Spoil = fn(&Console, &Path);

/// What makes a staged file fail "a regular file, one link, owner SYSTEM or
/// Administrators, a protected DACL with allow entries for SYSTEM and
/// Administrators only, at most 16 MiB".
fn spoils() -> [(&'static str, Spoil); 7] {
    [
        ("a directory", |_, path| {
            std::fs::remove_file(path).expect("remove");
            std::fs::create_dir(path).expect("a directory");
        }),
        ("a junction", |c, path| {
            let target = c.outside("target");
            std::fs::create_dir(&target).expect("a target");
            std::fs::remove_file(path).expect("remove");
            junction(path, &target);
        }),
        ("another link", |c, path| {
            std::fs::hard_link(path, c.outside("another-name")).expect("a link");
        }),
        ("readable by others", |_, path| {
            set_dacl(path, &format!("D:P(A;;FA;;;{})(A;;FR;;;BU)", sid()));
        }),
        ("a DACL that is not protected", |_, path| {
            set_dacl(path, &format!("D:(A;;FA;;;{})", sid()));
        }),
        ("no DACL", |_, path| set_dacl(path, "D:NO_ACCESS_CONTROL")),
        ("above 16 MiB", |_, path| {
            let file = std::fs::OpenOptions::new()
                .write(true)
                .open(path)
                .expect("open");
            file.set_len(16 * 1024 * 1024 + 1).expect("set the length");
        }),
    ]
}

#[test]
fn package_adoption_a_staged_configuration_failing_custody_is_code_2() {
    for (what, spoil) in spoils() {
        let console = fresh_and_staged();
        spoil(&console, &staged(&console, STAGED_CONFIG));
        let prefix = staged_config_line(&console);
        let line = assert_refused_unchanged(&console, 2, &prefix);
        assert!(line.contains(STAGED_CONFIG), "{what}: {line}");
    }
}

#[test]
fn package_adoption_a_staged_anchor_failing_custody_is_code_6() {
    for (what, spoil) in spoils() {
        let console = fresh_and_staged();
        spoil(&console, &staged(&console, STAGED_ANCHOR));
        let line = assert_refused_unchanged(&console, 6, ANCHOR_LINE);
        assert!(line.contains(STAGED_ANCHOR), "{what}: {line}");
    }
}

/// Step 4's owners: SYSTEM or Administrators, here this user and its default
/// owner; with neither allowed, the file this test owns is refused.
#[test]
fn package_adoption_a_staged_file_of_another_owner_is_refused() {
    let mut console = fresh_and_staged();
    console.custody.owners = vec![Principal::System];
    let prefix = staged_config_line(&console);
    assert_refused_unchanged(&console, 2, &prefix);
}

#[test]
fn package_adoption_a_staged_file_held_for_writing_is_refused() {
    let console = fresh_and_staged();
    let _held = std::fs::OpenOptions::new()
        .write(true)
        .share_mode(SHARE_ALL)
        .open(staged(&console, STAGED_CONFIG))
        .expect("a writer");
    let prefix = staged_config_line(&console);
    assert_refused_unchanged(&console, 2, &prefix);
    drop(_held);

    let console = fresh_and_staged();
    let _held = std::fs::OpenOptions::new()
        .write(true)
        .share_mode(SHARE_ALL)
        .open(staged(&console, STAGED_ANCHOR))
        .expect("a writer");
    assert_refused_unchanged(&console, 6, ANCHOR_LINE);
}

/// An open Windows makes wait is cancelled after `Environment.wait`.
/// A symbolic link to a file that would pass is refused, not followed. A
/// token without the privilege cannot make one: that skips here and fails in
/// CI, where a green run must mean it ran.
#[test]
fn package_adoption_a_staged_file_that_is_a_symbolic_link_is_refused() {
    use std::os::windows::fs::symlink_file;
    for (name, code, prefix) in [
        (STAGED_CONFIG, 2, None),
        (STAGED_ANCHOR, 6, Some(ANCHOR_LINE)),
    ] {
        let console = fresh_and_staged();
        let path = staged(&console, name);
        let target = console.outside("target.txt");
        custody::create_new_file(&target, &staged_security(), &bytes_of(&path)).expect("a target");
        std::fs::remove_file(&path).expect("remove");
        match symlink_file(&target, &path) {
            Ok(()) => {}
            Err(e) if !migration_fixtures::in_ci() => {
                eprintln!("skipping: a symbolic link cannot be made here: {e}");
                return;
            }
            Err(e) => panic!("a symbolic link: {e}"),
        }
        let config_prefix = staged_config_line(&console);
        let line = assert_refused_unchanged(&console, code, prefix.unwrap_or(&config_prefix));
        assert!(line.contains("reparse point"), "{line}");
    }
}

#[test]
fn package_adoption_a_staged_file_under_an_oplock_is_refused_in_time() {
    let console = fresh_and_staged();
    let _lock = batch_oplock(&staged(&console, STAGED_CONFIG)).expect("an oplock");
    // The tree is not compared: reading the file would wait on the oplock too.
    let (code, line) = refusal(run(&console, &Script::new()));
    assert_eq!(code, 2, "{line}");
    assert!(line.starts_with(&staged_config_line(&console)), "{line}");
    assert!(line.contains("in use"), "{line}");
}

fn with_replaced(console: &Console, from: &str, to: &str) -> String {
    let text = staged_toml(console);
    assert!(text.contains(from), "{from}");
    text.replace(from, to)
}

/// "fails ... the checks the service applies to `agent.toml`, has no token,
/// or has a `setup` table".
#[test]
fn package_adoption_a_staged_configuration_refused_for_what_it_says_is_code_2() {
    let elsewhere = r"C:\elsewhere\file";
    type Case = (&'static str, fn(&Console) -> String);
    let cases: [Case; 7] = [
        ("not TOML", |_| "not [ toml".to_string()),
        ("no token", |c| {
            with_replaced(c, &format!("token = \"{TOKEN}\"\n"), "")
        }),
        ("an empty token", |c| {
            with_replaced(c, &format!("token = \"{TOKEN}\"\n"), "token = \"\"\n")
        }),
        ("a setup table", |c| {
            format!("{}\n[setup]\ntrust_anchor_sha256 = \"x\"\n", staged_toml(c))
        }),
        ("no heartbeat URL", |c| {
            with_replaced(c, &format!("heartbeat_url = \"{HEARTBEAT_URL}\"\n"), "")
        }),
        ("a plain URL", |c| {
            with_replaced(
                c,
                "https://ingest.example.test:8443",
                "http://ingest.example.test:8443",
            )
        }),
        ("no [enrollment]", |c| {
            let text = staged_toml(c);
            text[..text.find("[enrollment]").expect("the table")].to_string()
        }),
    ];
    for (what, text) in cases {
        let console = host();
        stage_config(&console, &text(&console));
        stage_anchor(&console);
        let prefix = staged_config_line(&console);
        let line = assert_refused_unchanged(&console, 2, &prefix);
        assert_quotes_none(&line, &[TOKEN]);
        assert!(line.contains(STAGED_CONFIG), "{what}: {line}");
    }

    // A path other than the fixed one (Amendment 2026-11-04, item 2).
    let fields: [fn(&Console) -> PathBuf; 4] = [
        |c| c.paths.trust_anchor.clone(),
        |c| c.paths.cert.clone(),
        |c| c.paths.key.clone(),
        |c| c.paths.identity.clone(),
    ];
    for field in fields {
        let console = host();
        let text = with_replaced(&console, &field(&console).display().to_string(), elsewhere);
        stage_config(&console, &text);
        stage_anchor(&console);
        let prefix = staged_config_line(&console);
        assert_refused_unchanged(&console, 2, &prefix);
    }

    // Not UTF-8.
    let console = host();
    write_staged(&console, STAGED_CONFIG, &[0xff, 0xfe, 0x00]);
    stage_anchor(&console);
    let prefix = staged_config_line(&console);
    assert_refused_unchanged(&console, 2, &prefix);
}

#[test]
fn package_adoption_a_staged_anchor_not_matching_exactly_is_code_6() {
    let other = common::generate_test_pki("01934abc-def0-7000-89ab-000000000055");
    // Absent.
    let console = host();
    stage_config(&console, &staged_toml(&console));
    assert_refused_unchanged(&console, 6, ANCHOR_LINE);

    // Another certificate, unparsable bytes and nothing at all.
    let anchors: [&[u8]; 3] = [other.trust_anchor_pem.as_bytes(), b"not a PEM", b""];
    for anchor in anchors {
        let console = host();
        stage_config(&console, &staged_toml(&console));
        write_staged(&console, STAGED_ANCHOR, anchor);
        assert_refused_unchanged(&console, 6, ANCHOR_LINE);
    }

    // One more certificate than the fingerprints, and one fewer.
    let console = host();
    stage_config(&console, &staged_toml(&console));
    let both = format!("{}{}", console.pki.trust_anchor_pem, other.trust_anchor_pem);
    write_staged(&console, STAGED_ANCHOR, both.as_bytes());
    assert_refused_unchanged(&console, 6, ANCHOR_LINE);

    let console = fresh_and_staged();
    let before = console.whole();
    let mut two = fingerprints(&console);
    two.extend(migration_fixtures::fingerprints_of(
        other.trust_anchor_pem.as_bytes(),
    ));
    let script = Script::new();
    let (code, line) = refusal(adopt(&console.environment(&script), &two, &mut || Ok(())));
    assert_eq!(code, 6, "{line}");
    assert!(line.starts_with(ANCHOR_LINE), "{line}");
    assert_same_tree(&before, &console.whole(), "the anchor");
}

/// Step 4 holds each file open, excluding writers: from there on the bytes
/// checked are the bytes that stay, until the handles close before the renames.
#[test]
fn package_adoption_a_staged_file_cannot_be_changed_while_it_is_held() {
    let console = fresh_and_staged();
    let targets = [
        staged(&console, STAGED_CONFIG),
        staged(&console, STAGED_ANCHOR),
    ];
    let denied = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&denied);
    let probe = move || {
        for target in &targets {
            let write = std::fs::OpenOptions::new()
                .write(true)
                .share_mode(SHARE_ALL)
                .open(target);
            let delete = std::fs::remove_file(target);
            recorded
                .lock()
                .expect("recorded")
                .push((write.is_err(), delete.is_err()));
        }
    };
    let script = Script::new()
        .hook(Point::AdoptRestrict(STAGED_ANCHOR), probe.clone())
        .hook(Point::AdoptDelete(ANCHOR), probe);
    assert_eq!(run(&console, &script), Ok(()));
    assert_eq!(
        *denied.lock().expect("recorded"),
        vec![(true, true); 4],
        "writers and deleters are excluded until the handles close before the renames"
    );
}

// --- Steps 5 and 6: the commit -----------------------------------------------

fn assert_adopted(console: &Console, config: &[u8]) {
    assert_eq!(
        bytes_of(&console.paths.config),
        config,
        "agent.toml's bytes"
    );
    assert_eq!(
        bytes_of(&console.paths.trust_anchor),
        console.pki.trust_anchor_pem.as_bytes(),
        "server-ca.pem's bytes"
    );
    for name in [STAGED_CONFIG, STAGED_ANCHOR] {
        assert!(!staged(console, name).exists(), "{name} is gone");
    }
    for path in [&console.paths.config, &console.paths.trust_anchor] {
        assert_eq!(
            security(path),
            with_owner(&console.custody.shared_file_sddl),
            "{}: step 5's security",
            path.display()
        );
        custody::check(path, Kind::File, &console.custody.configuration)
            .expect("agent.toml's policy");
    }
    console.assert_directories_closed();
}

#[test]
fn package_adoption_a_fresh_host_adopts_in_the_contracts_order() {
    let console = fresh_and_staged();
    write_staged(&console, ATTEMPT_RECORD, b"{}");
    let script = Script::new();
    let mut calls = 0;
    assert_eq!(
        adopt_with(&console, &script, &mut || {
            calls += 1;
            Ok(())
        }),
        Ok(())
    );
    assert_eq!(calls, 1, "the lock is taken once");
    assert_eq!(script.seen(), steps());
    assert_adopted(&console, staged_toml(&console).as_bytes());
    assert_eq!(
        entries(&console.paths.data_root),
        [CONFIG, ANCHOR, ATTEMPT_RECORD],
        "the setup's record is not the service's"
    );
    assert_eq!(console.start(None, &Script::new()), Ok(Prepared::Plain));
}

#[test]
fn package_adoption_a_pending_hosts_files_are_replaced() {
    let console = pending();
    assert_eq!(run(&console, &Script::new()), Ok(()));
    assert_adopted(&console, staged_toml(&console).as_bytes());
}

/// "whatever final `agent.toml` or `server-ca.pem` exists": a reparse point is
/// deleted, what it names is not.
#[test]
fn package_adoption_a_final_reparse_point_is_deleted_and_not_followed() {
    let console = host();
    let target = console.outside("target");
    std::fs::create_dir(&target).expect("a target");
    std::fs::write(target.join("kept"), b"kept").expect("a file behind it");
    junction(&console.paths.config, &target);
    stage(&console);
    assert_eq!(run(&console, &Script::new()), Ok(()));
    assert_adopted(&console, staged_toml(&console).as_bytes());
    assert_eq!(bytes_of(&target.join("kept")), b"kept");
}

/// A step that fails: its code, and what it leaves.
#[test]
fn package_adoption_a_failure_at_each_step_has_its_code_and_leaves_a_retry() {
    for point in steps() {
        let console = pending();
        let script = Script::new().fail(point);
        let (code, line) = refusal(run(&console, &script));
        let (expected, prefix) = match point {
            Point::AdoptRead(STAGED_CONFIG) => (2, staged_config_line(&console)),
            Point::AdoptRead(_) => (6, ANCHOR_LINE.to_string()),
            Point::CloseProductRoot | Point::CloseDataRoot => (13, LOG_LINE.to_string()),
            _ => (5, IDENTITY_LINE.to_string()),
        };
        assert_eq!(code, expected, "{point:?}: {line}");
        assert!(line.starts_with(&prefix), "{point:?}: {line}");
        assert_eq!(
            script.seen().last(),
            Some(&point),
            "nothing after {point:?}"
        );
        assert_left_at(&console, point, false);
    }
}

/// A process that dies at each step (SPEC-024, the paragraph after item 6.6).
#[test]
fn package_adoption_a_crash_at_each_step_leaves_what_the_contract_says() {
    for point in steps() {
        let console = pending();
        let script = Script::new().crash(point);
        assert_eq!(run(&console, &script), Err(Stop::Crashed), "{point:?}");
        assert_eq!(
            script.seen().last(),
            Some(&point),
            "nothing after {point:?}"
        );
        assert_left_at(&console, point, true);
    }
}

/// What an adoption stopped at `point` leaves on a pending host: the earlier
/// files until step 6's deletions, nothing of the staged file a rename took,
/// and a restart without arguments that adopts nothing.
fn assert_left_at(console: &Console, point: Point, crashed: bool) {
    let position = steps().iter().position(|p| *p == point).expect("a step");
    let at = |p: Point| steps().iter().position(|q| *q == p).expect("a step");
    let what = format!("{point:?}, crashed: {crashed}");

    let config_deleted = position > at(Point::AdoptDelete(CONFIG));
    let anchor_deleted = position > at(Point::AdoptDelete(ANCHOR));
    let anchor_renamed = position > at(Point::AdoptRename(ANCHOR));

    if config_deleted {
        assert!(!console.paths.config.exists(), "{what}: agent.toml is gone");
    } else {
        assert_eq!(bytes_of(&console.paths.config), EARLIER_CONFIG, "{what}");
    }
    if anchor_renamed {
        assert_eq!(
            bytes_of(&console.paths.trust_anchor),
            console.pki.trust_anchor_pem.as_bytes(),
            "{what}"
        );
    } else if anchor_deleted {
        assert!(!console.paths.trust_anchor.exists(), "{what}");
    } else {
        assert_eq!(
            bytes_of(&console.paths.trust_anchor),
            EARLIER_ANCHOR,
            "{what}"
        );
    }
    assert!(
        staged(console, STAGED_CONFIG).exists(),
        "{what}: the retry is the setup's"
    );
    assert_eq!(
        staged(console, STAGED_ANCHOR).exists(),
        !anchor_renamed,
        "{what}"
    );

    // The closing of step 5 is not undone, and the next start adopts nothing.
    let before = console.whole();
    assert_eq!(
        console.start(None, &Script::new()),
        Ok(Prepared::Plain),
        "{what}"
    );
    assert_same_tree(&before, &console.whole(), &what);
}

/// Step 5: what cannot be closed is code 13, and nothing else changes.
#[test]
fn package_adoption_directories_that_cannot_be_closed_are_code_13() {
    let console = fresh_and_staged();
    set_dacl(
        &console.paths.data_root,
        &format!("D:P(A;OICI;FA;;;{})(A;OICI;WD;;;BU)", sid()),
    );
    let before = console.whole();
    let (code, line) = refusal(run(&console, &Script::new()));
    assert_eq!(code, 13, "{line}");
    assert!(line.starts_with(LOG_LINE), "{line}");
    let mut after = console.whole();
    let mut expected = before;
    // <P> is closed before the data root is refused, and stays so.
    after.remove("CyberGuard");
    expected.remove("CyberGuard");
    assert_same_tree(&expected, &after, "the staged files are not restricted");
}

/// Step 5, its link count read again after the change.
#[test]
fn package_adoption_a_link_made_while_restricting_is_code_5() {
    let console = fresh_and_staged();
    let target = staged(&console, STAGED_CONFIG);
    let other = console.outside("another-name");
    let script = Script::new().hook(Point::AdoptRestricted(STAGED_CONFIG), move || {
        std::fs::hard_link(&target, &other).expect("a link");
    });
    let (code, line) = refusal(run(&console, &script));
    assert_eq!(code, 5, "{line}");
    assert!(line.contains("hard-linked"), "{line}");
    assert!(!console.paths.config.exists(), "nothing was adopted");
    assert!(staged(&console, STAGED_CONFIG).exists());
}

/// Step 6 starts "with no name `identity`": one a file transaction committed
/// after step 3, or an administrator made.
#[test]
fn package_adoption_a_name_identity_that_appears_stops_step_6() {
    let console = pending();
    let identity = console.paths.identity_dir.clone();
    let script = Script::new().hook(Point::AdoptNames, move || {
        std::fs::create_dir(&identity).expect("a directory");
    });
    let (code, line) = refusal(run(&console, &script));
    assert_eq!(code, 5, "{line}");
    assert!(line.starts_with(IDENTITY_LINE), "{line}");
    assert_eq!(bytes_of(&console.paths.config), EARLIER_CONFIG);
    assert_eq!(bytes_of(&console.paths.trust_anchor), EARLIER_ANCHOR);
}

/// Step 6 stops on a listing that fails: it cannot say there is no `identity`.
#[test]
fn package_adoption_a_listing_that_fails_stops_step_6() {
    let console = pending();
    let data_root = console.paths.data_root.clone();
    let script = Script::new().hook(Point::AdoptNames, move || {
        set_dacl(&data_root, "D:P");
    });
    let (code, line) = refusal(run(&console, &script));
    set_dacl(&console.paths.data_root, &before_closing());
    assert_eq!(code, 5, "{line}");
    assert!(line.starts_with(IDENTITY_LINE), "{line}");
    assert_eq!(bytes_of(&console.paths.config), EARLIER_CONFIG);
}

/// Step 6: a final that cannot be deleted is code 5, and nothing is adopted.
#[test]
fn package_adoption_a_final_that_cannot_be_deleted_is_code_5() {
    let console = host();
    std::fs::create_dir(&console.paths.config).expect("a directory");
    std::fs::write(console.paths.config.join("inside"), b"x").expect("not empty");
    stage(&console);
    let (code, line) = refusal(run(&console, &Script::new()));
    assert_eq!(code, 5, "{line}");
    assert!(staged(&console, STAGED_CONFIG).exists());
}

/// "without replacing": a name that appears before a rename stays.
#[test]
fn package_adoption_a_rename_never_replaces() {
    for (point, staged_name) in [
        (Point::AdoptRename(ANCHOR), STAGED_ANCHOR),
        (Point::AdoptRename(CONFIG), STAGED_CONFIG),
    ] {
        let console = fresh_and_staged();
        let name = if staged_name == STAGED_ANCHOR {
            console.paths.trust_anchor.clone()
        } else {
            console.paths.config.clone()
        };
        let planted = name.clone();
        let script = Script::new().hook(point, move || {
            std::fs::write(&planted, b"planted").expect("a name");
        });
        let (code, line) = refusal(run(&console, &script));
        assert_eq!(code, 5, "{line}");
        assert_eq!(bytes_of(&name), b"planted");
        assert!(staged(&console, staged_name).exists());
    }
}

/// "The retry is the setup": whatever an interrupted adoption left, a new
/// staging adopts, the anchor it had already renamed included.
#[test]
fn package_adoption_retries_after_an_interruption() {
    for point in [
        Point::AdoptDelete(CONFIG),
        Point::AdoptDelete(ANCHOR),
        Point::AdoptRename(ANCHOR),
        Point::AdoptRename(CONFIG),
    ] {
        let console = pending();
        let script = Script::new().crash(point);
        assert_eq!(run(&console, &script), Err(Stop::Crashed), "{point:?}");
        for name in [STAGED_CONFIG, STAGED_ANCHOR] {
            let path = staged(&console, name);
            if path.exists() {
                std::fs::remove_file(path).expect("the setup removes what it staged");
            }
        }
        let retried = format!("{}# the retry\n", staged_toml(&console));
        stage_config(&console, &retried);
        stage_anchor(&console);
        assert_eq!(run(&console, &Script::new()), Ok(()), "{point:?}");
        assert_adopted(&console, retried.as_bytes());
    }
}

/// "A start without them never adopts, and leaves staged files untouched."
#[test]
fn package_adoption_a_start_without_arguments_adopts_nothing() {
    for console in [fresh_and_staged(), pending()] {
        let before = console.whole();
        assert_eq!(console.start(None, &Script::new()), Ok(Prepared::Plain));
        assert_same_tree(&before, &console.whole(), "a start without arguments");
    }
}
