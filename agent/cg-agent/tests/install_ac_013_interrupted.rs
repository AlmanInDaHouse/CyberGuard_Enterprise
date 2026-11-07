//! SPEC-024 install_ac_013, what surrounds the route (its Amendment
//! 2026-11-05, items 3, 4, 5 and 7, and the Amendment 2026-11-07 as item 2.3
//! meets it), unelevated over temporary roots: the record; each interrupted
//! state completed, reversed or refused; a staging or legacy directory that
//! does not count, ignored and left; the old copies under both choices, one
//! the service cannot reach reported without stopping the start; no secret
//! or SID in any line; and an operation an opportunistic lock makes wait,
//! cancelled after `Environment.wait`.
//!
//! An interruption is `Injected::Crash`: the test calls `prepare` again, as
//! the next start. The custody names the current user where the service's
//! names SYSTEM, and real SYSTEM is the enrolling account
//! (`migration_fixtures`).
#![cfg(windows)]

mod common;
mod custody_fixtures;
mod migration_fixtures;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine as _;
use cg_agent::custody::{self, Kind, Principal};
use cg_agent::service::migration::{
    copies, prepare, Choice, Copies, Environment, Point, Prepared, Request, Stop, DEFAULT_REFUSAL,
};
use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_DELETE;

use migration_fixtures::{
    arguments_refusal, assert_clean, assert_in_place_line, assert_quotes_none, assert_same_tree,
    capture, config_refusal, entries, hold, identity_refusal, junction, log_refusal, names, oplock,
    programdata_child_sddl, refused, security, set_dacl, sid, test_custody, tree, with_owner,
    Console, Oplock, Script, Tree, ACCESS_READ, ACCESS_WRITE, DIRECTORIES, ENROLLING_SID, FILES,
    HYGIENE, IDENTITY_FILES, IN_PLACE, OPLOCK_RH, OPLOCK_RWH, RESTRICTED, SHARE_ALL, STAGED, TOKEN,
    WAIT,
};
use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::CertificateDer;

/// The names of the legacy directory after a commit with a leftover
/// temporary.
const COPIES: [&str; 6] = [
    "agent.toml",
    "server-ca.pem",
    "cert.pem",
    "key.dat",
    "identity.json",
    HYGIENE,
];

/// The points of item 2.5 and the first rename, at each of which a staging
/// directory with a record exists beside the console layout.
const RECORDED: [Point; 8] = [
    Point::Stage("agent.toml"),
    Point::Stage("server-ca.pem"),
    Point::Stage("identity"),
    Point::Stage("cert.pem"),
    Point::Stage("key.dat"),
    Point::Stage("identity.json"),
    Point::CheckStaged,
    Point::FirstRename,
];

/// `console` committed, with the legacy directory there (`keep`) or removed
/// by a whole pass (`remove`).
fn committed(keep_legacy: bool) -> Console {
    let console = Console::new();
    if keep_legacy {
        console.migrate(Choice::Keep);
    } else {
        console.migrate(Choice::Remove);
        let pass = console.copies(&Script::new()).expect("the pass runs");
        assert!(pass.directory_removed, "{pass:?}");
    }
    console
}

/// A console interrupted between the two renames, with no mark.
fn between_the_renames(choice: Choice) -> (Console, Request) {
    let console = Console::new();
    let request = console.request(choice);
    console.crash_at(Point::SecondRename, &request);
    assert!(!console.paths.data_root.exists(), "the first rename ran");
    assert!(console.paths.legacy().is_dir() && console.paths.staging().is_dir());
    (console, request)
}

/// A console whose rename back failed: the mark stands and the data root is
/// at the legacy directory's name.
fn reversal_pending(choice: Choice) -> (Console, Request, Tree) {
    let console = Console::new();
    let before = console.files();
    let request = console.request(choice);
    let script = Script::new()
        .fail(Point::SecondRename)
        .fail(Point::RenameBack);
    let (code, line) = refused(console.start(Some(&request), &script));
    assert_eq!(code, 5, "{line}");
    assert!(console.staged_record().reversal_pending, "{line}");
    (console, request, before)
}

fn names_of(list: &[String]) -> BTreeSet<&str> {
    list.iter().map(String::as_str).collect()
}

fn lock_held() -> Stop {
    Stop::Refused {
        code: 10,
        line: "cg-agent: another agent holds the instance lock".to_string(),
    }
}

/// A data root that holds its record, with no legacy directory left: item 4
/// reads it as a data root alone and the start runs, which the amendment
/// leaves to `Plain` or `Committed`: all a `Committed` adds is one more
/// empty pass of the copies.
fn assert_runs_after_the_last_copy(result: Result<Prepared, Stop>) {
    assert!(
        matches!(result, Ok(Prepared::Plain | Prepared::Committed)),
        "{result:?}"
    );
}

/// A DACL that does not pass `agent.toml`'s policy, for a file.
fn readable_by_others() -> String {
    format!("D:P(A;;FA;;;{})(A;;FR;;;BU)", sid())
}

/// The staged identity becomes `other`'s, in place.
fn write_staged_identity_of(console: &Console, other: &Console) {
    for (path, bytes) in console.staged_identity_of(other) {
        std::fs::write(path, bytes).expect("write");
    }
}

/// Item 5's report of a copy not reached: a warning in the file log, and
/// the name in `migration.json`.
fn assert_reported(console: &Console, lines: &[String], name: &str) {
    assert!(
        lines
            .iter()
            .any(|line| line.contains("\"WARN\"") && line.contains(name)),
        "{name}: {lines:?}"
    );
    let record = console.record();
    assert!(
        record.copies_not_reached.iter().any(|c| c.contains(name)),
        "{name}: {record:?}"
    );
}

// --- Item 3: the record -----------------------------------------------------

/// Item 3: the record moves with the staging directory into the data root,
/// where it passes `agent.toml`'s policy, and holds what item 2.5 wrote.
#[test]
fn install_ac_013_the_record_moves_into_the_data_root() {
    let console = Console::new();
    let request = console.request(Choice::Keep);
    console.crash_at(Point::FirstRename, &request);
    let staged = console.staged_record();
    assert_eq!(
        console.start(Some(&request), &Script::new()),
        Ok(Prepared::Committed)
    );
    assert_eq!(console.record(), staged);
    custody::check(
        &console.paths.record(),
        Kind::File,
        &console.custody.configuration,
    )
    .expect("agent.toml's policy");
    assert_eq!(
        security(&console.paths.record()),
        with_owner(&console.custody.shared_file_sddl)
    );
}

/// Item 3: after the commit a record that fails `agent.toml`'s policy is
/// code 2, like a configuration file, with or without the legacy directory.
#[test]
fn install_ac_013_a_record_that_fails_the_policy_is_code_2() {
    type Spoil = fn(&Console);
    let spoils: [Spoil; 5] = [
        // Others can read it.
        |c| {
            let dacl = format!("D:P(A;;FA;;;{})(A;;FR;;;BU)", sid());
            set_dacl(&c.paths.record(), &dacl);
        },
        // A DACL that is not protected: a new file inherits the data root's.
        |c| {
            let bytes = std::fs::read(c.paths.record()).expect("read");
            std::fs::remove_file(c.paths.record()).expect("remove");
            std::fs::write(c.paths.record(), bytes).expect("write");
        },
        // Another link.
        |c| std::fs::hard_link(c.paths.record(), c.outside("another-name")).expect("link"),
        // A directory of that name.
        |c| {
            std::fs::remove_file(c.paths.record()).expect("remove");
            std::fs::create_dir(c.paths.record()).expect("a directory");
        },
        // A reparse point of that name.
        |c| {
            let target = c.outside("target");
            std::fs::create_dir(&target).expect("a target");
            std::fs::remove_file(c.paths.record()).expect("remove");
            junction(&c.paths.record(), &target);
        },
    ];
    for keep_legacy in [true, false] {
        for spoil in spoils {
            let console = committed(keep_legacy);
            spoil(&console);
            let before = console.whole();
            let line = config_refusal(console.start(None, &Script::new()), &console.paths.record());
            assert!(line.contains("migration.json"), "{line}");
            assert_same_tree(&before, &console.whole(), "nothing changes");
        }
    }
}

/// Item 3: a record that does not parse is code 2 too, and the line quotes
/// nothing of it.
#[test]
fn install_ac_013_a_record_that_does_not_parse_is_code_2() {
    for keep_legacy in [true, false] {
        let console = committed(keep_legacy);
        let torn = format!("{{\"enrolling_account\": \"{ENROLLING_SID}\", \"token\": \"{TOKEN}");
        std::fs::write(console.paths.record(), torn).expect("a torn record");
        let before = console.whole();
        let line = config_refusal(console.start(None, &Script::new()), &console.paths.record());
        assert!(line.contains("migration.json"), "{line}");
        assert_same_tree(&before, &console.whole(), "nothing changes");
    }
}

/// Item 4 against the Amendment 2026-11-04, item 3: the data directories'
/// check runs before the data root's record is read, with or without the
/// legacy directory.
#[test]
fn install_ac_013_the_data_directories_are_checked_before_the_record() {
    for keep_legacy in [true, false] {
        let console = committed(keep_legacy);
        std::fs::write(console.paths.record(), b"not a record").expect("write");
        set_dacl(&console.paths.data_root, &programdata_child_sddl());
        let line = log_refusal(console.start(None, &Script::new()));
        assert!(!line.contains(DEFAULT_REFUSAL), "no console layout: {line}");
    }
}

// --- Item 4: what counts ----------------------------------------------------

/// What an ordinary user, or anything but the service, left at a name.
#[derive(Debug, Clone, Copy)]
enum Stray {
    File,
    Junction,
    /// A directory others can reach, with a record that would pass.
    Broad,
    /// A directory with an inherited DACL, with such a record.
    Unprotected,
    /// A directory SYSTEM does not own, with such a record.
    NotSystems,
}

const STRAYS: [Stray; 5] = [
    Stray::File,
    Stray::Junction,
    Stray::Broad,
    Stray::Unprotected,
    Stray::NotSystems,
];

fn plant(console: &mut Console, path: &Path, stray: Stray) {
    match stray {
        Stray::File => std::fs::write(path, b"an ordinary user's").expect("a file"),
        // What it points to counts, so that following it changes the outcome.
        Stray::Junction => {
            let target = console.outside("target");
            console.counting_directory(&target);
            console.plant_record(&target, Choice::Keep);
            junction(path, &target);
        }
        Stray::Broad => {
            let sddl = format!("D:P(A;OICI;FA;;;{})(A;OICI;FA;;;BU)", sid());
            custody::ensure_directory(path, &sddl, &console.custody.configuration)
                .expect("a broad directory");
            console.plant_record(path, Choice::Keep);
        }
        Stray::Unprotected => {
            std::fs::create_dir(path).expect("a plain directory");
            assert!(!security(path).contains("D:P"), "not protected");
            console.plant_record(path, Choice::Keep);
        }
        Stray::NotSystems => {
            console.counting_directory(path);
            console.plant_record(path, Choice::Keep);
            console.custody.configuration.owners = vec![Principal::System];
        }
    }
}

/// Item 4: a staging or legacy directory that does not count is not used,
/// read or deleted, so it changes no outcome of a start without arguments,
/// whatever the rest of the host is, a committed one included. There the
/// real legacy directory may hold the name, and `Stray::NotSystems` is left
/// out: the policy it narrows is the data root's record's too, and an
/// unelevated token has no other owner to give.
#[test]
fn install_ac_013_a_name_that_does_not_count_changes_no_outcome() {
    let closed = test_custody().closed_directory_sddl;
    let hosts: [fn(&str) -> Console; 6] = [
        |_| Console::new(),
        |_| Console::closed(),
        |sddl| Console::service_host(sddl),
        |sddl| {
            let console = Console::bare();
            custody::ensure_directory(
                &console.paths.product_root,
                sddl,
                &console.custody.configuration,
            )
            .expect("<P> alone");
            console
        },
        |_| committed(true),
        |_| committed(false),
    ];
    for host in hosts {
        for stray in STRAYS {
            for legacy in [false, true] {
                let mut console = host(&closed);
                let outcome = console.start(None, &Script::new());
                match &outcome {
                    Ok(Prepared::Plain | Prepared::ConsoleLayout | Prepared::Committed) => {}
                    Err(Stop::Refused { code: 13, .. }) => {}
                    other => panic!("this host's outcome: {other:?}"),
                }
                let path = if legacy {
                    console.paths.legacy()
                } else {
                    console.paths.staging()
                };
                let recorded = console.paths.record().exists();
                if path.exists() || (recorded && matches!(stray, Stray::NotSystems)) {
                    continue;
                }
                plant(&mut console, &path, stray);
                let before = console.whole();
                let script = Script::new();
                assert_eq!(
                    console.start(None, &script),
                    outcome,
                    "{stray:?} at {}",
                    path.display()
                );
                assert_eq!(script.seen(), [], "{stray:?}");
                assert_same_tree(&before, &console.whole(), "ignored and left");
            }
        }
    }
}

