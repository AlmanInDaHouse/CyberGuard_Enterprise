//! What the package's host tests share, `package_msi_host` and
//! `package_bundle_host`: the product's names and paths, the guard that fails
//! without acting outside a disposable GitHub-hosted runner, and readings of
//! the registry, the product service, files and tokens through Win32. Each
//! test target that includes it builds only with the test-only `host-tests`
//! feature.
#![allow(dead_code)]

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};

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
    FILE_WRITE_EA, OPEN_EXISTING, WRITE_DAC, WRITE_OWNER,
};
use windows_sys::Win32::System::Services::{
    CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceConfig2W, QueryServiceConfigW,
    QueryServiceObjectSecurity, QueryServiceStatus, QUERY_SERVICE_CONFIGW, SC_HANDLE,
    SC_MANAGER_CONNECT, SERVICE_CHANGE_CONFIG, SERVICE_CONFIG_FAILURE_ACTIONS,
    SERVICE_FAILURE_ACTIONSW, SERVICE_PAUSE_CONTINUE, SERVICE_QUERY_STATUS, SERVICE_START,
    SERVICE_STATUS, SERVICE_STOP, SERVICE_USER_DEFINED_CONTROL,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// The product service (§Data contracts §3).
pub const PRODUCT_SERVICE: &str = "CyberGuardAgent";

/// The MSI's product name, as Windows lists installed programs.
pub const PRODUCT_NAME: &str = "CyberGuard Agent";

/// The upgrade family's code, `AF9D0A74-7206-49F9-B391-645F1C1F05AD`, in the
/// packed form Windows Installer registers a per-machine product under.
pub const UPGRADE_CODE_KEY: &str =
    r"SOFTWARE\Classes\Installer\UpgradeCodes\47A0D9FA60279F943B1946F5C1F150DA";

/// The Application log's source (§Operational §3).
pub const EVENT_SOURCE_KEY: &str =
    r"SYSTEM\CurrentControlSet\Services\EventLog\Application\CyberGuardAgent";

/// Where Windows lists installed programs, machine-wide.
pub const UNINSTALL_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";

/// The product service's DACL (SPEC-024 Amendment 2026-11-12, point 2):
/// Windows' default without user-defined control for interactive and
/// service users.
pub const SERVICE_DACL: &str = "D:(A;;CCLCSWRPWPDTLOCRRC;;;SY)(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA)\
                            (A;;CCLCSWLORC;;;IU)(A;;CCLCSWLORC;;;SU)";

/// A group that is in a token for deny only (`TOKEN_GROUPS`).
pub const SE_GROUP_USE_FOR_DENY_ONLY: u32 = 0x10;

// --- Inputs, paths and the guard -----------------------------------------------------

pub fn input(name: &str) -> PathBuf {
    let path = PathBuf::from(
        std::env::var_os(name).unwrap_or_else(|| panic!("{name} names the job's input")),
    );
    assert!(path.is_file(), "{name}: {} is a file", path.display());
    path
}

/// The release version this test and the MSI were built with.
pub fn release_version() -> &'static str {
    let version = cg_setup::version::SETUP_VERSION;
    assert!(
        cg_setup::version::numeric(version).is_some() && !version.ends_with("-dev"),
        "the test is built with CG_RELEASE_VERSION, as the MSI is: {version}"
    );
    version
}

/// The fixed paths (§Data contracts §2), under the folders the OS reports.
pub struct Paths {
    pub program_files_root: PathBuf,
    pub agent_dir: PathBuf,
    pub binary: PathBuf,
    pub product_root: PathBuf,
    pub config: PathBuf,
}

