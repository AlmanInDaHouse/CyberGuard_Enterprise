//! Preflight's staging, `setup-attempt.json` and the reversal (SPEC-024
//! Amendment 2026-11-10, items 5 and 7), on temporary roots, the current
//! user standing for SYSTEM and Administrators: the record's form; the order
//! of the directories, the record, the anchor and the configuration; what an
//! earlier attempt left removed first; `<P>` and the data root held and
//! checked through those handles, a junction planted after the read
//! refused; a directory under a staged name; a failure and a crash at each
//! step; the removal and its order; and the reversal on each state.
#![cfg(windows)]

mod fixtures;

use std::collections::BTreeSet;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cg_agent::custody::{self, Kind, Policy};
use cg_setup::codes;
use cg_setup::record::{Created, Record, RECORD, RECORD_LIMIT};
use cg_setup::staging::{
    read_record, reverse, stage, Files, Name, Reversal, Setup, SetupCustody, Step, Watch,
};
use cg_setup::Refusal;

use fixtures::{
    certificate, closed_directory_sddl, junction, open_directory_sddl, private_file_sddl,
    test_policy, Root, TOKEN, WAIT,
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

fn setup<'a>(root: &Root, watch: &'a (dyn Watch + 'a)) -> Setup<'a> {
    Setup {
        paths: root.paths.clone(),
        custody: custody(),
        wait: WAIT,
        watch,
    }
}

fn quiet(_: Step) -> Result<(), String> {
    Ok(())
}

/// What a route that stages writes, and its fingerprint.
struct Staged {
    anchor: Vec<u8>,
    config: String,
    fingerprints: BTreeSet<String>,
}

impl Staged {
    fn new() -> Self {
        let certificate = certificate("staging.example.test");
        Staged {
            anchor: certificate.pem.into_bytes(),
            config: format!("[enrollment]\ntoken = {TOKEN:?}\n"),
            fingerprints: BTreeSet::from([certificate.fingerprint]),
        }
    }

    fn files(&self) -> Files<'_> {
        Files {
            anchor: &self.anchor,
            config: &self.config,
            fingerprints: &self.fingerprints,
        }
    }
}

fn path(root: &Root, name: Name) -> PathBuf {
    name.path(&root.paths)
}

fn exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

/// Whether `name` is there, with the security the setup gives it.
fn has_final_security(root: &Root, name: Name) -> bool {
    let kind = match name {
        Name::ProductRoot | Name::DataRoot => Kind::Directory,
        _ => Kind::File,
    };
    let protected = Policy {
        protected: true,
        ..test_policy(false)
    };
    custody::check_if_exists(&path(root, name), kind, &protected).unwrap_or(false)
}

/// A file in the data root with the staged files' security.
fn plant_file(root: &Root, name: Name, contents: &[u8]) {
    custody::create_new_file(&path(root, name), &private_file_sddl(), contents)
        .expect("a planted file");
}

/// The names under the temporary `%ProgramData%`.
fn program_data(root: &Root) -> Vec<String> {
    root.tree()
        .into_keys()
        .filter_map(|name| name.strip_prefix("ProgramData\\").map(str::to_string))
        .collect()
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

fn failing_at(at: Step) -> impl Fn(Step) -> Result<(), String> + Sync {
    move |step| {
        if step == at {
            Err(format!("injected failure at {step:?}"))
        } else {
            Ok(())
        }
    }
}

/// Replace the DACL of `path` with `sddl`'s, protected.
fn set_dacl(path: &Path, sddl: &str) {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{
        SetFileSecurityW, DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    };
    let text: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
    let name: Vec<u16> = path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let mut descriptor = std::ptr::null_mut();
    // SAFETY: a NUL-terminated SDDL string and an out-pointer freed below.
    assert_ne!(
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        },
        0,
        "SDDL {sddl}"
    );
    // SAFETY: a NUL-terminated name and the descriptor built above.
    let ok = unsafe {
        SetFileSecurityW(
            name.as_ptr(),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            descriptor,
        )
    };
    // SAFETY: the descriptor the conversion allocated.
    unsafe { LocalFree(descriptor) };
    assert_ne!(ok, 0, "SetFileSecurityW {}", path.display());
}

/// The steps of a staging route on a host with no `<P>`.
fn fresh_steps() -> Vec<Step> {
    vec![
        Step::Hold(Name::ProductRoot),
        Step::Create(Name::ProductRoot),
        Step::Hold(Name::DataRoot),
        Step::Create(Name::DataRoot),
        Step::RemoveEarlier(Name::StagedConfig),
        Step::RemoveEarlier(Name::StagedAnchor),
        Step::RemoveEarlier(Name::Record),
        Step::Write(Name::Record),
        Step::Write(Name::StagedAnchor),
        Step::Write(Name::StagedConfig),
    ]
}

fn all_created() -> BTreeSet<Created> {
    Created::ALL.into_iter().collect()
}

// --- The record --------------------------------------------------------------------

#[test]
fn package_staging_the_record_has_exactly_its_four_keys() {
    let staged = Staged::new();
    let record = Record {
        created: all_created(),
        fingerprints: staged.fingerprints.clone(),
        service_running: true,
    };
    let bytes = record.to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&bytes).expect("JSON");
    let keys: Vec<&String> = value.as_object().expect("an object").keys().collect();
    assert_eq!(
        keys,
        ["created", "fingerprints", "format", "service_running"],
        "{value}"
    );
    assert_eq!(value["format"], serde_json::json!(1));
    assert_eq!(
        value["created"],
        serde_json::json!([
            "product_root",
            "data_root",
            "staged_anchor",
            "staged_config"
        ])
    );
    assert_eq!(value["service_running"], serde_json::json!(true));
    assert_eq!(Record::parse(&bytes), Ok(record));

    let empty = Record {
        created: BTreeSet::new(),
        fingerprints: BTreeSet::new(),
        service_running: false,
    };
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&empty.to_bytes()).unwrap(),
        serde_json::json!({"format": 1, "created": [], "fingerprints": [], "service_running": false})
    );
    assert_eq!(Record::parse(&empty.to_bytes()), Ok(empty));
}

