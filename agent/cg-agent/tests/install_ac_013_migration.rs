//! SPEC-024 install_ac_013: the legacy migration (Amendment 2026-11-05), by
//! a real Windows service run as LocalSystem, from a console installation a
//! real console agent enrolled at the fixed paths: the default refusal,
//! item 2's refusals with that installation in place and an elevated console
//! agent still starting from it, a name an ordinary user planted beside the
//! data root, an operation Windows makes wait, each choice for the old
//! copies, one of them out of reach, and a restart without arguments.
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
//! console agent alone; the migration is first asked for by the start that
//! the test asserts. Fragments of the Application-log entries avoid the
//! characters its XML escapes (`'`). A real ingest and reboots stay with the
//! isolated machine of point 4.
#![cfg(windows)]

mod common;
mod scm;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::CertificateDer;
use scm::*;
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};
use windows_sys::Win32::System::Services::{
    SERVICE_ACCEPT_SHUTDOWN, SERVICE_ACCEPT_STOP, SERVICE_RUNNING, SERVICE_STOPPED,
};

/// A token value no entry, log line or remaining file may carry.
const SECRET_TOKEN: &str = "s57-enrollment-token-never-disclosed";

/// Item 1's start arguments.
const REMOVE: &str = "--migrate-legacy=remove";
const KEEP: &str = "--migrate-legacy=keep";

/// Item 6's reason, split at the apostrophe the rendered XML escapes.
const REFUSAL_HEAD: &str = "the data directory holds a console installation";
const REFUSAL_TAIL: &str = "s identity files: migrate them before removing anything";

/// Item 2: what each refusal before the commit says.
const IN_PLACE: &str = "the console installation is in place";

/// The console layout's files, as `privileged_listing` sorts them.
const CONSOLE_FILES: [&str; 5] = [
    "agent.toml",
    "cert.pem",
    "identity.json",
    "key.dat",
    "server-ca.pem",
];
/// Those item 5 gives SYSTEM alone.
const IDENTITY_FILES: [&str; 3] = ["cert.pem", "identity.json", "key.dat"];
/// FR-014's leftover temporary, which items 2.2, 2.4 and 5 treat as a copy.
const HYGIENE_TEMPORARY: &str = "agent.toml.hygiene-tmp";

/// What item 2.5 sets itself, not from the legacy configuration.
const SET_BY_THE_MIGRATION: [(&str, &str); 6] = [
    ("server", "trust_anchor_path"),
    ("enrollment", "cert_path"),
    ("enrollment", "key_path"),
    ("enrollment", "identity_path"),
    ("agent", "hostname"),
    ("enrollment", "token"),
];

/// A copy's security after someone widened it: not SYSTEM's, and readable by
/// every local user.
const WIDENED_COPY_SDDL: &str = "O:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FR;;;BU)";

/// The entry an ordinary user creates through a handle it holds.
const ENTRY_THROUGH_A_HANDLE: &str = "created-through-a-held-handle.txt";

/// How long a start may take to report `RUNNING` through a migration.
const MIGRATION_DEADLINE: Duration = Duration::from_secs(120);

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

