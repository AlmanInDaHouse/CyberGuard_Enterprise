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
//! user's tokens, and as the SDDL of the Amendment 2026-11-12, point 2.
#![cfg(windows)]

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use cg_setup::scm::{self, Manager, Product, Route};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_ACCESS_DENIED, GENERIC_READ, HANDLE,
    INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Security::{
    CreateRestrictedToken, CreateWellKnownSid, GetTokenInformation, ImpersonateLoggedOnUser,
    LogonUserW, RevertToSelf, TokenGroups, WinBuiltinAdministratorsSid, DACL_SECURITY_INFORMATION,
    DISABLE_MAX_PRIVILEGE, LOGON32_LOGON_INTERACTIVE, LOGON32_PROVIDER_DEFAULT,
    PSECURITY_DESCRIPTOR, SECURITY_MAX_SID_SIZE, SID_AND_ATTRIBUTES, TOKEN_ASSIGN_PRIMARY,
    TOKEN_DUPLICATE, TOKEN_GROUPS, TOKEN_IMPERSONATE, TOKEN_QUERY,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, DELETE, FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_APPEND_DATA, FILE_DELETE_CHILD,
    FILE_EXECUTE, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA,
    FILE_WRITE_EA, OPEN_EXISTING, READ_CONTROL, WRITE_DAC, WRITE_OWNER,
};
use windows_sys::Win32::System::Services::{
    CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceConfig2W, QueryServiceConfigW,
    QueryServiceObjectSecurity, QueryServiceStatus, QUERY_SERVICE_CONFIGW, SC_HANDLE,
    SC_MANAGER_CONNECT, SERVICE_AUTO_START, SERVICE_CHANGE_CONFIG, SERVICE_CONFIG_FAILURE_ACTIONS,
    SERVICE_FAILURE_ACTIONSW, SERVICE_PAUSE_CONTINUE, SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS,
    SERVICE_START, SERVICE_STATUS, SERVICE_STOP, SERVICE_STOPPED, SERVICE_USER_DEFINED_CONTROL,
    SERVICE_WIN32_OWN_PROCESS,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// The product service (§Data contracts §3).
const PRODUCT_SERVICE: &str = "CyberGuardAgent";

/// The MSI's product name, as Windows lists installed programs.
const PRODUCT_NAME: &str = "CyberGuard Agent";

/// The upgrade family's code, `AF9D0A74-7206-49F9-B391-645F1C1F05AD`, in the
/// packed form Windows Installer registers a per-machine product under.
const UPGRADE_CODE_KEY: &str =
    r"SOFTWARE\Classes\Installer\UpgradeCodes\47A0D9FA60279F943B1946F5C1F150DA";

/// The Application log's source (§Operational §3).
const EVENT_SOURCE_KEY: &str =
    r"SYSTEM\CurrentControlSet\Services\EventLog\Application\CyberGuardAgent";

/// Where Windows lists installed programs, machine-wide.
const UNINSTALL_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";

/// The product service's DACL (SPEC-024 Amendment 2026-11-12, point 2):
/// Windows' default without user-defined control for interactive and
/// service users.
const SERVICE_DACL: &str = "D:(A;;CCLCSWRPWPDTLOCRRC;;;SY)(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA)\
                            (A;;CCLCSWLORC;;;IU)(A;;CCLCSWLORC;;;SU)";

/// The launch condition's message, from the MSI's source.
const DIRECT_REFUSAL: &str = "CyberGuard Agent is installed by its setup";

/// A group that is in a token for deny only (`TOKEN_GROUPS`).
const SE_GROUP_USE_FOR_DENY_ONLY: u32 = 0x10;

/// What `msiexec` exits with when an installation fails, a false launch
/// condition included.
const ERROR_INSTALL_FAILURE: i32 = 1603;

/// What `msiexec` exits with while another installation is in progress.
const ERROR_INSTALL_ALREADY_RUNNING: i32 = 1618;

/// The second launch condition's message, from the MSI's source.
const PER_MACHINE_ONLY: &str = "CyberGuard Agent is installed per machine only";

// --- Inputs, paths and the guard -----------------------------------------------------

fn input(name: &str) -> PathBuf {
    let path = PathBuf::from(
        std::env::var_os(name).unwrap_or_else(|| panic!("{name} names the job's input")),
    );
    assert!(path.is_file(), "{name}: {} is a file", path.display());
    path
}

/// The release version this test and the MSI were built with.
fn release_version() -> &'static str {
    let version = cg_setup::version::SETUP_VERSION;
    assert!(
        cg_setup::version::numeric(version).is_some() && !version.ends_with("-dev"),
        "the test is built with CG_RELEASE_VERSION, as the MSI is: {version}"
    );
    version
}