/// Items 4 and 2.2: on the migrating route `<P>` must hold nothing else, so
/// such a name is refused by name, and left.
#[test]
fn install_ac_013_a_name_that_does_not_count_is_refused_on_the_migrating_route() {
    for stray in STRAYS {
        for legacy in [false, true] {
            let mut console = Console::new();
            let path = if legacy {
                console.paths.legacy()
            } else {
                console.paths.staging()
            };
            let name = path.file_name().expect("a name").to_string_lossy();
            let name = name.into_owned();
            plant(&mut console, &path, stray);
            let before = tree(&path);
            let files = console.files();
            let request = console.request(Choice::Remove);
            let script = Script::new();
            let line = identity_refusal(console.start(Some(&request), &script));
            assert!(line.contains(&name), "{stray:?}: {line}");
            assert_in_place_line(&line, &DIRECTORIES);
            assert!(script.restrictions().is_empty(), "{line}");
            assert!(!script.saw(Point::DeleteInterruptedStaging), "{line}");
            assert_same_tree(&files, &console.files(), "no file changed");
            assert_same_tree(&before, &tree(&path), "neither read nor deleted");
        }
    }
}

// --- Item 4: a staging directory without a record ---------------------------

/// The ways a staging directory that counts holds no migration in progress:
/// a record that is absent, does not parse or fails the policy.
const WITHOUT_A_RECORD: [fn(&Console); 4] = [
    |c| std::fs::remove_file(c.staged("migration.json")).expect("remove"),
    |c| std::fs::write(c.staged("migration.json"), b"{\"choice").expect("torn"),
    |c| set_dacl(&c.staged("migration.json"), &readable_by_others()),
    |c| {
        std::fs::hard_link(c.staged("migration.json"), c.outside("another-name")).expect("link");
    },
];

/// Item 4: beside a console-layout data root, such a directory is the
/// service's own creation interrupted: a start without arguments is code 5
/// and names it, changing nothing. Items 2.1 and 2.4 ran before it existed,
/// so a refusal before the commit has all of them to name.
#[test]
fn install_ac_013_a_staging_directory_without_a_record_stops_a_plain_start() {
    let console = Console::new();
    let request = console.request(Choice::Remove);
    console.crash_at(Point::WriteRecord, &request);
    assert_eq!(entries(&console.paths.staging()), Vec::<String>::new());
    let before = console.whole();
    let line = identity_refusal(console.start(None, &Script::new()));
    assert!(line.contains("agent.migrating"), "{line}");
    assert_in_place_line(&line, &RESTRICTED);
    assert_same_tree(&before, &console.whole(), "nothing changes");

    for spoil in WITHOUT_A_RECORD {
        let console = Console::new();
        let request = console.request(Choice::Remove);
        console.crash_at(Point::Stage("server-ca.pem"), &request);
        spoil(&console);
        let before = console.whole();
        let line = identity_refusal(console.start(None, &Script::new()));
        assert!(line.contains("agent.migrating"), "{line}");
        assert_in_place_line(&line, &RESTRICTED);
        assert_same_tree(&before, &console.whole(), "nothing changes");
    }
}

/// Item 4: a start with migration arguments deletes it and migrates, the
/// lock taken before the deletion.
#[test]
fn install_ac_013_a_staging_directory_without_a_record_is_deleted_and_migrated() {
    let console = Console::new();
    let request = console.request(Choice::Remove);
    console.crash_at(Point::WriteRecord, &request);
    let script = Script::new();
    assert_eq!(
        console.start(Some(&request), &script),
        Ok(Prepared::Committed)
    );
    assert!(script.saw(Point::DeleteInterruptedStaging));
    console.assert_committed();

    for spoil in WITHOUT_A_RECORD {
        let console = Console::new();
        let request = console.request(Choice::Keep);
        console.crash_at(Point::Stage("server-ca.pem"), &request);
        spoil(&console);
        let before = console.whole();
        let script = Script::new();
        let result = console.start_with_lock(Some(&request), &script, &mut || Err(lock_held()));
        let Err(Stop::Refused { code: 10, line }) = result else {
            panic!("code 10: {result:?}");
        };
        assert_in_place_line(&line, &FILES);
        assert!(!script.saw(Point::DeleteInterruptedStaging));
        assert_same_tree(&before, &console.whole(), "the lock comes first");

        let script = Script::new();
        assert_eq!(
            console.start(Some(&request), &script),
            Ok(Prepared::Committed)
        );
        assert!(script.saw(Point::DeleteInterruptedStaging));
        console.assert_committed();
    }
}

/// Item 4: the deletion does not follow a reparse point.
#[test]
fn install_ac_013_deleting_the_interrupted_creation_follows_no_reparse_point() {
    let console = Console::new();
    let request = console.request(Choice::Remove);
    console.crash_at(Point::WriteRecord, &request);
    let target = console.outside("target");
    std::fs::create_dir(&target).expect("a target");
    std::fs::write(target.join("kept.txt"), b"not the service's").expect("a file");
    junction(&console.staged("link"), &target);
    assert_eq!(
        console.start(Some(&request), &Script::new()),
        Ok(Prepared::Committed)
    );
    assert_eq!(
        std::fs::read(target.join("kept.txt")).expect("still there"),
        b"not the service's"
    );
    console.assert_committed();
}

/// Item 4: a deletion that fails stops the start, which is a refusal before
/// the commit, the console installation in place. The item gives it no code;
/// code 5, the one item 4 gives what stops around it, is a decision taken
/// with the implementation.
#[test]
fn install_ac_013_a_failed_deletion_of_the_interrupted_creation_is_code_5() {
    let console = Console::new();
    let before = console.files();
    let request = console.request(Choice::Remove);
    console.crash_at(Point::WriteRecord, &request);
    let script = Script::new().fail(Point::DeleteInterruptedStaging);
    let line = identity_refusal(console.start(Some(&request), &script));
    assert!(line.contains("agent.migrating"), "{line}");
    assert_in_place_line(&line, &RESTRICTED);
    assert!(console.paths.staging().is_dir(), "not deleted");
    console.assert_console_in_place(&before, &FILES);
}

// --- Item 4: before the first rename ----------------------------------------

/// Item 4: a staging directory beside a console-layout data root, at each
/// point of item 2.5: a start without arguments is code 5, which names the
/// interrupted migration; one with the recorded arguments continues from
/// item 2.1 and commits, keeping the enrolling account of the record.
#[test]
fn install_ac_013_a_migration_interrupted_before_the_first_rename_needs_its_arguments() {
    for point in RECORDED {
        let console = Console::new();
        let request = console.request(Choice::Keep);
        console.crash_at(point, &request);
        assert_eq!(
            console.staged_record().enrolling_account.as_deref(),
            Some(ENROLLING_SID)
        );

        let before = console.whole();
        let script = Script::new();
        let line = identity_refusal(console.start(None, &script));
        assert!(line.contains("migration"), "{point:?}: {line}");
        assert_in_place_line(&line, &RESTRICTED);
        assert_eq!(script.seen(), [], "{point:?}");
        assert_same_tree(&before, &console.whole(), "nothing changes");

        let script = Script::new();
        assert_eq!(
            console.start(Some(&request), &script),
            Ok(Prepared::Committed),
            "{point:?}"
        );
        assert_eq!(
            script.seen()[..2],
            [Point::CloseProductRoot, Point::CloseDataRoot],
            "it continues from item 2.1"
        );
        console.assert_committed();
        assert_eq!(
            console.record().enrolling_account.as_deref(),
            Some(ENROLLING_SID),
            "{point:?}: the record holds it once item 2.4 has run"
        );
    }
}

/// Item 2: "On every start that finds a migration in progress the lock is
/// likewise taken before any rename or deletion", and a continued migration
/// is a migrating start: with the lock held nothing runs at all.
#[test]
fn install_ac_013_a_continued_migration_takes_the_lock_first() {
    let console = Console::new();
    let request = console.request(Choice::Remove);
    console.crash_at(Point::Stage("agent.toml"), &request);
    let before = console.whole();
    let script = Script::new();
    let result = console.start_with_lock(Some(&request), &script, &mut || Err(lock_held()));
    let Err(Stop::Refused { code: 10, line }) = result else {
        panic!("code 10: {result:?}");
    };
    assert_in_place_line(&line, &FILES);
    assert_eq!(script.seen(), [], "no step ran");
    assert_same_tree(&before, &console.whole(), "nothing changes");
}

/// Item 2: a refusal at item 2.1 of a continued migration still names what
/// the earlier attempt restricted, not "none of it".
#[test]
fn install_ac_013_a_continued_refusal_at_the_closing_names_what_is_restricted() {
    let console = Console::new();
    let request = console.request(Choice::Remove);
    console.crash_at(Point::Stage("agent.toml"), &request);
    let script = Script::new().fail(Point::CloseProductRoot);
    let result = console.start(Some(&request), &script);
    let Err(Stop::Refused { code: 13, line }) = result else {
        panic!("code 13: {result:?}");
    };
    assert_in_place_line(&line, &FILES);
}

/// Item 2: a later start's refusal says what an earlier, refused one
/// restricted, though there is no staging directory: item 2.1 is never
/// undone. Here the lock's refusal, the likely retry while the console
/// agent runs again.
#[test]
fn install_ac_013_a_retry_after_a_refusal_names_what_is_restricted() {
    let console = Console::new();
    let mut wrong = console.request(Choice::Remove);
    wrong.fingerprints = BTreeSet::from([format!("sha256:{}", "0".repeat(64))]);
    let line = match console.start(Some(&wrong), &Script::new()) {
        Err(Stop::Refused { code: 6, line }) => line,
        other => panic!("code 6: {other:?}"),
    };
    assert_in_place_line(&line, &DIRECTORIES);

    let request = console.request(Choice::Remove);
    let script = Script::new();
    let result = console.start_with_lock(Some(&request), &script, &mut || Err(lock_held()));
    let Err(Stop::Refused { code: 10, line }) = result else {
        panic!("code 10: {result:?}");
    };
    assert_in_place_line(&line, &DIRECTORIES);
    assert!(!line.contains("none of it restricted"), "{line}");
}

/// Item 4: between the renames, a record that cannot be read stops the
/// start, which names both directories.
#[test]
fn install_ac_013_a_record_held_open_between_the_renames_names_both_directories() {
    use std::os::windows::fs::OpenOptionsExt as _;
    let (console, request) = between_the_renames(Choice::Remove);
    let record = console.paths.staging().join("migration.json");
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&record)
        .expect("hold the record");
    let line = identity_refusal(console.start(Some(&request), &Script::new()));
    drop(held);
    assert!(line.contains("agent.migrating"), "{line}");
    assert!(line.contains("agent.legacy"), "{line}");
    // Beside no data root, removing the staging directory would strand it.
    assert!(!line.contains("removes the staging directory"), "{line}");
    assert_eq!(console.start(None, &Script::new()), Ok(Prepared::Committed));
}

