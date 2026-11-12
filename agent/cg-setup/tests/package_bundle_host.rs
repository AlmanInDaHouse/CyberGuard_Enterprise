//! PROBE, never lands: what the agent's setup, the WiX bundle, does on a
//! hosted runner (C7, `docs/handoff-session-67.md`). Each test runs the
//! real `cyberguard-agent-<version>-x64-setup.exe` against cg-agent's test
//! enrollment and heartbeat servers and prints `READING` lines; the job's
//! step cannot fail. The facts go to the C7 decisions (#156, #157, the
//! restart's codes, the restore of the older bundle, the rollback, the
//! package cache and the logs).
//!
//! It installs the product on the machine, so it builds only with the
//! test-only `host-tests` feature and runs only in `package-ci`, on a
//! GitHub-hosted runner discarded after the job; it refuses to act
//! elsewhere.
#![cfg(windows)]

#[path = "../../cg-agent/tests/common/mod.rs"]
mod common;
mod fixtures;

use std::os::windows::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const PRODUCT_SERVICE: &str = "CyberGuardAgent";

fn reading(name: &str, text: impl std::fmt::Display) {
    for line in text.to_string().lines() {
        println!("READING {name}: {line}");
    }
}

fn input(name: &str) -> PathBuf {
    let path = PathBuf::from(
        std::env::var_os(name).unwrap_or_else(|| panic!("{name} names the job's input")),
    );
    assert!(path.is_file(), "{name}: {} is a file", path.display());
    path
}

fn program_data() -> PathBuf {
    cg_agent::custody::program_data().expect("%ProgramData%")
}

fn product_root() -> PathBuf {
    program_data().join("CyberGuard")
}

fn binary() -> PathBuf {
    cg_agent::custody::program_files()
        .expect("%ProgramFiles%")
        .join("CyberGuard")
        .join("Agent")
        .join("cg-agent.exe")
}

/// A command's exit code and its output, standard error after standard output.
fn run(program: &str, arguments: &[&str]) -> (i32, String) {
    match Command::new(program).args(arguments).output() {
        Ok(output) => (
            output.status.code().unwrap_or(-1),
            format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
        ),
        Err(error) => (-1, format!("{program}: {error}")),
    }
}

fn text_of(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

fn refuse_unless_disposable_runner() {
    let actions = std::env::var("GITHUB_ACTIONS").ok();
    let environment = std::env::var("RUNNER_ENVIRONMENT").ok();
    assert!(
        actions.as_deref() == Some("true") && environment.as_deref() == Some("github-hosted"),
        "refusing to run the setup outside a GitHub-hosted runner \
         (GITHUB_ACTIONS={actions:?}, RUNNER_ENVIRONMENT={environment:?})"
    );
    let (code, _) = run("sc.exe", &["query", PRODUCT_SERVICE]);
    assert_eq!(code, 1060, "refusing to act: {PRODUCT_SERVICE} exists");
    assert!(
        !product_root().exists(),
        "refusing to act: the data root exists"
    );
}

/// The service's state lines.
fn service_facts() -> String {
    let mut out = String::new();
    for arguments in [
        &["queryex", PRODUCT_SERVICE][..],
        &["qc", PRODUCT_SERVICE][..],
        &["qfailure", PRODUCT_SERVICE][..],
        &["sdshow", PRODUCT_SERVICE][..],
    ] {
        let (code, text) = run("sc.exe", arguments);
        let lines: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|line| {
                line.starts_with("STATE")
                    || line.starts_with("PID")
                    || line.starts_with("WIN32_EXIT_CODE")
                    || line.starts_with("SERVICE_EXIT_CODE")
                    || line.starts_with("START_TYPE")
                    || line.starts_with("BINARY_PATH_NAME")
                    || line.starts_with("RESET_PERIOD")
                    || line.starts_with("FAILURE_ACTIONS")
                    || line.contains("RESTART --")
                    || line.starts_with("D:")
                    || line.starts_with("[SC]")
            })
            .collect();
        out.push_str(&format!(
            "sc {} -> {code}: {}\n",
            arguments[0],
            lines.join(" | ")
        ));
    }
    out
}

fn service_pid() -> Option<u32> {
    let (_, text) = run("sc.exe", &["queryex", PRODUCT_SERVICE]);
    text.lines()
        .find_map(|line| line.trim().strip_prefix("PID"))
        .and_then(|rest| rest.trim_start_matches([' ', ':']).trim().parse().ok())
}

