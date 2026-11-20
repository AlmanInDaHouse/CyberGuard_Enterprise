//! SPEC-024 install_ac_013, as the Amendment 2026-11-20 reads it: the
//! console installation is a development layout, which the product neither
//! adopts nor migrates. A real Windows service run as LocalSystem, from a
//! console installation a real console agent enrolled at the fixed paths,
//! or from an installed host's data root: the default refusal of a start
//! without arguments on a console layout, with point 3's reason; point 2's
//! refusals of the migration's start argument, code 2 before any change,
//! with that installation in place and an elevated console agent still
//! starting from it; a start without arguments beside a staging or legacy
//! directory that counts, code 5 before any change, beside a console layout
//! and on an installed host; and a name an ordinary user planted beside the
//! data root, which changes no outcome. The migration's route has no test
//! here that runs it as SYSTEM through the service (the amendment's
//! *Tests*): its code, unreachable from the service's start, is tested by
//! the tests that call it directly.
//!
//! These tests create, start, stop and delete Windows services, run the
//! agent as SYSTEM and create a local account, so they build only with the
//! test-only `scm-tests` feature and run only in rust-ci's Windows job, on
//! a GitHub-hosted runner that is discarded after the job (SPEC-024
//! Amendment 2026-11-01, point 4). Each one fails without acting outside
//! such a runner or when a service named `CyberGuardAgent` exists, creates
//! its own uniquely named service and account, and removes them on every
//! exit path (`TestService`'s and `TestUser`'s `Drop`); the console agent it
//! launches is ended by `ConsoleAgent`'s, and the fixed tree it plants under
//! `%ProgramData%` is removed, and what was there restored, by
//! `FixedDataRoot`'s. They run one at a time (`--test-threads=1`).
//!
//! The tests spell the fixed paths, the SDDLs, the start arguments and the
//! refusals' lines from the SPEC, not from the agent's code, so that a probe
//! pull request builds them against the tree without the implementation
//! (control 1's exception, point 4's Evidence). What a test sets up uses the
//! console agent alone, or the fixture's planted files. Fragments of the
//! Application-log entries avoid the characters its XML escapes (`'`).
#![cfg(windows)]

mod common;
mod scm;

use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::CertificateDer;
use scm::*;
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};
use windows_sys::Win32::System::Services::{SERVICE_RUNNING, SERVICE_STOPPED};

/// A token value no entry, log line or remaining file may carry.
const SECRET_TOKEN: &str = "s57-enrollment-token-never-disclosed";

/// The Amendment 2026-11-05, item 1's start arguments, which the Amendment
/// 2026-11-20, point 2, refuses.
const REMOVE: &str = "--migrate-legacy=remove";
const KEEP: &str = "--migrate-legacy=keep";

/// Item 6's reason, as the Amendment 2026-11-20, point 3, words it, split at
/// the apostrophe the rendered XML escapes.
const REFUSAL_HEAD: &str = "the data directory holds a console installation";
const REFUSAL_TAIL: &str =
    "s identity files, which the product neither adopts nor migrates; see the installation runbook, install.md";

/// Point 2's refusal of `--migrate-legacy=`: point 3's "a line for a variable
/// or an argument names it and says that the migration is not supported;
/// each sends the operator to the installation runbook".
const NOT_SUPPORTED: [&str; 4] = [
    "invalid migration arguments",
    "--migrate-legacy",
    "not supported",
    "installation runbook",
];

/// Point 3: "a line for a layout names the layout and says that the product
/// neither adopts nor migrates it; ... each sends the operator to the
/// installation runbook".
const NEITHER_ADOPTS_NOR_MIGRATES: &str = "neither adopts nor migrates";
const INSTALLATION_RUNBOOK: &str = "installation runbook";

/// What point 3 says no refusal line does: offer the migration or tell the
/// operator to set its variables or arguments.
const MIGRATION_OFFERS: [&str; 4] = [
    "--migrate-legacy",
    "CG_MIGRATE_LEGACY",
    "CG_TRUST_ANCHORS",
    "migrate them",
];

/// The console layout's files, as `privileged_listing` sorts them.
const CONSOLE_FILES: [&str; 5] = [
    "agent.toml",
    "cert.pem",
    "identity.json",
    "key.dat",
    "server-ca.pem",
];
/// Those the console agent gives SYSTEM and its own account.
const IDENTITY_FILES: [&str; 3] = ["cert.pem", "identity.json", "key.dat"];

