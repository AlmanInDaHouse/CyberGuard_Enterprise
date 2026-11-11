//! Preflight's states and their codes (SPEC-024 Amendment 2026-11-09, items
//! 3 and 7): a precondition of the package's criteria (install_ac_001,
//! install_ac_011, install_ac_013's refusal by default), not those criteria.
//! The table of item 3 for each state and request; the states read from a
//! temporary host by the service's own `classify`, with the service's check
//! of the directories; an enrolled host refusing a bootstrap without reading
//! it; the migration route reading no file under `<P>`; nothing changed; and
//! the platform and elevation checks.
#![cfg(windows)]

mod fixtures;

use std::os::windows::fs::OpenOptionsExt as _;

use cg_agent::custody::{Policy, Principal};
use cg_agent::service::layout::HostState;
use cg_agent::service::migration::Choice;
use cg_agent::service::InstalledPaths;
use cg_setup::codes;
use cg_setup::platform::{check_elevation, check_platform};
use cg_setup::preflight::{assess, decide, parse_migration, Decision, Host, Plan, Request};

use fixtures::{assert_clean, bootstrap_text, certificate, open_directory_sddl, set, Root};

/// A fingerprint whose digits hold letters: `sha256:` and `a<n>` 32 times.
fn fingerprint(n: u8) -> String {
    format!("sha256:{}", format!("{:02x}", 0xa0 | (n & 0x0f)).repeat(32))
}

fn bootstrap_request() -> Request {
    Request::new(Some("C:\\operator\\bootstrap.toml"), None, None)
}

fn migration_request() -> Request {
    Request::new(None, Some("keep"), Some(&fingerprint(1)))
}

// --- The request ------------------------------------------------------------------

#[test]
fn package_preflight_an_empty_variable_is_not_given() {
    assert_eq!(
        Request::new(Some(""), Some(""), Some("")),
        Request::default()
    );
    assert!(!Request::default().migration_requested());
    assert!(Request::new(None, Some("keep"), None).migration_requested());
    assert!(Request::new(None, None, Some(&fingerprint(1))).migration_requested());
    assert!(!bootstrap_request().migration_requested());
    assert_eq!(
        bootstrap_request().bootstrap.as_deref(),
        Some(std::path::Path::new("C:\\operator\\bootstrap.toml"))
    );
}

#[test]
fn package_preflight_the_migration_variables_have_the_services_form() {
    let (one, two) = (fingerprint(1), fingerprint(2));
    let parsed =
        parse_migration(Some("remove"), Some(&format!("{one},{two},{one}"))).expect("well formed");
    assert_eq!(parsed.choice, Choice::Remove);
    assert_eq!(parsed.fingerprints, set(&[&one, &two]));
    assert_eq!(
        parse_migration(Some("keep"), Some(&one))
            .expect("keep")
            .choice,
        Choice::Keep
    );
    for (mode, anchors, naming) in [
        (Some("Keep"), Some(one.as_str()), "CG_MIGRATE_LEGACY"),
        (Some("delete"), Some(one.as_str()), "CG_MIGRATE_LEGACY"),
        (None, Some(one.as_str()), "CG_MIGRATE_LEGACY"),
        (Some("keep"), None, "CG_TRUST_ANCHORS"),
        (Some("keep"), Some(""), "CG_TRUST_ANCHORS"),
        (Some("keep"), Some(&*format!("{one},")), "CG_TRUST_ANCHORS"),
        (
            Some("keep"),
            Some(&*format!("{one}, {two}")),
            "CG_TRUST_ANCHORS",
        ),
        (
            Some("keep"),
            Some(&*format!("sha256:{}", one[7..].to_uppercase())),
            "CG_TRUST_ANCHORS holds a value",
        ),
        (Some("keep"), Some(&one[7..]), "CG_TRUST_ANCHORS"),
    ] {
        let reason = parse_migration(mode, anchors).expect_err(naming);
        assert!(reason.contains(naming), "{naming}: {reason}");
    }
}

// --- Item 3's table -----------------------------------------------------------------