/// The fixed paths (§Data contracts §2), under the folders the OS reports.
struct Paths {
    program_files_root: PathBuf,
    agent_dir: PathBuf,
    binary: PathBuf,
    product_root: PathBuf,
    config: PathBuf,
}

impl Paths {
    fn machine() -> Self {
        let program_files = cg_agent::custody::program_files().expect("%ProgramFiles%");
        let program_data = cg_agent::custody::program_data().expect("%ProgramData%");
        let program_files_root = program_files.join("CyberGuard");
        let agent_dir = program_files_root.join("Agent");
        let product_root = program_data.join("CyberGuard");
        Paths {
            binary: agent_dir.join("cg-agent.exe"),
            config: product_root.join("agent").join("agent.toml"),
            program_files_root,
            agent_dir,
            product_root,
        }
    }
}

fn exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

fn product_service(manager: &Manager) -> Product {
    scm::check_product_service(
        manager,
        PRODUCT_SERVICE,
        Path::new(r"C:\absent.exe"),
        Path::new(r"C:\absent.toml"),
        Route::Other,
    )
    .unwrap_or_else(|refusal| panic!("{PRODUCT_SERVICE}: {refusal:?}"))
}

/// Fail without acting unless this is a GitHub-hosted runner with no product
/// service, no product directory and no product of the upgrade family.
fn refuse_unless_disposable_runner(paths: &Paths) {
    let actions = std::env::var("GITHUB_ACTIONS").ok();
    let environment = std::env::var("RUNNER_ENVIRONMENT").ok();
    assert!(
        actions.as_deref() == Some("true") && environment.as_deref() == Some("github-hosted"),
        "refusing to install the MSI outside a GitHub-hosted runner \
         (GITHUB_ACTIONS={actions:?}, RUNNER_ENVIRONMENT={environment:?})"
    );
    let manager = Manager::open().expect("the SCM, for connecting");
    assert_eq!(
        scm::check_product_service(
            &manager,
            PRODUCT_SERVICE,
            Path::new(r"C:\absent.exe"),
            Path::new(r"C:\absent.toml"),
            Route::Other,
        ),
        Ok(Product::Absent),
        "refusing to act: {PRODUCT_SERVICE} is not established absent"
    );
    for directory in [&paths.product_root, &paths.program_files_root] {
        assert!(
            !exists(directory),
            "refusing to act: {} exists on this host",
            directory.display()
        );
    }
    assert!(
        !key_exists(UPGRADE_CODE_KEY),
        "refusing to act: a product of the MSI's upgrade family is registered"
    );
}

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
        refuse_unless_disposable_runner(&paths);
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

// --- Win32 helpers -------------------------------------------------------------------

fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(Some(0)).collect()
}

fn last_error() -> u32 {
    // SAFETY: reads the calling thread's last-error value.
    unsafe { GetLastError() }
}

/// A NUL-terminated wide string Windows wrote, as a `String`.
///
/// # Safety
/// `text` is null or points to a NUL-terminated UTF-16 string.
unsafe fn from_wide_nul(text: *const u16) -> String {
    if text.is_null() {
        return String::new();
    }
    let length = (0..).take_while(|&i| *text.add(i) != 0).count();
    String::from_utf16_lossy(std::slice::from_raw_parts(text, length))
}

// --- The registry --------------------------------------------------------------------

/// A value of an `HKEY_LOCAL_MACHINE` key, in the 64-bit view, unexpanded.
#[derive(Debug, PartialEq, Eq)]
enum Value {
    Text(String),
    Number(u32),
}

fn registry_value(key: &str, name: &str) -> Option<Value> {
    use windows_sys::Win32::System::Registry::{
        RegGetValueW, HKEY_LOCAL_MACHINE, REG_DWORD, REG_EXPAND_SZ, REG_SZ, RRF_NOEXPAND,
        RRF_RT_ANY, RRF_SUBKEY_WOW6464KEY,
    };
    let (key, name) = (wide(OsStr::new(key)), wide(OsStr::new(name)));
    let mut kind = 0u32;
    let mut buffer = vec![0u8; 4096];
    let mut size = buffer.len() as u32;
    // SAFETY: NUL-terminated names and a buffer of `size` bytes.
    let error = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_ANY | RRF_NOEXPAND | RRF_SUBKEY_WOW6464KEY,
            &mut kind,
            buffer.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if error != 0 {
        return None;
    }
    buffer.truncate(size as usize);
    match kind {
        REG_SZ | REG_EXPAND_SZ => {
            let units: Vec<u16> = buffer
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .take_while(|&unit| unit != 0)
                .collect();
            Some(Value::Text(String::from_utf16_lossy(&units)))
        }
        REG_DWORD => Some(Value::Number(u32::from_le_bytes(
            buffer[..4].try_into().expect("a DWORD"),
        ))),
        other => panic!("{name:?}: registry type {other}"),
    }
}

