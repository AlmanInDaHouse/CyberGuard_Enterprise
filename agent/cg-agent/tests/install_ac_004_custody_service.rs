//! SPEC-024 install_ac_002 (the service's half), install_ac_004 and
//! install_ac_005 for a fresh service installation: the agent as a real
//! Windows service, run as LocalSystem, at the fixed installed paths, with
//! their custody checked before anything is read and the identity written
//! with its final security (SPEC-024 §Data contracts §2, §Operational §2
//! and its Amendment 2026-11-04, items 1 to 6).
//!
//! These tests create, start, stop and delete Windows services, run the
//! agent as SYSTEM and create a local account, so they build only with the
//! test-only `scm-tests` feature and run only in rust-ci's Windows job, on
//! a GitHub-hosted runner that is discarded after the job (SPEC-024
//! Amendment 2026-11-01, point 4). Each one fails without acting outside
//! such a runner or when a service named `CyberGuardAgent` exists, creates
//! its own uniquely named service and account, and removes them on every
//! exit path (`TestService`'s and `TestUser`'s `Drop`); the fixed tree it
//! plants under `%ProgramData%` is removed, and what was there restored, by
//! `FixedDataRoot`'s. They run one at a time (`--test-threads=1`).
//!
//! The tests spell the fixed paths, the SDDLs and the refusals' lines from
//! the SPEC, not from the agent's code, so that a probe pull request builds
//! them against the tree without the implementation (control 1's
//! exception, point 4's Evidence). Fragments of the Application-log
//! entries avoid the characters its XML escapes (`'`).
#![cfg(windows)]

mod common;
mod scm;

use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine as _;
use scm::*;
use windows_sys::Win32::Security::WinLocalSystemSid;
use windows_sys::Win32::System::Services::{
    SERVICE_ACCEPT_SHUTDOWN, SERVICE_ACCEPT_STOP, SERVICE_RUNNING, SERVICE_STOPPED,
};

/// A token value no entry, log line or remaining file may carry.
const SECRET_TOKEN: &str = "s55-enrollment-token-never-disclosed";

/// Another server-issued agent id (UUIDv7), for a certificate whose common
/// name is not the stored `agent_id`.
const OTHER_AGENT_ID: &str = "01934abc-def0-7000-89ab-000000000098";

/// The heartbeat envelopes the mock was sent, accepted or not.
fn heartbeats_sent(mock: &common::TlsMockServer) -> usize {
    mock.attempts().len()
}

/// No entry carries the token, a PEM block or the key's bytes in base64
/// (SPEC-024 §Operational §2: the failure exposes no key or token; the
/// second Amendment 2026-11-01, item 4: no entry carries a secret).
fn assert_no_secret(entries: &[Entry], secrets: &[String]) {
    for entry in entries {
        assert!(
            !entry.data.contains(SECRET_TOKEN),
            "an entry carries the token: {entry:?}"
        );
        assert!(
            !entry.data.contains("-----BEGIN"),
            "an entry carries a PEM block: {entry:?}"
        );
        for secret in secrets {
            assert!(
                !entry.data.contains(secret.as_str()),
                "an entry carries key material: {entry:?}"
            );
        }
    }
}

/// The key material of `pki`'s agent identity, as an entry might spell it:
/// its seed and public key in base64 (standard and URL-safe), and a line of
/// its certificate's PEM body.
fn key_material(pki: &common::TlsTestPki) -> Vec<String> {
    use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
    let mut material = vec![
        STANDARD.encode(pki.agent_seed),
        URL_SAFE_NO_PAD.encode(pki.agent_seed),
        STANDARD.encode(pki.agent_pubkey),
        URL_SAFE_NO_PAD.encode(pki.agent_pubkey),
    ];
    if let Some(line) = pki
        .client_cert_pem
        .lines()
        .find(|line| !line.starts_with("-----") && line.len() >= 40)
    {
        material.push(line.to_string());
    }
    material
}