/// The preamble: the record's rewrite that Windows makes wait is cancelled
/// too, here recording a reversal, and the start stops naming both
/// directories, the staging directory and its record left.
#[test]
fn install_ac_013_a_rewrite_of_the_record_an_oplock_makes_wait_is_cancelled() {
    let console = Console::new();
    let record = console.paths.staging().join("migration.json");
    let kept = Kept::default();
    let hook = kept.hook(record.clone(), OPLOCK_RH);
    let script = Arc::new(
        Script::new()
            .fail(Point::SecondRename)
            .hook(Point::RecordReversal, hook),
    );
    let request = console.request(Choice::Remove);
    let (result, elapsed) = timed_start(&console, Some(request), Arc::clone(&script));
    drop(kept.take());
    let line = identity_refusal(result);
    assert_waited(elapsed);
    assert!(line.contains("agent.legacy"), "{line}");
    assert!(line.contains("is in use by another process"), "{line}");
    assert!(record.is_file(), "the record stays");
}

/// The Amendment 2026-11-07 (second), item 2: a named stream on `<P>` or the
/// data root is neither listed nor read and stops nothing; the data root's
/// moves with it, and with `remove` goes with the legacy directory.
#[test]
fn install_ac_013_a_named_stream_stops_nothing() {
    let console = Console::new();
    let stream = |path: &Path| format!("{}:planted", path.display());
    std::fs::write(stream(&console.paths.product_root), b"p").expect("a stream on <P>");
    std::fs::write(stream(&console.paths.data_root), b"d").expect("a stream on the data root");
    console.migrate(Choice::Remove);
    assert_eq!(
        std::fs::read(stream(&console.paths.legacy())).expect("moved with it"),
        b"d"
    );
    let pass = console.copies(&Script::new()).expect("the pass runs");
    assert!(pass.directory_removed, "{pass:?}");
    assert_eq!(
        std::fs::read(stream(&console.paths.product_root)).expect("stays"),
        b"p"
    );
}

