//! The service's adoption of the staged configuration (SPEC-024 Amendment
//! 2026-11-09, item 6) by a real Windows service run as LocalSystem, a
//! precondition of the package's criteria (install_ac_001, install_ac_011): a
//! start with the arguments adopts and enrolls; one without leaves the staged
//! files; the arguments are refused on an enrolled host and on a console
//! layout; and a filtered administrator and a standard user reach neither the
//! staged files nor the service's start right.
//!
//! These tests create, start, stop and delete Windows services and run the
//! agent as SYSTEM, so they build only with the test-only `scm-tests` feature
//! and run only in rust-ci's Windows job, on a GitHub-hosted runner that is
//! discarded after the job (SPEC-024 Amendment 2026-11-01, point 4). Each one
//! fails without acting outside such a runner or when a service named
//! `CyberGuardAgent` exists; `TestService`, `TestUser` and `FixedDataRoot`
//! remove what it created on every exit path. They run one at a time
//! (`--test-threads=1`). That an administrator creates the staged files with
//! no privilege enabled is `cg-setup`'s to assert: here they are planted.
//!
//! The tests spell the fixed paths, the SDDLs, the start arguments and the
//! refusals' lines from the SPEC, not from the agent's code, so that a probe
//! pull request builds them against the tree without the implementation
//! (control 1's exception, point 4's Evidence).
#![cfg(windows)]

mod common;
mod scm;

use std::path::{Path, PathBuf};
use std::time::Duration;

use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::CertificateDer;
use scm::*;
use sha2::{Digest as _, Sha256};
use windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;
use windows_sys::Win32::System::Services::{SERVICE_RUNNING, SERVICE_STOPPED};

/// A token value no entry, log line or remaining file may carry.
const SECRET_TOKEN: &str = "s59-adoption-token-never-disclosed";

/// Item 6's start argument.
const ADOPT: &str = "--adopt-staged-config";

/// Item 3: `<P>` and the data root as the setup creates them.
const SETUP_DIRECTORY_SDDL: &str = "O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)";
/// Item 3: each staged file as the setup creates it.
const STAGED_FILE_SDDL: &str = "O:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)";

fn staged_config(root: &FixedDataRoot) -> PathBuf {
    root.paths.data_root.join("agent.toml.setup")
}

fn staged_anchor(root: &FixedDataRoot) -> PathBuf {
    root.paths.data_root.join("server-ca.pem.setup")
}

/// `<P>` and the data root as item 3 leaves them.
fn directories_as_the_setup_leaves_them() -> FixedDataRoot {
    let root = FixedDataRoot::plant();
    set_security(&root.paths.product_root, SETUP_DIRECTORY_SDDL);
    set_security(&root.paths.data_root, SETUP_DIRECTORY_SDDL);
    root
}

/// The start arguments' form of an anchor's certificates (§Data contracts
/// §3): `sha256:` and the lowercase hexadecimal SHA-256 of each one's DER.
fn trust_arguments(pem: &str) -> Vec<String> {
    CertificateDer::pem_slice_iter(pem.as_bytes())
        .map(|certificate| {
            let certificate = certificate.expect("a certificate of the anchor");
            let digest = Sha256::digest(certificate.as_ref());
            let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
            format!("--trust-anchor-sha256=sha256:{hex}")
        })
        .collect()
}

/// The staged configuration and anchor, each with item 3's security.
fn stage(root: &FixedDataRoot, enroll_url: &str, heartbeat_url: &str, anchor_pem: &str) {
    let mut file = ConfigFile::fixed(&root.paths, enroll_url, heartbeat_url);
    file.token = Some(SECRET_TOKEN.to_string());
    root.write_file(
        &staged_config(root),
        STAGED_FILE_SDDL,
        file.toml().as_bytes(),
    );
    root.write_file(
        &staged_anchor(root),
        STAGED_FILE_SDDL,
        anchor_pem.as_bytes(),
    );
}

/// An object's bytes and security, read before a start that must leave them.
struct Kept {
    path: PathBuf,
    bytes: Vec<u8>,
    security: String,
}

impl Kept {
    fn read(path: &Path) -> Self {
        Kept {
            path: path.to_path_buf(),
            bytes: privileged_read(path),
            security: owner_and_dacl(path),
        }
    }

    fn assert_unchanged(&self, what: &str) {
        assert!(
            privileged_read(&self.path) == self.bytes,
            "{what}: {} keeps its bytes",
            self.path.display()
        );
        assert_eq!(
            owner_and_dacl(&self.path),
            self.security,
            "{what}: {} keeps its owner and DACL",
            self.path.display()
        );
    }
}