/// A configuration at the fixed paths carrying `SECRET_TOKEN`.
fn config_file(
    root: &FixedDataRoot,
    enroll: &common::TlsEnrollMock,
    heartbeat_url: &str,
) -> ConfigFile {
    let mut file = ConfigFile::fixed(&root.paths, &enroll.base_url, heartbeat_url);
    file.token = Some(SECRET_TOKEN.to_string());
    file
}

/// install_ac_002, Amendment 2026-11-04, item 2: before `RUNNING`, the
/// service refuses a `--config` other than the fixed `agent.toml`, and a
/// configuration whose trust anchor or identity path is not its fixed path
/// (another directory, or the fixed path spelled with forward slashes,
/// which is refused, not resolved) or that names no heartbeat URL. Each is
/// code 2 with its line in entry 1002, no entry 1000, and no enrollment or
/// heartbeat.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_002_paths_other_than_the_fixed_ones_are_code_2() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    let heartbeat = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let root = FixedDataRoot::plant();
    root.write_trust_anchor(&pki.trust_anchor_pem);
    let fixed = config_file(&root, &enroll, &heartbeat.base_url);
    let config = root.write_config(&fixed);
    // Where the refused paths point: a directory with a complete, valid
    // configuration and trust anchor of its own, so a service that read
    // them would go on.
    let elsewhere = tempfile::tempdir().expect("tempdir");
    let other_anchor = elsewhere.path().join("server-ca.pem");
    std::fs::write(&other_anchor, &pki.trust_anchor_pem).expect("write the other anchor");

    // `--config` absolute, but not the fixed one: refused before it is read.
    let other_config = elsewhere.path().join("agent.toml");
    std::fs::write(&other_config, fixed.toml()).expect("write the other agent.toml");
    {
        let service = TestService::create(&service_args(&other_config));
        refused_before_running(
            &service,
            2,
            &[&format!(
                "--config must be {} in service mode",
                text(&root.paths.config)
            )],
        );
    }

    let service = TestService::create(&service_args(&config));
    let in_service_mode =
        |key: &str, fixed: &Path| format!("{key} must be {} in service mode", text(fixed));
    let mut cases: Vec<(&str, ConfigFile, String)> = Vec::new();
    let mut case = fixed.clone();
    case.trust_anchor_path = text(&other_anchor);
    cases.push((
        "the trust anchor in another directory",
        case,
        in_service_mode("server.trust_anchor_path", &root.paths.trust_anchor),
    ));
    let mut case = fixed.clone();
    case.cert_path = text(&elsewhere.path().join("cert.pem"));
    cases.push((
        "the certificate in another directory",
        case,
        in_service_mode("enrollment.cert_path", &root.paths.cert),
    ));
    let mut case = fixed.clone();
    case.cert_path = text(&root.paths.cert).replace('\\', "/");
    cases.push((
        "the fixed certificate path with forward slashes",
        case,
        in_service_mode("enrollment.cert_path", &root.paths.cert),
    ));
    let mut case = fixed.clone();
    case.key_path = text(&elsewhere.path().join("key.dat"));
    cases.push((
        "the key in another directory",
        case,
        in_service_mode("enrollment.key_path", &root.paths.key),
    ));
    let mut case = fixed.clone();
    case.identity_path = text(&elsewhere.path().join("identity.json"));
    cases.push((
        "identity.json in another directory",
        case,
        in_service_mode("enrollment.identity_path", &root.paths.identity),
    ));
    let mut case = fixed.clone();
    case.heartbeat_url = None;
    cases.push((
        "no heartbeat URL",
        case,
        "the service requires server.heartbeat_url".to_string(),
    ));
    for (what, file, line) in &cases {
        eprintln!("case: {what}");
        root.write_config(file);
        let entries = refused_before_running(&service, 2, &[&text(&config), line]);
        assert_no_secret(&entries, &[]);
    }
    assert_eq!(
        enroll.enroll_received_count(),
        0,
        "every refusal precedes enrollment"
    );
    assert_eq!(heartbeats_sent(&heartbeat), 0, "and every heartbeat");
}