/// The Amendment 2026-11-07 (second), item 1: a name a pending file
/// transaction holds is invisible to item 2.2's listing, and the operation
/// that needs it fails with code 5, whose line says so.
#[test]
fn install_ac_013_a_name_a_file_transaction_holds_says_so() {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateDirectoryTransactedW, CreateTransaction, RollbackTransaction,
    };
    let console = Console::new();
    let name: Vec<u16> = console
        .paths
        .staging()
        .display()
        .to_string()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: no attributes, no unit of work, no description.
    let transaction = unsafe {
        CreateTransaction(
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(transaction, INVALID_HANDLE_VALUE, "CreateTransaction");
    // SAFETY: no template and no attributes; the name lives for the call.
    let created = unsafe {
        CreateDirectoryTransactedW(
            std::ptr::null(),
            name.as_ptr(),
            std::ptr::null(),
            transaction,
        )
    };
    assert_ne!(created, 0, "CreateDirectoryTransactedW");
    let request = console.request(Choice::Remove);
    let line = identity_refusal(console.start(Some(&request), &Script::new()));
    // SAFETY: the transaction created above, rolled back and closed once.
    unsafe {
        RollbackTransaction(transaction);
        CloseHandle(transaction);
    }
    assert!(line.contains("file transaction"), "{line}");
    assert_in_place_line(&line, &FILES);
}

/// A file transaction and what it holds, rolled back on drop.
struct Transaction(windows_sys::Win32::Foundation::HANDLE);

impl Transaction {
    fn new() -> Self {
        // SAFETY: no attributes, no unit of work, no description.
        let handle = unsafe {
            windows_sys::Win32::Storage::FileSystem::CreateTransaction(
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(
            handle,
            windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE,
            "CreateTransaction"
        );
        Transaction(handle)
    }

    /// Create `name`, a file or a named stream, in the transaction, and
    /// close its handle: the entry stays pending.
    fn create(&self, name: &str) {
        use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_WRITE, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::Storage::FileSystem::{
            CreateFileTransactedW, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS,
        };
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: the name lives for the call; no attributes, template or
        // miniversion; the handle is closed below.
        let handle = unsafe {
            CreateFileTransactedW(
                wide.as_ptr(),
                GENERIC_WRITE,
                0,
                std::ptr::null(),
                CREATE_NEW,
                FILE_ATTRIBUTE_NORMAL | FILE_FLAG_BACKUP_SEMANTICS,
                std::ptr::null_mut(),
                self.0,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        assert_ne!(handle, INVALID_HANDLE_VALUE, "CreateFileTransactedW {name}");
        // SAFETY: the handle opened above.
        unsafe { CloseHandle(handle) };
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        // SAFETY: the transaction this value holds, rolled back and closed
        // once.
        unsafe {
            windows_sys::Win32::Storage::FileSystem::RollbackTransaction(self.0);
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

/// The Amendment 2026-11-07 (second), item 1: a failure a pending
/// transaction causes is the failing step's. A named stream pending on `<P>`
/// stops the closing, code 13, as a sharing failure.
#[test]
fn install_ac_013_a_pending_stream_stops_the_closing_with_code_13() {
    let console = Console::new();
    let transaction = Transaction::new();
    transaction.create(&format!("{}:held", console.paths.product_root.display()));
    let request = console.request(Choice::Remove);
    let result = console.start(Some(&request), &Script::new());
    drop(transaction);
    let Err(Stop::Refused { code: 13, line }) = result else {
        panic!("code 13: {result:?}");
    };
    assert!(line.contains("is in use by another process"), "{line}");
    assert_in_place_line(&line, &[]);
}

/// The same item: an entry pending in the data root is unseen by item 2.2
/// and stops the first rename, code 5, whose line says that a transaction
/// holds it; one pending under `<P>` stops nothing, and stays out of the
/// new data root.
#[test]
fn install_ac_013_a_pending_entry_stops_the_first_rename_with_code_5() {
    let console = Console::new();
    let transaction = Transaction::new();
    transaction.create(&format!("{}\\late", console.paths.data_root.display()));
    let request = console.request(Choice::Remove);
    let line = identity_refusal(console.start(Some(&request), &Script::new()));
    drop(transaction);
    assert!(line.contains("file transaction"), "{line}");
    assert_in_place_line(&line, &FILES);
    assert!(!console.paths.legacy().exists(), "no rename ran");

    let console = Console::new();
    let transaction = Transaction::new();
    transaction.create(&format!("{}\\late", console.paths.product_root.display()));
    let request = console.request(Choice::Remove);
    assert_eq!(
        console.start(Some(&request), &Script::new()),
        Ok(Prepared::Committed)
    );
    drop(transaction);
}

/// Item 4: a record that cannot be read now is not a record that is not
/// there: the start stops, and the staging directory and its record stay.
#[test]
fn install_ac_013_a_record_held_open_is_not_taken_for_absent() {
    use std::os::windows::fs::OpenOptionsExt as _;
    let console = Console::new();
    let request = console.request(Choice::Remove);
    console.crash_at(Point::Stage("agent.toml"), &request);
    let before = console.whole();
    let record = console.paths.staging().join("migration.json");
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&record)
        .expect("hold the record");
    let script = Script::new();
    let line = identity_refusal(console.start(Some(&request), &script));
    drop(held);
    assert!(line.contains("record cannot be read"), "{line}");
    assert!(line.contains("removes the staging directory"), "{line}");
    assert_in_place_line(&line, &FILES);
    assert_eq!(script.seen(), [], "no step ran");
    assert_same_tree(&before, &console.whole(), "nothing changes");
}

/// The preamble and item 4: a batch opportunistic lock on a name an ordinary
/// user planted makes even an open for attributes wait; it is cancelled, the
/// name does not count, and the start's outcome stands.
#[test]
fn install_ac_013_a_planted_name_under_a_batch_oplock_changes_no_outcome() {
    for name in ["agent.migrating", "agent.legacy"] {
        let console = Console::new();
        let planted = console.paths.product_root.join(name);
        std::fs::write(&planted, b"planted").expect("plant");
        let held = migration_fixtures::batch_oplock(&planted).expect("a batch oplock on NTFS");
        let (result, elapsed) = timed_start(&console, None, Arc::new(Script::new()));
        drop(held);
        assert_waited(elapsed);
        let Err(Stop::Refused { code: 13, line }) = result else {
            panic!("code 13: {result:?}");
        };
        assert!(line.ends_with(DEFAULT_REFUSAL), "{line}");
        assert_eq!(std::fs::read(&planted).expect("left"), b"planted");
    }
}

/// The preamble, on the paths of a start without arguments: `read_checked`
/// and `check_if_exists` wait no longer than 30 seconds either.
#[test]
fn install_ac_013_a_checked_read_an_oplock_makes_wait_is_cancelled() {
    let root = tempfile::tempdir().expect("tempdir");
    let file = root.path().join("agent.toml");
    std::fs::write(&file, b"held").expect("write");
    let held = migration_fixtures::batch_oplock(&file).expect("a batch oplock on NTFS");
    let started = std::time::Instant::now();
    let error = cg_agent::custody::read_checked(&file, &custody_fixtures::user_only())
        .expect_err("cancelled");
    let elapsed = started.elapsed();
    drop(held);
    assert!(error.contains("is in use by another process"), "{error}");
    assert!(
        (std::time::Duration::from_secs(29)..std::time::Duration::from_secs(45)).contains(&elapsed),
        "{elapsed:?}"
    );
}

/// Item 5: the read-only attribute, which ordinary users can set on the
/// console's objects, keeps neither a copy nor the legacy directory.
#[test]
fn install_ac_013_a_read_only_copy_is_removed() {
    let console = Console::new();
    let read_only = |path: &std::path::Path| {
        let mut permissions = std::fs::metadata(path).expect("metadata").permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(path, permissions).expect("read-only");
    };
    read_only(&console.file("server-ca.pem"));
    console.migrate(Choice::Remove);
    read_only(&console.paths.legacy());
    let pass = console.copies(&Script::new()).expect("the pass runs");
    assert!(pass.directory_removed, "{pass:?}");
    assert_eq!(pass.not_reached, Vec::<String>::new());
    assert!(!console.paths.legacy().exists());
}

/// Item 2.2 on a continued migration: `<P>` may hold the staging directory
/// beside the data root, and nothing else.
#[test]
fn install_ac_013_a_continued_migration_refuses_another_entry() {
    let console = Console::new();
    let request = console.request(Choice::Keep);
    console.crash_at(Point::Stage("identity"), &request);
    let stray = console.paths.product_root.join("not-the-agents");
    std::fs::write(&stray, b"its content").expect("a stray file");
    let line = identity_refusal(console.start(Some(&request), &Script::new()));
    assert!(line.contains("not-the-agents"), "{line}");
    assert_in_place_line(&line, &RESTRICTED);
    assert!(stray.exists() && console.paths.staging().is_dir());
}

/// Items 2 and 4: a crash at any point of the route leaves a host the same
/// request migrates, whichever state of item 4 that point left.
#[test]
fn install_ac_013_a_crash_at_any_point_of_the_route_is_recovered() {
    for point in [
        Point::CloseProductRoot,
        Point::CloseDataRoot,
        Point::Restrict("agent.toml"),
        Point::Restrict("server-ca.pem"),
        Point::Restrict("cert.pem"),
        Point::Restrict("key.dat"),
        Point::Restrict("identity.json"),
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
    ] {
        let console = Console::new();
        let request = console.request(Choice::Remove);
        console.crash_at(point, &request);
        assert_eq!(
            console.start(Some(&request), &Script::new()),
            Ok(Prepared::Committed),
            "{point:?}"
        );
        console.assert_committed();
        assert_eq!(console.load_identity().agent_id, console.pki.agent_id);
        let pass = console.copies(&Script::new()).expect("the pass runs");
        assert_eq!(pass.not_reached, Vec::<String>::new(), "{point:?}");
        assert!(pass.directory_removed, "{point:?}");
    }
}

/// Items 2.5, 3 and 4: an interruption of the continuation. "It keeps the
/// staging directory and its record": a continuation that dropped them and
/// held them in memory would lose both to a crash, and with them the
/// enrolling account's SID, which the DACLs no longer hold, and the
/// arguments. Whatever the second crash stopped, a start without arguments
/// is still item 4's code 5, and the recorded arguments still commit.
#[test]
fn install_ac_013_a_second_interruption_keeps_the_record() {
    for second in [Point::WriteRecord, Point::Stage("cert.pem")] {
        let console = Console::new();
        let request = console.request(Choice::Keep);
        console.crash_at(Point::Stage("agent.toml"), &request);
        console.crash_at(second, &request);
        let record = console.staged_record();
        assert_eq!(
            record.enrolling_account.as_deref(),
            Some(ENROLLING_SID),
            "{second:?}"
        );
        assert_eq!(record.choice, Choice::Keep, "{second:?}");

        let line = identity_refusal(console.start(None, &Script::new()));
        assert_in_place_line(&line, &RESTRICTED);
        assert_eq!(
            console.start(Some(&request), &Script::new()),
            Ok(Prepared::Committed),
            "{second:?}"
        );
        console.assert_committed();
        assert_eq!(
            console.record().enrolling_account.as_deref(),
            Some(ENROLLING_SID),
            "{second:?}"
        );
    }
}

/// Item 4: a crash at the two points only a later start reaches: the
/// completion of a commit and the deletion of an interrupted creation.
/// Nothing ran, and the next start does what this one was doing.
#[test]
fn install_ac_013_a_crash_of_a_later_start_is_recovered() {
    let (console, _) = between_the_renames(Choice::Keep);
    let before = console.whole();
    let script = Script::new().crash(Point::CompleteCommit);
    assert_eq!(console.start(None, &script), Err(Stop::Crashed));
    assert_same_tree(&before, &console.whole(), "nothing after the crash ran");
    assert_eq!(console.start(None, &Script::new()), Ok(Prepared::Committed));
    console.assert_committed();

    let console = Console::new();
    let request = console.request(Choice::Remove);
    console.crash_at(Point::WriteRecord, &request);
    let before = console.whole();
    console.crash_at(Point::DeleteInterruptedStaging, &request);
    assert_same_tree(&before, &console.whole(), "nothing after the crash ran");
    assert_eq!(
        console.start(Some(&request), &Script::new()),
        Ok(Prepared::Committed)
    );
    console.assert_committed();
}

/// Item 2.5: on a continued migration the staged files are replaced and
/// the record holds this attempt's file IDs.
#[test]
fn install_ac_013_a_continued_migration_replaces_the_staged_files() {
    let console = Console::new();
    let request = console.request(Choice::Keep);
    console.crash_at(Point::Stage("identity"), &request);
    std::fs::write(console.staged("agent.toml"), b"an older attempt's").expect("write");
    let first = console.staged_record();
    let config = console.file("agent.toml");
    let bytes = std::fs::read(&config).expect("read");
    std::fs::remove_file(&config).expect("remove");
    std::fs::write(&config, bytes).expect("a new file of that name");

    assert_eq!(
        console.start(Some(&request), &Script::new()),
        Ok(Prepared::Committed)
    );
    console.generated_config();
    let record = console.record();
    assert_ne!(record.files["agent.toml"], first.files["agent.toml"]);
    assert_eq!(record.files["key.dat"], first.files["key.dat"]);
    let pass = console.copies(&Script::new()).expect("the pass runs");
    assert_eq!(pass.not_reached, Vec::<String>::new(), "this attempt's IDs");
}

/// Item 2.3: on a continued migration the key's fingerprint and `agent_id`
/// are the recorded ones: a record that holds others is code 5, and the
/// line repeats neither what the record holds nor what the files do.
#[test]
fn install_ac_013_a_continued_migration_checks_the_recorded_identity() {
    for (agent_id, key_fingerprint) in [(true, false), (false, true)] {
        let console = Console::new();
        let before = console.files();
        let request = console.request(Choice::Remove);
        console.crash_at(Point::Stage("agent.toml"), &request);
        let mut record = console.staged_record();
        if agent_id {
            record.agent_id = "01934abc-def0-7000-89ab-0000000000cc".to_string();
        }
        if key_fingerprint {
            record.key_fingerprint = "0".repeat(64);
        }
        // A rewrite in place keeps the file, its ID and its DACL.
        let bytes = serde_json::to_vec(&record).expect("a record");
        std::fs::write(console.staged("migration.json"), bytes).expect("rewrite");
        let script = Script::new();
        let line = identity_refusal(console.start(Some(&request), &script));
        assert_in_place_line(&line, &RESTRICTED);
        assert_quotes_none(
            &line,
            &[
                "01934abc-def0-7000-89ab-0000000000cc",
                &"0".repeat(64),
                &console.pki.agent_id,
                &console.key_fingerprint(),
            ],
        );
        assert!(script.restrictions().is_empty(), "item 2.3 is before 2.4");
        console.assert_console_in_place(&before, &FILES);
    }
}

/// Items 2.5 and 4: a crash between items 2.4 and 2.5 loses the SID, which
/// the record gives as unknown; nothing was recorded, so no migration is in
/// progress, and a start without arguments changes nothing.
#[test]
fn install_ac_013_a_crash_before_the_staging_directory_loses_the_sid() {
    let console = Console::new();
    let request = console.request(Choice::Remove);
    console.crash_at(Point::CreateStaging, &request);
    let before = console.whole();
    assert_eq!(
        console.start(None, &Script::new()),
        Ok(Prepared::ConsoleLayout)
    );
    assert_same_tree(&before, &console.whole(), "item 6: nothing changes");

    assert_eq!(
        console.start(Some(&request), &Script::new()),
        Ok(Prepared::Committed)
    );
    assert_eq!(console.record().enrolling_account, None);
}

/// Item 2.2 on a host a crash left half restricted: the file not yet
/// restricted still names the enrolling account, and the route takes the
/// mix; a crash between the two closures leaves R17's code 13.
#[test]
fn install_ac_013_a_crash_during_items_2_1_and_2_4_can_be_migrated_again() {
    let console = Console::new();
    let request = console.request(Choice::Keep);
    console.crash_at(Point::Restrict("identity.json"), &request);
    assert_eq!(
        console.start(None, &Script::new()),
        Ok(Prepared::ConsoleLayout)
    );
    assert_eq!(
        console.start(Some(&request), &Script::new()),
        Ok(Prepared::Committed)
    );
    assert_eq!(
        console.record().enrolling_account.as_deref(),
        Some(ENROLLING_SID)
    );

    let console = Console::new();
    let request = console.request(Choice::Keep);
    console.crash_at(Point::CloseDataRoot, &request);
    let line = log_refusal(console.start(None, &Script::new()));
    assert!(line.ends_with(DEFAULT_REFUSAL), "{line}");
    assert_eq!(
        console.start(Some(&request), &Script::new()),
        Ok(Prepared::Committed)
    );
}

// --- Item 4: between the renames --------------------------------------------

/// Item 4: no data root, a staging and a legacy directory, no mark: the
/// next start completes the commit without arguments (the service's
/// automatic start or its recovery), once the lock is taken.
#[test]
fn install_ac_013_a_crash_between_the_renames_is_completed_without_arguments() {
    for choice in [Choice::Remove, Choice::Keep] {
        let (console, _) = between_the_renames(choice);
        let before = console.whole();
        let script = Script::new();
        let result = console.start_with_lock(None, &script, &mut || Err(lock_held()));
        assert_eq!(result, Err(lock_held()));
        assert!(!script.saw(Point::CompleteCommit), "the lock comes first");
        assert_same_tree(&before, &console.whole(), "nothing changes");

        let script = Script::new();
        assert_eq!(console.start(None, &script), Ok(Prepared::Committed));
        assert!(script.saw(Point::CompleteCommit));
        console.assert_committed();
        assert_eq!(console.record().choice, choice, "the recorded arguments");
    }
}

/// Item 4: the recorded arguments may come with it; others are code 2,
/// before any change (item 1).
#[test]
fn install_ac_013_a_commit_is_completed_with_the_recorded_arguments_alone() {
    let (console, request) = between_the_renames(Choice::Remove);
    let before = console.whole();
    let other_fingerprint = Request {
        choice: Choice::Remove,
        fingerprints: BTreeSet::from([format!("sha256:{}", "c".repeat(64))]),
    };
    for other in [console.request(Choice::Keep), other_fingerprint] {
        arguments_refusal(console.start(Some(&other), &Script::new()));
        assert_same_tree(&before, &console.whole(), "before any change");
    }
    assert_eq!(
        console.start(Some(&request), &Script::new()),
        Ok(Prepared::Committed)
    );
    console.assert_committed();
}

/// Item 4: the commit is completed only once the staged tree passes item
/// 2.5's checks; otherwise code 5, which names both directories, and
/// nothing changes. That takes a record that passes the policy and parses,
/// without which no migration is in progress, and an identity with the
/// recorded `agent_id` and key: another's, whole, is consistent in itself.
#[test]
fn install_ac_013_a_commit_whose_staged_tree_fails_is_code_5_naming_both() {
    let spoils: [fn(&Console); 8] = [
        |c| std::fs::remove_file(c.staged("agent.toml")).expect("remove"),
        |c| std::fs::write(c.staged("identity/key.dat"), b"not sealed").expect("write"),
        |c| std::fs::remove_file(c.staged("identity/identity.json")).expect("remove"),
        |c| set_dacl(&c.staged("server-ca.pem"), &readable_by_others()),
        |c| std::fs::write(c.staged("migration.json"), b"{\"choice").expect("torn"),
        |c| set_dacl(&c.staged("migration.json"), &readable_by_others()),
        |c| write_staged_identity_of(c, &Console::new()),
        |c| {
            let other = Console::for_agent("01934abc-def0-7000-89ab-0000000000bb");
            write_staged_identity_of(c, &other);
        },
    ];
    for spoil in spoils {
        let (console, _) = between_the_renames(Choice::Remove);
        spoil(&console);
        let before = console.whole();
        let script = Script::new();
        let line = identity_refusal(console.start(None, &script));
        assert!(line.contains("agent.migrating"), "{line}");
        assert!(line.contains("agent.legacy"), "{line}");
        assert!(!script.saw(Point::CompleteCommit), "{line}");
        assert_same_tree(&before, &console.whole(), "nothing changes");
    }
}

/// Item 4: between the renames, a directory that stops counting is as if
/// absent: the other one alone is "any other combination", and nothing is
/// completed.
#[test]
fn install_ac_013_a_directory_that_stops_counting_between_the_renames_is_code_5() {
    let broad = format!("D:P(A;OICI;FA;;;{})(A;OICI;FA;;;BU)", sid());
    for (spoiled_legacy, named) in [(true, "agent.migrating"), (false, "agent.legacy")] {
        let (console, _) = between_the_renames(Choice::Keep);
        let path = if spoiled_legacy {
            console.paths.legacy()
        } else {
            console.paths.staging()
        };
        set_dacl(&path, &broad);
        let before = console.whole();
        let script = Script::new();
        let line = identity_refusal(console.start(None, &script));
        assert!(line.contains(named), "{line}");
        assert!(!script.saw(Point::CompleteCommit), "{line}");
        assert_same_tree(&before, &console.whole(), "nothing changes");
    }
}

/// Item 4: a completion the rename refuses is code 5, naming both, and the
/// next start tries again.
#[test]
fn install_ac_013_a_failed_completion_of_the_commit_is_retried() {
    let (console, _) = between_the_renames(Choice::Keep);
    let before = console.whole();
    let script = Script::new().fail(Point::CompleteCommit);
    let line = identity_refusal(console.start(None, &script));
    assert!(line.contains("agent.migrating"), "{line}");
    assert!(line.contains("agent.legacy"), "{line}");
    assert_same_tree(&before, &console.whole(), "nothing changes");

    assert_eq!(console.start(None, &Script::new()), Ok(Prepared::Committed));
    console.assert_committed();
}

/// The paragraph after item 2.6: after the commit the start goes on "from
/// the checks of the Amendment 2026-11-04, item 3": a data directory that
/// fails them in that same start is that amendment's code 13, whether the
/// route committed or the start completed an interrupted commit. Those are
/// no longer refusals before the commit, and the commit stands.
#[test]
fn install_ac_013_after_the_commit_the_start_checks_the_data_directories() {
    let console = Console::new();
    let product_root = console.paths.product_root.clone();
    let sddl = programdata_child_sddl();
    let script = Script::new().hook(Point::SecondRename, move || {
        set_dacl(&product_root, &sddl);
    });
    let request = console.request(Choice::Keep);
    let line = log_refusal(console.start(Some(&request), &script));
    assert!(!line.contains(IN_PLACE), "{line}");
    assert!(!line.contains(DEFAULT_REFUSAL), "{line}");
    console.assert_committed();

    let (console, _) = between_the_renames(Choice::Keep);
    let product_root = console.paths.product_root.clone();
    let sddl = programdata_child_sddl();
    let script = Script::new().hook(Point::CompleteCommit, move || {
        set_dacl(&product_root, &sddl);
    });
    let line = log_refusal(console.start(None, &script));
    assert!(!line.contains(IN_PLACE), "{line}");
    assert!(!line.contains(DEFAULT_REFUSAL), "{line}");
    console.assert_committed();
}

// --- Item 4: a reversal -----------------------------------------------------

/// Item 4: with a reversal recorded, the next start, with or without
/// arguments, renames the legacy directory back, clears the mark and stops
/// with code 5; the lock comes first. The record's two rewrites keep its
/// owner and DACL and what it held (item 3).
#[test]
fn install_ac_013_a_recorded_reversal_is_finished_by_the_next_start() {
    for with_arguments in [false, true] {
        let (console, request, before) = reversal_pending(Choice::Remove);
        let shared = with_owner(&console.custody.shared_file_sddl);
        let staged_record = console.staged("migration.json");
        assert_eq!(security(&staged_record), shared, "marking keeps them");
        assert_eq!(entries(&console.paths.staging()), STAGED, "no temporary");
        let arguments = with_arguments.then_some(&request);
        let whole = console.whole();
        let script = Script::new();
        let result = console.start_with_lock(arguments, &script, &mut || Err(lock_held()));
        assert_eq!(result, Err(lock_held()));
        assert_same_tree(&whole, &console.whole(), "the lock comes first");

        let script = Script::new();
        let line = identity_refusal(console.start(arguments, &script));
        assert_eq!(script.seen(), [Point::RenameBack, Point::ClearReversal]);
        assert_in_place_line(&line, &RESTRICTED);
        console.assert_console_in_place(&before, &FILES);
        assert!(
            !console.staged_record().reversal_pending,
            "the mark cleared"
        );
        assert_eq!(security(&staged_record), shared, "clearing keeps them");
        assert_eq!(entries(&console.paths.staging()), STAGED, "no temporary");

        // Back to a staging directory beside a console layout.
        identity_refusal(console.start(None, &Script::new()));
        assert_eq!(
            console.start(Some(&request), &Script::new()),
            Ok(Prepared::Committed)
        );
        console.assert_committed();
        assert_eq!(
            console.record().enrolling_account.as_deref(),
            Some(ENROLLING_SID),
            "the rewrites kept it"
        );
    }
}

/// Items 1 and 4: "with or without arguments" are the recorded ones or
/// none: others are code 2, before any change, and the reversal waits.
#[test]
fn install_ac_013_other_arguments_leave_a_recorded_reversal_alone() {
    let (console, _, _) = reversal_pending(Choice::Remove);
    let before = console.whole();
    let script = Script::new();
    arguments_refusal(console.start(Some(&console.request(Choice::Keep)), &script));
    assert!(!script.saw(Point::RenameBack), "not put back");
    assert_same_tree(&before, &console.whole(), "before any change");
    assert!(console.staged_record().reversal_pending, "the mark stays");
}

/// Item 4: while the rename back keeps failing, the mark stays and each
/// start says where the data root is.
#[test]
fn install_ac_013_a_reversal_that_fails_again_stays_recorded() {
    let (console, _, before) = reversal_pending(Choice::Keep);
    let script = Script::new().fail(Point::RenameBack);
    let line = identity_refusal(console.start(None, &script));
    assert!(line.contains("agent.legacy"), "{line}");
    assert!(line.contains("next start"), "{line}");
    assert!(line.contains("arguments"), "with or without them: {line}");
    assert!(!line.contains(IN_PLACE), "{line}");
    assert!(console.staged_record().reversal_pending, "the mark stays");

    identity_refusal(console.start(None, &Script::new()));
    console.assert_console_in_place(&before, &FILES);
}

/// Items 2.6 and 4: a crash at each step of the reversal. Before the mark,
/// the next start completes the commit; with the mark and no data root, it
/// finishes the reversal; after the rename back, the mark is cleared by the
/// recorded arguments' continuation (item 2.5).
#[test]
fn install_ac_013_a_crash_during_a_reversal_ends_as_the_record_says() {
    let console = Console::new();
    let request = console.request(Choice::Keep);
    let script = Script::new()
        .fail(Point::SecondRename)
        .crash(Point::RecordReversal);
    assert_eq!(console.start(Some(&request), &script), Err(Stop::Crashed));
    assert!(!console.staged_record().reversal_pending, "no mark yet");
    assert_eq!(console.start(None, &Script::new()), Ok(Prepared::Committed));
    console.assert_committed();

    let console = Console::new();
    let before = console.files();
    let request = console.request(Choice::Keep);
    let script = Script::new()
        .fail(Point::SecondRename)
        .crash(Point::RenameBack);
    assert_eq!(console.start(Some(&request), &script), Err(Stop::Crashed));
    assert!(console.staged_record().reversal_pending, "the mark stands");
    let line = identity_refusal(console.start(None, &Script::new()));
    assert_in_place_line(&line, &RESTRICTED);
    console.assert_console_in_place(&before, &FILES);

    let console = Console::new();
    let before = console.files();
    let request = console.request(Choice::Keep);
    let script = Script::new()
        .fail(Point::SecondRename)
        .crash(Point::ClearReversal);
    assert_eq!(console.start(Some(&request), &script), Err(Stop::Crashed));
    console.assert_console_in_place(&before, &FILES);
    assert!(console.staged_record().reversal_pending, "not yet cleared");
    let whole = console.whole();
    let line = identity_refusal(console.start(None, &Script::new()));
    assert!(line.contains("migration"), "{line}");
    assert_in_place_line(&line, &RESTRICTED);
    assert_same_tree(&whole, &console.whole(), "a start without arguments");
    assert_eq!(
        console.start(Some(&request), &Script::new()),
        Ok(Prepared::Committed)
    );
    console.assert_committed();
    assert!(!console.record().reversal_pending, "the mark is cleared");
}

/// Item 2.6: a failure to clear the mark after the rename back stops the
/// start like the reversal itself, and the continuation clears it.
#[test]
fn install_ac_013_a_mark_that_cannot_be_cleared_is_cleared_by_the_continuation() {
    let console = Console::new();
    let before = console.files();
    let request = console.request(Choice::Remove);
    let script = Script::new()
        .fail(Point::SecondRename)
        .fail(Point::ClearReversal);
    let line = identity_refusal(console.start(Some(&request), &script));
    assert_in_place_line(&line, &RESTRICTED);
    console.assert_console_in_place(&before, &FILES);
    assert_eq!(
        console.start(Some(&request), &Script::new()),
        Ok(Prepared::Committed)
    );
    assert!(!console.record().reversal_pending, "cleared");
}

/// Item 2.6: "before reversing it records the reversal". When that write
/// fails the start stops, and what the next start finds is one of item 4's
/// states: nothing of the identity is lost. The item gives that stop no
/// code; code 5, as for the rest of the reversal, is a decision taken with
/// the implementation.
#[test]
fn install_ac_013_a_reversal_that_cannot_be_recorded_stops_and_stays_recoverable() {
    let console = Console::new();
    let request = console.request(Choice::Remove);
    let script = Script::new()
        .fail(Point::SecondRename)
        .fail(Point::RecordReversal);
    let (code, line) = refused(console.start(Some(&request), &script));
    assert_eq!(code, 5, "{line}");
    // The Amendment 2026-11-07 (second), item 5: the data root is at the
    // legacy directory's name, so the line says neither "in place".
    assert!(line.contains("agent.migrating"), "{line}");
    assert!(line.contains("agent.legacy"), "{line}");
    assert!(line.contains("next start completes"), "{line}");
    assert!(!line.contains(IN_PLACE), "{line}");
    match console.start(None, &Script::new()) {
        Ok(Prepared::Committed) => console.assert_committed(),
        other => {
            identity_refusal(other);
            assert_eq!(
                console.start(Some(&request), &Script::new()),
                Ok(Prepared::Committed)
            );
            console.assert_committed();
        }
    }
}

// --- Item 4: a committed migration and any other combination -----------------

/// Item 4: a data root that is not a console layout, with a legacy
/// directory and the data root's record: a migration committed. The start
/// runs, changing nothing here; item 5 continues after `RUNNING`.
#[test]
fn install_ac_013_a_committed_migration_runs_without_arguments() {
    let console = committed(true);
    let before = console.whole();
    let script = Script::new();
    assert_eq!(console.start(None, &script), Ok(Prepared::Committed));
    assert_eq!(script.seen(), [], "no step ran");
    assert_same_tree(&before, &console.whole(), "nothing changes");

    let console = committed(false);
    let before = console.whole();
    assert_runs_after_the_last_copy(console.start(None, &Script::new()));
    assert_same_tree(&before, &console.whole(), "nothing changes");
}

/// Items 3 and 4: the data root's record says a migration committed; a
/// legacy directory that does not count changes nothing of that.
#[test]
fn install_ac_013_a_committed_migration_ignores_a_legacy_directory_that_does_not_count() {
    let console = committed(true);
    let dacl = format!("D:P(A;OICI;FA;;;{})(A;OICI;FA;;;BU)", sid());
    set_dacl(&console.paths.legacy(), &dacl);
    let before = console.whole();
    // Item 4: the start goes on as if it were absent, so a data root alone.
    assert_runs_after_the_last_copy(console.start(None, &Script::new()));
    assert_same_tree(&before, &console.whole(), "ignored and left");
}

/// Items 4 and 5: "one that does not [count] is not used, read or deleted",
/// by the old copies' pass either. A legacy directory others can change, or
/// a junction to the real copies, whose file IDs the record does hold: the
/// pass restricts nothing there and deletes nothing, and may report what it
/// likes.
#[test]
fn install_ac_013_copies_leave_a_legacy_directory_that_does_not_count() {
    fn assert_left(console: &Console, target: &Path) {
        let before = tree(target);
        let link = tree(&console.paths.legacy());
        let script = Script::new();
        let _ = console.copies(&script);
        let touched: Vec<Point> = script
            .seen()
            .into_iter()
            .filter(|point| {
                matches!(
                    point,
                    Point::RestrictLegacyDirectory | Point::Copy(_) | Point::RemoveLegacyDirectory
                )
            })
            .collect();
        assert_eq!(touched, [], "nothing of it is used");
        assert_same_tree(&before, &tree(target), "left");
        assert_same_tree(&link, &tree(&console.paths.legacy()), "left");
    }

    let console = committed(true);
    let dacl = format!("D:P(A;OICI;FA;;;{})(A;OICI;FA;;;BU)", sid());
    set_dacl(&console.paths.legacy(), &dacl);
    assert_left(&console, &console.paths.legacy());

    let console = committed(true);
    let target = console.outside("target");
    std::fs::rename(console.paths.legacy(), &target).expect("move the legacy directory");
    junction(&console.paths.legacy(), &target);
    assert_left(&console, &target);
}

/// Items 1 and 4: with the copies left, the recorded arguments are not
/// other arguments: it is setup's retry after a crash that came after the
/// commit, and the start runs, changing nothing here.
#[test]
fn install_ac_013_the_recorded_arguments_after_the_commit_run() {
    let console = committed(true);
    let before = console.whole();
    let script = Script::new();
    let request = console.request(Choice::Keep);
    assert_eq!(
        console.start(Some(&request), &script),
        Ok(Prepared::Committed)
    );
    assert_eq!(script.seen(), [], "no step ran");
    assert_same_tree(&before, &console.whole(), "nothing changes");
}

/// Item 1 after the commit: no migration is left to ask for.
#[test]
fn install_ac_013_arguments_after_the_last_copy_are_code_2() {
    let console = committed(false);
    let before = console.whole();
    arguments_refusal(console.start(Some(&console.request(Choice::Remove)), &Script::new()));
    assert_same_tree(&before, &console.whole(), "before any change");

    let console = committed(true);
    let before = console.whole();
    let other = Request {
        choice: Choice::Remove,
        fingerprints: BTreeSet::from([format!("sha256:{}", "c".repeat(64))]),
    };
    arguments_refusal(console.start(Some(&other), &Script::new()));
    assert_same_tree(&before, &console.whole(), "before any change");
}

/// Item 4: any other combination is code 5, which names the directories,
/// and nothing is changed; the read of item 4 comes before the data
/// directories' check, so an open data directory does not turn it into 13.
/// With arguments too: beside a console layout that is still code 5, no
/// step of the route having run, since item 2 is for the first bullet alone;
/// on a host that is not one, item 1's code 2 comes first or item 4's code
/// 5 does, as the implementation reads them, and nothing changes either way.
#[test]
fn install_ac_013_any_other_combination_is_code_5_and_changes_nothing() {
    let closed = test_custody().closed_directory_sddl;
    let product_root_alone = || {
        let console = Console::bare();
        console.counting_directory(&console.paths.product_root);
        console
    };
    let builds: Vec<Box<dyn Fn() -> Console + '_>> = vec![
        // A legacy directory beside a console layout, open or closed.
        Box::new(|| {
            let console = Console::closed();
            console.counting_directory(&console.paths.legacy());
            console
        }),
        Box::new(|| {
            let console = Console::new();
            console.counting_directory(&console.paths.legacy());
            console
        }),
        // A legacy directory beside a data root with no record.
        Box::new(|| {
            let console = Console::service_host(&closed);
            console.counting_directory(&console.paths.legacy());
            console
        }),
        // A legacy directory alone.
        Box::new(|| {
            let console = product_root_alone();
            console.counting_directory(&console.paths.legacy());
            console
        }),
        // A staging directory alone, with a record and without.
        Box::new(|| {
            let console = product_root_alone();
            console.counting_directory(&console.paths.staging());
            console.plant_record(&console.paths.staging(), Choice::Keep);
            console
        }),
        Box::new(|| {
            let console = product_root_alone();
            console.counting_directory(&console.paths.staging());
            console
        }),
        // A staging directory beside a data root with no console layout.
        Box::new(|| {
            let console = Console::service_host(&closed);
            console.counting_directory(&console.paths.staging());
            console.plant_record(&console.paths.staging(), Choice::Keep);
            console
        }),
        Box::new(|| {
            let console = Console::service_host(&closed);
            console.counting_directory(&console.paths.staging());
            console
        }),
        // All three.
        Box::new(|| {
            let console = Console::closed();
            console.counting_directory(&console.paths.staging());
            console.plant_record(&console.paths.staging(), Choice::Keep);
            console.counting_directory(&console.paths.legacy());
            console
        }),
        Box::new(|| {
            let console = Console::service_host(&closed);
            console.counting_directory(&console.paths.staging());
            console.plant_record(&console.paths.staging(), Choice::Keep);
            console.counting_directory(&console.paths.legacy());
            console.plant_record(&console.paths.data_root, Choice::Keep);
            console
        }),
        // A console layout that holds a record beside a legacy directory.
        Box::new(|| {
            let console = Console::closed();
            console.plant_record(&console.paths.data_root, Choice::Keep);
            console.counting_directory(&console.paths.legacy());
            console
        }),
        // Between the renames, with no record to complete from.
        Box::new(|| {
            let console = product_root_alone();
            console.counting_directory(&console.paths.staging());
            console.counting_directory(&console.paths.legacy());
            console
        }),
    ];
    for (index, build) in builds.iter().enumerate() {
        let console = build();
        let before = console.whole();
        let script = Script::new();
        let line = identity_refusal(console.start(None, &script));
        if console.paths.legacy().exists() {
            assert!(line.contains("agent.legacy"), "case {index}: {line}");
        }
        if console.paths.staging().exists() {
            assert!(line.contains("agent.migrating"), "case {index}: {line}");
        }
        assert_eq!(script.seen(), [], "case {index}");
        assert_same_tree(&before, &console.whole(), "nothing changes");

        let request = console.request(Choice::Keep);
        let script = Script::new();
        let (code, line) = refused(console.start(Some(&request), &script));
        if console.file("key.dat").exists() {
            assert_eq!(code, 5, "case {index}: {line}");
        } else {
            assert!(matches!(code, 2 | 5), "case {index}: {line}");
        }
        assert_eq!(script.seen(), [], "case {index}");
        assert_same_tree(&before, &console.whole(), "nothing changes");
    }
}

// --- Item 5: the old copies -------------------------------------------------

/// A console migrated with `choice`, with a leftover temporary among its
/// copies.
fn migrated_with_a_temporary(choice: Choice) -> Console {
    let console = Console::new();
    console.write(HYGIENE, b"a consumed token");
    console.migrate(choice);
    console
}

/// Item 5, `remove`: the legacy directory and each copy restricted, in no
/// order the item sets, each copy deleted, the directory once empty.
#[test]
fn install_ac_013_copies_with_remove_are_restricted_and_deleted() {
    let console = migrated_with_a_temporary(Choice::Remove);
    let script = Script::new();
    let pass = console.copies(&script).expect("the pass runs");
    for name in COPIES {
        assert!(pass.restricted.iter().any(|n| n == name), "{pass:?}");
        assert!(script.saw(Point::Copy(name)), "{name}");
    }
    assert_eq!(
        names_of(&pass.removed),
        COPIES.into_iter().collect::<BTreeSet<_>>()
    );
    assert_eq!(pass.not_reached, Vec::<String>::new());
    assert!(pass.directory_removed);
    assert!(!console.paths.legacy().exists());
    assert_eq!(console.record().copies_not_reached, Vec::<String>::new());

    assert!(script.saw(Point::RestrictLegacyDirectory));
    let seen = script.seen();
    let at = |point: Point| seen.iter().position(|p| *p == point).expect("a point");
    let removal = at(Point::RemoveLegacyDirectory);
    for name in COPIES {
        assert!(at(Point::Copy(name)) < removal, "once empty");
    }
}

/// Item 5, `keep`: each object restricted and left: SYSTEM alone for an
/// identity file and the legacy directory, SYSTEM and Administrators for
/// the others.
#[test]
fn install_ac_013_copies_with_keep_are_restricted_and_kept() {
    let console = migrated_with_a_temporary(Choice::Keep);
    let before = tree(&console.paths.legacy());
    let pass = console.copies(&Script::new()).expect("the pass runs");
    for name in COPIES {
        assert!(pass.restricted.iter().any(|n| n == name), "{pass:?}");
    }
    assert_eq!(pass.removed, Vec::<String>::new());
    assert_eq!(pass.not_reached, Vec::<String>::new());
    assert!(!pass.directory_removed);

    let custody = &console.custody;
    assert_eq!(
        security(&console.paths.legacy()),
        with_owner(&custody.identity_directory_sddl)
    );
    for name in IDENTITY_FILES {
        assert_eq!(
            security(&console.legacy(name)),
            with_owner(&custody.identity_file_sddl),
            "{name}"
        );
    }
    for name in ["agent.toml", "server-ca.pem", HYGIENE] {
        assert_eq!(
            security(&console.legacy(name)),
            with_owner(&custody.shared_file_sddl),
            "{name}"
        );
    }
    for name in COPIES {
        assert_eq!(
            std::fs::read(console.legacy(name)).ok().as_ref(),
            before[name].bytes.as_ref(),
            "{name}: its bytes"
        );
    }
}

/// Item 5: "always with `keep`": each start restricts the copies again.
#[test]
fn install_ac_013_copies_with_keep_are_restricted_at_every_start() {
    let console = migrated_with_a_temporary(Choice::Keep);
    console.copies(&Script::new()).expect("the first pass");
    let dacl = format!("D:P(A;;FA;;;{})(A;;FR;;;BU)", sid());
    set_dacl(&console.legacy("key.dat"), &dacl);
    let script = Script::new();
    let pass = console.copies(&script).expect("the next start's pass");
    assert!(script.saw(Point::Copy("key.dat")));
    assert!(pass.restricted.iter().any(|n| n == "key.dat"), "{pass:?}");
    assert_eq!(
        security(&console.legacy("key.dat")),
        with_owner(&console.custody.identity_file_sddl)
    );
}

/// Item 5: "until no copy is left with `remove`": a later start finds
/// nothing to do, and runs.
#[test]
fn install_ac_013_a_start_after_the_last_copy_has_nothing_to_do() {
    let console = committed(false);
    let script = Script::new();
    let pass = console.copies(&script).expect("nothing to do");
    assert_eq!(pass.restricted, Vec::<String>::new());
    assert_eq!(pass.removed, Vec::<String>::new());
    assert_eq!(pass.not_reached, Vec::<String>::new());
    assert!(!script.saw(Point::RestrictLegacyDirectory));
    assert_runs_after_the_last_copy(console.start(None, &Script::new()));
}

/// Items 5 and 3: a copy the service cannot reach does not stop the start:
/// it is logged as a warning, named in the record, rewritten with its
/// owner and DACL, and retried at the next start. The others go on.
#[test]
fn install_ac_013_a_copy_the_service_cannot_reach_is_reported_and_retried() {
    for choice in [Choice::Remove, Choice::Keep] {
        let console = migrated_with_a_temporary(choice);
        let record_security = security(&console.paths.record());
        let names = entries(&console.paths.data_root);
        let script = Script::new().fail(Point::Copy("key.dat"));
        let (pass, lines) = capture(|| console.copies(&script));
        let pass = pass.expect("the pass runs");
        assert_eq!(pass.not_reached, ["key.dat"], "{pass:?}");
        assert!(!pass.directory_removed, "{pass:?}");
        assert!(
            lines
                .iter()
                .any(|line| line.contains("\"WARN\"") && line.contains("key.dat")),
            "{lines:?}"
        );
        for name in COPIES.into_iter().filter(|name| *name != "key.dat") {
            assert!(script.saw(Point::Copy(name)), "{name}: the others go on");
        }
        assert!(console.legacy("key.dat").exists());

        let record = console.record();
        assert!(
            record
                .copies_not_reached
                .iter()
                .any(|c| c.contains("key.dat")),
            "{record:?}"
        );
        assert_eq!(
            security(&console.paths.record()),
            record_security,
            "the rewrite keeps its owner and DACL"
        );
        assert_eq!(
            entries(&console.paths.data_root),
            names,
            "the rewrite leaves no temporary"
        );
        assert_eq!(console.start(None, &Script::new()), Ok(Prepared::Committed));

        let pass = console.copies(&Script::new()).expect("the next start");
        assert_eq!(pass.not_reached, Vec::<String>::new(), "{pass:?}");
        assert_eq!(console.record().copies_not_reached, Vec::<String>::new());
        assert_eq!(pass.directory_removed, choice == Choice::Remove);
    }
}

/// Item 5: a copy with another link is neither restricted nor deleted; it
/// is reported, and reached once the link is gone.
#[test]
fn install_ac_013_a_copy_with_another_link_is_not_reached() {
    let console = migrated_with_a_temporary(Choice::Remove);
    let link = console.outside("another-name");
    std::fs::hard_link(console.legacy("key.dat"), &link).expect("link");
    let before = security(&console.legacy("key.dat"));
    let (pass, lines) = capture(|| console.copies(&Script::new()));
    let pass = pass.expect("the pass runs");
    assert_eq!(pass.not_reached, ["key.dat"], "{pass:?}");
    assert_reported(&console, &lines, "key.dat");
    assert!(console.legacy("key.dat").exists(), "not deleted");
    assert_eq!(
        security(&console.legacy("key.dat")),
        before,
        "not restricted"
    );
    assert!(!pass.directory_removed);

    std::fs::remove_file(&link).expect("the link is gone");
    let pass = console.copies(&Script::new()).expect("the next start");
    assert_eq!(pass.not_reached, Vec::<String>::new(), "{pass:?}");
    assert!(pass.directory_removed);
}

/// Item 5: a copy is matched by the file ID the record holds: a new file of
/// that name is not the copy, and is left alone.
#[test]
fn install_ac_013_a_copy_replaced_after_the_commit_is_not_touched() {
    let console = migrated_with_a_temporary(Choice::Remove);
    let copy = console.legacy("agent.toml");
    std::fs::remove_file(&copy).expect("remove");
    std::fs::write(&copy, b"a new file of that name").expect("write");
    let before = security(&copy);
    let (pass, lines) = capture(|| console.copies(&Script::new()));
    let pass = pass.expect("the pass runs");
    assert_eq!(pass.not_reached, ["agent.toml"], "{pass:?}");
    assert_reported(&console, &lines, "agent.toml");
    assert_eq!(
        std::fs::read(&copy).expect("kept"),
        b"a new file of that name"
    );
    assert_eq!(security(&copy), before, "not restricted");
    assert!(!pass.directory_removed);
}

/// Item 5: an object of the legacy directory the record does not hold has
/// no file ID to match: it is not reached, and the directory stays. A
/// junction is not followed.
#[test]
fn install_ac_013_an_object_the_record_does_not_hold_is_not_reached() {
    let console = migrated_with_a_temporary(Choice::Remove);
    std::fs::write(console.legacy("stray.txt"), b"not the agent's").expect("write");
    let target = console.outside("target");
    std::fs::create_dir(&target).expect("a target");
    std::fs::write(target.join("kept.txt"), b"kept").expect("write");
    let before = tree(&target);
    junction(&console.legacy("link"), &target);

    let (pass, lines) = capture(|| console.copies(&Script::new()));
    let pass = pass.expect("the pass runs");
    assert_eq!(
        names_of(&pass.not_reached),
        BTreeSet::from(["link", "stray.txt"]),
        "{pass:?}"
    );
    assert_reported(&console, &lines, "link");
    assert_reported(&console, &lines, "stray.txt");
    assert_eq!(
        std::fs::read(console.legacy("stray.txt")).expect("left"),
        b"not the agent's"
    );
    assert_same_tree(&before, &tree(&target), "not followed");
    assert!(!pass.directory_removed);
}

/// Item 5: a copy held open without sharing its deletion is a real copy the
/// service cannot delete: reported, and deleted at the next start.
#[test]
fn install_ac_013_a_copy_held_open_is_reported_and_deleted_later() {
    let console = migrated_with_a_temporary(Choice::Remove);
    let held = hold(&console.legacy("cert.pem"), ACCESS_READ, 0);
    let pass = console.copies(&Script::new()).expect("the pass runs");
    assert_eq!(pass.not_reached, ["cert.pem"], "{pass:?}");
    assert!(!pass.directory_removed);
    drop(held);
    assert!(console.legacy("cert.pem").exists());

    let pass = console.copies(&Script::new()).expect("the next start");
    assert_eq!(pass.removed, ["cert.pem"], "{pass:?}");
    assert!(pass.directory_removed);
}

/// Item 5: "gives it owner SYSTEM and a protected DACL ...; with `remove` it
/// then deletes it", in that order: `Copies` says what the pass declares,
/// so what shows it is a copy the deletion alone cannot reach. A reader that
/// does not share deletion is that, where an oplock is not: a read-handle
/// oplock makes the security change wait too. The copy ends restricted, for
/// a copy left behind is the case the order is there for.
#[test]
fn install_ac_013_a_copy_that_cannot_be_deleted_ends_restricted() {
    let console = migrated_with_a_temporary(Choice::Remove);
    let held = hold(
        &console.legacy("key.dat"),
        ACCESS_READ,
        SHARE_ALL & !FILE_SHARE_DELETE,
    );
    let pass = console.copies(&Script::new()).expect("the pass runs");
    drop(held);
    assert_eq!(pass.not_reached, ["key.dat"], "{pass:?}");
    assert!(console.legacy("key.dat").exists(), "not deleted");
    assert_eq!(
        security(&console.legacy("key.dat")),
        with_owner(&console.custody.identity_file_sddl),
        "restricted before the deletion"
    );
}

/// Item 5: the legacy directory that cannot be restricted or removed stops
/// no start, and the next pass finishes the work.
#[test]
fn install_ac_013_a_legacy_directory_the_service_cannot_reach_stops_no_start() {
    for point in [Point::RestrictLegacyDirectory, Point::RemoveLegacyDirectory] {
        let console = migrated_with_a_temporary(Choice::Remove);
        let script = Script::new().fail(point);
        let pass = console.copies(&script);
        assert!(
            !matches!(
                &pass,
                Ok(Copies {
                    directory_removed: true,
                    ..
                })
            ),
            "{point:?}: {pass:?}"
        );
        assert!(console.paths.legacy().exists(), "{point:?}");
        assert_eq!(console.start(None, &Script::new()), Ok(Prepared::Committed));

        let pass = console.copies(&Script::new()).expect("the next start");
        assert!(pass.directory_removed, "{point:?}: {pass:?}");
        assert!(!console.paths.legacy().exists(), "{point:?}");
    }
}

/// Item 5: a record that cannot be rewritten stops no start either; the
/// copies are retried, so the next pass settles the record.
#[test]
fn install_ac_013_a_record_that_cannot_be_rewritten_stops_no_start() {
    let console = migrated_with_a_temporary(Choice::Keep);
    let script = Script::new()
        .fail(Point::Copy("key.dat"))
        .fail(Point::RewriteRecord);
    let pass = console.copies(&script);
    assert!(
        pass.as_ref().map_or(true, |p| !p.not_reached.is_empty()),
        "{pass:?}"
    );
    assert_eq!(console.start(None, &Script::new()), Ok(Prepared::Committed));
    let pass = console.copies(&Script::new()).expect("the next start");
    assert_eq!(pass.not_reached, Vec::<String>::new());
    assert_eq!(console.record().copies_not_reached, Vec::<String>::new());
}

// --- Item 7: evidence and no secret -----------------------------------------

/// The fields of the pass's line of item 7: `agent_id`, the key's
/// fingerprint and the choice, as a word of its own in any ASCII case, so
/// that neither `Remove` from a `Debug` field nor `removed` decides it.
fn evidence(console: &Console, lines: &[String], choice: &str) -> String {
    lines
        .iter()
        .find_map(|line| {
            let line: serde_json::Value = serde_json::from_str(line).ok()?;
            let fields = line.get("fields")?.to_string();
            (fields.contains(&console.pki.agent_id)
                && fields.contains(&console.key_fingerprint())
                && names(&fields.to_lowercase(), choice))
            .then_some(fields)
        })
        .unwrap_or_else(|| panic!("no line of item 7: {lines:?}"))
}

/// Item 7: after a migration the service's file log says so, with
/// `agent_id`, the key's fingerprint, the choice and the copies' result: a
/// pass that does not reach a copy reports it, and its line is not the line
/// of a clean pass.
#[test]
fn install_ac_013_the_file_log_says_what_the_migration_did() {
    for (choice, word) in [(Choice::Remove, "remove"), (Choice::Keep, "keep")] {
        let console = Console::new();
        console.migrate(choice);
        let script = Script::new().fail(Point::Copy("key.dat"));
        let (pass, lines) = capture(|| console.copies(&script));
        pass.expect("the pass runs");
        let partial = evidence(&console, &lines, word);
        assert_reported(&console, &lines, "key.dat");

        let (pass, lines) = capture(|| console.copies(&Script::new()));
        pass.expect("the next pass runs");
        let clean = evidence(&console, &lines, word);
        assert_ne!(clean, partial, "the line carries the copies' result");
    }
}

/// Item 7 and the amendment's preamble: no log line and no refusal carries
/// a token, a SID, a PEM or the key's bytes, in any encoding, through a
/// refusal of each kind, a commit and a pass that does not reach a copy.
#[test]
fn install_ac_013_no_line_carries_a_secret_or_a_sid() {
    let console = Console::new();
    let sealed = std::fs::read(console.file("key.dat")).expect("read");
    let seed = console.pki.agent_seed;
    let hex = |bytes: &[u8]| -> String { bytes.iter().map(|b| format!("{b:02x}")).collect() };
    let standard = base64::engine::general_purpose::STANDARD;
    let url = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let certificate_body: String = console
        .pki
        .client_cert_pem
        .lines()
        .nth(1)
        .expect("a PEM body")
        .to_string();
    let secrets = [
        hex(&sealed),
        hex(&sealed[..24]),
        standard.encode(&sealed),
        hex(&seed),
        standard.encode(seed),
        url.encode(seed),
        format!("{:?}", &seed[..]),
        format!("{:?}", &sealed[..16]),
        certificate_body,
        sid().to_string(),
    ];

    let (committed, lines) = capture(|| {
        let request = console.request(Choice::Remove);
        let config = console.config_text.clone();
        // A refusal that could quote the file.
        console.write(
            "agent.toml",
            format!("{config}\nthis is not toml = = \"{TOKEN}\"\n").as_bytes(),
        );
        assert_eq!(refused(console.start(Some(&request), &Script::new())).0, 2);
        console.write("agent.toml", config.as_bytes());
        // A refusal that could quote the identity.
        let identity = std::fs::read(console.file("identity.json")).expect("read");
        console.write("identity.json", b"{\"agent_id\": ");
        assert_eq!(refused(console.start(Some(&request), &Script::new())).0, 5);
        console.write("identity.json", &identity);
        // Refusals with a file named, restricted or held.
        let held = hold(&console.file("key.dat"), ACCESS_WRITE, 7);
        assert_eq!(refused(console.start(Some(&request), &Script::new())).0, 5);
        drop(held);
        let script = Script::new().fail(Point::Restrict("key.dat"));
        assert_eq!(refused(console.start(Some(&request), &script)).0, 5);
        // The commit, and a pass that reports a copy.
        assert_eq!(
            console.start(Some(&request), &Script::new()),
            Ok(Prepared::Committed)
        );
        let committed = console.whole();
        let script = Script::new().fail(Point::Copy("key.dat"));
        console.copies(&script).expect("the pass runs");
        console.copies(&Script::new()).expect("the next pass");
        committed
    });
    assert!(!lines.is_empty(), "the migration logged");
    for line in &lines {
        assert_clean(line);
        for secret in &secrets {
            assert!(!line.contains(secret.as_str()), "{line}");
        }
    }
    assert_eq!(
        console.record().enrolling_account.as_deref(),
        Some(ENROLLING_SID),
        "the SID is recorded in migration.json"
    );
    for (name, object) in committed {
        if !name.ends_with("migration.json") {
            let bytes = object.bytes.unwrap_or_default();
            assert!(
                !bytes
                    .windows(ENROLLING_SID.len())
                    .any(|window| window == ENROLLING_SID.as_bytes()),
                "the SID is in migration.json alone: {name}"
            );
        }
    }
}

// --- The Amendment 2026-11-07: the client certificate's PEM ------------------

/// The Amendment 2026-11-07 as item 2.3 meets it: a `cert.pem` whose first
/// block is good and whose later section TLS refuses fails the consistency
/// check: code 5, before any file changes.
#[test]
fn install_ac_013_a_later_section_tls_refuses_is_code_5() {
    for later in [
        "-----BEGIN CERTIFICATE-----\nnot base64 !!\n-----END CERTIFICATE-----\n",
        "-----BEGIN CERTIFICATE-----\nAAAA\n",
        "-----BEGIN PRIVATE KEY-----\nnot base64 !!\n-----END PRIVATE KEY-----\n",
    ] {
        let console = Console::new();
        let document = format!("{}{later}", console.pki.client_cert_pem);
        console.write("cert.pem", document.as_bytes());
        assert!(
            CertificateDer::pem_slice_iter(document.as_bytes())
                .collect::<Result<Vec<_>, _>>()
                .is_err(),
            "TLS refuses it: {later}"
        );
        let first = CertificateDer::pem_slice_iter(document.as_bytes())
            .next()
            .expect("a first block")
            .expect("a first block TLS reads");
        assert!(
            cg_agent::x509::certificate_identity_der(first.as_ref()).is_ok(),
            "its first block is good"
        );

        let before = console.files();
        let request = console.request(Choice::Remove);
        let script = Script::new();
        let line = identity_refusal(console.start(Some(&request), &script));
        assert_in_place_line(&line, &DIRECTORIES);
        assert!(script.restrictions().is_empty(), "{line}");
        console.assert_console_in_place(&before, &[]);
    }
}

/// The Amendment 2026-11-07: the document is read as TLS reads it, skipping
/// sections of another kind: one certificate among them is the certificate
/// of item 2.3, wherever it stands, and its bytes are staged as read.
#[test]
fn install_ac_013_a_section_of_another_kind_is_skipped() {
    for (before_it, after_it) in [
        (
            "",
            "-----BEGIN X509 CRL-----\nAAAA\n-----END X509 CRL-----\n",
        ),
        (
            "-----BEGIN UNKNOWN KIND-----\nAAAA\n-----END UNKNOWN KIND-----\n",
            "",
        ),
        ("Bag Attributes\n    friendlyName: agent\n", ""),
    ] {
        let console = Console::new();
        let document = format!("{before_it}{}{after_it}", console.pki.client_cert_pem);
        console.write("cert.pem", document.as_bytes());
        assert_eq!(
            CertificateDer::pem_slice_iter(document.as_bytes())
                .collect::<Result<Vec<_>, _>>()
                .expect("TLS accepts it")
                .len(),
            1
        );
        console.migrate(Choice::Keep);
        assert_eq!(
            std::fs::read(&console.paths.cert).expect("read"),
            document.as_bytes()
        );
    }
}

// --- The amendment's preamble: operations an oplock makes wait --------------

fn environment<'a>(
    paths: cg_agent::service::InstalledPaths,
    custody: cg_agent::service::migration::MigrationCustody,
    watch: &'a Script,
) -> Environment<'a> {
    Environment {
        paths,
        custody,
        now: chrono::Utc::now(),
        wait: WAIT,
        watch,
    }
}

/// `prepare` on a thread of its own, so that a wait nobody cancels fails
/// the test instead of hanging it.
fn timed_start(
    console: &Console,
    request: Option<Request>,
    script: Arc<Script>,
) -> (Result<Prepared, Stop>, Duration) {
    let (paths, custody) = (console.paths.clone(), console.custody.clone());
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let began = Instant::now();
        let result = prepare(
            &environment(paths, custody, &script),
            request.as_ref(),
            &mut || Ok(()),
        );
        let _ = sender.send((result, began.elapsed()));
    });
    receiver
        .recv_timeout(WAIT * 20)
        .expect("the start still waits: the operation was not cancelled")
}

/// `copies` on a thread of its own.
fn timed_copies(console: &Console) -> (Result<Copies, String>, Duration) {
    let (paths, custody) = (console.paths.clone(), console.custody.clone());
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let script = Script::new();
        let began = Instant::now();
        let result = copies(&environment(paths, custody, &script));
        let _ = sender.send((result, began.elapsed()));
    });
    receiver
        .recv_timeout(WAIT * 20)
        .expect("the pass still waits: the operation was not cancelled")
}

