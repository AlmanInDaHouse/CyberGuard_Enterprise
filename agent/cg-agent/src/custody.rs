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
use std::os::windows::io::{AsRawHandle, FromRawHandle, RawHandle};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use windows_sys::core::PWSTR;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS,
    ERROR_FILE_NOT_FOUND, ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER, ERROR_LOCK_VIOLATION,
    ERROR_MORE_DATA, ERROR_NOT_SUPPORTED, ERROR_OPERATION_ABORTED, ERROR_PATH_NOT_FOUND,
    ERROR_SHARING_VIOLATION, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
    SDDL_REVISION_1, SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    EqualSid, GetAce, GetLengthSid, GetSecurityDescriptorControl, GetSecurityDescriptorDacl,
    GetSecurityDescriptorOwner, IsValidSid, IsWellKnownSid, SetKernelObjectSecurity,
    WinBuiltinAdministratorsSid, WinLocalSystemSid, ACCESS_ALLOWED_ACE, ACE_HEADER, ACL,
    DACL_SECURITY_INFORMATION, INHERIT_ONLY_ACE, OWNER_SECURITY_INFORMATION,
    PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES,
    SE_DACL_PROTECTED,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateDirectoryW, CreateFileW, DeleteFileW, FileCaseSensitiveInfo, FileDispositionInfo,
    FileDispositionInfoEx, GetFileInformationByHandle, GetFileInformationByHandleEx, MoveFileExW,
    SetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, CREATE_NEW, FILE_ATTRIBUTE_DIRECTORY,
    FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT, FILE_DISPOSITION_FLAG_DELETE,
    FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
    FILE_DISPOSITION_INFO, FILE_DISPOSITION_INFO_EX, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, OPEN_EXISTING,
    READ_CONTROL,
};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::SystemInformation::{
    ComputerNamePhysicalDnsHostname, GetComputerNameExW,
};
use windows_sys::Win32::System::IO::CancelSynchronousIo;
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
    /// Whether `sid`, a whole SID in its binary form, is this principal.
    pub(crate) fn is(&self, sid: &[u8]) -> bool {
        self.matches(sid.as_ptr() as PSID)
    }

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

    /// Whether the descriptor names an owner.
    fn has_owner(&self) -> Result<bool, String> {
        Ok(!self.owner()?.is_null())
    }

    fn owner(&self) -> Result<PSID, String> {
        let mut owner: PSID = std::ptr::null_mut();
        let mut defaulted = 0;
        // SAFETY: a live descriptor; `owner` points into it.
        if unsafe { GetSecurityDescriptorOwner(self.0, &mut owner, &mut defaulted) } == 0 {
            return Err(format!(
                "read a descriptor's owner: Win32 error {}",
                last_error()
            ));
        }
        Ok(owner)
    }

    /// Whether one of `owners` owns the object.
    pub(crate) fn owned_by(&self, owners: &[Principal]) -> Result<bool, String> {
        let owner = self.owner()?;
        Ok(owners.iter().any(|p| p.matches(owner)))
    }

    /// The allow entries that act on the object itself, an inherit-only one
    /// acting only on what is created in it; `None` for a null DACL. An
    /// entry of another type is refused, as the custody check does.
    pub(crate) fn allow_entries(&self, path: &Path) -> Result<Option<Vec<AllowEntry>>, String> {
        let mut present = 0;
        let mut dacl: *mut ACL = std::ptr::null_mut();
        let mut defaulted = 0;
        // SAFETY: a live descriptor; `dacl` points into it.
        if unsafe { GetSecurityDescriptorDacl(self.0, &mut present, &mut dacl, &mut defaulted) }
            == 0
        {
            return Err(format!(
                "read the security of {}: Win32 error {}",
                path.display(),
                last_error()
            ));
        }
        if present == 0 || dacl.is_null() {
            return Ok(None);
        }
        let mut entries = Vec::new();
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
            let header = unsafe { *(ace as *const ACE_HEADER) };
            if u32::from(header.AceFlags) & INHERIT_ONLY_ACE != 0 {
                continue;
            }
            match header.AceType {
                ACCESS_DENIED_ACE_TYPE => {}
                ACCESS_ALLOWED_ACE_TYPE => {
                    // SAFETY: an ACCESS_ALLOWED_ACE; its SID starts at SidStart.
                    let (mask, sid) = unsafe {
                        let allowed = ace as *const ACCESS_ALLOWED_ACE;
                        (
                            (*allowed).Mask,
                            std::ptr::addr_of!((*allowed).SidStart) as PSID,
                        )
                    };
                    // SAFETY: a valid SID inside the ACE, of GetLengthSid bytes.
                    let sid = unsafe {
                        std::slice::from_raw_parts(sid as *const u8, GetLengthSid(sid) as usize)
                    };
                    entries.push(AllowEntry {
                        sid: sid.to_vec(),
                        mask,
                    });
                }
                other => {
                    return Err(format!(
                        "{} holds an ACE of type {other}, which is not checked",
                        path.display()
                    ))
                }
            }
        }
        Ok(Some(entries))
    }
}

