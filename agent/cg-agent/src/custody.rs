//! Custody of the service's private files (SPEC-024 §Data contracts §2,
//! §Operational §1 and §2, and its Amendment 2026-11-04, items 3, 4 and 6).
//!
//! Every object is opened without following a reparse point and checked
//! through that handle: a directory where one is expected, otherwise a
//! regular file with one link; not a reparse point; owned by one of the
//! policy's owners; and a DACL whose allow entries name only the policy's
//! principals (deny entries may name anyone, an entry of another type is
//! refused). What this module creates gets its final security when it is
//! created, never after a write: a directory by `CreateDirectoryW` and a
//! file by `CreateFileW` with `CREATE_NEW`, each with a security descriptor
//! built from SDDL.
//!
//! `%ProgramData%` comes from the OS (`SHGetKnownFolderPath`), not from an
//! environment variable.

use std::fs::File;
use std::io::{Read, Write};
use std::os::windows::io::{FromRawHandle, RawHandle};
use std::path::{Path, PathBuf};

use windows_sys::core::PWSTR;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND,
    ERROR_PATH_NOT_FOUND, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1,
    SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    EqualSid, GetAce, GetSecurityDescriptorControl, GetSecurityDescriptorDacl,
    GetSecurityDescriptorOwner, IsValidSid, IsWellKnownSid, WinBuiltinAdministratorsSid,
    WinLocalSystemSid, ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, DACL_SECURITY_INFORMATION,
    OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES, SE_DACL_PROTECTED,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateDirectoryW, CreateFileW, DeleteFileW, GetFileInformationByHandle, MoveFileExW,
    BY_HANDLE_FILE_INFORMATION, CREATE_NEW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, OPEN_EXISTING, READ_CONTROL,
};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::UI::Shell::{FOLDERID_ProgramData, SHGetKnownFolderPath};

/// The ACE types a checked DACL may hold (winnt.h); any other is refused.
const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;
const ACCESS_DENIED_ACE_TYPE: u8 = 1;

/// A principal a policy names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Principal {
    /// `S-1-5-18`.
    System,
    /// `S-1-5-32-544`.
    Administrators,
    /// Any other SID, in its binary form (tests name the current user).
    Sid(Vec<u8>),
}

impl Principal {
    fn matches(&self, sid: PSID) -> bool {
        // SAFETY: `sid` points into a live security descriptor; IsValidSid
        // guards the comparisons.
        unsafe {
            if sid.is_null() || IsValidSid(sid) == 0 {
                return false;
            }
            match self {
                Principal::System => IsWellKnownSid(sid, WinLocalSystemSid) != 0,
                Principal::Administrators => IsWellKnownSid(sid, WinBuiltinAdministratorsSid) != 0,
                // A SID is 8 bytes and 4 per subauthority; checked before
                // any call reads it.
                Principal::Sid(bytes) => {
                    bytes.len() >= 8
                        && bytes.len() == 8 + 4 * usize::from(bytes[1])
                        && IsValidSid(bytes.as_ptr() as PSID) != 0
                        && EqualSid(bytes.as_ptr() as PSID, sid) != 0
                }
            }
        }
    }
}

/// Who may own a checked object, whom its allow entries may name, and
/// whether its DACL must be protected.
#[derive(Debug, Clone)]
pub struct Policy {
    pub owners: Vec<Principal>,
    pub allowed: Vec<Principal>,
    pub protected: bool,
}

impl Policy {
    /// `CyberGuard`, the data root and the logs, as the file log checks
    /// them (the second Amendment 2026-11-01, item 3): owned by SYSTEM or
    /// Administrators, SYSTEM and Administrators allowed.
    pub fn shared() -> Self {
        Policy {
            owners: vec![Principal::System, Principal::Administrators],
            allowed: vec![Principal::System, Principal::Administrators],
            protected: false,
        }
    }

