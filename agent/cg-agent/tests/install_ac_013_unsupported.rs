//! SPEC-024 Amendment 2026-11-20, points 2 and 3, unelevated over temporary
//! roots: the service's start argument `--migrate-legacy=` is code 2 before
//! any change; a start without arguments beside a staging or legacy directory
//! that counts is code 5 before any change, its line naming the layout,
//! completing, reversing or continuing no migration and deleting no old copy;
//! and no line of a start, the adoption's or the purge's offers the migration.
//!
//! The lines are spelled from the amendment's words, not from the code:
//! point 3's "neither adopts nor migrates", "not supported" and the
//! installation runbook. The migration's own code, which the tests of
//! install_ac_013 call directly, builds the interrupted states.
#![cfg(windows)]

mod common;
mod custody_fixtures;
mod migration_fixtures;

use std::path::{Path, PathBuf};

use cg_agent::custody;
use cg_agent::service::migration::{
    parse_start, prepare_start, Choice, Point, Prepared, Start, Stop, DEFAULT_REFUSAL,
};

use migration_fixtures::{
    assert_clean, identity_refusal, test_custody, Console, Script, ARGUMENTS_LINE,
};

/// Point 3's words for a layout the product does not support.
const NEITHER: &str = "neither adopts nor migrates";
/// Where each refusal sends the operator (point 3).
const RUNBOOK: &str = "installation runbook";

/// What no refusal line says any longer (point 3; Session 85's round 12,
/// list B, item 13).
const OFFERS: [&str; 8] = [
    "migrate them",
    "start the migration",
    "arguments it recorded",
    "the migration route",
    "CG_MIGRATE_LEGACY",
    "CG_TRUST_ANCHORS",
    "finish or reverse",
    "without --migrate-legacy",
];

fn assert_offers_no_migration(line: &str) {
    for offer in OFFERS {
        assert!(
            !line.contains(offer),
            "offers the migration ({offer}): {line}"
        );
    }
}

fn arguments(list: &[&str]) -> Vec<String> {
    list.iter().map(|argument| argument.to_string()).collect()
}

/// Point 2: `--migrate-legacy=`, whatever its value and whatever is beside
/// it but `--purge`, is code 2 with a line that names it, says that the
/// migration is not supported and sends to the runbook, and repeats no value.
#[test]
fn install_ac_013_unsupported_migrate_legacy_is_code_2() {
    let console = Console::new();
    let fingerprint = console.fingerprint();
    let anchor = format!("--trust-anchor-sha256={fingerprint}");
    for list in [
        vec!["--migrate-legacy=keep", anchor.as_str()],
        vec!["--migrate-legacy=remove", anchor.as_str()],
        vec!["--migrate-legacy=keep"],
        vec!["--migrate-legacy=s86-not-a-choice"],
        vec![anchor.as_str(), "--migrate-legacy=remove"],
        vec![
            "--adopt-staged-config",
            anchor.as_str(),
            "--migrate-legacy=keep",
        ],
    ] {
        let line = parse_start(&arguments(&list)).expect_err("--migrate-legacy is refused");
        assert!(line.starts_with(ARGUMENTS_LINE), "{list:?}: {line}");
        assert!(line.contains("--migrate-legacy"), "{list:?}: {line}");
        assert!(line.contains("not supported"), "{list:?}: {line}");
        assert!(line.contains(RUNBOOK), "{list:?}: {line}");
        for value in ["keep", "remove", "s86-not-a-choice", fingerprint.as_str()] {
            assert!(
                !line.contains(&format!("={value}")) && !line.contains(fingerprint.as_str()),
                "{list:?} repeats {value}: {line}"
            );
        }
        assert_offers_no_migration(&line);
        assert_clean(&line);
    }
}

/// Point 2: a `--trust-anchor-sha256=` without the adoption's argument stays
/// code 2, and its line no longer names the migration's argument.
#[test]
fn install_ac_013_unsupported_a_fingerprint_alone_is_code_2() {
    let console = Console::new();
    let anchor = format!("--trust-anchor-sha256={}", console.fingerprint());
    for list in [vec![anchor.as_str()], vec![anchor.as_str(), "--s86-other"]] {
        let line = parse_start(&arguments(&list)).expect_err("refused");
        assert!(line.starts_with(ARGUMENTS_LINE), "{list:?}: {line}");
        assert!(line.contains("--adopt-staged-config"), "{list:?}: {line}");
        assert!(!line.contains("--migrate-legacy"), "{list:?}: {line}");
        assert_offers_no_migration(&line);
    }
    // The adoption's and the purge's arguments are unchanged.
    assert_eq!(parse_start(&[]), Ok(Start::Plain));
    assert_eq!(parse_start(&arguments(&["--purge"])), Ok(Start::Purge));
    assert!(matches!(
        parse_start(&arguments(&["--adopt-staged-config", anchor.as_str()])),
        Ok(Start::Adopt(_))
    ));
}

/// Point 3: the reason a refusal on a console layout adds names the layout,
/// says that the product neither adopts nor migrates it, and sends to the
/// runbook.
#[test]
fn install_ac_013_unsupported_the_console_reason() {
    assert!(
        DEFAULT_REFUSAL.contains("console installation"),
        "{DEFAULT_REFUSAL}"
    );
    assert!(DEFAULT_REFUSAL.contains(NEITHER), "{DEFAULT_REFUSAL}");
    assert!(DEFAULT_REFUSAL.contains(RUNBOOK), "{DEFAULT_REFUSAL}");
    assert_offers_no_migration(DEFAULT_REFUSAL);
}

