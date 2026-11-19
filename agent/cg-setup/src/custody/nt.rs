//! The custody verb's primitives (SPEC-024 Amendment 2026-11-19, points 2
//! and 3): every object opened through the native API, a root by its NT path,
//! whose components above the last are resolved as Windows resolves them (the
//! caller opens from a drive's root, or checks the final path), and every
//! other object by one name relative to a handle on its parent; the object
//! itself never followed when it is a reparse point, and never opened with
//! backup intent; each act
//! made through the handle of the one object it acts on; each call cancelled
//! after `WAIT`, as the Amendment 2026-11-05's are.
//!
//! An act on an entry asks `DELETE` and `FILE_READ_ATTRIBUTES` alone
//! (`ACT`), which the parent's `FILE_DELETE_CHILD` and `FILE_LIST_DIRECTORY`
//! grant on an object whose own DACL admits no one. Such a handle has no
//! `SYNCHRONIZE`, so it is opened for asynchronous I/O; the calls made
//! through it, a rename, a deletion and the reading of its information, wait
//! for their own completion, and a listing, which would not, is refused on
//! it. A directory is held (`HOLD`) with `FILE_LIST_DIRECTORY`, so that its
//! share mode binds, sharing reading and writing and no deletion: no other
//! handle renames or deletes it while it is held, and the hold is refused
//! while another handle may. A hold does not keep its entries' names: an
//! account with `DELETE` on an entry, or the parent's `FILE_DELETE_CHILD`,
//! may rename or delete it; only the parent's closed DACL stops that, and a
//! handle opened before the close keeps its access.
//!
//! An open that another process's opportunistic lock would make wait
//! completes at once instead (`FILE_COMPLETE_IF_OPLOCKED`) and is refused as
//! in use, having done nothing (`Failure::Oplocked`). A call that outlives
//! `WAIT` is cancelled; one the cancellation cannot reach is left to its
//! thread, which keeps the object open, and the caller cannot tell whether it
//! acted (`Failure::Unfinished`). Nothing here
//! enables a privilege or writes a security descriptor anywhere but through
//! the handle of the one object it is for; which object that may be, and
//! which owner, is the caller's to check.

use std::fmt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::path::{Component, Path, Prefix};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cg_agent::custody::{self, Descriptor, Handle};
use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
use windows_sys::Wdk::Storage::FileSystem::{
    FileAttributeTagInformation, FileDispositionInformationEx, FileFsAttributeInformation,
    FileFsVolumeInformation, FileHardLinkInformation, FileIdBothDirectoryInformation,
    FileInternalInformation, FileRenameInformation, FileStandardInformation, NtCreateFile,
    NtQueryDirectoryFile, NtQueryInformationFile, NtQueryVolumeInformationFile,
    NtSetInformationFile, FILE_COMPLETE_IF_OPLOCKED, FILE_CREATE, FILE_DIRECTORY_FILE,
    FILE_DISPOSITION_DELETE, FILE_DISPOSITION_INFORMATION_EX, FILE_DISPOSITION_POSIX_SEMANTICS,
    FILE_FS_ATTRIBUTE_INFORMATION, FILE_ID_BOTH_DIR_INFORMATION, FILE_INTERNAL_INFORMATION,
    FILE_LINKS_INFORMATION, FILE_LINK_ENTRY_INFORMATION, FILE_NON_DIRECTORY_FILE, FILE_OPEN,
    FILE_OPEN_REPARSE_POINT, FILE_RENAME_INFORMATION, FILE_STANDARD_INFORMATION,
    FILE_SYNCHRONOUS_IO_NONALERT,
};
use windows_sys::Wdk::System::SystemServices::{
    FILE_ATTRIBUTE_TAG_INFORMATION, FILE_FS_VOLUME_INFORMATION,
};
use windows_sys::Win32::Foundation::{
    HANDLE, NTSTATUS, STATUS_BUFFER_OVERFLOW, STATUS_CANCELLED, STATUS_INVALID_PARAMETER,
    STATUS_NO_MORE_FILES, STATUS_OBJECT_NAME_INVALID, STATUS_OPLOCK_BREAK_IN_PROGRESS,
    STATUS_PENDING, STATUS_SHARING_VIOLATION, UNICODE_STRING,
};
use windows_sys::Win32::Security::{
    GetSecurityDescriptorControl, GetSecurityDescriptorDacl, GetSecurityDescriptorOwner,
    SetKernelObjectSecurity, ACL, DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
    PROTECTED_DACL_SECURITY_INFORMATION, PSID, SE_DACL_PROTECTED,
};
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_ATTRIBUTE_DIRECTORY,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, READ_CONTROL, SYNCHRONIZE,
};
use windows_sys::Win32::System::IO::{CancelSynchronousIo, IO_STATUS_BLOCK};