/// The ARP entries whose display name starts with "CyberGuard", with their
/// version and whether they are system components.
fn arp_facts() -> String {
    let mut out = Vec::new();
    for root in [
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
    ] {
        let (_, text) = run("reg.exe", &["query", root, "/s", "/f", "CyberGuard", "/d"]);
        let mut key = String::new();
        for line in text.lines() {
            if line.starts_with("HKEY_") {
                key = line.rsplit('\\').next().unwrap_or_default().to_string();
            } else if line.contains("DisplayName") {
                let (_, details) = run("reg.exe", &["query", &format!(r"{root}\{key}")]);
                let pick = |name: &str| {
                    details
                        .lines()
                        .find(|l| l.trim_start().starts_with(name))
                        .map(|l| l.trim().to_string())
                        .unwrap_or_default()
                };
                out.push(format!(
                    "{key}: {} {} {} {}",
                    pick("DisplayName"),
                    pick("DisplayVersion"),
                    pick("SystemComponent"),
                    pick("BundleUpgradeCode")
                ));
            }
        }
    }
    if out.is_empty() {
        "no ARP entry".into()
    } else {
        out.join("\n")
    }
}

fn package_cache_facts() -> String {
    let cache = program_data().join("Package Cache");
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&cache) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.contains("cg-setup") || cache_holds_setup(&entry.path()) {
                out.push(name);
            }
        }
    }
    if out.is_empty() {
        "no CyberGuard folder in the package cache".into()
    } else {
        out.join(", ")
    }
}

fn cache_holds_setup(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries.flatten().any(|e| {
                let name = e.file_name().to_string_lossy().to_lowercase();
                name.starts_with("cyberguard-agent")
            })
        })
        .unwrap_or(false)
}

fn binary_version() -> String {
    let path = binary();
    if !path.exists() {
        return "absent".into();
    }
    let (code, text) = run(path.to_str().unwrap(), &["--version"]);
    format!("{} (exit {code})", text.trim())
}

/// Take ownership of the data root and remove it, the identity included,
/// which only SYSTEM can reach otherwise.
fn remove_product_root() {
    let root = product_root();
    if !root.exists() {
        return;
    }
    let path = root.to_str().unwrap();
    let _ = run("takeown.exe", &["/F", path, "/R", "/A", "/D", "Y"]);
    let _ = run("icacls.exe", &[path, "/reset", "/T", "/C", "/Q"]);
    if let Err(error) = std::fs::remove_dir_all(&root) {
        eprintln!("remove {}: {error}", root.display());
    }
}

// --- The servers -----------------------------------------------------------------------