/// install_ac_002 and install_ac_004, Amendment 2026-11-04, item 3: before
/// the service reads `agent.toml`, which can carry a token, its custody is
/// checked through the handle it is read from: owned by SYSTEM, a protected
/// DACL, allow entries for SYSTEM and Administrators alone. A file another
/// principal may open, one owned by Administrators, and one whose DACL is
/// only inherited are each code 2 before `RUNNING`, naming the path and the
/// failed condition, with no enrollment.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_002_a_configuration_others_can_reach_is_code_2() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    let heartbeat = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let root = FixedDataRoot::plant();
    root.write_trust_anchor(&pki.trust_anchor_pem);
    let file = config_file(&root, &enroll, &heartbeat.base_url);
    let config = root.write_config(&file);
    let toml = file.toml();
    let service = TestService::create(&service_args(&config));
    let path = text(&config);

    // An extra allow entry for BUILTIN\Users.
    root.write_file(
        &config,
        "O:SYD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;BU)",
        toml.as_bytes(),
    );
    assert!(
        parse_sddl(&owner_and_dacl(&config))
            .aces
            .iter()
            .any(|ace| ace.kind == "A" && ace.sid == "BU"),
        "the fixture allows BUILTIN\\Users"
    );
    let entries = refused_before_running(&service, 2, &[&path, "allows access"]);
    assert_no_secret(&entries, &[]);

    // Owned by Administrators, with the production DACL.
    root.write_file(&config, "O:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)", toml.as_bytes());
    assert_eq!(
        parse_sddl(&owner_and_dacl(&config)).owner,
        "BA",
        "the fixture is owned by Administrators"
    );
    let entries = refused_before_running(&service, 2, &[&path, "is not owned by SYSTEM"]);
    assert_no_secret(&entries, &[]);

    // Owned by SYSTEM, with a DACL only inherited from the data root
    // (SYSTEM and Administrators): protection is the one failing condition.
    // A DACL given at creation comes out protected, so the file is created
    // without one and SYSTEM made its owner after.
    privileged_delete(&config);
    create_file_inheriting(&config, WinLocalSystemSid, toml.as_bytes());
    let security = parse_sddl(&owner_and_dacl(&config));
    assert!(
        security.owner == "SY"
            && !security.protected()
            && !security.aces.is_empty()
            && security
                .aces
                .iter()
                .all(|ace| ace.kind == "A" && (ace.sid == "SY" || ace.sid == "BA")),
        "the fixture is SYSTEM's, unprotected, and allows SYSTEM and Administrators alone: \
         {security:?}"
    );
    let entries = refused_before_running(&service, 2, &[&path, "DACL is not protected"]);
    assert_no_secret(&entries, &[]);

    assert_eq!(
        enroll.enroll_received_count(),
        0,
        "every refusal precedes enrollment"
    );
    assert_eq!(heartbeats_sent(&heartbeat), 0, "and every heartbeat");
}

/// install_ac_002, Amendment 2026-11-04, item 3: after `RUNNING`, before
/// the first enrollment round, `server-ca.pem`'s custody is checked; a
/// trust anchor another principal may change is code 6 with the TLS
/// client-configuration line naming the path and the condition, and the
/// service neither enrolls nor sends a heartbeat.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_002_a_trust_anchor_others_can_change_is_code_6() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    let heartbeat = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let root = FixedDataRoot::plant();
    let config = root.write_config(&config_file(&root, &enroll, &heartbeat.base_url));
    root.write_file(
        &root.paths.trust_anchor,
        "O:SYD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;BU)",
        pki.trust_anchor_pem.as_bytes(),
    );
    let service = TestService::create(&service_args(&config));
    let entries = failed_after_running(
        &service,
        6,
        &[
            "tls: client configuration failed",
            &text(&root.paths.trust_anchor),
            "allows access",
        ],
    );
    assert_no_secret(&entries, &[]);
    assert_eq!(
        enroll.enroll_received_count(),
        0,
        "the check precedes the first enrollment round"
    );
    assert_eq!(heartbeats_sent(&heartbeat), 0, "no heartbeat");
}

