//! `cg-setup`'s verbs (SPEC-024 Amendment 2026-11-10, item 9), after the
//! platform and the elevation, on temporary roots, the current user standing
//! for SYSTEM and Administrators, and a product service that does not exist:
//! preflight's order up to the staging and the stop, configure's checks in
//! order before anything it starts, the reversal's start from the record
//! alone, and the purge's refusals before the stop (27, 28, the absent
//! service's 46), its reading of the service's end and its wait (the
//! Amendment 2026-11-15, point 3). What a real service adds is in the SCM
//! tests (`setup_service`).
#![cfg(windows)]

mod fixtures;

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use cg_setup::codes;
use cg_setup::preflight::{Request, Staged};
use cg_setup::record::{Created, Record};
use cg_setup::scm::{self, Manager, Product, Route};
use cg_setup::staging::{self, read_record, Name, Setup, SetupCustody, Step, Watch};
use cg_setup::starter::{own_creation, Process};
use cg_setup::verbs::{self, Machine};
use cg_setup::version::{numeric, SETUP_VERSION};
use cg_setup::wizard::{asks_again, Answer, Closed, Fields, Offer, Starter, Wizard};
use cg_setup::Refusal;
use zeroize::Zeroizing;

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
        wizard: None,
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

/// SPEC-024 Amendment 2026-11-20, point 2: a migration variable is 28 before
/// the conflicting service and the downgrade, and creates nothing.
#[test]
fn package_verbs_preflight_a_migration_variable_is_28_before_the_downgrade() {
    let root = Root::bare();
    let [x, y, z, _] = own_numbers();
    let binary = installed(&root, [x, y, z, 1], "9.9.9");
    let (request, _) = bootstrap(&root);
    let request = Request {
        migrate_legacy: Some("keep".to_string()),
        ..request
    };
    let recorder = Recorder::new();
    let on = machine(&root, &recorder, binary);
    assert_code(
        verbs::preflight(&on, &request),
        codes::CONSOLE_LAYOUT,
        &["CG_MIGRATE_LEGACY", "not supported", "installation runbook"],
    );
    assert!(recorder.steps().is_empty(), "{:?}", recorder.steps());
    assert!(program_data(&root).is_empty(), "{:?}", program_data(&root));
}

/// A pending host with no input: the record alone; a migration asked on a
/// console layout: 28 (SPEC-024 Amendment 2026-11-20, point 2), nothing
/// under `<P>`, no record.
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
    assert_code(
        verbs::preflight(&on, &request),
        codes::CONSOLE_LAYOUT,
        &["CG_MIGRATE_LEGACY", "not supported", "installation runbook"],
    );
    assert!(recorder.steps().is_empty(), "{:?}", recorder.steps());
    assert_eq!(root.tree(), before);
}

// --- The wizard (SPEC-024 Amendment 2026-11-13) --------------------------------------

/// The process that started the setup, as the tests stand in for it.
struct FakeStarter(Rc<Cell<bool>>);

impl Starter for FakeStarter {
    fn running(&self) -> bool {
        self.0.get()
    }
}

type Script<'a> = dyn Fn(&mut dyn FnMut(&Fields) -> Result<Staged, Refusal>, &Cell<bool>) -> Result<Answer, Refusal>
    + 'a;

/// A wizard whose dialog is `script`: it is given the submission's check and
/// the starter's state, and answers as the dialog would.
struct FakeWizard<'a> {
    holds: bool,
    running: Rc<Cell<bool>>,
    offers: RefCell<Vec<Offer>>,
    script: Box<Script<'a>>,
}

impl<'a> FakeWizard<'a> {
    fn new(
        script: impl Fn(
                &mut dyn FnMut(&Fields) -> Result<Staged, Refusal>,
                &Cell<bool>,
            ) -> Result<Answer, Refusal>
            + 'a,
    ) -> Self {
        FakeWizard {
            holds: true,
            running: Rc::new(Cell::new(true)),
            offers: RefCell::new(Vec::new()),
            script: Box::new(script),
        }
    }