/// Every state, with each request: the decision or the code.
#[test]
fn package_preflight_each_state_takes_what_item_3_says() {
    let untrusted = HostState::Untrusted("C:\\x is a reparse point".to_string());
    let malformed = Request::new(None, Some("keep"), Some("sha256:00"));
    let both = Request::new(
        Some("C:\\operator\\bootstrap.toml"),
        Some("keep"),
        Some(&fingerprint(1)),
    );
    let anchors_alone = Request::new(None, None, Some(&fingerprint(1)));
    let migrate = Decision::Migrate(parse_migration(Some("keep"), Some(&fingerprint(1))).unwrap());
    let stage = Decision::Stage("C:\\operator\\bootstrap.toml".into());
    let none = Request::default();
    let (bootstrap, migration) = (bootstrap_request(), migration_request());

    let expected: Vec<(&HostState, &Request, Result<Decision, u8>)> = vec![
        // Untrusted: refused whatever is asked.
        (&untrusted, &none, Err(codes::UNTRUSTED_PATH)),
        (&untrusted, &bootstrap, Err(codes::UNTRUSTED_PATH)),
        (&untrusted, &migration, Err(codes::UNTRUSTED_PATH)),
        // A migration in progress and a console layout: the migration route
        // alone, its variables in their form.
        (
            &HostState::MigrationInProgress,
            &migration,
            Ok(migrate.clone()),
        ),
        (
            &HostState::MigrationInProgress,
            &none,
            Err(codes::CONSOLE_LAYOUT),
        ),
        (
            &HostState::MigrationInProgress,
            &bootstrap,
            Err(codes::CONSOLE_LAYOUT),
        ),
        (
            &HostState::MigrationInProgress,
            &both,
            Err(codes::CONSOLE_LAYOUT),
        ),
        (
            &HostState::MigrationInProgress,
            &malformed,
            Err(codes::INVALID_INPUT),
        ),
        (
            &HostState::MigrationInProgress,
            &anchors_alone,
            Err(codes::INVALID_INPUT),
        ),
        (&HostState::ConsoleLayout, &migration, Ok(migrate.clone())),
        (&HostState::ConsoleLayout, &none, Err(codes::CONSOLE_LAYOUT)),
        (
            &HostState::ConsoleLayout,
            &bootstrap,
            Err(codes::CONSOLE_LAYOUT),
        ),
        (&HostState::ConsoleLayout, &both, Err(codes::CONSOLE_LAYOUT)),
        (
            &HostState::ConsoleLayout,
            &malformed,
            Err(codes::INVALID_INPUT),
        ),
        // Any other state: a migration is refused, well formed or not.
        (&HostState::Enrolled, &migration, Err(codes::CONSOLE_LAYOUT)),
        (&HostState::Enrolled, &malformed, Err(codes::CONSOLE_LAYOUT)),
        (&HostState::Enrolled, &both, Err(codes::CONSOLE_LAYOUT)),
        (
            &HostState::Enrolled,
            &bootstrap,
            Err(codes::ALREADY_CONFIGURED),
        ),
        (&HostState::Enrolled, &none, Ok(Decision::Start)),
        (&HostState::Pending, &migration, Err(codes::CONSOLE_LAYOUT)),
        (&HostState::Pending, &bootstrap, Ok(stage.clone())),
        (&HostState::Pending, &none, Ok(Decision::Start)),
        (&HostState::Fresh, &migration, Err(codes::CONSOLE_LAYOUT)),
        (
            &HostState::Fresh,
            &anchors_alone,
            Err(codes::CONSOLE_LAYOUT),
        ),
        (&HostState::Fresh, &bootstrap, Ok(stage.clone())),
        (&HostState::Fresh, &none, Err(codes::NO_INPUT)),
    ];
    for (state, request, outcome) in expected {
        let got = decide(state, request);
        match (&got, &outcome) {
            (Ok(decision), Ok(want)) => assert_eq!(decision, want, "{state:?} {request:?}"),
            (Err(refusal), Err(code)) => {
                assert_eq!(
                    refusal.code, *code,
                    "{state:?} {request:?}: {}",
                    refusal.line
                );
                assert!(refusal.line.starts_with("cg-setup: "), "{}", refusal.line);
            }
            _ => panic!("{state:?} {request:?}: {got:?}, expected {outcome:?}"),
        }
    }
}