impl Paths {
    pub fn machine() -> Self {
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

pub fn exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

pub fn product_service(manager: &Manager) -> Product {
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
/// service, no product directory and no product of the upgrade family;
/// `action` names what the test would do.
pub fn refuse_unless_disposable_runner(paths: &Paths, action: &str) {
    let actions = std::env::var("GITHUB_ACTIONS").ok();
    let environment = std::env::var("RUNNER_ENVIRONMENT").ok();
    assert!(
        actions.as_deref() == Some("true") && environment.as_deref() == Some("github-hosted"),
        "refusing to {action} outside a GitHub-hosted runner \
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

// --- Win32 helpers -------------------------------------------------------------------

pub fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(Some(0)).collect()
}

pub fn last_error() -> u32 {
    // SAFETY: reads the calling thread's last-error value.
    unsafe { GetLastError() }
}

/// A NUL-terminated wide string Windows wrote, as a `String`.
///
/// # Safety
/// `text` is null or points to a NUL-terminated UTF-16 string.
pub unsafe fn from_wide_nul(text: *const u16) -> String {
    if text.is_null() {
        return String::new();
    }
    let length = (0..).take_while(|&i| *text.add(i) != 0).count();
    String::from_utf16_lossy(std::slice::from_raw_parts(text, length))
}

// --- The registry --------------------------------------------------------------------

/// A value of an `HKEY_LOCAL_MACHINE` key, in the 64-bit view, unexpanded.
#[derive(Debug, PartialEq, Eq)]
pub enum Value {
    Text(String),
    Number(u32),
}

pub fn registry_value(key: &str, name: &str) -> Option<Value> {
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
pub struct Key(pub windows_sys::Win32::System::Registry::HKEY);

impl Key {
    pub fn open(key: &str) -> Option<Key> {
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
    pub fn subkeys(&self) -> Vec<String> {
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

pub fn key_exists(key: &str) -> bool {
    Key::open(key).is_some()
}

/// The `DisplayVersion` of each machine-wide installed program named
/// `PRODUCT_NAME`.
pub fn registered_versions() -> Vec<String> {
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
pub struct ServiceHandle(pub SC_HANDLE);

impl Drop for ServiceHandle {
    fn drop(&mut self) {
        // SAFETY: a handle this value owns, closed once.
        unsafe { CloseServiceHandle(self.0) };
    }
}

/// The SCM, then the service `name`, opened with `access` by whoever the
/// calling thread is: the Win32 error of the open, or the handle.
pub fn open_service(name: &str, access: u32) -> Result<ServiceHandle, u32> {
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
pub struct ServiceConfig {
    pub service_type: u32,
    pub start_type: u32,
    pub command_line: String,
    pub account: String,
    pub display_name: String,
}

pub fn service_config(service: &ServiceHandle) -> ServiceConfig {
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
pub fn failure_actions(service: &ServiceHandle) -> u32 {
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

pub fn service_state(service: &ServiceHandle) -> u32 {
    // SAFETY: an all-zero SERVICE_STATUS is valid.
    let mut status: SERVICE_STATUS = unsafe { std::mem::zeroed() };
    // SAFETY: the out-buffer is one SERVICE_STATUS.
    let ok = unsafe { QueryServiceStatus(service.0, &mut status) };
    assert_ne!(ok, 0, "QueryServiceStatus: Win32 error {}", last_error());
    status.dwCurrentState
}

/// The service's DACL, as SDDL.
pub fn service_dacl(service: &ServiceHandle) -> String {
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
pub const CONTROL_RIGHTS: [(&str, u32); 8] = [
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
pub const CONTROL_RIGHT_NAMES: [&str; 11] = [
    "DC", "RP", "WP", "DT", "CR", "SD", "WD", "WO", "GA", "GW", "GX",
];

/// §Data contracts §3: of the DACL's entries, those that allow one of
/// `CONTROL_RIGHTS` name SYSTEM or Administrators alone. An entry's rights
/// in hexadecimal are read as granting them all.
pub fn control_trustees(dacl: &str) -> Vec<String> {
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
pub struct Token(pub HANDLE);

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
pub struct TestUser {
    pub name: String,
    pub token: Token,
}

impl TestUser {
    pub fn create() -> Self {
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
pub fn filtered_administrator_token() -> Token {
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
pub const ADMINISTRATORS: &str = "S-1-5-32-544";
pub const INTERACTIVE: &str = "S-1-5-4";
pub const SERVICE: &str = "S-1-5-6";

/// `token`'s groups, each SID in full form with its attributes.
pub fn groups(token: &Token) -> Vec<(String, u32)> {
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
pub fn as_token<T: Send + 'static>(
    token: &Token,
    action: impl FnOnce() -> T + Send + 'static,
) -> T {
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
pub fn open_file(path: &Path, access: u32) -> Result<(), u32> {
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
pub fn open_key(key: &str, access: u32) -> Result<(), u32> {
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
pub const SERVICE_KEY: &str = r"SYSTEM\CurrentControlSet\Services\CyberGuardAgent";

/// What an ordinary user may and may not do, as a list of problems: on the
/// service, query its status but not one of `CONTROL_RIGHTS`; on its
/// registry key and the Application log source's, read them but not set a
/// value, create a subkey, delete them, or change their DACL or owner; on the
/// binary, read and run it but not write, append, delete, or change its DACL
/// or owner; on its directories, list them but not add a file or a
/// directory, delete an entry, delete them, or change their DACL or owner
/// (§Data contracts §2 and §3). Each grant is a positive control: a token
/// that is denied everything cannot pass.
pub fn ordinary_access_problems(token: &Token, who: &str, paths: &Paths) -> Vec<String> {
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
pub fn message_of(path: &Path, id: u32, insert: &str) -> Option<String> {
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

/// The owner of `path`, as SDDL spells it (`SY`, `BA`, or a full SID).
pub fn owner_of(path: &Path) -> String {
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

// --- Windows Installer's databases ---------------------------------------------------

/// An MSI handle, closed on drop.
struct MsiHandle(u32);

impl Drop for MsiHandle {
    fn drop(&mut self) {
        if self.0 != 0 {
            // SAFETY: a handle this value owns, closed once.
            unsafe {
                windows_sys::Win32::System::ApplicationInstallationAndServicing::MsiCloseHandle(
                    self.0,
                )
            };
        }
    }
}

/// The first column of each row `query` selects from the database `msi`,
/// opened read-only.
fn msi_query(msi: &Path, query: &str) -> Vec<String> {
    use windows_sys::Win32::System::ApplicationInstallationAndServicing::{
        MsiDatabaseOpenViewW, MsiOpenDatabaseW, MsiRecordGetStringW, MsiViewExecute, MsiViewFetch,
        MSIDBOPEN_READONLY,
    };
    const ERROR_NO_MORE_ITEMS: u32 = 259;
    let path = wide(msi.as_os_str());
    let mut database = MsiHandle(0);
    // SAFETY: a NUL-terminated path and an out-handle, owned below.
    let error = unsafe { MsiOpenDatabaseW(path.as_ptr(), MSIDBOPEN_READONLY, &mut database.0) };
    assert_eq!(error, 0, "MsiOpenDatabaseW {}", msi.display());
    let query_text = wide(OsStr::new(query));
    let mut view = MsiHandle(0);
    // SAFETY: an open database, a NUL-terminated query and an out-handle.
    let error = unsafe { MsiDatabaseOpenViewW(database.0, query_text.as_ptr(), &mut view.0) };
    assert_eq!(error, 0, "MsiDatabaseOpenViewW {query}");
    // SAFETY: an open view with no parameters.
    let error = unsafe { MsiViewExecute(view.0, 0) };
    assert_eq!(error, 0, "MsiViewExecute {query}");
    let mut rows = Vec::new();
    loop {
        let mut record = MsiHandle(0);
        // SAFETY: an executed view and an out-handle.
        let error = unsafe { MsiViewFetch(view.0, &mut record.0) };
        if error == ERROR_NO_MORE_ITEMS {
            break;
        }
        assert_eq!(error, 0, "MsiViewFetch {query}");
        let mut text = vec![0u16; 256];
        let mut length = text.len() as u32;
        // SAFETY: a fetched record and a buffer of `length` units.
        let error = unsafe { MsiRecordGetStringW(record.0, 1, text.as_mut_ptr(), &mut length) };
        assert_eq!(error, 0, "MsiRecordGetStringW {query}");
        rows.push(String::from_utf16_lossy(&text[..length as usize]));
    }
    rows
}

/// The custom actions of the MSI at `msi`, by name: none when it has no
/// `CustomAction` table.
pub fn custom_actions(msi: &Path) -> Vec<String> {
    let tables = msi_query(
        msi,
        "SELECT `Name` FROM `_Tables` WHERE `Name` = 'CustomAction'",
    );
    if tables.is_empty() {
        return Vec::new();
    }
    msi_query(msi, "SELECT `Action` FROM `CustomAction`")
}