/// An `HKEY_LOCAL_MACHINE` key, opened in the 64-bit view, closed on drop.
struct Key(windows_sys::Win32::System::Registry::HKEY);

impl Key {
    fn open(key: &str) -> Option<Key> {
        use windows_sys::Win32::System::Registry::{
            RegOpenKeyExW, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY,
        };
        let key = wide(OsStr::new(key));
        let mut handle = std::ptr::null_mut();
        // SAFETY: a NUL-terminated name and an out-value, owned below.
        let error = unsafe {
            RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                key.as_ptr(),
                0,
                KEY_READ | KEY_WOW64_64KEY,
                &mut handle,
            )
        };
        (error == 0).then_some(Key(handle))
    }

    /// The names of its subkeys.
    fn subkeys(&self) -> Vec<String> {
        use windows_sys::Win32::System::Registry::RegEnumKeyExW;
        let mut names = Vec::new();
        for index in 0.. {
            let mut name = [0u16; 256];
            let mut length = name.len() as u32;
            // SAFETY: a buffer of `length` units; the rest are not asked for.
            let error = unsafe {
                RegEnumKeyExW(
                    self.0,
                    index,
                    name.as_mut_ptr(),
                    &mut length,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            if error != 0 {
                break;
            }
            names.push(String::from_utf16_lossy(&name[..length as usize]));
        }
        names
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: a key this value owns, closed once.
        unsafe { windows_sys::Win32::System::Registry::RegCloseKey(self.0) };
    }
}

fn key_exists(key: &str) -> bool {
    Key::open(key).is_some()
}

/// The `DisplayVersion` of each machine-wide installed program named
/// `PRODUCT_NAME`.
fn registered_versions() -> Vec<String> {
    let Some(uninstall) = Key::open(UNINSTALL_KEY) else {
        return Vec::new();
    };
    uninstall
        .subkeys()
        .into_iter()
        .map(|subkey| format!(r"{UNINSTALL_KEY}\{subkey}"))
        .filter(|key| {
            registry_value(key, "DisplayName") == Some(Value::Text(PRODUCT_NAME.to_string()))
        })
        .map(|key| match registry_value(&key, "DisplayVersion") {
            Some(Value::Text(version)) => version,
            other => panic!("{key}: DisplayVersion {other:?}"),
        })
        .collect()
}

// --- The service ---------------------------------------------------------------------

/// A service handle, closed on drop.
struct ServiceHandle(SC_HANDLE);

impl Drop for ServiceHandle {
    fn drop(&mut self) {
        // SAFETY: a handle this value owns, closed once.
        unsafe { CloseServiceHandle(self.0) };
    }
}

/// The SCM, then the service `name`, opened with `access` by whoever the
/// calling thread is: the Win32 error of the open, or the handle.
fn open_service(name: &str, access: u32) -> Result<ServiceHandle, u32> {
    // SAFETY: null machine and database name the local active database.
    let manager = unsafe { OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT) };
    assert!(
        !manager.is_null(),
        "OpenSCManagerW: Win32 error {}",
        last_error()
    );
    let manager = ServiceHandle(manager);
    let name = wide(OsStr::new(name));
    // SAFETY: an open manager and a NUL-terminated name.
    let service = unsafe { OpenServiceW(manager.0, name.as_ptr(), access) };
    if service.is_null() {
        return Err(last_error());
    }
    Ok(ServiceHandle(service))
}

/// What `QueryServiceConfigW` reports of a service.
#[derive(Debug)]
struct ServiceConfig {
    service_type: u32,
    start_type: u32,
    command_line: String,
    account: String,
    display_name: String,
}

fn service_config(service: &ServiceHandle) -> ServiceConfig {
    let mut needed = 0u32;
    // SAFETY: a size query.
    unsafe { QueryServiceConfigW(service.0, std::ptr::null_mut(), 0, &mut needed) };
    let mut buffer = vec![0u64; (needed as usize).div_ceil(8).max(1)];
    // SAFETY: a buffer of at least `needed` bytes, 8-byte aligned.
    let ok = unsafe {
        QueryServiceConfigW(
            service.0,
            buffer.as_mut_ptr().cast(),
            (buffer.len() * 8) as u32,
            &mut needed,
        )
    };
    assert_ne!(ok, 0, "QueryServiceConfigW: Win32 error {}", last_error());
    // SAFETY: the call wrote a QUERY_SERVICE_CONFIGW whose strings lie in the
    // buffer.
    unsafe {
        let config = &*buffer.as_ptr().cast::<QUERY_SERVICE_CONFIGW>();
        ServiceConfig {
            service_type: config.dwServiceType,
            start_type: config.dwStartType,
            command_line: from_wide_nul(config.lpBinaryPathName),
            account: from_wide_nul(config.lpServiceStartName),
            display_name: from_wide_nul(config.lpDisplayName),
        }
    }
}