#[test]
fn package_preflight_an_untrusted_state_names_its_reason() {
    let refusal = decide(
        &HostState::Untrusted("C:\\P is a reparse point".to_string()),
        &bootstrap_request(),
    )
    .expect_err("untrusted");
    assert!(
        refusal.line.contains("C:\\P is a reparse point"),
        "{}",
        refusal.line
    );
}

// --- The states, read from the host ---------------------------------------------------

/// `assess` with a request that has no bootstrap, on a host: the code, or the
/// plan; the host left as it was.
fn assess_unchanged(root: &Root, request: &Request) -> Result<Plan, u8> {
    let before = root.tree();
    let outcome = assess(&root.host, request).map_err(|refusal| {
        assert_clean(&refusal.line);
        refusal.code
    });
    assert_eq!(root.tree(), before, "assess changes nothing");
    outcome
}

#[test]
fn package_preflight_states_are_read_with_the_services_classification() {
    // Fresh, and no input: 22.
    let root = Root::bare();
    assert_eq!(
        assess_unchanged(&root, &Request::default()).err(),
        Some(codes::NO_INPUT)
    );

    // Pending, and no input: started with no argument.
    let root = Root::empty();
    root.touch("agent.toml");
    assert!(matches!(
        assess_unchanged(&root, &Request::default()),
        Ok(Plan::Start)
    ));

    // Enrolled: started with no argument, or 30 with a bootstrap.
    let root = Root::empty();
    std::fs::create_dir(root.paths.data_root.join("identity")).expect("identity");
    assert!(matches!(
        assess_unchanged(&root, &Request::default()),
        Ok(Plan::Start)
    ));

    // A console layout: 28 by default, the migration route when asked.
    let root = Root::empty();
    root.touch("cert.pem");
    assert_eq!(
        assess_unchanged(&root, &Request::default()).err(),
        Some(codes::CONSOLE_LAYOUT)
    );
    match assess_unchanged(&root, &migration_request()) {
        Ok(Plan::Migrate(request)) => assert_eq!(request.choice, Choice::Keep),
        other => panic!("the migration route: {other:?}"),
    }

    // A migration in progress.
    let root = Root::empty();
    std::fs::create_dir(root.paths.staging()).expect("agent.migrating");
    assert_eq!(
        assess_unchanged(&root, &Request::default()).err(),
        Some(codes::CONSOLE_LAYOUT)
    );
    assert!(matches!(
        assess_unchanged(&root, &migration_request()),
        Ok(Plan::Migrate(_))
    ));

    // Directories ordinary users can create in: the service's check refuses
    // them, 27.
    let root = Root::bare();
    root.directories(&open_directory_sddl());
    assert_eq!(
        assess_unchanged(&root, &Request::default()).err(),
        Some(codes::UNTRUSTED_PATH)
    );

    // A reparse point at <P>: 27, and not followed.
    let root = Root::bare();
    let elsewhere = root.dir.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).expect("elsewhere");
    fixtures::junction(&root.paths.product_root, &elsewhere);
    assert_eq!(
        assess_unchanged(&root, &Request::default()).err(),
        Some(codes::UNTRUSTED_PATH)
    );
}

#[test]
fn package_preflight_an_enrolled_host_refuses_a_bootstrap_without_reading_it() {
    let root = Root::empty();
    std::fs::create_dir(root.paths.data_root.join("identity")).expect("identity");
    let ca = certificate("ca.setup.test");
    let anchor = root.anchor(&[&ca]);
    let bootstrap = root.bootstrap(&bootstrap_text(
        anchor.to_str().expect("UTF-8"),
        &[&ca.fingerprint],
    ));
    // Held with no sharing: any open of it by preflight would fail.
    let _held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&bootstrap)
        .expect("held");
    let request = Request {
        bootstrap: Some(bootstrap),
        ..Request::default()
    };
    assert_eq!(
        assess_unchanged(&root, &request).err(),
        Some(codes::ALREADY_CONFIGURED)
    );
}