/// The operation waited `WAIT`, and no longer than the wait explains.
fn assert_waited(elapsed: Duration) {
    assert!(elapsed >= WAIT - Duration::from_millis(50), "{elapsed:?}");
    assert!(elapsed < WAIT * 10, "{elapsed:?}");
}

/// What a hook takes: an oplock, or the code Windows refused it with. The
/// value lets go of it when dropped, before the `Console` it is declared
/// after, so that a test that fails while an operation still waits on the
/// oplock does not leave the deletion of the temporary root waiting too.
#[derive(Default)]
struct Kept(Arc<Mutex<Option<Result<Oplock, u32>>>>);

impl Kept {
    /// A hook that takes an oplock of `level` on `path`, immediately before
    /// the operation of its point.
    fn hook(&self, path: PathBuf, level: u32) -> impl FnMut() + Send + 'static {
        let kept = Arc::clone(&self.0);
        move || {
            *kept.lock().expect("kept") = Some(oplock(&path, level));
        }
    }

    fn take(&self) -> Option<Result<Oplock, u32>> {
        self.0.lock().expect("kept").take()
    }
}

impl Drop for Kept {
    fn drop(&mut self) {
        if let Ok(mut kept) = self.0.lock() {
            kept.take();
        }
    }
}

/// The preamble: an open that Windows makes wait on another process is
/// cancelled after `Environment.wait` and fails as one the object's sharing
/// refuses: item 2.3's code 5, the file named.
#[test]
fn install_ac_013_an_open_an_oplock_makes_wait_is_cancelled() {
    for name in FILES {
        let console = Console::new();
        let before = console.files();
        let held = oplock(&console.file(name), OPLOCK_RWH).expect("an oplock on NTFS");
        let script = Arc::new(Script::new());
        let request = console.request(Choice::Remove);
        let (result, elapsed) = timed_start(&console, Some(request), Arc::clone(&script));
        drop(held);
        let line = identity_refusal(result);
        assert_waited(elapsed);
        assert!(line.contains(name), "{line}");
        assert_in_place_line(&line, &DIRECTORIES);
        assert!(script.restrictions().is_empty(), "{line}");
        console.assert_console_in_place(&before, &[]);
    }
}