#[test]
fn package_staging_a_record_of_another_form_is_refused() {
    let fingerprint = Staged::new().fingerprints.into_iter().next().unwrap();
    let good = |created: &str, fingerprints: &str, running: &str| {
        format!(
            r#"{{"format":1,"created":{created},"fingerprints":{fingerprints},"service_running":{running}}}"#
        )
    };
    let quoted = format!("[{fingerprint:?}]");
    assert!(Record::parse(good(r#"["data_root"]"#, &quoted, "false").as_bytes()).is_ok());

    let padded = format!("{}{}", good("[]", "[]", "true"), " ".repeat(RECORD_LIMIT));
    let cases: Vec<(&str, String)> =
        vec![
        ("larger than 64 KiB", padded),
        ("not JSON", "{".to_string()),
        ("not an object", "[]".to_string()),
        (
            "a key missing",
            r#"{"format":1,"created":[],"fingerprints":[]}"#.to_string(),
        ),
        (
            "a key more",
            r#"{"format":1,"created":[],"fingerprints":[],"service_running":true,"path":"C:\\x"}"#
                .to_string(),
        ),
        (
            "format 2",
            r#"{"format":2,"created":[],"fingerprints":[],"service_running":true}"#.to_string(),
        ),
        (
            "format 1.0",
            r#"{"format":1.0,"created":[],"fingerprints":[],"service_running":true}"#.to_string(),
        ),
        (
            "format \"1\"",
            r#"{"format":"1","created":[],"fingerprints":[],"service_running":true}"#.to_string(),
        ),
        ("created not an array", good(r#""data_root""#, "[]", "true")),
        ("created a path", good(r#"["C:\\ProgramData"]"#, "[]", "true")),
        ("created another name", good(r#"["logs"]"#, "[]", "true")),
        ("created a number", good("[1]", "[]", "true")),
        ("fingerprints malformed", good("[]", r#"["sha256:00"]"#, "true")),
        (
            "fingerprints uppercase",
            good("[]", &format!("[{:?}]", fingerprint.to_uppercase()), "true"),
        ),
        ("service_running a string", good("[]", "[]", r#""true""#)),
        ("service_running null", good("[]", "[]", "null")),
        (
            "a key twice",
            r#"{"format":1,"created":[],"fingerprints":[],"service_running":true,"service_running":false}"#
                .to_string(),
        ),
    ];
    for (what, text) in cases {
        let reason = Record::parse(text.as_bytes()).expect_err(what);
        assert!(
            !reason.contains("C:\\"),
            "{what}: the reason quotes the file: {reason}"
        );
    }
}

// --- Item 5: the staging -----------------------------------------------------------

#[test]
fn package_staging_a_fresh_host_gets_the_directories_the_record_the_anchor_and_the_configuration_in_that_order(
) {
    let root = Root::bare();
    let staged = Staged::new();
    let recorder = Recorder::new();
    let attempt = stage(&setup(&root, &recorder), Some(staged.files()), false).expect("staged");
    assert_eq!(recorder.steps(), fresh_steps());
    let expected = Record {
        created: all_created(),
        fingerprints: staged.fingerprints.clone(),
        service_running: false,
    };
    assert_eq!(attempt.record(), &expected);
    drop(attempt);

    for name in [
        Name::ProductRoot,
        Name::DataRoot,
        Name::Record,
        Name::StagedAnchor,
        Name::StagedConfig,
    ] {
        assert!(has_final_security(&root, name), "{name:?}");
    }
    assert_eq!(
        read_record(&setup(&root, &quiet)).expect("readable"),
        Some(expected)
    );
    assert_eq!(
        std::fs::read(path(&root, Name::StagedAnchor)).unwrap(),
        staged.anchor
    );
    assert_eq!(
        std::fs::read_to_string(path(&root, Name::StagedConfig)).unwrap(),
        staged.config
    );
    // The record names no path and holds no secret.
    let record = std::fs::read_to_string(path(&root, Name::Record)).unwrap();
    assert!(!record.contains(TOKEN), "{record}");
    assert!(
        !record.contains('\\') && !record.contains("ProgramData"),
        "{record}"
    );
}

#[test]
fn package_staging_what_an_earlier_attempt_left_is_removed_first() {
    let root = Root::empty();
    let staged = Staged::new();
    plant_file(&root, Name::StagedAnchor, b"an earlier anchor");
    plant_file(&root, Name::StagedConfig, b"an earlier token");
    plant_file(&root, Name::Record, b"an earlier record");
    let attempt = stage(&setup(&root, &quiet), Some(staged.files()), true).expect("staged");
    // The directories were there: the record does not name them.
    assert_eq!(
        attempt.record().created,
        BTreeSet::from([Created::StagedAnchor, Created::StagedConfig])
    );
    assert!(attempt.record().service_running);
    drop(attempt);
    assert_eq!(
        std::fs::read(path(&root, Name::StagedAnchor)).unwrap(),
        staged.anchor
    );
    assert_eq!(
        std::fs::read_to_string(path(&root, Name::StagedConfig)).unwrap(),
        staged.config
    );
}

#[test]
fn package_staging_the_route_that_starts_writes_the_record_alone() {
    let root = Root::empty();
    root.touch("agent.toml");
    plant_file(&root, Name::StagedAnchor, b"an earlier anchor");
    plant_file(&root, Name::StagedConfig, b"an earlier token");
    let attempt = stage(&setup(&root, &quiet), None, true).expect("the record");
    let expected = Record {
        created: BTreeSet::new(),
        fingerprints: BTreeSet::new(),
        service_running: true,
    };
    assert_eq!(attempt.record(), &expected);
    drop(attempt);
    assert!(!exists(&path(&root, Name::StagedAnchor)));
    assert!(!exists(&path(&root, Name::StagedConfig)));
    assert_eq!(read_record(&setup(&root, &quiet)), Ok(Some(expected)));
    assert!(exists(&root.paths.config), "agent.toml is not the setup's");

    // That route creates nothing: with no data root it is refused.
    let bare = Root::bare();
    let before = bare.tree();
    let refusal = stage(&setup(&bare, &quiet), None, false).expect_err("no <P>");
    assert_eq!(refusal.code, codes::UNTRUSTED_PATH, "{}", refusal.line);
    assert_eq!(bare.tree(), before);
}

#[test]
fn package_staging_a_directory_under_a_staged_name_is_27_and_nothing_is_written() {
    for name in [Name::StagedConfig, Name::StagedAnchor, Name::Record] {
        let root = Root::empty();
        std::fs::create_dir(path(&root, name)).unwrap();
        let before = root.tree();
        let refusal =
            stage(&setup(&root, &quiet), Some(Staged::new().files()), false).expect_err("27");
        assert_eq!(
            refusal.code,
            codes::UNTRUSTED_PATH,
            "{name:?}: {}",
            refusal.line
        );
        assert_eq!(root.tree(), before, "{name:?}");
    }

    // A junction under a staged name is not followed: what it points at stays.
    let root = Root::empty();
    let elsewhere = root.operator.join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    std::fs::write(elsewhere.join("kept"), b"not the setup's").unwrap();
    junction(&path(&root, Name::StagedConfig), &elsewhere);
    let refusal = stage(&setup(&root, &quiet), Some(Staged::new().files()), false).expect_err("");
    assert_eq!(refusal.code, codes::UNTRUSTED_PATH, "{}", refusal.line);
    assert!(elsewhere.join("kept").exists());
    assert!(!exists(&path(&root, Name::Record)));
}

#[test]
fn package_staging_directories_that_fail_the_check_are_27() {
    // <P> that ordinary users may create in.
    let root = Root::bare();
    root.directories(&open_directory_sddl());
    let before = root.tree();
    let refusal = stage(&setup(&root, &quiet), Some(Staged::new().files()), false).expect_err("");
    assert_eq!(refusal.code, codes::UNTRUSTED_PATH, "{}", refusal.line);
    assert_eq!(root.tree(), before);

    // A data root that a file holds the name of.
    let root = Root::bare();
    custody::create_new_directory(&root.paths.product_root, &closed_directory_sddl()).unwrap();
    std::fs::write(&root.paths.data_root, b"x").unwrap();
    let refusal = stage(&setup(&root, &quiet), Some(Staged::new().files()), false).expect_err("");
    assert_eq!(refusal.code, codes::UNTRUSTED_PATH, "{}", refusal.line);
}

/// A junction planted after the host was read, at `<P>` or at the data
/// root, is refused through the held handle, and nothing is written where it
/// points.
#[test]
fn package_staging_a_junction_planted_after_the_read_is_27() {
    for at in [Name::ProductRoot, Name::DataRoot] {
        let root = Root::empty();
        let elsewhere = root.operator.join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        let moved = root.operator.join("moved");
        let target = path(&root, at);
        let plant = |step: Step| {
            if step == Step::Hold(at) {
                std::fs::rename(&target, &moved).expect("move the directory away");
                junction(&target, &elsewhere);
            }
            Ok(())
        };
        let refusal =
            stage(&setup(&root, &plant), Some(Staged::new().files()), false).expect_err("27");
        assert_eq!(
            refusal.code,
            codes::UNTRUSTED_PATH,
            "{at:?}: {}",
            refusal.line
        );
        assert!(refusal.line.contains("reparse point"), "{}", refusal.line);
        assert_eq!(
            std::fs::read_dir(&elsewhere).unwrap().count(),
            0,
            "{at:?}: something was written through the junction"
        );
    }
}

/// Once held, `<P>` and the data root cannot be renamed until the attempt
/// ends, in the staging and in the reversal.
#[test]
fn package_staging_held_directories_cannot_be_renamed() {
    let root = Root::empty();
    let moved = root.operator.join("moved");
    let renamed = Mutex::new(Vec::new());
    let try_renaming = |step: Step| {
        if matches!(
            step,
            Step::RemoveEarlier(Name::StagedConfig) | Step::ReadRecord
        ) {
            for name in [Name::DataRoot, Name::ProductRoot] {
                let outcome = std::fs::rename(path(&root, name), &moved);
                renamed.lock().unwrap().push((step, name, outcome.is_ok()));
            }
        }
        Ok(())
    };
    let attempt = stage(
        &setup(&root, &try_renaming),
        Some(Staged::new().files()),
        false,
    )
    .expect("staged");
    // Released once the attempt ends.
    drop(attempt);
    assert_eq!(
        reverse(&setup(&root, &try_renaming)),
        Ok(Reversal::Removed { start: false })
    );
    let renamed = renamed.into_inner().unwrap();
    assert_eq!(renamed.len(), 4, "{renamed:?}");
    assert!(
        renamed.iter().all(|(_, _, ok)| !ok),
        "a held directory was renamed: {renamed:?}"
    );
}

/// A name that is there by the time the setup creates the directory.
#[test]
fn package_staging_a_directory_is_created_new() {
    let root = Root::bare();
    let data_root = root.paths.data_root.clone();
    let plant = |step: Step| {
        if step == Step::Create(Name::DataRoot) {
            std::fs::create_dir(&data_root).unwrap();
        }
        Ok(())
    };
    let refusal = stage(&setup(&root, &plant), Some(Staged::new().files()), false).expect_err("");
    assert_eq!(refusal.code, codes::UNTRUSTED_PATH, "{}", refusal.line);
    // Before the record, what was created stays, with its final security.
    assert!(has_final_security(&root, Name::ProductRoot));
    assert!(!exists(&path(&root, Name::Record)));
}

#[test]
fn package_staging_a_failure_at_each_step_leaves_what_item_5_says() {
    for at in fresh_steps() {
        let root = Root::bare();
        let refusal = stage(
            &setup(&root, &failing_at(at)),
            Some(Staged::new().files()),
            false,
        )
        .expect_err("injected");
        let expected = match at {
            Step::Hold(_) => codes::UNTRUSTED_PATH,
            _ => codes::OWN_STATE,
        };
        assert_eq!(refusal.code, expected, "{at:?}: {}", refusal.line);
        assert!(
            refusal.line.contains("injected failure"),
            "{}",
            refusal.line
        );
        fixtures::assert_clean(&refusal.line);
        let left = program_data(&root);
        match at {
            // Before the record: the directories created stay, empty.
            Step::Hold(Name::ProductRoot) | Step::Create(Name::ProductRoot) => {
                assert!(left.is_empty(), "{at:?}: {left:?}")
            }
            Step::Hold(Name::DataRoot) | Step::Create(Name::DataRoot) => {
                assert_eq!(left, ["CyberGuard"], "{at:?}")
            }
            Step::RemoveEarlier(_) => {
                assert_eq!(left, ["CyberGuard", "CyberGuard\\agent"], "{at:?}");
                assert!(has_final_security(&root, Name::DataRoot));
            }
            // From the record on: what it names is removed.
            _ => assert!(left.is_empty(), "{at:?}: {left:?}"),
        }
    }

    // A host whose directories were there keeps them, and its own files.
    let root = Root::empty();
    root.touch("agent.toml");
    let refusal = stage(
        &setup(&root, &failing_at(Step::Write(Name::StagedConfig))),
        Some(Staged::new().files()),
        false,
    )
    .expect_err("injected");
    assert_eq!(refusal.code, codes::OWN_STATE);
    assert_eq!(
        program_data(&root),
        [
            "CyberGuard",
            "CyberGuard\\agent",
            "CyberGuard\\agent\\agent.toml"
        ]
    );
}

#[test]
fn package_staging_after_a_crash_at_each_step_the_next_preflight_proceeds() {
    for at in fresh_steps() {
        let root = Root::bare();
        let crash = move |step: Step| -> Result<(), String> {
            if step == at {
                panic!("crash at {step:?}");
            }
            Ok(())
        };
        let crashed = catch_unwind(AssertUnwindSafe(|| {
            stage(&setup(&root, &crash), Some(Staged::new().files()), false)
        }));
        assert!(crashed.is_err(), "{at:?}");
        let staged = Staged::new();
        let attempt = stage(&setup(&root, &quiet), Some(staged.files()), true)
            .unwrap_or_else(|refusal| panic!("after a crash at {at:?}: {}", refusal.line));
        assert!(attempt.record().service_running);
        drop(attempt);
        assert_eq!(
            std::fs::read_to_string(path(&root, Name::StagedConfig)).unwrap(),
            staged.config,
            "{at:?}"
        );
    }
}

#[test]
fn package_staging_a_refusal_after_the_record_removes_what_it_names_in_order() {
    let root = Root::bare();
    let recorder = Recorder::new();
    let attempt = stage(&setup(&root, &recorder), Some(Staged::new().files()), true).unwrap();
    let refusal = attempt.refuse(
        &setup(&root, &recorder),
        Refusal::new(codes::NOT_STOPPED, "not stopped"),
    );
    assert_eq!(refusal, Refusal::new(codes::NOT_STOPPED, "not stopped"));
    assert_eq!(
        recorder.steps()[fresh_steps().len()..],
        [
            Step::Remove(Name::StagedConfig),
            Step::Remove(Name::StagedAnchor),
            Step::Remove(Name::Record),
            Step::Remove(Name::DataRoot),
            Step::Remove(Name::ProductRoot),
        ]
    );
    assert!(program_data(&root).is_empty(), "{:?}", program_data(&root));

    // A directory is removed only if it is empty.
    let root = Root::bare();
    let attempt = stage(&setup(&root, &quiet), Some(Staged::new().files()), false).unwrap();
    std::fs::create_dir(root.paths.data_root.join("logs")).unwrap();
    attempt.refuse(&setup(&root, &quiet), Refusal::new(codes::NOT_STOPPED, ""));
    assert_eq!(
        program_data(&root),
        ["CyberGuard", "CyberGuard\\agent", "CyberGuard\\agent\\logs"]
    );

    // Only what it names: a pending host's directories and agent.toml stay.
    let root = Root::empty();
    root.touch("agent.toml");
    let attempt = stage(&setup(&root, &quiet), Some(Staged::new().files()), false).unwrap();
    attempt.refuse(&setup(&root, &quiet), Refusal::new(codes::NOT_STOPPED, ""));
    assert_eq!(
        program_data(&root),
        [
            "CyberGuard",
            "CyberGuard\\agent",
            "CyberGuard\\agent\\agent.toml"
        ]
    );

    // A removal that fails keeps the code and names it; the record stays
    // while a staged file does.
    let root = Root::bare();
    let attempt = stage(&setup(&root, &quiet), Some(Staged::new().files()), false).unwrap();
    let refusal = attempt.refuse(
        &setup(&root, &failing_at(Step::Remove(Name::StagedAnchor))),
        Refusal::new(codes::NOT_STOPPED, "not stopped"),
    );
    assert_eq!(refusal.code, codes::NOT_STOPPED);
    assert!(
        refusal.line.contains("injected failure"),
        "{}",
        refusal.line
    );
    assert!(!exists(&path(&root, Name::StagedConfig)));
    assert!(exists(&path(&root, Name::StagedAnchor)));
    assert!(exists(&path(&root, Name::Record)));
}

// --- Item 7: the reversal ----------------------------------------------------------

/// A host as a staging attempt leaves it, held no more.
fn staged_host(root: &Root, running: bool) {
    let attempt = stage(&setup(root, &quiet), Some(Staged::new().files()), running).unwrap();
    drop(attempt);
}

#[test]
fn package_reversal_changes_nothing_where_preflight_would_not_stage() {
    type Plant = fn(&Root);
    let cases: Vec<(&str, Plant)> = vec![
        ("no <P>", |_| {}),
        ("no data root", |root| {
            custody::create_new_directory(&root.paths.product_root, &closed_directory_sddl())
                .unwrap()
        }),
        ("an untrusted <P>", |root| {
            staged_host(root, true);
            set_dacl(&root.paths.product_root, &open_directory_sddl());
        }),
        ("a reparse point at <P>", |root| {
            let elsewhere = root.operator.join("elsewhere");
            std::fs::create_dir(&elsewhere).unwrap();
            junction(&root.paths.product_root, &elsewhere);
        }),
        ("a migration in progress", |root| {
            staged_host(root, true);
            std::fs::create_dir(root.paths.staging()).unwrap();
        }),
        ("a console layout", |root| {
            staged_host(root, true);
            root.touch("identity.json");
        }),
    ];
    for (what, plant) in cases {
        let root = Root::bare();
        plant(&root);
        let before = root.tree();
        let reversal = reverse(&setup(&root, &quiet)).expect(what);
        assert!(
            matches!(reversal, Reversal::Unchanged(_)),
            "{what}: {reversal:?}"
        );
        assert_eq!(root.tree(), before, "{what}");
    }
}

#[test]
fn package_reversal_of_a_host_whose_names_cannot_be_read_is_40() {
    for at in [Name::ProductRoot, Name::DataRoot] {
        let root = Root::bare();
        staged_host(&root, true);
        let before = root.tree();
        set_dacl(&path(&root, at), "D:P");
        let result = reverse(&setup(&root, &quiet));
        set_dacl(&path(&root, at), &closed_directory_sddl());
        let refusal = result.expect_err("40");
        assert_eq!(refusal.code, codes::OWN_STATE, "{at:?}: {}", refusal.line);
        assert_eq!(root.tree(), before, "{at:?}");
    }

    // In the order of the states: a migration in progress is read first.
    let root = Root::bare();
    staged_host(&root, true);
    std::fs::create_dir(root.paths.staging()).unwrap();
    set_dacl(&root.paths.data_root, "D:P");
    let result = reverse(&setup(&root, &quiet));
    set_dacl(&root.paths.data_root, &closed_directory_sddl());
    assert!(matches!(result, Ok(Reversal::Unchanged(_))), "{result:?}");
}

#[test]
fn package_reversal_removes_what_the_record_names_and_says_whether_to_start() {
    for running in [true, false] {
        let root = Root::bare();
        staged_host(&root, running);
        let recorder = Recorder::new();
        assert_eq!(
            reverse(&setup(&root, &recorder)),
            Ok(Reversal::Removed { start: running })
        );
        assert_eq!(
            recorder.steps(),
            [
                Step::Hold(Name::ProductRoot),
                Step::Hold(Name::DataRoot),
                Step::ReadRecord,
                Step::Remove(Name::StagedConfig),
                Step::Remove(Name::StagedAnchor),
                Step::Remove(Name::Record),
                Step::Remove(Name::DataRoot),
                Step::Remove(Name::ProductRoot),
            ]
        );
        assert!(program_data(&root).is_empty(), "{:?}", program_data(&root));
    }

    // The route that started: the record alone, and the host's own files stay.
    let root = Root::empty();
    root.touch("agent.toml");
    drop(stage(&setup(&root, &quiet), None, true).unwrap());
    assert_eq!(
        reverse(&setup(&root, &quiet)),
        Ok(Reversal::Removed { start: true })
    );
    assert_eq!(
        program_data(&root),
        [
            "CyberGuard",
            "CyberGuard\\agent",
            "CyberGuard\\agent\\agent.toml"
        ]
    );

    // A directory the record names is removed only if it is empty.
    let root = Root::bare();
    staged_host(&root, false);
    std::fs::create_dir(root.paths.data_root.join("logs")).unwrap();
    assert_eq!(
        reverse(&setup(&root, &quiet)),
        Ok(Reversal::Removed { start: false })
    );
    assert_eq!(
        program_data(&root),
        ["CyberGuard", "CyberGuard\\agent", "CyberGuard\\agent\\logs"]
    );
}

#[test]
fn package_reversal_without_a_record_changes_nothing() {
    let root = Root::empty();
    plant_file(&root, Name::StagedAnchor, b"anchor");
    plant_file(&root, Name::StagedConfig, b"token");
    let before = root.tree();
    let reversal = reverse(&setup(&root, &quiet)).expect("0");
    assert!(matches!(reversal, Reversal::Unchanged(_)), "{reversal:?}");
    assert_eq!(root.tree(), before);
}

#[test]
fn package_reversal_of_a_record_that_fails_its_checks_removes_the_staged_files_and_is_40() {
    type Spoil = fn(&Root);
    let cases: Vec<(&str, Spoil)> = vec![
        ("not its form", |root| {
            std::fs::remove_file(path(root, Name::Record)).unwrap();
            plant_file(root, Name::Record, b"{\"format\":2}");
        }),
        ("readable by users", |root| {
            set_dacl(
                &path(root, Name::Record),
                &format!("{}(A;;FR;;;BU)", private_file_sddl()),
            );
        }),
        ("hard-linked", |root| {
            std::fs::hard_link(
                path(root, Name::Record),
                root.paths.data_root.join("another-name"),
            )
            .unwrap();
        }),
    ];
    for (what, spoil) in cases {
        let root = Root::bare();
        staged_host(&root, true);
        spoil(&root);
        let refusal = reverse(&setup(&root, &quiet)).expect_err(what);
        assert_eq!(refusal.code, codes::OWN_STATE, "{what}: {}", refusal.line);
        for name in [Name::StagedConfig, Name::StagedAnchor, Name::Record] {
            assert!(!exists(&path(&root, name)), "{what}: {name:?} stays");
        }
        // Starts nothing, and the directories it may name stay.
        assert!(exists(&root.paths.data_root), "{what}");
    }

    // A directory under the record's name is left, with 40.
    let root = Root::bare();
    staged_host(&root, true);
    std::fs::remove_file(path(&root, Name::Record)).unwrap();
    std::fs::create_dir(path(&root, Name::Record)).unwrap();
    let refusal = reverse(&setup(&root, &quiet)).expect_err("40");
    assert_eq!(refusal.code, codes::OWN_STATE, "{}", refusal.line);
    assert!(path(&root, Name::Record).is_dir());
    assert!(!exists(&path(&root, Name::StagedConfig)));
}

#[test]
fn package_reversal_a_directory_under_a_named_file_or_a_failed_deletion_is_40() {
    let root = Root::bare();
    staged_host(&root, true);
    std::fs::remove_file(path(&root, Name::StagedAnchor)).unwrap();
    std::fs::create_dir(path(&root, Name::StagedAnchor)).unwrap();
    let refusal = reverse(&setup(&root, &quiet)).expect_err("40");
    assert_eq!(refusal.code, codes::OWN_STATE, "{}", refusal.line);
    assert!(path(&root, Name::StagedAnchor).is_dir());
    // The record stays while a staged name does.
    assert!(exists(&path(&root, Name::Record)));

    for at in [
        Step::Remove(Name::StagedConfig),
        Step::Remove(Name::Record),
        Step::Remove(Name::DataRoot),
    ] {
        let root = Root::bare();
        staged_host(&root, true);
        let refusal = reverse(&setup(&root, &failing_at(at))).expect_err("40");
        assert_eq!(refusal.code, codes::OWN_STATE, "{at:?}: {}", refusal.line);
        assert!(
            refusal.line.contains("injected failure"),
            "{}",
            refusal.line
        );
        assert!(exists(&root.paths.data_root), "{at:?}");
    }
}

/// A junction planted at the data root after the read: the held handle
/// refuses it, nothing changes, and what it points at is not touched.
#[test]
fn package_reversal_a_junction_planted_after_the_read_changes_nothing() {
    let root = Root::bare();
    staged_host(&root, true);
    let elsewhere = root.operator.join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    for name in [RECORD, "agent.toml.setup", "server-ca.pem.setup"] {
        std::fs::write(elsewhere.join(name), b"not the setup's").unwrap();
    }
    let data_root = root.paths.data_root.clone();
    let moved = root.operator.join("moved");
    let plant = |step: Step| {
        if step == Step::Hold(Name::DataRoot) {
            std::fs::rename(&data_root, &moved).unwrap();
            junction(&data_root, &elsewhere);
        }
        Ok(())
    };
    let reversal = reverse(&setup(&root, &plant)).expect("0");
    assert!(matches!(reversal, Reversal::Unchanged(_)), "{reversal:?}");
    assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 3);
}

// --- Further steps, from the self-review ---------------------------------------------

/// A record that would hold more than 64 KiB is not written: nothing is left.
#[test]
fn package_staging_a_record_larger_than_64_kib_is_not_written() {
    let root = Root::bare();
    let many: BTreeSet<String> = (0..900u32)
        .map(|n| format!("sha256:{}", format!("{n:08x}").repeat(8)))
        .collect();
    let staged = Staged::new();
    let files = Files {
        fingerprints: &many,
        ..staged.files()
    };
    let refusal = stage(&setup(&root, &quiet), Some(files), false).expect_err("40");
    assert_eq!(refusal.code, codes::OWN_STATE, "{}", refusal.line);
    assert!(program_data(&root).is_empty(), "{:?}", program_data(&root));
}

/// `<P>` too is created new.
#[test]
fn package_staging_a_name_at_the_product_root_by_its_creation_is_27() {
    let root = Root::bare();
    let product_root = root.paths.product_root.clone();
    let plant = |step: Step| {
        if step == Step::Create(Name::ProductRoot) {
            std::fs::create_dir(&product_root).unwrap();
        }
        Ok(())
    };
    let refusal = stage(&setup(&root, &plant), Some(Staged::new().files()), false).expect_err("");
    assert_eq!(refusal.code, codes::UNTRUSTED_PATH, "{}", refusal.line);
    assert_eq!(program_data(&root), ["CyberGuard"]);
}

/// A crash during the removal, between the record's removal and a
/// directory's included, leaves what the next preflight accepts.
#[test]
fn package_staging_after_a_crash_during_the_removal_the_next_preflight_proceeds() {
    for at in [
        Step::Remove(Name::StagedConfig),
        Step::Remove(Name::StagedAnchor),
        Step::Remove(Name::Record),
        Step::Remove(Name::DataRoot),
        Step::Remove(Name::ProductRoot),
    ] {
        let root = Root::bare();
        let attempt = stage(&setup(&root, &quiet), Some(Staged::new().files()), false).unwrap();
        let crash = move |step: Step| -> Result<(), String> {
            if step == at {
                panic!("crash at {step:?}");
            }
            Ok(())
        };
        let crashed = catch_unwind(AssertUnwindSafe(|| {
            attempt.refuse(&setup(&root, &crash), Refusal::new(codes::NOT_STOPPED, ""))
        }));
        assert!(crashed.is_err(), "{at:?}");
        let staged = Staged::new();
        let attempt = stage(&setup(&root, &quiet), Some(staged.files()), false)
            .unwrap_or_else(|refusal| panic!("after a crash at {at:?}: {}", refusal.line));
        drop(attempt);
        assert_eq!(
            std::fs::read_to_string(path(&root, Name::StagedConfig)).unwrap(),
            staged.config,
            "{at:?}"
        );
    }
}

/// A failure at each step of the reversal: a hold that fails changes nothing;
/// a failed read is a refused record; a failed removal is 40, and what comes
/// after it stays.
#[test]
fn package_reversal_a_failure_at_each_step() {
    for at in [Step::Hold(Name::ProductRoot), Step::Hold(Name::DataRoot)] {
        let root = Root::bare();
        staged_host(&root, true);
        let before = root.tree();
        let reversal = reverse(&setup(&root, &failing_at(at))).expect("0");
        assert!(
            matches!(reversal, Reversal::Unchanged(_)),
            "{at:?}: {reversal:?}"
        );
        assert_eq!(root.tree(), before, "{at:?}");
    }

    let root = Root::bare();
    staged_host(&root, true);
    let refusal = reverse(&setup(&root, &failing_at(Step::ReadRecord))).expect_err("40");
    assert_eq!(refusal.code, codes::OWN_STATE, "{}", refusal.line);
    assert_eq!(program_data(&root), ["CyberGuard", "CyberGuard\\agent"]);

    let order = [
        Name::StagedConfig,
        Name::StagedAnchor,
        Name::Record,
        Name::DataRoot,
        Name::ProductRoot,
    ];
    for (index, at) in order.into_iter().enumerate() {
        let root = Root::bare();
        staged_host(&root, true);
        let refusal = reverse(&setup(&root, &failing_at(Step::Remove(at)))).expect_err("40");
        assert_eq!(refusal.code, codes::OWN_STATE, "{at:?}: {}", refusal.line);
        assert!(
            refusal.line.contains("injected failure"),
            "{}",
            refusal.line
        );
        for (other, name) in order.into_iter().enumerate() {
            assert_eq!(
                exists(&path(&root, name)),
                other >= index,
                "a failure at {at:?}: {name:?}"
            );
        }
    }
}

/// With a refused record each name is tried: only the one holding a
/// directory is left.
#[test]
fn package_reversal_of_a_refused_record_tries_every_name() {
    let root = Root::bare();
    staged_host(&root, true);
    std::fs::remove_file(path(&root, Name::Record)).unwrap();
    plant_file(&root, Name::Record, b"{\"format\":2}");
    std::fs::remove_file(path(&root, Name::StagedConfig)).unwrap();
    std::fs::create_dir(path(&root, Name::StagedConfig)).unwrap();
    let refusal = reverse(&setup(&root, &quiet)).expect_err("40");
    assert_eq!(refusal.code, codes::OWN_STATE, "{}", refusal.line);
    assert!(path(&root, Name::StagedConfig).is_dir());
    assert!(!exists(&path(&root, Name::StagedAnchor)));
    assert!(!exists(&path(&root, Name::Record)));
}

/// More records that fail their checks: an unprotected DACL, more than 64
/// KiB, and a reparse point under the record's name, which is not followed
/// and is left.
#[test]
fn package_reversal_more_records_that_fail_their_checks() {
    let root = Root::bare();
    staged_host(&root, true);
    let bytes = std::fs::read(path(&root, Name::Record)).unwrap();
    std::fs::remove_file(path(&root, Name::Record)).unwrap();
    // Inherited from the data root: not protected.
    std::fs::write(path(&root, Name::Record), &bytes).unwrap();
    let refusal = reverse(&setup(&root, &quiet)).expect_err("unprotected");
    assert!(refusal.line.contains("not protected"), "{}", refusal.line);
    assert!(!exists(&path(&root, Name::Record)));

    let root = Root::bare();
    staged_host(&root, true);
    std::fs::remove_file(path(&root, Name::Record)).unwrap();
    let mut large = Record {
        created: all_created(),
        fingerprints: BTreeSet::new(),
        service_running: true,
    }
    .to_bytes();
    large.extend(std::iter::repeat_n(b' ', RECORD_LIMIT));
    plant_file(&root, Name::Record, &large);
    let refusal = reverse(&setup(&root, &quiet)).expect_err("larger");
    assert_eq!(refusal.code, codes::OWN_STATE);
    assert!(!exists(&path(&root, Name::Record)));

    let root = Root::bare();
    staged_host(&root, true);
    let elsewhere = root.operator.join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    std::fs::write(elsewhere.join("kept"), b"not the setup's").unwrap();
    std::fs::remove_file(path(&root, Name::Record)).unwrap();
    junction(&path(&root, Name::Record), &elsewhere);
    let refusal = reverse(&setup(&root, &quiet)).expect_err("a reparse point");
    assert_eq!(refusal.code, codes::OWN_STATE, "{}", refusal.line);
    assert!(elsewhere.join("kept").exists());
    assert!(exists(&path(&root, Name::Record)), "the junction is left");
    assert!(!exists(&path(&root, Name::StagedConfig)));
}

/// `<P>`, held, fails the check: nothing changes.
#[test]
fn package_reversal_a_held_product_root_that_fails_the_check_changes_nothing() {
    let root = Root::bare();
    staged_host(&root, true);
    let product_root = root.paths.product_root.clone();
    let widen = |step: Step| {
        if step == Step::Hold(Name::ProductRoot) {
            set_dacl(&product_root, &open_directory_sddl());
        }
        Ok(())
    };
    let reversal = reverse(&setup(&root, &widen)).expect("0");
    assert!(matches!(reversal, Reversal::Unchanged(_)), "{reversal:?}");
    for name in [Name::StagedConfig, Name::StagedAnchor, Name::Record] {
        assert!(exists(&path(&root, name)), "{name:?}");
    }
}