#[test]
fn package_preflight_the_migration_route_reads_no_file_under_p() {
    let root = Root::empty();
    for name in [
        "agent.toml",
        "server-ca.pem",
        "cert.pem",
        "key.dat",
        "identity.json",
    ] {
        root.touch(name);
    }
    // Each file held with no sharing: preflight could open none of them.
    let _held: Vec<std::fs::File> = [
        "agent.toml",
        "server-ca.pem",
        "cert.pem",
        "key.dat",
        "identity.json",
    ]
    .iter()
    .map(|name| {
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(root.paths.data_root.join(name))
            .expect("held")
    })
    .collect();
    assert!(matches!(
        assess_unchanged(&root, &migration_request()),
        Ok(Plan::Migrate(_))
    ));
    // Its directories are the console runbook's, which the migration's
    // service checks, not preflight.
    let root = Root::bare();
    root.directories(&open_directory_sddl());
    root.touch("key.dat");
    assert!(matches!(
        assess_unchanged(&root, &migration_request()),
        Ok(Plan::Migrate(_))
    ));
}

#[test]
fn package_preflight_a_pending_host_stages_new_input() {
    let root = Root::empty();
    root.touch("agent.toml");
    let ca = certificate("ca.setup.test");
    let anchor = root.anchor(&[&ca]);
    let bootstrap = root.bootstrap(&bootstrap_text(
        anchor.to_str().expect("UTF-8"),
        &[&ca.fingerprint],
    ));
    let request = Request {
        bootstrap: Some(bootstrap),
        ..Request::default()
    };
    assert!(matches!(
        assess_unchanged(&root, &request),
        Ok(Plan::Stage(_))
    ));
}

// --- The machine's host, the platform and the elevation -----------------------------

#[test]
fn package_preflight_the_machines_host_is_the_services() {
    let host = Host::system().expect("the machine's host");
    let program_data = cg_agent::custody::program_data().expect("ProgramData");
    assert_eq!(host.paths, InstalledPaths::under(&program_data));
    let shared = Policy::shared();
    assert_eq!(host.directories.owners, shared.owners);
    assert_eq!(host.directories.allowed, shared.allowed);
    assert_eq!(host.directories.protected, shared.protected);
    let both = vec![Principal::System, Principal::Administrators];
    assert_eq!(host.bootstrap.owners, both);
    assert_eq!(host.bootstrap.allowed, both);
    // Read only by a route that stages: Windows' own then.
    assert_eq!(host.hostname, None);
    assert_eq!(host.wait, cg_agent::custody::OPEN_WAIT);
}

#[test]
fn package_preflight_this_host_is_a_supported_platform() {
    // The development host and the hosted runner are Windows on x86-64.
    check_platform().expect("Windows x86-64");
}

fn u16_at(bytes: &[u8], at: usize) -> usize {
    u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()) as usize
}

fn u32_at(bytes: &[u8], at: usize) -> usize {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
}

/// The functions a PE32+ image imports by name, as `dll!function`, from its
/// import table and its delay-load table.
fn imported_functions(path: &std::path::Path) -> Vec<String> {
    let bytes = std::fs::read(path).expect("read the executable");
    let pe = u32_at(&bytes, 0x3C);
    assert_eq!(&bytes[pe..pe + 4], b"PE\0\0", "the PE signature");
    let count = u16_at(&bytes, pe + 6);
    let optional = pe + 24;
    assert_eq!(u16_at(&bytes, optional), 0x20B, "a PE32+ image");
    let sections = optional + u16_at(&bytes, pe + 20);
    let offset = |rva: usize| {
        (0..count)
            .map(|i| sections + i * 40)
            .find_map(|section| {
                let start = u32_at(&bytes, section + 12);
                let size = u32_at(&bytes, section + 8).max(u32_at(&bytes, section + 16));
                (start..start + size)
                    .contains(&rva)
                    .then(|| rva - start + u32_at(&bytes, section + 20))
            })
            .unwrap_or_else(|| panic!("RVA {rva:#x} is in no section"))
    };
    let text_at = |at: usize| {
        let len = bytes[at..].iter().position(|b| *b == 0).expect("a NUL");
        String::from_utf8_lossy(&bytes[at..at + len]).to_ascii_lowercase()
    };

    let mut functions = Vec::new();
    // (directory index, descriptor size, offset of its Name RVA, offsets of
    // its name table RVA, then of its address table RVA)
    for (index, size, name, tables) in [(1, 20, 12, [0, 16]), (13, 32, 4, [16, 12])] {
        let rva = u32_at(&bytes, optional + 112 + index * 8);
        if rva == 0 {
            continue;
        }
        let mut descriptor = offset(rva);
        while bytes[descriptor..descriptor + size].iter().any(|b| *b != 0) {
            let dll = text_at(offset(u32_at(&bytes, descriptor + name)));
            let table = tables
                .iter()
                .map(|at| u32_at(&bytes, descriptor + at))
                .find(|rva| *rva != 0)
                .expect("a name or address table");
            let mut thunk = offset(table);
            loop {
                let entry = u64::from_le_bytes(bytes[thunk..thunk + 8].try_into().unwrap());
                if entry == 0 {
                    break;
                }
                // An ordinal import has no name.
                if entry & (1 << 63) == 0 {
                    let hint_name = offset(entry as usize & 0x7FFF_FFFF);
                    functions.push(format!("{dll}!{}", text_at(hint_name + 2)));
                }
                thunk += 8;
            }
            descriptor += size;
        }
    }
    functions
}

