//! The read-only classification of the host's state (SPEC-024 Amendment
//! 2026-11-09, item 3): a precondition of the package's criteria
//! (install_ac_001, install_ac_011), not those criteria. Unelevated over
//! temporary roots, with the current user in SYSTEM's place
//! (`migration_fixtures`): each state, the order in which the first that
//! applies wins, that a reparse point is refused without being followed, that
//! nothing is changed, and that nothing under the data root is opened.
#![cfg(windows)]

mod common;
mod custody_fixtures;
mod migration_fixtures;

use cg_agent::custody::Policy;
use cg_agent::service::layout::{classify, HostState};

use migration_fixtures::{
    batch_oplock, junction, programdata_child_sddl, set_dacl, sid, Console, WAIT,
};

/// The service's check of `<P>` and the data root under the test's custody:
/// owned by this user or their default owner, allowing this user and
/// Administrators (Amendment 2026-11-04, item 3).
fn directories(console: &Console) -> Policy {
    Policy {
        owners: console.custody.owners.clone(),
        allowed: console.custody.configuration.allowed.clone(),
        protected: false,
    }
}

/// `<P>` and the data root as item 3 creates them, and nothing in them.
fn empty() -> Console {
    let console = Console::bare();
    let sddl = console.custody.closed_directory_sddl.clone();
    console.make_directories(&sddl, &sddl);
    console
}

/// The directories of a console runbook's host: ordinary users create in them.
fn runbook_directories() -> Console {
    let console = Console::bare();
    let sddl = programdata_child_sddl();
    console.make_directories(&sddl, &sddl);
    console
}

fn touch(path: &std::path::Path) {
    std::fs::write(path, b"x").expect("a file");
}

fn directory(path: &std::path::Path) {
    std::fs::create_dir(path).expect("a directory");
}

/// The state, which leaves the whole tree as it was.
fn state(console: &Console) -> HostState {
    let before = console.whole();
    let state = classify(&console.paths, &directories(console), WAIT);
    migration_fixtures::assert_same_tree(&before, &console.whole(), "the classification");
    state
}

fn assert_untrusted(console: &Console, naming: &str) {
    match state(console) {
        HostState::Untrusted(reason) => {
            assert!(reason.contains(naming), "{naming}: {reason}");
            migration_fixtures::assert_clean(&reason);
        }
        other => panic!("{naming}: untrusted was expected: {other:?}"),
    }
}

// --- The last states: neither, pending, enrolled ----------------------------

#[test]
fn package_layout_a_host_with_nothing_is_fresh() {
    assert_eq!(state(&Console::bare()), HostState::Fresh);

    let console = Console::bare();
    let sddl = console.custody.closed_directory_sddl.clone();
    custody_ensure(&console.paths.product_root, &sddl);
    assert_eq!(state(&console), HostState::Fresh, "<P> alone");

    assert_eq!(state(&empty()), HostState::Fresh, "an empty data root");

    let console = empty();
    touch(&console.paths.data_root.join("server-ca.pem"));
    assert_eq!(
        state(&console),
        HostState::Fresh,
        "an anchor is not a configuration"
    );

    let console = empty();
    for name in [
        "agent.toml.setup",
        "server-ca.pem.setup",
        "setup-attempt.json",
    ] {
        touch(&console.paths.data_root.join(name));
    }
    assert_eq!(state(&console), HostState::Fresh, "what a setup left");
}

fn custody_ensure(path: &std::path::Path, sddl: &str) {
    let any = Policy {
        owners: vec![],
        allowed: vec![],
        protected: false,
    };
    cg_agent::custody::ensure_directory(path, sddl, &any).expect("a directory");
}

#[test]
fn package_layout_a_configuration_without_identity_is_pending() {
    let console = empty();
    touch(&console.paths.config);
    assert_eq!(state(&console), HostState::Pending);

    touch(&console.paths.trust_anchor);
    assert_eq!(state(&console), HostState::Pending, "with its anchor");

    touch(&console.paths.data_root.join("agent.toml.setup"));
    assert_eq!(state(&console), HostState::Pending, "with a staged file");

    let console = empty();
    touch(&console.paths.data_root.join("AGENT.TOML"));
    assert_eq!(state(&console), HostState::Pending, "in another case");
}

