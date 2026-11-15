//! PROBE, never lands: the C9 reading probe's stub of the purge step
//! (SPEC-024 install_ac_012, before its contract is written). Burn runs it
//! as the bundle's `Purge` package, `cg-setup.exe purge
//! --confirm="[CG_PURGE]"`, and the probe's host test
//! (`tests/package_purge_probe.rs`) reads, on a disposable GitHub-hosted
//! runner, when Burn runs it and with what.
//!
//! It deletes nothing and reads no secret: it appends one JSON line to
//! `C:\cg-probe\purge-readings.jsonl` with its command line, its executable,
//! its token's user and elevation, the product service's state, and whether
//! the installed binary, the MSI's upgrade family and the data directory
//! exist at that moment; then it exits with the integer in
//! `C:\cg-probe\purge-exit` if that file exists, else 0.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Security::{
    GetSidIdentifierAuthority, GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation,
    TokenElevation, TokenUser, TOKEN_ELEVATION, TOKEN_QUERY, TOKEN_USER,
};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY,
};
use windows_sys::Win32::System::Services::SERVICE_QUERY_STATUS;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use crate::scm::Manager;
use crate::service::PRODUCT_SERVICE;

/// Where the stub writes, and where the job leaves its exit code.
const PROBE_DIR: &str = r"C:\cg-probe";

/// The MSI's upgrade family, `AF9D0A74-7206-49F9-B391-645F1C1F05AD`, packed,
/// as Windows Installer registers a per-machine product under it.
const UPGRADE_CODE_KEY: &str =
    r"SOFTWARE\Classes\Installer\UpgradeCodes\47A0D9FA60279F943B1946F5C1F150DA";

/// Record this run and return the exit code the job left, or 0.
pub fn record() -> i32 {
    let directory = Path::new(PROBE_DIR);
    let exit = std::fs::read_to_string(directory.join("purge-exit"))
        .ok()
        .and_then(|text| text.trim().parse::<i32>().ok())
        .unwrap_or(0);
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default();
    let (user, elevated) = token_user_and_elevation();
    let (service_exists, service_state, service_pid) = service_state();
    let program_files = cg_agent::custody::program_files().ok();
    let program_data = cg_agent::custody::program_data().ok();
    let binary = program_files.map(|root| crate::verbs::installed_binary(&root));
    let data = program_data.map(|root| root.join("CyberGuard").join("agent"));
    let line = serde_json::json!({
        "millis": millis.to_string(),
        "pid": std::process::id(),
        "argv": std::env::args_os()
            .skip(1)
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        "exe": std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|e| format!("error: {e}")),
        "cwd": std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_else(|e| format!("error: {e}")),
        "user_sid": user,
        "token_elevated": elevated,
        "administrators_enabled": crate::platform::check_elevation().is_ok(),
        "service_exists": service_exists,
        "service_state": service_state,
        "service_pid": service_pid,
        "binary": binary.as_ref().map(|p| p.display().to_string()),
        "binary_exists": binary.as_deref().map(exists),
        "msi_family_registered": key_exists(UPGRADE_CODE_KEY),
        "data_dir": data.as_ref().map(|p| p.display().to_string()),
        "data_dir_exists": data.as_deref().map(exists),
        "exit": exit,
    });
    if let Err(error) = append(directory, &line.to_string()) {
        eprintln!("the purge probe cannot write its reading: {error}");
    }
    exit
}

fn exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

fn append(directory: &Path, line: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(directory)?;
    let path: PathBuf = directory.join("purge-readings.jsonl");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{line}")
}

/// The product service: whether it exists, its state and its process, or
/// the Win32 error that kept them from being read.
fn service_state() -> (Option<bool>, Option<u32>, Option<u32>) {
    let Ok(scm) = Manager::open() else {
        return (None, None, None);
    };
    match scm.service(PRODUCT_SERVICE, SERVICE_QUERY_STATUS) {
        Ok(None) => (Some(false), None, None),
        Ok(Some(service)) => match service.status() {
            Ok(status) => (
                Some(true),
                Some(status.dwCurrentState),
                Some(status.dwProcessId),
            ),
            Err(_) => (Some(true), None, None),
        },
        Err(_) => (None, None, None),
    }
}

/// The process token's user, as an SID string, and whether the token is
/// elevated (`TokenElevation`).
fn token_user_and_elevation() -> (Option<String>, Option<bool>) {
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: the current process's pseudo-handle and an out-value.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return (None, None);
    }
    let user = {
        let mut buffer = vec![0u64; 64];
        let mut needed = 0u32;
        // SAFETY: a buffer of 512 bytes, aligned for TOKEN_USER.
        let ok = unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                (buffer.len() * 8) as u32,
                &mut needed,
            )
        };
        if ok == 0 {
            None
        } else {
            // SAFETY: Windows wrote a TOKEN_USER at the buffer's head, its SID
            // within the buffer.
            Some(unsafe { sid_string((*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid) })
        }
    };
    let elevated = {
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut needed = 0u32;
        // SAFETY: the out-buffer is one TOKEN_ELEVATION.
        let ok = unsafe {
            GetTokenInformation(
                token,
                TokenElevation,
                (&mut elevation as *mut TOKEN_ELEVATION).cast(),
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut needed,
            )
        };
        (ok != 0).then_some(elevation.TokenIsElevated != 0)
    };
    // SAFETY: the token opened above, closed once.
    unsafe { CloseHandle(token) };
    (user, elevated)
}

/// `S-1-<authority>-<sub>...` of a valid SID.
///
/// # Safety
/// `sid` points to a valid SID.
unsafe fn sid_string(sid: *mut core::ffi::c_void) -> String {
    let authority = (*GetSidIdentifierAuthority(sid)).Value;
    let authority = authority
        .iter()
        .fold(0u64, |value, byte| (value << 8) | u64::from(*byte));
    let count = *GetSidSubAuthorityCount(sid);
    let mut text = format!("S-1-{authority}");
    for index in 0..u32::from(count) {
        text.push_str(&format!("-{}", *GetSidSubAuthority(sid, index)));
    }
    text
}

/// Whether the `HKEY_LOCAL_MACHINE` key `key` exists, in the 64-bit view.
fn key_exists(key: &str) -> bool {
    let name: Vec<u16> = key.encode_utf16().chain(std::iter::once(0)).collect();
    let mut handle = std::ptr::null_mut();
    // SAFETY: a NUL-terminated name and an out-value, closed below.
    let error = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            name.as_ptr(),
            0,
            KEY_READ | KEY_WOW64_64KEY,
            &mut handle,
        )
    };
    if error != 0 {
        return false;
    }
    // SAFETY: a key opened above, closed once.
    unsafe { RegCloseKey(handle) };
    true
}
