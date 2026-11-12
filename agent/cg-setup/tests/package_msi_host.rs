//! The agent's MSI on the host (SPEC-024 §Data contracts §1 to §3,
//! install_ac_001, its Amendment 2026-11-09, item 4, and its Amendment
//! 2026-11-12 with its *Tests*):
//! installed as the setup installs it, it registers per machine one
//! automatic `LocalSystem` service of its own process at the quoted binary
//! and configuration paths, which only SYSTEM and Administrators may change,
//! start, stop or delete and ordinary users may query; the binary, read and
//! run by ordinary users, is theirs neither to replace nor to change; the
//! Application log's source names the binary's message table; nothing is
//! created under `%ProgramData%\CyberGuard`. A direct `msiexec` installation
//! or major upgrade, and one in a per-user context, are refused. Its
//! uninstallation removes what it installed and leaves
//! `%ProgramData%\CyberGuard`.
//!
//! The test installs the MSI and its fixed-name service on the machine, so
//! it builds only with the test-only `host-tests` feature and runs only in
//! `package-ci`, on a GitHub-hosted runner discarded after the job (CLAUDE.md,
//! *Local pre-commit gate*, *Windows package*). It fails without acting
//! outside such a runner, when a service named `CyberGuardAgent` exists, when
//! `%ProgramData%\CyberGuard` or `%ProgramFiles%\CyberGuard` exists, or when
//! a product of the MSI's upgrade family is registered; `Host` uninstalls what
//! it installed and removes what it created on every exit path.
//!
//! Its inputs come from the job: `CG_TEST_MSI`, the MSI built with
//! `CG_RELEASE_VERSION`, which this test is built with too;
//! `CG_TEST_MSI_NEXT`, the same package at a higher version; and
//! `CG_TEST_AGENT_EXE`, the raw executable the MSI carries; and
//! `CG_TEST_MSI_LOGS`, if set, where msiexec's logs go. The service's access
//! is read as §Data contracts §3 words it, by its effect on an ordinary
//! user's tokens, and as the SDDL of the Amendment 2026-11-12, point 2. The
//! helpers it shares with the setup's test are `package_host`.
#![cfg(windows)]

mod package_host;

use std::path::{Path, PathBuf};
use std::process::Command;

use cg_setup::scm::{self, Manager, Product, Route};
use package_host::*;
use windows_sys::Win32::Storage::FileSystem::READ_CONTROL;
use windows_sys::Win32::System::Services::{
    SERVICE_AUTO_START, SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS, SERVICE_START, SERVICE_STOPPED,
    SERVICE_WIN32_OWN_PROCESS,
};

/// The launch condition's message, from the MSI's source.
const DIRECT_REFUSAL: &str = "CyberGuard Agent is installed by its setup";

/// What `msiexec` exits with when an installation fails, a false launch
/// condition included.
const ERROR_INSTALL_FAILURE: i32 = 1603;

/// What `msiexec` exits with while another installation is in progress.
const ERROR_INSTALL_ALREADY_RUNNING: i32 = 1618;

/// The second launch condition's message, from the MSI's source.
const PER_MACHINE_ONLY: &str = "CyberGuard Agent is installed per machine only";

/// What the run installs and creates, removed on drop, guard first.
struct Host {
    paths: Paths,
    /// Where msiexec's logs go: `CG_TEST_MSI_LOGS` when the job names it, so
    /// that it can keep them, or a temporary directory.
    logs: PathBuf,
    _temporary: Option<tempfile::TempDir>,
    /// Each log's name starts with the test's.
    test: &'static str,
    installed: Vec<PathBuf>,
}

