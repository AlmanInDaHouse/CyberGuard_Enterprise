//! `cg-setup`'s verbs (SPEC-024 Amendment 2026-11-10, item 9), after the
//! platform and the elevation, on temporary roots, the current user standing
//! for SYSTEM and Administrators, and a product service that does not exist:
//! preflight's order up to the staging and the stop, configure's checks in
//! order before anything it starts, and the reversal's start from the record
//! alone. What a real service adds is in the SCM tests (`setup_service`).
#![cfg(windows)]

mod fixtures;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use cg_setup::codes;
use cg_setup::preflight::Request;
use cg_setup::record::{Created, Record};
use cg_setup::scm::{self, Manager, Product, Route};
use cg_setup::staging::{self, read_record, Name, Setup, SetupCustody, Step, Watch};
use cg_setup::verbs::{self, Machine};
use cg_setup::version::{numeric, SETUP_VERSION};
use cg_setup::Refusal;

use fixtures::{
    bootstrap_text, certificate, closed_directory_sddl, private_file_sddl, test_policy,
    versioned_copy, Root, TOKEN, WAIT,
};

// --- Helpers -----------------------------------------------------------------------

fn custody() -> SetupCustody {
    SetupCustody {
        directories: test_policy(false),
        staged: test_policy(true),
        directory_sddl: closed_directory_sddl(),
        file_sddl: private_file_sddl(),
    }
}

fn quiet(_: Step) -> Result<(), String> {
    Ok(())
}

/// A watch that records each step.
struct Recorder(Mutex<Vec<Step>>);

impl Recorder {
    fn new() -> Self {
        Recorder(Mutex::new(Vec::new()))
    }

    fn steps(&self) -> Vec<Step> {
        self.0.lock().unwrap().clone()
    }
}

impl Watch for Recorder {
    fn at(&self, step: Step) -> Result<(), String> {
        self.0.lock().unwrap().push(step);
        Ok(())
    }
}