    /// `agent.toml` and `server-ca.pem` (Amendment 2026-11-04, item 3):
    /// owner SYSTEM, a protected DACL allowing SYSTEM and Administrators.
    pub fn configuration() -> Self {
        Policy {
            owners: vec![Principal::System],
            allowed: vec![Principal::System, Principal::Administrators],
            protected: true,
        }
    }

    /// `identity\` and its files: owner SYSTEM, a protected DACL allowing
    /// SYSTEM alone.
    pub fn system_only() -> Self {
        Policy {
            owners: vec![Principal::System],
            allowed: vec![Principal::System],
            protected: true,
        }
    }
}

/// What a checked path must be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Directory,
    File,
}

pub(crate) fn wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

pub(crate) fn last_error() -> u32 {
    // SAFETY: reads the calling thread's last-error value.
    unsafe { GetLastError() }
}

/// `%ProgramData%`, from the OS.
pub fn program_data() -> Result<PathBuf, String> {
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
        use std::os::windows::ffi::OsStringExt;
        let len = (0..).take_while(|&i| *raw.add(i) != 0).count();
        std::ffi::OsString::from_wide(std::slice::from_raw_parts(raw, len))
    };
    // SAFETY: the string the call allocated, freed once.
    unsafe { CoTaskMemFree(raw.cast()) };
    Ok(PathBuf::from(path))
}

/// A security descriptor built from SDDL, freed on drop.
pub(crate) struct Descriptor(PSECURITY_DESCRIPTOR);

impl Descriptor {
    pub(crate) fn from_sddl(sddl: &str) -> Result<Self, String> {
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

    pub(crate) fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0,
            bInheritHandle: 0,
        }
    }
}

impl Drop for Descriptor {
    fn drop(&mut self) {
        // SAFETY: a LocalAlloc'd descriptor (from SDDL or GetSecurityInfo),
        // freed once.
        unsafe { LocalFree(self.0.cast()) };
    }
}

/// A handle closed on drop.
pub(crate) struct Handle(pub(crate) HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: a handle CreateFileW returned, closed once.
        unsafe { CloseHandle(self.0) };
    }
}

impl Handle {
    /// Hand the handle to a `File`, which closes it.
    pub(crate) fn into_file(self) -> File {
        let raw = self.0;
        std::mem::forget(self);
        // SAFETY: a file handle this value owned, handed to the File.
        unsafe { File::from_raw_handle(raw as RawHandle) }
    }
}