/// The preamble: a security change that Windows makes wait is cancelled
/// and fails as one the sharing refuses: item 2.4's code 5, the file named.
/// A handle of this start's own on the file rules a read-write-handle
/// oplock out; then nothing waits, and the route goes on.
#[test]
fn install_ac_013_a_security_change_an_oplock_makes_wait_is_cancelled() {
    let console = Console::new();
    let before = console.files();
    let kept = Kept::default();
    let hook = kept.hook(console.file("key.dat"), OPLOCK_RWH);
    let script = Arc::new(Script::new().hook(Point::Restrict("key.dat"), hook));
    let request = console.request(Choice::Remove);
    let (result, elapsed) = timed_start(&console, Some(request), Arc::clone(&script));
    let held = kept.take();
    match held {
        Some(Err(code)) => {
            assert_eq!(code, 300, "ERROR_OPLOCK_NOT_GRANTED");
            assert_eq!(result, Ok(Prepared::Committed));
        }
        _ => {
            drop(held);
            let line = identity_refusal(result);
            assert_waited(elapsed);
            assert!(line.contains("key.dat"), "{line}");
            assert!(line.contains(IN_PLACE), "{line}");
            assert!(!script.saw(Point::CreateStaging), "{line}");
            assert_eq!(
                console.files()["key.dat"].security,
                before["key.dat"].security
            );
        }
    }
}