/// How long a call may wait on another process before it is cancelled.
pub const WAIT: Duration = custody::OPEN_WAIT;

/// What a held directory is opened with.
pub const HOLD: u32 = FILE_LIST_DIRECTORY | READ_CONTROL | FILE_READ_ATTRIBUTES | SYNCHRONIZE;
/// A held directory shares reading and writing, and no deletion.
pub const HOLD_SHARE: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE;
/// What an act on an entry opens it with.
pub const ACT: u32 = DELETE | FILE_READ_ATTRIBUTES;
/// An entry acted on shares everything, so that another process's open
/// handle does not make the verb's open fail.
pub const SHARE_ALL: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;
/// What a directory the verb creates is opened with: a hold that may also
/// receive what the verb moves into it.
const CREATED: u32 = HOLD | FILE_ADD_FILE | FILE_ADD_SUBDIRECTORY;
/// The only options a caller may ask for: the kind the object must be.
const KINDS: u32 = FILE_DIRECTORY_FILE | FILE_NON_DIRECTORY_FILE;

/// `IO_REPARSE_TAG_MOUNT_POINT`: a junction or a volume mount point.
pub const MOUNT_POINT: u32 = 0xA000_0003;
/// `STATUS_NOT_A_DIRECTORY`, for a hold of what is not a plain directory.
pub const STATUS_NOT_A_PLAIN_DIRECTORY: NTSTATUS = 0xC000_0103_u32 as NTSTATUS;

/// How often a call past `WAIT` is cancelled again, and how many times,
/// before its thread is left behind (as `custody::bounded`).
const CANCEL_INTERVAL: Duration = Duration::from_millis(100);
const CANCEL_TRIES: u32 = 50;

/// Why a call failed: an NT status, or a Win32 error, each printed as its
/// number alone (point 10); a call that did not finish within `WAIT`; or
/// the verb's own fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    Status(NTSTATUS),
    Win32(u32),
    /// An open of an existing object that another process's opportunistic
    /// lock would have made wait, refused at once: the handle it got is
    /// closed, and the other process is told to break its lock; nothing is
    /// changed. A creation that meets one is `Unfinished`.
    Oplocked,
    /// A call cancelled after `WAIT`: for an act, whether it acted is
    /// unknown.
    Unfinished,
    /// The call's thread could not start, or it panicked.
    Internal,
}

impl Failure {
    /// Whether another process's open, lock or pending deletion refused it
    /// (point 3: "in use").
    pub fn in_use(self) -> bool {
        matches!(
            self,
            Failure::Oplocked
                | Failure::Unfinished
                | Failure::Status(
                    STATUS_SHARING_VIOLATION | STATUS_CANCELLED | STATUS_DELETE_PENDING
                )
                | Failure::Win32(32 | 33)
        )
    }
}

/// `STATUS_DELETE_PENDING`: the object's name is going with another handle.
pub const STATUS_DELETE_PENDING: NTSTATUS = 0xC000_0056_u32 as NTSTATUS;

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::Status(status) => write!(f, "NTSTATUS 0x{:08X}", *status as u32),
            Failure::Win32(error) => write!(f, "Win32 error {error}"),
            Failure::Oplocked => write!(f, "in use: another process holds a lock on it"),
            Failure::Unfinished => write!(
                f,
                "in use: the call did not finish within {} seconds",
                WAIT.as_secs()
            ),
            Failure::Internal => write!(f, "the call's thread failed"),
        }
    }
}

/// A handle shared with the worker thread of a bounded call, which keeps it
/// open until that thread ends, even when the call is abandoned.
struct Shared {
    handle: Handle,
    /// Opened for synchronous I/O, which a listing needs.
    synchronous: bool,
    /// A directory's listing position belongs to the handle: one listing at
    /// a time.
    listing: Mutex<()>,
}