fn staged_files(root: &FixedDataRoot) -> [Kept; 2] {
    [
        Kept::read(&staged_config(root)),
        Kept::read(&staged_anchor(root)),
    ]
}

/// Owner SYSTEM and a protected DACL allowing SYSTEM and Administrators
/// alone, full access for each.
fn assert_systems_with_administrators(path: &Path) {
    let sddl = owner_and_dacl(path);
    let security = parse_sddl(&sddl);
    assert_eq!(security.owner, "SY", "{}: {sddl}", path.display());
    assert!(security.protected(), "{}: {sddl}", path.display());
    let mut allowed: Vec<&str> = security
        .aces
        .iter()
        .map(|ace| {
            assert!(
                ace.kind == "A" && ace.is_full_access(),
                "{}: {sddl}",
                path.display()
            );
            ace.sid.as_str()
        })
        .collect();
    allowed.sort_unstable();
    assert_eq!(allowed, ["BA", "SY"], "{}: {sddl}", path.display());
}

/// Item 6's whole route on a fresh host: the service adopts during
/// `START_PENDING`, with its checkpoint advancing and no privilege enabled,
/// then starts as any other, enrolls with the staged token and delivers.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn package_adoption_a_start_with_the_arguments_adopts_and_enrolls() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    let port = reserve_port();
    let root = directories_as_the_setup_leaves_them();
    stage(
        &root,
        &enroll.base_url,
        &format!("https://127.0.0.1:{port}"),
        &pki.trust_anchor_pem,
    );
    let staged_bytes = privileged_read(&staged_anchor(&root));
    let baseline = application_baseline();
    let service = TestService::create(&service_args(&root.paths.config));

    let trust = trust_arguments(&pki.trust_anchor_pem);
    let mut arguments = vec![ADOPT];
    arguments.extend(trust.iter().map(String::as_str));
    service.start_with(&arguments).expect("start");
    let running = service.wait_for(SERVICE_RUNNING, Duration::from_secs(60));
    assert_pending_progress(&running, "START_PENDING");
    assert_eq!(
        enabled_custody_privileges(running.status.dwProcessId),
        Vec::<String>::new(),
        "no privilege is enabled (ADR-0010 §Compliance)"
    );

    assert!(
        common::wait_until(Duration::from_secs(60), || enroll.enroll_received_count()
            >= 1)
        .await,
        "the service enrolls with the token it adopted"
    );
    let request = enroll.enroll_received()[0].clone();
    assert_eq!(
        request["enrollment_token"].as_str(),
        Some(SECRET_TOKEN),
        "the staged token is the one presented"
    );
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
        "the adopted configuration reaches the heartbeat server"
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

    assert!(
        !privileged_exists(&staged_config(&root)) && !privileged_exists(&staged_anchor(&root)),
        "the staged names are gone: renamed, not copied"
    );
    assert_eq!(privileged_read(&root.paths.trust_anchor), staged_bytes);
    for path in [&root.paths.config, &root.paths.trust_anchor] {
        assert_systems_with_administrators(path);
    }
    for directory in [&root.paths.product_root, &root.paths.data_root] {
        assert_systems_with_administrators(directory);
    }
    assert!(
        !String::from_utf8_lossy(&privileged_read(&root.paths.config)).contains(SECRET_TOKEN),
        "token hygiene removed the token once the identity was written"
    );
    let entries = wait_entries(baseline, &[1000, 1001]);
    assert!(
        !entries
            .iter()
            .any(|entry| entry.data.contains(SECRET_TOKEN)),
        "no Application-log entry carries the token: {entries:#?}"
    );
}

/// "A start without them never adopts, and leaves staged files untouched":
/// on a fresh host there is no `agent.toml` to run from, which is code 2.
#[test]
fn package_adoption_a_start_without_the_arguments_leaves_the_staged_files() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let root = directories_as_the_setup_leaves_them();
    stage(
        &root,
        "https://127.0.0.1:1",
        "https://127.0.0.1:2",
        &pki.trust_anchor_pem,
    );
    let kept = staged_files(&root);
    let service = TestService::create(&service_args(&root.paths.config));

    refused_before_running(&service, 2, &["invalid config", "agent.toml"]);

    assert!(
        !privileged_exists(&root.paths.config),
        "no agent.toml was made"
    );
    kept.iter()
        .for_each(|file| file.assert_unchanged("a start without arguments"));
}