/// The preamble: a rename that Windows makes wait is cancelled and fails as
/// one the sharing refuses. The first rename, waiting on an oplock inside
/// the data root: item 2.6's code 5, the console in place. The second:
/// item 2.6's reversal.
#[test]
fn install_ac_013_a_rename_an_oplock_makes_wait_is_cancelled() {
    let console = Console::new();
    let before = console.files();
    let kept = Kept::default();
    let hook = kept.hook(console.file("agent.toml"), OPLOCK_RH);
    let script = Arc::new(Script::new().hook(Point::FirstRename, hook));
    let request = console.request(Choice::Remove);
    let (result, elapsed) = timed_start(&console, Some(request), Arc::clone(&script));
    let held = kept.take();
    let line = identity_refusal(result);
    assert!(matches!(held, Some(Ok(_))), "an RH oplock is taken");
    drop(held);
    assert_waited(elapsed);
    assert_in_place_line(&line, &RESTRICTED);
    assert!(!script.saw(Point::SecondRename), "{line}");
    console.assert_console_in_place(&before, &FILES);

    let console = Console::new();
    let before = console.files();
    let kept = Kept::default();
    let hook = kept.hook(console.staged("agent.toml"), OPLOCK_RH);
    let script = Arc::new(Script::new().hook(Point::SecondRename, hook));
    let request = console.request(Choice::Remove);
    let (result, elapsed) = timed_start(&console, Some(request), Arc::clone(&script));
    let held = kept.take();
    let line = identity_refusal(result);
    assert!(matches!(held, Some(Ok(_))), "an RH oplock is taken");
    drop(held);
    assert_waited(elapsed);
    assert_in_place_line(&line, &RESTRICTED);
    assert!(script.saw(Point::RenameBack), "{line}");
    console.assert_console_in_place(&before, &FILES);
}