/// `<P>` alone, closed, with no data root.
fn product_root_alone() -> Console {
    let console = Console::bare();
    custody::ensure_directory(
        &console.paths.product_root,
        &test_custody().closed_directory_sddl,
        &console.custody.configuration,
    )
    .expect("<P> alone");
    console
}

/// A host on which a start without arguments finds a staging or legacy
/// directory that counts, and the paths its line must name.
struct Counting {
    what: &'static str,
    console: Console,
    named: Vec<PathBuf>,
}

fn counting_hosts() -> Vec<Counting> {
    let closed = test_custody().closed_directory_sddl;
    let mut hosts = Vec::new();
    let mut add = |what, console: Console, staging: bool, legacy: bool| {
        let mut named = Vec::new();
        if staging {
            named.push(console.paths.staging());
        }
        if legacy {
            named.push(console.paths.legacy());
        }
        hosts.push(Counting {
            what,
            console,
            named,
        });
    };

    // The migration's own states, made by its code.
    let console = Console::new();
    let request = console.request(Choice::Keep);
    console.crash_at(Point::SecondRename, &request);
    add("between the renames", console, true, true);

    let console = Console::new();
    let request = console.request(Choice::Remove);
    let script = Script::new()
        .fail(Point::SecondRename)
        .fail(Point::RenameBack);
    assert!(console.start(Some(&request), &script).is_err());
    assert!(console.staged_record().reversal_pending);
    add("a reversal pending", console, true, true);

    let console = Console::new();
    let request = console.request(Choice::Keep);
    console.crash_at(Point::Stage("identity.json"), &request);
    add(
        "a staging directory beside a console layout",
        console,
        true,
        false,
    );

    let console = Console::new();
    console.migrate(Choice::Keep);
    add(
        "a committed migration's legacy directory",
        console,
        false,
        true,
    );

    // What an administrator, SYSTEM or a moved-in object makes on an
    // endpoint the setup installed (point 2).
    let console = Console::closed();
    console.counting_directory(&console.paths.staging());
    add(
        "a staging directory without a record beside a console layout",
        console,
        true,
        false,
    );

    let console = Console::service_host(&closed);
    console.counting_directory(&console.paths.legacy());
    add(
        "a legacy directory beside an enrolled host's identity",
        console,
        false,
        true,
    );

    let console = Console::service_host(&closed);
    console.counting_directory(&console.paths.staging());
    console.plant_record(&console.paths.staging(), Choice::Keep);
    add(
        "a staging directory beside an enrolled host's identity",
        console,
        true,
        false,
    );

    let console = product_root_alone();
    console.counting_directory(&console.paths.legacy());
    add("a legacy directory alone", console, false, true);

    let console = product_root_alone();
    console.counting_directory(&console.paths.staging());
    add("a staging directory alone", console, true, false);

    hosts
}

fn display(path: &Path) -> String {
    path.display().to_string()
}

/// Point 2: a start without arguments beside a staging or legacy directory
/// that counts is code 5 before any change: nothing is completed, reversed,
/// continued or deleted, no operation of the migration is reached, and the
/// line names the layout, says that the product neither adopts nor migrates
/// it and sends to the runbook.
#[test]
fn install_ac_013_unsupported_a_counting_directory_is_code_5_before_any_change() {
    for Counting {
        what,
        console,
        named,
    } in counting_hosts()
    {
        let before = console.whole();
        let script = Script::new();
        let line = identity_refusal(prepare_start(&console.environment(&script)));
        for path in &named {
            assert!(
                line.contains(&display(path)),
                "{what}: names {path:?}: {line}"
            );
        }
        assert!(line.contains(NEITHER), "{what}: {line}");
        assert!(line.contains("nothing was changed"), "{what}: {line}");
        assert!(line.contains(RUNBOOK), "{what}: {line}");
        assert_offers_no_migration(&line);
        assert!(
            script.seen().is_empty(),
            "{what}: an operation was reached: {:?}",
            script.seen()
        );
        migration_fixtures::assert_same_tree(&before, &console.whole(), what);
    }
}

/// The start goes on as before where nothing counts: a console layout under
/// closed directories, and an enrolled host.
#[test]
fn install_ac_013_unsupported_nothing_counting_starts_as_before() {
    let console = Console::closed();
    assert_eq!(
        prepare_start(&console.environment(&Script::new())),
        Ok(Prepared::ConsoleLayout)
    );
    let console = Console::service_host(&test_custody().closed_directory_sddl);
    assert_eq!(
        prepare_start(&console.environment(&Script::new())),
        Ok(Prepared::Plain)
    );
    // A console layout under directories that fail their check is 13, its
    // reason adding the layout's (point 3).
    let console = Console::new();
    match prepare_start(&console.environment(&Script::new())) {
        Err(Stop::Refused { code: 13, line }) => {
            assert!(line.contains(NEITHER), "{line}");
            assert_offers_no_migration(&line);
        }
        other => panic!("a console layout under open directories: {other:?}"),
    }
}