/// Item 4's security, read through a backup-semantics handle: owner
/// SYSTEM and a protected DACL whose single entry allows SYSTEM full
/// access; for a directory, inherited by what it holds.
fn assert_system_alone(path: &Path, directory: bool) {
    let sddl = owner_and_dacl(path);
    let security = parse_sddl(&sddl);
    assert_eq!(
        security.owner,
        "SY",
        "{} is SYSTEM's: {sddl}",
        path.display()
    );
    assert!(
        security.protected(),
        "{}'s DACL is protected: {sddl}",
        path.display()
    );
    assert_eq!(
        security.aces.len(),
        1,
        "{}'s DACL has a single entry: {sddl}",
        path.display()
    );
    let ace = &security.aces[0];
    assert!(
        ace.kind == "A" && ace.sid == "SY" && ace.is_full_access(),
        "{}'s one entry allows SYSTEM full access: {sddl}",
        path.display()
    );
    if directory {
        // Inherited by the files and directories it holds, and applying to
        // the directory itself.
        assert!(
            ace.has_flag("OI") && ace.has_flag("CI") && !ace.has_flag("IO"),
            "{}'s entry is inherited by objects and containers: {sddl}",
            path.display()
        );
    } else {
        // A protected DACL holds no inherited entry; the entry would be
        // SYSTEM's in any case, checked above.
        assert!(
            ace.flags.iter().all(|flag| flag == "ID"),
            "{}'s entry carries no inheritance of its own: {sddl}",
            path.display()
        );
    }
}