impl Host {
    fn take(test: &'static str) -> Self {
        let paths = Paths::machine();
        refuse_unless_disposable_runner(&paths, "install the MSI");
        let (logs, temporary) = match std::env::var_os("CG_TEST_MSI_LOGS") {
            Some(logs) => {
                let logs = PathBuf::from(logs);
                std::fs::create_dir_all(&logs).expect("the logs' directory");
                (logs, None)
            }
            None => {
                let temporary = tempfile::tempdir().expect("a directory for msiexec's logs");
                (temporary.path().to_path_buf(), Some(temporary))
            }
        };
        Host {
            paths,
            logs,
            _temporary: temporary,
            test,
            installed: Vec::new(),
        }
    }

    /// `msiexec` with `verb` (`/i` or `/x`) on `msi` and `properties`, quiet,
    /// without a restart, and its verbose log under `name`: its exit code and
    /// its log. msiexec parses its own command line, so it is passed as
    /// written (`raw_arg`): a property such as `ALLUSERS=""` reaches it empty.
    /// Each `/i` is recorded for `Drop`, refused or not, so that one a launch
    /// condition should have refused is removed too. Another installation in
    /// progress (1618) is waited for, up to five minutes.
    fn msiexec(
        &mut self,
        name: &str,
        verb: &str,
        msi: &Path,
        properties: &[&str],
    ) -> (i32, String) {
        use std::os::windows::process::CommandExt as _;
        if verb == "/i" && !self.installed.iter().any(|known| known == msi) {
            self.installed.push(msi.to_path_buf());
        }
        let log = self.logs.join(format!("{}-{name}.log", self.test));
        let mut command_line = format!("{verb} \"{}\"", msi.display());
        for property in properties {
            command_line.push(' ');
            command_line.push_str(property);
        }
        command_line.push_str(&format!(" /qn /norestart /l*v \"{}\"", log.display()));
        let mut code = ERROR_INSTALL_ALREADY_RUNNING;
        for _ in 0..10 {
            code = Command::new("msiexec.exe")
                .raw_arg(&command_line)
                .status()
                .expect("run msiexec")
                .code()
                .expect("msiexec's exit code");
            if code != ERROR_INSTALL_ALREADY_RUNNING {
                break;
            }
            eprintln!("{name}: another installation is in progress (1618); waiting");
            std::thread::sleep(std::time::Duration::from_secs(30));
        }
        let bytes = std::fs::read(&log).unwrap_or_default();
        // msiexec writes its log in UTF-16 when it starts with a BOM.
        let text = if bytes.starts_with(&[0xFF, 0xFE]) {
            let units: Vec<u16> = bytes[2..]
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            String::from_utf16_lossy(&units)
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        };
        (code, text)
    }

    /// Install `msi` as the setup does: with the property its chain passes.
    fn install_as_the_setup(&mut self, name: &str, msi: &Path) {
        let (code, log) = self.msiexec(name, "/i", msi, &["CG_SETUP_CHAIN=1"]);
        assert_eq!(code, 0, "{name}: msiexec exits 0\n{}", tail(&log));
    }

    fn uninstall(&mut self, name: &str, msi: &Path) {
        let (code, log) = self.msiexec(name, "/x", msi, &[]);
        assert_eq!(code, 0, "{name}: msiexec exits 0\n{}", tail(&log));
        self.installed.retain(|installed| installed != msi);
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        // Newest first; a package that is not installed answers 1605.
        for msi in std::mem::take(&mut self.installed).into_iter().rev() {
            let (code, _) = self.msiexec("cleanup", "/x", &msi, &[]);
            eprintln!("cleanup: msiexec /x {}: exit {code}", msi.display());
        }
        self.installed.clear();
        if exists(&self.paths.product_root) {
            if let Err(error) = std::fs::remove_dir_all(&self.paths.product_root) {
                eprintln!("remove {}: {error}", self.paths.product_root.display());
            }
        }
    }
}

/// The last lines of an msiexec log, for a failure's message: it holds no
/// secret, the MSI carrying none.
fn tail(log: &str) -> String {
    let lines: Vec<&str> = log.lines().collect();
    lines[lines.len().saturating_sub(60)..].join("\n")
}

