//! PROBE, never lands: what WiX v7.0.1's Burn does with the purge step on a
//! hosted runner (C9, SPEC-024 install_ac_012, before its contract is
//! written), as probe pull request #123 read the setup bundle's behaviour
//! for C7. The bundle carries a probe-only `Purge` ExePackage after the
//! rollback boundary, detected present and not permanent, whose uninstall
//! runs `cg-setup.exe purge --confirm="[CG_PURGE]"`; that verb is a stub
//! (`src/probe_purge.rs`) that appends one JSON line to
//! `C:\cg-probe\purge-readings.jsonl` and exits with the code in
//! `C:\cg-probe\purge-exit`, or 0.
//!
//! Each test is one scenario, R1 to R8, run in name order with
//! `--test-threads=1 --nocapture`. It asserts nothing about Burn: it prints
//! each reading as `READING <scenario> <label>: <line>` and appends it to
//! `<CG_TEST_BURN_LOGS>\purge-readings.txt`, and fails only when its own
//! machinery breaks, an installation that is not operational or a missing
//! input. Each scenario starts from a clean host, removes `purge-exit` and
//! empties the stub's readings, and leaves the host clean, uninstalling with
//! the stub's exit 0.
//!
//! It installs the product on the machine, so it builds only with the
//! test-only `host-tests` feature and runs only in `package-ci`'s `bundle`
//! job, on a GitHub-hosted runner discarded after the job; it refuses to act
//! elsewhere (`package_host::refuse_unless_disposable_runner`). Its inputs are
//! `package_bundle_host`'s: `CG_TEST_SETUP` (the release version this test is
//! built with), `CG_TEST_SETUP_NEXT` (1.2.4), `CG_TEST_SETUP_REBUILT` (a
//! second build of `CG_TEST_SETUP`'s version) and `CG_TEST_BURN_LOGS`.
#![cfg(windows)]

#[path = "../../cg-agent/tests/common/mod.rs"]
mod common;
mod fixtures;
mod package_host;

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime};

use package_host::*;
use windows_sys::Win32::System::Services::{
    SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_STOPPED,
};

/// The setup's upgrade family (`Bundle.wxs`), as Windows lists it.
const BUNDLE_UPGRADE_CODE: &str = "{B3D1FE6C-5F20-4E58-9B7A-3A7C8E1D4F10}";

/// Where the stub writes its readings and reads its exit code.
const PROBE_DIR: &str = r"C:\cg-probe";

// --- Inputs and the servers ----------------------------------------------------------

struct Setups {
    current: PathBuf,
    next: PathBuf,
    rebuilt: PathBuf,
}

fn setups() -> Setups {
    Setups {
        current: input("CG_TEST_SETUP"),
        next: input("CG_TEST_SETUP_NEXT"),
        rebuilt: input("CG_TEST_SETUP_REBUILT"),
    }
}