/// How many failure actions the service has.
fn failure_actions(service: &ServiceHandle) -> u32 {
    let mut needed = 0u32;
    // SAFETY: a size query.
    unsafe {
        QueryServiceConfig2W(
            service.0,
            SERVICE_CONFIG_FAILURE_ACTIONS,
            std::ptr::null_mut(),
            0,
            &mut needed,
        )
    };
    let mut buffer = vec![0u64; (needed as usize).div_ceil(8).max(1)];
    // SAFETY: a buffer of at least `needed` bytes, 8-byte aligned.
    let ok = unsafe {
        QueryServiceConfig2W(
            service.0,
            SERVICE_CONFIG_FAILURE_ACTIONS,
            buffer.as_mut_ptr().cast(),
            (buffer.len() * 8) as u32,
            &mut needed,
        )
    };
    assert_ne!(ok, 0, "QueryServiceConfig2W: Win32 error {}", last_error());
    // SAFETY: the call wrote a SERVICE_FAILURE_ACTIONSW.
    unsafe { (*buffer.as_ptr().cast::<SERVICE_FAILURE_ACTIONSW>()).cActions }
}

fn service_state(service: &ServiceHandle) -> u32 {
    // SAFETY: an all-zero SERVICE_STATUS is valid.
    let mut status: SERVICE_STATUS = unsafe { std::mem::zeroed() };
    // SAFETY: the out-buffer is one SERVICE_STATUS.
    let ok = unsafe { QueryServiceStatus(service.0, &mut status) };
    assert_ne!(ok, 0, "QueryServiceStatus: Win32 error {}", last_error());
    status.dwCurrentState
}

/// The service's DACL, as SDDL.
fn service_dacl(service: &ServiceHandle) -> String {
    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, SDDL_REVISION_1,
    };
    let mut needed = 0u32;
    // SAFETY: a size query.
    unsafe {
        QueryServiceObjectSecurity(
            service.0,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            0,
            &mut needed,
        )
    };
    let mut buffer = vec![0u64; (needed as usize).div_ceil(8).max(1)];
    // SAFETY: a buffer of at least `needed` bytes, 8-byte aligned.
    let ok = unsafe {
        QueryServiceObjectSecurity(
            service.0,
            DACL_SECURITY_INFORMATION,
            buffer.as_mut_ptr().cast(),
            (buffer.len() * 8) as u32,
            &mut needed,
        )
    };
    assert_ne!(
        ok,
        0,
        "QueryServiceObjectSecurity: Win32 error {}",
        last_error()
    );
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: a self-relative descriptor in the buffer; `text` is freed below.
    let ok = unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            buffer.as_ptr() as PSECURITY_DESCRIPTOR,
            SDDL_REVISION_1,
            DACL_SECURITY_INFORMATION,
            &mut text,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(
        ok,
        0,
        "ConvertSecurityDescriptorToStringSecurityDescriptorW: Win32 error {}",
        last_error()
    );
    // SAFETY: a NUL-terminated string from the call, read then freed once.
    unsafe {
        let sddl = from_wide_nul(text);
        LocalFree(text.cast());
        sddl
    }
}

/// The service rights that change, start, stop, pause, control or delete
/// it, or its security: §Data contracts §3's "change, start, stop or delete",
/// SDDL's names for them.
const CONTROL_RIGHTS: [(&str, u32); 8] = [
    ("SERVICE_CHANGE_CONFIG", SERVICE_CHANGE_CONFIG),
    ("SERVICE_START", SERVICE_START),
    ("SERVICE_STOP", SERVICE_STOP),
    ("SERVICE_PAUSE_CONTINUE", SERVICE_PAUSE_CONTINUE),
    ("SERVICE_USER_DEFINED_CONTROL", SERVICE_USER_DEFINED_CONTROL),
    ("DELETE", DELETE),
    ("WRITE_DAC", WRITE_DAC),
    ("WRITE_OWNER", WRITE_OWNER),
];

/// The SDDL right names that grant one of `CONTROL_RIGHTS`, or all of them.
const CONTROL_RIGHT_NAMES: [&str; 11] = [
    "DC", "RP", "WP", "DT", "CR", "SD", "WD", "WO", "GA", "GW", "GX",
];