// SAFETY: a file handle may be used from any thread at once; a listing,
// whose position the handle keeps, takes `listing` first.
unsafe impl Sync for Shared {}

/// An object opened here, closed when its last clone is dropped.
#[derive(Clone)]
pub struct Object(Arc<Shared>);

impl Object {
    fn raw(&self) -> HANDLE {
        self.0.handle.as_raw_handle() as HANDLE
    }

    /// The handle, for the reading of its security
    /// (`cg_agent::custody::security_of`).
    pub fn handle(&self) -> &Handle {
        &self.0.handle
    }
}

/// Run `call` on a thread of its own, cancelling its synchronous I/O after
/// `WAIT` (as `custody::bounded`, whose failures are text).
fn within<T, F>(call: F) -> Result<T, Failure>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, Failure> + Send + 'static,
{
    let (sender, receiver) = mpsc::channel();
    let worker = std::thread::Builder::new()
        .name("cg-custody-nt".to_string())
        .spawn(move || {
            let _ = sender.send(call());
        })
        .map_err(|_| Failure::Internal)?;
    match receiver.recv_timeout(WAIT) {
        Ok(result) => return result,
        Err(RecvTimeoutError::Disconnected) => return Err(Failure::Internal),
        Err(RecvTimeoutError::Timeout) => {}
    }
    for _ in 0..CANCEL_TRIES {
        // SAFETY: the thread's handle, which `worker` keeps open. A thread
        // with no I/O under way makes the call fail, which is why it repeats.
        unsafe { CancelSynchronousIo(worker.as_raw_handle() as HANDLE) };
        match receiver.recv_timeout(CANCEL_INTERVAL) {
            Ok(result) => return result,
            Err(RecvTimeoutError::Disconnected) => return Err(Failure::Internal),
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
    Err(Failure::Unfinished)
}

/// A call that left a request pending on a handle the caller does not wait
/// on: the kernel may still write `memory`, so it is never freed. The
/// queries and settings made here complete before they return, so this is
/// a guard, not a path.
fn pending<T>(memory: T) -> Failure {
    std::mem::forget(memory);
    // Whether a pending act completes later is unknown.
    Failure::Unfinished
}

/// The descriptor `sddl` gives, with a DACL that is present, not NULL, which
/// would admit everyone, and protected, which takes nothing from a parent.
fn checked_descriptor(sddl: &str) -> Result<(Descriptor, bool), Failure> {
    let descriptor = Descriptor::from_sddl(sddl).map_err(|_| Failure::Win32(1336))?;
    let mut present = 0;
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let mut owner: PSID = std::ptr::null_mut();
    let mut defaulted = 0;
    let mut control = 0u16;
    let mut revision = 0u32;
    // SAFETY: a live descriptor; the pointers point into it.
    let read = unsafe {
        GetSecurityDescriptorDacl(descriptor.as_ptr(), &mut present, &mut dacl, &mut defaulted) != 0
            && GetSecurityDescriptorOwner(descriptor.as_ptr(), &mut owner, &mut defaulted) != 0
            && GetSecurityDescriptorControl(descriptor.as_ptr(), &mut control, &mut revision) != 0
    };
    if !read || present == 0 || dacl.is_null() || control & SE_DACL_PROTECTED == 0 {
        return Err(Failure::Win32(1336));
    }
    Ok((descriptor, !owner.is_null()))
}

/// `path`, absolute on a drive, as an NT name in the global namespace,
/// `\GLOBAL??\C:\...`; `None` for any other path (a share, a device, a
/// relative path).
pub fn nt_name(path: &Path) -> Option<Vec<u16>> {
    use std::os::windows::ffi::OsStrExt;
    let mut components = path.components();
    let skip = match components.next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(_) => 0,
            Prefix::VerbatimDisk(_) => 4,
            _ => return None,
        },
        _ => return None,
    };
    if components.next() != Some(Component::RootDir) {
        return None;
    }
    let mut name: Vec<u16> = r"\GLOBAL??\".encode_utf16().collect();
    name.extend(path.as_os_str().encode_wide().skip(skip));
    Some(name)
}