#[test]
fn package_preflight_is_wow64_process2_is_resolved_at_run_time() {
    // A static import of IsWow64Process2 would keep a Windows 10 that lacks
    // it from loading the setup before the floor's check (the Amendment
    // 2026-11-10, item 1): neither this test executable, which links
    // `check_platform`, nor the shipped cg-setup.exe imports it.
    for executable in [
        std::env::current_exe().expect("the test executable"),
        std::path::PathBuf::from(env!("CARGO_BIN_EXE_cg-setup")),
    ] {
        let functions = imported_functions(&executable);
        assert!(
            functions.iter().any(|f| f == "kernel32.dll!getprocaddress"),
            "{}: the reader sees kernel32's imports: {functions:?}",
            executable.display()
        );
        assert!(
            !functions.iter().any(|f| f.ends_with("!iswow64process2")),
            "{}: IsWow64Process2 is imported statically",
            executable.display()
        );
    }
}

#[test]
fn package_preflight_this_hosts_version_is_windows_own() {
    use cg_setup::platform::{windows_version, FLOOR};
    let version = windows_version();
    // The registry's build, which no compatibility shim rewrites for this
    // process, against RtlGetVersion's.
    let output = std::process::Command::new("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            "/v",
            "CurrentBuildNumber",
        ])
        .output()
        .expect("reg query");
    let text = String::from_utf8_lossy(&output.stdout);
    let build: u32 = text
        .split_whitespace()
        .last()
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| panic!("CurrentBuildNumber in {text:?}"));
    assert_eq!(
        (version.major, version.minor, version.build),
        (10, 0, build)
    );
    assert!(version >= FLOOR, "{version}");
}

#[test]
fn package_preflight_elevation_is_the_tokens() {
    match check_elevation() {
        Ok(()) => assert!(fixtures::token_is_elevated(), "elevated"),
        Err(refusal) => {
            assert!(!fixtures::token_is_elevated(), "{}", refusal.line);
            assert_eq!(refusal.code, codes::ELEVATION);
            assert!(refusal.line.contains("elevated"), "{}", refusal.line);
        }
    }
}

#[test]
fn package_preflight_a_filtered_administrators_token_is_not_elevated() {
    use cg_setup::platform::check_elevation_of;
    // SAFETY, for each call below: an open impersonation token, which the
    // fixture opened with TOKEN_QUERY and keeps until the call returns.
    // Administrators for deny only, as UAC filters an administrator's token:
    // refused here and on the elevated runner alike.
    let refusal =
        unsafe { check_elevation_of(fixtures::filtered_token().0) }.expect_err("filtered");
    assert_eq!(refusal.code, codes::ELEVATION, "{}", refusal.line);
    // The same token unfiltered: what Windows says of it.
    assert_eq!(
        unsafe { check_elevation_of(fixtures::own_token().0) }.is_ok(),
        fixtures::token_is_elevated()
    );
    if fixtures::in_ci() {
        unsafe { check_elevation_of(fixtures::own_token().0) }.expect("the runner is elevated");
    }
}