    fn offers(&self) -> Vec<Offer> {
        self.offers.borrow().clone()
    }
}

impl Wizard for FakeWizard<'_> {
    fn hold_starter(&self) -> Result<Box<dyn Starter>, Refusal> {
        if self.holds {
            Ok(Box::new(FakeStarter(self.running.clone())))
        } else {
            Err(Refusal::new(
                codes::OWN_STATE,
                "the process that started the setup cannot be held: a test",
            ))
        }
    }

    fn ask(
        &self,
        offer: Offer,
        starter: &dyn Starter,
        take: &mut dyn FnMut(&Fields) -> Result<Staged, Refusal>,
    ) -> Result<Answer, Refusal> {
        assert!(starter.running(), "the dialog shows while the starter runs");
        self.offers.borrow_mut().push(offer);
        (self.script)(take, &self.running)
    }
}

/// The wizard's fields for `root`: an anchor of a new certificate, and its
/// fingerprint.
fn wizard_fields(root: &Root) -> (Fields, BTreeSet<String>) {
    let certificate = certificate("wizard.example.test");
    let anchor = root.anchor(&[&certificate]);
    (
        Fields {
            server_url: fixtures::ENROLL_URL.to_string(),
            heartbeat_url: fixtures::HEARTBEAT_URL.to_string(),
            anchor_path: anchor.to_str().expect("a Unicode path").to_string(),
            fingerprints: format!("{}\r\n", certificate.fingerprint),
            token: Zeroizing::new(TOKEN.to_string()),
        },
        BTreeSet::from([certificate.fingerprint]),
    )
}

fn full_window() -> Request {
    Request::default().with_ui(Some("4"))
}

fn with_wizard<'a>(
    root: &Root,
    watch: &'a (dyn Watch + 'a),
    binary: PathBuf,
    wizard: &'a dyn Wizard,
) -> Machine<'a> {
    Machine {
        wizard: Some(wizard),
        ..machine(root, watch, binary)
    }
}

/// A submission the script accepts.
fn submit(
    take: &mut dyn FnMut(&Fields) -> Result<Staged, Refusal>,
    fields: &Fields,
) -> Result<Answer, Refusal> {
    take(fields).map(Answer::Input)
}

/// A fresh host with the setup's full window: the wizard's input staged as a
/// bootstrap file's is, the record naming what it created and its
/// fingerprints.
#[test]
fn package_verbs_wizard_a_fresh_host_stages_its_input() {
    let root = Root::bare();
    let (fields, fingerprints) = wizard_fields(&root);
    let wizard = FakeWizard::new(|take, _| submit(take, &fields));
    let on = with_wizard(&root, &quiet, root.operator.join("absent.exe"), &wizard);
    assert_eq!(verbs::preflight(&on, &full_window()), Ok(()));
    assert_eq!(wizard.offers(), vec![Offer::Fresh]);
    assert_eq!(
        read_record(&on.setup).unwrap(),
        Some(Record {
            created: Created::ALL.into_iter().collect(),
            fingerprints,
            service_running: false,
        })
    );
    let config = std::fs::read_to_string(root.paths.staged_config()).unwrap();
    assert!(config.contains(TOKEN), "the token is staged");
    assert!(exists(&root.paths.staged_anchor()));
}

