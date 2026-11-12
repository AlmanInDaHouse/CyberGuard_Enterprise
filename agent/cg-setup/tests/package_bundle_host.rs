//! The agent's setup on the host (SPEC-024 Amendment 2026-11-12 (second)
//! and its *Tests*, over the Amendment 2026-11-09, items 1 to 8, and the
//! Amendment 2026-11-12): the real `cyberguard-agent-<version>-x64-setup.exe`
//! against cg-agent's test enrollment and heartbeat servers. A fresh
//! installation is operational (0), registers one program, its own, and
//! keeps the token to the bootstrap file and the configuration; preflight's
//! refusals pass through with the reversal run; an upgrade, a downgrade
//! refused with 1638, and the restores of the installed setup after a failed
//! upgrade; a second build of the same version; a repair; an uninstallation
//! while the agent's final request never answers; an uninstallation and a
//! reinstallation with no new enrollment; the payloads it carries, the raw
//! executables, and a changed payload refused before preflight runs; and
//! cg-setup's system-directory loading. Only the test build of the MSI carries
//! a custom action.
//!
//! The test installs the product on the machine, so it builds only with the
//! test-only `host-tests` feature and runs only in `package-ci`, on a
//! GitHub-hosted runner discarded after the job (CLAUDE.md, *Local pre-commit
//! gate*, *Windows package*). It fails without acting outside such a runner,
//! when a product service, `%ProgramData%\CyberGuard`,
//! `%ProgramFiles%\CyberGuard`, a product of the MSI's upgrade family or a
//! setup of the bundle's exists; `Host` uninstalls each setup it ran, the
//! products of the family, the service and the data root on every exit path.
//! No test holds `cg-agent.exe` open from its own process: Windows
//! Installer's Restart Manager closes such a process.
//!
//! Its inputs come from the job, each built with the release version this
//! test is built with unless named otherwise: `CG_TEST_SETUP`, the setup;
//! `CG_TEST_SETUP_NEXT`, the setup at `NEXT_VERSION`; `CG_TEST_SETUP_FAILING`,
//! the setup at `NEXT_VERSION` whose MSI is the test build that fails inside
//! its transaction; `CG_TEST_SETUP_REBUILT`, a second build of the setup;
//! `CG_TEST_MSI` and `CG_TEST_MSI_FAILING`, the MSIs the first and third
//! carry; `CG_TEST_AGENT_EXE` and `CG_TEST_SETUP_EXE`, the raw executables the
//! first carries; and `CG_TEST_BURN_LOGS`, if set, where Burn's logs go. The
//! payloads Burn extracts and verifies are read where it runs them, in its
//! package cache.
#![cfg(windows)]

#[path = "../../cg-agent/tests/common/mod.rs"]
mod common;
mod fixtures;
mod package_host;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use package_host::*;
use windows_sys::Win32::Storage::FileSystem::READ_CONTROL;
use windows_sys::Win32::System::Services::{
    SERVICE_AUTO_START, SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS, SERVICE_RUNNING,
    SERVICE_STOPPED,
};

/// The setup's upgrade family (`Bundle.wxs`), as Windows lists it.
const BUNDLE_UPGRADE_CODE: &str = "{B3D1FE6C-5F20-4E58-9B7A-3A7C8E1D4F10}";

/// The version the job builds the higher setups at.
const NEXT_VERSION: &str = "1.2.4";

/// The failure actions configure applies (the Amendment 2026-11-09, item
/// 8): restart after 60, 120 and 300 seconds, the count reset after a day.
fn configured_failure_actions() -> FailureActions {
    FailureActions {
        reset_seconds: 86_400,
        restarts_ms: vec![60_000, 120_000, 300_000],
    }
}

/// `LOAD_LIBRARY_SEARCH_SYSTEM32`, cg-setup's `DependentLoadFlags` (point 10).
const LOAD_LIBRARY_SEARCH_SYSTEM32: u16 = 0x0800;

/// Burn's exit for a setup older than the installed one (point 4).
const ERROR_PRODUCT_VERSION: i32 = 1638;

/// Windows Installer's failure (point 3's injected failure).
const ERROR_INSTALL_FAILURE: i32 = 1603;

/// The test build of the MSI's one custom action (`Package.wxs`).
const INJECTED_FAILURE: &str = "CgInjectedFailure";

// --- Inputs and the servers ----------------------------------------------------------

struct Setups {
    current: PathBuf,
    next: PathBuf,
    failing: PathBuf,
    rebuilt: PathBuf,
}

fn setups() -> Setups {
    Setups {
        current: input("CG_TEST_SETUP"),
        next: input("CG_TEST_SETUP_NEXT"),
        failing: input("CG_TEST_SETUP_FAILING"),
        rebuilt: input("CG_TEST_SETUP_REBUILT"),
    }
}

/// cg-agent's test enrollment server and a heartbeat server that verifies
/// under the key the enrollment certified, with switches to answer every
/// heartbeat 401 or to hold each final `going_offline` request open; and an
/// operator's directory for the anchor and the bootstrap files, whose token
/// is a canary.
struct Servers {
    runtime: tokio::runtime::Runtime,
    pki: common::TlsTestPki,
    enroll: Option<common::TlsEnrollMock>,
    heartbeat: Option<common::TlsMockServer>,
    stall: Arc<AtomicBool>,
    reject: Arc<AtomicBool>,
    operator: tempfile::TempDir,
    canary: String,
}