/// install_ac_004 and install_ac_006's token hygiene, Amendment 2026-11-04,
/// items 4 and 6: the service enrolls as SYSTEM and writes `identity\` and
/// its three files with their final security, SYSTEM's alone, owner SYSTEM
/// and protected; the rewrite of `agent.toml` drops the token and keeps the
/// owner and DACL it had. Neither a standard user created for the test nor
/// a filtered administrator's token can read, write, delete, change the DACL
/// of or take ownership of the identity, the configuration, the trust
/// anchor or the data root, effective access checked as well as the
/// SDDL. SYSTEM can use the identity: a second start heartbeats with it and
/// does not enroll again.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_004_the_identity_is_systems_alone_and_others_cannot_reach_it() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    // The heartbeat mock verifies envelopes under the key the agent
    // generates when it enrolls; its URL is named before that key exists.
    let port = reserve_port();
    let root = FixedDataRoot::plant();
    root.write_trust_anchor(&pki.trust_anchor_pem);
    let config = root.write_config(&config_file(
        &root,
        &enroll,
        &format!("https://127.0.0.1:{port}"),
    ));
    let config_security = owner_and_dacl(&config);
    let anchor_security = owner_and_dacl(&root.paths.trust_anchor);
    let baseline = application_baseline();
    let service = TestService::create(&service_args(&config));

    service.start().expect("start");
    let running = service.wait_for(SERVICE_RUNNING, Duration::from_secs(60));
    assert_pending_progress(&running, "START_PENDING");
    assert_eq!(
        running.status.dwControlsAccepted,
        SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN,
        "RUNNING accepts STOP and SHUTDOWN and no other control"
    );
    assert!(
        common::wait_until(Duration::from_secs(60), || enroll.enroll_received_count()
            >= 1)
        .await,
        "the service enrolls after RUNNING"
    );
    let request = enroll.enroll_received()[0].clone();
    let agent_pubkey: [u8; 32] = common::base64url_decode(
        request["agent_pubkey"]
            .as_str()
            .expect("the request's agent_pubkey"),
    )
    .try_into()
    .expect("a 32-byte Ed25519 key");
    let heartbeat = common::TlsMockServer::start_for_key_on_port(&pki, port, agent_pubkey).await;
    assert!(
        common::wait_until(Duration::from_secs(90), || online_count(&heartbeat) >= 1).await,
        "the enrolled service delivers heartbeats signed by its key"
    );
    service.control_stop().expect("STOP");
    let stopped = service.wait_for(SERVICE_STOPPED, STOP_DEADLINE + Duration::from_secs(15));
    assert_eq!(
        (
            stopped.status.dwWin32ExitCode,
            stopped.status.dwServiceSpecificExitCode
        ),
        (0, 0),
        "a clean stop"
    );

    // Item 4: the identity as the service wrote it.
    assert_system_alone(&root.paths.identity_dir, true);
    for file in [&root.paths.cert, &root.paths.key, &root.paths.identity] {
        assert_system_alone(file, false);
    }
    assert_eq!(
        privileged_listing(&root.paths.identity_dir).expect("identity\\ exists"),
        vec!["cert.pem", "identity.json", "key.dat"],
        "identity\\ holds the three files and nothing else"
    );
    // Item 6: the token is gone, the owner and DACL stayed.
    let rewritten = String::from_utf8(privileged_read(&config)).expect("agent.toml is UTF-8");
    let parsed: toml::Value = toml::from_str(&rewritten).expect("agent.toml parses");
    assert!(
        parsed["enrollment"].get("token").is_none() && !rewritten.contains(SECRET_TOKEN),
        "token hygiene removed the token: {rewritten}"
    );
    assert_eq!(
        owner_and_dacl(&config),
        config_security,
        "the rewrite keeps agent.toml's owner and DACL"
    );
    assert_eq!(
        owner_and_dacl(&root.paths.trust_anchor),
        anchor_security,
        "the trust anchor is unchanged"
    );
    assert!(
        !privileged_exists(&root.paths.hygiene_temporary),
        "no temporary file of the rewrite remains"
    );

    // install_ac_004: effective access, under a standard user created for
    // the test and a filtered administrator's token (the runner's built-in
    // Administrator has no linked one, so it is built; see
    // `filtered_administrator_token`).
    let targets = [
        (&root.paths.identity_dir, true),
        (&root.paths.cert, false),
        (&root.paths.key, false),
        (&root.paths.identity, false),
        (&config, false),
        (&root.paths.trust_anchor, false),
        (&root.paths.data_root, true),
    ]
    .map(|(path, directory)| ProbeTarget {
        path: PathBuf::from(path),
        directory,
    });
    let user = TestUser::create();
    let filtered = filtered_administrator_token();
    let mut problems = access_problems(
        user.token(),
        &format!("standard user {}", user.name),
        &targets,
    );
    problems.extend(access_problems(
        &filtered,
        "filtered administrator",
        &targets,
    ));
    assert!(
        problems.is_empty(),
        "every attempt is denied with ERROR_ACCESS_DENIED:\n{}",
        problems.join("\n")
    );

    // SYSTEM can use the identity: a second start loads it and heartbeats
    // under the same key, without enrolling again.
    let online_before = online_count(&heartbeat);
    service.start().expect("second start");
    service.wait_for(SERVICE_RUNNING, Duration::from_secs(60));
    assert!(
        common::wait_until(Duration::from_secs(60), || online_count(&heartbeat)
            > online_before)
        .await,
        "the second start delivers heartbeats with the persisted identity"
    );
    service.control_stop().expect("STOP");
    let stopped = service.wait_for(SERVICE_STOPPED, STOP_DEADLINE + Duration::from_secs(15));
    assert_eq!(
        (
            stopped.status.dwWin32ExitCode,
            stopped.status.dwServiceSpecificExitCode
        ),
        (0, 0),
        "a clean second stop"
    );
    assert_eq!(
        enroll.enroll_received_count(),
        1,
        "the persisted identity is loaded, never re-enrolled"
    );
    let entries = wait_entries(baseline, &[1000, 1001]);
    assert!(
        !entries
            .iter()
            .any(|e| e.event_id == 1002 || e.event_id == 1003),
        "no failure entry: {entries:#?}"
    );
    assert_no_secret(&entries, &[]);
}