// --- The tests -----------------------------------------------------------------------

/// Nothing of the product on the machine: no service, no product of the
/// upgrade family, no program listed, and no binary under either
/// `%ProgramFiles%` or the per-user program folder `per_user`.
fn assert_nothing_installed(paths: &Paths, per_user: &Path) {
    let manager = Manager::open().expect("the SCM, for connecting");
    assert_eq!(product_service(&manager), Product::Absent, "no service");
    assert!(!exists(&paths.program_files_root), "no binary");
    assert!(
        !exists(&per_user.join("CyberGuard")),
        "nothing under {}",
        per_user.display()
    );
    assert!(!key_exists(UPGRADE_CODE_KEY), "no product registered");
    assert!(registered_versions().is_empty(), "no program listed");
    assert!(!key_exists(EVENT_SOURCE_KEY), "no Application log source");
    assert!(
        !exists(&paths.product_root),
        "nothing under {}",
        paths.product_root.display()
    );
}

/// The per-user program folder of the account the test runs as, where a
/// per-user context puts `ProgramFiles64Folder`.
fn per_user_program_files() -> PathBuf {
    PathBuf::from(std::env::var_os("LOCALAPPDATA").expect("%LOCALAPPDATA%")).join("Programs")
}

/// The Amendment 2026-11-09, item 4: a direct `msiexec` installation, without
/// the property the setup's chain passes, is refused by the launch condition
/// and installs nothing.
#[test]
fn package_msi_a_direct_installation_is_refused() {
    let mut host = Host::take("direct");
    let msi = input("CG_TEST_MSI");
    let (code, log) = host.msiexec("install", "/i", &msi, &[]);
    assert_eq!(
        code,
        ERROR_INSTALL_FAILURE,
        "a direct installation fails\n{}",
        tail(&log)
    );
    assert!(
        log.contains(DIRECT_REFUSAL),
        "the launch condition refused it\n{}",
        tail(&log)
    );
    assert_nothing_installed(&host.paths, &per_user_program_files());
}