/// A refused field (24) or anchor (25) is the dialog's to show and ask again,
/// naming the field, the anchor's path or the check, never the token; the
/// next submission is taken.
#[test]
fn package_verbs_wizard_a_refused_field_or_anchor_is_asked_again() {
    let root = Root::bare();
    let (fields, _) = wizard_fields(&root);
    let other = certificate("other.example.test");
    let refusals = RefCell::new(Vec::new());
    let wizard = FakeWizard::new(|take, _| {
        let attempts: Vec<Fields> = vec![
            Fields {
                anchor_path: "server-ca.pem".to_string(),
                ..copy(&fields)
            },
            Fields {
                fingerprints: other.fingerprint.clone(),
                ..copy(&fields)
            },
            Fields {
                anchor_path: root
                    .operator
                    .join("absent.pem")
                    .to_str()
                    .unwrap()
                    .to_string(),
                ..copy(&fields)
            },
            Fields {
                server_url: "http://enroll.example.test:8444".to_string(),
                ..copy(&fields)
            },
            Fields {
                token: Zeroizing::new(String::new()),
                ..copy(&fields)
            },
        ];
        for attempt in &attempts {
            let refusal = take(attempt).expect_err("a refused submission");
            assert!(asks_again(&refusal), "{}", refusal.line);
            refusals.borrow_mut().push(refusal);
        }
        submit(take, &fields)
    });
    let on = with_wizard(&root, &quiet, root.operator.join("absent.exe"), &wizard);
    assert_eq!(verbs::preflight(&on, &full_window()), Ok(()));
    drop(on);
    drop(wizard);
    let refusals = refusals.into_inner();
    let expected: [(u8, &str); 5] = [
        (codes::INVALID_INPUT, "server.trust_anchor_path"),
        (codes::ANCHOR, "server-ca.pem"),
        (codes::ANCHOR, "absent.pem"),
        (codes::INVALID_INPUT, "server.url"),
        (codes::INVALID_INPUT, "enrollment.token"),
    ];
    assert_eq!(refusals.len(), expected.len());
    for (refusal, (code, naming)) in refusals.iter().zip(expected) {
        assert_eq!(refusal.code, code, "{}", refusal.line);
        assert!(refusal.line.contains(naming), "{naming}: {}", refusal.line);
        assert!(!refusal.line.contains("bootstrap file"), "{}", refusal.line);
        fixtures::assert_clean(&refusal.line);
    }
    assert!(exists(&root.paths.staged_config()));
}

fn copy(fields: &Fields) -> Fields {
    Fields {
        server_url: fields.server_url.clone(),
        heartbeat_url: fields.heartbeat_url.clone(),
        anchor_path: fields.anchor_path.clone(),
        fingerprints: fields.fingerprints.clone(),
        token: fields.token.clone(),
    }
}

/// A cancellation, the 15 minutes, the starter's exit, and a submission once
/// the starter has exited: 22, with nothing changed and no step taken.
#[test]
fn package_verbs_wizard_closed_or_a_late_submission_is_22_with_nothing_changed() {
    for closed in [Closed::Cancelled, Closed::TimedOut, Closed::StarterExited] {
        let root = Root::bare();
        let wizard = FakeWizard::new(|_, _| Ok(Answer::Closed(closed)));
        let recorder = Recorder::new();
        let on = with_wizard(&root, &recorder, root.operator.join("absent.exe"), &wizard);
        assert_code(
            verbs::preflight(&on, &full_window()),
            codes::NO_INPUT,
            &["nothing was changed"],
        );
        assert!(recorder.steps().is_empty(), "{:?}", recorder.steps());
        assert!(program_data(&root).is_empty(), "{:?}", program_data(&root));
    }

    let root = Root::bare();
    let (fields, _) = wizard_fields(&root);
    let wizard = FakeWizard::new(|take, running| {
        running.set(false);
        submit(take, &fields)
    });
    let recorder = Recorder::new();
    let on = with_wizard(&root, &recorder, root.operator.join("absent.exe"), &wizard);
    assert_code(
        verbs::preflight(&on, &full_window()),
        codes::NO_INPUT,
        &["the process that started the setup exited"],
    );
    assert!(recorder.steps().is_empty(), "{:?}", recorder.steps());
    assert!(program_data(&root).is_empty(), "{:?}", program_data(&root));

    // A pending host's choice to keep, once the starter has exited.
    let root = Root::empty();
    root.touch("agent.toml");
    let before = root.tree();
    let wizard = FakeWizard::new(|_, running| {
        running.set(false);
        Ok(Answer::Keep)
    });
    let recorder = Recorder::new();
    let on = with_wizard(&root, &recorder, root.operator.join("absent.exe"), &wizard);
    assert_code(
        verbs::preflight(&on, &full_window()),
        codes::NO_INPUT,
        &["the process that started the setup exited"],
    );
    assert!(recorder.steps().is_empty(), "{:?}", recorder.steps());
    assert_eq!(root.tree(), before);
}