/// The start arguments' form of an anchor's certificates (§Data contracts
/// §3): `sha256:` and the lowercase hexadecimal SHA-256 of each one's DER.
fn anchor_fingerprints(pem: &[u8]) -> Vec<String> {
    CertificateDer::pem_slice_iter(pem)
        .map(|certificate| {
            let certificate = certificate.expect("a certificate of the anchor");
            format!("sha256:{}", hex(&Sha256::digest(certificate.as_ref())))
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// `migration.json` as the Amendment 2026-11-05, item 3, lists it, which a
/// committed migration leaves in the data root: `choice`, the fingerprint
/// given, `agent_id` and the key's fingerprint.
fn record_json(choice: &str, fingerprint: &str, key_fingerprint: &str) -> String {
    json!({
        "choice": choice,
        "fingerprints": [fingerprint],
        "agent_id": common::TEST_AGENT_ID,
        "key_fingerprint": key_fingerprint,
        "enrolling_account": null,
        "files": {},
        "reversal_pending": false,
        "copies_not_reached": [],
    })
    .to_string()
}

/// A real console agent delivers its next heartbeat to `mock`.
async fn wait_console_heartbeat(mock: &common::TlsMockServer, console: &mut ConsoleAgent) {
    let before = online_count(mock);
    let delivered = common::wait_until(Duration::from_secs(90), || {
        online_count(mock) > before || console.exited().is_some()
    })
    .await;
    let exited = console.exited();
    assert!(
        delivered && exited.is_none(),
        "the elevated console agent delivers a heartbeat (it exited: {exited:?}): {}",
        console.output()
    );
}

/// The console layout as the Amendment 2026-11-05's *What surfaced it*
/// describes it: the five files directly in the data root and nothing else
/// there or in `<P>`, owned by Administrators, the default owner of the
/// runner's built-in Administrator; each identity file with the protected
/// DACL the console agent sets, SYSTEM and the account that ran it;
/// `agent.toml` without the token it enrolled with, which the console's
/// FR-014 removed.
fn assert_console_layout(paths: &FixedPaths) {
    assert_eq!(
        privileged_listing(&paths.product_root),
        Some(vec!["agent".to_string()]),
        "the fixture's <P> holds the data root alone"
    );
    assert_eq!(
        privileged_listing(&paths.data_root),
        Some(CONSOLE_FILES.map(String::from).to_vec()),
        "the fixture's data root holds the console layout's files"
    );
    for name in CONSOLE_FILES {
        let sddl = owner_and_dacl(&paths.data_root.join(name));
        assert_eq!(
            parse_sddl(&sddl).owner,
            "BA",
            "the fixture's {name} is owned by Administrators: {sddl}"
        );
    }
    for name in IDENTITY_FILES {
        let sddl = owner_and_dacl(&paths.data_root.join(name));
        let security = parse_sddl(&sddl);
        assert!(
            security.protected()
                && security.aces.len() == 2
                && security.aces.iter().all(|ace| ace.kind == "A")
                && security.aces.iter().any(|ace| ace.sid == "SY"),
            "the fixture's {name} allows SYSTEM and one other SID: {sddl}"
        );
    }
    let config = privileged_read(&paths.config);
    assert!(
        !String::from_utf8_lossy(&config).contains(SECRET_TOKEN),
        "the console agent removed the token from agent.toml"
    );
}

/// An object as a refusal must leave it: its owner and DACL, and a file's
/// bytes or a directory's names.
struct State {
    path: PathBuf,
    directory: bool,
    security: String,
    content: Vec<u8>,
}

impl State {
    fn read(path: &Path, directory: bool) -> Self {
        let content = if directory {
            privileged_listing(path)
                .unwrap_or_else(|| panic!("{} exists", path.display()))
                .join("\n")
                .into_bytes()
        } else {
            privileged_read(path)
        };
        State {
            path: path.to_path_buf(),
            directory,
            security: owner_and_dacl(path),
            content,
        }
    }

    fn assert_same_content(&self, what: &str) {
        assert!(
            State::read(&self.path, self.directory).content == self.content,
            "{what}: {} keeps its content",
            self.path.display()
        );
    }

    fn assert_unchanged(&self, what: &str) {
        self.assert_same_content(what);
        assert_eq!(
            owner_and_dacl(&self.path),
            self.security,
            "{what}: {} keeps its owner and DACL",
            self.path.display()
        );
    }
}

fn assert_all_unchanged(states: &[State], what: &str) {
    states.iter().for_each(|state| state.assert_unchanged(what));
}

/// Neither directory of a migration is there: none started.
fn assert_no_migration(paths: &FixedPaths, what: &str) {
    assert!(
        !privileged_exists(&paths.staging) && !privileged_exists(&paths.legacy),
        "{what}: no staging or legacy directory"
    );
}

/// `text` carries no SID, token, PEM block or key material (the Amendment
/// 2026-11-05's rule that a reason never names a file's content or a SID).
fn assert_discloses_nothing(text: &str, what: &str, secrets: &[String]) {
    assert!(
        !text.contains("S-1-5-"),
        "{what} carries a SID: {text:.2000}"
    );
    assert!(
        !text.contains(SECRET_TOKEN),
        "{what} carries the token: {text:.2000}"
    );
    assert!(
        !text.contains("-----BEGIN"),
        "{what} carries a PEM block: {text:.2000}"
    );
    for secret in secrets {
        assert!(
            !text.contains(secret.as_str()),
            "{what} carries key material"
        );
    }
}

fn entries_text(entries: &[Entry]) -> String {
    entries
        .iter()
        .map(|entry| entry.data.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Point 3: the entries' line offers no migration and tells the operator to
/// set none of its variables or arguments.
fn assert_offers_no_migration(entries: &[Entry], what: &str) {
    let line = entries_text(entries);
    for offer in MIGRATION_OFFERS {
        assert!(
            !line.contains(offer),
            "{what}: the line offers no migration ({offer:?}): {line:.2000}"
        );
    }
}

/// Item 1: "the line never repeats an argument's value": no value of
/// `arguments`, the bare word or digits, stands in the entries as a word of
/// its own. A value in a longer word (`removes`) is not one.
fn assert_repeats_no_value(entries: &[Entry], arguments: &[&str]) {
    let line = entries_text(entries);
    for argument in arguments {
        let value = argument
            .split_once('=')
            .map_or(*argument, |(_, value)| value);
        let value = value.trim_start_matches("sha256:");
        let repeated = line.match_indices(value).any(|(at, _)| {
            let before = line[..at].chars().next_back();
            let after = line[at + value.len()..].chars().next();
            ![before, after]
                .into_iter()
                .flatten()
                .any(char::is_alphanumeric)
        });
        assert!(!repeated, "the line repeats {value:?}: {line}");
    }
}

/// A host the console runbook set up (the Amendment 2026-11-05's *What
/// surfaced it*): `<P>` and the data root inheriting `%ProgramData%`'s
/// permissions, `server-ca.pem` and `agent.toml` written plainly, and a real
/// console agent that enrolled, elevated, against the test's own enrollment
/// and heartbeat servers, wrote its identity files directly in the data root
/// and was ended. The heartbeat server verifies signatures under the key the
/// console agent generated.
struct ConsoleHost {
    enroll: common::TlsEnrollMock,
    heartbeat: common::TlsMockServer,
    root: FixedDataRoot,
    config: PathBuf,
    /// `--trust-anchor-sha256=sha256:<hex>`, the anchor's one certificate.
    anchor: String,
    /// The seed `key.dat` seals, as a log line might spell it.
    secrets: Vec<String>,
}

impl ConsoleHost {
    async fn enrolled() -> Self {
        let pki = common::generate_test_pki(common::TEST_AGENT_ID);
        let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
        // The heartbeat mock verifies envelopes under the key the agent
        // generates when it enrolls; its URL is named before that key exists.
        let port = reserve_port();
        let root = FixedDataRoot::plant_inheriting();
        let config = root.paths.config.clone();
        let anchor_path = root.paths.data_root.join("server-ca.pem");
        std::fs::write(&anchor_path, &pki.trust_anchor_pem).expect("write server-ca.pem");
        let mut file = ConfigFile::console(
            &root.paths,
            &enroll.base_url,
            &format!("https://127.0.0.1:{port}"),
        );
        file.token = Some(SECRET_TOKEN.to_string());
        std::fs::write(&config, file.toml()).expect("write agent.toml");

        let mut console = ConsoleAgent::start(&config);
        common::wait_until(Duration::from_secs(60), || {
            enroll.enroll_received_count() >= 1 || console.exited().is_some()
        })
        .await;
        assert_eq!(
            enroll.enroll_received_count(),
            1,
            "the console agent enrolls: {}",
            console.output()
        );
        let request = enroll.enroll_received()[0].clone();
        let agent_pubkey: [u8; 32] = common::base64url_decode(
            request["agent_pubkey"]
                .as_str()
                .expect("the request's agent_pubkey"),
        )
        .try_into()
        .expect("a 32-byte Ed25519 key");
        let heartbeat =
            common::TlsMockServer::start_for_key_on_port(&pki, port, agent_pubkey).await;
        wait_console_heartbeat(&heartbeat, &mut console).await;
        drop(console);

        assert_console_layout(&root.paths);
        let key = privileged_read(&root.paths.data_root.join("key.dat"));
        let seed = cg_agent::secure_storage::default_store()
            .unseal(&key)
            .unwrap_or_else(|e| panic!("unseal the console agent's key.dat: {e:?}"));
        let secrets = vec![
            STANDARD.encode(&seed),
            URL_SAFE_NO_PAD.encode(&seed),
            hex(&seed),
            STANDARD.encode(&key),
        ];
        let anchor = anchor_fingerprints(pki.trust_anchor_pem.as_bytes());
        assert_eq!(anchor.len(), 1, "the test CA is one certificate");
        ConsoleHost {
            enroll,
            heartbeat,
            config,
            anchor: format!("--trust-anchor-sha256={}", anchor[0]),
            secrets,
            root,
        }
    }

    fn paths(&self) -> &FixedPaths {
        &self.root.paths
    }

    /// The file of that name in the data root.
    fn console_file(&self, name: &str) -> PathBuf {
        self.paths().data_root.join(name)
    }

    /// The fingerprint of the console agent's key, which `identity.json`
    /// holds.
    fn key_fingerprint(&self) -> String {
        let identity: Value =
            serde_json::from_slice(&privileged_read(&self.console_file("identity.json")))
                .expect("identity.json parses");
        identity["agent_pubkey_fingerprint"]
            .as_str()
            .expect("identity.json's fingerprint")
            .to_string()
    }

    fn directory_states(&self) -> Vec<State> {
        [&self.paths().product_root, &self.paths().data_root]
            .into_iter()
            .map(|path| State::read(path, true))
            .collect()
    }

    fn file_states(&self) -> Vec<State> {
        CONSOLE_FILES
            .iter()
            .map(|name| State::read(&self.console_file(name), false))
            .collect()
    }

    /// `refused_with_arguments`, and the entries disclose nothing.
    fn refused(
        &self,
        service: &TestService,
        arguments: &[&str],
        code: u32,
        fragments: &[&str],
    ) -> Vec<Entry> {
        let entries = refused_with_arguments(service, arguments, code, fragments);
        assert_discloses_nothing(
            &entries_text(&entries),
            "the Application log",
            &self.secrets,
        );
        entries
    }

    /// Point 2's code 2 for the migration's argument, `arguments`: the line
    /// says the migration is not supported and repeats no value.
    fn refused_as_not_supported(&self, service: &TestService, arguments: &[&str]) {
        let entries = self.refused(service, arguments, 2, &NOT_SUPPORTED);
        assert_repeats_no_value(&entries, arguments);
    }

    /// The Amendment 2026-11-05, item 2: "an elevated console agent still can
    /// [open them], so a refusal leaves a console installation that runs": a
    /// real console agent starts from it as it stands and delivers a
    /// heartbeat.
    async fn assert_a_console_agent_runs(&self) {
        let mut console = ConsoleAgent::start(&self.config);
        wait_console_heartbeat(&self.heartbeat, &mut console).await;
    }
}

fn stop_cleanly(service: &TestService) {
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
}

/// The fragments of point 2's code 5 for the staging or legacy directories
/// `counting`: the line is an identity error, names each, says that the
/// product neither adopts nor migrates such a directory and sends the
/// operator to the installation runbook.
fn migration_directory_fragments(counting: &[&Path]) -> Vec<String> {
    let mut fragments = vec![
        "identity error".to_string(),
        NEITHER_ADOPTS_NOR_MIGRATES.to_string(),
        "nothing was changed".to_string(),
        INSTALLATION_RUNBOOK.to_string(),
    ];
    fragments.extend(counting.iter().map(|path| text(path)));
    fragments
}

/// Item 6, as the Amendment 2026-11-20, point 3, keeps it: a service started
/// with no arguments on a console layout, with its directories as the
/// runbook left them, keeps the outcome of the Amendment 2026-11-04's
/// checks, code 13 at the data directories before `RUNNING` (R17), and its
/// reason adds that the data directory holds a console installation's
/// identity files, which the product neither adopts nor migrates, and sends
/// the operator to the installation runbook; it no longer says "migrate
/// them before removing anything". It never starts a migration: nothing
/// changes, neither a byte nor an owner nor a DACL nor a name under `<P>`,
/// and it neither enrolls nor sends a heartbeat.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_013_a_start_without_arguments_on_a_console_layout_is_code_13() {
    let host = ConsoleHost::enrolled().await;
    let directories = host.directory_states();
    let files = host.file_states();
    let sent = host.heartbeat.attempts().len();
    let service = TestService::create(&service_args(&host.config));

    let entries = host.refused(
        &service,
        &[],
        13,
        &[
            "the service log cannot be written",
            &text(&host.paths().product_root),
            REFUSAL_HEAD,
            REFUSAL_TAIL,
        ],
    );
    assert_offers_no_migration(&entries, "the default refusal");

    assert_all_unchanged(&directories, "the default refusal");
    assert_all_unchanged(&files, "the default refusal");
    assert_no_migration(host.paths(), "the default refusal");
    assert_eq!(
        host.enroll.enroll_received_count(),
        1,
        "the service does not enroll; the one enrollment is the console agent's"
    );
    assert_eq!(host.heartbeat.attempts().len(), sent, "no heartbeat");
}

/// The Amendment 2026-11-20, points 2 and 3, with a console installation in
/// place: each refusal comes before any change, so each directory and file
/// keeps its names or bytes, owner and DACL, and no staging or legacy
/// directory is made.
///
/// - Point 2: the start argument `--migrate-legacy=` is code 2, whatever
///   its choice and whatever stands beside it, the line naming the argument,
///   saying the migration is not supported, sending the operator to the
///   installation runbook and repeating no value; a `--trust-anchor-sha256=`
///   without the adoption's argument stays code 2, and its line offers no
///   migration.
/// - The Amendment 2026-11-04's rule: a `--config` other than the fixed one,
///   the same path with forward slashes, is code 2 before anything else.
/// - Point 2: while an elevated console agent runs, the migration's argument
///   is still code 2, before the instance lock, and that agent runs on.
/// - Point 3: with the directories as an administrator would close them,
///   owner SYSTEM and a protected DACL that allows SYSTEM and
///   Administrators, `agent.toml` is what fails a start without arguments,
///   code 2 at the configuration with point 3's reason, and it changes
///   nothing.
///
/// Then a real elevated console agent starts from the installation and
/// delivers a heartbeat: the refusals left a console installation that runs.
/// The service neither enrolled nor sent a heartbeat.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_013_refusals_leave_a_console_installation_that_runs() {
    let host = ConsoleHost::enrolled().await;
    let paths = host.paths();
    let directories = host.directory_states();
    let files = host.file_states();
    let service = TestService::create(&service_args(&host.config));
    let anchor_hex = host.anchor.rsplit(':').next().expect("the anchor's hex");
    let upper_anchor = format!("--trust-anchor-sha256=sha256:{}", anchor_hex.to_uppercase());
    let wrong_anchor = format!("--trust-anchor-sha256=sha256:{}", "0".repeat(64));
    let anchor = host.anchor.as_str();

    // Point 2: the migration's argument.
    let sent = host.heartbeat.attempts().len();
    let cases: Vec<(&str, Vec<&str>)> = vec![
        ("the migration's arguments, remove", vec![REMOVE, anchor]),
        ("the migration's arguments, keep", vec![KEEP, anchor]),
        ("other fingerprints", vec![REMOVE, wrong_anchor.as_str()]),
        ("the choice without a fingerprint", vec![REMOVE]),
        (
            "a choice that is neither remove nor keep",
            vec!["--migrate-legacy=shred-the-copies", anchor],
        ),
        (
            "a fingerprint not in lowercase",
            vec![REMOVE, upper_anchor.as_str()],
        ),
        (
            "another start argument",
            vec![REMOVE, anchor, "--s57-other=s57-other-value"],
        ),
    ];
    for (what, arguments) in &cases {
        eprintln!("case: {what}");
        host.refused_as_not_supported(&service, arguments);
        assert_all_unchanged(&directories, what);
        assert_all_unchanged(&files, what);
        assert_no_migration(paths, what);
    }

    // Point 2: a fingerprint without the adoption's argument.
    {
        let what = "a fingerprint without --adopt-staged-config";
        eprintln!("case: {what}");
        let entries = host.refused(
            &service,
            &[anchor],
            2,
            &[
                "invalid migration arguments",
                "--trust-anchor-sha256",
                "--adopt-staged-config",
            ],
        );
        assert_repeats_no_value(&entries, &[anchor]);
        assert_offers_no_migration(&entries, what);
        assert_all_unchanged(&directories, what);
        assert_all_unchanged(&files, what);
        assert_no_migration(paths, what);
    }
    assert_eq!(host.heartbeat.attempts().len(), sent, "no heartbeat");

    // The Amendment 2026-11-04: `--config` comes first.
    {
        let slashes = text(&host.config).replace('\\', "/");
        let other = TestService::create(&service_args(Path::new(&slashes)));
        host.refused(
            &other,
            &[REMOVE, anchor],
            2,
            &[&format!(
                "--config must be {} in service mode",
                text(&host.config)
            )],
        );
    }
    assert_all_unchanged(&directories, "another --config");
    assert_all_unchanged(&files, "another --config");
    assert_no_migration(paths, "another --config");

    // Point 2: the argument is refused before the lock, so before any
    // change; the console agent that holds it runs on.
    {
        let mut console = ConsoleAgent::start(&host.config);
        wait_console_heartbeat(&host.heartbeat, &mut console).await;
        host.refused_as_not_supported(&service, &[REMOVE, anchor]);
        wait_console_heartbeat(&host.heartbeat, &mut console).await;
    }
    assert_all_unchanged(&directories, "a console agent running");
    assert_all_unchanged(&files, "a console agent running");
    assert_no_migration(paths, "a console agent running");

    // Point 3: the directories pass, `agent.toml` does not.
    let sent = host.heartbeat.attempts().len();
    set_security(&paths.product_root, PRIVATE_DIRECTORY_SDDL);
    set_security(&paths.data_root, PRIVATE_DIRECTORY_SDDL);
    let closed = host.directory_states();
    let entries = host.refused(
        &service,
        &[],
        2,
        &[
            "invalid config",
            &text(&host.config),
            REFUSAL_HEAD,
            REFUSAL_TAIL,
        ],
    );
    assert_offers_no_migration(&entries, "a start without arguments");
    assert_all_unchanged(&closed, "a start without arguments");
    assert_all_unchanged(&files, "a start without arguments");
    assert_no_migration(paths, "a start without arguments");
    assert_eq!(host.heartbeat.attempts().len(), sent, "no heartbeat");

    host.assert_a_console_agent_runs().await;
    assert_all_unchanged(&files, "the console agent's run");
    assert_eq!(
        host.enroll.enroll_received_count(),
        1,
        "the service does not enroll; the one enrollment is the console agent's"
    );
}

/// The Amendment 2026-11-05, item 4, as the Amendment 2026-11-20, point 2,
/// leaves it: a staging or legacy directory counts only when SYSTEM owns it
/// and its DACL passes `agent.toml`'s policy. This is R17's attack with
/// `<P>` still open: an ordinary user creates the name and, as its owner,
/// gives it a DACL that passes, so the owner alone tells it apart, with a
/// record that parses inside. A start without arguments goes on as if it
/// were absent: the default refusal with point 3's reason, not point 2's
/// code 5. With the migration's arguments the start is point 2's code 2,
/// before any name is read. Neither reads, changes or deletes the name, and
/// the console installation still runs.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_013_a_name_an_ordinary_user_planted_changes_no_outcome() {
    let host = ConsoleHost::enrolled().await;
    let paths = host.paths();
    let user = TestUser::create();
    let owner = user.sid();
    let service = TestService::create(&service_args(&host.config));
    let given = host.anchor.trim_start_matches("--trust-anchor-sha256=");
    let record = record_json("remove", given, &host.key_fingerprint());
    let plant = |decoy: &Path| {
        create_directory_with(
            decoy,
            &format!("O:{owner}D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)"),
        );
        let inside = decoy.join("migration.json");
        create_file_with(
            &inside,
            &format!("O:{owner}D:P(A;;FA;;;SY)(A;;FA;;;BA)"),
            record.as_bytes(),
        );
        [State::read(decoy, true), State::read(&inside, false)]
    };
    let remove = |decoy: &Path| {
        privileged_delete(&decoy.join("migration.json"));
        privileged_delete(decoy);
    };
    let sent = host.heartbeat.attempts().len();
    let decoys = [&paths.staging, &paths.legacy];

    for decoy in decoys {
        let name = decoy.display();
        eprintln!("case: {name}, no arguments");
        let planted = plant(decoy);
        let directories = host.directory_states();
        let files = host.file_states();
        let entries = host.refused(
            &service,
            &[],
            13,
            &[
                "the service log cannot be written",
                REFUSAL_HEAD,
                REFUSAL_TAIL,
            ],
        );
        assert_offers_no_migration(&entries, "a planted name, no arguments");
        assert_all_unchanged(&planted, "a planted name, no arguments");
        assert_all_unchanged(&directories, "a planted name, no arguments");
        assert_all_unchanged(&files, "a planted name, no arguments");
        remove(decoy);
    }

    for decoy in decoys {
        let name = decoy.display();
        eprintln!("case: {name}, with the arguments");
        let planted = plant(decoy);
        let directories = host.directory_states();
        let files = host.file_states();
        host.refused_as_not_supported(&service, &[REMOVE, &host.anchor]);
        assert_all_unchanged(&planted, "a planted name, with the arguments");
        assert_all_unchanged(&directories, "a planted name, with the arguments");
        assert_all_unchanged(&files, "a planted name, with the arguments");
        remove(decoy);
    }

    assert_eq!(host.heartbeat.attempts().len(), sent, "no heartbeat");
    assert_no_migration(paths, "a planted name");
    host.assert_a_console_agent_runs().await;
}

/// The Amendment 2026-11-20, point 2: a start without arguments beside a
/// console layout and a staging or legacy directory that counts, owned by
/// SYSTEM with a DACL that passes `agent.toml`'s policy, is code 5 before
/// any change, its line naming the layout: each directory that counts and
/// the console installation's identity files beside them, and saying that
/// the product neither adopts nor migrates them (point 3). It completes,
/// reverses or continues no migration: nothing changes, neither a byte nor
/// an owner nor a DACL nor a name under `<P>`, and it never reports
/// `RUNNING`, enrolls or sends a heartbeat. The migration's argument beside
/// them is point 2's code 2 all the same. A real elevated console agent
/// then still starts from the installation.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_013_a_migration_directory_beside_a_console_layout_is_code_5() {
    let host = ConsoleHost::enrolled().await;
    let paths = host.paths();
    let service = TestService::create(&service_args(&host.config));
    let given = host.anchor.trim_start_matches("--trust-anchor-sha256=");
    let record = record_json("remove", given, &host.key_fingerprint());
    let sent = host.heartbeat.attempts().len();
    let beside = format!("s identity files in {}", text(&paths.data_root));

    let staging = paths.staging.as_path();
    let legacy = paths.legacy.as_path();
    let cases: [(&str, Vec<&Path>); 3] = [
        ("a staging directory", vec![staging]),
        ("a legacy directory", vec![legacy]),
        ("a staging and a legacy directory", vec![staging, legacy]),
    ];
    for (what, counting) in &cases {
        eprintln!("case: {what}");
        let mut planted = Vec::new();
        for directory in counting {
            create_directory_with(directory, PRIVATE_DIRECTORY_SDDL);
        }
        // What an interrupted migration leaves in its staging directory.
        if counting.contains(&staging) {
            let inside = staging.join("migration.json");
            create_file_with(&inside, CONFIG_SDDL, record.as_bytes());
            planted.push(State::read(&inside, false));
        }
        planted.extend(
            counting
                .iter()
                .map(|directory| State::read(directory, true)),
        );
        let directories = host.directory_states();
        let files = host.file_states();

        let mut fragments = migration_directory_fragments(counting);
        fragments.push("beside a console installation".to_string());
        fragments.push(beside.clone());
        let fragments: Vec<&str> = fragments.iter().map(String::as_str).collect();
        let entries = host.refused(&service, &[], 5, &fragments);
        assert_offers_no_migration(&entries, what);
        assert_all_unchanged(&planted, what);
        assert_all_unchanged(&directories, what);
        assert_all_unchanged(&files, what);

        // Point 2: the migration's argument is refused before the
        // directories are read.
        let with_arguments = format!("{what}, with the migration's arguments");
        host.refused_as_not_supported(&service, &[KEEP, &host.anchor]);
        assert_all_unchanged(&planted, &with_arguments);
        assert_all_unchanged(&directories, &with_arguments);
        assert_all_unchanged(&files, &with_arguments);

        let inside = staging.join("migration.json");
        if privileged_exists(&inside) {
            privileged_delete(&inside);
        }
        for directory in counting {
            privileged_delete(directory);
        }
    }

    assert_eq!(host.heartbeat.attempts().len(), sent, "no heartbeat");
    assert_no_migration(paths, "the planted directories removed");
    host.assert_a_console_agent_runs().await;
    assert_eq!(
        host.enroll.enroll_received_count(),
        1,
        "the service does not enroll; the one enrollment is the console agent's"
    );
}

/// The Amendment 2026-11-20, points 2 and 3, on a host the setup installed:
/// a legacy directory beside `identity\` stays an enrolled host's, whose
/// service refuses it when it counts. The data root has its production
/// security and an identity at `identity\`; beside it the fixture plants a
/// directory that counts, owned by SYSTEM with a DACL that passes
/// `agent.toml`'s policy: a legacy directory with the record a committed
/// migration leaves in the data root, the same without the record, a
/// staging directory, and both. A start without arguments is code 5 before
/// any change, its line naming each directory, saying that the product
/// neither adopts nor migrates it and sending the operator to the
/// installation runbook: it completes no migration and deletes or restricts
/// no old copy, so nothing changes, and the service never reports `RUNNING`,
/// enrolls or sends a heartbeat. Once the fixture removes them, the same
/// start runs and delivers a heartbeat: the directory was the refusal's
/// cause.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_013_a_migration_directory_on_an_installed_host_is_code_5() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    let heartbeat = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let root = FixedDataRoot::plant();
    let paths = root.paths.clone();
    let config = write_config(
        &root,
        &enroll.base_url,
        &heartbeat.base_url,
        &pki.trust_anchor_pem,
        &Knobs::default(),
    );
    provision_identity(&root, &pki);
    let anchor = anchor_fingerprints(pki.trust_anchor_pem.as_bytes());
    let record = record_json("keep", &anchor[0], "the-legacy-key-fingerprint");
    let service = TestService::create(&service_args(&config));
    let installed_states = || {
        let mut states = vec![
            State::read(&paths.product_root, true),
            State::read(&paths.data_root, true),
            State::read(&paths.identity_dir, true),
        ];
        states.extend(
            [
                &paths.config,
                &paths.trust_anchor,
                &paths.cert,
                &paths.key,
                &paths.identity,
            ]
            .map(|path| State::read(path, false)),
        );
        states
    };

    let staging = paths.staging.as_path();
    let legacy = paths.legacy.as_path();
    let cases: [(&str, Vec<&Path>, bool); 4] = [
        (
            "a legacy directory and a committed record",
            vec![legacy],
            true,
        ),
        ("a legacy directory", vec![legacy], false),
        ("a staging directory", vec![staging], false),
        (
            "a staging and a legacy directory",
            vec![staging, legacy],
            false,
        ),
    ];
    for (what, counting, with_record) in &cases {
        eprintln!("case: {what}");
        let mut planted = Vec::new();
        for directory in counting {
            create_directory_with(directory, PRIVATE_DIRECTORY_SDDL);
        }
        if *with_record {
            root.write_file(&paths.record, CONFIG_SDDL, record.as_bytes());
            planted.push(State::read(&paths.record, false));
        }
        // A legacy directory holds the old copies a migration kept.
        let copy = legacy.join("agent.toml");
        if counting.contains(&legacy) {
            create_file_with(
                &copy,
                CONFIG_SDDL,
                b"[agent]\nid = \"a copy of the console's agent.toml\"\n",
            );
            planted.push(State::read(&copy, false));
        }
        planted.extend(
            counting
                .iter()
                .map(|directory| State::read(directory, true)),
        );
        let states = installed_states();

        let fragments = migration_directory_fragments(counting);
        let fragments: Vec<&str> = fragments.iter().map(String::as_str).collect();
        let entries = refused_with_arguments(&service, &[], 5, &fragments);
        assert_discloses_nothing(&entries_text(&entries), "the Application log", &[]);
        assert_offers_no_migration(&entries, what);
        assert_all_unchanged(&planted, what);
        assert_all_unchanged(&states, what);
        assert!(
            !privileged_exists(&paths.data_root.join("logs")),
            "{what}: the file log is not opened"
        );
        assert_eq!(enroll.enroll_received_count(), 0, "{what}: no enrollment");
        assert!(heartbeat.attempts().is_empty(), "{what}: no heartbeat");

        if *with_record {
            privileged_delete(&paths.record);
        }
        if privileged_exists(&copy) {
            privileged_delete(&copy);
        }
        for directory in counting {
            privileged_delete(directory);
        }
    }

    // The positive control: the same host, without them, runs.
    assert_no_migration(&paths, "the planted directories removed");
    let online_before = online_count(&heartbeat);
    service.start().expect("start without arguments");
    service.wait_for(SERVICE_RUNNING, Duration::from_secs(60));
    assert!(
        common::wait_until(Duration::from_secs(60), || online_count(&heartbeat)
            > online_before)
        .await,
        "the installed host's service delivers heartbeats"
    );
    stop_cleanly(&service);
    assert_eq!(
        enroll.enroll_received_count(),
        0,
        "the service loads its identity and does not enroll"
    );
}