/// A name of one component: not empty, not `.` or `..`, with no separator,
/// stream mark or NUL.
fn one_component(name: &[u16]) -> Result<(), Failure> {
    let dot = u16::from(b'.');
    let bad = name.is_empty()
        || name == [dot]
        || name == [dot, dot]
        || name
            .iter()
            .any(|unit| [0, u16::from(b'\\'), u16::from(b'/'), u16::from(b':')].contains(unit));
    if bad {
        return Err(Failure::Status(STATUS_OBJECT_NAME_INVALID));
    }
    Ok(())
}

/// `NtCreateFile`: `name`, one component, relative to `root`, or an NT path
/// when there is no root. Every open is made without following a reparse
/// point, and completes rather than wait on another process's opportunistic
/// lock; the caller chooses only the kind.
fn create(
    root: Option<Object>,
    name: Vec<u16>,
    access: u32,
    share: u32,
    disposition: u32,
    options: u32,
    sddl: Option<String>,
) -> Result<Object, Failure> {
    if options & !KINDS != 0 {
        return Err(Failure::Status(STATUS_INVALID_PARAMETER));
    }
    if root.is_some() {
        one_component(&name)?;
    }
    within(move || {
        let length = u16::try_from(name.len() * 2)
            .map_err(|_| Failure::Status(STATUS_OBJECT_NAME_INVALID))?;
        let unicode = UNICODE_STRING {
            Length: length,
            MaximumLength: length,
            Buffer: name.as_ptr().cast_mut(),
        };
        let descriptor = match &sddl {
            Some(sddl) => Some(checked_descriptor(sddl)?.0),
            None => None,
        };
        let attributes = OBJECT_ATTRIBUTES {
            Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
            RootDirectory: root.as_ref().map_or(std::ptr::null_mut(), Object::raw),
            ObjectName: &unicode,
            // Not inheritable. OBJ_CASE_INSENSITIVE is not asked, but Windows'
            // default makes the lookup ignore case outside a case-sensitive
            // directory: the caller compares the file ID it listed.
            Attributes: 0,
            SecurityDescriptor: descriptor.as_ref().map_or(std::ptr::null(), |descriptor| {
                descriptor.as_ptr().cast_const()
            }),
            SecurityQualityOfService: std::ptr::null(),
        };
        let synchronous = access & SYNCHRONIZE != 0;
        let synchronous_option = if synchronous {
            FILE_SYNCHRONOUS_IO_NONALERT
        } else {
            0
        };
        let mut handle: HANDLE = std::ptr::null_mut();
        // SAFETY: an all-zero IO_STATUS_BLOCK is a valid out-value.
        let mut io: IO_STATUS_BLOCK = unsafe { std::mem::zeroed() };
        // SAFETY: the name, the attributes and the descriptor live for the
        // call; the handle it returns is owned below.
        let status = unsafe {
            NtCreateFile(
                &mut handle,
                access,
                &attributes,
                &mut io,
                std::ptr::null(),
                0,
                share,
                disposition,
                options | FILE_OPEN_REPARSE_POINT | FILE_COMPLETE_IF_OPLOCKED | synchronous_option,
                std::ptr::null(),
                0,
            )
        };
        if status < 0 {
            return Err(Failure::Status(status));
        }
        // SAFETY: a handle the call opened, owned from here on.
        let owned = unsafe { OwnedHandle::from_raw_handle(handle as RawHandle) };
        let object = Object(Arc::new(Shared {
            handle: Handle::from(owned),
            synchronous,
            listing: Mutex::new(()),
        }));
        if status == STATUS_OPLOCK_BREAK_IN_PROGRESS {
            return Err(if disposition == FILE_CREATE {
                Failure::Unfinished
            } else {
                Failure::Oplocked
            });
        }
        Ok(object)
    })
}

/// Open the object at the NT path `name` (`nt_name`).
pub fn open_path(name: &[u16], access: u32, share: u32, options: u32) -> Result<Object, Failure> {
    create(None, name.to_vec(), access, share, FILE_OPEN, options, None)
}

/// Open the entry `name` of the directory `parent`, by that one name.
pub fn open_child(
    parent: &Object,
    name: &[u16],
    access: u32,
    share: u32,
    options: u32,
) -> Result<Object, Failure> {
    create(
        Some(parent.clone()),
        name.to_vec(),
        access,
        share,
        FILE_OPEN,
        options,
        None,
    )
}