/// An allow entry of a DACL: a whole SID and its access mask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AllowEntry {
    pub(crate) sid: Vec<u8>,
    pub(crate) mask: u32,
}

/// A SID's text, for the migration's record alone: no line carries it.
pub(crate) fn sid_text(sid: &[u8]) -> Result<String, String> {
    let mut text: PWSTR = std::ptr::null_mut();
    // SAFETY: a whole SID; `text` is LocalAlloc'd, freed below.
    if unsafe { ConvertSidToStringSidW(sid.as_ptr() as PSID, &mut text) } == 0 {
        return Err(format!("render a SID: Win32 error {}", last_error()));
    }
    // SAFETY: a NUL-terminated string from the call, read then freed.
    unsafe {
        let len = (0..).take_while(|&i| *text.add(i) != 0).count();
        let rendered = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
        LocalFree(text.cast());
        Ok(rendered)
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

// SAFETY: a file handle may be used from any thread; this value owns it and
// hands it from one thread to the next (`bounded`).
unsafe impl Send for Handle {}

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

/// How long an open of a checked object may wait on another process: even
/// one for attributes and security alone waits on a batch opportunistic lock,
/// which a name an ordinary user planted can hold (SPEC-024 Amendment
/// 2026-11-05).
pub const OPEN_WAIT: Duration = Duration::from_secs(30);

/// Open `path` without following a reparse point; `None` when nothing is
/// there. A directory opens too, so that the caller's kind check names it.
fn open_existing(
    path: &Path,
    access: u32,
    share: u32,
    wait: Duration,
) -> Result<Option<Handle>, String> {
    let owned = path.to_path_buf();
    bounded(wait, path, move || {
        open_raw(&owned, access, share).map_err(|error| failure("open", &owned, error))
    })
}

/// `open_existing`, with the Win32 error of a failure.
fn open_raw(path: &Path, access: u32, share: u32) -> Result<Option<Handle>, u32> {
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
        return Err(error);
    }
    Ok(Some(Handle(raw)))
}

/// The kind, link and reparse checks on an open handle.
pub(crate) fn check_shape(handle: &Handle, path: &Path, kind: Kind) -> Result<(), String> {
    shape_of(&file_information(handle, path)?, path, kind)
}

/// `check_shape` on what the handle reported.
fn shape_of(info: &BY_HANDLE_FILE_INFORMATION, path: &Path, kind: Kind) -> Result<(), String> {
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

/// What a name is, without following it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Directory,
    ReparsePoint,
    Other,
}

fn shape_from(attributes: u32) -> Shape {
    if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        Shape::ReparsePoint
    } else if attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
        Shape::Directory
    } else {
        Shape::Other
    }
}