/// The starter that cannot be held: 40, and no dialog.
#[test]
fn package_verbs_wizard_a_starter_not_held_is_40_and_no_dialog() {
    let root = Root::bare();
    let mut wizard = FakeWizard::new(|_, _| panic!("no dialog shows"));
    wizard.holds = false;
    let recorder = Recorder::new();
    let on = with_wizard(&root, &recorder, root.operator.join("absent.exe"), &wizard);
    assert_code(
        verbs::preflight(&on, &full_window()),
        codes::OWN_STATE,
        &["the process that started the setup"],
    );
    assert!(recorder.steps().is_empty(), "{:?}", recorder.steps());
    assert!(program_data(&root).is_empty(), "{:?}", program_data(&root));
}

/// An answer the route cannot take, a choice to keep on a fresh host's
/// offer, is 40 with nothing changed, not a panic of the elevated process.
#[test]
fn package_verbs_wizard_an_answer_it_cannot_take_is_40() {
    let root = Root::bare();
    let wizard = FakeWizard::new(|_, _| Ok(Answer::Keep));
    let recorder = Recorder::new();
    let on = with_wizard(&root, &recorder, root.operator.join("absent.exe"), &wizard);
    assert_code(
        verbs::preflight(&on, &full_window()),
        codes::OWN_STATE,
        &["inconsistent"],
    );
    assert_eq!(wizard.offers(), vec![Offer::Fresh]);
    assert!(recorder.steps().is_empty(), "{:?}", recorder.steps());
    assert!(program_data(&root).is_empty(), "{:?}", program_data(&root));
}

/// No dialog without the full window, with a bootstrap file, with a migration
/// variable, on an enrolled host, or on a machine with no wizard; and none
/// after a downgrade (29), which is read before it.
#[test]
fn package_verbs_wizard_shows_only_where_point_2_says() {
    let never = || FakeWizard::new(|_, _| panic!("no dialog shows"));
    let absent = |root: &Root| root.operator.join("absent.exe");

    for ui in [None, Some(""), Some("2"), Some("3")] {
        let root = Root::bare();
        let wizard = never();
        let on = with_wizard(&root, &quiet, absent(&root), &wizard);
        assert_code(
            verbs::preflight(&on, &Request::default().with_ui(ui)),
            codes::NO_INPUT,
            &["no input"],
        );
        assert!(program_data(&root).is_empty(), "{ui:?}");
    }

    let root = Root::bare();
    let on = machine(&root, &quiet, absent(&root));
    assert_code(
        verbs::preflight(&on, &full_window()),
        codes::NO_INPUT,
        &["no input"],
    );

    let root = Root::bare();
    let (request, fingerprints) = bootstrap(&root);
    let wizard = never();
    let on = with_wizard(&root, &quiet, absent(&root), &wizard);
    assert_eq!(
        verbs::preflight(
            &on,
            &Request {
                ui: Some("4".into()),
                ..request
            }
        ),
        Ok(())
    );
    assert_eq!(
        read_record(&on.setup).unwrap().unwrap().fingerprints,
        fingerprints
    );

    let root = Root::bare();
    let wizard = never();
    let on = with_wizard(&root, &quiet, absent(&root), &wizard);
    let migration = Request::new(
        None,
        Some("keep"),
        Some(&format!("sha256:{}", "ab".repeat(32))),
    )
    .with_ui(Some("4"));
    assert_code(
        verbs::preflight(&on, &migration),
        codes::CONSOLE_LAYOUT,
        &["CG_MIGRATE_LEGACY", "not supported", "installation runbook"],
    );

    let root = Root::empty();
    std::fs::create_dir(root.paths.data_root.join("identity")).expect("identity");
    let wizard = never();
    let on = with_wizard(&root, &quiet, absent(&root), &wizard);
    assert_eq!(verbs::preflight(&on, &full_window()), Ok(()));
    assert!(!exists(&root.paths.staged_config()));

    let root = Root::bare();
    let [x, y, z, _] = own_numbers();
    let binary = installed(&root, [x, y, z, 1], "9.9.9");
    let wizard = never();
    let recorder = Recorder::new();
    let on = with_wizard(&root, &recorder, binary, &wizard);
    assert_code(
        verbs::preflight(&on, &full_window()),
        codes::DOWNGRADE,
        &["a downgrade"],
    );
    assert!(recorder.steps().is_empty(), "{:?}", recorder.steps());
}

