//! The service's purge (SPEC-024 Amendment 2026-11-15, points 4 to 6,
//! install_ac_012): unelevated over temporary roots, with the current user in
//! SYSTEM's place (`migration_fixtures`). The start argument; every name of
//! point 5 removed in point 6's order on a fresh, a pending, an enrolled and a
//! migrated host; a failure and a crash injected at each step, each leaving a
//! state the purge and a plain start go on from; what it leaves and names; its
//! refusals before any removal; its time bound; its lines and entries.
//!
//! An interruption is `Stop::Crashed`; the purge run again is the same command
//! run again (point 6).
#![cfg(windows)]

mod common;
mod custody_fixtures;
mod migration_fixtures;

use std::os::windows::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cg_agent::custody::{self, Policy, Principal};
use cg_agent::service::layout::{classify, HostState};
use cg_agent::service::migration::{parse_start, prepare, Choice, Point, Prepared, Start, Stop};
use cg_agent::service::purge::{self, Area, ABNORMAL, BOUND};
use cg_agent::service::{
    ServiceExit, EVENT_FAILED, EVENT_STOPPED, EXIT_PURGE_LEFT, EXIT_PURGE_REFUSED, EXIT_SERVICE_LOG,
};

use custody_fixtures::{default_owner_sid, sid_bytes};
use migration_fixtures::{
    assert_clean, assert_same_tree, batch_oplock, hold, in_ci, junction, set_dacl, set_owner, sid,
    test_custody, Console, Script, ACCESS_READ, SHARE_ALL, TOKEN, WAIT,
};

const DIRECTORIES_LINE: &str = "cg-agent: the purge's directories fail their check: ";
const REFUSED_LINE: &str = "cg-agent: the purge refuses this host: ";
const LEFT_LINE: &str = "cg-agent: the purge left ";
const ARGUMENTS_LINE: &str =
    "cg-agent: invalid purge arguments: --purge is given with another argument";

/// Point 5's names, each list in point 6's order.
const FIRST: [&str; 3] = ["agent.toml.setup", "agent.toml", "agent.toml.hygiene-tmp"];
const LEGACY_NAMES: [&str; 6] = [
    "agent.toml",
    "server-ca.pem",
    "cert.pem",
    "key.dat",
    "identity.json",
    "agent.toml.hygiene-tmp",
];
const IDENTITY_NAMES: [&str; 3] = ["cert.pem", "key.dat", "identity.json"];
const REST: [&str; 5] = [
    "server-ca.pem",
    "server-ca.pem.setup",
    "setup-attempt.json",
    "migration.json",
    "migration.json.rewrite-tmp",
];
const LOG_NAMES: [&str; 7] = [
    "agent.jsonl",
    "agent.1.jsonl",
    "agent.2.jsonl",
    "agent.3.jsonl",
    "agent.4.jsonl",
    "agent.5.jsonl",
    "agent.rotating.jsonl",
];

fn closed() -> String {
    test_custody().closed_directory_sddl
}

