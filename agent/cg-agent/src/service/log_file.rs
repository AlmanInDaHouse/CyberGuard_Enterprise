//! The service's file log (SPEC-024 §Data contracts §2):
//! `%ProgramData%\CyberGuard\agent\logs\agent.jsonl`, SYSTEM-owned, written
//! by SYSTEM and readable by elevated Administrators.
//!
//! `%ProgramData%` comes from the OS (`SHGetKnownFolderPath`), not from an
//! environment variable. Each directory below it is created with its final
//! protected DACL, or, when it exists, checked through a handle before the
//! service writes into it: a directory, not a reparse point, owned by
//! SYSTEM or Administrators, whose DACL allows no other principal. The
//! file is opened without following a reparse point, refused when it is
//! hard-linked, and, when it exists, checked the same way. A service
//! running as SYSTEM never writes where an ordinary user could redirect
//! it (SPEC-024 §Operational §1 and §2). Rotation is install_ac_010's.

use std::fs::File;
use std::os::windows::io::{FromRawHandle, RawHandle};
use std::path::{Path, PathBuf};

use windows_sys::core::PWSTR;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_ALREADY_EXISTS, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1,
    SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    GetAce, IsWellKnownSid, WinBuiltinAdministratorsSid, WinLocalSystemSid, ACCESS_ALLOWED_ACE,
    ACE_HEADER, ACL, DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
    PSID, SECURITY_ATTRIBUTES,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateDirectoryW, CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    FILE_APPEND_DATA, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_ALWAYS,
    OPEN_EXISTING, READ_CONTROL,
};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::UI::Shell::{FOLDERID_ProgramData, SHGetKnownFolderPath};

/// `CyberGuard` and `CyberGuard\agent`: SYSTEM and Administrators only.
const PRIVATE_DIRECTORY: &str = "O:SYD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)";
/// `logs`: SYSTEM writes, Administrators read.
const LOG_DIRECTORY: &str = "O:SYD:P(A;OICI;FA;;;SY)(A;OICI;FR;;;BA)";
/// `agent.jsonl`: as its directory.
const LOG_FILE: &str = "O:SYD:P(A;;FA;;;SY)(A;;FR;;;BA)";

/// The ACE types a checked DACL may hold (winnt.h); any other is refused.
const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;
const ACCESS_DENIED_ACE_TYPE: u8 = 1;

/// Open (creating as needed) the service's log file for appending.
pub(super) fn open() -> Result<File, String> {
    let agent = program_data()?.join("CyberGuard").join("agent");
    ensure_private_directory(agent.parent().expect("CyberGuard"), PRIVATE_DIRECTORY)?;
    ensure_private_directory(&agent, PRIVATE_DIRECTORY)?;
    let logs = agent.join("logs");
    ensure_private_directory(&logs, LOG_DIRECTORY)?;
    open_log_file(&logs.join("agent.jsonl"))
}

fn wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn last_error() -> u32 {
    // SAFETY: reads the calling thread's last-error value.
    unsafe { GetLastError() }
}

/// `%ProgramData%`, from the OS.
fn program_data() -> Result<PathBuf, String> {
    let mut raw: PWSTR = std::ptr::null_mut();
    // SAFETY: a known-folder id and an out-pointer the call fills with a
    // CoTaskMemAlloc'd string, freed below.
    let hr =
        unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramData, 0, std::ptr::null_mut(), &mut raw) };
    if hr < 0 || raw.is_null() {
        if !raw.is_null() {
            // SAFETY: the string the call allocated.
            unsafe { CoTaskMemFree(raw.cast()) };
        }
        return Err(format!("resolve %ProgramData%: HRESULT {hr:#010x}"));
    }
    // SAFETY: a NUL-terminated wide string from the call.
    let path = unsafe {
        let len = (0..).take_while(|&i| *raw.add(i) != 0).count();
        String::from_utf16_lossy(std::slice::from_raw_parts(raw, len))
    };
    // SAFETY: the string the call allocated, freed once.
    unsafe { CoTaskMemFree(raw.cast()) };
    Ok(PathBuf::from(path))
}

/// A security descriptor built from SDDL, freed on drop.
struct Descriptor(PSECURITY_DESCRIPTOR);

impl Descriptor {
    fn from_sddl(sddl: &str) -> Result<Self, String> {
        let text: Vec<u16> = sddl.encode_utf16().chain(std::iter::once(0)).collect();
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: the string lives for the call; the descriptor is freed on
        // drop with LocalFree.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(format!(
                "build a security descriptor: Win32 error {}",
                last_error()
            ));
        }
        Ok(Descriptor(descriptor))
    }

    fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0,
            bInheritHandle: 0,
        }
    }
}

impl Drop for Descriptor {
    fn drop(&mut self) {
        // SAFETY: allocated by ConvertStringSecurityDescriptor…, freed once.
        unsafe { LocalFree(self.0.cast()) };
    }
}

/// A handle closed on drop.
struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: a handle CreateFileW returned, closed once.
        unsafe { CloseHandle(self.0) };
    }
}