/// Hold the directory `name` of `parent`: a directory, not a reparse point.
pub fn hold(parent: &Object, name: &[u16]) -> Result<Object, Failure> {
    let object = open_child(parent, name, HOLD, HOLD_SHARE, FILE_DIRECTORY_FILE)?;
    let info = info(&object)?;
    if info.attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 || !info.directory {
        return Err(Failure::Status(STATUS_NOT_A_PLAIN_DIRECTORY));
    }
    Ok(object)
}

/// Create the directory `name` in `parent`, new, with `sddl` as its security
/// from its creation, and hold it.
pub fn create_directory(parent: &Object, name: &[u16], sddl: &str) -> Result<Object, Failure> {
    create(
        Some(parent.clone()),
        name.to_vec(),
        CREATED,
        HOLD_SHARE,
        FILE_CREATE,
        FILE_DIRECTORY_FILE,
        Some(sddl.to_string()),
    )
}

/// An entry of a directory, as its listing gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: u64,
    /// The name as the directory stores it.
    pub name: Vec<u16>,
    pub attributes: u32,
    /// The reparse tag, for a reparse point.
    pub reparse_tag: u32,
}

impl Entry {
    pub fn is_directory(&self) -> bool {
        self.attributes & FILE_ATTRIBUTE_DIRECTORY != 0
    }

    pub fn is_reparse_point(&self) -> bool {
        self.attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
}

/// The offset of `FILE_ID_BOTH_DIR_INFORMATION`'s name.
const DIRECTORY_NAME: usize = std::mem::offset_of!(FILE_ID_BOTH_DIR_INFORMATION, FileName);

/// The entries of a held directory, without `.` and `..`, in the
/// directory's order. A handle opened without `SYNCHRONIZE` is refused: the
/// query would complete after this call returned, into its buffer.
pub fn list(directory: &Object) -> Result<Vec<Entry>, Failure> {
    if !directory.0.synchronous {
        return Err(Failure::Status(STATUS_INVALID_PARAMETER));
    }
    let directory = directory.clone();
    within(move || {
        let _listing = directory.0.listing.lock().map_err(|_| Failure::Internal)?;
        let mut entries = Vec::new();
        let mut buffer = vec![0u64; 8192];
        let mut restart = 1u8;
        loop {
            // SAFETY: an all-zero IO_STATUS_BLOCK is a valid out-value.
            let mut io: IO_STATUS_BLOCK = unsafe { std::mem::zeroed() };
            // SAFETY: a handle opened for synchronous I/O and a buffer of the
            // length passed.
            let status = unsafe {
                NtQueryDirectoryFile(
                    directory.raw(),
                    std::ptr::null_mut(),
                    None,
                    std::ptr::null(),
                    &mut io,
                    buffer.as_mut_ptr().cast(),
                    (buffer.len() * 8) as u32,
                    FileIdBothDirectoryInformation,
                    0,
                    std::ptr::null(),
                    restart,
                )
            };
            restart = 0;
            if status == STATUS_NO_MORE_FILES {
                return Ok(entries);
            }
            if status < 0 || status == STATUS_PENDING {
                return Err(Failure::Status(status));
            }
            // What the call wrote, which no entry may pass.
            let filled = io.Information.min(buffer.len() * 8);
            if filled == 0 {
                return Err(Failure::Status(STATUS_INVALID_PARAMETER));
            }
            let bytes = buffer.as_ptr().cast::<u8>();
            let mut offset = 0usize;
            loop {
                if offset + std::mem::size_of::<FILE_ID_BOTH_DIR_INFORMATION>() > buffer.len() * 8
                    || offset + DIRECTORY_NAME > filled
                {
                    return Err(Failure::Status(STATUS_INVALID_PARAMETER));
                }
                // SAFETY: the whole structure lies inside the buffer and its
                // fixed part inside what the call wrote, at an 8-aligned offset
                // (checked above and below).
                let header = unsafe { &*bytes.add(offset).cast::<FILE_ID_BOTH_DIR_INFORMATION>() };
                let length = header.FileNameLength as usize;
                if !length.is_multiple_of(2) || offset + DIRECTORY_NAME + length > filled {
                    return Err(Failure::Status(STATUS_INVALID_PARAMETER));
                }
                // SAFETY: the name's bytes lie inside what the call wrote.
                let name: Vec<u16> = unsafe {
                    let start = bytes.add(offset + DIRECTORY_NAME).cast::<u16>();
                    (0..length / 2)
                        .map(|i| start.add(i).read_unaligned())
                        .collect()
                };
                let attributes = header.FileAttributes;
                let dot = u16::from(b'.');
                if name != [dot] && name != [dot, dot] {
                    entries.push(Entry {
                        id: header.FileId as u64,
                        name,
                        attributes,
                        reparse_tag: if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                            header.EaSize
                        } else {
                            0
                        },
                    });
                }
                let next = header.NextEntryOffset as usize;
                if next == 0 {
                    break;
                }
                if !next.is_multiple_of(8) {
                    return Err(Failure::Status(STATUS_INVALID_PARAMETER));
                }
                offset += next;
            }
        }
    })
}