fn present(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

fn file(console: &Console, path: &Path, bytes: &[u8]) {
    custody::create_new_file(path, &console.custody.shared_file_sddl, bytes)
        .unwrap_or_else(|e| panic!("create {}: {e}", path.display()));
}

fn directory(path: &Path, sddl: &str) {
    let any = Policy {
        owners: vec![],
        allowed: vec![],
        protected: false,
    };
    custody::ensure_directory(path, sddl, &any)
        .unwrap_or_else(|e| panic!("create {}: {e}", path.display()));
}

fn logs(console: &Console) -> PathBuf {
    console.paths.data_root.join("logs")
}

/// `<P>` and the data root as the service leaves them, empty.
fn empty_host() -> Console {
    let console = Console::bare();
    console.make_directories(&closed(), &closed());
    console
}

/// A host the setup staged on and never adopted: neither `agent.toml` nor
/// `identity\`.
fn fresh() -> Console {
    let console = empty_host();
    let root = &console.paths.data_root;
    for name in [
        "agent.toml.setup",
        "server-ca.pem.setup",
        "setup-attempt.json",
    ] {
        file(&console, &root.join(name), b"staged");
    }
    console
}

/// `agent.toml` with a token, its anchor and a log.
fn pending() -> Console {
    let console = empty_host();
    file(
        &console,
        &console.paths.config,
        format!("[enrollment]\ntoken = \"{TOKEN}\"\n").as_bytes(),
    );
    file(&console, &console.paths.trust_anchor, b"anchor");
    directory(&logs(&console), &closed());
    file(&console, &logs(&console).join("agent.jsonl"), b"{}\n");
    console
}

fn add_identity(console: &Console) {
    let custody = &console.custody;
    custody::ensure_directory(
        &console.paths.identity_dir,
        &custody.identity_directory_sddl,
        &custody.identity,
    )
    .expect("identity\\");
    for name in IDENTITY_NAMES {
        custody::create_new_file(
            &console.paths.identity_dir.join(name),
            &custody.identity_file_sddl,
            b"identity",
        )
        .expect("an identity file");
    }
}

/// A pending host that enrolled.
fn enrolled() -> Console {
    let console = pending();
    add_identity(&console);
    file(&console, &logs(&console).join("agent.1.jsonl"), b"{}\n");
    console
}

/// Every name of point 5: a migrated host whose copies are kept, with every
/// file the setup, the migration and the log write.
fn full() -> Console {
    let console = enrolled();
    let root = &console.paths.data_root;
    console.plant_record(root, Choice::Keep);
    for name in FIRST.iter().chain(REST.iter()) {
        if !present(&root.join(name)) {
            file(&console, &root.join(name), b"x");
        }
    }
    for name in LOG_NAMES {
        if !present(&logs(&console).join(name)) {
            file(&console, &logs(&console).join(name), b"{}\n");
        }
    }
    console.counting_directory(&console.paths.legacy());
    for name in LEGACY_NAMES {
        file(&console, &console.legacy(name), b"legacy");
    }
    console
}

/// A console installation migrated by the service, its copies kept or
/// removed, and a log.
fn migrated(choice: Choice) -> Console {
    let console = Console::new();
    console.migrate(choice);
    console.copies(&Script::new()).expect("the copies' pass");
    directory(&logs(&console), &closed());
    file(&console, &logs(&console).join("agent.jsonl"), b"{}\n");
    console
}

fn migrated_keep() -> Console {
    migrated(Choice::Keep)
}

fn migrated_remove() -> Console {
    migrated(Choice::Remove)
}

/// The purge, its lock free, taken once and before anything else.
fn purge_within(console: &Console, script: &Script, bound: Duration) -> Result<(), Stop> {
    let mut calls = 0;
    let result = purge::run(&console.environment(script), bound, &mut || {
        calls += 1;
        assert!(script.seen().is_empty(), "the lock is taken first");
        Ok(())
    });
    assert_eq!(calls, 1, "the lock is taken once");
    result
}

fn purge(console: &Console, script: &Script) -> Result<(), Stop> {
    purge_within(console, script, BOUND)
}

fn refusal(result: Result<(), Stop>) -> (u8, String) {
    match result {
        Err(Stop::Refused { code, line }) => {
            assert_clean(&line);
            (code, line)
        }
        other => panic!("a refusal was expected: {other:?}"),
    }
}

fn left(result: Result<(), Stop>) -> String {
    let (code, line) = refusal(result);
    assert_eq!(code, EXIT_PURGE_LEFT, "{line}");
    assert!(line.starts_with(LEFT_LINE), "{line}");
    line
}

fn names(line: &str, path: &Path) -> bool {
    line.contains(&path.display().to_string())
}

fn assert_gone(console: &Console, what: &str) {
    assert!(
        !present(&console.paths.product_root),
        "{what}: <P> is left: {:?}",
        migration_fixtures::tree(console.dir.path()).keys()
    );
}

/// The service's check of `<P>` and the data root, under the test custody.
fn directories(console: &Console) -> Policy {
    Policy {
        owners: console.custody.owners.clone(),
        allowed: console.custody.configuration.allowed.clone(),
        protected: false,
    }
}

/// What a host held before the purge.
#[derive(Clone, Copy)]
struct Had {
    config: bool,
    identity: bool,
    record: bool,
}

impl Had {
    fn of(console: &Console) -> Self {
        Had {
            config: present(&console.paths.config),
            identity: present(&console.paths.identity_dir),
            record: present(&console.paths.record()),
        }
    }
}

/// Point 6: what an interrupted purge leaves is a state point 3.2 goes on
/// from, a plain start reads without code 5, and no configuration stands
/// without its identity, nor a data root without its record beside the
/// legacy directory.
fn assert_goes_on(console: &Console, had: &Had, what: &str) {
    let Had {
        config: had_config,
        identity: had_identity,
        record: had_record,
    } = *had;
    let paths = &console.paths;
    if present(&paths.config) && had_identity {
        assert!(present(&paths.identity_dir), "{what}: identity\\ is gone");
        for name in IDENTITY_NAMES {
            assert!(
                present(&paths.identity_dir.join(name)),
                "{what}: {name} is gone while agent.toml is there"
            );
        }
    }
    if had_config && !present(&paths.config) {
        assert!(
            !present(&paths.staged_config()),
            "{what}: agent.toml.setup outlives agent.toml"
        );
    }
    if present(&paths.legacy()) {
        if had_identity {
            assert!(
                present(&paths.identity_dir),
                "{what}: legacy without identity\\"
            );
        }
        if had_record {
            assert!(
                present(&paths.record()),
                "{what}: legacy without its record"
            );
        }
    }
    let state = classify(paths, &directories(console), WAIT);
    assert!(
        matches!(
            state,
            HostState::Enrolled | HostState::Pending | HostState::Fresh
        ),
        "{what}: {state:?}"
    );
    let started = prepare(&console.environment(&Script::new()), None, &mut || Ok(()));
    assert!(
        matches!(started, Ok(Prepared::Plain | Prepared::Committed)),
        "{what}: a plain start reads {started:?}"
    );
}

// The start argument (point 4).

#[test]
fn install_ac_012_purge_alone_is_the_purge_start() {
    assert_eq!(parse_start(&["--purge".to_string()]), Ok(Start::Purge));
}

#[test]
fn install_ac_012_purge_with_another_argument_is_code_2() {
    let fingerprint = format!("--trust-anchor-sha256=sha256:{}", "a".repeat(64));
    for arguments in [
        vec!["--purge", "--purge"],
        vec!["--purge", "--adopt-staged-config"],
        vec!["--adopt-staged-config", "--purge"],
        vec!["--migrate-legacy=keep", "--purge"],
        vec!["--purge", fingerprint.as_str()],
        vec!["--purge", "--other"],
    ] {
        let arguments: Vec<String> = arguments.iter().map(|a| a.to_string()).collect();
        assert_eq!(
            parse_start(&arguments),
            Err(ARGUMENTS_LINE.to_string()),
            "{arguments:?}"
        );
    }
}

// What it removes, in order (points 5 and 6).

fn full_order() -> Vec<Point> {
    let mut points = vec![
        Point::PurgeHold(Area::ProductRoot),
        Point::PurgeHold(Area::DataRoot),
        Point::PurgeState,
    ];
    points.extend(
        FIRST
            .iter()
            .map(|name| Point::PurgeFile(Area::DataRoot, name)),
    );
    points.push(Point::PurgeHold(Area::Legacy));
    points.extend(
        LEGACY_NAMES
            .iter()
            .map(|name| Point::PurgeFile(Area::Legacy, name)),
    );
    points.push(Point::PurgeDirectory(Area::Legacy));
    points.push(Point::PurgeHold(Area::Identity));
    points.extend(
        IDENTITY_NAMES
            .iter()
            .map(|name| Point::PurgeFile(Area::Identity, name)),
    );
    points.push(Point::PurgeDirectory(Area::Identity));
    points.extend(
        REST.iter()
            .map(|name| Point::PurgeFile(Area::DataRoot, name)),
    );
    points.push(Point::PurgeHold(Area::Logs));
    points.extend(
        LOG_NAMES
            .iter()
            .map(|name| Point::PurgeFile(Area::Logs, name)),
    );
    points.push(Point::PurgeDirectory(Area::Logs));
    points.push(Point::PurgeDirectory(Area::DataRoot));
    points.push(Point::PurgeDirectory(Area::ProductRoot));
    points
}

#[test]
fn install_ac_012_every_name_is_removed_in_point_6_s_order() {
    let console = full();
    let script = Script::new();
    assert_eq!(purge(&console, &script), Ok(()));
    assert_eq!(script.seen(), full_order());
    assert_gone(&console, "full");
}

#[test]
fn install_ac_012_names_are_compared_but_for_ascii_case() {
    let console = empty_host();
    file(&console, &console.paths.data_root.join("AGENT.TOML"), b"x");
    file(
        &console,
        &console.paths.data_root.join("Server-CA.pem"),
        b"x",
    );
    assert_eq!(purge(&console, &Script::new()), Ok(()));
    assert_gone(&console, "case");
}

#[test]
fn install_ac_012_each_kind_of_host_is_purged() {
    for (what, make) in [
        ("fresh", fresh as fn() -> Console),
        ("pending", pending),
        ("enrolled", enrolled),
        ("migrated, keep", migrated_keep),
        ("migrated, remove", migrated_remove),
    ] {
        let console = make();
        assert_eq!(purge(&console, &Script::new()), Ok(()), "{what}");
        assert_gone(&console, what);
        assert!(present(console.dir.path()), "{what}: %ProgramData% stays");
    }
}

#[test]
fn install_ac_012_an_empty_tree_is_code_0() {
    let bare = Console::bare();
    assert_eq!(purge(&bare, &Script::new()), Ok(()));
    assert!(!present(&bare.paths.product_root), "nothing is created");
    let empty = empty_host();
    assert_eq!(purge(&empty, &Script::new()), Ok(()));
    assert_gone(&empty, "empty");
}

// A failure and a crash at each step (point 6).

fn interrupted_at_each_point(make: fn() -> Console, what: &str) {
    let probe = make();
    let script = Script::new();
    assert_eq!(purge(&probe, &script), Ok(()), "{what}");
    let points = script.seen();
    assert!(points.len() > 3, "{what}: {points:?}");
    for point in points {
        for crash in [true, false] {
            let console = make();
            let had = Had::of(&console);
            let before = console.whole();
            let what = format!(
                "{what}, {} at {point:?}",
                if crash { "crash" } else { "failure" }
            );
            let script = if crash {
                Script::new().crash(point)
            } else {
                Script::new().fail(point)
            };
            let result = purge(&console, &script);
            if crash {
                assert_eq!(result, Err(Stop::Crashed), "{what}");
                assert_eq!(script.seen().last(), Some(&point), "{what}");
            } else {
                let (code, line) = refusal(result);
                let expected = match point {
                    Point::PurgeHold(Area::ProductRoot | Area::DataRoot) => EXIT_SERVICE_LOG,
                    Point::PurgeState => EXIT_PURGE_REFUSED,
                    _ => EXIT_PURGE_LEFT,
                };
                assert_eq!(code, expected, "{what}: {line}");
                if code != EXIT_PURGE_LEFT {
                    assert_same_tree(&before, &console.whole(), &what);
                }
            }
            assert_goes_on(&console, &had, &what);
            assert_eq!(purge(&console, &Script::new()), Ok(()), "{what}: run again");
            assert_gone(&console, &what);
        }
    }
}

#[test]
fn install_ac_012_a_fresh_host_interrupted_at_each_step_goes_on() {
    interrupted_at_each_point(fresh, "fresh");
}

#[test]
fn install_ac_012_a_pending_host_interrupted_at_each_step_goes_on() {
    interrupted_at_each_point(pending, "pending");
}

#[test]
fn install_ac_012_an_enrolled_host_interrupted_at_each_step_goes_on() {
    interrupted_at_each_point(enrolled, "enrolled");
}

#[test]
fn install_ac_012_a_migrated_host_kept_interrupted_at_each_step_goes_on() {
    interrupted_at_each_point(migrated_keep, "migrated, keep");
}

#[test]
fn install_ac_012_a_migrated_host_removed_interrupted_at_each_step_goes_on() {
    interrupted_at_each_point(migrated_remove, "migrated, remove");
}

#[test]
fn install_ac_012_every_name_interrupted_at_each_step_goes_on() {
    interrupted_at_each_point(full, "full");
}

// What it leaves at the first three steps (point 6).

#[test]
fn install_ac_012_agent_toml_left_ends_the_purge_at_its_step() {
    let console = enrolled();
    let _reader = hold(&console.paths.config, ACCESS_READ, 0x1);
    let line = left(purge(&console, &Script::new()));
    assert!(names(&line, &console.paths.config), "{line}");
    assert!(present(&console.paths.trust_anchor), "nothing after it");
    assert!(present(&console.paths.identity), "the identity stays");
    assert!(
        present(&logs(&console).join("agent.jsonl")),
        "the log stays"
    );
}

#[test]
fn install_ac_012_a_legacy_file_left_ends_the_purge_at_its_step() {
    let console = full();
    let held = console.legacy("cert.pem");
    let _reader = hold(&held, ACCESS_READ, 0x1);
    let line = left(purge(&console, &Script::new()));
    assert!(names(&line, &held), "{line}");
    assert!(!present(&console.paths.config), "the first step ran");
    assert!(
        !present(&console.legacy("agent.toml")),
        "its agent.toml first"
    );
    assert!(present(&console.paths.identity), "the identity stays");
    assert!(present(&console.paths.record()), "the record stays");
}

#[test]
fn install_ac_012_an_identity_file_left_ends_the_purge_at_its_step() {
    let console = enrolled();
    let _reader = hold(&console.paths.key, ACCESS_READ, 0x1);
    let line = left(purge(&console, &Script::new()));
    assert!(names(&line, &console.paths.key), "{line}");
    assert!(present(&console.paths.identity_dir), "identity\\ stays");
    assert!(present(&console.paths.trust_anchor), "the rest stays");
}

// What it leaves and names (point 5).

#[test]
fn install_ac_012_a_file_in_use_is_left_and_named_and_the_rest_goes_on() {
    let console = enrolled();
    let reader = hold(&console.paths.trust_anchor, ACCESS_READ, 0x1);
    let line = left(purge(&console, &Script::new()));
    assert!(names(&line, &console.paths.trust_anchor), "{line}");
    assert!(line.contains("in use"), "{line}");
    assert!(!present(&console.paths.identity_dir), "identity\\ removed");
    assert!(!present(&logs(&console)), "logs\\ removed");
    assert!(present(&console.paths.data_root), "the data root keeps it");
    drop(reader);
    assert_eq!(purge(&console, &Script::new()), Ok(()), "run again");
    assert_gone(&console, "in use");
}

#[test]
fn install_ac_012_an_unknown_name_and_directory_are_left_and_named() {
    let console = enrolled();
    let unknown = console.paths.data_root.join("notes.txt");
    let other = console.paths.data_root.join("other");
    let in_logs = logs(&console).join("agent.6.jsonl");
    std::fs::write(&unknown, b"an administrator's").expect("notes.txt");
    std::fs::create_dir(&other).expect("other");
    std::fs::write(&in_logs, b"{}").expect("agent.6.jsonl");
    let line = left(purge(&console, &Script::new()));
    for path in [&unknown, &other, &in_logs] {
        assert!(
            names(&line, path),
            "{} is not named: {line}",
            path.display()
        );
        assert!(present(path), "{} is left", path.display());
    }
    assert!(!present(&console.paths.config), "agent.toml removed");
    assert!(!present(&console.paths.identity_dir), "identity\\ removed");
    assert_eq!(
        std::fs::read(&unknown).expect("read notes.txt"),
        b"an administrator's",
        "never opened for writing"
    );
}

#[test]
fn install_ac_012_a_file_with_a_second_link_is_left_and_named() {
    let console = enrolled();
    let outside = console.outside("linked.pem");
    std::fs::hard_link(&console.paths.trust_anchor, &outside).expect("a hard link");
    let line = left(purge(&console, &Script::new()));
    assert!(names(&line, &console.paths.trust_anchor), "{line}");
    assert!(present(&console.paths.trust_anchor), "left");
    assert_eq!(std::fs::read(&outside).expect("read the link"), b"anchor");
}

/// The owner the custody does not accept: this needs a token with two
/// owners to give, an elevated one, as in rust-ci's Windows job.
#[test]
fn install_ac_012_a_file_of_another_owner_is_left_and_named() {
    let owner = default_owner_sid();
    if owner == sid() {
        assert!(!in_ci(), "this job's token has one owner to give");
        eprintln!("an unelevated token has one owner to give");
        return;
    }
    let mut console = enrolled();
    console.custody.owners = vec![Principal::Sid(sid_bytes(&owner))];
    set_owner(&console.paths.trust_anchor, sid());
    let line = left(purge(&console, &Script::new()));
    assert!(names(&line, &console.paths.trust_anchor), "{line}");
    assert!(present(&console.paths.trust_anchor), "left");
    assert!(!present(&console.paths.identity_dir), "the rest goes on");
}

fn canary(console: &Console) -> PathBuf {
    let target = console.outside("target");
    std::fs::create_dir(&target).expect("the target");
    std::fs::write(target.join("canary"), b"canary").expect("the canary");
    target
}

fn assert_canary(target: &Path, what: &str) {
    assert_eq!(
        std::fs::read(target.join("canary")).ok().as_deref(),
        Some(b"canary".as_slice()),
        "{what}: the canary"
    );
}

#[test]
fn install_ac_012_a_junction_as_p_or_the_data_root_is_code_13() {
    let console = Console::bare();
    let target = canary(&console);
    junction(&console.paths.product_root, &target);
    let (code, line) = refusal(purge(&console, &Script::new()));
    assert_eq!(code, EXIT_SERVICE_LOG, "{line}");
    assert!(line.starts_with(DIRECTORIES_LINE), "{line}");
    assert!(names(&line, &console.paths.product_root), "{line}");
    assert_canary(&target, "<P>");

    let console = Console::bare();
    let target = canary(&console);
    directory(&console.paths.product_root, &closed());
    junction(&console.paths.data_root, &target);
    let (code, line) = refusal(purge(&console, &Script::new()));
    assert_eq!(code, EXIT_SERVICE_LOG, "{line}");
    assert!(line.starts_with(DIRECTORIES_LINE), "{line}");
    assert!(names(&line, &console.paths.data_root), "{line}");
    assert_canary(&target, "the data root");
}

/// A junction in place of `identity\`, `logs\` or the legacy directory is
/// left and named, its target intact; removed, the purge run again ends.
#[test]
fn install_ac_012_a_junction_as_a_directory_is_left_and_named() {
    for (what, make, place) in [
        ("identity", pending as fn() -> Console, "agent\\identity"),
        ("logs", pending, "agent\\logs"),
        ("legacy", enrolled, "agent.legacy"),
    ] {
        let console = make();
        let link = console.paths.product_root.join(place);
        if present(&link) {
            std::fs::remove_dir_all(&link).expect("make room");
        }
        let target = canary(&console);
        junction(&link, &target);
        let line = left(purge(&console, &Script::new()));
        assert!(names(&line, &link), "{what}: {line}");
        assert!(present(&link), "{what}: the junction is left");
        assert_canary(&target, what);
        if what == "legacy" {
            assert!(present(&console.paths.identity), "the identity stays");
        }
        std::fs::remove_dir(&link).expect("remove the junction");
        assert_eq!(purge(&console, &Script::new()), Ok(()), "{what}: run again");
        assert_gone(&console, what);
        assert_canary(&target, what);
    }
}

/// A junction planted as an entry of each directory, a product name or not.
#[test]
fn install_ac_012_a_junction_as_an_entry_is_left_and_named() {
    for (what, make, entry) in [
        ("<P>", enrolled as fn() -> Console, "planted"),
        (
            "data root, a product name",
            enrolled,
            "agent\\server-ca.pem",
        ),
        ("data root, another name", enrolled, "agent\\planted"),
        ("identity", enrolled, "agent\\identity\\cert.pem"),
        ("logs", enrolled, "agent\\logs\\agent.jsonl"),
        ("legacy", full, "agent.legacy\\agent.toml"),
    ] {
        let console = make();
        let link = console.paths.product_root.join(entry);
        if present(&link) {
            std::fs::remove_file(&link).expect("make room");
        }
        let target = canary(&console);
        junction(&link, &target);
        let line = left(purge(&console, &Script::new()));
        assert!(names(&line, &link), "{what}: {line}");
        assert!(present(&link), "{what}: left");
        assert_canary(&target, what);
    }
}

#[test]
fn install_ac_012_an_entry_that_appears_before_a_directory_s_removal_is_left() {
    let console = enrolled();
    let late = logs(&console).join("late.txt");
    let created = late.clone();
    let script = Script::new().hook(Point::PurgeDirectory(Area::Logs), move || {
        std::fs::write(&created, b"late").expect("a late entry");
    });
    let line = left(purge(&console, &script));
    assert!(names(&line, &late), "{line}");
    assert!(present(&late), "left");
}

#[test]
fn install_ac_012_p_and_the_data_root_are_held_sharing_no_deletion() {
    let console = enrolled();
    let outcomes: Arc<Mutex<Vec<Option<i32>>>> = Arc::default();
    let seen = Arc::clone(&outcomes);
    let paths = console.paths.clone();
    let script = Script::new().hook(Point::PurgeFile(Area::DataRoot, "agent.toml"), move || {
        for path in [&paths.product_root, &paths.data_root] {
            let opened = std::fs::OpenOptions::new()
                .access_mode(0x0001_0000)
                .share_mode(SHARE_ALL)
                .custom_flags(0x0200_0000)
                .open(path);
            seen.lock()
                .expect("outcomes")
                .push(opened.err().and_then(|e| e.raw_os_error()));
        }
    });
    assert_eq!(purge(&console, &script), Ok(()));
    assert_eq!(
        *outcomes.lock().expect("outcomes"),
        vec![Some(32), Some(32)],
        "an open for deletion is a sharing violation"
    );
}

// Refusals before any removal (point 4).

#[test]
fn install_ac_012_a_refused_lock_is_its_code_with_nothing_changed() {
    let console = enrolled();
    let before = console.whole();
    let script = Script::new();
    let result = purge::run(&console.environment(&script), BOUND, &mut || {
        Err(Stop::Refused {
            code: 10,
            line: "held".to_string(),
        })
    });
    assert_eq!(
        result,
        Err(Stop::Refused {
            code: 10,
            line: "held".to_string()
        })
    );
    assert!(script.seen().is_empty(), "nothing after the lock");
    assert_same_tree(&before, &console.whole(), "a refused lock");
}

#[test]
fn install_ac_012_states_the_purge_refuses_are_code_14_with_nothing_changed() {
    let untrusted = empty_host();
    untrusted.counting_directory(&untrusted.paths.legacy());
    let in_progress = enrolled();
    in_progress.counting_directory(&in_progress.paths.staging());
    let console_layout = Console::closed();
    for (what, console) in [
        ("untrusted", untrusted),
        ("a migration in progress", in_progress),
        ("a console layout", console_layout),
    ] {
        let before = console.whole();
        let (code, line) = refusal(purge(&console, &Script::new()));
        assert_eq!(code, EXIT_PURGE_REFUSED, "{what}: {line}");
        assert!(line.starts_with(REFUSED_LINE), "{what}: {line}");
        assert_same_tree(&before, &console.whole(), what);
    }
}

#[test]
fn install_ac_012_a_console_layout_whose_directories_fail_is_code_13() {
    let console = Console::new();
    let before = console.whole();
    let (code, line) = refusal(purge(&console, &Script::new()));
    assert_eq!(code, EXIT_SERVICE_LOG, "{line}");
    assert!(line.starts_with(DIRECTORIES_LINE), "{line}");
    assert_same_tree(&before, &console.whole(), "13");
}

// The time bound, the lines and the entries (points 4 and 7).

#[test]
fn install_ac_012_no_removal_begins_after_the_bound() {
    let console = full();
    let before = console.whole();
    let line = left(purge_within(&console, &Script::new(), Duration::ZERO));
    assert!(line.contains("not reached"), "{line}");
    assert!(names(&line, &console.paths.staged_config()), "{line}");
    for directory in [&console.paths.data_root, &console.paths.product_root] {
        let not_reached = format!("{} was not reached", directory.display());
        assert!(line.contains(&not_reached), "{not_reached}: {line}");
    }
    assert_same_tree(&before, &console.whole(), "nothing removed");

    let console = enrolled();
    let script = Script::new().hook(Point::PurgeFile(Area::DataRoot, "agent.toml"), || {
        std::thread::sleep(Duration::from_millis(600));
    });
    let line = left(purge_within(&console, &script, Duration::from_millis(300)));
    assert!(!present(&console.paths.config), "begun before the bound");
    assert!(present(&console.paths.identity), "nothing begun after it");
    assert!(names(&line, &console.paths.identity_dir), "{line}");
}

#[test]
fn install_ac_012_the_line_names_at_most_20_names_then_their_number() {
    let console = empty_host();
    let extras: Vec<PathBuf> = (0..25)
        .map(|n| console.paths.data_root.join(format!("extra-{n:02}.txt")))
        .collect();
    for path in &extras {
        std::fs::write(path, b"x").expect("an extra");
    }
    let line = left(purge(&console, &Script::new()));
    let named = extras.iter().filter(|path| names(&line, path)).count();
    assert_eq!(named, 20, "{line}");
    assert!(line.contains("and 5 more"), "{line}");
}

#[test]
fn install_ac_012_no_line_quotes_a_file() {
    let console = enrolled();
    let _reader = hold(&console.paths.config, ACCESS_READ, 0x1);
    let line = left(purge(&console, &Script::new()));
    assert!(!line.contains(TOKEN), "{line}");
}

#[test]
fn install_ac_012_an_abnormal_end_is_code_15() {
    let console = enrolled();
    let script = Script::new().hook(Point::PurgeFile(Area::Identity, "key.dat"), || {
        panic!("an injected panic")
    });
    let result = purge::start(&console.environment(&script), BOUND, &mut || Ok(()));
    assert_eq!(
        result,
        Err(Stop::Refused {
            code: EXIT_PURGE_LEFT,
            line: ABNORMAL.to_string()
        })
    );
    let console = enrolled();
    let script = Script::new().crash(Point::PurgeFile(Area::Logs, "agent.jsonl"));
    let result = purge::start(&console.environment(&script), BOUND, &mut || Ok(()));
    assert_eq!(
        result,
        Err(Stop::Refused {
            code: EXIT_PURGE_LEFT,
            line: ABNORMAL.to_string()
        })
    );
}

#[test]
fn install_ac_012_the_entries_1001_and_1002() {
    let purged = ServiceExit::Purged;
    assert_eq!(purged.specific_code(), None, "STOPPED with code 0");
    let entry = purged.entry();
    assert_eq!(entry.id, EVENT_STOPPED);
    assert!(!entry.error);
    let failed = ServiceExit::Failed {
        code: EXIT_PURGE_LEFT,
        line: ABNORMAL.to_string(),
    };
    assert_eq!(failed.specific_code(), Some(15));
    let entry = failed.entry();
    assert_eq!(entry.id, EVENT_FAILED);
    assert!(
        entry.error && entry.text.contains(ABNORMAL),
        "{}",
        entry.text
    );
}

// Point 4's bounded opens and point 5's directory checks.

/// An open Windows makes wait on another process is cancelled after the
/// wait and fails as one the sharing refuses: the file is left, named in
/// use, and the rest goes on.
#[test]
fn install_ac_012_an_open_held_by_an_oplock_is_cancelled_and_left() {
    let console = enrolled();
    let _oplock = batch_oplock(&console.paths.trust_anchor).expect("a batch oplock");
    let started = std::time::Instant::now();
    let line = left(purge(&console, &Script::new()));
    assert!(names(&line, &console.paths.trust_anchor), "{line}");
    assert!(line.contains("in use"), "{line}");
    assert!(
        started.elapsed() < WAIT * 15,
        "bounded: {:?}",
        started.elapsed()
    );
    assert!(!present(&console.paths.identity_dir), "the rest goes on");
}

/// The legacy directory counts only under `agent.toml`'s policy: one whose
/// DACL allows another principal is left and ends the purge at its step.
#[test]
fn install_ac_012_a_legacy_directory_that_fails_its_policy_is_left() {
    let console = enrolled();
    directory(
        &console.paths.legacy(),
        &format!("D:P(A;OICI;FA;;;{})(A;OICI;0x1200a9;;;BU)", sid()),
    );
    let line = left(purge(&console, &Script::new()));
    assert!(names(&line, &console.paths.legacy()), "{line}");
    assert!(present(&console.paths.legacy()), "left");
    assert!(present(&console.paths.identity), "the identity stays");
}

/// `logs\` allowing anyone but SYSTEM and Administrators is left and named,
/// its files unopened.
#[test]
fn install_ac_012_a_directory_allowing_others_is_left() {
    let console = enrolled();
    set_dacl(
        &logs(&console),
        &format!("D:P(A;OICI;FA;;;{})(A;OICI;0x1200a9;;;BU)", sid()),
    );
    let line = left(purge(&console, &Script::new()));
    assert!(names(&line, &logs(&console)), "{line}");
    assert!(present(&logs(&console).join("agent.jsonl")), "unopened");
    assert!(
        !present(&console.paths.identity_dir),
        "the steps before ran"
    );
}

/// Past the first three steps the purge names everything it leaves: an
/// unknown name in the data root does not hide one in `<P>`.
#[test]
fn install_ac_012_names_left_in_the_data_root_and_in_p_are_all_named() {
    let console = enrolled();
    let in_data_root = console.paths.data_root.join("notes.txt");
    let in_product_root = console.paths.product_root.join("planted.txt");
    std::fs::write(&in_data_root, b"x").expect("notes.txt");
    std::fs::write(&in_product_root, b"x").expect("planted.txt");
    let line = left(purge(&console, &Script::new()));
    for path in [&in_data_root, &in_product_root] {
        assert!(
            names(&line, path),
            "{} is not named: {line}",
            path.display()
        );
    }
}