#[test]
fn package_layout_an_identity_name_is_enrolled() {
    let console = empty();
    directory(&console.paths.identity_dir);
    assert_eq!(state(&console), HostState::Enrolled);

    touch(&console.paths.config);
    assert_eq!(state(&console), HostState::Enrolled, "with a configuration");

    let console = empty();
    directory(&console.paths.data_root.join("IDENTITY"));
    assert_eq!(state(&console), HostState::Enrolled, "in another case");

    let console = empty();
    touch(&console.paths.identity_dir);
    assert_eq!(
        state(&console),
        HostState::Enrolled,
        "by name, whatever it is"
    );
}

/// Item 3: "an enrolled or preserved host, a kept legacy directory included".
#[test]
fn package_layout_a_kept_legacy_directory_with_an_identity_is_enrolled() {
    let console = empty();
    directory(&console.paths.identity_dir);
    directory(&console.paths.legacy());
    assert_eq!(state(&console), HostState::Enrolled);
}

/// Item 3: "by name without opening `identity\`": a directory this user has
/// no access to is still read as there.
#[test]
fn package_layout_identity_is_not_opened() {
    let console = empty();
    directory(&console.paths.identity_dir);
    let open = format!("D:P(A;OICI;FA;;;{})", sid());
    set_dacl(&console.paths.identity_dir, "D:P");
    let found = classify(&console.paths, &directories(&console), WAIT);
    set_dacl(&console.paths.identity_dir, &open);
    assert_eq!(found, HostState::Enrolled);
}

/// A name in the data root is read from its listing: an opportunistic lock on
/// it, which an open would wait on, costs nothing.
#[test]
fn package_layout_names_are_read_without_opening_them() {
    let console = empty();
    touch(&console.paths.config);
    let _held = batch_oplock(&console.paths.config).expect("an oplock on agent.toml");
    assert_eq!(
        classify(&console.paths, &directories(&console), WAIT),
        HostState::Pending
    );

    let console = Console::closed();
    let _held = batch_oplock(&console.file("cert.pem")).expect("an oplock on cert.pem");
    assert_eq!(
        classify(&console.paths, &directories(&console), WAIT),
        HostState::ConsoleLayout
    );
}

// --- Console layout ---------------------------------------------------------

/// The Amendment 2026-11-05: "a data root that holds `cert.pem`, `key.dat` or
/// `identity.json` directly".
#[test]
fn package_layout_any_identity_file_directly_is_a_console_layout() {
    for name in ["cert.pem", "key.dat", "identity.json", "CERT.PEM"] {
        let console = empty();
        touch(&console.paths.data_root.join(name));
        assert_eq!(state(&console), HostState::ConsoleLayout, "{name}");
    }
}

/// The first that applies: the check of the directories comes after, and a
/// console runbook's directories fail it.
#[test]
fn package_layout_a_console_layout_comes_before_the_directories_check() {
    let console = runbook_directories();
    touch(&console.paths.data_root.join("key.dat"));
    assert_eq!(state(&console), HostState::ConsoleLayout);

    assert_eq!(
        state(&Console::new()),
        HostState::ConsoleLayout,
        "a whole one"
    );
}

// --- Migration in progress --------------------------------------------------

#[test]
fn package_layout_a_staging_directory_is_a_migration_in_progress() {
    let console = empty();
    directory(&console.paths.staging());
    assert_eq!(state(&console), HostState::MigrationInProgress);

    let console = Console::new();
    directory(&console.paths.staging());
    assert_eq!(
        state(&console),
        HostState::MigrationInProgress,
        "before a console layout"
    );

    let console = runbook_directories();
    directory(&console.paths.product_root.join("AGENT.MIGRATING"));
    assert_eq!(
        state(&console),
        HostState::MigrationInProgress,
        "before the directories check, in another case"
    );
}

#[test]
fn package_layout_a_legacy_name_with_no_data_root_is_a_migration_in_progress() {
    let console = Console::bare();
    let sddl = console.custody.closed_directory_sddl.clone();
    custody_ensure(&console.paths.product_root, &sddl);
    directory(&console.paths.legacy());
    assert_eq!(state(&console), HostState::MigrationInProgress);

    // Item 3 reads this one by name, whatever it is.
    let console = Console::bare();
    custody_ensure(&console.paths.product_root, &sddl);
    touch(&console.paths.legacy());
    assert_eq!(state(&console), HostState::MigrationInProgress, "a file");
}