struct Servers {
    runtime: tokio::runtime::Runtime,
    pki: common::TlsTestPki,
    enroll: Option<common::TlsEnrollMock>,
    heartbeat: Option<common::TlsMockServer>,
    /// Hold each final going_offline request open.
    stall: Arc<AtomicBool>,
    /// Answer every heartbeat 401.
    reject: Arc<AtomicBool>,
    /// The anchor and the bootstrap file.
    operator: tempfile::TempDir,
    pub canary: String,
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
        let canary = format!("cgcanary{}", uuid::Uuid::now_v7().simple());
        Servers {
            runtime,
            pki,
            enroll: Some(enroll),
            heartbeat: Some(heartbeat),
            stall,
            reject,
            operator: tempfile::tempdir().expect("the operator's directory"),
            canary,
        }
    }

    fn enroll(&self) -> &common::TlsEnrollMock {
        self.enroll.as_ref().unwrap()
    }

    fn heartbeat(&self) -> &common::TlsMockServer {
        self.heartbeat.as_ref().unwrap()
    }

    fn fingerprint(&self) -> String {
        let der = pem::parse(&self.pki.trust_anchor_pem)
            .expect("the anchor is PEM")
            .into_contents();
        cg_agent::service::anchor::fingerprint(&der)
    }

    /// A bootstrap file only SYSTEM and Administrators can reach, with the
    /// canary as its token, and `fingerprint` as its list.
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
            fixtures::quoted(anchor.to_str().unwrap()),
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

    fn online(&self) -> usize {
        self.heartbeat()
            .received()
            .iter()
            .filter(|envelope| common::envelope_status(envelope) == "online")
            .count()
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

// --- The setup ---------------------------------------------------------------------------

struct Setups {
    current: PathBuf,
    next: PathBuf,
    failing: PathBuf,
    rebuilt: PathBuf,
    next_msi: PathBuf,
}

fn setups() -> Setups {
    Setups {
        current: input("CG_TEST_SETUP"),
        next: input("CG_TEST_SETUP_NEXT"),
        failing: input("CG_TEST_SETUP_FAILING"),
        rebuilt: input("CG_TEST_SETUP_REBUILT"),
        next_msi: input("CG_TEST_MSI_NEXT"),
    }
}

/// What a test ran, undone on drop: each setup uninstalled, any MSI of the
/// family removed, the data root removed.
struct Host {
    logs: PathBuf,
    test: &'static str,
    ran: Vec<PathBuf>,
}

struct Outcome {
    code: i32,
    took: Duration,
    log: String,
    files: Vec<PathBuf>,
}

impl Host {
    fn take(test: &'static str) -> Self {
        refuse_unless_disposable_runner();
        let logs = PathBuf::from(
            std::env::var_os("CG_TEST_BURN_LOGS").expect("CG_TEST_BURN_LOGS names a directory"),
        );
        std::fs::create_dir_all(&logs).expect("the logs' directory");
        Host {
            logs,
            test,
            ran: Vec::new(),
        }
    }

    /// `setup` with `arguments`, then `-quiet -norestart -log`, and its logs
    /// (the bundle's and its packages').
    fn setup(&mut self, name: &str, setup: &Path, arguments: &[&str]) -> Outcome {
        if !self.ran.iter().any(|known| known == setup) {
            self.ran.push(setup.to_path_buf());
        }
        let stem = format!("{}-{name}", self.test);
        let log = self.logs.join(format!("{stem}.log"));
        let mut command = Command::new(setup);
        command
            .args(arguments)
            .args(["-quiet", "-norestart", "-log"])
            .arg(&log);
        let started = Instant::now();
        let code = command
            .status()
            .expect("run the setup")
            .code()
            .unwrap_or(-1);
        let took = started.elapsed();
        let mut files = Vec::new();
        let mut text = String::new();
        if let Ok(entries) = std::fs::read_dir(&self.logs) {
            for entry in entries.flatten() {
                let file = entry.file_name().to_string_lossy().into_owned();
                if file.starts_with(&stem) {
                    let bytes = std::fs::read(entry.path()).unwrap_or_default();
                    text.push_str(&format!("===== {file}\n{}\n", text_of(&bytes)));
                    files.push(entry.path());
                }
            }
        }
        reading(
            self.test,
            format!("{name}: exit {code} after {:.1} s", took.as_secs_f64()),
        );
        Outcome {
            code,
            took,
            log: text,
            files,
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        for setup in std::mem::take(&mut self.ran).into_iter().rev() {
            let code = Command::new(&setup)
                .args(["-uninstall", "-quiet", "-norestart"])
                .status()
                .map(|s| s.code().unwrap_or(-1))
                .unwrap_or(-1);
            eprintln!("cleanup: {} -uninstall: exit {code}", setup.display());
        }
        // Any product of the MSI's upgrade family left behind.
        let (_, text) = run(
            "reg.exe",
            &[
                "query",
                r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
                "/s",
                "/f",
                "CyberGuard Agent",
                "/d",
            ],
        );
        for line in text.lines().filter(|l| l.starts_with("HKEY_")) {
            let code = line.rsplit('\\').next().unwrap_or_default();
            if code.starts_with('{') && code.len() == 38 {
                let (exit, _) = run("msiexec.exe", &["/x", code, "/qn", "/norestart"]);
                eprintln!("cleanup: msiexec /x {code}: {exit}");
            }
        }
        let _ = run("sc.exe", &["stop", PRODUCT_SERVICE]);
        std::thread::sleep(Duration::from_secs(2));
        let _ = run("sc.exe", &["delete", PRODUCT_SERVICE]);
        remove_product_root();
    }
}

/// The lines of a Burn log that say what ran, rolled back or was restored.
fn chain_lines(log: &str) -> String {
    log.lines()
        .filter(|line| {
            let l = line.to_lowercase();
            l.contains("applying execute package")
                || l.contains("applied execute package")
                || l.contains("applying rollback package")
                || l.contains("applied rollback package")
                || l.contains("related bundle")
                || l.contains("restore")
                || l.contains("exit code")
                || l.contains("exited with")
                || l.contains("restart")
                || l.contains("reboot")
                || l.contains("apply complete")
                || l.contains("detect complete")
                || l.contains("plan complete")
                || l.contains("planned package")
                || l.contains("planned related bundle")
                || l.contains("error 0x")
                || l.contains("result:")
                || l.contains("wixbundleuilevel")
                || l.contains("return value 3")
                || l.contains("error 19")
                || l.contains("error 1603")
                || l.contains("stopservices")
                || l.contains("cg-setup:")
        })
        .map(|line| line.chars().take(300).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// A fresh installation, operational: exit 0 expected.
fn install_operational(host: &mut Host, servers: &Servers, setup: &Path) -> Outcome {
    let bootstrap = servers.bootstrap("bootstrap.toml");
    let outcome = host.setup(
        "install",
        setup,
        &[&format!("CG_BOOTSTRAP={}", bootstrap.display())],
    );
    reading(
        host.test,
        format!(
            "install: enrolled {} time(s), online {}",
            servers.enroll().enroll_received_count(),
            servers.online()
        ),
    );
    reading(host.test, service_facts());
    if outcome.code != 0 {
        reading(host.test, chain_lines(&outcome.log));
    }
    outcome
}

// --- The canary --------------------------------------------------------------------------

/// Whether `bytes` holds the canary as UTF-8 or UTF-16LE.
fn holds(bytes: &[u8], canary: &str) -> bool {
    let utf8 = canary.as_bytes();
    let utf16: Vec<u8> = canary
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    bytes.windows(utf8.len()).any(|w| w == utf8) || bytes.windows(utf16.len()).any(|w| w == utf16)
}

fn scan_tree(root: &Path, canary: &str, hits: &mut Vec<String>, budget: &mut usize) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        if *budget == 0 {
            return;
        }
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            scan_tree(&path, canary, hits, budget);
        } else if let Ok(meta) = entry.metadata() {
            if meta.len() <= 64 * 1024 * 1024 {
                *budget -= 1;
                if let Ok(bytes) = std::fs::read(&path) {
                    if holds(&bytes, canary) {
                        hits.push(path.display().to_string());
                    }
                }
            }
        }
    }
}

/// Every place outside the bootstrap file and the staged and adopted
/// configuration where the canary is found.
fn canary_hits(servers: &Servers, extra: &[&Path]) -> Vec<String> {
    let canary = &servers.canary;
    let mut hits = Vec::new();
    let mut budget = 200_000;
    let mut roots: Vec<PathBuf> = vec![
        std::env::temp_dir(),
        PathBuf::from(r"C:\Windows\Temp"),
        PathBuf::from(r"C:\Windows\SystemTemp"),
        program_data().join("Package Cache"),
        PathBuf::from(r"C:\Windows\Installer"),
        product_root(),
    ];
    roots.extend(extra.iter().map(|p| p.to_path_buf()));
    for root in roots {
        scan_tree(&root, canary, &mut hits, &mut budget);
    }
    hits.retain(|hit| !hit.starts_with(servers.operator.path().to_str().unwrap()));
    for key in [
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce",
        r"HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce",
        r"HKLM\SOFTWARE\Classes\Installer",
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Installer",
        r"HKLM\SYSTEM\CurrentControlSet\Services\CyberGuardAgent",
        r"HKCU\SOFTWARE",
    ] {
        let (_, text) = run("reg.exe", &["query", key, "/s", "/f", canary]);
        if text.contains(canary.as_str()) {
            hits.push(format!("registry {key}"));
        }
    }
    for log in [
        "Application",
        "System",
        "Microsoft-Windows-PowerShell/Operational",
    ] {
        let (_, text) = run(
            "wevtutil.exe",
            &["qe", log, "/c:2000", "/rd:true", "/f:text"],
        );
        if text.contains(canary.as_str()) {
            hits.push(format!("event log {log}"));
        }
    }
    hits
}

// --- The readings ------------------------------------------------------------------------

/// A fresh installation, its outcome, its registration, its logs and
/// where the token is found; the resume command line during apply; a
/// normal uninstallation.
#[test]
fn a_fresh_install_canary_and_uninstall() {
    let setups = setups();
    let mut host = Host::take("a");
    let servers = Servers::start();

    // The resume command line and RunOnce, sampled during the apply.
    let sampling = Arc::new(AtomicBool::new(true));
    let sampler = {
        let sampling = sampling.clone();
        let canary = servers.canary.clone();
        std::thread::spawn(move || {
            let mut seen = Vec::<String>::new();
            while sampling.load(Ordering::SeqCst) {
                for key in [
                    r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
                    r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce",
                ] {
                    let (_, text) = run("reg.exe", &["query", key, "/s", "/f", "cyberguard", "/d"]);
                    for line in text.lines().filter(|l| {
                        l.contains("BundleResumeCommandLine")
                            || l.contains("RunOnce")
                            || l.contains("{")
                    }) {
                        let line = line.trim().replace(&canary, "<CANARY>");
                        if !seen.contains(&line) {
                            seen.push(line);
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            seen
        })
    };
    let outcome = install_operational(&mut host, &servers, &setups.current);
    sampling.store(false, Ordering::SeqCst);
    let seen = sampler.join().unwrap();
    reading(
        "a",
        format!(
            "resume and RunOnce lines seen during apply:\n{}",
            seen.join("\n")
        ),
    );
    reading("a", chain_lines(&outcome.log));
    reading("a", format!("arp:\n{}", arp_facts()));
    reading("a", format!("cache: {}", package_cache_facts()));
    reading("a", format!("binary: {}", binary_version()));
    reading("a", format!("log files: {:?}", outcome.files));
    reading(
        "a",
        format!(
            "the canary is in the Burn log: {}; the enroll mock received it: {}",
            outcome.log.contains(&servers.canary),
            servers
                .enroll()
                .enroll_received()
                .iter()
                .any(|r| r.to_string().contains(&servers.canary))
        ),
    );
    let hits = canary_hits(&servers, &[&host.logs]);
    reading("a", format!("canary hits after install: {hits:?}"));
    let icacls = run(
        "icacls.exe",
        &[program_data().join("Package Cache").to_str().unwrap()],
    )
    .1;
    reading("a", format!("Package Cache DACL:\n{icacls}"));

    // A normal uninstallation.
    let outcome = host.setup("uninstall", &setups.current, &["-uninstall"]);
    reading("a", chain_lines(&outcome.log));
    reading("a", service_facts());
    reading("a", format!("arp after uninstall:\n{}", arp_facts()));
    reading(
        "a",
        format!("cache after uninstall: {}", package_cache_facts()),
    );
    reading("a", format!("data root kept: {}", product_root().exists()));
    let hits = canary_hits(&servers, &[&host.logs]);
    reading("a", format!("canary hits after uninstall: {hits:?}"));

    // A reinstallation over the preserved identity, with no bootstrap.
    let before = servers.enroll().enroll_received_count();
    let outcome = host.setup("reinstall", &setups.current, &[]);
    reading(
        "a",
        format!(
            "reinstall: enrollments {before} -> {}; online {}",
            servers.enroll().enroll_received_count(),
            servers.online()
        ),
    );
    reading("a", chain_lines(&outcome.log));
}

/// Refusals through Burn: no input (22) and a wrong fingerprint (25):
/// the exit code, the rollback of preflight, what remains registered, and
/// whether cg-setup's line reaches any log.
#[test]
fn b_refusals() {
    let setups = setups();
    let mut host = Host::take("b");
    let servers = Servers::start();

    let outcome = host.setup("no-input", &setups.current, &[]);
    reading("b", chain_lines(&outcome.log));
    reading("b", format!("arp: {}", arp_facts()));
    reading("b", format!("cache: {}", package_cache_facts()));
    reading(
        "b",
        format!(
            "cg-setup's line in a log: {}",
            outcome.log.contains("cg-setup:")
        ),
    );

    let wrong = format!("sha256:{}", "0".repeat(64));
    let bootstrap = servers.bootstrap_with("wrong.toml", &wrong);
    let outcome = host.setup(
        "wrong-fingerprint",
        &setups.current,
        &[&format!("CG_BOOTSTRAP={}", bootstrap.display())],
    );
    reading("b", chain_lines(&outcome.log));
    reading("b", format!("arp: {}", arp_facts()));
    reading("b", format!("data root: {}", product_root().exists()));

    // An injection attempt through the bundle variable.
    let bootstrap = servers.bootstrap("inject.toml");
    let outcome = host.setup(
        "injection",
        &setups.current,
        &[&format!(
            "CG_BOOTSTRAP={}\" --migrate-legacy=\"remove",
            bootstrap.display()
        )],
    );
    reading("b", chain_lines(&outcome.log));

    // A command-line attempt to set the UI level.
    let outcome = host.setup("ui-level", &setups.current, &["WixBundleUILevel=4"]);
    reading("b", chain_lines(&outcome.log));
}

/// Upgrade, downgrade, and a refusal by the newer preflight over an
/// installed older setup (the restore of the older bundle).
#[test]
fn c_upgrade_downgrade_and_refusal_on_upgrade() {
    let setups = setups();
    let mut host = Host::take("c");
    let servers = Servers::start();
    install_operational(&mut host, &servers, &setups.current);
    let enrolled = servers.enroll().enroll_received_count();

    let outcome = host.setup("upgrade", &setups.next, &[]);
    reading("c", chain_lines(&outcome.log));
    reading(
        "c",
        format!(
            "after upgrade: binary {}; enrollments {enrolled} -> {}",
            binary_version(),
            servers.enroll().enroll_received_count()
        ),
    );
    reading("c", service_facts());
    reading("c", format!("arp:\n{}", arp_facts()));

    let pid = service_pid();
    let outcome = host.setup("downgrade", &setups.current, &[]);
    reading("c", chain_lines(&outcome.log));
    reading(
        "c",
        format!(
            "after downgrade: binary {}; pid {pid:?} -> {:?}",
            binary_version(),
            service_pid()
        ),
    );
    reading("c", format!("arp:\n{}", arp_facts()));

    // Back to 1.2.3, then 1.2.4 with a bootstrap on an enrolled host (30).
    let outcome = host.setup("uninstall-next", &setups.next, &["-uninstall"]);
    reading("c", format!("uninstall 1.2.4: {}", outcome.code));
    let outcome = host.setup("reinstall-current", &setups.current, &[]);
    reading(
        "c",
        format!(
            "reinstall 1.2.3: {}; binary {}",
            outcome.code,
            binary_version()
        ),
    );
    let bootstrap = servers.bootstrap("again.toml");
    let outcome = host.setup(
        "refused-upgrade",
        &setups.next,
        &[&format!("CG_BOOTSTRAP={}", bootstrap.display())],
    );
    reading("c", chain_lines(&outcome.log));
    reading(
        "c",
        format!("after the refused upgrade: binary {}", binary_version()),
    );
    reading("c", service_facts());
    reading("c", format!("arp:\n{}", arp_facts()));
    reading(
        "c",
        format!(
            "record left: {}",
            product_root()
                .join("agent")
                .join("setup-attempt.json")
                .exists()
        ),
    );
}

/// The 1.2.4 MSI fails inside its transaction over an operational 1.2.3;
/// then a configure that fails (heartbeats refused) on the upgrade.
#[test]
fn d_injected_failure_and_configure_failure_on_upgrade() {
    let setups = setups();
    let mut host = Host::take("d");
    let servers = Servers::start();
    install_operational(&mut host, &servers, &setups.current);
    let before = service_facts();

    let outcome = host.setup("failing-upgrade", &setups.failing, &[]);
    reading("d", chain_lines(&outcome.log));
    reading("d", format!("binary after: {}", binary_version()));
    reading("d", format!("service before:\n{before}"));
    reading("d", format!("service after:\n{}", service_facts()));
    reading("d", format!("arp:\n{}", arp_facts()));
    reading(
        "d",
        format!(
            "record left: {}",
            product_root()
                .join("agent")
                .join("setup-attempt.json")
                .exists()
        ),
    );

    servers.reject.store(true, Ordering::SeqCst);
    let outcome = host.setup("configure-fails", &setups.next, &[]);
    reading("d", chain_lines(&outcome.log));
    reading("d", format!("binary after: {}", binary_version()));
    reading("d", format!("service after:\n{}", service_facts()));
    reading("d", format!("arp:\n{}", arp_facts()));
    reading(
        "d",
        format!(
            "record left: {}",
            product_root()
                .join("agent")
                .join("setup-attempt.json")
                .exists()
        ),
    );
    servers.reject.store(false, Ordering::SeqCst);
}

/// Two builds of the same version (#157): A installed, then B.
#[test]
fn e_same_version_rebuild() {
    let setups = setups();
    let mut host = Host::take("e");
    let servers = Servers::start();
    install_operational(&mut host, &servers, &setups.current);
    let enrolled = servers.enroll().enroll_received_count();
    let outcome = host.setup("rebuilt", &setups.rebuilt, &[]);
    reading("e", chain_lines(&outcome.log));
    reading("e", format!("arp:\n{}", arp_facts()));
    reading("e", service_facts());
    reading(
        "e",
        format!(
            "enrollments {enrolled} -> {}",
            servers.enroll().enroll_received_count()
        ),
    );
    let (_, products) = run(
        "reg.exe",
        &[
            "query",
            r"HKLM\SOFTWARE\Classes\Installer\UpgradeCodes",
            "/s",
            "/f",
            "CyberGuard",
            "/d",
        ],
    );
    reading("e", format!("upgrade-code products: {products}"));
}

/// Repair: a changed binary of the same version and a changed start type.
#[test]
fn f_repair() {
    let setups = setups();
    let mut host = Host::take("f");
    let servers = Servers::start();
    install_operational(&mut host, &servers, &setups.current);
    let enrolled = servers.enroll().enroll_received_count();
    let original = std::fs::read(binary()).expect("the binary");
    let _ = run("sc.exe", &["stop", PRODUCT_SERVICE]);
    std::thread::sleep(Duration::from_secs(10));
    let mut changed = original.clone();
    changed.extend_from_slice(b"tampered");
    let written = std::fs::write(binary(), &changed);
    reading("f", format!("tamper write: {written:?}"));
    let _ = run("sc.exe", &["config", PRODUCT_SERVICE, "start=", "demand"]);
    let _ = run(
        "sc.exe",
        &["failure", PRODUCT_SERVICE, "reset=", "0", "actions=", ""],
    );
    reading("f", format!("before repair:\n{}", service_facts()));

    let outcome = host.setup("repair", &setups.current, &["-repair"]);
    reading("f", chain_lines(&outcome.log));
    let now = std::fs::read(binary()).unwrap_or_default();
    reading(
        "f",
        format!(
            "binary restored: {}; tampered still: {}",
            now == original,
            now == changed
        ),
    );
    reading("f", format!("after repair:\n{}", service_facts()));
    reading(
        "f",
        format!(
            "enrollments {enrolled} -> {}",
            servers.enroll().enroll_received_count()
        ),
    );
}

/// #156: an uninstallation while the agent's final request stalls, with
/// the configuration the setup writes and then with a request timeout of
/// 55 seconds, as a migrated or administrator-edited configuration can
/// hold.
#[test]
fn g_uninstall_with_a_stalled_final_request() {
    let setups = setups();
    for (leg, timeout) in [("generated", None), ("timeout-55", Some(55u64))] {
        let mut host = Host::take("g");
        let servers = Servers::start();
        install_operational(&mut host, &servers, &setups.current);
        if let Some(seconds) = timeout {
            let config = product_root().join("agent").join("agent.toml");
            let text = std::fs::read_to_string(&config).expect("agent.toml, as an administrator");
            let mut table: toml::Table = text.parse().expect("TOML");
            let heartbeat = table
                .entry("heartbeat")
                .or_insert_with(|| toml::Value::Table(toml::Table::new()));
            heartbeat.as_table_mut().unwrap().insert(
                "request_timeout_seconds".into(),
                toml::Value::Integer(seconds as i64),
            );
            std::fs::write(&config, toml::to_string(&table).unwrap()).expect("rewrite agent.toml");
            let (code, text) = run("sc.exe", &["stop", PRODUCT_SERVICE]);
            reading("g", format!("{leg}: stop {code} {}", text.lines().count()));
            std::thread::sleep(Duration::from_secs(15));
            let online = servers.online();
            let (code, _) = run("sc.exe", &["start", PRODUCT_SERVICE]);
            let deadline = Instant::now() + Duration::from_secs(90);
            while servers.online() <= online && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(500));
            }
            reading(
                "g",
                format!(
                    "{leg}: restarted {code}; online {online} -> {}",
                    servers.online()
                ),
            );
        }
        servers.stall.store(true, Ordering::SeqCst);
        let outcome = host.setup(
            &format!("uninstall-{leg}"),
            &setups.current,
            &["-uninstall"],
        );
        reading(
            "g",
            format!(
                "{leg}: uninstall took {:.1} s, exit {}",
                outcome.took.as_secs_f64(),
                outcome.code
            ),
        );
        reading("g", chain_lines(&outcome.log));
        reading(
            "g",
            format!(
                "{leg}: at once:\n{}binary: {}\narp:\n{}",
                service_facts(),
                binary_version(),
                arp_facts()
            ),
        );
        std::thread::sleep(Duration::from_secs(60));
        reading(
            "g",
            format!(
                "{leg}: 60 s later:\n{}binary: {}\narp:\n{}",
                service_facts(),
                binary_version(),
                arp_facts()
            ),
        );
        let (_, pending) = run(
            "reg.exe",
            &[
                "query",
                r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager",
                "/v",
                "PendingFileRenameOperations",
            ],
        );
        reading(
            "g",
            format!("{leg}: pending renames: {}", pending.contains("CyberGuard")),
        );
        if outcome.code != 0 {
            servers.stall.store(false, Ordering::SeqCst);
            let retry = host.setup(
                &format!("uninstall-retry-{leg}"),
                &setups.current,
                &["-uninstall"],
            );
            reading(
                "g",
                format!(
                    "{leg}: retry exit {} after {:.1} s",
                    retry.code,
                    retry.took.as_secs_f64()
                ),
            );
            reading("g", chain_lines(&retry.log));
        }
        drop(servers);
        drop(host);
    }
}

/// The setup started as SYSTEM by a scheduled task, as a deployment tool
/// does: where Burn writes its log, and that directory's DACL.
#[test]
fn h_system_launch_log() {
    let setups = setups();
    let host = Host::take("h");
    let marker = Instant::now();
    let task = "CgProbeSetupAsSystem";
    let command = format!("\"{}\" -quiet -norestart", setups.current.display());
    let (code, text) = run(
        "schtasks.exe",
        &[
            "/Create", "/TN", task, "/RU", "SYSTEM", "/SC", "ONCE", "/ST", "23:59", "/TR",
            &command, "/F",
        ],
    );
    reading("h", format!("create {code}: {}", text.trim()));
    let (code, text) = run("schtasks.exe", &["/Run", "/TN", task]);
    reading("h", format!("run {code}: {}", text.trim()));
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        std::thread::sleep(Duration::from_secs(5));
        let (_, text) = run(
            "schtasks.exe",
            &["/Query", "/TN", task, "/V", "/FO", "LIST"],
        );
        let running = text
            .lines()
            .any(|l| l.contains("Status:") && l.contains("Running"));
        if !running || Instant::now() > deadline {
            let result: Vec<&str> = text
                .lines()
                .filter(|l| l.contains("Last Result") || l.contains("Status:"))
                .collect();
            reading("h", format!("task: {}", result.join(" | ")));
            break;
        }
    }
    for dir in [r"C:\Windows\SystemTemp", r"C:\Windows\Temp"] {
        let mut found = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let recent = entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .map(|t| {
                        t.elapsed()
                            .map(|e| e < marker.elapsed() + Duration::from_secs(5))
                            .unwrap_or(false)
                    })
                    .unwrap_or(false);
                if recent && name.to_lowercase().contains("cyberguard") {
                    found.push(name);
                }
            }
        }
        let (_, acl) = run("icacls.exe", &[dir]);
        reading("h", format!("{dir}: logs {found:?}\n{acl}"));
    }
    let _ = run("schtasks.exe", &["/Delete", "/TN", task, "/F"]);
    drop(host);
}