/// §Data contracts §3: of the DACL's entries, those that allow one of
/// `CONTROL_RIGHTS` name SYSTEM or Administrators alone. An entry's rights
/// in hexadecimal are read as granting them all.
fn control_trustees(dacl: &str) -> Vec<String> {
    let list = &dacl[dacl.find('(').unwrap_or(dacl.len())..];
    let mut found = Vec::new();
    for ace in list.split_inclusive(')') {
        let fields: Vec<&str> = ace
            .trim_matches(|c| c == '(' || c == ')')
            .split(';')
            .collect();
        assert_eq!(fields.len(), 6, "an entry of six fields: {ace} in {dacl}");
        if fields[0] != "A" {
            continue;
        }
        let rights = fields[2];
        let grants = rights.starts_with("0x")
            || rights
                .as_bytes()
                .chunks(2)
                .any(|name| CONTROL_RIGHT_NAMES.contains(&std::str::from_utf8(name).unwrap()));
        if grants {
            found.push(fields[5].to_string());
        }
    }
    found
}

// --- Tokens and their access ---------------------------------------------------------

/// A token handle, closed on drop.
struct Token(HANDLE);

impl Drop for Token {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: a token handle this value owns, closed once.
            unsafe { CloseHandle(self.0) };
        }
    }
}

/// A standard local user this test created (`USER_PRIV_USER`, a member of
/// Users only), with a token from `LogonUserW`; `Drop` deletes the account.
struct TestUser {
    name: String,
    token: Token,
}

impl TestUser {
    fn create() -> Self {
        use windows_sys::Win32::NetworkManagement::NetManagement::{
            NERR_Success, NetUserAdd, UF_DONT_EXPIRE_PASSWD, UF_SCRIPT, USER_INFO_1, USER_PRIV_USER,
        };
        let random = uuid::Uuid::now_v7().simple().to_string();
        let name = format!("cgm{}", &random[random.len() - 8..]);
        // Upper and lower case, digits and symbols, none of the name.
        let password = format!("Kp-{}-xQ7!", &random[..16]);
        let (mut wname, mut wpassword) = (wide(OsStr::new(&name)), wide(OsStr::new(&password)));
        let info = USER_INFO_1 {
            usri1_name: wname.as_mut_ptr(),
            usri1_password: wpassword.as_mut_ptr(),
            usri1_password_age: 0,
            usri1_priv: USER_PRIV_USER,
            usri1_home_dir: std::ptr::null_mut(),
            usri1_comment: std::ptr::null_mut(),
            usri1_flags: UF_SCRIPT | UF_DONT_EXPIRE_PASSWD,
            usri1_script_path: std::ptr::null_mut(),
        };
        let mut parameter = 0u32;
        // SAFETY: level 1 with one USER_INFO_1 whose strings live for the call.
        let status = unsafe {
            NetUserAdd(
                std::ptr::null(),
                1,
                (&info as *const USER_INFO_1).cast(),
                &mut parameter,
            )
        };
        assert_eq!(
            status, NERR_Success,
            "NetUserAdd {name}: status {status}, parameter {parameter}"
        );
        // The guard exists before the logon: a failure below deletes it.
        let mut user = TestUser {
            name,
            token: Token(std::ptr::null_mut()),
        };
        // An interactive logon, so that the token holds INTERACTIVE, which
        // the product service's query entry names; no fallback to another type.
        let domain = wide(OsStr::new("."));
        let mut token: HANDLE = std::ptr::null_mut();
        // SAFETY: the strings live for the call; `token` is owned below.
        let ok = unsafe {
            LogonUserW(
                wname.as_ptr(),
                domain.as_ptr(),
                wpassword.as_ptr(),
                LOGON32_LOGON_INTERACTIVE,
                LOGON32_PROVIDER_DEFAULT,
                &mut token,
            )
        };
        assert_ne!(
            ok,
            0,
            "LogonUserW {} (interactive): Win32 error {}",
            user.name,
            last_error()
        );
        user.token = Token(token);
        let groups = groups(&user.token);
        eprintln!("the standard user {}'s groups: {groups:?}", user.name);
        assert!(
            groups.iter().any(|(sid, _)| sid == INTERACTIVE),
            "the standard user's token holds INTERACTIVE"
        );
        assert!(
            !groups.iter().any(|(sid, _)| sid == ADMINISTRATORS),
            "the standard user is no administrator"
        );
        user
    }
}

impl Drop for TestUser {
    fn drop(&mut self) {
        use windows_sys::Win32::NetworkManagement::NetManagement::{NERR_Success, NetUserDel};
        drop(std::mem::replace(
            &mut self.token,
            Token(std::ptr::null_mut()),
        ));
        let name = wide(OsStr::new(&self.name));
        // SAFETY: the name lives for the call.
        let status = unsafe { NetUserDel(std::ptr::null(), name.as_ptr()) };
        if status != NERR_Success {
            eprintln!("NetUserDel {}: status {status}", self.name);
        }
    }
}