/// What an open object is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Info {
    pub id: u64,
    pub links: u32,
    pub directory: bool,
    pub attributes: u32,
    pub reparse_tag: u32,
}

/// Query `class` into a value of type `T`.
fn query<T: Copy>(object: &Object, class: i32) -> Result<T, Failure> {
    // SAFETY: the information classes queried here are plain data, for which
    // all-zero is a valid value; both live on the heap, so that a request
    // left pending writes memory never freed (`pending`).
    let mut value: Box<T> = Box::new(unsafe { std::mem::zeroed() });
    // SAFETY: as above.
    let mut io: Box<IO_STATUS_BLOCK> = Box::new(unsafe { std::mem::zeroed() });
    // SAFETY: an open handle and an out-value of the size passed.
    let status = unsafe {
        NtQueryInformationFile(
            object.raw(),
            &mut *io,
            std::ptr::from_mut(&mut *value).cast(),
            std::mem::size_of::<T>() as u32,
            class,
        )
    };
    if status == STATUS_PENDING {
        return Err(pending((value, io)));
    }
    if status < 0 {
        return Err(Failure::Status(status));
    }
    Ok(*value)
}

/// The object's file ID, link count, kind, attributes and reparse tag,
/// through a handle with `FILE_READ_ATTRIBUTES`.
pub fn info(object: &Object) -> Result<Info, Failure> {
    let object = object.clone();
    within(move || {
        let internal: FILE_INTERNAL_INFORMATION = query(&object, FileInternalInformation)?;
        let standard: FILE_STANDARD_INFORMATION = query(&object, FileStandardInformation)?;
        let tag: FILE_ATTRIBUTE_TAG_INFORMATION = query(&object, FileAttributeTagInformation)?;
        Ok(Info {
            id: internal.IndexNumber as u64,
            links: standard.NumberOfLinks,
            directory: standard.Directory != 0,
            attributes: tag.FileAttributes,
            reparse_tag: tag.ReparseTag,
        })
    })
}

/// Query the volume's `class` into a buffer of 512 bytes; a label longer
/// than it only truncates the label.
fn query_volume(object: &Object, class: i32) -> Result<Vec<u64>, Failure> {
    let mut buffer = vec![0u64; 64];
    // SAFETY: an all-zero IO_STATUS_BLOCK is a valid out-value; on the heap,
    // as the buffer, for `pending`.
    let mut io: Box<IO_STATUS_BLOCK> = Box::new(unsafe { std::mem::zeroed() });
    // SAFETY: an open handle and a buffer of the length passed.
    let status = unsafe {
        NtQueryVolumeInformationFile(
            object.raw(),
            &mut *io,
            buffer.as_mut_ptr().cast(),
            (buffer.len() * 8) as u32,
            class,
        )
    };
    if status == STATUS_PENDING {
        return Err(pending((buffer, io)));
    }
    if status < 0 && status != STATUS_BUFFER_OVERFLOW {
        return Err(Failure::Status(status));
    }
    Ok(buffer)
}

/// The serial number and the file system's name of the object's volume.
pub fn volume(object: &Object) -> Result<(u32, String), Failure> {
    let object = object.clone();
    within(move || {
        let buffer = query_volume(&object, FileFsVolumeInformation)?;
        // SAFETY: the call wrote the structure's fixed part at the start.
        let serial =
            unsafe { (*buffer.as_ptr().cast::<FILE_FS_VOLUME_INFORMATION>()).VolumeSerialNumber };
        let buffer = query_volume(&object, FileFsAttributeInformation)?;
        let start = std::mem::offset_of!(FILE_FS_ATTRIBUTE_INFORMATION, FileSystemName);
        // SAFETY: the structure and its name, bounded by the buffer.
        let name = unsafe {
            let header = &*buffer.as_ptr().cast::<FILE_FS_ATTRIBUTE_INFORMATION>();
            let length = (header.FileSystemNameLength as usize / 2).min((512 - start) / 2);
            let text = buffer.as_ptr().cast::<u8>().add(start).cast::<u16>();
            (0..length)
                .map(|i| text.add(i).read_unaligned())
                .collect::<Vec<u16>>()
        };
        Ok((serial, String::from_utf16_lossy(&name)))
    })
}