/// What the object at `path` is, opened without following a reparse point and
/// waiting on another process no longer than `wait`; `None` when nothing is
/// there.
pub(crate) fn shape(path: &Path, wait: Duration) -> Result<Option<Shape>, String> {
    let share = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;
    let Some(handle) = open_existing(path, FILE_READ_ATTRIBUTES, share, wait)? else {
        return Ok(None);
    };
    Ok(Some(shape_from(
        file_information(&handle, path)?.dwFileAttributes,
    )))
}

/// The entries of the directory `path`, each with what it is, from the
/// directory's own listing: no entry is opened, so an opportunistic lock on
/// one does not matter.
pub(crate) fn listing(path: &Path, wait: Duration) -> Result<Vec<(String, Shape)>, String> {
    use std::os::windows::fs::MetadataExt as _;
    let owned = path.to_path_buf();
    bounded(wait, path, move || {
        let list_error = |e: std::io::Error| format!("list {}: {e}", owned.display());
        std::fs::read_dir(&owned)
            .map_err(list_error)?
            .map(|entry| {
                let entry = entry.map_err(list_error)?;
                let attributes = entry.metadata().map_err(list_error)?.file_attributes();
                Ok((
                    entry.file_name().to_string_lossy().into_owned(),
                    shape_from(attributes),
                ))
            })
            .collect()
    })
}

/// Check the object at `path` against `policy`; `Ok(false)` when nothing is
/// there.
pub fn check_if_exists(path: &Path, kind: Kind, policy: &Policy) -> Result<bool, String> {
    check_if_exists_within(path, kind, policy, OPEN_WAIT)
}

/// `check_if_exists`, waiting on another process no longer than `wait`.
pub(crate) fn check_if_exists_within(
    path: &Path,
    kind: Kind,
    policy: &Policy,
    wait: Duration,
) -> Result<bool, String> {
    let Some(handle) = open_existing(
        path,
        READ_CONTROL | FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        wait,
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
    read_checked_within(path, policy, OPEN_WAIT).map_err(|unread| match unread {
        Unread::Unreadable(reason) | Unread::Refused(reason) => reason,
    })
}

/// Whether the object at `path`, whatever else it is, has the owner and
/// DACL `policy` asks for. Anything else, an open that fails included, is
/// `false`.
pub(crate) fn has_security(path: &Path, policy: &Policy, wait: Duration) -> bool {
    let access = READ_CONTROL | FILE_READ_ATTRIBUTES;
    let share = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;
    match open_existing(path, access, share, wait) {
        Ok(Some(handle)) => check_security(&handle, path, policy).is_ok(),
        _ => false,
    }
}

/// Whether `path` is a directory and not a reparse point. Anything else, an
/// open that fails included, is `false`.
pub(crate) fn is_plain_directory(path: &Path, wait: Duration) -> bool {
    let share = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;
    match open_existing(path, FILE_READ_ATTRIBUTES, share, wait) {
        Ok(Some(handle)) => check_shape(&handle, path, Kind::Directory).is_ok(),
        _ => false,
    }
}

/// Why `read_checked_within` returned no bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Unread {
    /// It could not be opened or read: it may be fine.
    Unreadable(String),
    /// It is there and fails the check.
    Refused(String),
}