/// This process's token as UAC's filtered administrator token would be: no
/// privilege but `SeChangeNotifyPrivilege` (`DISABLE_MAX_PRIVILEGE`) and
/// Administrators for deny only. The runner runs as the built-in
/// Administrator, which has no linked filtered token, so the test builds one.
fn filtered_administrator_token() -> Token {
    let mut process: HANDLE = std::ptr::null_mut();
    // SAFETY: the current-process pseudo-handle; `process` is owned below.
    let opened = unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_DUPLICATE | TOKEN_QUERY | TOKEN_ASSIGN_PRIMARY | TOKEN_IMPERSONATE,
            &mut process,
        )
    };
    assert_ne!(opened, 0, "OpenProcessToken: Win32 error {}", last_error());
    let process = Token(process);
    let mut administrators = vec![0u8; SECURITY_MAX_SID_SIZE as usize];
    let mut size = SECURITY_MAX_SID_SIZE;
    // SAFETY: a buffer of SECURITY_MAX_SID_SIZE bytes.
    let ok = unsafe {
        CreateWellKnownSid(
            WinBuiltinAdministratorsSid,
            std::ptr::null_mut(),
            administrators.as_mut_ptr().cast(),
            &mut size,
        )
    };
    assert_ne!(ok, 0, "CreateWellKnownSid: Win32 error {}", last_error());
    let deny_only = SID_AND_ATTRIBUTES {
        Sid: administrators.as_mut_ptr().cast(),
        Attributes: 0,
    };
    let mut restricted: HANDLE = std::ptr::null_mut();
    // SAFETY: one SID to disable, living for the call; `restricted` is owned
    // below.
    let ok = unsafe {
        CreateRestrictedToken(
            process.0,
            DISABLE_MAX_PRIVILEGE,
            1,
            &deny_only,
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
            &mut restricted,
        )
    };
    assert_ne!(ok, 0, "CreateRestrictedToken: Win32 error {}", last_error());
    let restricted = Token(restricted);
    let groups = groups(&restricted);
    eprintln!("the filtered administrator's groups: {groups:?}");
    assert!(
        groups.iter().any(|(sid, attributes)| sid == ADMINISTRATORS
            && attributes & SE_GROUP_USE_FOR_DENY_ONLY != 0),
        "Administrators is for deny only in the filtered token"
    );
    assert!(
        groups
            .iter()
            .any(|(sid, attributes)| (sid == INTERACTIVE || sid == SERVICE)
                && attributes & SE_GROUP_USE_FOR_DENY_ONLY == 0),
        "the filtered token holds INTERACTIVE or SERVICE, which the product service's \
         query entries name, so that its positive control means something"
    );
    restricted
}

/// The well-known SIDs the test reads in a token's groups.
const ADMINISTRATORS: &str = "S-1-5-32-544";
const INTERACTIVE: &str = "S-1-5-4";
const SERVICE: &str = "S-1-5-6";

/// `token`'s groups, each SID in full form with its attributes.
fn groups(token: &Token) -> Vec<(String, u32)> {
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    let mut needed = 0u32;
    // SAFETY: a size query.
    unsafe { GetTokenInformation(token.0, TokenGroups, std::ptr::null_mut(), 0, &mut needed) };
    let mut buffer = vec![0u64; (needed as usize).div_ceil(8).max(1)];
    // SAFETY: a buffer of at least `needed` bytes, 8-byte aligned.
    let ok = unsafe {
        GetTokenInformation(
            token.0,
            TokenGroups,
            buffer.as_mut_ptr().cast(),
            (buffer.len() * 8) as u32,
            &mut needed,
        )
    };
    assert_ne!(ok, 0, "GetTokenInformation: Win32 error {}", last_error());
    // SAFETY: the call wrote a TOKEN_GROUPS and its GroupCount entries; each
    // string the conversion allocates is freed once.
    unsafe {
        let groups = buffer.as_ptr().cast::<TOKEN_GROUPS>();
        let first = std::ptr::addr_of!((*groups).Groups).cast::<SID_AND_ATTRIBUTES>();
        (0..(*groups).GroupCount as usize)
            .map(|i| *first.add(i))
            .map(|group| {
                let mut text: *mut u16 = std::ptr::null_mut();
                assert_ne!(
                    ConvertSidToStringSidW(group.Sid, &mut text),
                    0,
                    "ConvertSidToStringSidW: Win32 error {}",
                    last_error()
                );
                let sid = from_wide_nul(text);
                LocalFree(text.cast());
                (sid, group.Attributes)
            })
            .collect()
    }
}