/// A name of a file: its parent directory's file ID and the name in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub parent: u64,
    pub name: Vec<u16>,
}

/// The offsets of `FILE_LINKS_INFORMATION`'s first entry and of an entry's
/// name.
const FIRST_LINK: usize = std::mem::offset_of!(FILE_LINKS_INFORMATION, Entry);
const LINK_NAME: usize = std::mem::offset_of!(FILE_LINK_ENTRY_INFORMATION, FileName);

/// Every name of the file, through a handle with `FILE_READ_ATTRIBUTES`.
pub fn names(object: &Object) -> Result<Vec<Link>, Failure> {
    let object = object.clone();
    within(move || {
        let mut buffer = vec![0u64; 512];
        loop {
            // SAFETY: an all-zero IO_STATUS_BLOCK is a valid out-value; on the
            // heap, as the buffer, for `pending`.
            let mut io: Box<IO_STATUS_BLOCK> = Box::new(unsafe { std::mem::zeroed() });
            // SAFETY: an open handle and a buffer of the length passed.
            let status = unsafe {
                NtQueryInformationFile(
                    object.raw(),
                    &mut *io,
                    buffer.as_mut_ptr().cast(),
                    (buffer.len() * 8) as u32,
                    FileHardLinkInformation,
                )
            };
            if status == STATUS_PENDING {
                return Err(pending((buffer, io)));
            }
            // SAFETY: the structure's fixed part, at the start.
            let header = unsafe { *buffer.as_ptr().cast::<FILE_LINKS_INFORMATION>() };
            if status == STATUS_BUFFER_OVERFLOW
                && header.BytesNeeded as usize > buffer.len() * 8
                && header.BytesNeeded <= 1 << 24
            {
                buffer = vec![0u64; (header.BytesNeeded as usize).div_ceil(8)];
                continue;
            }
            // An open file whose every name was deleted has none: no name is
            // then known to lie anywhere.
            if status < 0 || status == STATUS_BUFFER_OVERFLOW || header.EntriesReturned == 0 {
                return Err(Failure::Status(if status < 0 {
                    status
                } else {
                    STATUS_DELETE_PENDING
                }));
            }
            let filled = io.Information.min(buffer.len() * 8);
            let bytes = buffer.as_ptr().cast::<u8>();
            let mut links = Vec::new();
            let mut offset = FIRST_LINK;
            for _ in 0..header.EntriesReturned {
                if offset + LINK_NAME > filled {
                    return Err(Failure::Status(STATUS_INVALID_PARAMETER));
                }
                // SAFETY: the entry's fixed part lies inside what the call
                // wrote, at an 8-aligned offset.
                let entry = unsafe { &*bytes.add(offset).cast::<FILE_LINK_ENTRY_INFORMATION>() };
                // In characters.
                let length = entry.FileNameLength as usize;
                if offset + LINK_NAME + 2 * length > filled {
                    return Err(Failure::Status(STATUS_INVALID_PARAMETER));
                }
                // SAFETY: the name lies inside what the call wrote.
                let name = unsafe {
                    let start = bytes.add(offset + LINK_NAME).cast::<u16>();
                    (0..length).map(|i| start.add(i).read_unaligned()).collect()
                };
                links.push(Link {
                    parent: entry.ParentFileId as u64,
                    name,
                });
                let next = entry.NextEntryOffset as usize;
                if next == 0 {
                    break;
                }
                if !next.is_multiple_of(8) {
                    return Err(Failure::Status(STATUS_INVALID_PARAMETER));
                }
                offset += next;
            }
            return Ok(links);
        }
    })
}