/// Step 3: "adoption never points an enrolled identity at new inputs".
#[test]
fn package_adoption_the_arguments_on_an_enrolled_host_are_code_5() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let root = directories_as_the_setup_leaves_them();
    create_directory_with(&root.paths.identity_dir, IDENTITY_DIRECTORY_SDDL);
    stage(
        &root,
        "https://127.0.0.1:1",
        "https://127.0.0.1:2",
        &pki.trust_anchor_pem,
    );
    let kept = staged_files(&root);
    let service = TestService::create(&service_args(&root.paths.config));

    let trust = trust_arguments(&pki.trust_anchor_pem);
    let mut arguments = vec![ADOPT];
    arguments.extend(trust.iter().map(String::as_str));
    refused_with_arguments(
        &service,
        &arguments,
        5,
        &["cg-agent: identity error", "identity"],
    );

    // "the file log opens after adoption": a refusal never reached it.
    assert!(
        !privileged_exists(&root.paths.data_root.join("logs")),
        "the file log is not opened before the adoption"
    );

    assert!(
        !privileged_exists(&root.paths.config),
        "nothing was adopted"
    );
    kept.iter()
        .for_each(|file| file.assert_unchanged("an enrolled host"));
}

/// Step 1: on a console layout these arguments are code 2, the line the
/// Amendment 2026-11-05, item 1, gives arguments a migration did not record.
#[test]
fn package_adoption_the_arguments_on_a_console_layout_are_code_2() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let root = directories_as_the_setup_leaves_them();
    root.write_file(
        &root.paths.data_root.join("key.dat"),
        CONFIG_SDDL,
        b"sealed",
    );
    stage(
        &root,
        "https://127.0.0.1:1",
        "https://127.0.0.1:2",
        &pki.trust_anchor_pem,
    );
    let kept = staged_files(&root);
    let service = TestService::create(&service_args(&root.paths.config));

    let trust = trust_arguments(&pki.trust_anchor_pem);
    let mut arguments = vec![ADOPT];
    arguments.extend(trust.iter().map(String::as_str));
    refused_with_arguments(
        &service,
        &arguments,
        2,
        &["cg-agent: invalid migration arguments", "console layout"],
    );

    kept.iter()
        .for_each(|file| file.assert_unchanged("a console layout"));
}

/// The amendment's *Tests*: a filtered administrator's and a standard user's
/// access to the staged files, and `SERVICE_START` denied to both. Each token
/// reads a file planted in the same tree for it, so a probe that denies
/// everything cannot pass.
#[test]
fn package_adoption_the_staged_files_and_the_start_right_are_denied_to_others() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let root = directories_as_the_setup_leaves_them();
    stage(
        &root,
        "https://127.0.0.1:1",
        "https://127.0.0.1:2",
        &pki.trust_anchor_pem,
    );
    let service = TestService::create(&service_args(&root.paths.config));

    let targets = [
        (root.paths.product_root.clone(), true),
        (root.paths.data_root.clone(), true),
        (staged_config(&root), false),
        (staged_anchor(&root), false),
    ]
    .map(|(path, directory)| ProbeTarget { path, directory });
    let readable = root.paths.data_root.join("access-probe-control.txt");
    root.write_file(
        &readable,
        "O:BAD:P(A;;FA;;;SY)(A;;FR;;;BU)",
        b"readable by BUILTIN\\Users",
    );
    let user = TestUser::create();
    let filtered = filtered_administrator_token();
    let mut problems = access_problems(
        user.token(),
        &format!("standard user {}", user.name),
        &targets,
        std::slice::from_ref(&readable),
    );
    problems.extend(access_problems(
        &filtered,
        "filtered administrator",
        &targets,
        std::slice::from_ref(&readable),
    ));
    assert!(
        problems.is_empty(),
        "every attempt is denied with ERROR_ACCESS_DENIED:\n{}",
        problems.join("\n")
    );

    // The test process, the runner's built-in Administrator: the positive
    // control, so that a denial is the service's doing. That the product's
    // service has this default DACL is the MSI's to assert.
    assert_eq!(open_service_to_start_as_this_process(&service.name), Ok(()));
    for (who, token) in [("standard user", user.token()), ("filtered", &filtered)] {
        assert_eq!(
            open_service_to_start(token, &service.name),
            Err(ERROR_ACCESS_DENIED),
            "{who}: SERVICE_START is denied"
        );
    }
}