/// The hostname Windows reports.
fn windows_hostname() -> String {
    let output = std::process::Command::new("hostname")
        .output()
        .expect("run hostname");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// The enrollment mock answers a fixed `expires_at` (`common::enroll_response`)
/// that is already past, and item 2.3 refuses an identity that expired. A
/// real server's answer lies ahead, so the fixture moves that one field,
/// rewriting the file through its name: its file ID, owner and DACL stay, and
/// the runner's account, which the DACL allows, needs no privilege for it.
fn renew_expiry(identity_json: &Path) {
    let bytes = std::fs::read(identity_json).expect("read identity.json");
    let mut identity: Value = serde_json::from_slice(&bytes).expect("identity.json parses");
    identity["expires_at"] = json!((chrono::Utc::now() + chrono::Duration::days(365))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
    let bytes = serde_json::to_vec_pretty(&identity).expect("identity.json");
    std::fs::write(identity_json, bytes).expect("rewrite identity.json");
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

/// The console layout as the amendment's *What surfaced it* describes it: the
/// five files directly in the data root and nothing else there or in `<P>`,
/// owned by Administrators, the default owner of the runner's built-in
/// Administrator; each identity file with the protected DACL the console
/// agent sets, SYSTEM and the account that ran it; `agent.toml` without the
/// token it enrolled with, which the console's FR-014 removed.
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

/// Owner SYSTEM and a protected DACL of allow entries, each of full access,
/// for `principals` and no one else; on a directory each entry applies to it
/// and is inherited by what it holds, as the SDDLs of the SPEC's custody give
/// it.
fn assert_owned_by_system_allowing(path: &Path, principals: &[&str], directory: bool) {
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
    for ace in &security.aces {
        assert!(
            ace.kind == "A" && ace.is_full_access(),
            "{}'s DACL holds full-access allow entries alone: {sddl}",
            path.display()
        );
        let inheritance = if directory {
            ace.has_flag("OI") && ace.has_flag("CI") && !ace.has_flag("IO")
        } else {
            // A protected DACL holds no inherited entry, so a file's carries
            // no inheritance of its own.
            ace.flags.iter().all(|flag| flag == "ID")
        };
        assert!(
            inheritance,
            "{}'s entries have its inheritance: {sddl}",
            path.display()
        );
    }
    let mut allowed: Vec<&str> = security.aces.iter().map(|ace| ace.sid.as_str()).collect();
    allowed.sort_unstable();
    let mut expected = principals.to_vec();
    expected.sort_unstable();
    assert_eq!(
        allowed,
        expected,
        "{}'s DACL allows them alone: {sddl}",
        path.display()
    );
}

/// Items 2.1 and 2.4: owner SYSTEM and a protected DACL that allows SYSTEM
/// and Administrators.
fn assert_system_and_administrators(path: &Path, directory: bool) {
    assert_owned_by_system_allowing(path, &["BA", "SY"], directory);
}

/// §Data contracts §2's identity security, which item 5 also gives the
/// legacy directory and its identity files: owner SYSTEM and a protected
/// DACL whose single entry allows SYSTEM full access; for a directory,
/// inherited by what it holds.
fn assert_system_alone(path: &Path, directory: bool) {
    assert_owned_by_system_allowing(path, &["SY"], directory);
}

/// `text` carries no SID, token, PEM block or key material (item 7).
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

/// What entry 1002 says after `IN_PLACE`: item 2's list of what was
/// restricted. Everything before it is the reason, which may name a path of
/// the same tree.
fn after_in_place(entries: &[Entry]) -> &str {
    let failed = entry(entries, 1002);
    failed
        .data
        .split_once(IN_PLACE)
        .unwrap_or_else(|| panic!("the line says {IN_PLACE:?}: {failed:?}"))
        .1
}

/// Whether `line` names `path` itself, not only something under it: `<P>`
/// is a prefix of every other path of the tree.
fn names_path(line: &str, path: &Path) -> bool {
    let name = text(path);
    line.match_indices(&name)
        .any(|(at, _)| !line[at + name.len()..].starts_with('\\'))
}

/// Item 2: "each refusal before the commit says the console installation is
/// in place, and which of its objects item 2.1 or 2.4 restricted": after
/// `IN_PLACE` the line names `<P>` and the data root, when `closed`, and each
/// of `files`.
fn assert_in_place(entries: &[Entry], paths: &FixedPaths, closed: bool, files: &[&str]) {
    let restricted = after_in_place(entries);
    if closed {
        for directory in [&paths.product_root, &paths.data_root] {
            assert!(
                names_path(restricted, directory),
                "the line names {} as restricted: {restricted}",
                directory.display()
            );
        }
    }
    for name in files {
        assert!(
            restricted.contains(name),
            "the line names {name} as restricted: {restricted}"
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

/// A file-log line with its field names and values run together as
/// `name=value`, whichever way a line writes them: `"choice":"keep"`,
/// `choice=keep` and `choice: keep` are `choice=keep`, and `copies_not_reached`
/// is `copiesnotreached`.
fn flat(line: &str) -> String {
    line.chars()
        .filter(|c| !matches!(c, '"' | ' ' | '_'))
        .map(|c| if c == ':' { '=' } else { c })
        .collect()
}

/// What the console installation held before the migration.
struct Legacy {
    cert: Vec<u8>,
    key: Vec<u8>,
    identity_bytes: Vec<u8>,
    identity: Value,
    config: Vec<u8>,
    anchor: Vec<u8>,
    /// FR-014's leftover temporary, when the host has one.
    leftover: Option<Vec<u8>>,
    key_fingerprint: String,
}

impl Legacy {
    fn read(paths: &FixedPaths) -> Self {
        let file = |name: &str| privileged_read(&paths.data_root.join(name));
        let identity_bytes = file("identity.json");
        let identity: Value =
            serde_json::from_slice(&identity_bytes).expect("identity.json parses");
        Legacy {
            cert: file("cert.pem"),
            key: file("key.dat"),
            leftover: privileged_exists(&paths.hygiene_temporary).then(|| file(HYGIENE_TEMPORARY)),
            key_fingerprint: identity["agent_pubkey_fingerprint"]
                .as_str()
                .expect("identity.json's fingerprint")
                .to_string(),
            identity_bytes,
            identity,
            config: file("agent.toml"),
            anchor: file("server-ca.pem"),
        }
    }

    /// The checked files, as `privileged_listing` sorts them.
    fn names(&self) -> Vec<String> {
        let mut names = CONSOLE_FILES.map(String::from).to_vec();
        if self.leftover.is_some() {
            names.push(HYGIENE_TEMPORARY.to_string());
            names.sort();
        }
        names
    }
}

/// A host the console runbook set up (the amendment's *What surfaced it*):
/// `<P>` and the data root inheriting `%ProgramData%`'s permissions,
/// `server-ca.pem` and `agent.toml` written plainly, and a real console agent
/// that enrolled, elevated, against the test's own enrollment and heartbeat
/// servers, wrote its identity files directly in the data root and was
/// ended. The heartbeat server verifies signatures under the key the console
/// agent generated, so what it accepts later was signed by that key.
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

        renew_expiry(&root.paths.data_root.join("identity.json"));
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

    /// What `docs/operations/install.md` warns of when FR-014's best-effort
    /// rewrite fails: the token stays in `agent.toml`, rewritten here through
    /// its name, which keeps its file ID, owner and DACL, and in the rewrite's
    /// temporary, a new file the console's account owns by default.
    fn leave_a_token_behind(&self) {
        let config = std::fs::read_to_string(&self.config).expect("read agent.toml");
        let with_token = config.replacen(
            "[enrollment]",
            &format!("[enrollment]\ntoken = \"{SECRET_TOKEN}\""),
            1,
        );
        assert_ne!(with_token, config, "agent.toml has an [enrollment] table");
        std::fs::write(&self.config, &with_token).expect("put the token back in agent.toml");
        let temporary = &self.paths().hygiene_temporary;
        std::fs::write(temporary, &with_token).expect("leave the rewrite's temporary");
        let sddl = owner_and_dacl(temporary);
        assert_eq!(
            parse_sddl(&sddl).owner,
            "BA",
            "the fixture's temporary is owned by Administrators: {sddl}"
        );
    }

    /// The file of that name in the data root.
    fn console_file(&self, name: &str) -> PathBuf {
        self.paths().data_root.join(name)
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

    /// Item 2: "an elevated console agent still can [open them], so a
    /// refusal leaves a console installation that runs": a real console
    /// agent starts from it as it stands and delivers a heartbeat.
    async fn assert_a_console_agent_runs(&self) {
        let mut console = ConsoleAgent::start(&self.config);
        wait_console_heartbeat(&self.heartbeat, &mut console).await;
    }

    /// The heartbeats sent from attempt number `from` on: the mock answered
    /// each 200, having verified it under the console agent's key on a
    /// connection that presented a client certificate its CA issued, and
    /// each carries the legacy `agent_id`.
    fn assert_heartbeats_carry_the_legacy_identity(&self, from: usize) {
        let attempts = self.heartbeat.attempts();
        assert!(
            attempts.len() > from,
            "the service delivers heartbeats after {from}"
        );
        for attempt in &attempts[from..] {
            assert_eq!(
                attempt.status, 200,
                "the mock accepts the envelope under the legacy key"
            );
            assert_eq!(
                attempt.envelope["agent_id"].as_str(),
                Some(common::TEST_AGENT_ID),
                "the envelope carries the legacy agent_id"
            );
        }
    }

    /// Every line of the service's log directory, the active file and any
    /// archive.
    fn log_text(&self) -> String {
        let logs = self.paths().data_root.join("logs");
        let names = privileged_listing(&logs).expect("the service's log directory");
        assert!(
            names.iter().any(|name| name == "agent.jsonl"),
            "the active log is there: {names:?}"
        );
        names
            .iter()
            .map(|name| String::from_utf8_lossy(&privileged_read(&logs.join(name))).to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Item 7: one line of the file log says the migration happened, with
    /// `agent_id`, the key's fingerprint, the choice and the copies' result.
    /// The SPEC fixes no spelling, so this fixes the least a line can state
    /// them with: a `choice` field; the word `copies`; what was done to them,
    /// `removed` or `restricted`; and that none is left out of reach, a
    /// `not_reached` of 0. With `remove` those two say no copy is left.
    fn says_migrated(&self, legacy: &Legacy, choice: &str) -> bool {
        let outcome = if choice == "remove" {
            "removed"
        } else {
            "restricted"
        };
        self.log_text().lines().any(|line| {
            let fields = flat(line);
            line.contains(common::TEST_AGENT_ID)
                && line.contains(&legacy.key_fingerprint)
                && fields.contains(&format!("choice={choice}"))
                && line.contains("copies")
                && line.contains(outcome)
                && fields.contains("notreached=0")
        })
    }

    /// The copies the record names as not reached; `None` while it is being
    /// replaced.
    fn copies_not_reached(&self) -> Option<Vec<String>> {
        let bytes = try_privileged_read(&self.paths().record).ok()?;
        let record: Value = serde_json::from_slice(&bytes).ok()?;
        serde_json::from_value(record["copies_not_reached"].clone()).ok()
    }

    /// A migrating start reports `RUNNING` with its checkpoint advancing,
    /// delivers a heartbeat after `online_before` and, item 5 having run
    /// after `RUNNING`, logs the migration (item 7). Item 1's "no privilege is
    /// enabled", as far as a token read after the fact shows it: a privilege
    /// enabled and left on is caught, one enabled for a moment is the
    /// review's.
    async fn wait_migrated(
        &self,
        service: &TestService,
        legacy: &Legacy,
        choice: &str,
        online_before: usize,
    ) {
        let running = service.wait_for(SERVICE_RUNNING, MIGRATION_DEADLINE);
        assert_pending_progress(&running, "START_PENDING through a migration");
        assert_eq!(
            running.status.dwControlsAccepted,
            SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN,
            "RUNNING accepts STOP and SHUTDOWN and no other control"
        );
        assert_eq!(
            enabled_custody_privileges(running.status.dwProcessId),
            Vec::<String>::new(),
            "the migration enabled no privilege"
        );
        assert!(
            common::wait_until(Duration::from_secs(90), || online_count(&self.heartbeat)
                > online_before)
            .await,
            "the service delivers heartbeats with the migrated identity"
        );
        assert!(
            common::wait_until(Duration::from_secs(60), || self
                .says_migrated(legacy, choice))
            .await,
            "the file log says so, with agent_id, the key's fingerprint, the choice and the \
             copies' result: {:.2000}",
            self.log_text()
        );
    }

    /// Item 2.5: what the commit left in the data root, with its final
    /// security. The identity is the console's: `cert.pem` and `key.dat`,
    /// "its sealed bytes unchanged", byte for byte; `agent.toml` is generated
    /// from the legacy configuration; `server-ca.pem` holds the verified
    /// certificates.
    fn assert_migrated(&self, legacy: &Legacy) {
        let paths = self.paths();
        assert_system_alone(&paths.identity_dir, true);
        for file in [&paths.cert, &paths.key, &paths.identity] {
            assert_system_alone(file, false);
        }
        assert_eq!(
            privileged_listing(&paths.identity_dir),
            Some(vec![
                "cert.pem".to_string(),
                "identity.json".to_string(),
                "key.dat".to_string()
            ]),
            "identity\\ holds the three files and nothing else"
        );
        assert!(
            privileged_read(&paths.cert) == legacy.cert,
            "cert.pem is the console's"
        );
        assert!(
            privileged_read(&paths.key) == legacy.key,
            "key.dat keeps its sealed bytes"
        );
        let identity: Value =
            serde_json::from_slice(&privileged_read(&paths.identity)).expect("identity.json");
        assert_eq!(identity, legacy.identity, "identity.json is the console's");

        for directory in [&paths.product_root, &paths.data_root] {
            assert_system_and_administrators(directory, true);
        }
        for file in [&paths.config, &paths.trust_anchor] {
            assert_system_and_administrators(file, false);
        }
        assert_eq!(
            anchor_fingerprints(&privileged_read(&paths.trust_anchor)),
            anchor_fingerprints(&legacy.anchor),
            "server-ca.pem holds the verified certificates"
        );
        self.assert_generated_config(legacy);
        assert_eq!(
            owner_and_dacl(&paths.record),
            owner_and_dacl(&paths.config),
            "migration.json has agent.toml's owner and DACL"
        );
        assert_eq!(
            privileged_listing(&paths.data_root),
            Some(
                [
                    "agent.toml",
                    "identity",
                    "logs",
                    "migration.json",
                    "server-ca.pem"
                ]
                .map(String::from)
                .to_vec()
            ),
            "the data root is the installed layout, with the record"
        );
        assert!(
            !privileged_exists(&paths.staging),
            "the staging directory became the data root"
        );
    }

    /// Item 2.5: the fixed paths, the hostname Windows reports and no
    /// `enrollment.token`, even with one in the legacy file; every other
    /// setting of the legacy configuration, `agent.id` and the URLs among
    /// them, keeps its value. A default the legacy file left unspoken may be
    /// spelled out.
    fn assert_generated_config(&self, legacy: &Legacy) {
        let paths = self.paths();
        let generated = String::from_utf8(privileged_read(&paths.config)).expect("UTF-8");
        let parsed: toml::Value = toml::from_str(&generated).expect("agent.toml parses");
        let setting = |table: &str, key: &str| {
            parsed
                .get(table)
                .and_then(|table| table.get(key))
                .and_then(toml::Value::as_str)
                .unwrap_or_else(|| panic!("{table}.{key} in {generated}"))
                .to_string()
        };
        for (table, key, path) in [
            ("server", "trust_anchor_path", &paths.trust_anchor),
            ("enrollment", "cert_path", &paths.cert),
            ("enrollment", "key_path", &paths.key),
            ("enrollment", "identity_path", &paths.identity),
        ] {
            assert_eq!(setting(table, key), text(path), "{table}.{key} is fixed");
        }
        let hostname = setting("agent", "hostname");
        assert!(
            hostname.eq_ignore_ascii_case(&windows_hostname()),
            "agent.hostname is the one Windows reports, not the legacy one: {hostname}"
        );
        assert!(
            parsed["enrollment"].get("token").is_none() && !generated.contains(SECRET_TOKEN),
            "no enrollment.token: {generated}"
        );
        let console: toml::Value = toml::from_str(&String::from_utf8_lossy(&legacy.config))
            .expect("the console's agent.toml parses");
        for (table, settings) in console.as_table().expect("a TOML document") {
            let settings = settings.as_table().expect("the console's tables");
            for (key, value) in settings {
                if !SET_BY_THE_MIGRATION.contains(&(table.as_str(), key.as_str())) {
                    assert_eq!(
                        parsed.get(table).and_then(|table| table.get(key)),
                        Some(value),
                        "{table}.{key} keeps its legacy value: {generated}"
                    );
                }
            }
        }
    }

    /// Item 3: `migration.json` in the data root has `agent.toml`'s owner and
    /// DACL, and holds what item 2.5 lists: the choice, the fingerprints
    /// given, `agent_id`, the key's fingerprint, the enrolling account's SID
    /// and an ID per checked file; no reversal is pending and every copy was
    /// reached.
    fn assert_record(&self, path: &Path, legacy: &Legacy, choice: &str, enrolling: &str) {
        assert_system_and_administrators(path, false);
        let record: Value =
            serde_json::from_slice(&privileged_read(path)).expect("migration.json parses");
        assert_eq!(record["choice"], json!(choice), "the choice: {record}");
        let given = self.anchor.trim_start_matches("--trust-anchor-sha256=");
        assert_eq!(
            record["fingerprints"],
            json!([given]),
            "the fingerprints given: {record}"
        );
        assert_eq!(
            record["agent_id"],
            json!(common::TEST_AGENT_ID),
            "agent_id: {record}"
        );
        assert_eq!(
            record["key_fingerprint"],
            json!(legacy.key_fingerprint),
            "the key's fingerprint: {record}"
        );
        assert_eq!(
            record["enrolling_account"],
            json!(enrolling),
            "the enrolling account's SID: {record}"
        );
        for name in legacy.names() {
            assert!(
                record["files"].get(&name).is_some(),
                "the file ID of {name}: {record}"
            );
        }
        assert_eq!(
            record["reversal_pending"],
            json!(false),
            "no reversal is pending: {record}"
        );
        assert_eq!(
            record["copies_not_reached"],
            json!([]),
            "every copy was reached: {record}"
        );
    }

    /// Item 5: with `remove` no copy and no legacy directory are left; with
    /// `keep` the copies stay, restricted, with their bytes: SYSTEM alone for
    /// the identity files and the directory, SYSTEM and Administrators for
    /// the others, FR-014's leftover temporary among them.
    fn assert_copies(&self, legacy: &Legacy, choice: &str) {
        let directory = &self.paths().legacy;
        if choice == "remove" {
            assert!(
                !privileged_exists(directory),
                "remove leaves no agent.legacy"
            );
            return;
        }
        assert_eq!(
            privileged_listing(directory),
            Some(legacy.names()),
            "keep leaves each copy in agent.legacy"
        );
        assert_system_alone(directory, true);
        for name in legacy.names() {
            let copy = directory.join(&name);
            if IDENTITY_FILES.contains(&name.as_str()) {
                assert_system_alone(&copy, false);
            } else {
                assert_system_and_administrators(&copy, false);
            }
        }
        for (name, bytes) in [
            ("cert.pem", &legacy.cert),
            ("key.dat", &legacy.key),
            ("identity.json", &legacy.identity_bytes),
            ("agent.toml", &legacy.config),
            ("server-ca.pem", &legacy.anchor),
        ]
        .into_iter()
        .chain(
            legacy
                .leftover
                .iter()
                .map(|bytes| (HYGIENE_TEMPORARY, bytes)),
        ) {
            assert!(
                privileged_read(&directory.join(name)) == *bytes,
                "the copy of {name} keeps its bytes"
            );
        }
    }

    /// Item 7: no line of the file log and no Application-log entry carries
    /// a SID, a token or key bytes.
    fn assert_logs_disclose_nothing(&self, entries: &[Entry]) {
        assert_discloses_nothing(&self.log_text(), "the file log", &self.secrets);
        assert_discloses_nothing(&entries_text(entries), "the Application log", &self.secrets);
    }

    /// install_ac_013: neither a standard user created for the test nor the
    /// restricted administrator token can read, write, delete, change the
    /// DACL of or take ownership of the migrated identity, its configuration,
    /// the record, the closed directories or the remaining copies, while both
    /// read a file planted for them in the same tree (the probe's positive
    /// control). Runs last: the control is an entry of the data root.
    fn assert_others_are_denied(&self, legacy: &Legacy, choice: &str) {
        let paths = self.paths();
        let mut targets = vec![
            (paths.product_root.clone(), true),
            (paths.data_root.clone(), true),
            (paths.identity_dir.clone(), true),
            (paths.cert.clone(), false),
            (paths.key.clone(), false),
            (paths.identity.clone(), false),
            (paths.config.clone(), false),
            (paths.trust_anchor.clone(), false),
            (paths.record.clone(), false),
        ];
        if choice == "keep" {
            targets.push((paths.legacy.clone(), true));
            targets.extend(
                legacy
                    .names()
                    .iter()
                    .map(|name| (paths.legacy.join(name), false)),
            );
        }
        let readable = self.console_file("access-probe-control.txt");
        self.root.write_file(
            &readable,
            "O:SYD:P(A;;FA;;;SY)(A;;FR;;;BU)",
            b"readable by BUILTIN\\Users",
        );
        assert_denied_to_others(&targets, std::slice::from_ref(&readable));
    }
}

/// Effective access (`access_problems`): under a standard user created for
/// the test, and under the restricted administrator token, whose user is the
/// runner's built-in Administrator, the account that enrolled, every attempt
/// on each `(path, directory)` is denied, while each of `readable` opens.
fn assert_denied_to_others(targets: &[(PathBuf, bool)], readable: &[PathBuf]) {
    let targets: Vec<ProbeTarget> = targets
        .iter()
        .map(|(path, directory)| ProbeTarget {
            path: path.clone(),
            directory: *directory,
        })
        .collect();
    let user = TestUser::create();
    let filtered = filtered_administrator_token();
    let mut problems = access_problems(
        user.token(),
        &format!("standard user {}", user.name),
        &targets,
        readable,
    );
    problems.extend(access_problems(
        &filtered,
        "restricted administrator",
        &targets,
        readable,
    ));
    assert!(
        problems.is_empty(),
        "every attempt is denied with ERROR_ACCESS_DENIED:\n{}",
        problems.join("\n")
    );
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

/// A start that ran and stopped cleanly wrote entry 1000 and entry 1001 and
/// no other: the migration adds no Application-log entry (the amendment's
/// *Effect*).
fn assert_ran_and_stopped(entries: &[Entry]) {
    entry(entries, 1000);
    entry(entries, 1001);
    assert!(
        entries.iter().all(|e| matches!(e.event_id, 1000 | 1001)),
        "no other entry: {entries:#?}"
    );
}

/// Item 6: a service started with no arguments on a console layout, with its
/// directories as the runbook left them, keeps the outcome of the Amendment
/// 2026-11-04's checks, code 13 at the data directories before `RUNNING`
/// (R17), and its reason adds that the data directory holds a console
/// installation's identity files. It never starts a migration: nothing
/// changes, neither a byte nor an owner nor a DACL nor a name under `<P>`,
/// and it neither enrolls nor sends a heartbeat.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_013_a_start_without_arguments_on_a_console_layout_is_code_13() {
    let host = ConsoleHost::enrolled().await;
    let directories = host.directory_states();
    let files = host.file_states();
    let sent = host.heartbeat.attempts().len();
    let service = TestService::create(&service_args(&host.config));

    host.refused(
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

/// Items 1, 2 and 6: refusals with the console installation in place, each
/// before item 2.4, so each file keeps its bytes, owner and DACL.
///
/// - Item 1: start arguments of the wrong form are code 2 before any change,
///   and the line repeats no argument's value. Those lines are decided from
///   the arguments alone, the same on a host with no console installation
///   (`parse_start_arguments`), so they do not say one is in place: item 2's
///   "each refusal" is of the route, which they do not reach.
/// - Item 2: a `--config` other than the fixed one, the same path with
///   forward slashes, is code 2 before anything under `<P>` is read.
/// - Item 2: while an elevated console agent runs, the start is code 10
///   before any change, and that agent runs on.
/// - Items 2.1 and 2.3: fingerprints other than those of `server-ca.pem` are
///   code 6, after item 2.1 closed `<P>` and the data root, which the line
///   names: effective access to them is now denied, creating an entry
///   included, and through a handle an ordinary user held from before the
///   start too, while the files were not restricted.
/// - Item 6: `agent.toml` is then what fails a start without arguments, code
///   2 at the configuration with the same added reason, and it changes
///   nothing.
/// - Item 2.2: an entry of the data root that is not the agent's is code 5,
///   named and neither read nor deleted, with the repair.
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
    let anchor = host.anchor.as_str();

    // Item 1.
    let sent = host.heartbeat.attempts().len();
    let cases: Vec<(&str, Vec<&str>)> = vec![
        ("the choice without a fingerprint", vec![REMOVE]),
        ("a fingerprint without the choice", vec![anchor]),
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
        let entries = host.refused(&service, arguments, 2, &["invalid migration arguments"]);
        assert_repeats_no_value(&entries, arguments);
        assert_all_unchanged(&directories, what);
        assert_all_unchanged(&files, what);
        assert_no_migration(paths, what);
    }
    assert_eq!(host.heartbeat.attempts().len(), sent, "no heartbeat");

    // Item 2: `--config` comes first, by the Amendment 2026-11-04's rule.
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

    // Item 2: the lock precedes every change.
    {
        let mut console = ConsoleAgent::start(&host.config);
        wait_console_heartbeat(&host.heartbeat, &mut console).await;
        host.refused(
            &service,
            &[REMOVE, &host.anchor],
            10,
            &["another CyberGuard agent instance is running on this host"],
        );
        wait_console_heartbeat(&host.heartbeat, &mut console).await;
    }
    assert_all_unchanged(&directories, "a console agent running");
    assert_all_unchanged(&files, "a console agent running");
    assert_no_migration(paths, "a console agent running");

    // Items 2.1 and 2.3. The directories' security changes, not their names.
    // The fixture's premise comes first: an ordinary user creates entries in
    // the console's data root, here through a handle held across the start.
    let user = TestUser::create();
    let held = open_to_add_entries(user.token(), &paths.data_root);
    assert_eq!(
        create_through(user.token(), &held, ENTRY_THROUGH_A_HANDLE),
        0,
        "an ordinary user creates an entry in the console's data root"
    );
    let sent = host.heartbeat.attempts().len();
    let wrong_anchor = format!("--trust-anchor-sha256=sha256:{}", "0".repeat(64));
    let entries = host.refused(
        &service,
        &[REMOVE, &wrong_anchor],
        6,
        &["tls: client configuration failed: migration:", IN_PLACE],
    );
    assert_in_place(&entries, paths, true, &[]);
    let restricted = after_in_place(&entries);
    assert!(
        !CONSOLE_FILES.iter().any(|name| restricted.contains(name)),
        "the line names what was restricted, and no file was: {restricted}"
    );
    assert_eq!(
        create_through(user.token(), &held, ENTRY_THROUGH_A_HANDLE),
        STATUS_ACCESS_DENIED,
        "the handle held from before the start creates no entry"
    );
    drop(held);
    for directory in &directories {
        assert_system_and_administrators(&directory.path, true);
        directory.assert_same_content("the wrong fingerprints");
    }
    assert_all_unchanged(&files, "the wrong fingerprints");
    assert_no_migration(paths, "the wrong fingerprints");
    assert_denied_to_others(
        &[
            (paths.product_root.clone(), true),
            (paths.data_root.clone(), true),
        ],
        &[],
    );

    // Item 6's other half: the directories now pass, `agent.toml` does not.
    let closed = host.directory_states();
    host.refused(
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
    assert_all_unchanged(&closed, "a start without arguments");
    assert_all_unchanged(&files, "a start without arguments");
    assert_no_migration(paths, "a start without arguments");

    // Item 2.2.
    let planted = host.console_file("planted.txt");
    std::fs::write(&planted, b"not the agent's").expect("plant an entry in the data root");
    let planted_state = State::read(&planted, false);
    let entries = host.refused(
        &service,
        &[REMOVE, &host.anchor],
        5,
        &["identity error", "planted.txt", "administrator", IN_PLACE],
    );
    assert_in_place(&entries, paths, true, &[]);
    planted_state.assert_unchanged("the planted entry");
    privileged_delete(&planted);
    assert_all_unchanged(&files, "the planted entry");
    assert_no_migration(paths, "the planted entry");
    assert_eq!(host.heartbeat.attempts().len(), sent, "no heartbeat");

    host.assert_a_console_agent_runs().await;
    assert_all_unchanged(&files, "the console agent's run");
    assert_eq!(
        host.enroll.enroll_received_count(),
        1,
        "the service does not enroll; the one enrollment is the console agent's"
    );
}

/// Items 2.1 to 2.3 and the Amendment 2026-11-07: what is checked before any
/// file is changed, on objects the fixture bends in turn and puts back. Each
/// refusal says the console installation is in place, and each leaves the
/// files with their bytes, owner and DACL.
///
/// - Item 2.1: `<P>` owned by another account is code 13 with item 2.2's
///   repair, and nothing changes, directories included. Later a data root
///   whose DACL lets another account change it is the same refusal.
/// - Item 2.3: an `agent.toml` without `server.heartbeat_url` is code 2, the
///   host that relied on SPEC-003's fallback adding it first.
/// - Item 2.2: `key.dat` owned by another account is code 5 with the repair.
/// - Item 2.3, as the Amendment 2026-11-07 says: a `cert.pem` whose first
///   block is good and whose later section is malformed is code 5.
///
/// Then a real elevated console agent starts from the installation.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_013_what_items_2_1_to_2_3_refuse_leaves_the_files_as_they_were() {
    let host = ConsoleHost::enrolled().await;
    let paths = host.paths();
    let user = TestUser::create();
    let other_account = user.sid();
    let service = TestService::create(&service_args(&host.config));
    let arguments = [REMOVE, host.anchor.as_str()];
    let sent = host.heartbeat.attempts().len();

    // Item 2.1, the owner.
    set_owner_of(&paths.product_root, &other_account);
    let directories = host.directory_states();
    let files = host.file_states();
    let product_root = text(&paths.product_root);
    let entries = host.refused(
        &service,
        &arguments,
        13,
        &[
            "the service log cannot be written",
            &product_root,
            "administrator",
            IN_PLACE,
        ],
    );
    assert_in_place(&entries, paths, false, &[]);
    assert_all_unchanged(&directories, "another owner of <P>");
    assert_all_unchanged(&files, "another owner of <P>");
    assert_no_migration(paths, "another owner of <P>");
    set_owner_of(&paths.product_root, "BA");

    // Item 2.3, code 2.
    let config = std::fs::read_to_string(&host.config).expect("read agent.toml");
    let without_url: String = config
        .lines()
        .filter(|line| !line.starts_with("heartbeat_url"))
        .map(|line| format!("{line}\n"))
        .collect();
    assert_ne!(without_url, config, "agent.toml has a heartbeat URL");
    std::fs::write(&host.config, without_url).expect("rewrite agent.toml");
    let files = host.file_states();
    let entries = host.refused(
        &service,
        &arguments,
        2,
        &[
            "invalid config",
            &text(&host.config),
            "heartbeat_url",
            IN_PLACE,
        ],
    );
    assert_in_place(&entries, paths, true, &[]);
    assert_all_unchanged(&files, "no heartbeat URL");
    assert_no_migration(paths, "no heartbeat URL");
    std::fs::write(&host.config, &config).expect("restore agent.toml");

    // Item 2.2, the owner of a file.
    let key = host.console_file("key.dat");
    set_owner_of(&key, &other_account);
    let files = host.file_states();
    let entries = host.refused(
        &service,
        &arguments,
        5,
        &["identity error", "key.dat", "administrator", IN_PLACE],
    );
    assert_in_place(&entries, paths, true, &[]);
    assert_all_unchanged(&files, "another owner of key.dat");
    assert_no_migration(paths, "another owner of key.dat");
    set_owner_of(&key, "BA");

    // Item 2.3 and the Amendment 2026-11-07.
    let cert = host.console_file("cert.pem");
    let sound = std::fs::read_to_string(&cert).expect("read cert.pem");
    std::fs::write(&cert, format!("{sound}\n{MALFORMED_PEM_SECTION}")).expect("rewrite cert.pem");
    let files = host.file_states();
    let entries = host.refused(
        &service,
        &arguments,
        5,
        &["identity error", "cert.pem", IN_PLACE],
    );
    assert_in_place(&entries, paths, true, &[]);
    assert_all_unchanged(&files, "a malformed later section");
    assert_no_migration(paths, "a malformed later section");
    std::fs::write(&cert, sound).expect("restore cert.pem");

    // Item 2.1, the DACL, now that the directories are closed.
    set_security(
        &paths.data_root,
        "O:SYD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;;WD;;;BU)",
    );
    let directories = host.directory_states();
    let files = host.file_states();
    host.refused(
        &service,
        &arguments,
        13,
        &[
            "the service log cannot be written",
            &text(&paths.data_root),
            "administrator",
            IN_PLACE,
        ],
    );
    assert_all_unchanged(&directories, "a DACL others can change");
    assert_all_unchanged(&files, "a DACL others can change");
    assert_no_migration(paths, "a DACL others can change");
    set_security(&paths.data_root, PRIVATE_DIRECTORY_SDDL);

    assert_eq!(host.heartbeat.attempts().len(), sent, "no heartbeat");
    host.assert_a_console_agent_runs().await;
    assert_eq!(
        host.enroll.enroll_received_count(),
        1,
        "the service does not enroll; the one enrollment is the console agent's"
    );
}

/// Item 4: a staging or legacy directory counts only when SYSTEM owns it and
/// its DACL passes `agent.toml`'s policy. This is R17's attack with `<P>` still
/// open: an ordinary user creates the name and, as its owner, gives it a DACL
/// that passes, so the owner alone tells it apart, with a record that parses
/// inside. A start without arguments goes on as if it were absent, the default
/// refusal and not code 5's migration in progress; with the arguments item
/// 2.2 refuses the name, code 5, named, with the repair. Neither reads,
/// changes or deletes it, and the console installation still runs.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_013_a_name_an_ordinary_user_planted_changes_no_outcome() {
    let host = ConsoleHost::enrolled().await;
    let paths = host.paths();
    let legacy = Legacy::read(paths);
    let user = TestUser::create();
    let owner = user.sid();
    let service = TestService::create(&service_args(&host.config));
    let given = host.anchor.trim_start_matches("--trust-anchor-sha256=");
    let record = json!({
        "choice": "remove",
        "fingerprints": [given],
        "agent_id": common::TEST_AGENT_ID,
        "key_fingerprint": legacy.key_fingerprint,
        "enrolling_account": null,
        "files": {},
        "reversal_pending": false,
        "copies_not_reached": [],
    })
    .to_string();
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
        host.refused(
            &service,
            &[],
            13,
            &[
                "the service log cannot be written",
                REFUSAL_HEAD,
                REFUSAL_TAIL,
            ],
        );
        assert_all_unchanged(&planted, "a planted name, no arguments");
        assert_all_unchanged(&directories, "a planted name, no arguments");
        assert_all_unchanged(&files, "a planted name, no arguments");
        remove(decoy);
    }

    for decoy in decoys {
        let name = decoy.display();
        eprintln!("case: {name}, with the arguments");
        let planted = plant(decoy);
        let files = host.file_states();
        let file_name = decoy
            .file_name()
            .expect("the name of the decoy")
            .to_string_lossy();
        let entries = host.refused(
            &service,
            &[REMOVE, &host.anchor],
            5,
            &["identity error", &file_name, "administrator", IN_PLACE],
        );
        assert_in_place(&entries, paths, true, &[]);
        assert_all_unchanged(&planted, "a planted name, with the arguments");
        assert_all_unchanged(&files, "a planted name, with the arguments");
        remove(decoy);
    }

    assert_eq!(host.heartbeat.attempts().len(), sent, "no heartbeat");
    assert_no_migration(paths, "a planted name");
    host.assert_a_console_agent_runs().await;
}

/// The preamble and item 1: an open that Windows makes wait on another
/// process, here on an opportunistic lock whose break is never acknowledged,
/// is cancelled after 30 seconds and fails as one the object's sharing
/// refuses, item 2.3's code 5, the file named, and the start keeps reporting
/// `START_PENDING` meanwhile: the checkpoint advances at least every 10
/// seconds (the second Amendment 2026-11-01, item 3), which is at least twice
/// in 30 seconds, so a service that advances it only between the migration's
/// steps fails here. The 30 seconds of the unelevated tests are a short
/// `Environment.wait`: this is the start's own.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_013_an_open_windows_makes_wait_is_cancelled_after_30_seconds() {
    let host = ConsoleHost::enrolled().await;
    let paths = host.paths();
    let files = host.file_states();
    let service = TestService::create(&service_args(&host.config));
    let baseline = application_baseline();

    let held = Oplock::take(&host.config);
    let started = Instant::now();
    let stopped =
        service.start_with_and_wait_for_stop(&[REMOVE, &host.anchor], Duration::from_secs(120));
    let elapsed = started.elapsed();
    let broken = held.broken();
    drop(held);

    assert!(
        broken,
        "the start's open of agent.toml waited on the oplock"
    );
    assert_stopped_with(&stopped.status, 5);
    assert_pending_progress(&stopped, "START_PENDING through a 30-second wait");
    assert!(
        stopped.checkpoint_advances >= 2,
        "the checkpoint advanced during the wait: {}",
        stopped.checkpoint_advances
    );
    assert!(
        (Duration::from_secs(30)..Duration::from_secs(45)).contains(&elapsed),
        "the wait is cancelled after 30 seconds and the start stops: {elapsed:?}"
    );
    let entries = wait_entries(baseline, &[1002]);
    assert_never_ran(&entries);
    assert_failure_entry(&entries, &["identity error", "agent.toml", IN_PLACE]);
    assert_in_place(&entries, paths, true, &[]);
    assert_discloses_nothing(
        &entries_text(&entries),
        "the Application log",
        &host.secrets,
    );
    assert_all_unchanged(&files, "an open made to wait");
    assert_no_migration(paths, "an open made to wait");
}

/// Items 2.4 to 2.6 and 4: a refusal past the restriction, and what follows
/// it. A file of the data root held open for reading passes items 2.3 and
/// 2.4, and the data root cannot then be renamed: "a failed first rename
/// stops the start (code 5), the console installation in place". The line
/// names what was restricted; each file keeps its bytes and is owned by
/// SYSTEM, protected, SYSTEM and Administrators; the staging directory holds
/// the record, with the enrolling account's SID read before item 2.4; nothing
/// was renamed. Item 2.4's "the enrolling account's unelevated processes, and
/// ordinary users, can no longer open them" holds as effective access, for
/// the files and the record too.
///
/// A real elevated console agent still starts from the installation and
/// delivers a heartbeat. A start without arguments is code 5, naming the
/// interrupted migration, and arguments other than the recorded ones, another
/// choice or other fingerprints, are code 2 that repeats neither, all three
/// changing nothing and saying what stands restricted: the state in which an
/// operator most needs it. A start with the recorded arguments
/// continues (item 4) and migrates, and the record keeps the enrolling
/// account, whom the restricted files no longer name.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_013_a_failed_first_rename_leaves_a_running_console_and_continues() {
    let host = ConsoleHost::enrolled().await;
    let paths = host.paths();
    let legacy = Legacy::read(paths);
    let enrolling = current_user_sid();
    let files = host.file_states();
    let sent = host.heartbeat.attempts().len();
    let service = TestService::create(&service_args(&host.config));

    let held = hold_for_reading(&host.console_file("cert.pem"));
    let entries = host.refused(
        &service,
        &[REMOVE, &host.anchor],
        5,
        &["identity error", IN_PLACE],
    );
    drop(held);
    assert_in_place(&entries, paths, true, &CONSOLE_FILES);

    for file in &files {
        file.assert_same_content("the failed first rename");
        assert_system_and_administrators(&file.path, false);
    }
    assert_system_and_administrators(&paths.product_root, true);
    assert_system_and_administrators(&paths.data_root, true);
    assert_eq!(
        privileged_listing(&paths.data_root),
        Some(CONSOLE_FILES.map(String::from).to_vec()),
        "the console installation is in place"
    );
    assert!(
        !privileged_exists(&paths.legacy),
        "the data root was not renamed"
    );
    let record = paths.staging.join("migration.json");
    host.assert_record(&record, &legacy, "remove", &enrolling);
    assert_eq!(host.heartbeat.attempts().len(), sent, "no heartbeat");
    let mut targets: Vec<(PathBuf, bool)> = files
        .iter()
        .map(|file| (file.path.clone(), false))
        .collect();
    targets.push((paths.staging.clone(), true));
    targets.push((record.clone(), false));
    assert_denied_to_others(&targets, &[]);

    host.assert_a_console_agent_runs().await;

    // Item 4: a staging directory beside a console-layout data root.
    let mut states = host.directory_states();
    states.extend(host.file_states());
    states.push(State::read(&paths.staging, true));
    states.push(State::read(&record, false));
    let sent = host.heartbeat.attempts().len();
    let entries = host.refused(
        &service,
        &[],
        5,
        &["identity error", "agent.migrating", IN_PLACE],
    );
    assert_in_place(&entries, paths, true, &CONSOLE_FILES);
    assert_all_unchanged(&states, "a start without arguments");
    let wrong_anchor = format!("--trust-anchor-sha256=sha256:{}", "0".repeat(64));
    for (what, arguments) in [
        ("another choice", [KEEP, host.anchor.as_str()]),
        ("other fingerprints", [REMOVE, wrong_anchor.as_str()]),
    ] {
        eprintln!("case: {what}");
        let entries = host.refused(
            &service,
            &arguments,
            2,
            &["invalid migration arguments", IN_PLACE],
        );
        assert_in_place(&entries, paths, true, &CONSOLE_FILES);
        assert_repeats_no_value(&entries, &arguments);
        assert_all_unchanged(&states, what);
    }
    assert_eq!(host.heartbeat.attempts().len(), sent, "no heartbeat");

    let online_before = online_count(&host.heartbeat);
    service
        .start_with(&[REMOVE, &host.anchor])
        .expect("start with the recorded arguments");
    host.wait_migrated(&service, &legacy, "remove", online_before)
        .await;
    stop_cleanly(&service);
    host.assert_heartbeats_carry_the_legacy_identity(sent);
    assert_eq!(
        host.enroll.enroll_received_count(),
        1,
        "the continued migration does not enroll"
    );
    host.assert_migrated(&legacy);
    host.assert_record(&paths.record, &legacy, "remove", &enrolling);
    host.assert_copies(&legacy, "remove");
}

/// Items 1, 2, 3, 5 and 7 and install_ac_013's Tests paragraph, with `choice`.
/// A service started with the arguments migrates during `START_PENDING`,
/// reaches `RUNNING` and sends heartbeats with the legacy `agent_id`, signed
/// by the legacy key over the legacy client certificate, without enrolling.
/// The identity is SYSTEM's alone at `identity\`, the configuration and
/// anchor have their final security, `migration.json` has `agent.toml`'s
/// owner and DACL and holds the enrolling account's SID, the copies are as
/// the choice says, and the logs carry no SID or token. The host is one where
/// FR-014's rewrite failed, the case the runbook documents: the token in
/// `agent.toml` and in its leftover temporary. It reaches neither the generated
/// configuration nor a log line, and the temporary is a copy like the others.
/// A restart without arguments runs, loads the same identity, does not enroll
/// and leaves the same state; with `keep` item 5 runs on it too, so a copy
/// someone widened meanwhile gets its security back. Effective access to the
/// migrated identity and the remaining copies is denied to a standard user and
/// the restricted administrator token.
async fn a_service_migrates_and_restarts(argument: &str, choice: &str) {
    let host = ConsoleHost::enrolled().await;
    let paths = host.paths();
    host.leave_a_token_behind();
    let legacy = Legacy::read(paths);
    let enrolling = current_user_sid();
    let service = TestService::create(&service_args(&host.config));

    let baseline = application_baseline();
    let delivered = host.heartbeat.attempts().len();
    let online_before = online_count(&host.heartbeat);
    service
        .start_with(&[argument, &host.anchor])
        .expect("start with the migration's arguments");
    host.wait_migrated(&service, &legacy, choice, online_before)
        .await;
    stop_cleanly(&service);

    host.assert_heartbeats_carry_the_legacy_identity(delivered);
    assert_eq!(
        host.enroll.enroll_received_count(),
        1,
        "the service does not enroll; the one enrollment is the console agent's"
    );
    host.assert_migrated(&legacy);
    host.assert_record(&paths.record, &legacy, choice, &enrolling);
    host.assert_copies(&legacy, choice);
    let entries = wait_entries(baseline, &[1000, 1001]);
    assert_ran_and_stopped(&entries);
    host.assert_logs_disclose_nothing(&entries);

    // Item 5, "each start ... always with `keep`".
    let widened = paths.legacy.join("agent.toml");
    if choice == "keep" {
        set_security(&widened, WIDENED_COPY_SDDL);
    }

    // A restart without arguments.
    let restart_baseline = application_baseline();
    let delivered = host.heartbeat.attempts().len();
    let online_before = online_count(&host.heartbeat);
    service.start().expect("restart without arguments");
    service.wait_for(SERVICE_RUNNING, Duration::from_secs(60));
    assert!(
        common::wait_until(Duration::from_secs(60), || online_count(&host.heartbeat)
            > online_before)
        .await,
        "the restart delivers heartbeats with the same identity"
    );
    if choice == "keep" {
        assert!(
            common::wait_until(Duration::from_secs(60), || {
                let security = parse_sddl(&owner_and_dacl(&widened));
                security.owner == "SY" && !security.aces.iter().any(|ace| ace.sid == "BU")
            })
            .await,
            "the restart restricted the widened copy"
        );
    }
    stop_cleanly(&service);
    host.assert_heartbeats_carry_the_legacy_identity(delivered);
    assert_eq!(
        host.enroll.enroll_received_count(),
        1,
        "the restart loads the identity and does not enroll"
    );
    host.assert_migrated(&legacy);
    host.assert_record(&paths.record, &legacy, choice, &enrolling);
    host.assert_copies(&legacy, choice);
    let entries = wait_entries(restart_baseline, &[1000, 1001]);
    assert_ran_and_stopped(&entries);
    host.assert_logs_disclose_nothing(&entries);

    host.assert_others_are_denied(&legacy, choice);
}

/// `a_service_migrates_and_restarts` with `remove`: no copy and no
/// `agent.legacy` are left (item 5).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_013_a_service_migrates_with_remove_and_restarts() {
    a_service_migrates_and_restarts(REMOVE, "remove").await;
}

/// `a_service_migrates_and_restarts` with `keep`: the copies stay in
/// `agent.legacy` with item 5's security, on each start.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_013_a_service_migrates_with_keep_and_restarts() {
    a_service_migrates_and_restarts(KEEP, "keep").await;
}

/// Item 5: a copy the service cannot reach, here one whose open an oplock
/// makes wait, does not stop the start, which comes after `RUNNING`: the
/// service reports `RUNNING` long before the 30 seconds pass and keeps
/// delivering heartbeats; once the wait is cancelled the copy is logged as a
/// warning and named in `migration.json`, keeps the security it had, and the
/// next start reaches it, and clears the name.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_013_a_copy_out_of_reach_does_not_stop_the_start() {
    let host = ConsoleHost::enrolled().await;
    let paths = host.paths();
    let legacy = Legacy::read(paths);
    let enrolling = current_user_sid();
    let service = TestService::create(&service_args(&host.config));
    let online_before = online_count(&host.heartbeat);
    service
        .start_with(&[KEEP, &host.anchor])
        .expect("start with the migration's arguments");
    host.wait_migrated(&service, &legacy, "keep", online_before)
        .await;
    stop_cleanly(&service);

    let copy = paths.legacy.join("agent.toml");
    set_security(&copy, WIDENED_COPY_SDDL);
    let widened = owner_and_dacl(&copy);
    let baseline = application_baseline();
    let held = Oplock::take(&copy);
    service.start().expect("start with the copy out of reach");
    service.wait_for(SERVICE_RUNNING, Duration::from_secs(20));
    let named = common::wait_until(Duration::from_secs(60), || {
        host.copies_not_reached()
            .is_some_and(|names| names.iter().any(|name| name == "agent.toml"))
    })
    .await;
    assert!(
        named,
        "migration.json names the copy once its wait is cancelled"
    );
    assert!(held.broken(), "the pass's open of the copy waited");
    let online_before = online_count(&host.heartbeat);
    assert!(
        common::wait_until(Duration::from_secs(30), || online_count(&host.heartbeat)
            > online_before)
        .await,
        "the start goes on, delivering heartbeats"
    );
    assert_eq!(
        service.status().dwCurrentState,
        SERVICE_RUNNING,
        "the copy did not stop the start"
    );
    assert!(
        host.log_text().lines().any(|line| {
            line.to_ascii_uppercase().contains("WARN") && line.contains("agent.toml")
        }),
        "the file log warns of the copy: {:.2000}",
        host.log_text()
    );
    stop_cleanly(&service);
    drop(held);
    assert_eq!(
        owner_and_dacl(&copy),
        widened,
        "the copy out of reach keeps the security it had"
    );
    let entries = wait_entries(baseline, &[1000, 1001]);
    assert_ran_and_stopped(&entries);
    host.assert_logs_disclose_nothing(&entries);

    service.start().expect("the next start");
    service.wait_for(SERVICE_RUNNING, Duration::from_secs(60));
    assert!(
        common::wait_until(Duration::from_secs(60), || host
            .copies_not_reached()
            .is_some_and(|names| names.is_empty()))
        .await,
        "the next start reaches the copy and clears its name"
    );
    stop_cleanly(&service);
    host.assert_copies(&legacy, "keep");
    host.assert_record(&paths.record, &legacy, "keep", &enrolling);
}

/// Item 1: migration arguments on a host that is neither a console layout
/// nor a migration in progress are code 2 before any change: an installed
/// host's data root, with its production security and an identity at
/// `identity\`, gets no staging or legacy directory, nothing is written, and
/// the service neither enrolls nor sends a heartbeat.
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

    let entries = refused_with_arguments(
        &service,
        &[REMOVE, &anchor],
        2,
        &["invalid migration arguments"],
    );

    assert_discloses_nothing(&entries_text(&entries), "the Application log", &[]);
    assert_repeats_no_value(&entries, &[REMOVE, &anchor]);
    assert_all_unchanged(&states, "migration arguments on an installed host");
    assert_no_migration(&root.paths, "migration arguments on an installed host");
    assert_eq!(enroll.enroll_received_count(), 0, "no enrollment");
    assert!(heartbeat.attempts().is_empty(), "no heartbeat");
}