impl Servers {
    fn start() -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .expect("a runtime");
        let pki = common::generate_test_pki(common::TEST_AGENT_ID);
        let stall = Arc::new(AtomicBool::new(false));
        let reject = Arc::new(AtomicBool::new(false));
        let (enroll, heartbeat) = runtime.block_on(async {
            let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
            let (stall, reject) = (stall.clone(), reject.clone());
            let responder: common::Responder = Arc::new(move |envelope| {
                if reject.load(Ordering::SeqCst) {
                    return Some(401);
                }
                if stall.load(Ordering::SeqCst)
                    && common::envelope_status(envelope) == "going_offline"
                {
                    return Some(common::STALL);
                }
                None
            });
            let heartbeat =
                common::TlsMockServer::start_for_enrolled(&pki, &enroll, 0, Some(responder)).await;
            (enroll, heartbeat)
        });
        Servers {
            runtime,
            pki,
            enroll: Some(enroll),
            heartbeat: Some(heartbeat),
            stall,
            reject,
            operator: tempfile::tempdir().expect("the operator's directory"),
            canary: format!("cgcanary{}", uuid::Uuid::now_v7().simple()),
        }
    }

    fn enroll(&self) -> &common::TlsEnrollMock {
        self.enroll.as_ref().expect("the enrollment server")
    }

    fn heartbeat(&self) -> &common::TlsMockServer {
        self.heartbeat.as_ref().expect("the heartbeat server")
    }

    fn enrollments(&self) -> usize {
        self.enroll().enroll_received_count()
    }

    fn online(&self) -> usize {
        self.heartbeat()
            .received()
            .iter()
            .filter(|envelope| common::envelope_status(envelope) == "online")
            .count()
    }

    fn fingerprint(&self) -> String {
        let der = pem::parse(&self.pki.trust_anchor_pem)
            .expect("the anchor is PEM")
            .into_contents();
        cg_agent::service::anchor::fingerprint(&der)
    }

    /// A bootstrap file only SYSTEM and Administrators can reach, with the
    /// canary as its token and `fingerprint` as its list (§Data contracts §3).
    fn bootstrap_with(&self, name: &str, fingerprint: &str) -> PathBuf {
        let anchor = self.operator.path().join("server-ca.pem");
        if !anchor.exists() {
            std::fs::write(&anchor, &self.pki.trust_anchor_pem).expect("the anchor's source");
        }
        let text = format!(
            "[server]\nurl = {}\nheartbeat_url = {}\ntrust_anchor_path = {}\n\n\
             [enrollment]\ntoken = {}\n\n[setup]\ntrust_anchor_sha256 = [{}]\n",
            fixtures::quoted(&self.enroll().base_url),
            fixtures::quoted(&self.heartbeat().base_url),
            fixtures::quoted(anchor.to_str().expect("a UTF-8 path")),
            fixtures::quoted(&self.canary),
            fixtures::quoted(fingerprint),
        );
        let path = self.operator.path().join(name);
        cg_agent::custody::create_new_file(
            &path,
            "O:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)",
            text.as_bytes(),
        )
        .expect("the bootstrap file");
        path
    }

    fn bootstrap(&self, name: &str) -> PathBuf {
        self.bootstrap_with(name, &self.fingerprint())
    }

    /// Wait, up to `timeout`, until more than `seen` online heartbeats came.
    fn wait_online_beyond(&self, seen: usize, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.online() > seen {
                return true;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        false
    }
}

impl Drop for Servers {
    fn drop(&mut self) {
        let (enroll, heartbeat) = (self.enroll.take(), self.heartbeat.take());
        self.runtime.block_on(async move {
            drop(enroll);
            drop(heartbeat);
        });
    }
}

// --- Burn's logs ---------------------------------------------------------------------

/// A run of the setup: its exit code, how long it took, and its logs, Burn's
/// own first, each by its file name.
struct Outcome {
    name: String,
    code: i32,
    took: Duration,
    logs: Vec<(String, String)>,
}

impl Outcome {
    /// Burn's own log, its lines without their process and time prefixes.
    fn burn(&self) -> Vec<String> {
        burn_lines(&self.logs.first().expect("Burn's log").1)
    }

    /// The log Burn names `<stem>_NNN_<tail>` for this run, a package's or a
    /// related bundle's; not one of the logs that related bundle names after
    /// its own.
    fn log_named(&self, tail: &str) -> Option<&str> {
        let prefix = format!("{}_", self.name);
        self.logs
            .iter()
            .skip(1)
            .find(|(file, _)| {
                file.strip_prefix(&prefix)
                    .and_then(|rest| rest.split_once('_'))
                    .is_some_and(|(number, rest)| {
                        number.len() == 3
                            && number.bytes().all(|b| b.is_ascii_digit())
                            && rest == tail
                    })
            })
            .map(|(_, text)| text.as_str())
    }

    /// Windows Installer's log of the MSI this run applied.
    fn msi_log(&self) -> &str {
        self.log_named("Agent.log")
            .unwrap_or_else(|| panic!("{}: the MSI's log\n{}", self.name, self.summary()))
    }

    /// The restore's log: the installed setup `code` run again by this run's
    /// rollback (point 3).
    fn restore(&self, code: &str) -> Vec<String> {
        burn_lines(
            self.log_named(&format!("{code}_rollback.log"))
                .unwrap_or_else(|| panic!("{}: a restore's log\n{}", self.name, self.summary())),
        )
    }

    /// What ran and how it ended, for a failure's message: Burn's lines of
    /// detection, plan, application and exit, which carry no secret (the
    /// bundle variables name files).
    fn summary(&self) -> String {
        format!(
            "{}: exit {} after {:.1} s\n{}",
            self.name,
            self.code,
            self.took.as_secs_f64(),
            chain(&self.burn()).join("\n")
        )
    }
}

/// A Burn log's lines without the `[pid:tid][time]` prefixes, so that each
/// starts with its message's identifier (`i301: ...`).
fn burn_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| {
            let mut rest = line;
            while let Some(after) = rest.strip_prefix('[').and_then(|r| r.split_once(']')) {
                rest = after.1;
            }
            rest.to_string()
        })
        .collect()
}

/// The lines that say what Burn detected, planned, ran, rolled back and
/// returned.
fn chain(lines: &[String]) -> Vec<String> {
    const KEPT: [&str; 13] = [
        "i102", "i103", "i201", "i207", "i301", "i318", "i319", "i366", "i399", "i007", "w224",
        "w350", "e000",
    ];
    lines
        .iter()
        .filter(|line| KEPT.iter().any(|id| line.starts_with(id)))
        .map(|line| line.chars().take(240).collect())
        .collect()
}

/// Whether Burn applied package `package` in its `kind` pass (`execute` or
/// `rollback`).
fn applied(lines: &[String], kind: &str, package: &str) -> bool {
    let start = format!("i301: Applying {kind} package: {package}, ");
    lines.iter().any(|line| line.starts_with(&start))
}

/// The results Burn recorded for package `package` in its `kind` pass.
fn results(lines: &[String], kind: &str, package: &str) -> Vec<String> {
    let start = format!("i319: Applied {kind} package: {package}, result: ");
    lines
        .iter()
        .filter_map(|line| line.strip_prefix(&start))
        .map(|rest| rest.split(',').next().unwrap_or_default().to_string())
        .collect()
}

/// The third amendment's point 1: the failed step's own line, "Applied
/// execute package: (step), result:", carries `code` as an HRESULT whose
/// last four hexadecimal digits are the code.
fn assert_failed_step(outcome: &Outcome, step: &str, code: u32) {
    assert_eq!(
        results(&outcome.burn(), "execute", step)
            .last()
            .map(String::as_str),
        Some(format!("0x{:x}", 0x8007_0000 | code).as_str()),
        "{step}'s failure on its own line\n{}",
        outcome.summary()
    );
}

/// The exit codes Burn read from package `package`'s process.
fn exits(lines: &[String], package: &str) -> Vec<u32> {
    let start = format!("i366: The process for package: {package} exited with code: 0x");
    lines
        .iter()
        .filter_map(|line| line.strip_prefix(&start))
        .map(|rest| {
            let hex = rest.split('.').next().unwrap_or_default();
            u32::from_str_radix(hex, 16).unwrap_or_else(|_| panic!("an exit code: {rest}"))
        })
        .collect()
}

/// Burn's plan line for package `package`.
fn plan(lines: &[String], package: &str) -> String {
    let start = format!("i201: Planned package: {package}, ");
    lines
        .iter()
        .find(|line| line.starts_with(&start))
        .unwrap_or_else(|| panic!("a plan line for {package}"))
        .clone()
}

// --- The host ------------------------------------------------------------------------