/// The preamble, on the reversal: a rename back that Windows makes wait is
/// cancelled and fails: without that the start would hang in
/// `START_PENDING`. The line is item 2.6's failed rename back, and the next
/// start puts the data root back.
#[test]
fn install_ac_013_a_rename_back_an_oplock_makes_wait_is_cancelled() {
    let console = Console::new();
    let before = console.files();
    let kept = Kept::default();
    let hook = kept.hook(console.legacy("agent.toml"), OPLOCK_RH);
    let script = Arc::new(
        Script::new()
            .fail(Point::SecondRename)
            .hook(Point::RenameBack, hook),
    );
    let request = console.request(Choice::Remove);
    let (result, elapsed) = timed_start(&console, Some(request), Arc::clone(&script));
    let held = kept.take();
    let line = identity_refusal(result);
    assert!(matches!(held, Some(Ok(_))), "an RH oplock is taken");
    drop(held);
    assert_waited(elapsed);
    assert!(line.contains("agent.legacy"), "{line}");
    assert!(!line.contains(IN_PLACE), "{line}");
    assert!(console.staged_record().reversal_pending, "the mark stays");

    identity_refusal(console.start(None, &Script::new()));
    console.assert_console_in_place(&before, &FILES);
}

/// The preamble, on item 4's completion: a rename that Windows makes wait
/// is cancelled, code 5 naming both directories, and the next start
/// completes the commit.
#[test]
fn install_ac_013_a_completion_an_oplock_makes_wait_is_cancelled() {
    let (console, _) = between_the_renames(Choice::Keep);
    let kept = Kept::default();
    let hook = kept.hook(console.staged("agent.toml"), OPLOCK_RH);
    let script = Arc::new(Script::new().hook(Point::CompleteCommit, hook));
    let (result, elapsed) = timed_start(&console, None, Arc::clone(&script));
    let held = kept.take();
    let line = identity_refusal(result);
    assert!(matches!(held, Some(Ok(_))), "an RH oplock is taken");
    drop(held);
    assert_waited(elapsed);
    assert!(line.contains("agent.migrating"), "{line}");
    assert!(line.contains("agent.legacy"), "{line}");

    assert_eq!(console.start(None, &Script::new()), Ok(Prepared::Committed));
    console.assert_committed();
}

/// The preamble, on item 4's deletion of an interrupted creation: a deletion
/// that Windows makes wait is cancelled, a refusal before the commit that
/// names the directory, and the next start deletes it and migrates.
#[test]
fn install_ac_013_a_deletion_of_the_interrupted_creation_an_oplock_makes_wait_is_cancelled() {
    let console = Console::new();
    let request = console.request(Choice::Remove);
    console.crash_at(Point::Stage("server-ca.pem"), &request);
    std::fs::remove_file(console.staged("migration.json")).expect("remove");
    let kept = Kept::default();
    let hook = kept.hook(console.staged("agent.toml"), OPLOCK_RH);
    let script = Arc::new(Script::new().hook(Point::DeleteInterruptedStaging, hook));
    let (result, elapsed) = timed_start(&console, Some(request.clone()), Arc::clone(&script));
    let held = kept.take();
    let line = identity_refusal(result);
    assert!(matches!(held, Some(Ok(_))), "an RH oplock is taken");
    drop(held);
    assert_waited(elapsed);
    assert!(line.contains("agent.migrating"), "{line}");
    assert_in_place_line(&line, &RESTRICTED);

    assert_eq!(
        console.start(Some(&request), &Script::new()),
        Ok(Prepared::Committed)
    );
    console.assert_committed();
}

/// The preamble, on item 5: a deletion that Windows makes wait, and a
/// security change, are cancelled: the copy is not reached, the start is
/// not stopped, and the next pass reaches it.
#[test]
fn install_ac_013_a_deletion_an_oplock_makes_wait_is_cancelled() {
    for level in [OPLOCK_RH, OPLOCK_RWH] {
        let console = migrated_with_a_temporary(Choice::Remove);
        let held = oplock(&console.legacy("key.dat"), level).expect("an oplock on NTFS");
        let (pass, elapsed) = timed_copies(&console);
        drop(held);
        let pass = pass.expect("the pass runs");
        assert_waited(elapsed);
        assert_eq!(pass.not_reached, ["key.dat"], "{pass:?}");
        assert!(console.legacy("key.dat").exists(), "not deleted");
        assert!(!pass.directory_removed);

        let pass = console.copies(&Script::new()).expect("the next start");
        assert!(pass.directory_removed, "{pass:?}");
    }
}