/// cg-agent's test enrollment server and a heartbeat server that verifies
/// under the key the enrollment certified, and an operator's directory for
/// the anchor and the bootstrap file, whose token is a canary.
struct Servers {
    runtime: tokio::runtime::Runtime,
    pki: common::TlsTestPki,
    enroll: Option<common::TlsEnrollMock>,
    heartbeat: Option<common::TlsMockServer>,
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
        let (enroll, heartbeat) = runtime.block_on(async {
            let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
            let heartbeat = common::TlsMockServer::start_for_enrolled(&pki, &enroll, 0, None).await;
            (enroll, heartbeat)
        });
        Servers {
            runtime,
            pki,
            enroll: Some(enroll),
            heartbeat: Some(heartbeat),
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

    /// A bootstrap file only SYSTEM and Administrators can reach, with the
    /// canary as its token and the anchor's fingerprint as its list.
    fn bootstrap(&self, name: &str) -> PathBuf {
        let anchor = self.operator.path().join("server-ca.pem");
        if !anchor.exists() {
            std::fs::write(&anchor, &self.pki.trust_anchor_pem).expect("the anchor's source");
        }
        let der = pem::parse(&self.pki.trust_anchor_pem)
            .expect("the anchor is PEM")
            .into_contents();
        let fingerprint = cg_agent::service::anchor::fingerprint(&der);
        let text = format!(
            "[server]\nurl = {}\nheartbeat_url = {}\ntrust_anchor_path = {}\n\n\
             [enrollment]\ntoken = {}\n\n[setup]\ntrust_anchor_sha256 = [{}]\n",
            fixtures::quoted(&self.enroll().base_url),
            fixtures::quoted(&self.heartbeat().base_url),
            fixtures::quoted(anchor.to_str().expect("a UTF-8 path")),
            fixtures::quoted(&self.canary),
            fixtures::quoted(&fingerprint),
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

// --- The stub ------------------------------------------------------------------------

fn probe_dir() -> PathBuf {
    PathBuf::from(PROBE_DIR)
}

/// The stub's exit code: `Some(code)` written to `purge-exit`, `None` its
/// removal (the stub then exits 0).
fn stub_exit(code: Option<i32>) {
    std::fs::create_dir_all(probe_dir()).expect("the probe's directory");
    let path = probe_dir().join("purge-exit");
    match code {
        Some(code) => std::fs::write(&path, code.to_string()).expect("purge-exit"),
        None => {
            if let Err(error) = std::fs::remove_file(&path) {
                assert_eq!(
                    error.kind(),
                    std::io::ErrorKind::NotFound,
                    "remove purge-exit"
                );
            }
        }
    }
}

/// Empty the stub's readings.
fn stub_truncate() {
    std::fs::create_dir_all(probe_dir()).expect("the probe's directory");
    std::fs::write(probe_dir().join("purge-readings.jsonl"), b"").expect("purge-readings.jsonl");
}

/// The stub's JSON lines since the last `stub_truncate`.
fn stub_lines() -> Vec<String> {
    std::fs::read_to_string(probe_dir().join("purge-readings.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_string)
        .collect()
}

// --- Burn's logs ---------------------------------------------------------------------

/// A run of the setup: its exit code, how long it took, its logs, Burn's own
/// first, each by its file name, and the stub's lines written while it ran.
struct Outcome {
    code: i32,
    took: Duration,
    logs: Vec<(String, String)>,
    stub: Vec<String>,
}

impl Outcome {
    /// Burn's own log, its lines without their process and time prefixes.
    fn burn(&self) -> Vec<String> {
        self.logs
            .first()
            .map(|(_, text)| burn_lines(text))
            .unwrap_or_default()
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
/// returned, its warnings and errors, and every line that names the purge
/// step.
fn chain(lines: &[String]) -> Vec<String> {
    const KEPT: [&str; 17] = [
        "i101", "i102", "i103", "i104", "i200", "i201", "i207", "i299", "i300", "i301", "i318",
        "i319", "i366", "i399", "i007", "i370", "i371",
    ];
    lines
        .iter()
        .filter(|line| {
            KEPT.iter().any(|id| line.starts_with(id))
                || line.contains("Purge")
                || line.contains("purge")
                || is_warning_or_error(line)
        })
        .map(|line| line.chars().take(400).collect())
        .collect()
}

fn is_warning_or_error(line: &str) -> bool {
    let mut characters = line.chars();
    matches!(characters.next(), Some('e' | 'w'))
        && characters.take(3).all(|c| c.is_ascii_digit())
        && line.get(4..5) == Some(":")
}

/// Burn's plan lines for package `package`.
fn plans(lines: &[String], package: &str) -> Vec<String> {
    let start = format!("i201: Planned package: {package}, ");
    lines
        .iter()
        .filter(|line| line.starts_with(&start))
        .cloned()
        .collect()
}

/// A log Burn or Windows Installer wrote, UTF-16 when it starts with a BOM.
fn read_log(path: &Path) -> String {
    text_of(&std::fs::read(path).unwrap_or_default())
}

// --- The host ------------------------------------------------------------------------

/// What a scenario runs, undone on drop: the stub's exit 0, each setup it ran
/// uninstalled, newest first, any setup of the family still registered
/// uninstalled from its entry or else unregistered, then any product of the
/// MSI's family, the service and the data root.
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
        stub_exit(None);
        stub_truncate();
        Host {
            paths,
            logs,
            _temporary: temporary,
            test,
            ran: Vec::new(),
        }
    }

    /// A reading, printed and appended to `purge-readings.txt`.
    fn read(&self, label: &str, text: impl std::fmt::Display) {
        let text = text.to_string();
        for line in text.lines() {
            println!("READING {} {label}: {line}", self.test);
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.logs.join("purge-readings.txt"))
            .expect("purge-readings.txt");
        writeln!(file, "== {} {label} ==\n{text}", self.test).expect("write a reading");
    }

    /// `setup` with `arguments` and `-quiet`, without a restart, its logs
    /// under `<test>-<name>`.
    fn setup(&mut self, name: &str, setup: &Path, arguments: &[&str]) -> Outcome {
        if !self.ran.iter().any(|known| known == setup) {
            self.ran.push(setup.to_path_buf());
        }
        let mut arguments: Vec<String> = arguments.iter().map(|a| a.to_string()).collect();
        arguments.push("-quiet".to_string());
        self.run(name, setup, &arguments, true)
    }

    /// `program` with `arguments`, its stub readings emptied first; with
    /// `logged`, `-norestart -log <test>-<name>.log` added, otherwise
    /// nothing added and Burn's logs read from where it writes them by
    /// default, `%TEMP%\CyberGuard_Agent_*.log`, and copied beside the others.
    fn run(&mut self, name: &str, program: &Path, arguments: &[String], logged: bool) -> Outcome {
        stub_truncate();
        let stem = format!("{}-{name}", self.test);
        let log = self.logs.join(format!("{stem}.log"));
        let since = SystemTime::now();
        let started = Instant::now();
        let mut command = Command::new(program);
        command.args(arguments);
        if logged {
            command.args(["-norestart", "-log"]).arg(&log);
        }
        let mut child = command.spawn().expect("run the setup");
        let deadline = Instant::now() + Duration::from_secs(900);
        let status = loop {
            if let Some(status) = child.try_wait().expect("the setup's state") {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = Command::new("taskkill.exe")
                    .args(["/F", "/T", "/IM", "cg-setup.exe"])
                    .status();
                let _ = child.wait();
                panic!("{stem}: the setup still runs after 15 minutes");
            }
            std::thread::sleep(Duration::from_millis(250));
        };
        let code = status.code().expect("the setup's exit code");
        let took = started.elapsed();
        let logs = if logged {
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
            logs
        } else {
            // Burn's own log sorts before its packages' (`.log` < `_NNN_`).
            let temporary = std::env::temp_dir();
            let mut files: Vec<String> = std::fs::read_dir(&temporary)
                .expect("the temporary folder")
                .flatten()
                .filter(|entry| {
                    entry
                        .metadata()
                        .and_then(|metadata| metadata.modified())
                        .is_ok_and(|modified| modified >= since)
                })
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|file| file.starts_with("CyberGuard_Agent_") && file.ends_with(".log"))
                .collect();
            files.sort();
            files
                .into_iter()
                .map(|file| {
                    let path = temporary.join(&file);
                    let _ = std::fs::copy(&path, self.logs.join(format!("{stem}-{file}")));
                    (file, read_log(&path))
                })
                .collect()
        };
        Outcome {
            code,
            took,
            logs,
            stub: stub_lines(),
        }
    }

    /// The readings of one run: its exit, the stub's lines, Burn's chain and
    /// the purge step's plan, and each other log's chain but the MSI's.
    fn report(&self, label: &str, outcome: &Outcome) {
        let burn = outcome.burn();
        self.read(
            &format!("{label} exit"),
            format!(
                "code {} (0x{:x}) after {:.1} s; Burn's last line: {}",
                outcome.code,
                outcome.code as u32,
                outcome.took.as_secs_f64(),
                burn.iter()
                    .rev()
                    .find(|line| !line.trim().is_empty())
                    .map(String::as_str)
                    .unwrap_or("(no log)")
            ),
        );
        self.read(
            &format!("{label} logs"),
            outcome
                .logs
                .iter()
                .map(|(file, _)| file.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        );
        let mut stub = vec![format!("the stub ran {} time(s)", outcome.stub.len())];
        stub.extend(outcome.stub.iter().cloned());
        self.read(&format!("{label} stub"), stub.join("\n"));
        let mut planned = plans(&burn, "Purge");
        planned.extend(plans(&burn, "Agent"));
        self.read(&format!("{label} plan"), lines_or_none(&planned));
        self.read(&format!("{label} burn"), lines_or_none(&chain(&burn)));
        for (file, text) in outcome.logs.iter().skip(1) {
            if file.ends_with("Agent.log") {
                continue;
            }
            self.read(
                &format!("{label} log {file}"),
                lines_or_none(&chain(&burn_lines(text))),
            );
        }
    }

    /// The machine's state: the setups and MSI products registered, the
    /// service, the binary, the data root, and the package cache's folders.
    fn state(&self, label: &str) {
        let mut lines = Vec::new();
        let setups = setups_registered();
        if setups.is_empty() {
            lines.push("setups registered: none".to_string());
        }
        for setup in &setups {
            let key = format!(r"{UNINSTALL_KEY}\{}", setup.key);
            lines.push(format!(
                "setup registered: {} {} hidden={} QuietUninstallString={:?}",
                setup.key,
                setup.version,
                setup.hidden,
                registry_value(&key, "QuietUninstallString")
            ));
        }
        let products = msi_products();
        if products.is_empty() {
            lines.push("MSI products: none".to_string());
        }
        for product in &products {
            lines.push(format!(
                "MSI product: {} {} hidden={}",
                product.key, product.version, product.hidden
            ));
        }
        lines.push(service_reading());
        lines.push(format!(
            "installed version: {:?}",
            cg_setup::version::read_installed(&self.paths.binary)
                .ok()
                .flatten()
                .and_then(|version| version.product_version)
        ));
        lines.push(format!(
            "binary {} exists: {}",
            self.paths.binary.display(),
            exists(&self.paths.binary)
        ));
        let agent = self.paths.product_root.join("agent");
        lines.push(format!(
            "{} exists: {}; {} exists: {}; its entries: {:?}",
            self.paths.product_root.display(),
            exists(&self.paths.product_root),
            agent.display(),
            exists(&agent),
            std::fs::read_dir(&agent)
                .map(|entries| entries
                    .flatten()
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .collect::<Vec<_>>())
                .map_err(|error| error.to_string())
        ));
        lines.push(format!("package cache: {:?}", cached()));
        lines.push(format!("purge folders: {:?}", step_folders("purge")));
        self.read(&format!("{label} state"), lines.join("\n"));
    }

    /// The package cache's purge folders and the files under each.
    fn purge_cache(&self, label: &str) {
        let cache = program_data().join("Package Cache");
        let mut lines = Vec::new();
        for folder in step_folders("purge") {
            lines.push(format!("folder {folder}"));
            let mut files = Vec::new();
            list_files(&cache.join(&folder), &mut files);
            lines.extend(files.into_iter().map(|file| format!("  {file}")));
        }
        self.read(&format!("{label} purge cache"), lines_or_none(&lines));
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(probe_dir().join("purge-exit"));
        for setup in std::mem::take(&mut self.ran).into_iter().rev() {
            let code = Command::new(&setup)
                .args(["-uninstall", "-quiet", "-norestart"])
                .status()
                .map(|status| status.code().unwrap_or(-1))
                .unwrap_or(-1);
            eprintln!("cleanup: {} -uninstall: exit {code}", setup.display());
        }
        // A setup still registered: its own entry's quiet uninstallation, then,
        // if that left it, its entry and its cache folder removed, so that the
        // next scenario starts on a clean host.
        for setup in setups_registered() {
            let key = format!(r"{UNINSTALL_KEY}\{}", setup.key);
            if let Some(Value::Text(line)) = registry_value(&key, "QuietUninstallString") {
                let (program, arguments) = split_command_line(&line);
                let code = Command::new(&program)
                    .args(&arguments)
                    .arg("-norestart")
                    .status()
                    .map(|status| status.code().unwrap_or(-1))
                    .unwrap_or(-1);
                eprintln!("cleanup: {line}: exit {code}");
            }
        }
        for setup in setups_registered() {
            let code = Command::new("reg.exe")
                .args([
                    "delete",
                    &format!(r"HKLM\{UNINSTALL_KEY}\{}", setup.key),
                    "/f",
                    "/reg:64",
                ])
                .status()
                .map(|status| status.code().unwrap_or(-1))
                .unwrap_or(-1);
            eprintln!("cleanup: reg delete {}: exit {code}", setup.key);
            let _ = std::fs::remove_dir_all(program_data().join("Package Cache").join(&setup.key));
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
        if exists(&self.paths.program_files_root) {
            if let Err(error) = std::fs::remove_dir_all(&self.paths.program_files_root) {
                eprintln!(
                    "remove {}: {error}",
                    self.paths.program_files_root.display()
                );
            }
        }
    }
}

/// A command line as Windows stores it in an uninstall entry: the program,
/// quoted or not, then its arguments split on white space.
fn split_command_line(line: &str) -> (String, Vec<String>) {
    let line = line.trim();
    let (program, rest) = match line.strip_prefix('"') {
        Some(rest) => rest.split_once('"').unwrap_or((rest, "")),
        None => line.split_once(' ').unwrap_or((line, "")),
    };
    (
        program.to_string(),
        rest.split_whitespace().map(str::to_string).collect(),
    )
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

fn lines_or_none(lines: &[String]) -> String {
    if lines.is_empty() {
        "(none)".to_string()
    } else {
        lines.join("\n")
    }
}

fn list_files(root: &Path, found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        found.push(format!("{} (not read)", root.display()));
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            list_files(&path, found);
        } else {
            let size = entry.metadata().map(|m| m.len()).unwrap_or_default();
            found.push(format!("{} ({size} bytes)", path.display()));
        }
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

/// The package cache's folders of cg-setup's step `step`.
fn step_folders(step: &str) -> Vec<String> {
    cached()
        .into_iter()
        .filter(|name| name.starts_with(&format!("cg-setup-{step}-")))
        .collect()
}

fn program_data() -> PathBuf {
    cg_agent::custody::program_data().expect("%ProgramData%")
}

/// The product service's state and process, or why it could not be opened.
fn service_reading() -> String {
    match open_service(PRODUCT_SERVICE, SERVICE_QUERY_STATUS) {
        Ok(service) => {
            let (state, pid) = service_process(&service);
            let name = match state {
                1 => "STOPPED",
                2 => "START_PENDING",
                3 => "STOP_PENDING",
                4 => "RUNNING",
                5 => "CONTINUE_PENDING",
                6 => "PAUSE_PENDING",
                7 => "PAUSED",
                _ => "unknown",
            };
            format!("service {PRODUCT_SERVICE}: state {state} ({name}), pid {pid}")
        }
        Err(error) => format!("service {PRODUCT_SERVICE}: not opened, Win32 error {error}"),
    }
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

/// A fresh installation of `setup`, which must be operational: exit 0, one
/// enrollment, an online heartbeat and the service running. Anything else is
/// the probe's machinery failing, not a reading.
fn install_operational(host: &mut Host, servers: &Servers, setup: &Path) -> Outcome {
    let bootstrap = servers.bootstrap("bootstrap.toml");
    let outcome = host.setup(
        "install",
        setup,
        &[&format!("CG_BOOTSTRAP={}", bootstrap.display())],
    );
    host.report("install", &outcome);
    assert_eq!(outcome.code, 0, "the installation is operational");
    assert_eq!(servers.enrollments(), 1, "one enrollment");
    assert!(servers.online() >= 1, "an online heartbeat came");
    assert!(
        wait_for_state(SERVICE_RUNNING, Duration::from_secs(60)),
        "the service runs after the installation"
    );
    host.state("after install");
    outcome
}

/// The agent after a setup that should leave it installed: whether the
/// service runs, and the enrollments and online heartbeats so far.
fn agent_reading(host: &Host, servers: &Servers, label: &str) {
    let running = wait_for_state(SERVICE_RUNNING, Duration::from_secs(60));
    host.read(
        &format!("{label} agent"),
        format!(
            "service RUNNING within 60 s: {running}; enrollments: {}; online heartbeats: {}",
            servers.enrollments(),
            servers.online()
        ),
    );
}

// --- The scenarios -------------------------------------------------------------------

/// R1: an installation, then `-uninstall -quiet CG_PURGE=yes`.
#[test]
fn r1_install_then_uninstall_with_purge() {
    let setups = setups();
    let mut host = Host::take("r1");
    let servers = Servers::start();
    install_operational(&mut host, &servers, &setups.current);

    let outcome = host.setup(
        "uninstall-purge",
        &setups.current,
        &["-uninstall", "CG_PURGE=yes"],
    );
    host.report("uninstall CG_PURGE=yes", &outcome);
    host.state("after uninstall CG_PURGE=yes");
}

/// R2: an installation, then `-uninstall -quiet` with no variable.
#[test]
fn r2_uninstall_without_variable() {
    let setups = setups();
    let mut host = Host::take("r2");
    let servers = Servers::start();
    install_operational(&mut host, &servers, &setups.current);

    let outcome = host.setup("uninstall", &setups.current, &["-uninstall"]);
    host.report("uninstall", &outcome);
    host.state("after uninstall");
}

/// R3: an installation of `CG_TEST_SETUP`, then `CG_TEST_SETUP_NEXT`
/// installed with `-quiet CG_PURGE=yes`: whether the older setup's purge ran
/// while Burn removed it, and with what; then the higher agent.
#[test]
fn r3_upgrade_with_variable_on_the_new_setup() {
    let setups = setups();
    let mut host = Host::take("r3");
    let servers = Servers::start();
    install_operational(&mut host, &servers, &setups.current);

    let outcome = host.setup("upgrade-purge", &setups.next, &["CG_PURGE=yes"]);
    host.report("upgrade CG_PURGE=yes", &outcome);
    agent_reading(&host, &servers, "after upgrade CG_PURGE=yes");
    host.state("after upgrade CG_PURGE=yes");
}

/// R4: an uninstallation with `CG_PURGE=yes` whose purge fails with 1, then
/// again with 46: the setup's exit, what Burn rolls back, and what remains;
/// then an uninstallation whose stub exits 0.
#[test]
fn r4_uninstall_with_a_failing_purge() {
    let setups = setups();
    let mut host = Host::take("r4");
    let servers = Servers::start();
    install_operational(&mut host, &servers, &setups.current);

    for code in [1, 46] {
        stub_exit(Some(code));
        let label = format!("uninstall CG_PURGE=yes, purge exits {code}");
        let outcome = host.setup(
            &format!("uninstall-purge-fails-{code}"),
            &setups.current,
            &["-uninstall", "CG_PURGE=yes"],
        );
        host.report(&label, &outcome);
        host.state(&format!("after {label}"));
    }

    stub_exit(None);
    let outcome = host.setup("uninstall", &setups.current, &["-uninstall"]);
    host.report("uninstall, purge exits 0", &outcome);
    host.state("after uninstall, purge exits 0");
}

/// R5: an installation, then the setup's own uninstall entry run as Windows
/// runs it, its `QuietUninstallString` with nothing added.
#[test]
fn r5_uninstall_from_the_registered_entry() {
    let setups = setups();
    let mut host = Host::take("r5");
    let servers = Servers::start();
    install_operational(&mut host, &servers, &setups.current);

    let registered = setups_registered();
    assert_eq!(registered.len(), 1, "one setup registered: {registered:?}");
    let key = format!(r"{UNINSTALL_KEY}\{}", registered[0].key);
    let quiet = registry_value(&key, "QuietUninstallString");
    host.read(
        "entry",
        format!(
            "QuietUninstallString: {quiet:?}\nUninstallString: {:?}\nModifyPath: {:?}",
            registry_value(&key, "UninstallString"),
            registry_value(&key, "ModifyPath")
        ),
    );
    let Some(Value::Text(line)) = quiet else {
        panic!("the setup's entry has no QuietUninstallString: {quiet:?}");
    };
    let (program, arguments) = split_command_line(&line);
    host.read(
        "entry parsed",
        format!("program: {program}\narguments: {arguments:?}"),
    );
    let outcome = host.run("entry-uninstall", Path::new(&program), &arguments, false);
    host.report("uninstall from the entry", &outcome);
    host.state("after uninstall from the entry");
}

/// R6: an installation, then `-repair -quiet CG_PURGE=yes`: whether the
/// purge runs on a repair.
#[test]
fn r6_repair_with_variable() {
    let setups = setups();
    let mut host = Host::take("r6");
    let servers = Servers::start();
    install_operational(&mut host, &servers, &setups.current);

    let outcome = host.setup(
        "repair-purge",
        &setups.current,
        &["-repair", "CG_PURGE=yes"],
    );
    host.report("repair CG_PURGE=yes", &outcome);
    agent_reading(&host, &servers, "after repair CG_PURGE=yes");
    host.state("after repair CG_PURGE=yes");
}

/// R7: an installation of `CG_TEST_SETUP`, then its second build,
/// `CG_TEST_SETUP_REBUILT`, with `-quiet CG_PURGE=yes`: whether the replaced
/// build's purge ran, and with what.
#[test]
fn r7_second_build_with_variable() {
    let setups = setups();
    let mut host = Host::take("r7");
    let servers = Servers::start();
    install_operational(&mut host, &servers, &setups.current);

    let outcome = host.setup("rebuilt-purge", &setups.rebuilt, &["CG_PURGE=yes"]);
    host.report("second build CG_PURGE=yes", &outcome);
    agent_reading(&host, &servers, "after second build CG_PURGE=yes");
    host.state("after second build CG_PURGE=yes");
}

/// R8: an installation, and the purge step at install: its plan line,
/// whether its cache folder exists, and what that folder holds.
#[test]
fn r8_the_purge_payload_at_install() {
    let setups = setups();
    let mut host = Host::take("r8");
    let servers = Servers::start();
    let outcome = install_operational(&mut host, &servers, &setups.current);

    let burn = outcome.burn();
    host.read("install purge plan", lines_or_none(&plans(&burn, "Purge")));
    host.read(
        "install purge lines",
        lines_or_none(
            &burn
                .iter()
                .filter(|line| line.contains("Purge") || line.contains("purge"))
                .cloned()
                .collect::<Vec<_>>(),
        ),
    );
    host.purge_cache("after install");
}