/// One planted identity of install_ac_005.
struct Planted {
    what: &'static str,
    files: IdentityFiles,
    directory_sddl: &'static str,
    file_sddls: [&'static str; 3],
    fragments: Vec<String>,
}

/// install_ac_005, Amendment 2026-11-04, items 3 and 5, and SPEC-002
/// Amendment 2026-10-27: after `RUNNING`, at the identity step, a partial,
/// inconsistent or insecure identity fails closed with code 5 and a line
/// naming the condition (and, for a set, the repair), never the token or
/// key material; the service does not enroll, sends no heartbeat, and
/// leaves every planted file as it was. The cases: an empty `identity\`
/// (item 4: the set is absent only when that directory is); the
/// certificate and key without `identity.json`; a certificate of another key; a certificate
/// whose common name is another agent id; a valid set whose `identity\`
/// also allows Administrators; a valid set whose `key.dat` is owned by
/// Administrators.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_005_a_partial_inconsistent_or_insecure_identity_fails_closed() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let another_key = common::generate_test_pki(common::TEST_AGENT_ID);
    let another_id = common::generate_test_pki(OTHER_AGENT_ID);
    let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    // Verifies under the identity's key: a service that went on with a
    // valid set would be seen heartbeating.
    let heartbeat = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let paths = FixedPaths::resolve();
    let repair = "restore the identity".to_string();

    let mut incomplete = IdentityFiles::of(&pki);
    incomplete.json = None;
    let empty = IdentityFiles {
        cert: None,
        key: None,
        json: None,
    };
    let cases = vec![
        // Item 4: the set is absent only when `identity\` is; an empty one
        // is an incomplete set, not a reason to enroll.
        Planted {
            what: "an empty identity directory",
            files: empty,
            directory_sddl: IDENTITY_DIRECTORY_SDDL,
            file_sddls: [IDENTITY_FILE_SDDL; 3],
            fragments: vec!["incomplete identity".to_string(), repair.clone()],
        },
        Planted {
            what: "the certificate and key without identity.json",
            files: incomplete,
            directory_sddl: IDENTITY_DIRECTORY_SDDL,
            file_sddls: [IDENTITY_FILE_SDDL; 3],
            fragments: vec!["incomplete identity".to_string(), repair.clone()],
        },
        Planted {
            what: "a certificate of another key",
            files: IdentityFiles::new(
                &another_key.client_cert_pem,
                &pki.agent_seed,
                &pki.agent_pubkey,
                &pki.agent_id,
            ),
            directory_sddl: IDENTITY_DIRECTORY_SDDL,
            file_sddls: [IDENTITY_FILE_SDDL; 3],
            fragments: vec!["public key is not the agent".to_string(), repair.clone()],
        },
        Planted {
            what: "a certificate whose common name is another agent id",
            files: IdentityFiles::new(
                &another_id.client_cert_pem,
                &another_id.agent_seed,
                &another_id.agent_pubkey,
                &pki.agent_id,
            ),
            directory_sddl: IDENTITY_DIRECTORY_SDDL,
            file_sddls: [IDENTITY_FILE_SDDL; 3],
            fragments: vec!["common name is not the agent".to_string(), repair],
        },
        Planted {
            what: "a valid set whose identity directory also allows Administrators",
            files: IdentityFiles::of(&pki),
            directory_sddl: "O:SYD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)",
            file_sddls: [IDENTITY_FILE_SDDL; 3],
            fragments: vec![format!("{} allows access", text(&paths.identity_dir))],
        },
        Planted {
            what: "a valid set whose key.dat is owned by Administrators",
            files: IdentityFiles::of(&pki),
            directory_sddl: IDENTITY_DIRECTORY_SDDL,
            file_sddls: [
                IDENTITY_FILE_SDDL,
                "O:BAD:P(A;;FA;;;SY)",
                IDENTITY_FILE_SDDL,
            ],
            fragments: vec![format!("{} is not owned by SYSTEM", text(&paths.key))],
        },
    ];
    let mut secrets = key_material(&pki);
    secrets.extend(key_material(&another_key));
    secrets.extend(key_material(&another_id));