/// The Amendment 2026-11-20, point 2: the migration's argument on a host
/// that is neither a console layout nor a migration in progress is code 2
/// before any change, the line naming the argument, saying the migration is
/// not supported and sending the operator to the installation runbook: an
/// installed host's data root, with its production security and an identity
/// at `identity\`, gets no staging or legacy directory, nothing is written,
/// and the service neither enrolls nor sends a heartbeat.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_013_migration_arguments_without_a_console_layout_are_code_2() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    let heartbeat = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let root = FixedDataRoot::plant();
    let config = write_config(
        &root,
        &enroll.base_url,
        &heartbeat.base_url,
        &pki.trust_anchor_pem,
        &Knobs::default(),
    );
    provision_identity(&root, &pki);
    let mut states = vec![
        State::read(&root.paths.product_root, true),
        State::read(&root.paths.data_root, true),
        State::read(&root.paths.identity_dir, true),
    ];
    states.extend(
        [
            &root.paths.config,
            &root.paths.trust_anchor,
            &root.paths.cert,
            &root.paths.key,
            &root.paths.identity,
        ]
        .map(|path| State::read(path, false)),
    );
    let anchor = anchor_fingerprints(pki.trust_anchor_pem.as_bytes());
    let anchor = format!("--trust-anchor-sha256={}", anchor[0]);
    let service = TestService::create(&service_args(&config));

    let entries = refused_with_arguments(&service, &[REMOVE, &anchor], 2, &NOT_SUPPORTED);

    assert_discloses_nothing(&entries_text(&entries), "the Application log", &[]);
    assert_repeats_no_value(&entries, &[REMOVE, &anchor]);
    assert_all_unchanged(&states, "migration arguments on an installed host");
    assert_no_migration(&root.paths, "migration arguments on an installed host");
    assert_eq!(enroll.enroll_received_count(), 0, "no enrollment");
    assert!(heartbeat.attempts().is_empty(), "no heartbeat");
}