/// `action`'s value, run on a thread of its own under `token`'s
/// impersonation.
fn as_token<T: Send + 'static>(token: &Token, action: impl FnOnce() -> T + Send + 'static) -> T {
    let raw = token.0 as usize;
    std::thread::spawn(move || {
        // SAFETY: a token handle the caller keeps open for the call.
        let impersonating = unsafe { ImpersonateLoggedOnUser(raw as HANDLE) };
        assert_ne!(
            impersonating,
            0,
            "ImpersonateLoggedOnUser: Win32 error {}",
            last_error()
        );
        let value = action();
        // SAFETY: ends this thread's impersonation.
        let reverted = unsafe { RevertToSelf() };
        assert_ne!(reverted, 0, "RevertToSelf: Win32 error {}", last_error());
        value
    })
    .join()
    .expect("the impersonating thread")
}

/// Open `path` with `access`, without following a reparse point and with
/// backup semantics, which grant nothing to a token without the privileges:
/// the Win32 error, or nothing when the access is granted.
fn open_file(path: &Path, access: u32) -> Result<(), u32> {
    let name = wide(path.as_os_str());
    // SAFETY: the name lives for the call; the handle is closed below.
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(last_error());
    }
    // SAFETY: the handle the call returned, closed once.
    unsafe { CloseHandle(handle) };
    Ok(())
}

/// Open the `HKEY_LOCAL_MACHINE` key `key` with `access`, in the 64-bit
/// view: the Win32 error, or nothing when the access is granted.
fn open_key(key: &str, access: u32) -> Result<(), u32> {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY,
    };
    let name = wide(OsStr::new(key));
    let mut handle = std::ptr::null_mut();
    // SAFETY: a NUL-terminated name and an out-value, closed below.
    let error = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            name.as_ptr(),
            0,
            access | KEY_WOW64_64KEY,
            &mut handle,
        )
    };
    if error != 0 {
        return Err(error);
    }
    // SAFETY: the key the call opened, closed once.
    unsafe { RegCloseKey(handle) };
    Ok(())
}

/// The service's own registry key, where the SCM keeps its configuration.
const SERVICE_KEY: &str = r"SYSTEM\CurrentControlSet\Services\CyberGuardAgent";

/// What an ordinary user may and may not do, as a list of problems: on the
/// service, query its status but not one of `CONTROL_RIGHTS`; on its
/// registry key and the Application log source's, read them but not set a
/// value, create a subkey, delete them, or change their DACL or owner; on the
/// binary, read and run it but not write, append, delete, or change its DACL
/// or owner; on its directories, list them but not add a file or a
/// directory, delete an entry, delete them, or change their DACL or owner
/// (§Data contracts §2 and §3). Each grant is a positive control: a token
/// that is denied everything cannot pass.
fn ordinary_access_problems(token: &Token, who: &str, paths: &Paths) -> Vec<String> {
    let who = who.to_string();
    let binary = paths.binary.clone();
    let directories = [paths.agent_dir.clone(), paths.program_files_root.clone()];
    as_token(token, move || {
        let mut problems = Vec::new();
        let mut expect = |what: String, result: Result<(), u32>, granted: bool| match result {
            Ok(()) if granted => {}
            Err(ERROR_ACCESS_DENIED) if !granted => {}
            Ok(()) => problems.push(format!("{who}: {what} was granted")),
            Err(error) if granted => {
                problems.push(format!("{who}: {what} was refused: Win32 error {error}"))
            }
            Err(error) => problems.push(format!(
                "{who}: {what}: Win32 error {error}, not access denied"
            )),
        };
        expect(
            "SERVICE_QUERY_STATUS on the service".into(),
            open_service(PRODUCT_SERVICE, SERVICE_QUERY_STATUS).map(drop),
            true,
        );
        for (name, right) in CONTROL_RIGHTS {
            expect(
                format!("{name} on the service"),
                open_service(PRODUCT_SERVICE, right).map(drop),
                false,
            );
        }
        {
            use windows_sys::Win32::System::Registry::{
                KEY_CREATE_SUB_KEY, KEY_READ, KEY_SET_VALUE,
            };
            for key in [SERVICE_KEY, EVENT_SOURCE_KEY] {
                expect(format!("KEY_READ on {key}"), open_key(key, KEY_READ), true);
                for (name, right) in [
                    ("KEY_SET_VALUE", KEY_SET_VALUE),
                    ("KEY_CREATE_SUB_KEY", KEY_CREATE_SUB_KEY),
                    ("DELETE", DELETE),
                    ("WRITE_DAC", WRITE_DAC),
                    ("WRITE_OWNER", WRITE_OWNER),
                ] {
                    expect(format!("{name} on {key}"), open_key(key, right), false);
                }
            }
        }
        expect(
            format!("GENERIC_READ and FILE_EXECUTE on {}", binary.display()),
            open_file(&binary, GENERIC_READ | FILE_EXECUTE),
            true,
        );
        for (name, right) in [
            ("FILE_WRITE_DATA", FILE_WRITE_DATA),
            ("FILE_APPEND_DATA", FILE_APPEND_DATA),
            ("FILE_WRITE_ATTRIBUTES", FILE_WRITE_ATTRIBUTES),
            ("FILE_WRITE_EA", FILE_WRITE_EA),
            ("DELETE", DELETE),
            ("WRITE_DAC", WRITE_DAC),
            ("WRITE_OWNER", WRITE_OWNER),
        ] {
            expect(
                format!("{name} on {}", binary.display()),
                open_file(&binary, right),
                false,
            );
        }
        for directory in &directories {
            expect(
                format!("FILE_LIST_DIRECTORY on {}", directory.display()),
                open_file(directory, FILE_LIST_DIRECTORY),
                true,
            );
            for (name, right) in [
                ("FILE_ADD_FILE", FILE_ADD_FILE),
                ("FILE_ADD_SUBDIRECTORY", FILE_ADD_SUBDIRECTORY),
                ("FILE_DELETE_CHILD", FILE_DELETE_CHILD),
                ("DELETE", DELETE),
                ("WRITE_DAC", WRITE_DAC),
                ("WRITE_OWNER", WRITE_OWNER),
            ] {
                expect(
                    format!("{name} on {}", directory.display()),
                    open_file(directory, right),
                    false,
                );
            }
        }
        problems
    })
}