/// `read_checked`, waiting on another process no longer than `wait`, and
/// telling a file that fails its check from one that cannot be read.
pub(crate) fn read_checked_within(
    path: &Path,
    policy: &Policy,
    wait: Duration,
) -> Result<Option<Vec<u8>>, Unread> {
    let handle = open_existing(
        path,
        GENERIC_READ | READ_CONTROL | FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ,
        wait,
    )
    .map_err(Unread::Unreadable)?;
    let Some(handle) = handle else {
        return Ok(None);
    };
    let info = file_information(&handle, path).map_err(Unread::Unreadable)?;
    shape_of(&info, path, Kind::File).map_err(Unread::Refused)?;
    let descriptor = security_of(&handle, path).map_err(Unread::Unreadable)?;
    check_descriptor(&descriptor, path, policy).map_err(Unread::Refused)?;
    let mut bytes = Vec::new();
    handle
        .into_file()
        .read_to_end(&mut bytes)
        .map_err(|e| Unread::Unreadable(format!("read {}: {e}", path.display())))?;
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
        OPEN_WAIT,
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
            return Err(failure("delete", temporary, error));
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
        return Err(failure("create", temporary, last_error()));
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
                return Err(failure("replace", path, last_error()));
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
pub(crate) fn security_of(handle: &Handle, path: &Path) -> Result<Descriptor, String> {
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

/// How often a wait past its bound is cancelled again, and how many times,
/// before the waiting thread is left behind.
const CANCEL_INTERVAL: Duration = Duration::from_millis(100);
const CANCEL_TRIES: u32 = 50;

/// What the sharing of `path` refuses, an operation cancelled included
/// (SPEC-024 Amendment 2026-11-05): another process holds it, whether by
/// its share mode or by an opportunistic lock Windows waited on.
fn in_use(path: &Path) -> String {
    format!("{} is in use by another process", path.display())
}

/// A Win32 failure of `verb` on `path`.
fn failure(verb: &str, path: &Path, error: u32) -> String {
    match error {
        ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION | ERROR_OPERATION_ABORTED => in_use(path),
        // ERROR_TRANSACTIONAL_CONFLICT and its family (winerror.h).
        6800..=6899 => format!(
            "{} is held by another process's file transaction (Win32 error {error})",
            path.display()
        ),
        other => format!("{verb} {}: Win32 error {other}", path.display()),
    }
}

/// Run `operation` on a thread of its own and cancel its synchronous I/O
/// after `wait`: an open, a security change, a rename or a deletion that
/// Windows makes wait on another process's opportunistic lock would
/// otherwise wait for as long as that process likes (SPEC-024 Amendment
/// 2026-11-05). The operation sees the cancellation as its own failure. A
/// thread the cancellation cannot reach is left behind, and the object is
/// reported as in use.
pub(crate) fn bounded<T, F>(wait: Duration, path: &Path, operation: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    let (sender, receiver) = mpsc::channel();
    let worker = std::thread::Builder::new()
        .name("cg-custody-io".to_string())
        .spawn(move || {
            let _ = sender.send(operation());
        })
        .map_err(|e| format!("start a thread for {}: {e}", path.display()))?;
    match receiver.recv_timeout(wait) {
        Ok(result) => return result,
        Err(RecvTimeoutError::Disconnected) => {
            return Err(format!("the operation on {} panicked", path.display()))
        }
        Err(RecvTimeoutError::Timeout) => {}
    }
    for _ in 0..CANCEL_TRIES {
        // SAFETY: the thread's handle, which `worker` keeps open. A thread
        // with no I/O under way makes the call fail, which is why it repeats.
        unsafe { CancelSynchronousIo(worker.as_raw_handle() as HANDLE) };
        match receiver.recv_timeout(CANCEL_INTERVAL) {
            Ok(result) => return result,
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
    Err(in_use(path))
}

/// A file's identity on its volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileId {
    volume: u32,
    index: u64,
}

impl std::fmt::Display for FileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:08x}-{:016x}", self.volume, self.index)
    }
}

pub(crate) fn file_id(info: &BY_HANDLE_FILE_INFORMATION) -> FileId {
    FileId {
        volume: info.dwVolumeSerialNumber,
        index: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    }
}

/// Open `path` without following a reparse point, for `access`, sharing
/// `share`, waiting on another process no longer than `wait`. Absent is
/// refused.
pub(crate) fn open_waiting(
    path: &Path,
    access: u32,
    share: u32,
    wait: Duration,
) -> Result<Handle, String> {
    let owned = path.to_path_buf();
    bounded(wait, path, move || match open_raw(&owned, access, share) {
        Ok(Some(handle)) => Ok(handle),
        Ok(None) => Err(format!("{} does not exist", owned.display())),
        Err(error) => Err(failure("open", &owned, error)),
    })
}