/// Create `path` with `sddl` as its security, or check the directory that
/// is there.
fn ensure_private_directory(path: &Path, sddl: &str) -> Result<(), String> {
    let descriptor = Descriptor::from_sddl(sddl)?;
    let attributes = descriptor.attributes();
    let name = wide(path);
    // SAFETY: the name and attributes live for the call.
    if unsafe { CreateDirectoryW(name.as_ptr(), &attributes) } != 0 {
        return Ok(());
    }
    let error = last_error();
    if error != ERROR_ALREADY_EXISTS {
        return Err(format!("create {}: Win32 error {error}", path.display()));
    }
    // SAFETY: the name lives for the call; the handle is closed on drop.
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            READ_CONTROL | FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(format!(
            "open {}: Win32 error {}",
            path.display(),
            last_error()
        ));
    }
    let handle = Handle(handle);
    let info = file_information(&handle, path)?;
    if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(format!("{} is a reparse point", path.display()));
    }
    if info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0 {
        return Err(format!("{} is not a directory", path.display()));
    }
    check_custody(&handle, path)
}

/// Open the log file for appending, created with its final security.
fn open_log_file(path: &Path) -> Result<File, String> {
    let descriptor = Descriptor::from_sddl(LOG_FILE)?;
    let attributes = descriptor.attributes();
    let name = wide(path);
    // SAFETY: the name and attributes live for the call; the handle is
    // owned by the returned File or closed on drop.
    let raw = unsafe {
        CreateFileW(
            name.as_ptr(),
            FILE_APPEND_DATA | READ_CONTROL | FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ,
            &attributes,
            OPEN_ALWAYS,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        return Err(format!(
            "open {}: Win32 error {}",
            path.display(),
            last_error()
        ));
    }
    let existed = last_error() == ERROR_ALREADY_EXISTS;
    let handle = Handle(raw);
    let info = file_information(&handle, path)?;
    if info.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY) != 0 {
        return Err(format!("{} is not a regular file", path.display()));
    }
    if info.nNumberOfLinks != 1 {
        return Err(format!("{} is hard-linked", path.display()));
    }
    if existed {
        check_custody(&handle, path)?;
    }
    let raw = handle.0;
    std::mem::forget(handle);
    // SAFETY: a file handle this function owns, handed to the File.
    Ok(unsafe { File::from_raw_handle(raw as RawHandle) })
}

fn file_information(handle: &Handle, path: &Path) -> Result<BY_HANDLE_FILE_INFORMATION, String> {
    // SAFETY: an all-zero BY_HANDLE_FILE_INFORMATION is a valid out-buffer.
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: an open handle and the out-buffer.
    if unsafe { GetFileInformationByHandle(handle.0, &mut info) } == 0 {
        return Err(format!(
            "inspect {}: Win32 error {}",
            path.display(),
            last_error()
        ));
    }
    Ok(info)
}

/// SYSTEM or Administrators own the object, and its DACL allows no other
/// principal (deny entries may name anyone).
fn check_custody(handle: &Handle, path: &Path) -> Result<(), String> {
    let mut owner: PSID = std::ptr::null_mut();
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: an open handle with READ_CONTROL; the out-pointers point into
    // `descriptor`, freed below.
    let error = unsafe {
        GetSecurityInfo(
            handle.0,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            &mut dacl,
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if error != 0 {
        return Err(format!(
            "read the security of {}: Win32 error {error}",
            path.display()
        ));
    }
    let descriptor = Descriptor(descriptor);
    let verdict = (|| {
        if !is_system_or_administrators(owner) {
            return Err(format!(
                "{} is not owned by SYSTEM or Administrators",
                path.display()
            ));
        }
        if dacl.is_null() {
            return Err(format!("{} has no DACL", path.display()));
        }
        // SAFETY: a DACL inside the descriptor.
        let count = unsafe { (*dacl).AceCount };
        for index in 0..u32::from(count) {
            let mut ace: *mut core::ffi::c_void = std::ptr::null_mut();
            // SAFETY: an index below the DACL's count.
            if unsafe { GetAce(dacl, index, &mut ace) } == 0 {
                return Err(format!(
                    "read an ACE of {}: Win32 error {}",
                    path.display(),
                    last_error()
                ));
            }
            // SAFETY: every ACE starts with an ACE_HEADER.
            let kind = unsafe { (*(ace as *const ACE_HEADER)).AceType };
            match kind {
                ACCESS_DENIED_ACE_TYPE => {}
                ACCESS_ALLOWED_ACE_TYPE => {
                    // SAFETY: an ACCESS_ALLOWED_ACE; its SID starts at SidStart.
                    let sid = unsafe {
                        std::ptr::addr_of!((*(ace as *const ACCESS_ALLOWED_ACE)).SidStart)
                    } as PSID;
                    if !is_system_or_administrators(sid) {
                        return Err(format!(
                            "{} allows access to a principal other than SYSTEM and Administrators",
                            path.display()
                        ));
                    }
                }
                other => {
                    return Err(format!(
                        "{} holds an ACE of type {other}, which is not checked",
                        path.display()
                    ))
                }
            }
        }
        Ok(())
    })();
    drop(descriptor);
    verdict
}

fn is_system_or_administrators(sid: PSID) -> bool {
    // SAFETY: a SID inside a live security descriptor.
    !sid.is_null()
        && unsafe {
            IsWellKnownSid(sid, WinLocalSystemSid) != 0
                || IsWellKnownSid(sid, WinBuiltinAdministratorsSid) != 0
        }
}