/// What a test runs, undone on drop: each setup it ran uninstalled, newest
/// first, then any product of the MSI's family, the service and the data
/// root.
struct Host {
    paths: Paths,
    logs: PathBuf,
    _temporary: Option<tempfile::TempDir>,
    test: &'static str,
    ran: Vec<PathBuf>,
}

impl Host {
    fn take(test: &'static str) -> Self {
        let paths = Paths::machine();
        refuse_unless_disposable_runner(&paths, "run the setup");
        assert!(
            setups_registered().is_empty(),
            "refusing to act: a setup of the bundle's family is registered"
        );
        let (logs, temporary) = match std::env::var_os("CG_TEST_BURN_LOGS") {
            Some(logs) => {
                let logs = PathBuf::from(logs);
                std::fs::create_dir_all(&logs).expect("the logs' directory");
                (logs, None)
            }
            None => {
                let temporary = tempfile::tempdir().expect("a directory for Burn's logs");
                (temporary.path().to_path_buf(), Some(temporary))
            }
        };
        Host {
            paths,
            logs,
            _temporary: temporary,
            test,
            ran: Vec::new(),
        }
    }

    /// `setup` with `arguments`, quiet, without a restart, and its logs under
    /// `<test>-<name>`: Burn names each package's and related bundle's log
    /// after its own.
    fn setup(&mut self, name: &str, setup: &Path, arguments: &[&str]) -> Outcome {
        if !self.ran.iter().any(|known| known == setup) {
            self.ran.push(setup.to_path_buf());
        }
        let stem = format!("{}-{name}", self.test);
        let log = self.logs.join(format!("{stem}.log"));
        let started = Instant::now();
        let code = Command::new(setup)
            .args(arguments)
            .args(["-quiet", "-norestart", "-log"])
            .arg(&log)
            .status()
            .expect("run the setup")
            .code()
            .expect("the setup's exit code");
        let took = started.elapsed();
        let mut logs = vec![(format!("{stem}.log"), read_log(&log))];
        let mut others: Vec<(String, String)> = std::fs::read_dir(&self.logs)
            .expect("the logs' directory")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|file| file.starts_with(&format!("{stem}_")))
            .map(|file| {
                let text = read_log(&self.logs.join(&file));
                (file, text)
            })
            .collect();
        others.sort();
        logs.extend(others);
        let outcome = Outcome {
            name: stem,
            code,
            took,
            logs,
        };
        eprintln!("{}", outcome.summary());
        // SPEC-024 Amendment 2026-11-12 (third), point 1: Burn's last line
        // carries the code setup.exe exits with, in hexadecimal.
        let exit_line = format!("i007: Exit code: 0x{:x}, restarting: ", code as u32);
        assert!(
            outcome
                .burn()
                .iter()
                .any(|line| line.starts_with(&exit_line)),
            "Burn's log names setup.exe's exit code\n{}",
            outcome.summary()
        );
        outcome
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        for setup in std::mem::take(&mut self.ran).into_iter().rev() {
            let code = Command::new(&setup)
                .args(["-uninstall", "-quiet", "-norestart"])
                .status()
                .map(|status| status.code().unwrap_or(-1))
                .unwrap_or(-1);
            eprintln!("cleanup: {} -uninstall: exit {code}", setup.display());
        }
        for product in msi_products() {
            let code = Command::new("msiexec.exe")
                .args(["/x", &product.key, "/qn", "/norestart"])
                .status()
                .map(|status| status.code().unwrap_or(-1))
                .unwrap_or(-1);
            eprintln!("cleanup: msiexec /x {}: exit {code}", product.key);
        }
        let _ = Command::new("sc.exe")
            .args(["stop", PRODUCT_SERVICE])
            .status();
        let _ = wait_for_state(SERVICE_STOPPED, Duration::from_secs(90));
        let _ = Command::new("sc.exe")
            .args(["delete", PRODUCT_SERVICE])
            .status();
        remove_product_root(&self.paths.product_root);
    }
}

/// A log Burn or Windows Installer wrote, UTF-16 when it starts with a BOM.
fn read_log(path: &Path) -> String {
    text_of(&std::fs::read(path).unwrap_or_default())
}

/// Take ownership of the data root and remove it, the identity included,
/// which only SYSTEM can reach otherwise.
fn remove_product_root(root: &Path) {
    if !exists(root) {
        return;
    }
    let path = root.to_str().expect("a UTF-8 path");
    let _ = Command::new("takeown.exe")
        .args(["/F", path, "/R", "/A", "/D", "Y"])
        .output();
    let _ = Command::new("icacls.exe")
        .args([path, "/reset", "/T", "/C", "/Q"])
        .output();
    if let Err(error) = std::fs::remove_dir_all(root) {
        eprintln!("remove {}: {error}", root.display());
    }
}

// --- The machine's state -------------------------------------------------------------

/// The setups of the bundle's family that Windows lists.
fn setups_registered() -> Vec<Program> {
    programs()
        .into_iter()
        .filter(|program| program.bundle_upgrade_code.as_deref() == Some(BUNDLE_UPGRADE_CODE))
        .collect()
}

/// The programs Windows lists for a user to see: hidden ones are not.
fn visible_programs() -> Vec<Program> {
    programs().into_iter().filter(|p| !p.hidden).collect()
}

/// The MSI's products: the programs named as the product that are no setup.
fn msi_products() -> Vec<Program> {
    programs()
        .into_iter()
        .filter(|program| program.bundle_upgrade_code.is_none())
        .collect()
}

/// The package cache's folders of the setup: the setups' own and their
/// packages', cg-setup's and the MSI's.
fn cached() -> Vec<String> {
    let cache = program_data().join("Package Cache");
    let Ok(entries) = std::fs::read_dir(&cache) else {
        return Vec::new();
    };
    let mut found: Vec<String> = entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.starts_with("cg-setup-")
                || std::fs::read_dir(entry.path())
                    .map(|files| {
                        files.flatten().any(|file| {
                            file.file_name()
                                .to_string_lossy()
                                .to_ascii_lowercase()
                                .starts_with("cyberguard-agent")
                        })
                    })
                    .unwrap_or(false)
        })
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    found
}

/// The package cache's folders of cg-setup's step `step`, each named for
/// its version and its build.
fn step_folders(step: &str) -> Vec<String> {
    cached()
        .into_iter()
        .filter(|name| name.starts_with(&format!("cg-setup-{step}-")))
        .collect()
}

fn program_data() -> PathBuf {
    cg_agent::custody::program_data().expect("%ProgramData%")
}

/// What the product service is: its state, process, start type, failure
/// actions and DACL.
#[derive(Debug, PartialEq, Eq)]
struct ServiceFacts {
    state: u32,
    pid: u32,
    start_type: u32,
    failure_actions: FailureActions,
    dacl: String,
}