pub(crate) fn file_information(
    handle: &Handle,
    path: &Path,
) -> Result<BY_HANDLE_FILE_INFORMATION, String> {
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

/// Open `path` without following a reparse point; `None` when nothing is
/// there. A directory opens too, so that the caller's kind check names it.
fn open_existing(path: &Path, access: u32, share: u32) -> Result<Option<Handle>, String> {
    let name = wide(path);
    // SAFETY: the name lives for the call; the handle is closed on drop.
    let raw = unsafe {
        CreateFileW(
            name.as_ptr(),
            access,
            share,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        let error = last_error();
        if error == ERROR_FILE_NOT_FOUND || error == ERROR_PATH_NOT_FOUND {
            return Ok(None);
        }
        return Err(format!("open {}: Win32 error {error}", path.display()));
    }
    Ok(Some(Handle(raw)))
}

/// The kind, link and reparse checks on an open handle.
fn check_shape(handle: &Handle, path: &Path, kind: Kind) -> Result<(), String> {
    let info = file_information(handle, path)?;
    if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(format!("{} is a reparse point", path.display()));
    }
    let is_directory = info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
    match kind {
        Kind::Directory if !is_directory => Err(format!("{} is not a directory", path.display())),
        Kind::File if is_directory => Err(format!("{} is not a regular file", path.display())),
        Kind::File if info.nNumberOfLinks != 1 => Err(format!("{} is hard-linked", path.display())),
        _ => Ok(()),
    }
}

/// Check the object at `path` against `policy`; `Ok(false)` when nothing is
/// there.
pub fn check_if_exists(path: &Path, kind: Kind, policy: &Policy) -> Result<bool, String> {
    let Some(handle) = open_existing(
        path,
        READ_CONTROL | FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
    )?
    else {
        return Ok(false);
    };
    check_shape(&handle, path, kind)?;
    check_security(&handle, path, policy)?;
    Ok(true)
}

/// Check the object at `path` against `policy`; absent is refused.
pub fn check(path: &Path, kind: Kind, policy: &Policy) -> Result<(), String> {
    if check_if_exists(path, kind, policy)? {
        Ok(())
    } else {
        Err(format!("{} does not exist", path.display()))
    }
}

/// Open the file at `path` for reading, check it against `policy` through
/// that handle and read it all from the same handle. `Ok(None)` when it is
/// absent. Others may read it meanwhile; no one may change it.
pub fn read_checked(path: &Path, policy: &Policy) -> Result<Option<Vec<u8>>, String> {
    let Some(handle) = open_existing(
        path,
        GENERIC_READ | READ_CONTROL | FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ,
    )?
    else {
        return Ok(None);
    };
    check_shape(&handle, path, Kind::File)?;
    check_security(&handle, path, policy)?;
    let mut bytes = Vec::new();
    handle
        .into_file()
        .read_to_end(&mut bytes)
        .map_err(|e| format!("read {}: {e}", path.display()))?;
    Ok(Some(bytes))
}

/// Create the directory `path` with `sddl` as its security, or check the
/// directory that is there against `policy`.
pub fn ensure_directory(path: &Path, sddl: &str, policy: &Policy) -> Result<(), String> {
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
    check(path, Kind::Directory, policy)
}

/// Create the file `path`, which must not exist, with `sddl` as its
/// security, write `contents` and flush them. Nothing is written under
/// another security.
pub fn create_new_file(path: &Path, sddl: &str, contents: &[u8]) -> Result<(), String> {
    let descriptor = Descriptor::from_sddl(sddl)?;
    let attributes = descriptor.attributes();
    let name = wide(path);
    // SAFETY: the name and attributes live for the call; the handle is
    // owned by the File below.
    let raw = unsafe {
        CreateFileW(
            name.as_ptr(),
            GENERIC_WRITE,
            0,
            &attributes,
            CREATE_NEW,
            // An existing link of that name is refused, not followed.
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        return Err(format!(
            "create {}: Win32 error {}",
            path.display(),
            last_error()
        ));
    }
    let mut file = Handle(raw).into_file();
    file.write_all(contents)
        .and_then(|()| file.sync_all())
        .map_err(|e| format!("write {}: {e}", path.display()))
}

/// Replace the file `path` with `contents` through a new `temporary` file
/// created with the owner and DACL read from `path` through its handle,
/// then moved over it (Amendment 2026-11-04, item 6). `path` passes
/// `policy` first, so what is copied is a protected DACL the service
/// accepts. A leftover `temporary` is deleted first.
pub fn replace_keeping_security(
    path: &Path,
    temporary: &Path,
    contents: &[u8],
    policy: &Policy,
) -> Result<(), String> {
    let handle = open_existing(
        path,
        READ_CONTROL | FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
    )?
    .ok_or_else(|| format!("{} does not exist", path.display()))?;
    check_shape(&handle, path, Kind::File)?;
    // One read: the descriptor checked is the descriptor copied (#146).
    let descriptor = security_of(&handle, path)?;
    check_descriptor(&descriptor, path, policy)?;
    drop(handle);

    let temporary_name = wide(temporary);
    // SAFETY: the name lives for the call.
    if unsafe { DeleteFileW(temporary_name.as_ptr()) } == 0 {
        let error = last_error();
        if error != ERROR_FILE_NOT_FOUND {
            return Err(format!(
                "delete {}: Win32 error {error}",
                temporary.display()
            ));
        }
    }
    let attributes = descriptor.attributes();
    // SAFETY: the name and attributes live for the call; the handle is
    // owned by the File below.
    let raw = unsafe {
        CreateFileW(
            temporary_name.as_ptr(),
            GENERIC_WRITE,
            0,
            &attributes,
            CREATE_NEW,
            // An existing link of that name is refused, not followed.
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        return Err(format!(
            "create {}: Win32 error {}",
            temporary.display(),
            last_error()
        ));
    }
    let written = {
        let mut file = Handle(raw).into_file();
        file.write_all(contents).and_then(|()| file.sync_all())
    };
    let target = wide(path);
    let moved = written
        .map_err(|e| format!("write {}: {e}", temporary.display()))
        .and_then(|()| {
            // SAFETY: both names live for the call.
            if unsafe {
                MoveFileExW(
                    temporary_name.as_ptr(),
                    target.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            } == 0
            {
                return Err(format!(
                    "replace {}: Win32 error {}",
                    path.display(),
                    last_error()
                ));
            }
            Ok(())
        });
    if moved.is_err() {
        // SAFETY: the name lives for the call; the temporary file is ours.
        unsafe { DeleteFileW(temporary_name.as_ptr()) };
    }
    moved
}

/// The owner and DACL of an open object, as a self-relative descriptor.
fn security_of(handle: &Handle, path: &Path) -> Result<Descriptor, String> {
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: an open handle with READ_CONTROL; the descriptor is freed by
    // the returned value.
    let error = unsafe {
        GetSecurityInfo(
            handle.0,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
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
    Ok(Descriptor(descriptor))
}

/// One of `policy`'s owners owns the object, and its DACL allows no
/// principal outside `policy` (deny entries may name anyone).
pub(crate) fn check_security(handle: &Handle, path: &Path, policy: &Policy) -> Result<(), String> {
    check_descriptor(&security_of(handle, path)?, path, policy)
}

/// `check_security` on a descriptor already read.
fn check_descriptor(descriptor: &Descriptor, path: &Path, policy: &Policy) -> Result<(), String> {
    let unreadable = || {
        format!(
            "read the security of {}: Win32 error {}",
            path.display(),
            last_error()
        )
    };
    let mut owner: PSID = std::ptr::null_mut();
    let mut defaulted = 0;
    // SAFETY: a descriptor GetSecurityInfo returned; `owner` points into it.
    if unsafe { GetSecurityDescriptorOwner(descriptor.0, &mut owner, &mut defaulted) } == 0 {
        return Err(unreadable());
    }
    let mut present = 0;
    let mut dacl: *mut ACL = std::ptr::null_mut();
    // SAFETY: the same descriptor; `dacl` points into it.
    if unsafe { GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut dacl, &mut defaulted) }
        == 0
    {
        return Err(unreadable());
    }
    if present == 0 {
        dacl = std::ptr::null_mut();
    }
    let verdict = (|| {
        if !policy.owners.iter().any(|p| p.matches(owner)) {
            return Err(format!(
                "{} is not owned by {}",
                path.display(),
                names(&policy.owners, " or ")
            ));
        }
        if dacl.is_null() {
            return Err(format!("{} has no DACL", path.display()));
        }
        if policy.protected {
            let mut control = 0u16;
            let mut revision = 0u32;
            // SAFETY: a descriptor GetSecurityInfo returned, and two
            // out-values.
            if unsafe { GetSecurityDescriptorControl(descriptor.0, &mut control, &mut revision) }
                == 0
            {
                return Err(format!(
                    "read the security of {}: Win32 error {}",
                    path.display(),
                    last_error()
                ));
            }
            if control & SE_DACL_PROTECTED == 0 {
                return Err(format!("{}'s DACL is not protected", path.display()));
            }
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
                    if !policy.allowed.iter().any(|p| p.matches(sid)) {
                        return Err(format!(
                            "{} allows access to a principal other than {}",
                            path.display(),
                            names(&policy.allowed, " and ")
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
    verdict
}

fn names(principals: &[Principal], joiner: &str) -> String {
    let named: Vec<&str> = principals
        .iter()
        .map(|p| match p {
            Principal::System => "SYSTEM",
            Principal::Administrators => "Administrators",
            Principal::Sid(_) => "the policy's account",
        })
        .collect();
    named.join(joiner)
}