/// A pending host: the wizard offers to keep its configuration, which
/// starts the service with no argument and stages nothing, or to replace it.
#[test]
fn package_verbs_wizard_a_pending_host_kept_or_replaced() {
    let root = Root::empty();
    root.touch("agent.toml");
    let wizard = FakeWizard::new(|_, _| Ok(Answer::Keep));
    let on = with_wizard(&root, &quiet, root.operator.join("absent.exe"), &wizard);
    assert_eq!(verbs::preflight(&on, &full_window()), Ok(()));
    assert_eq!(wizard.offers(), vec![Offer::Pending]);
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
    root.touch("agent.toml");
    let (fields, fingerprints) = wizard_fields(&root);
    let wizard = FakeWizard::new(|take, _| submit(take, &fields));
    let on = with_wizard(&root, &quiet, root.operator.join("absent.exe"), &wizard);
    assert_eq!(verbs::preflight(&on, &full_window()), Ok(()));
    assert_eq!(wizard.offers(), vec![Offer::Pending]);
    let record = read_record(&on.setup).unwrap().unwrap();
    assert_eq!(record.fingerprints, fingerprints);
    assert!(record.created.contains(&Created::StagedConfig));
    assert!(std::fs::read_to_string(root.paths.staged_config())
        .unwrap()
        .contains(TOKEN));
}

/// Point 3: after the dialog, the state, the conflicting service and the
/// downgrade are read again, and item 3 applies to what is read as to a
/// bootstrap file's input; a choice to keep needs the host still pending.
/// A refusal there changes nothing.
#[test]
fn package_verbs_wizard_the_state_is_read_again_after_the_dialog() {
    type Change = Box<dyn Fn(&Root)>;
    let identity: Change =
        Box::new(|root| std::fs::create_dir(root.paths.data_root.join("identity")).unwrap());
    let migrating: Change = Box::new(|root| std::fs::create_dir(root.paths.staging()).unwrap());
    let console: Change = Box::new(|root| root.touch("cert.pem"));
    let untrusted: Change = Box::new(|root| {
        std::fs::remove_dir(&root.paths.data_root).unwrap();
        std::fs::remove_dir(&root.paths.product_root).unwrap();
        root.directories(&fixtures::open_directory_sddl());
    });
    let gone: Change =
        Box::new(|root| std::fs::remove_file(root.paths.data_root.join("agent.toml")).unwrap());
    let newer: Change = Box::new(|root| {
        let [x, y, z, _] = own_numbers();
        versioned_copy(&root.operator.join("cg-agent.exe"), [x, y, z, 1], "9.9.9");
    });

    // (pending at the start, keep rather than submit, the change, the code)
    let cases: Vec<(bool, bool, &Change, u8)> = vec![
        (false, false, &identity, codes::ALREADY_CONFIGURED),
        (false, false, &migrating, codes::CONSOLE_LAYOUT),
        (false, false, &console, codes::CONSOLE_LAYOUT),
        (false, false, &untrusted, codes::UNTRUSTED_PATH),
        (false, false, &newer, codes::DOWNGRADE),
        (true, false, &identity, codes::ALREADY_CONFIGURED),
        (true, true, &identity, codes::ALREADY_CONFIGURED),
        (true, true, &gone, codes::NO_INPUT),
        (true, true, &console, codes::CONSOLE_LAYOUT),
        (true, true, &newer, codes::DOWNGRADE),
    ];
    for (index, (pending, keep, change, code)) in cases.into_iter().enumerate() {
        let root = Root::empty();
        if pending {
            root.touch("agent.toml");
        }
        let (fields, _) = wizard_fields(&root);
        let binary = root.operator.join("cg-agent.exe");
        let wizard = FakeWizard::new(|take, _| {
            let answer = if keep {
                Answer::Keep
            } else {
                submit(take, &fields)?
            };
            change(&root);
            Ok(answer)
        });
        let recorder = Recorder::new();
        let on = with_wizard(&root, &recorder, binary, &wizard);
        assert_code(verbs::preflight(&on, &full_window()), code, &[]);
        assert!(
            recorder.steps().is_empty(),
            "{index}: {:?}",
            recorder.steps()
        );
        assert!(!exists(&root.paths.staged_config()), "{index}");
        assert!(read_record(&on.setup).unwrap_or(None).is_none(), "{index}");
    }

    // A fresh host that became pending takes the input.
    let root = Root::empty();
    let (fields, fingerprints) = wizard_fields(&root);
    let wizard = FakeWizard::new(|take, _| {
        let answer = submit(take, &fields);
        root.touch("agent.toml");
        answer
    });
    let on = with_wizard(&root, &quiet, root.operator.join("absent.exe"), &wizard);
    assert_eq!(verbs::preflight(&on, &full_window()), Ok(()));
    assert_eq!(wizard.offers(), vec![Offer::Fresh]);
    assert_eq!(
        read_record(&on.setup).unwrap().unwrap().fingerprints,
        fingerprints
    );

    // A pending host that became fresh takes the input too.
    let root = Root::empty();
    root.touch("agent.toml");
    let (fields, _) = wizard_fields(&root);
    let wizard = FakeWizard::new(|take, _| {
        let answer = submit(take, &fields);
        std::fs::remove_file(root.paths.data_root.join("agent.toml")).unwrap();
        answer
    });
    let on = with_wizard(&root, &quiet, root.operator.join("absent.exe"), &wizard);
    assert_eq!(verbs::preflight(&on, &full_window()), Ok(()));
    assert!(exists(&root.paths.staged_config()));
}