fn service_facts() -> ServiceFacts {
    let service = open_service(
        PRODUCT_SERVICE,
        SERVICE_QUERY_CONFIG | SERVICE_QUERY_STATUS | READ_CONTROL,
    )
    .unwrap_or_else(|error| panic!("open {PRODUCT_SERVICE}: Win32 error {error}"));
    let (state, pid) = service_process(&service);
    ServiceFacts {
        state,
        pid,
        start_type: service_config(&service).start_type,
        failure_actions: failure_actions_of(&service),
        dacl: service_dacl(&service),
    }
}

/// The product service running, automatic, with configure's failure actions
/// and the MSI's DACL (the Amendment 2026-11-12, point 2).
fn assert_operational_service(context: &str) -> ServiceFacts {
    let facts = service_facts();
    assert_eq!(
        (
            facts.state,
            facts.start_type,
            &facts.failure_actions,
            facts.dacl.as_str()
        ),
        (
            SERVICE_RUNNING,
            SERVICE_AUTO_START,
            &configured_failure_actions(),
            SERVICE_DACL
        ),
        "{context}: the service runs, automatic, with its failure actions and DACL"
    );
    facts
}

/// Wait, up to `timeout`, for the product service to reach `state`, or to
/// be absent when `state` is `SERVICE_STOPPED`.
fn wait_for_state(state: u32, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        match open_service(PRODUCT_SERVICE, SERVICE_QUERY_STATUS) {
            Ok(service) if service_process(&service).0 == state => return true,
            Err(_) if state == SERVICE_STOPPED => return true,
            _ => {}
        }
        if Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// The installed binary's version string, from its version resource.
fn installed_version(paths: &Paths) -> Option<String> {
    cg_setup::version::read_installed(&paths.binary)
        .expect("the binary's version is readable")
        .and_then(|version| version.product_version)
}

/// The record preflight writes and configure deletes.
fn record(paths: &Paths) -> PathBuf {
    paths.product_root.join("agent").join("setup-attempt.json")
}

/// After a fresh installation: operational (0), one enrollment, an online
/// heartbeat, the service as configure leaves it, and one program listed,
/// the setup at this version, the MSI's entry hidden.
fn install_operational(host: &mut Host, servers: &Servers, setup: &Path) -> Outcome {
    let bootstrap = servers.bootstrap("bootstrap.toml");
    let outcome = host.setup(
        "install",
        setup,
        &[&format!("CG_BOOTSTRAP={}", bootstrap.display())],
    );
    assert_eq!(outcome.code, 0, "operational\n{}", outcome.summary());
    assert_eq!(servers.enrollments(), 1, "one enrollment");
    assert!(servers.online() >= 1, "an online heartbeat came");
    assert_operational_service("a fresh installation");
    outcome
}

// --- The token's canary --------------------------------------------------------------

/// Whether `bytes` holds `canary` as UTF-8 or UTF-16LE.
fn holds(bytes: &[u8], canary: &str) -> bool {
    let utf8 = canary.as_bytes();
    let utf16: Vec<u8> = canary.encode_utf16().flat_map(u16::to_le_bytes).collect();
    bytes.windows(utf8.len()).any(|w| w == utf8) || bytes.windows(utf16.len()).any(|w| w == utf16)
}

/// The files under `root` that hold the canary, and how many could not be
/// read.
fn scan_tree(root: &Path, canary: &str, hits: &mut Vec<String>, unread: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        unread.push(root.display().to_string());
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            unread.push(path.display().to_string());
            continue;
        };
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            scan_tree(&path, canary, hits, unread);
        } else {
            match std::fs::read(&path) {
                Ok(bytes) if holds(&bytes, canary) => hits.push(path.display().to_string()),
                Ok(_) => {}
                Err(_) => unread.push(path.display().to_string()),
            }
        }
    }
}

/// The places of the Amendment 2026-11-12 (second)'s *Tests* where the
/// canary is found, but the bootstrap file and the staged and adopted
/// configuration: Burn's and Windows Installer's logs (`logs`), the temporary
/// folders, the package cache, Windows Installer's cache and the data root;
/// the registry's installation, uninstallation, `RunOnce`, service,
/// environment and current-user keys, where Windows Installer keeps a
/// product's properties; and the latest entries of the Application, System
/// and PowerShell logs.
fn canary_hits(servers: &Servers, paths: &Paths, logs: &Path) -> Vec<String> {
    let canary = servers.canary.as_str();
    let mut hits = Vec::new();
    let mut unread = Vec::new();
    let windows = PathBuf::from(std::env::var_os("SystemRoot").expect("%SystemRoot%"));
    for root in [
        logs.to_path_buf(),
        std::env::temp_dir(),
        windows.join("Temp"),
        windows.join("SystemTemp"),
        program_data().join("Package Cache"),
        windows.join("Installer"),
        paths.product_root.clone(),
    ] {
        scan_tree(&root, canary, &mut hits, &mut unread);
    }
    // What could not be read, such as a file in use or the identity only
    // SYSTEM reaches, is listed for the record; it holds no hit.
    eprintln!(
        "the canary's scan: {} files or folders could not be read: {:?}",
        unread.len(),
        &unread[..unread.len().min(40)]
    );
    let operator = servers.operator.path().display().to_string();
    let config = paths.config.display().to_string();
    let staged = format!("{config}.setup");
    hits.retain(|hit| !hit.starts_with(&operator) && *hit != config && *hit != staged);
    for key in [
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce",
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Installer",
        r"HKLM\SOFTWARE\Classes\Installer",
        r"HKLM\SYSTEM\CurrentControlSet\Services\CyberGuardAgent",
        r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
        r"HKCU",
    ] {
        let output = Command::new("reg.exe")
            .args(["query", key, "/s", "/f", canary])
            .output()
            .expect("run reg.exe");
        if String::from_utf8_lossy(&output.stdout).contains(canary) {
            hits.push(format!("registry {key}"));
        }
    }
    for log in [
        "Application",
        "System",
        "Microsoft-Windows-PowerShell/Operational",
    ] {
        let output = Command::new("wevtutil.exe")
            .args(["qe", log, "/c:2000", "/rd:true", "/f:text"])
            .output()
            .expect("run wevtutil.exe");
        assert!(output.status.success(), "wevtutil reads {log}");
        if String::from_utf8_lossy(&output.stdout).contains(canary) {
            hits.push(format!("event log {log}"));
        }
    }
    hits
}

// --- The tests -----------------------------------------------------------------------