/// The text of message `id` of the module `path`, with `insert` as its one
/// insertion string, as the Event Viewer renders an event of a source whose
/// message file it is; `None` when the module has no such message.
fn message_of(path: &Path, id: u32, insert: &str) -> Option<String> {
    use windows_sys::Win32::Foundation::FreeLibrary;
    use windows_sys::Win32::System::Diagnostics::Debug::{
        FormatMessageW, FORMAT_MESSAGE_ARGUMENT_ARRAY, FORMAT_MESSAGE_FROM_HMODULE,
    };
    use windows_sys::Win32::System::LibraryLoader::{LoadLibraryExW, LOAD_LIBRARY_AS_DATAFILE};
    let name = wide(path.as_os_str());
    // SAFETY: a NUL-terminated path; a data file runs none of its code.
    let module = unsafe {
        LoadLibraryExW(
            name.as_ptr(),
            std::ptr::null_mut(),
            LOAD_LIBRARY_AS_DATAFILE,
        )
    };
    assert!(
        !module.is_null(),
        "load {} as a data file: Win32 error {}",
        path.display(),
        last_error()
    );
    let insert = wide(OsStr::new(insert));
    let arguments = [insert.as_ptr() as usize];
    let mut text = vec![0u16; 1024];
    // SAFETY: a loaded image, a buffer of the length passed, and one pointer
    // to a NUL-terminated string that outlives the call.
    let length = unsafe {
        FormatMessageW(
            FORMAT_MESSAGE_FROM_HMODULE | FORMAT_MESSAGE_ARGUMENT_ARRAY,
            module.cast_const(),
            id,
            0,
            text.as_mut_ptr(),
            text.len() as u32,
            arguments.as_ptr().cast(),
        )
    };
    // SAFETY: the module LoadLibraryExW returned, freed once.
    unsafe { FreeLibrary(module) };
    (length != 0).then(|| String::from_utf16_lossy(&text[..length as usize]))
}

// --- The tests -----------------------------------------------------------------------

/// The owner of `path`, as SDDL spells it (`SY`, `BA`, or a full SID).
fn owner_of(path: &Path) -> String {
    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetNamedSecurityInfoW,
        SDDL_REVISION_1, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::OWNER_SECURITY_INFORMATION;
    let name = wide(path.as_os_str());
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: a NUL-terminated path; the descriptor is LocalAlloc'd, freed below.
    let error = unsafe {
        GetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    assert_eq!(error, 0, "GetNamedSecurityInfoW {}", path.display());
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: the descriptor above; `text` is LocalAlloc'd, freed below.
    let ok = unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor,
            SDDL_REVISION_1,
            OWNER_SECURITY_INFORMATION,
            &mut text,
            std::ptr::null_mut(),
        )
    };
    // SAFETY: the descriptor the first call allocated, freed once.
    unsafe { LocalFree(descriptor) };
    assert_ne!(
        ok, 0,
        "ConvertSecurityDescriptorToStringSecurityDescriptorW"
    );
    // SAFETY: a NUL-terminated string from the call, read then freed once.
    let sddl = unsafe {
        let sddl = from_wide_nul(text);
        LocalFree(text.cast());
        sddl
    };
    sddl.strip_prefix("O:").unwrap_or(&sddl).to_string()
}

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