// --- The process that started the setup --------------------------------------------

/// The test's own starter, cargo or the shell, is held and running; a
/// process created after this one is refused by its creation time; and a
/// held process that exits is seen to have exited.
#[test]
fn package_verbs_wizard_the_starter_is_held_by_its_creation() {
    let starter = Process::starter().expect("this test's starter is held");
    assert!(starter.running());

    let mut child = std::process::Command::new("cmd")
        .args(["/c", "exit", "0"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("cmd");
    let refused = Process::hold(child.id(), own_creation().unwrap())
        .expect_err("a process created after this one");
    assert!(refused.contains("created after this one"), "{refused}");

    let held = Process::hold(child.id(), u64::MAX).expect("the child, held");
    child.wait().expect("the child exits");
    assert!(!held.running(), "an exited process does not run");

    assert!(Process::hold(0, u64::MAX).is_err(), "the idle process");
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

/// SPEC-024 Amendment 2026-11-20, point 2: configure takes no value of a
/// migration variable; a non-empty one, well formed or not, is 40 before it
/// holds, reads or starts anything, its line naming the variables, never a
/// value, saying that the migration is not supported and sending to the
/// installation runbook.
#[test]
fn package_verbs_configure_with_a_migration_variable_is_40() {
    let root = Root::bare();
    let recorder = Recorder::new();
    let configure = machine(&root, &recorder, root.operator.join("absent.exe"));
    let before = root.tree();
    for (legacy, anchors) in [
        (Some("remove"), Some(format!("sha256:{}", "cd".repeat(32)))),
        (Some("keep"), None),
        (None, Some("sha256:00".to_string())),
        (Some("s86-not-a-choice"), Some("sha256:00".to_string())),
    ] {
        let request = Request::new(None, legacy, anchors.as_deref());
        let refusal = assert_code(
            verbs::configure(&configure, &request),
            codes::OWN_STATE,
            &[
                "CG_MIGRATE_LEGACY",
                "CG_TRUST_ANCHORS",
                "not supported",
                "installation runbook",
            ],
        );
        for value in ["remove", "keep", "s86-not-a-choice", "sha256:"] {
            assert!(!refusal.line.contains(value), "{}", refusal.line);
        }
        assert!(
            !refusal.line.contains("no installed agent"),
            "{}",
            refusal.line
        );
    }
    assert!(recorder.steps().is_empty(), "{:?}", recorder.steps());
    assert_eq!(root.tree(), before);
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

// --- purge (SPEC-024 Amendment 2026-11-15, point 3) ----------------------------

const PURGE_WITHIN: std::time::Duration = std::time::Duration::from_secs(5);

/// A purge refusal's line is one sentence: no run of spaces inside it.
fn assert_one_line(refusal: &Refusal) {
    assert!(!refusal.line.contains("  "), "{}", refusal.line);
}

/// With `<P>` absent there is nothing to remove: 0, with nothing created.
#[test]
fn package_verbs_purge_with_no_product_root_is_0() {
    let root = Root::bare();
    let before = root.tree();
    let on = machine(&root, &quiet, root.operator.join("cg-agent.exe"));
    assert_eq!(verbs::purge(&on, PURGE_WITHIN), Ok(()));
    assert_eq!(root.tree(), before, "nothing created");
}

/// Point 3.2: the host's state as preflight reads it, before the service:
/// an untrusted path 27, a migration in progress or a console layout 28,
/// each with nothing changed and before the absent service's 46.
#[test]
fn package_verbs_purge_refuses_the_states_preflight_refuses() {
    let untrusted = Root::empty();
    std::fs::create_dir(untrusted.paths.legacy()).unwrap();
    let in_progress = Root::empty();
    std::fs::create_dir(in_progress.paths.staging()).unwrap();
    let console = Root::empty();
    console.touch("cert.pem");
    let junction = Root::bare();
    let target = junction.operator.join("target");
    std::fs::create_dir(&target).unwrap();
    fixtures::junction(&junction.paths.product_root, &target);
    for (what, root, code) in [
        ("untrusted", untrusted, codes::UNTRUSTED_PATH),
        (
            "a migration in progress",
            in_progress,
            codes::CONSOLE_LAYOUT,
        ),
        ("a console layout", console, codes::CONSOLE_LAYOUT),
        ("a junction as <P>", junction, codes::UNTRUSTED_PATH),
    ] {
        let before = root.tree();
        let on = machine(&root, &quiet, root.operator.join("cg-agent.exe"));
        let refusal = assert_code(verbs::purge(&on, PURGE_WITHIN), code, &["purge"]);
        assert_one_line(&refusal);
        assert_eq!(root.tree(), before, "{what}: {}", refusal.line);
        // SPEC-024 Amendment 2026-11-20, point 3: a layout's line names it,
        // says that the product neither adopts nor migrates it and sends to
        // the installation runbook; none offers the migration.
        if code == codes::CONSOLE_LAYOUT {
            for fragment in ["neither adopts nor migrates", "installation runbook"] {
                assert!(refusal.line.contains(fragment), "{what}: {}", refusal.line);
            }
        }
        for offer in ["finish or reverse", "migrate them", "CG_MIGRATE_LEGACY"] {
            assert!(!refusal.line.contains(offer), "{what}: {}", refusal.line);
        }
        // The legacy directory beside a data root with no identity: its 27
        // names it as a layout the product neither adopts nor migrates.
        if what == "untrusted" {
            assert!(refusal.line.contains("agent.legacy"), "{}", refusal.line);
            assert!(
                refusal.line.contains("neither adopts nor migrates"),
                "{}",
                refusal.line
            );
        }
    }
}

/// Point 3.3: with no product service, nothing but SYSTEM can remove what
/// only SYSTEM reaches: 46, with nothing changed.
#[test]
fn package_verbs_purge_with_no_product_service_is_46() {
    let root = Root::empty();
    root.touch("agent.toml");
    let before = root.tree();
    let on = machine(&root, &quiet, root.operator.join("cg-agent.exe"));
    let refusal = assert_code(
        verbs::purge(&on, PURGE_WITHIN),
        codes::PURGE_REFUSED,
        &["absent"],
    );
    assert_one_line(&refusal);
    assert_eq!(root.tree(), before);
}

/// Point 3.5's reading of the end: 0 with `dwWin32ExitCode` 0 or with 1066
/// and code 0; 46 for each code given before any removal; 47 for 15, another
/// code, another Win32 error, an unreadable state and any state at the end;
/// each cause one sentence.
#[test]
fn package_verbs_purge_outcome_maps_the_service_s_end() {
    use cg_setup::scm::Ended;
    use cg_setup::verbs::{outcome, Outcome};
    let within = std::time::Duration::from_secs(300);
    let stopped = |win32: u32, specific: u32| Ended::Stopped { win32, specific };
    assert_eq!(outcome(&stopped(0, 0), within), Outcome::Purged);
    assert_eq!(outcome(&stopped(0, 7), within), Outcome::Purged);
    assert_eq!(outcome(&stopped(1066, 0), within), Outcome::Purged);
    for code in [1, 2, 10, 13, 14] {
        match outcome(&stopped(1066, code), within) {
            Outcome::RemovedNothing(cause) => {
                assert!(cause.contains(&format!("code {code},")), "{cause}")
            }
            other => panic!("{code}: {other:?}"),
        }
    }
    let incomplete = |ended: Ended| match outcome(&ended, within) {
        Outcome::Incomplete(cause) => {
            assert!(!cause.contains("  "), "{cause}");
            cause
        }
        other => panic!("{ended:?}: {other:?}"),
    };
    let fifteen = incomplete(stopped(1066, 15));
    assert!(fifteen.contains("code 15;"), "{fifteen}");
    assert!(fifteen.contains("when it left a name"), "{fifteen}");
    assert!(incomplete(stopped(1066, 3)).contains("code 3"));
    assert!(incomplete(stopped(1067, 0)).contains("Win32 error 1067"));
    assert!(incomplete(Ended::Unreadable("unread".to_string())).contains("unread"));
    let pending = incomplete(Ended::Still(2));
    assert!(pending.contains("within 300 seconds"), "{pending}");
    assert!(pending.contains("START_PENDING"), "{pending}");
}

/// Point 3.5's wait: a `STOPPED` read at the end of the 300 seconds, the
/// last reading, is read with its codes as any other, and only another state
/// at the end is `Still`; a reading that fails ends the wait.
#[test]
fn package_verbs_purge_wait_reads_a_stop_at_its_end_with_its_codes() {
    use cg_setup::scm::{await_stopped, Ended};
    use std::time::{Duration, Instant};
    let within = Duration::from_millis(200);
    let poll = Duration::from_millis(50);
    // The service reports STOPPED with code 0 from the end of the wait on:
    // only the last reading sees it.
    let end = Instant::now() + within;
    let mut readings = 0;
    let ended = await_stopped(within, poll, || {
        readings += 1;
        if Instant::now() >= end {
            Ok((1, 1066, 0))
        } else {
            Ok((2, 0, 0))
        }
    });
    assert_eq!(
        ended,
        Ended::Stopped {
            win32: 1066,
            specific: 0
        }
    );
    assert!(readings > 1, "{readings} readings");
    assert_eq!(
        await_stopped(within, poll, || Ok((2, 0, 0))),
        Ended::Still(2)
    );
    assert_eq!(
        await_stopped(within, poll, || Err("unread".to_string())),
        Ended::Unreadable("unread".to_string())
    );
}

/// 46's line after a service that was running: started again, not stopped,
/// or the start's failure, named as the purge's and not the reversal's.
#[test]
fn package_verbs_purge_restart_note_names_the_restart() {
    use cg_setup::verbs::restart_note;
    assert_eq!(restart_note(Ok(true)), "; the service was started again");
    assert!(restart_note(Ok(false)).contains("not started again"));
    let failed = restart_note(Err(Refusal::new(
        codes::OWN_STATE,
        "the reversal could not start the service S: the service control manager refused the start: Win32 error 1058",
    )));
    assert_eq!(
        failed,
        "; the service could not be started again: the service control manager refused the start: Win32 error 1058"
    );
}