/// Set `class` from `value`'s bytes.
fn set(object: &Object, class: i32, value: Vec<u8>) -> Result<(), Failure> {
    // SAFETY: an all-zero IO_STATUS_BLOCK is a valid out-value; on the heap,
    // as the value, for `pending`.
    let mut io: Box<IO_STATUS_BLOCK> = Box::new(unsafe { std::mem::zeroed() });
    // SAFETY: an open handle and a value of the length passed.
    let status = unsafe {
        NtSetInformationFile(
            object.raw(),
            &mut *io,
            value.as_ptr().cast(),
            value.len() as u32,
            class,
        )
    };
    if status == STATUS_PENDING {
        return Err(pending((value, io)));
    }
    if status < 0 {
        return Err(Failure::Status(status));
    }
    Ok(())
}

/// Move the object into the held directory `target` as `name`, one
/// component, never over an existing name: its descriptor, attributes and
/// contents go with it.
pub fn move_into(object: &Object, target: &Object, name: &[u16]) -> Result<(), Failure> {
    one_component(name)?;
    let (object, target, name) = (object.clone(), target.clone(), name.to_vec());
    within(move || {
        let start = std::mem::offset_of!(FILE_RENAME_INFORMATION, FileName);
        let size = (start + name.len() * 2).max(std::mem::size_of::<FILE_RENAME_INFORMATION>());
        let mut buffer = vec![0u64; size.div_ceil(8)];
        // SAFETY: the buffer holds the structure and the name after it.
        unsafe {
            let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
            (*info).Anonymous.ReplaceIfExists = 0;
            (*info).RootDirectory = target.raw();
            (*info).FileNameLength = (name.len() * 2) as u32;
            let text = buffer.as_mut_ptr().cast::<u8>().add(start).cast::<u16>();
            for (index, unit) in name.iter().enumerate() {
                text.add(index).write_unaligned(*unit);
            }
        }
        // SAFETY: the buffer's first `size` bytes.
        let bytes = unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), size) };
        set(&object, FileRenameInformation, bytes.to_vec())
    })
}

/// Delete the object's name through its handle, POSIX-style: the name goes
/// when this handle closes, which the caller does by dropping its last
/// clone, whatever other handles remain. A read-only file, whose attribute
/// such a handle cannot clear, and a directory that is not empty refuse.
pub fn dispose(object: &Object) -> Result<(), Failure> {
    let object = object.clone();
    within(move || {
        let flags = FILE_DISPOSITION_INFORMATION_EX {
            Flags: FILE_DISPOSITION_DELETE | FILE_DISPOSITION_POSIX_SEMANTICS,
        };
        // SAFETY: a plain-data value, read as its bytes.
        let bytes = unsafe {
            std::slice::from_raw_parts(
                std::ptr::from_ref(&flags).cast::<u8>(),
                std::mem::size_of::<FILE_DISPOSITION_INFORMATION_EX>(),
            )
        };
        set(&object, FileDispositionInformationEx, bytes.to_vec())
    })
}

/// Write the security `sddl` gives through the object's handle: its DACL,
/// protected, which it must hold, and its owner when it names one. No other
/// object receives it.
pub fn set_security(object: &Object, sddl: &str) -> Result<(), Failure> {
    let (object, sddl) = (object.clone(), sddl.to_string());
    within(move || {
        let (descriptor, has_owner) = checked_descriptor(&sddl)?;
        let mut information = DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION;
        if has_owner {
            information |= OWNER_SECURITY_INFORMATION;
        }
        // SAFETY: an open handle and a live descriptor.
        if unsafe { SetKernelObjectSecurity(object.raw(), information, descriptor.as_ptr()) } == 0 {
            return Err(Failure::Win32(custody::last_error()));
        }
        Ok(())
    })
}

/// Write the object's owner alone, the SID `sid` gives in its text form,
/// through its handle.
pub fn set_owner(object: &Object, sid: &str) -> Result<(), Failure> {
    let (object, sddl) = (object.clone(), format!("O:{sid}"));
    within(move || {
        let descriptor = Descriptor::from_sddl(&sddl).map_err(|_| Failure::Win32(1337))?;
        // SAFETY: an open handle and a live descriptor.
        if unsafe {
            SetKernelObjectSecurity(
                object.raw(),
                OWNER_SECURITY_INFORMATION,
                descriptor.as_ptr(),
            )
        } == 0
        {
            return Err(Failure::Win32(custody::last_error()));
        }
        Ok(())
    })
}