/// A service name no service has.
fn absent_service() -> String {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let name = format!(
        "CgSetupAbsent-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    );
    let found = scm::check_product_service(
        &Manager::open().expect("the SCM, for connecting"),
        &name,
        Path::new(r"C:\absent.exe"),
        Path::new(r"C:\absent.toml"),
        Route::Other,
    );
    assert_eq!(
        found,
        Ok(Product::Absent),
        "{name} is not established absent"
    );
    name
}

/// The verbs' machine on `root`, with no product service and `binary` as
/// the installed binary.
fn machine<'a>(root: &Root, watch: &'a (dyn Watch + 'a), binary: PathBuf) -> Machine<'a> {
    Machine {
        host: root.host.clone(),
        setup: Setup {
            paths: root.paths.clone(),
            custody: custody(),
            wait: WAIT,
            watch,
        },
        service: absent_service(),
        binary,
    }
}

/// `X.Y.Z.0`, the setup's own numeric version.
fn own_numbers() -> [u16; 4] {
    let [x, y, z] = numeric(SETUP_VERSION).expect("X.Y.Z");
    [x, y, z, 0]
}

/// An installed binary of this package's version, under `root`.
fn installed(root: &Root, numbers: [u16; 4], product_version: &str) -> PathBuf {
    let path = root.operator.join("cg-agent.exe");
    versioned_copy(&path, numbers, product_version);
    path
}

/// A bootstrap for `root`, and its fingerprints.
fn bootstrap(root: &Root) -> (Request, BTreeSet<String>) {
    let certificate = certificate("verbs.example.test");
    let anchor = root.anchor(&[&certificate]);
    let file = root.bootstrap(&bootstrap_text(
        anchor.to_str().expect("a Unicode path"),
        &[&certificate.fingerprint],
    ));
    (
        Request {
            bootstrap: Some(file),
            ..Request::default()
        },
        BTreeSet::from([certificate.fingerprint]),
    )
}

/// The names under the temporary `%ProgramData%`.
fn program_data(root: &Root) -> Vec<String> {
    root.tree()
        .into_keys()
        .filter_map(|name| name.strip_prefix("ProgramData\\").map(str::to_string))
        .collect()
}

fn exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

fn assert_code(result: Result<(), Refusal>, code: u8, fragments: &[&str]) -> Refusal {
    let refusal = result.expect_err("a refusal");
    assert_eq!(refusal.code, code, "{}", refusal.line);
    for fragment in fragments {
        assert!(
            refusal.line.contains(fragment),
            "{fragment}: {}",
            refusal.line
        );
    }
    fixtures::assert_clean(&refusal.line);
    refusal
}

// --- preflight ---------------------------------------------------------------------

/// A fresh host with a bootstrap: the directories, the record, the anchor and
/// the configuration staged, `service_running` false with no product service,
/// with no installed binary or one of this package's version.
#[test]
fn package_verbs_preflight_stages_a_fresh_host() {
    for binary_present in [false, true] {
        let root = Root::bare();
        let binary = if binary_present {
            installed(&root, own_numbers(), SETUP_VERSION)
        } else {
            root.operator.join("absent").join("cg-agent.exe")
        };
        let (request, fingerprints) = bootstrap(&root);
        let on = machine(&root, &quiet, binary);
        assert_eq!(verbs::preflight(&on, &request), Ok(()));
        assert_eq!(
            read_record(&on.setup).unwrap(),
            Some(Record {
                created: Created::ALL.into_iter().collect(),
                fingerprints,
                service_running: false,
            }),
            "binary present: {binary_present}"
        );
        let config = std::fs::read_to_string(root.paths.staged_config()).unwrap();
        assert!(config.contains(TOKEN), "the token is staged");
        assert!(exists(&root.paths.staged_anchor()));
    }
}

/// The downgrade (29) and the conflicting service precede the staging: a
/// refusal there creates nothing.
#[test]
fn package_verbs_preflight_refuses_a_downgrade_before_it_stages() {
    let root = Root::bare();
    let [x, y, z, _] = own_numbers();
    let binary = installed(&root, [x, y, z, 1], "9.9.9");
    let (request, _) = bootstrap(&root);
    let recorder = Recorder::new();
    let on = machine(&root, &recorder, binary);
    assert_code(
        verbs::preflight(&on, &request),
        codes::DOWNGRADE,
        &["a downgrade"],
    );
    assert!(recorder.steps().is_empty(), "{:?}", recorder.steps());
    assert!(program_data(&root).is_empty(), "{:?}", program_data(&root));
}

/// A pending host with no input: the record alone; the migration route on a
/// console layout: nothing under `<P>`, no record.
#[test]
fn package_verbs_preflight_the_routes_that_stage_nothing() {
    let root = Root::empty();
    root.touch("agent.toml");
    let on = machine(&root, &quiet, root.operator.join("absent.exe"));
    assert_eq!(verbs::preflight(&on, &Request::default()), Ok(()));
    assert_eq!(
        read_record(&on.setup).unwrap(),
        Some(Record {
            created: BTreeSet::new(),
            fingerprints: BTreeSet::new(),
            service_running: false,
        })
    );
    assert!(!exists(&root.paths.staged_config()));

    let root = Root::empty();
    root.touch("identity.json");
    let before = root.tree();
    let recorder = Recorder::new();
    let on = machine(&root, &recorder, root.operator.join("absent.exe"));
    let request = Request::new(
        None,
        Some("keep"),
        Some(&format!("sha256:{}", "ab".repeat(32))),
    );
    assert_eq!(verbs::preflight(&on, &request), Ok(()));
    assert!(recorder.steps().is_empty(), "{:?}", recorder.steps());
    assert_eq!(root.tree(), before);
}

// --- configure ---------------------------------------------------------------------

/// Configure's checks come in item 8's order before anything it starts:
/// `<P>` and the data root held, the record, each staged file, then the
/// installed binary's version, then the failure actions. A failure of any
/// is 40 or 45 and removes nothing: the record stays for the next preflight.
#[test]
fn package_verbs_configure_checks_in_order_and_leaves_the_record() {
    let root = Root::bare();
    let (request, _) = bootstrap(&root);
    assert_eq!(
        verbs::preflight(
            &machine(&root, &quiet, root.operator.join("absent.exe")),
            &request
        ),
        Ok(())
    );
    let before = root.tree();

    // No installed binary: 40 after the record's and the staged files'
    // checks.
    let recorder = Recorder::new();
    let configure = machine(&root, &recorder, root.operator.join("absent.exe"));
    assert_code(
        verbs::configure(&configure, &Request::default()),
        codes::OWN_STATE,
        &["no installed agent"],
    );
    assert_eq!(
        recorder.steps(),
        [
            Step::Hold(Name::ProductRoot),
            Step::Hold(Name::DataRoot),
            Step::ReadRecord,
            Step::CheckStaged(Name::StagedAnchor),
            Step::CheckStaged(Name::StagedConfig),
        ]
    );
    assert_eq!(root.tree(), before);

    // This package's binary: on to the failure actions, which a product
    // service that does not exist fails (40).
    let binary = installed(&root, own_numbers(), SETUP_VERSION);
    let before = root.tree();
    let configure = machine(&root, &quiet, binary);
    assert_code(
        verbs::configure(&configure, &Request::default()),
        codes::OWN_STATE,
        &["failure actions", &configure.service],
    );
    assert_eq!(root.tree(), before);
}

/// Configure reads the record, and the staged files only on a route that
/// staged: no record, a record that fails its checks, or a staged file that
/// is not there is 40, and nothing is removed.
#[test]
fn package_verbs_configure_without_its_record_or_staged_files_is_40() {
    type Spoil = fn(&Root);
    let cases: Vec<(&str, &str, Spoil)> = vec![
        ("no record", "there is no record", |root| {
            std::fs::remove_file(root.paths.data_root.join("setup-attempt.json")).unwrap()
        }),
        ("a record of another form", "is refused", |root| {
            let path = root.paths.data_root.join("setup-attempt.json");
            std::fs::remove_file(&path).unwrap();
            cg_agent::custody::create_new_file(&path, &private_file_sddl(), b"{}").unwrap();
        }),
        ("no staged configuration", "does not exist", |root| {
            std::fs::remove_file(root.paths.staged_config()).unwrap()
        }),
        ("a hard-linked staged anchor", "hard-linked", |root| {
            std::fs::hard_link(
                root.paths.staged_anchor(),
                root.paths.data_root.join("another-name"),
            )
            .unwrap()
        }),
    ];
    for (what, fragment, spoil) in cases {
        let root = Root::bare();
        let (request, _) = bootstrap(&root);
        let binary = installed(&root, own_numbers(), SETUP_VERSION);
        assert_eq!(
            verbs::preflight(&machine(&root, &quiet, binary.clone()), &request),
            Ok(()),
            "{what}"
        );
        spoil(&root);
        let before = root.tree();
        let refusal = assert_code(
            verbs::configure(&machine(&root, &quiet, binary), &Request::default()),
            codes::OWN_STATE,
            &[fragment],
        );
        assert!(!refusal.line.contains("failure actions"), "{what}");
        assert_eq!(root.tree(), before, "{what}");
    }

    // The route that started: no staged file is read.
    let root = Root::empty();
    root.touch("agent.toml");
    let binary = installed(&root, own_numbers(), SETUP_VERSION);
    assert_eq!(
        verbs::preflight(&machine(&root, &quiet, binary.clone()), &Request::default()),
        Ok(())
    );
    let recorder = Recorder::new();
    assert_code(
        verbs::configure(&machine(&root, &recorder, binary), &Request::default()),
        codes::OWN_STATE,
        &["failure actions"],
    );
    assert_eq!(
        recorder.steps(),
        [
            Step::Hold(Name::ProductRoot),
            Step::Hold(Name::DataRoot),
            Step::ReadRecord,
        ]
    );
}

/// On the migration route configure holds nothing and reads no record; its
/// variables in another form than the service's are 40.
#[test]
fn package_verbs_configure_on_the_migration_route_reads_no_record() {
    let root = Root::bare();
    let recorder = Recorder::new();
    let configure = machine(&root, &recorder, root.operator.join("absent.exe"));
    let migration = Request::new(
        None,
        Some("remove"),
        Some(&format!("sha256:{}", "cd".repeat(32))),
    );
    assert_code(
        verbs::configure(&configure, &migration),
        codes::OWN_STATE,
        &["no installed agent"],
    );
    assert!(recorder.steps().is_empty(), "{:?}", recorder.steps());

    for (legacy, anchors) in [
        (Some("purge"), Some("sha256:00")),
        (None, Some("sha256:00")),
    ] {
        assert_code(
            verbs::configure(&configure, &Request::new(None, legacy, anchors)),
            codes::OWN_STATE,
            &["migration variables"],
        );
    }
    assert!(recorder.steps().is_empty());
}

// --- reverse -----------------------------------------------------------------------

/// The reversal removes what the record names and starts the product service
/// only when the record found it running: with no such service that start is
/// 40, after the removals, its line naming the service.
#[test]
fn package_verbs_reverse_starts_only_from_the_record() {
    let root = Root::bare();
    let (request, _) = bootstrap(&root);
    let reverse = machine(&root, &quiet, root.operator.join("absent.exe"));
    assert_eq!(verbs::preflight(&reverse, &request), Ok(()));
    assert_eq!(
        verbs::reverse(&reverse),
        Ok(()),
        "the record says not running"
    );
    assert!(program_data(&root).is_empty(), "{:?}", program_data(&root));

    let root = Root::bare();
    let reverse = machine(&root, &quiet, root.operator.join("absent.exe"));
    let staged = certificate("reverse.example.test");
    drop(
        staging::stage(
            &reverse.setup,
            Some(staging::Files {
                anchor: staged.pem.as_bytes(),
                config: "[enrollment]\n",
                fingerprints: &BTreeSet::from([staged.fingerprint.clone()]),
            }),
            true,
        )
        .unwrap(),
    );
    assert_code(
        verbs::reverse(&reverse),
        codes::OWN_STATE,
        &["reversal", &reverse.service, "Win32 error 1060"],
    );
    assert!(
        program_data(&root).is_empty(),
        "removed before the start: {:?}",
        program_data(&root)
    );

    // A removal that fails and a start that fails: 40, one line naming both.
    let root = Root::bare();
    let setup = machine(&root, &quiet, root.operator.join("absent.exe"));
    drop(
        staging::stage(
            &setup.setup,
            Some(staging::Files {
                anchor: staged.pem.as_bytes(),
                config: "[enrollment]\n",
                fingerprints: &BTreeSet::from([staged.fingerprint.clone()]),
            }),
            true,
        )
        .unwrap(),
    );
    std::fs::remove_file(root.paths.staged_anchor()).unwrap();
    std::fs::create_dir(root.paths.staged_anchor()).unwrap();
    let refusal = assert_code(
        verbs::reverse(&setup),
        codes::OWN_STATE,
        &["Win32 error 1060", "is a directory"],
    );
    assert_eq!(
        refusal.line.matches("cg-setup: ").count(),
        1,
        "{}",
        refusal.line
    );

    // Nothing to reverse: no start.
    let root = Root::bare();
    assert_eq!(
        verbs::reverse(&machine(&root, &quiet, root.operator.join("absent.exe"))),
        Ok(())
    );
}

// --- From the review ---------------------------------------------------------------

/// Configure holds the data root from its first check on: it cannot be
/// renamed, at the record's reading or at the staged files' checks. (`<P>`,
/// which holds the data root, is held too, but a directory with an open
/// descendant cannot be renamed whether or not it is held.)
#[test]
fn package_verbs_configure_holds_the_directories() {
    let root = Root::bare();
    let (request, _) = bootstrap(&root);
    assert_eq!(
        verbs::preflight(
            &machine(&root, &quiet, root.operator.join("absent.exe")),
            &request
        ),
        Ok(())
    );
    let moved = root.paths.product_root.join("moved");
    let renamed = Mutex::new(Vec::new());
    let try_renaming = |step: Step| {
        if matches!(step, Step::ReadRecord | Step::CheckStaged(_)) {
            let outcome = std::fs::rename(&root.paths.data_root, &moved).is_ok();
            renamed.lock().unwrap().push((step, outcome));
        }
        Ok(())
    };
    let configure = machine(&root, &try_renaming, root.operator.join("absent.exe"));
    assert_code(
        verbs::configure(&configure, &Request::default()),
        codes::OWN_STATE,
        &["no installed agent"],
    );
    let renamed = renamed.into_inner().unwrap();
    assert_eq!(renamed.len(), 3, "{renamed:?}");
    assert!(renamed.iter().all(|(_, ok)| !ok), "{renamed:?}");
}

/// The staged configuration, which holds the token, is checked as item 6.4
/// of the Amendment 2026-11-09 opens it, as the anchor is: hard-linked, open
/// to users or a reparse point, it is 40, and nothing changes.
#[test]
fn package_verbs_configure_checks_the_staged_configuration() {
    type Spoil = fn(&Root);
    let cases: Vec<(&str, &str, Spoil)> = vec![
        ("hard-linked", "hard-linked", |root| {
            std::fs::hard_link(
                root.paths.staged_config(),
                root.paths.data_root.join("another-name"),
            )
            .unwrap()
        }),
        (
            "open to users",
            "allows access to a principal other than",
            |root| {
                let bytes = std::fs::read(root.paths.staged_config()).unwrap();
                std::fs::remove_file(root.paths.staged_config()).unwrap();
                cg_agent::custody::create_new_file(
                    &root.paths.staged_config(),
                    &format!("{}(A;;FR;;;BU)", private_file_sddl()),
                    &bytes,
                )
                .unwrap();
            },
        ),
        ("a junction", "reparse point", |root| {
            std::fs::remove_file(root.paths.staged_config()).unwrap();
            let elsewhere = root.operator.join("elsewhere");
            std::fs::create_dir(&elsewhere).unwrap();
            fixtures::junction(&root.paths.staged_config(), &elsewhere);
        }),
    ];
    for (what, fragment, spoil) in cases {
        let root = Root::bare();
        let (request, _) = bootstrap(&root);
        let binary = installed(&root, own_numbers(), SETUP_VERSION);
        assert_eq!(
            verbs::preflight(&machine(&root, &quiet, binary.clone()), &request),
            Ok(()),
            "{what}"
        );
        spoil(&root);
        let before = root.tree();
        assert_code(
            verbs::configure(&machine(&root, &quiet, binary), &Request::default()),
            codes::OWN_STATE,
            &[fragment],
        );
        assert_eq!(root.tree(), before, "{what}");
    }
}

/// Configure has no token in hand (the Amendment 2026-11-09, item 5): it
/// checks the staged configuration without reading it. A lock on its bytes
/// that no reader can pass leaves configure's checks to pass, on to the
/// installed binary's version.
#[test]
fn package_verbs_configure_reads_no_token() {
    let root = Root::bare();
    let (request, _) = bootstrap(&root);
    assert_eq!(
        verbs::preflight(
            &machine(&root, &quiet, root.operator.join("absent.exe")),
            &request
        ),
        Ok(())
    );
    let locked = std::fs::File::open(root.paths.staged_config()).expect("open to lock");
    locked.lock().expect("an exclusive lock on the whole file");
    let outcome = verbs::configure(
        &machine(&root, &quiet, root.operator.join("absent.exe")),
        &Request::default(),
    );
    drop(locked);
    assert_code(outcome, codes::OWN_STATE, &["no installed agent"]);
}

/// An installed binary that is not this package's is 40 at configure, before
/// the failure actions, and the record stays.
#[test]
fn package_verbs_configure_refuses_another_packages_binary() {
    let root = Root::bare();
    let (request, _) = bootstrap(&root);
    let binary = installed(&root, [0, 0, 1, 0], "0.0.1");
    assert_eq!(
        verbs::preflight(&machine(&root, &quiet, binary.clone()), &request),
        Ok(())
    );
    let before = root.tree();
    let refusal = assert_code(
        verbs::configure(&machine(&root, &quiet, binary), &Request::default()),
        codes::OWN_STATE,
        &["not this package's"],
    );
    assert!(
        !refusal.line.contains("failure actions"),
        "{}",
        refusal.line
    );
    assert_eq!(root.tree(), before);
}