/// Item 3 reads "a directory `agent.migrating`": another object of that name
/// is not one, and the next states apply.
#[test]
fn package_layout_a_staging_name_that_is_not_a_directory_is_no_migration() {
    let console = empty();
    touch(&console.paths.staging());
    assert_eq!(state(&console), HostState::Fresh, "a file");

    let console = empty();
    let target = console.outside("target");
    directory(&target);
    junction(&console.paths.staging(), &target);
    assert_eq!(state(&console), HostState::Fresh, "a junction");
}

// --- Untrusted --------------------------------------------------------------

/// Item 3: a reparse point is refused first, without being followed: what
/// lies behind it is not read.
#[test]
fn package_layout_a_reparse_point_is_refused_first_and_not_followed() {
    let behind = Console::new();
    let console = Console::bare();
    junction(&console.paths.product_root, &behind.paths.product_root);
    assert_untrusted(&console, "CyberGuard");

    let behind = Console::new();
    let console = Console::bare();
    let sddl = console.custody.closed_directory_sddl.clone();
    custody_ensure(&console.paths.product_root, &sddl);
    junction(&console.paths.data_root, &behind.paths.data_root);
    assert_untrusted(&console, "agent");

    // Before a staging directory, which would otherwise come first.
    let target = tempfile::tempdir().expect("a target");
    let console = Console::bare();
    custody_ensure(&console.paths.product_root, &sddl);
    junction(&console.paths.data_root, target.path());
    directory(&console.paths.staging());
    assert_untrusted(&console, "agent");
}

#[test]
fn package_layout_a_data_directory_that_is_no_directory_is_untrusted() {
    let console = Console::bare();
    touch(&console.paths.product_root);
    assert_untrusted(&console, "CyberGuard");

    let console = Console::bare();
    let sddl = console.custody.closed_directory_sddl.clone();
    custody_ensure(&console.paths.product_root, &sddl);
    touch(&console.paths.data_root);
    assert_untrusted(&console, "agent");
}

/// The Amendment 2026-11-04, item 3's check: each of the two directories.
#[test]
fn package_layout_a_directory_failing_the_services_check_is_untrusted() {
    let console = Console::bare();
    let closed = console.custody.closed_directory_sddl.clone();
    console.make_directories(&programdata_child_sddl(), &closed);
    assert_untrusted(&console, "CyberGuard");

    let console = Console::bare();
    console.make_directories(&closed, &programdata_child_sddl());
    assert_untrusted(&console, "agent");

    // Before the last states: this host would otherwise be pending.
    let console = runbook_directories();
    touch(&console.paths.config);
    assert_untrusted(&console, "CyberGuard");

    let console = runbook_directories();
    directory(&console.paths.identity_dir);
    assert_untrusted(&console, "CyberGuard");
}

/// Item 3: "`agent.legacy` exists with a data root and no `identity\`".
#[test]
fn package_layout_a_legacy_directory_beside_a_data_root_without_identity_is_untrusted() {
    let console = empty();
    directory(&console.paths.legacy());
    assert_untrusted(&console, "agent.legacy");

    let console = empty();
    directory(&console.paths.legacy());
    touch(&console.paths.config);
    assert_untrusted(&console, "agent.legacy");

    // The two earlier states come first.
    let console = empty();
    directory(&console.paths.legacy());
    directory(&console.paths.staging());
    assert_eq!(state(&console), HostState::MigrationInProgress);

    let console = empty();
    directory(&console.paths.legacy());
    touch(&console.paths.data_root.join("cert.pem"));
    assert_eq!(state(&console), HostState::ConsoleLayout);
}

/// A listing that cannot be read is no state: nothing can be said of the host.
#[test]
fn package_layout_directories_that_cannot_be_listed_are_untrusted() {
    let console = empty();
    let open = console.custody.closed_directory_sddl.clone();
    set_dacl(&console.paths.product_root, "D:P");
    let found = classify(&console.paths, &directories(&console), WAIT);
    set_dacl(&console.paths.product_root, &open);
    assert!(matches!(found, HostState::Untrusted(_)), "{found:?}");

    let console = empty();
    set_dacl(&console.paths.data_root, "D:P");
    let found = classify(&console.paths, &directories(&console), WAIT);
    set_dacl(&console.paths.data_root, &open);
    assert!(matches!(found, HostState::Untrusted(_)), "{found:?}");

    // The first that applies: a migration is read from <P>'s listing alone.
    let console = empty();
    directory(&console.paths.staging());
    set_dacl(&console.paths.data_root, "D:P");
    let found = classify(&console.paths, &directories(&console), WAIT);
    set_dacl(&console.paths.data_root, &open);
    assert_eq!(found, HostState::MigrationInProgress);
}