/// Open the regular file `path`, sharing read access alone, so that the
/// open fails while any process holds it for writing or deletion, and read
/// it whole through that handle (SPEC-024 Amendment 2026-11-05, item 2.3).
pub(crate) fn read_unshared(
    path: &Path,
    limit: u64,
    wait: Duration,
) -> Result<(Vec<u8>, FileId), String> {
    let owned = path.to_path_buf();
    bounded(wait, path, move || {
        let handle = match open_raw(
            &owned,
            GENERIC_READ | READ_CONTROL | FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ,
        ) {
            Ok(Some(handle)) => handle,
            Ok(None) => return Err(format!("{} does not exist", owned.display())),
            Err(error) => return Err(failure("open", &owned, error)),
        };
        check_shape(&handle, &owned, Kind::File)?;
        let info = file_information(&handle, &owned)?;
        let id = file_id(&info);
        if (u64::from(info.nFileSizeHigh) << 32) | u64::from(info.nFileSizeLow) > limit {
            return Err(format!("{} is larger than {limit} bytes", owned.display()));
        }
        let mut bytes = Vec::new();
        handle
            .into_file()
            .read_to_end(&mut bytes)
            .map_err(|e| format!("read {}: {e}", owned.display()))?;
        Ok((bytes, id))
    })
}

/// Give the object behind `handle` the owner `sddl` names, if any, and its
/// DACL, through the handle: the change applies to that object and
/// propagates to no child, as `SetSecurityInfo`'s would.
pub(crate) fn set_security(
    handle: Handle,
    path: &Path,
    sddl: &str,
    wait: Duration,
) -> Result<Handle, String> {
    let owned = path.to_path_buf();
    let sddl = sddl.to_string();
    bounded(wait, path, move || {
        let descriptor = Descriptor::from_sddl(&sddl)?;
        let mut information = DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION;
        if descriptor.has_owner()? {
            information |= OWNER_SECURITY_INFORMATION;
        }
        // SAFETY: an open handle and a live descriptor.
        if unsafe { SetKernelObjectSecurity(handle.0, information, descriptor.0) } == 0 {
            return Err(failure("change the security of", &owned, last_error()));
        }
        Ok(handle)
    })
}

/// Create the directory `path`, which must not exist, with `sddl` as its
/// security.
pub(crate) fn create_new_directory(path: &Path, sddl: &str) -> Result<(), String> {
    let descriptor = Descriptor::from_sddl(sddl)?;
    let attributes = descriptor.attributes();
    let name = wide(path);
    // SAFETY: the name and attributes live for the call.
    if unsafe { CreateDirectoryW(name.as_ptr(), &attributes) } == 0 {
        return Err(failure("create", path, last_error()));
    }
    Ok(())
}

/// Rename `from` to `to`, never over an existing name.
pub(crate) fn rename(from: &Path, to: &Path, wait: Duration) -> Result<(), String> {
    let (source, target) = (wide(from), wide(to));
    let (owned, other) = (from.to_path_buf(), to.to_path_buf());
    bounded(wait, from, move || {
        // SAFETY: both names live for the call.
        if unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
            return Err(match last_error() {
                // A directory with something open under it, whatever that
                // handle shares, cannot be renamed.
                ERROR_ACCESS_DENIED => format!(
                    "{} or something under it is in use by another process, or Windows denies the rename",
                    owned.display()
                ),
                // Either name can be the one a transaction holds.
                error @ 6800..=6899 => format!(
                    "{} or {} is held by another process's file transaction (Win32 error {error})",
                    owned.display(),
                    other.display()
                ),
                error => failure("rename", &owned, error),
            });
        }
        Ok(())
    })
}