    let service = TestService::create(&service_args(&paths.config));
    for case in &cases {
        eprintln!("case: {}", case.what);
        let root = FixedDataRoot::plant();
        root.write_trust_anchor(&pki.trust_anchor_pem);
        root.write_config(&config_file(&root, &enroll, &heartbeat.base_url));
        root.plant_identity(case.directory_sddl, &case.files, case.file_sddls);
        let planted: Vec<(PathBuf, Vec<u8>)> = [
            (&root.paths.cert, &case.files.cert),
            (&root.paths.key, &case.files.key),
            (&root.paths.identity, &case.files.json),
        ]
        .into_iter()
        .filter_map(|(path, bytes)| bytes.clone().map(|bytes| (path.clone(), bytes)))
        .collect();
        let listing = privileged_listing(&root.paths.identity_dir);

        let mut fragments: Vec<&str> = vec!["identity error"];
        fragments.extend(case.fragments.iter().map(String::as_str));
        let entries = failed_after_running(&service, 5, &fragments);
        assert_no_secret(&entries, &secrets);

        for (path, bytes) in &planted {
            assert!(
                privileged_read(path) == *bytes,
                "{}: {} is unchanged",
                case.what,
                path.display()
            );
        }
        assert_eq!(
            privileged_listing(&root.paths.identity_dir),
            listing,
            "{}: no file is added to identity\\ or removed from it",
            case.what
        );
        assert_eq!(
            enroll.enroll_received_count(),
            0,
            "{}: no enrollment over the identity",
            case.what
        );
        assert_eq!(
            heartbeats_sent(&heartbeat),
            0,
            "{}: no heartbeat",
            case.what
        );
    }
}

/// install_ac_005, Amendment 2026-11-04, item 5: a server's answer whose
/// certificate is for another key is a malformed answer, code 3 after
/// `RUNNING`, and nothing is written: no `identity\` (it is created only to
/// persist an identity, item 4) nor any identity file, `agent.toml` with
/// its token as it was, and no heartbeat.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_005_a_certificate_for_another_key_in_the_answer_is_code_3_and_writes_nothing() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let another_key = common::generate_test_pki(common::TEST_AGENT_ID);
    let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    enroll.set_certificate_override(&another_key.client_cert_pem);
    let heartbeat = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let root = FixedDataRoot::plant();
    root.write_trust_anchor(&pki.trust_anchor_pem);
    let config = root.write_config(&config_file(&root, &enroll, &heartbeat.base_url));
    let config_before = privileged_read(&config);
    let service = TestService::create(&service_args(&config));

    let entries = failed_after_running(
        &service,
        3,
        &["malformed response", "public key is not the agent"],
    );
    assert_no_secret(&entries, &key_material(&another_key));
    assert_eq!(
        enroll.enroll_received_count(),
        1,
        "the service enrolled once and stopped on the answer"
    );
    let listing = privileged_listing(&root.paths.identity_dir);
    assert!(listing.is_none(), "identity\\ was not created: {listing:?}");
    for file in [&root.paths.cert, &root.paths.key, &root.paths.identity] {
        assert!(
            !privileged_exists(file),
            "{} was not written",
            file.display()
        );
    }
    assert!(
        privileged_read(&config) == config_before,
        "agent.toml is unchanged, its token included"
    );
    assert_eq!(heartbeats_sent(&heartbeat), 0, "no heartbeat");
}