/// install_ac_001, "per-machine": an installation in a per-user context,
/// which would put the binary of a LocalSystem service in the user's own
/// profile, is refused and installs nothing, with the setup's property too.
#[test]
fn package_msi_a_per_user_installation_is_refused() {
    let mut host = Host::take("per-user");
    let msi = input("CG_TEST_MSI");
    let cases: [(&str, &[&str]); 2] = [
        ("allusers-2", &["ALLUSERS=2", "MSIINSTALLPERUSER=1"]),
        ("allusers-empty", &[r#"ALLUSERS="""#]),
    ];
    for (name, properties) in cases {
        let properties: Vec<&str> = std::iter::once("CG_SETUP_CHAIN=1")
            .chain(properties.iter().copied())
            .collect();
        let (code, log) = host.msiexec(name, "/i", &msi, &properties);
        assert_eq!(
            code,
            ERROR_INSTALL_FAILURE,
            "{name}: a per-user installation fails\n{}",
            tail(&log)
        );
        assert!(
            log.contains(PER_MACHINE_ONLY),
            "{name}: the launch condition refused it\n{}",
            tail(&log)
        );
        assert_nothing_installed(&host.paths, &per_user_program_files());
    }
}

/// The Amendment 2026-11-09, item 4: a direct `msiexec` major upgrade of a
/// product the setup installed is refused and leaves it as it was.
#[test]
fn package_msi_a_direct_major_upgrade_is_refused() {
    let mut host = Host::take("direct-upgrade");
    let msi = input("CG_TEST_MSI");
    let next = input("CG_TEST_MSI_NEXT");
    let version = release_version();
    host.install_as_the_setup("install", &msi);
    let installed = std::fs::read(&host.paths.binary).expect("the installed binary");

    let (code, log) = host.msiexec("upgrade", "/i", &next, &[]);
    assert_eq!(
        code,
        ERROR_INSTALL_FAILURE,
        "a direct major upgrade fails\n{}",
        tail(&log)
    );
    assert!(
        log.contains(DIRECT_REFUSAL),
        "the launch condition refused it\n{}",
        tail(&log)
    );
    assert_eq!(
        registered_versions(),
        [version],
        "the product the setup installed stays, alone"
    );
    assert_eq!(
        std::fs::read(&host.paths.binary).expect("the installed binary"),
        installed,
        "its binary is unchanged"
    );
    let manager = Manager::open().expect("the SCM, for connecting");
    assert_eq!(
        scm::check_product_service(
            &manager,
            PRODUCT_SERVICE,
            &host.paths.binary,
            &host.paths.config,
            Route::Other,
        ),
        Ok(Product::Present { running: false }),
        "its service stays, the product's"
    );
    drop(manager);
    host.uninstall("uninstall", &msi);
}

/// install_ac_001 and §Data contracts §1 to §3, with the Amendment
/// 2026-11-09, item 4: the MSI as the setup installs it, then its default
/// uninstallation.
#[test]
fn package_msi_installs_the_product_service_and_binary_and_uninstalls_them() {
    let mut host = Host::take("install");
    let msi = input("CG_TEST_MSI");
    let raw = input("CG_TEST_AGENT_EXE");
    let version = release_version();
    host.install_as_the_setup("install", &msi);
    let paths = &host.paths;

    // Per machine, at the release version.
    assert!(
        key_exists(UPGRADE_CODE_KEY),
        "the upgrade family is registered per machine"
    );
    assert_eq!(
        registered_versions(),
        [version],
        "one program, this version"
    );

    // The binary: the raw executable, whole, at the fixed path, of this
    // package's version as configure checks it (the Amendment 2026-11-10,
    // item 8), owned by SYSTEM or Administrators with its directories.
    assert_eq!(
        std::fs::read(&paths.binary).expect("the installed binary"),
        std::fs::read(&raw).expect("the raw executable"),
        "the MSI carries the raw executable, byte for byte"
    );
    let numeric = cg_setup::version::numeric(version).expect("X.Y.Z");
    assert_eq!(
        cg_setup::version::configure_version(&paths.binary, numeric, version),
        Ok(()),
        "the installed binary is this package's version"
    );
    for path in [&paths.binary, &paths.agent_dir, &paths.program_files_root] {
        let owner = owner_of(path);
        assert!(
            owner == "SY" || owner == "BA",
            "{} is owned by SYSTEM or Administrators, not {owner}",
            path.display()
        );
    }

    // The service (§Data contracts §3).
    // READ_CONTROL, to read its DACL below.
    let service = open_service(
        PRODUCT_SERVICE,
        SERVICE_QUERY_CONFIG | SERVICE_QUERY_STATUS | READ_CONTROL,
    )
    .unwrap_or_else(|error| panic!("open {PRODUCT_SERVICE}: Win32 error {error}"));
    let config = service_config(&service);
    let expected = format!(
        "\"{}\" --service --config \"{}\"",
        paths.binary.display(),
        paths.config.display()
    );
    assert_eq!(
        (
            config.service_type,
            config.start_type,
            config.account.as_str(),
            config.command_line.as_str(),
            config.display_name.as_str(),
        ),
        (
            SERVICE_WIN32_OWN_PROCESS,
            SERVICE_AUTO_START,
            "LocalSystem",
            expected.as_str(),
            "CyberGuard Agent",
        ),
        "own process, automatic, LocalSystem, the quoted binary and configuration, \
         its display name"
    );
    let manager = Manager::open().expect("the SCM, for connecting");
    assert_eq!(
        scm::check_product_service(
            &manager,
            PRODUCT_SERVICE,
            &paths.binary,
            &paths.config,
            Route::Other,
        ),
        Ok(Product::Present { running: false }),
        "the setup reads it as the product service, not a conflicting one"
    );
    assert_eq!(
        service_state(&service),
        SERVICE_STOPPED,
        "the MSI does not start it; configure does"
    );
    assert_eq!(
        failure_actions(&service),
        0,
        "the MSI sets no failure action; configure does"
    );

    // Its access (§Data contracts §3): only SYSTEM and Administrators may
    // change, start, stop or delete it; ordinary users may query its status.
    // The effect on an ordinary user's tokens comes first, then the DACL.
    let dacl = service_dacl(&service);
    eprintln!("the product service's DACL: {dacl}");
    drop(service);
    let user = TestUser::create();
    let filtered = filtered_administrator_token();
    let mut problems =
        ordinary_access_problems(&user.token, &format!("standard user {}", user.name), paths);
    problems.extend(ordinary_access_problems(
        &filtered,
        "filtered administrator",
        paths,
    ));
    assert!(
        problems.is_empty(),
        "an ordinary user's access:\n{}",
        problems.join("\n")
    );
    // The positive control: the runner's Administrator may start it.
    assert!(
        open_service(PRODUCT_SERVICE, SERVICE_START).is_ok(),
        "an elevated Administrator may start it"
    );
    drop(user);
    drop(filtered);
    let mut trustees = control_trustees(&dacl);
    trustees.sort();
    trustees.dedup();
    assert!(
        !trustees.is_empty()
            && trustees
                .iter()
                .all(|trustee| trustee == "SY" || trustee == "BA"),
        "only SYSTEM and Administrators are allowed to control it: {dacl}"
    );
    assert_eq!(
        dacl, SERVICE_DACL,
        "the DACL of SPEC-024 Amendment 2026-11-12, point 2"
    );

    // The Application log's source names the installed binary, whose message
    // table renders the service's entries (§Operational §3; the Amendment
    // 2026-11-09, item 4).
    assert_eq!(
        registry_value(EVENT_SOURCE_KEY, "EventMessageFile"),
        Some(Value::Text(paths.binary.display().to_string())),
        "the source's message file is the installed binary"
    );
    assert_eq!(
        registry_value(EVENT_SOURCE_KEY, "TypesSupported"),
        Some(Value::Number(7)),
        "errors, warnings and information"
    );
    let insert = "cg-agent 1.2.3 probe";
    for id in 1000..=1003 {
        assert_eq!(
            message_of(&paths.binary, id, insert).as_deref(),
            Some(insert),
            "entry {id} through the registered message file"
        );
    }

    // Nothing under %ProgramData%\CyberGuard (the Amendment 2026-11-09, item 4).
    assert!(
        !exists(&paths.product_root),
        "the MSI creates nothing under {}",
        paths.product_root.display()
    );

    // The default uninstallation removes the service, the binary, its
    // directories and the source, and leaves %ProgramData%\CyberGuard
    // (§Operational §4).
    let kept = paths.product_root.join("agent").join("kept.txt");
    std::fs::create_dir_all(kept.parent().unwrap()).expect("a data root of the test's");
    std::fs::write(&kept, b"kept").expect("a file in it");
    host.uninstall("uninstall", &msi);
    let paths = &host.paths;
    assert_eq!(product_service(&manager), Product::Absent, "no service");
    for path in [&paths.binary, &paths.agent_dir, &paths.program_files_root] {
        assert!(!exists(path), "{} is removed", path.display());
    }
    assert!(!key_exists(EVENT_SOURCE_KEY), "the source is removed");
    assert!(!key_exists(UPGRADE_CODE_KEY), "the product is unregistered");
    assert!(registered_versions().is_empty(), "no program listed");
    assert_eq!(
        std::fs::read(&kept).expect("the data root's file"),
        b"kept",
        "the uninstallation leaves {}",
        paths.product_root.display()
    );
}