/// Mark the object behind `handle` for deletion, which happens when the
/// handle closes: what is deleted is what was opened and checked, not what
/// its name holds by then. The read-only attribute, which ordinary users may
/// have set, does not keep it, and its name goes with this handle, whatever
/// other handles remain. Where the file system lacks that form, the plain one.
pub(crate) fn delete_through(handle: Handle, path: &Path, wait: Duration) -> Result<(), String> {
    let owned = path.to_path_buf();
    bounded(wait, path, move || {
        // The whole value moves here, and the handle closes with it.
        let handle = handle;
        let extended = FILE_DISPOSITION_INFO_EX {
            Flags: FILE_DISPOSITION_FLAG_DELETE
                | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS
                | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE,
        };
        // SAFETY: an open handle, and a value of the size passed.
        let ok = unsafe {
            SetFileInformationByHandle(
                handle.0,
                FileDispositionInfoEx,
                std::ptr::from_ref(&extended).cast(),
                std::mem::size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
            )
        };
        if ok != 0 {
            return Ok(());
        }
        let error = last_error();
        if !matches!(
            error,
            ERROR_INVALID_PARAMETER | ERROR_INVALID_FUNCTION | ERROR_NOT_SUPPORTED
        ) {
            return Err(failure("delete", &owned, error));
        }
        let plain = FILE_DISPOSITION_INFO { DeleteFile: 1 };
        // SAFETY: an open handle, and a value of the size passed.
        let ok = unsafe {
            SetFileInformationByHandle(
                handle.0,
                FileDispositionInfo,
                std::ptr::from_ref(&plain).cast(),
                std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
            )
        };
        if ok == 0 {
            return Err(failure("delete", &owned, last_error()));
        }
        Ok(())
    })
}

/// The entries of the directory `path`, by name; a name that is not valid
/// Unicode comes out with a replacement character, which matches no name of
/// the product's.
pub(crate) fn entries(path: &Path, wait: Duration) -> Result<Vec<String>, String> {
    let owned = path.to_path_buf();
    bounded(wait, path, move || {
        let reader =
            std::fs::read_dir(&owned).map_err(|e| format!("list {}: {e}", owned.display()))?;
        reader
            .map(|entry| {
                entry
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .map_err(|e| format!("list {}: {e}", owned.display()))
            })
            .collect()
    })
}

/// `FILE_CASE_SENSITIVE_INFO`, which windows-sys does not bind.
#[repr(C)]
struct CaseSensitiveInfo {
    flags: u32,
}

const FILE_CS_FLAG_CASE_SENSITIVE_DIR: u32 = 1;

/// Whether the directory behind `handle` has case-sensitive lookup on. A
/// file system or a Windows without the setting has it off.
pub(crate) fn is_case_sensitive(handle: &Handle, path: &Path) -> Result<bool, String> {
    let mut info = CaseSensitiveInfo { flags: 0 };
    // SAFETY: an open handle and an out-value of the size passed.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            handle.0,
            FileCaseSensitiveInfo,
            std::ptr::from_mut(&mut info).cast(),
            std::mem::size_of::<CaseSensitiveInfo>() as u32,
        )
    };
    if ok != 0 {
        return Ok(info.flags & FILE_CS_FLAG_CASE_SENSITIVE_DIR != 0);
    }
    match last_error() {
        ERROR_INVALID_PARAMETER | ERROR_INVALID_FUNCTION | ERROR_NOT_SUPPORTED => Ok(false),
        error => Err(format!(
            "read the case sensitivity of {}: Win32 error {error}",
            path.display()
        )),
    }
}

/// The host's name, as Windows reports it.
pub fn hostname() -> Result<String, String> {
    let mut size = 0u32;
    // SAFETY: a size query: a null buffer of size 0.
    unsafe {
        GetComputerNameExW(
            ComputerNamePhysicalDnsHostname,
            std::ptr::null_mut(),
            &mut size,
        )
    };
    let error = last_error();
    if error != ERROR_MORE_DATA {
        return Err(format!("read the host name: Win32 error {error}"));
    }
    let mut buffer = vec![0u16; size as usize];
    // SAFETY: a buffer of `size` wide characters, the size passed.
    if unsafe {
        GetComputerNameExW(
            ComputerNamePhysicalDnsHostname,
            buffer.as_mut_ptr(),
            &mut size,
        )
    } == 0
    {
        return Err(format!("read the host name: Win32 error {}", last_error()));
    }
    String::from_utf16(&buffer[..size as usize])
        .map_err(|_| "the host name is not valid Unicode".to_string())
}