/// The restart Windows Installer asks for when the binary is held: a
/// handle that shares only reading (no rename), then an image mapping.
#[test]
fn z_restart_pending() {
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows_sys::Win32::Foundation::FreeLibrary;
    use windows_sys::Win32::System::LibraryLoader::{
        LoadLibraryExW, LOAD_LIBRARY_AS_IMAGE_RESOURCE,
    };

    let setups = setups();
    for leg in ["share-read", "image-mapping"] {
        let mut host = Host::take("z");
        let servers = Servers::start();
        install_operational(&mut host, &servers, &setups.current);
        let _ = run("sc.exe", &["stop", PRODUCT_SERVICE]);
        std::thread::sleep(Duration::from_secs(15));
        let path = binary();
        let mut file = None;
        let mut module = std::ptr::null_mut();
        if leg == "share-read" {
            file = Some(
                std::fs::OpenOptions::new()
                    .read(true)
                    .share_mode(1)
                    .open(&path)
                    .expect("hold the binary"),
            );
        } else {
            let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
            // SAFETY: a NUL-terminated path; the module is freed below.
            module = unsafe {
                LoadLibraryExW(
                    wide.as_ptr(),
                    std::ptr::null_mut(),
                    LOAD_LIBRARY_AS_IMAGE_RESOURCE,
                )
            };
            reading("z", format!("{leg}: mapped {}", !module.is_null()));
        }
        let outcome = host.setup(&format!("upgrade-{leg}"), &setups.next, &[]);
        reading("z", chain_lines(&outcome.log));
        drop(file);
        if !module.is_null() {
            // SAFETY: loaded above.
            unsafe { FreeLibrary(module) };
        }
        reading("z", format!("{leg}: binary {}", binary_version()));
        reading("z", service_facts());
        let (_, pending) = run(
            "reg.exe",
            &[
                "query",
                r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager",
                "/v",
                "PendingFileRenameOperations",
            ],
        );
        reading("z", format!("{leg}: pending renames:\n{pending}"));
        // Clear the pending operations this leg caused, for the next leg.
        let _ = run(
            "reg.exe",
            &[
                "delete",
                r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager",
                "/v",
                "PendingFileRenameOperations",
                "/f",
            ],
        );
        drop(servers);
        drop(host);
    }
}

/// Downgrade over an MSI of a higher version installed alone (29).
#[test]
fn i_downgrade_over_a_lone_msi() {
    let setups = setups();
    let mut host = Host::take("i");
    let msi = setups.next_msi.to_str().unwrap().to_string();
    let (code, _) = run(
        "msiexec.exe",
        &["/i", &msi, "CG_SETUP_CHAIN=1", "/qn", "/norestart"],
    );
    reading("i", format!("msi 1.2.4 alone: {code}"));
    let outcome = host.setup("over-msi", &setups.current, &[]);
    reading("i", chain_lines(&outcome.log));
    reading("i", format!("binary: {}", binary_version()));
    let _ = run("msiexec.exe", &["/x", &msi, "/qn", "/norestart"]);
}