/// A fresh installation, operational, its registration and where the token
/// is not; a default uninstallation; a reinstallation over the preserved
/// identity with no new enrollment (point 1 and *Tests*).
#[test]
fn package_bundle_a_fresh_installation_keeps_the_token_and_survives_a_reinstallation() {
    let setups = setups();
    let mut host = Host::take("fresh");
    let servers = Servers::start();
    let version = release_version();

    // The registry sampled for the canary while the setup applies, where
    // Burn writes its resume command line and its RunOnce entry.
    let sampling = Arc::new(AtomicBool::new(true));
    let sampler = {
        let (sampling, canary) = (sampling.clone(), servers.canary.clone());
        std::thread::spawn(move || {
            let mut seen = Vec::new();
            while sampling.load(Ordering::SeqCst) {
                for key in [
                    r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
                    r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce",
                ] {
                    let output = Command::new("reg.exe")
                        .args(["query", key, "/s", "/f", &canary])
                        .output()
                        .expect("run reg.exe");
                    if String::from_utf8_lossy(&output.stdout).contains(&canary) {
                        seen.push(key.to_string());
                    }
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            seen
        })
    };
    let outcome = install_operational(&mut host, &servers, &setups.current);
    sampling.store(false, Ordering::SeqCst);
    let seen = sampler.join().expect("the sampler");
    assert!(
        seen.is_empty(),
        "the canary in the registry while applying: {seen:?}"
    );

    // The chain as built (point 1): preflight rolled back by the reversal,
    // the MSI with the chain's property, configure rolled back by nothing.
    let burn = outcome.burn();
    for (package, rollback) in [
        ("Preflight", "rollback: Uninstall"),
        ("Agent", "rollback: Uninstall"),
        ("Configure", "rollback: None"),
    ] {
        let line = plan(&burn, package);
        assert!(
            line.contains(&format!("execute: Install, {rollback},")),
            "{package}'s plan: {line}"
        );
    }
    assert!(
        burn.iter().any(
            |line| line.starts_with("i301: Applying execute package: Agent, ")
                && line.contains(r#"CG_SETUP_CHAIN="1""#)
        ),
        "the MSI is run with the chain's property\n{}",
        outcome.summary()
    );

    // One program listed, the setup's, at this version; the MSI's hidden.
    let setups_listed = setups_registered();
    assert_eq!(
        visible_programs(),
        setups_listed,
        "the setup is the one program listed"
    );
    assert_eq!(
        setups_listed
            .iter()
            .map(|p| p.version.as_str())
            .collect::<Vec<_>>(),
        [version],
        "one setup, this version"
    );
    let products = msi_products();
    assert!(
        products.len() == 1 && products[0].hidden && products[0].version == version,
        "one MSI product, hidden, this version: {products:?}"
    );
    // The bundle variables are not persisted: a repair or an uninstallation
    // from the registered setup passes none (point 1).
    let variables = format!(r"{UNINSTALL_KEY}\{}\variables", setups_listed[0].key);
    for name in ["CG_BOOTSTRAP", "CG_MIGRATE_LEGACY", "CG_TRUST_ANCHORS"] {
        assert_eq!(
            registry_value(&variables, name),
            None,
            "{name} is not persisted"
        );
    }
    assert!(!cached().is_empty(), "the setup is cached");
    assert_eq!(
        installed_version(&host.paths).as_deref(),
        Some(version),
        "the binary is this version"
    );

    // The payloads Burn extracted and verified, where it runs them: the raw
    // executables, digest for digest, and cg-setup loading its imports from
    // the system directory alone (point 10).
    let cache = program_data().join("Package Cache");
    let raw_setup = std::fs::read(input("CG_TEST_SETUP_EXE")).expect("the raw cg-setup.exe");
    for step in ["preflight", "configure"] {
        let folders = step_folders(step);
        assert_eq!(folders.len(), 1, "one cached {step}: {folders:?}");
        let carried = cache.join(&folders[0]).join(step).join("cg-setup.exe");
        assert_eq!(
            digest(
                &std::fs::read(&carried).unwrap_or_else(|e| panic!("{}: {e}", carried.display()))
            ),
            digest(&raw_setup),
            "{step}'s cg-setup.exe is the raw executable"
        );
        assert_eq!(
            fixtures::dependent_load_flags(&carried),
            Some(LOAD_LIBRARY_SEARCH_SYSTEM32),
            "{step}'s cg-setup.exe loads its imports from the system directory"
        );
    }
    let msi = input("CG_TEST_MSI");
    let msi_name = msi.file_name().expect("the MSI's name").to_owned();
    let cached_msi: Vec<PathBuf> = std::fs::read_dir(&cache)
        .expect("the package cache")
        .flatten()
        .map(|entry| entry.path().join(&msi_name))
        .filter(|path| path.is_file())
        .collect();
    assert_eq!(cached_msi.len(), 1, "one cached MSI: {cached_msi:?}");
    assert_eq!(
        digest(&std::fs::read(&cached_msi[0]).expect("the cached MSI")),
        digest(&std::fs::read(&msi).expect("the MSI")),
        "the MSI is the one built"
    );
    assert_eq!(
        digest(&std::fs::read(&host.paths.binary).expect("the binary")),
        digest(&std::fs::read(input("CG_TEST_AGENT_EXE")).expect("the raw executable")),
        "the installed binary is the raw agent executable"
    );
    assert!(
        !exists(&record(&host.paths)),
        "configure deleted the record"
    );

    // The token: in the bootstrap file and the configuration alone.
    let enrolled = servers
        .enroll()
        .enroll_received()
        .iter()
        .any(|request| request.to_string().contains(&servers.canary));
    assert!(
        enrolled,
        "the enrollment carried the token: the canary is the token"
    );
    let hits = canary_hits(&servers, &host.paths, &host.logs);
    assert!(
        hits.is_empty(),
        "the token found after the installation: {hits:?}"
    );

    // A default uninstallation: the program, the MSI, the service, the
    // binary and the cache removed; the data root kept (§Operational §4).
    let outcome = host.setup("uninstall", &setups.current, &["-uninstall"]);
    assert_eq!(outcome.code, 0, "{}", outcome.summary());
    assert!(
        !applied(&outcome.burn(), "execute", "Preflight")
            && !applied(&outcome.burn(), "execute", "Configure"),
        "on uninstall neither step runs (item 1)\n{}",
        outcome.summary()
    );
    assert!(programs().is_empty(), "no program listed: {:?}", programs());
    assert!(
        open_service(PRODUCT_SERVICE, SERVICE_QUERY_STATUS).is_err(),
        "the service is removed"
    );
    assert!(
        !exists(&host.paths.program_files_root),
        "the binary is removed"
    );
    assert!(cached().is_empty(), "the cache is removed: {:?}", cached());
    assert!(exists(&host.paths.product_root), "the data root is kept");
    let hits = canary_hits(&servers, &host.paths, &host.logs);
    assert!(
        hits.is_empty(),
        "the token found after the uninstallation: {hits:?}"
    );

    // A reinstallation over the preserved identity, with no input: no new
    // enrollment, operational again.
    let online = servers.online();
    let outcome = host.setup("reinstall", &setups.current, &[]);
    assert_eq!(outcome.code, 0, "{}", outcome.summary());
    assert_eq!(servers.enrollments(), 1, "no new enrollment");
    assert!(
        servers.wait_online_beyond(online, Duration::from_secs(5)),
        "the reinstalled agent heartbeats"
    );
    assert_operational_service("the reinstallation");
}

/// Preflight's refusals pass through Burn with the reversal run, and leave
/// nothing registered, cached, installed or created (points 1 and 2): no
/// input (22), an anchor the fingerprints do not match (25), and a quote in
/// a bundle variable that adds an option (40). Burn refuses to set its own
/// variables from the command line.
#[test]
fn package_bundle_refusals_pass_through_with_the_reversal() {
    let setups = setups();
    let mut host = Host::take("refusals");
    let servers = Servers::start();
    let wrong = servers.bootstrap_with("wrong.toml", &format!("sha256:{}", "0".repeat(64)));
    let bootstrap = servers.bootstrap("quoted.toml");
    let cases: [(&str, String, i32); 4] = [
        ("no-input", String::new(), 22),
        (
            "wrong-anchor",
            format!("CG_BOOTSTRAP={}", wrong.display()),
            25,
        ),
        (
            "quote",
            format!(
                "CG_BOOTSTRAP={}\" --migrate-legacy=\"remove",
                bootstrap.display()
            ),
            40,
        ),
        ("ui-level", "WixBundleUILevel=4".to_string(), 22),
    ];
    for (name, argument, code) in cases {
        let arguments: Vec<&str> = [argument.as_str()]
            .into_iter()
            .filter(|a| !a.is_empty())
            .collect();
        let outcome = host.setup(name, &setups.current, &arguments);
        let burn = outcome.burn();
        assert_eq!(outcome.code, code, "{name}\n{}", outcome.summary());
        assert_failed_step(&outcome, "Preflight", code as u32);
        assert_eq!(
            exits(&burn, "Preflight"),
            [code as u32, 0],
            "{name}: preflight, then its reversal\n{}",
            outcome.summary()
        );
        assert!(
            burn.iter().any(|line| line
                .starts_with("i301: Applying rollback package: Preflight, action: Uninstall, ")
                && line.ends_with(r#"cg-setup.exe" reverse'"#)),
            "{name}: Burn ran the reversal\n{}",
            outcome.summary()
        );
        assert_eq!(
            results(&burn, "rollback", "Preflight"),
            ["0x0"],
            "{name}: the reversal succeeded"
        );
        assert!(
            !applied(&burn, "execute", "Agent"),
            "{name}: the MSI did not run"
        );
        assert!(
            programs().is_empty(),
            "{name}: nothing registered: {:?}",
            programs()
        );
        assert!(
            cached().is_empty(),
            "{name}: nothing cached: {:?}",
            cached()
        );
        assert!(
            open_service(PRODUCT_SERVICE, SERVICE_QUERY_STATUS).is_err(),
            "{name}: no service"
        );
        assert!(!exists(&host.paths.product_root), "{name}: no data root");
        if name == "ui-level" {
            assert!(
                burn.iter().any(|line| {
                    line
                    == "e000: Ignoring attempt to set non-overridable variable: 'WixBundleUILevel'."
                }),
                "Burn ignores its own variable on the command line\n{}",
                outcome.summary()
            );
        }
    }
    assert_eq!(servers.enrollments(), 0, "no enrollment");
}

/// An upgrade with no new enrollment, then a downgrade Burn refuses with
/// 1638 before any package runs (points 1 and 4).
#[test]
fn package_bundle_an_upgrade_and_a_refused_downgrade() {
    let setups = setups();
    let mut host = Host::take("upgrade");
    let servers = Servers::start();
    install_operational(&mut host, &servers, &setups.current);
    let installed = setups_registered();

    let outcome = host.setup("upgrade", &setups.next, &[]);
    let burn = outcome.burn();
    assert_eq!(outcome.code, 0, "{}", outcome.summary());
    assert_eq!(
        results(&burn, "execute", &installed[0].key),
        ["0x0"],
        "the installed setup was removed by the upgrade\n{}",
        outcome.summary()
    );
    assert_eq!(servers.enrollments(), 1, "no new enrollment");
    assert_eq!(
        installed_version(&host.paths).as_deref(),
        Some(NEXT_VERSION)
    );
    let listed = setups_registered();
    assert!(
        listed.len() == 1 && listed[0].version == NEXT_VERSION && listed[0].key != installed[0].key,
        "the higher setup alone: {listed:?}"
    );
    assert_eq!(
        msi_products()
            .iter()
            .map(|p| p.version.as_str())
            .collect::<Vec<_>>(),
        [NEXT_VERSION]
    );
    let running = assert_operational_service("the upgrade");

    let outcome = host.setup("downgrade", &setups.current, &[]);
    let burn = outcome.burn();
    assert_eq!(outcome.code, ERROR_PRODUCT_VERSION, "{}", outcome.summary());
    assert!(
        burn.iter().any(|line| line
            .starts_with("w224: Plan skipped due to related bundle of plan type Downgrade")),
        "Burn refused the downgrade itself\n{}",
        outcome.summary()
    );
    assert!(
        !burn.iter().any(|line| line.starts_with("i301: ")),
        "no package ran\n{}",
        outcome.summary()
    );
    assert_eq!(
        installed_version(&host.paths).as_deref(),
        Some(NEXT_VERSION)
    );
    assert_eq!(setups_registered(), listed, "the higher setup stays alone");
    assert_eq!(
        assert_operational_service("the refused downgrade").pid,
        running.pid,
        "the agent was not restarted"
    );
}

/// The restores of point 3: an upgrade whose MSI fails inside its
/// transaction (1603) and an upgrade preflight refuses (30) each end with the
/// installed setup run again, its agent operational with its DACL and
/// failure actions; an upgrade whose configure fails (44), after the
/// rollback boundary, runs no reversal, and the installed setup's own
/// preflight refuses the downgrade (29), the higher agent running and both
/// setups registered.
#[test]
fn package_bundle_failed_upgrades_restore_the_installed_setup() {
    let setups = setups();
    let mut host = Host::take("restore");
    let servers = Servers::start();
    let version = release_version();
    install_operational(&mut host, &servers, &setups.current);
    let installed = setups_registered();
    let old = installed[0].key.clone();

    // The machine's paths, read apart from the host the closure would borrow.
    let paths = Paths::machine();
    let assert_restored = |outcome: &Outcome, context: &str| {
        let burn = outcome.burn();
        assert!(
            applied(&burn, "rollback", "Preflight"),
            "{context}: the reversal ran\n{}",
            outcome.summary()
        );
        assert_eq!(
            results(&burn, "rollback", &old),
            ["0x0"],
            "{context}: Burn ran the installed setup again\n{}",
            outcome.summary()
        );
        let restore = outcome.restore(&old);
        assert_eq!(
            exits(&restore, "Preflight"),
            [0],
            "{context}: the restore's preflight"
        );
        assert_eq!(
            exits(&restore, "Configure"),
            [0],
            "{context}: the restore's configure"
        );
        assert_eq!(
            installed_version(&paths).as_deref(),
            Some(version),
            "{context}"
        );
        assert_eq!(
            setups_registered(),
            installed,
            "{context}: the installed setup alone"
        );
        assert_eq!(
            msi_products()
                .iter()
                .map(|p| p.version.as_str())
                .collect::<Vec<_>>(),
            [version],
            "{context}: the installed MSI"
        );
        assert_operational_service(context);
        assert!(!exists(&record(&paths)), "{context}: no record left");
    };

    let outcome = host.setup("injected-failure", &setups.failing, &[]);
    assert_eq!(outcome.code, ERROR_INSTALL_FAILURE, "{}", outcome.summary());
    assert_failed_step(&outcome, "Agent", ERROR_INSTALL_FAILURE as u32);
    assert!(
        outcome.msi_log().contains(INJECTED_FAILURE),
        "the MSI failed in its injected action"
    );
    assert_restored(&outcome, "the MSI's failure");

    let bootstrap = servers.bootstrap("again.toml");
    let outcome = host.setup(
        "refused",
        &setups.next,
        &[&format!("CG_BOOTSTRAP={}", bootstrap.display())],
    );
    assert_eq!(outcome.code, 30, "{}", outcome.summary());
    assert_failed_step(&outcome, "Preflight", 30);
    assert_eq!(exits(&outcome.burn(), "Preflight"), [30, 0]);
    assert!(
        !applied(&outcome.burn(), "execute", "Agent"),
        "the MSI did not run"
    );
    assert_restored(&outcome, "preflight's refusal");
    assert_eq!(servers.enrollments(), 1, "no new enrollment");

    servers.reject.store(true, Ordering::SeqCst);
    let outcome = host.setup("configure-fails", &setups.next, &[]);
    let burn = outcome.burn();
    assert_eq!(outcome.code, 44, "{}", outcome.summary());
    assert_failed_step(&outcome, "Configure", 44);
    assert!(
        !applied(&burn, "rollback", "Preflight") && !applied(&burn, "rollback", "Configure"),
        "no reversal after the rollback boundary\n{}",
        outcome.summary()
    );
    assert!(
        applied(&burn, "rollback", &old),
        "the installed setup ran again"
    );
    let restore = outcome.restore(&old);
    assert_eq!(
        exits(&restore, "Preflight"),
        [29, 0],
        "the installed setup's preflight refuses the downgrade, then reverses"
    );
    assert!(!applied(&restore, "execute", "Configure"));
    assert!(
        !exists(&record(&paths)),
        "the installed setup's preflight refused before its record"
    );
    assert_eq!(
        installed_version(&host.paths).as_deref(),
        Some(NEXT_VERSION)
    );
    let mut versions: Vec<String> = setups_registered().into_iter().map(|p| p.version).collect();
    versions.sort();
    assert_eq!(
        versions,
        [version, NEXT_VERSION],
        "both setups stay registered"
    );
    assert_eq!(
        service_facts().state,
        SERVICE_RUNNING,
        "the higher agent runs"
    );
    servers.reject.store(false, Ordering::SeqCst);
}

/// Point 9 (#157): a second build of the installed version replaces the
/// first, the later MSI removing the earlier inside its own transaction; one
/// setup and one MSI left, the agent running, no new enrollment.
#[test]
fn package_bundle_a_second_build_of_the_same_version_replaces_the_first() {
    let setups = setups();
    let mut host = Host::take("rebuilt");
    let servers = Servers::start();
    let version = release_version();
    install_operational(&mut host, &servers, &setups.current);
    let (first_setup, first_msi) = (setups_registered(), msi_products());

    let outcome = host.setup("rebuilt", &setups.rebuilt, &[]);
    assert_eq!(outcome.code, 0, "{}", outcome.summary());
    let msi_log = outcome.msi_log();
    let detected = format!(
        "PROPERTY CHANGE: Adding WIX_UPGRADE_DETECTED property. Its value is '{}'.",
        first_msi[0].key
    );
    assert!(
        msi_log.contains(&detected),
        "the later MSI found the earlier one of its version, to remove it in its own transaction"
    );
    let (setup_now, msi_now) = (setups_registered(), msi_products());
    assert!(
        setup_now.len() == 1
            && setup_now[0].key != first_setup[0].key
            && setup_now[0].version == version,
        "one setup, the later build: {setup_now:?}"
    );
    assert!(
        msi_now.len() == 1 && msi_now[0].key != first_msi[0].key && msi_now[0].version == version,
        "one MSI, the later build: {msi_now:?}"
    );
    let earlier = burn_lines(
        outcome
            .log_named(&format!("{}.log", first_setup[0].key))
            .expect("the earlier setup's uninstallation log"),
    );
    let detected: Vec<&String> = earlier
        .iter()
        .filter(|line| line.starts_with("i101: Detected package: Agent, state: "))
        .collect();
    assert!(
        !detected.is_empty()
            && detected
                .iter()
                .all(|line| !line.starts_with("i101: Detected package: Agent, state: Present,")),
        "the earlier setup found its MSI already removed\n{}",
        chain(&earlier).join("\n")
    );
    for step in ["preflight", "configure"] {
        assert_eq!(
            step_folders(step).len(),
            1,
            "the later build's {step} stays cached: {:?}",
            cached()
        );
    }
    assert_eq!(servers.enrollments(), 1, "no new enrollment");
    assert_operational_service("the second build");

    let outcome = host.setup("rebuilt-repair", &setups.rebuilt, &["-repair"]);
    assert_eq!(outcome.code, 0, "{}", outcome.summary());
    assert_operational_service("the second build's repair");
}

/// A repair runs the same build again: a changed binary, start type and
/// failure actions restored, with no new enrollment (point 9).
#[test]
fn package_bundle_a_repair_restores_the_binary_and_the_service() {
    let setups = setups();
    let mut host = Host::take("repair");
    let servers = Servers::start();
    install_operational(&mut host, &servers, &setups.current);
    let raw = std::fs::read(input("CG_TEST_AGENT_EXE")).expect("the raw executable");

    let stopped = Command::new("sc.exe")
        .args(["stop", PRODUCT_SERVICE])
        .status()
        .expect("sc stop");
    assert!(stopped.success(), "sc stop");
    assert!(
        wait_for_state(SERVICE_STOPPED, Duration::from_secs(90)),
        "the agent stops"
    );
    let mut changed = raw.clone();
    changed.extend_from_slice(b"changed");
    // The stopped agent's process may still be closing its image.
    let deadline = Instant::now() + Duration::from_secs(30);
    while let Err(error) = std::fs::write(&host.paths.binary, &changed) {
        assert!(Instant::now() < deadline, "change the binary: {error}");
        std::thread::sleep(Duration::from_millis(500));
    }
    for arguments in [
        &["config", PRODUCT_SERVICE, "start=", "demand"][..],
        &["failure", PRODUCT_SERVICE, "reset=", "0", "actions=", ""],
    ] {
        let status = Command::new("sc.exe")
            .args(arguments)
            .status()
            .expect("run sc.exe");
        assert!(status.success(), "sc {arguments:?}");
    }
    assert_ne!(
        service_facts().failure_actions,
        configured_failure_actions(),
        "the failure actions changed"
    );

    let outcome = host.setup("repair", &setups.current, &["-repair"]);
    assert_eq!(outcome.code, 0, "{}", outcome.summary());
    for step in ["Preflight", "Configure"] {
        assert_eq!(
            exits(&outcome.burn(), step),
            [0],
            "{step} runs on a repair (item 1)"
        );
    }
    assert_eq!(
        std::fs::read(&host.paths.binary).expect("the binary"),
        raw,
        "the binary is restored"
    );
    assert_operational_service("the repair");
    assert_eq!(servers.enrollments(), 1, "no new enrollment");
}

/// Point 7 (#156): an uninstallation while the agent's final request never
/// answers, under a request timeout of 55 seconds, which Windows Installer
/// waits out past its own 30 seconds, then removes the service and the
/// binary.
#[test]
fn package_bundle_an_uninstallation_waits_out_a_final_request_that_never_answers() {
    let setups = setups();
    let mut host = Host::take("stalled");
    let servers = Servers::start();
    install_operational(&mut host, &servers, &setups.current);

    // A configuration an administrator edited, as a migrated one can be.
    let text =
        std::fs::read_to_string(&host.paths.config).expect("agent.toml, as an administrator");
    let mut table: toml::Table = text.parse().expect("agent.toml is TOML");
    table
        .entry("heartbeat")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()))
        .as_table_mut()
        .expect("a table")
        .insert("request_timeout_seconds".into(), toml::Value::Integer(55));
    std::fs::write(&host.paths.config, toml::to_string(&table).expect("TOML"))
        .expect("edit agent.toml");
    let status = Command::new("sc.exe")
        .args(["stop", PRODUCT_SERVICE])
        .status()
        .expect("sc stop");
    assert!(status.success() && wait_for_state(SERVICE_STOPPED, Duration::from_secs(90)));
    let online = servers.online();
    let status = Command::new("sc.exe")
        .args(["start", PRODUCT_SERVICE])
        .status()
        .expect("sc start");
    assert!(status.success(), "sc start");
    assert!(
        servers.wait_online_beyond(online, Duration::from_secs(90)),
        "the agent heartbeats under the edited configuration"
    );

    servers.stall.store(true, Ordering::SeqCst);
    let outcome = host.setup("uninstall", &setups.current, &["-uninstall"]);
    assert_eq!(outcome.code, 0, "{}", outcome.summary());
    assert!(
        outcome.took > Duration::from_secs(40) && outcome.took < Duration::from_secs(120),
        "Windows Installer waited past its 30 seconds for the agent's bounded stop: {:.1} s",
        outcome.took.as_secs_f64()
    );
    assert!(programs().is_empty(), "no program listed: {:?}", programs());
    assert!(
        open_service(PRODUCT_SERVICE, SERVICE_QUERY_STATUS).is_err(),
        "the service is removed"
    );
    assert!(
        !exists(&host.paths.program_files_root),
        "the binary is removed"
    );
    let (renames, _) = cg_setup::version::pending_renames();
    assert!(
        !renames
            .iter()
            .any(|name| name.to_ascii_lowercase().contains("cyberguard")),
        "no rename waits for a restart: {renames:?}"
    );
}

/// A setup whose carried payload was changed is refused before preflight
/// runs: Burn verifies each package against the digest it was built with
/// (point 1), and nothing is registered, cached, installed or created.
#[test]
fn package_bundle_a_changed_payload_is_refused_before_preflight() {
    let setups = setups();
    let mut host = Host::take("changed");
    let servers = Servers::start();
    let copy = tempfile::tempdir().expect("a directory for the copy");
    let changed = copy
        .path()
        .join(setups.current.file_name().expect("the setup's name"));
    let mut bytes = std::fs::read(&setups.current).expect("the setup");
    let (start, length) = payload_container(&bytes);
    for quarter in 1..=3 {
        bytes[start + length * quarter / 4] ^= 0xFF;
    }
    std::fs::write(&changed, &bytes).expect("the changed setup");

    let bootstrap = servers.bootstrap("bootstrap.toml");
    let outcome = host.setup(
        "install",
        &changed,
        &[&format!("CG_BOOTSTRAP={}", bootstrap.display())],
    );
    assert_ne!(outcome.code, 0, "{}", outcome.summary());
    let burn = outcome.burn();
    assert!(
        burn.iter()
            .any(|line| line.starts_with("e310: Failed to verify payload")
                || line.starts_with("e312: Failed to extract payloads from container")),
        "Burn refused the changed payload\n{}",
        burn.iter()
            .filter(|line| line.starts_with('e') || line.starts_with('w'))
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        !burn.iter().any(|line| line.starts_with("i301: ")),
        "no package ran\n{}",
        outcome.summary()
    );
    assert!(
        open_service(PRODUCT_SERVICE, SERVICE_QUERY_STATUS).is_err(),
        "no service"
    );
    assert!(!exists(&host.paths.product_root), "no data root");
    assert!(!exists(&host.paths.program_files_root), "no binary");
    assert_eq!(servers.enrollments(), 0, "no enrollment");
}

/// Only the test build of the MSI carries a custom action: the release build
/// the setup carries has none (the Amendment 2026-11-12 (second), *Tests*).
/// Reading the packages' databases acts on nothing.
#[test]
fn package_bundle_only_the_test_msi_carries_a_custom_action() {
    assert_eq!(
        custom_actions(&input("CG_TEST_MSI")),
        Vec::<String>::new(),
        "the release MSI has no custom action"
    );
    assert_eq!(
        custom_actions(&input("CG_TEST_MSI_FAILING")),
        [INJECTED_FAILURE],
        "the test MSI has its one"
    );
}

/// The byte range of the setup's last attached container, its payloads',
/// from its `.wixburn` section (Burn's section header at `v7.0.1`:
/// magic, version, bundle code, stub size, original checksum, signature
/// offset and size, format, container count, then each container's size).
/// The setup is unsigned, so its engine ends where the bootstrapper
/// application's container does, and the payloads' container ends the file.
fn payload_container(bytes: &[u8]) -> (usize, usize) {
    let u16_at = |at: usize| usize::from(u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()));
    let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    let pe = u32_at(0x3C);
    assert_eq!(&bytes[pe..pe + 4], b"PE\0\0", "the PE signature");
    let sections = pe + 24 + u16_at(pe + 20);
    let section = (0..u16_at(pe + 6))
        .map(|i| sections + i * 40)
        .find(|section| &bytes[*section..*section + 8] == b".wixburn")
        .expect("a .wixburn section");
    let header = u32_at(section + 20);
    assert_eq!(u32_at(header), 0x00F1_4300, "Burn's section magic");
    assert_eq!(u32_at(header + 4), 2, "Burn's section version");
    let (stub, signature) = (u32_at(header + 24), u32_at(header + 32));
    assert_eq!(signature, 0, "an unsigned setup");
    assert_eq!(
        u32_at(header + 44),
        2,
        "the application's container and the payloads'"
    );
    let (application, payloads) = (u32_at(header + 48), u32_at(header + 52));
    let start = stub + application;
    assert_eq!(
        start + payloads,
        bytes.len(),
        "the payloads' container ends the file"
    );
    (start, payloads)
}

/// The SHA-256 of `bytes`, as the agent computes an anchor's fingerprint,
/// so that a failure prints digests rather than files.
fn digest(bytes: &[u8]) -> String {
    cg_agent::service::anchor::fingerprint(bytes)
}
