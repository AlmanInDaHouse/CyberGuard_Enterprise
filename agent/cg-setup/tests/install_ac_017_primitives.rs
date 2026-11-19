//! SPEC-024 install_ac_017, the primitives (its Amendment 2026-11-19, points
//! 2 and 3, and *Tests*): unelevated, over temporary roots, the current user's
//! SID in the places of the accounts each fixture names. Each object a test
//! makes unreachable keeps its owner, the test's own account, whose implicit
//! `WRITE_DAC` resets it before the root is removed.
//!
//! An open asking `DELETE` and `FILE_READ_ATTRIBUTES` alone, by one name
//! relative to a held parent, without following a reparse point, is granted
//! by the parent on an object whose DACL admits no one; a rename through it
//! into a held directory keeps the object's ID, stored descriptor,
//! attributes and contents, a junction moved as a link, never over a name; a
//! deletion through it removes a file or a junction and refuses a read-only
//! file and a directory that is not empty; a close written through the
//! directory's handle leaves its children's stored DACLs as they were, where
//! `SetNamedSecurityInfoW` rewrites them; a file's names come with their
//! parents' IDs through `FILE_READ_ATTRIBUTES` alone; a held directory
//! refuses an open for deletion and is refused while another handle may
//! delete it; and names another account could choose are acted on exactly.
#![cfg(windows)]

mod fixtures;

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

use cg_setup::custody::nt::{
    self, Failure, Link, Object, ACT, HOLD, HOLD_SHARE, MOUNT_POINT, SHARE_ALL,
};
use windows_sys::Wdk::Storage::FileSystem::FILE_DIRECTORY_FILE;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_ACCESS_DENIED, ERROR_IO_PENDING,
    ERROR_SHARING_VIOLATION, GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE,
    STATUS_ACCESS_DENIED, STATUS_CANNOT_DELETE, STATUS_DIRECTORY_NOT_EMPTY,
    STATUS_INVALID_PARAMETER, STATUS_OBJECT_NAME_COLLISION, STATUS_OBJECT_NAME_INVALID,
    STATUS_SHARING_VIOLATION,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW,
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SetNamedSecurityInfoW, SDDL_REVISION_1,
    SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    GetFileSecurityW, GetKernelObjectSecurity, GetSecurityDescriptorDacl, SetFileSecurityW, ACL,
    DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateDirectoryW, CreateFileW, CreateHardLinkW, GetFileAttributesW, SetFileAttributesW,
    WriteFile, CREATE_NEW, DELETE, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_READONLY,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_FLAG_OVERLAPPED, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, INVALID_FILE_ATTRIBUTES, OPEN_EXISTING, READ_CONTROL,
    WRITE_DAC, WRITE_OWNER,
};
use windows_sys::Win32::System::IO::{DeviceIoControl, OVERLAPPED};

/// `FSCTL_REQUEST_BATCH_OPLOCK` (winioctl.h).
const FSCTL_REQUEST_BATCH_OPLOCK: u32 = 0x0009_0008;

/// `\\?\` and `path`, then `name`'s units, NUL-terminated.
fn verbatim(path: &Path, name: &[u16]) -> Vec<u16> {
    let mut units: Vec<u16> = r"\\?\".encode_utf16().collect();
    units.extend(path.as_os_str().encode_wide());
    if !name.is_empty() {
        units.push(u16::from(b'\\'));
        units.extend_from_slice(name);
    }
    units.push(0);
    units
}

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

fn me() -> String {
    fixtures::sid()
}

/// A protected DACL that admits no one: the owner keeps its implicit rights.
const NOBODY: &str = "D:P";

/// A temporary root, each of whose objects is reset to the test's account
/// before it is removed.
struct Root {
    dir: tempfile::TempDir,
}

impl Root {
    fn new() -> Self {
        Root {
            dir: tempfile::tempdir().expect("temporary root"),
        }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path().join(name)
    }

    /// Hold the root itself.
    fn hold(&self) -> Object {
        nt::open_path(
            &nt::nt_name(self.path()).expect("a drive path"),
            HOLD,
            HOLD_SHARE,
            FILE_DIRECTORY_FILE,
        )
        .expect("hold the root")
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        reset_tree(self.path());
    }
}

/// Give `path` and everything under it the test's account, not following a
/// reparse point, and clear the read-only attribute.
fn reset_tree(path: &Path) {
    let wide = verbatim(path, &[]);
    // SetFileSecurityW by name would write a junction's target: a link is
    // removed through its parent's grant, its own DACL whatever it is.
    if attributes(path) & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return;
    }
    set_dacl_units(&wide, &format!("D:(A;OICI;FA;;;{})", me()));
    // SAFETY: a NUL-terminated name.
    unsafe {
        let attributes = GetFileAttributesW(wide.as_ptr());
        if attributes != INVALID_FILE_ATTRIBUTES && attributes & FILE_ATTRIBUTE_READONLY != 0 {
            SetFileAttributesW(wide.as_ptr(), attributes & !FILE_ATTRIBUTE_READONLY);
        }
    }
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return;
    };
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                reset_tree(&entry.path());
            }
        }
    }
}

fn descriptor(sddl: &str) -> PSECURITY_DESCRIPTOR {
    let text: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: the text lives for the call; the caller frees the descriptor.
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(ok, 0, "SDDL {sddl}");
    descriptor
}

/// Write `sddl`'s DACL to the object `wide` names alone: no inheritance is
/// propagated (`SetFileSecurityW`).
fn set_dacl_units(wide: &[u16], sddl: &str) {
    let descriptor = descriptor(sddl);
    // SAFETY: a NUL-terminated name and a live descriptor, freed after.
    unsafe {
        SetFileSecurityW(wide.as_ptr(), DACL_SECURITY_INFORMATION, descriptor);
        LocalFree(descriptor.cast());
    }
}

fn set_dacl(path: &Path, sddl: &str) {
    set_dacl_units(&verbatim(path, &[]), sddl);
}

/// The owner and DACL stored on the object behind `object`, as bytes, read
/// through its handle, which follows no link.
fn stored_through(object: &Object) -> Vec<u8> {
    use std::os::windows::io::AsRawHandle;
    let handle = object.handle().as_raw_handle();
    let information = OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
    let mut needed = 0u32;
    // SAFETY: a size query on an open handle.
    unsafe { GetKernelObjectSecurity(handle, information, std::ptr::null_mut(), 0, &mut needed) };
    let mut buffer = vec![0u8; needed as usize];
    // SAFETY: a buffer of `needed` bytes.
    let ok = unsafe {
        GetKernelObjectSecurity(
            handle,
            information,
            buffer.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    };
    assert_ne!(ok, 0, "GetKernelObjectSecurity");
    buffer
}

/// The owner and DACL `path` stores, as bytes.
fn stored(path: &Path) -> Vec<u8> {
    let wide = verbatim(path, &[]);
    let information = OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
    let mut needed = 0u32;
    // SAFETY: a size query.
    unsafe {
        GetFileSecurityW(
            wide.as_ptr(),
            information,
            std::ptr::null_mut(),
            0,
            &mut needed,
        )
    };
    let mut buffer = vec![0u8; needed as usize];
    // SAFETY: a buffer of `needed` bytes.
    let ok = unsafe {
        GetFileSecurityW(
            wide.as_ptr(),
            information,
            buffer.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    };
    assert_ne!(ok, 0, "GetFileSecurityW {}", path.display());
    buffer
}

/// A stored self-relative descriptor's owner, as bytes.
fn owner_of(stored: &[u8]) -> Vec<u8> {
    let offset = u32::from_le_bytes([stored[4], stored[5], stored[6], stored[7]]) as usize;
    let count = usize::from(stored[offset + 1]);
    stored[offset..offset + 8 + 4 * count].to_vec()
}

/// A stored descriptor's DACL as SDDL.
fn dacl_text(stored: &[u8]) -> String {
    rendered_dacl(stored.as_ptr().cast_mut().cast())
}

/// `sddl`'s DACL as Windows renders it, the form `dacl_text` compares with:
/// SDDL writes some SIDs by alias, the built-in Administrator (RID 500, the
/// Windows runner's account) as `LA` (CLAUDE.md, *Windows CI job*).
fn expected_dacl(sddl: &str) -> String {
    let descriptor = descriptor(sddl);
    let rendered = rendered_dacl(descriptor);
    // SAFETY: the descriptor `descriptor` allocated, freed once.
    unsafe { LocalFree(descriptor.cast()) };
    rendered
}

fn rendered_dacl(descriptor: PSECURITY_DESCRIPTOR) -> String {
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: a self-relative descriptor; `text` is LocalAlloc'd, freed below.
    unsafe {
        let ok = ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor,
            SDDL_REVISION_1,
            DACL_SECURITY_INFORMATION,
            &mut text,
            std::ptr::null_mut(),
        );
        assert_ne!(ok, 0, "render a DACL");
        let len = (0..).take_while(|&i| *text.add(i) != 0).count();
        let rendered = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
        LocalFree(text.cast());
        rendered
    }
}

/// Whether a stored descriptor's DACL is protected (`SE_DACL_PROTECTED`).
fn protected(stored: &[u8]) -> bool {
    u16::from_le_bytes([stored[2], stored[3]]) & 0x1000 != 0
}

/// A new file `name` in `dir`, holding `contents`.
fn file(dir: &Path, name: &[u16], contents: &[u8]) {
    let wide = verbatim(dir, name);
    // SAFETY: a NUL-terminated name; the handle is closed below.
    unsafe {
        let handle = CreateFileW(
            wide.as_ptr(),
            GENERIC_WRITE,
            0,
            std::ptr::null(),
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        );
        assert_ne!(handle, INVALID_HANDLE_VALUE, "create {:?}", name);
        let mut written = 0u32;
        WriteFile(
            handle,
            contents.as_ptr(),
            contents.len() as u32,
            &mut written,
            std::ptr::null_mut(),
        );
        CloseHandle(handle);
    }
}

fn directory(dir: &Path, name: &str) -> PathBuf {
    let wide = verbatim(dir, &units(name));
    // SAFETY: a NUL-terminated name.
    assert_ne!(
        unsafe { CreateDirectoryW(wide.as_ptr(), std::ptr::null()) },
        0,
        "create {name}"
    );
    dir.join(name)
}

fn read_only(path: &Path) {
    let wide = verbatim(path, &[]);
    // SAFETY: a NUL-terminated name.
    assert_ne!(
        unsafe { SetFileAttributesW(wide.as_ptr(), FILE_ATTRIBUTE_READONLY) },
        0
    );
}

fn attributes(path: &Path) -> u32 {
    let wide = verbatim(path, &[]);
    // SAFETY: a NUL-terminated name.
    unsafe { GetFileAttributesW(wide.as_ptr()) }
}

fn exists(path: &Path) -> bool {
    attributes(path) != INVALID_FILE_ATTRIBUTES
}

/// Hard-link `link` to `target`.
fn hard_link(link: &Path, target: &Path) {
    let (link, target) = (verbatim(link, &[]), verbatim(target, &[]));
    // SAFETY: two NUL-terminated names.
    assert_ne!(
        unsafe { CreateHardLinkW(link.as_ptr(), target.as_ptr(), std::ptr::null()) },
        0,
        "CreateHardLinkW"
    );
}

/// `CreateFileW`'s open of `path` for `access`; the error when refused.
fn win32_open(path: &Path, access: u32, share: u32) -> Result<(), u32> {
    let wide = verbatim(path, &[]);
    // SAFETY: a NUL-terminated name; the handle is closed at once.
    unsafe {
        let handle = CreateFileW(
            wide.as_ptr(),
            access,
            share,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        );
        if handle == INVALID_HANDLE_VALUE {
            return Err(GetLastError());
        }
        CloseHandle(handle);
    }
    Ok(())
}

/// The entry of `listing` named exactly `name`.
fn listed<'a>(listing: &'a [nt::Entry], name: &[u16]) -> &'a nt::Entry {
    listing
        .iter()
        .find(|entry| entry.name == name)
        .unwrap_or_else(|| panic!("{} is not listed", String::from_utf16_lossy(name)))
}

/// A quarantine directory created in the root by the primitive, held.
fn quarantine(root: &Object) -> Object {
    nt::create_directory(root, &units("q"), &format!("D:P(A;OICI;FA;;;{})", me()))
        .expect("create the quarantine")
}

/// prim_01: a junction opened by its name, without following it, is the
/// link; moved, it moves as the link, and its target is untouched.
#[test]
fn install_ac_017_prim_01_junction_is_the_link() {
    let root = Root::new();
    let target = directory(root.path(), "t");
    file(&target, &units("canary"), b"canary");
    fixtures::junction(&root.join("j"), &target);
    let held = root.hold();
    let listing = nt::list(&held).expect("list");
    let entry = listed(&listing, &units("j"));
    assert!(entry.is_reparse_point());
    assert_eq!(entry.reparse_tag, MOUNT_POINT);
    assert_eq!(
        nt::hold(&held, &units("j")).err(),
        Some(Failure::Status(nt::STATUS_NOT_A_PLAIN_DIRECTORY)),
        "a junction is not held as a directory"
    );
    // The link's own DACL, written through a handle on the link, admits no
    // one; its target's stays.
    let target_before = stored(&target);
    let security = nt::open_child(
        &held,
        &units("j"),
        READ_CONTROL | WRITE_DAC | FILE_READ_ATTRIBUTES,
        SHARE_ALL,
        0,
    )
    .expect("open the link's security");
    nt::set_security(&security, NOBODY).expect("close the link");
    assert_eq!(
        dacl_text(&stored_through(&security)),
        NOBODY,
        "the link's own DACL, read through its handle"
    );
    drop(security);
    assert_eq!(stored(&target), target_before, "the target's DACL");

    let link = nt::open_child(&held, &units("j"), ACT, SHARE_ALL, 0).expect("open the link");
    let info = nt::info(&link).expect("info");
    assert_eq!(info.id, entry.id, "the listed entry is the one opened");
    assert_eq!(info.reparse_tag, MOUNT_POINT);
    assert_ne!(info.id, listed(&listing, &units("t")).id);

    let q = quarantine(&held);
    nt::move_into(&link, &q, &units("1")).expect("move the junction");
    drop(link);
    assert!(!exists(&root.join("j")));
    let moved = nt::list(&q).expect("list the quarantine");
    let moved = listed(&moved, &units("1"));
    assert_eq!(moved.id, entry.id);
    assert_eq!(moved.reparse_tag, MOUNT_POINT, "moved as a link");
    assert_eq!(
        std::fs::read(target.join("canary")).expect("canary"),
        b"canary"
    );
}

/// prim_02: the parent's grants open an object whose DACL admits no one, for
/// `DELETE` and `FILE_READ_ATTRIBUTES` and nothing else; `CreateFileW`, which
/// asks `SYNCHRONIZE` too, is denied; without the parent's
/// `FILE_DELETE_CHILD` the open is denied.
#[test]
fn install_ac_017_prim_02_parent_grants_the_act() {
    let root = Root::new();
    file(root.path(), &units("x"), b"x");
    set_dacl(&root.join("x"), NOBODY);
    let held = root.hold();
    let object = nt::open_child(&held, &units("x"), ACT, SHARE_ALL, 0).expect("open for the act");
    nt::info(&object).expect("its attributes, through the parent's listing grant");
    assert_eq!(
        win32_open(&root.join("x"), DELETE, SHARE_ALL),
        Err(ERROR_ACCESS_DENIED),
        "CreateFileW asks SYNCHRONIZE, which only the object's DACL grants"
    );

    let parent = directory(root.path(), "p");
    file(&parent, &units("y"), b"y");
    set_dacl(&parent.join("y"), NOBODY);
    // Listing, traversal, attributes, READ_CONTROL and SYNCHRONIZE; no
    // FILE_DELETE_CHILD.
    set_dacl(&parent, &format!("D:P(A;;0x1200a9;;;{})", me()));
    let p = nt::hold(&held, &units("p")).expect("hold the parent");
    assert_eq!(
        nt::open_child(&p, &units("y"), ACT, SHARE_ALL, 0).err(),
        Some(Failure::Status(STATUS_ACCESS_DENIED))
    );
}

/// prim_03: a move keeps the object's ID, stored descriptor, attributes and
/// contents, for a read-only file, a directory and a hard link that admit no
/// one, the link's other name staying; it never replaces a name.
#[test]
fn install_ac_017_prim_03_move_keeps_the_object() {
    let root = Root::new();
    file(root.path(), &units("m"), b"abc");
    read_only(&root.join("m"));
    set_dacl(&root.join("m"), NOBODY);
    let d = directory(root.path(), "d");
    file(&d, &units("inner"), b"inner");
    set_dacl(&d, NOBODY);
    let outside = directory(root.path(), "o");
    file(&outside, &units("target"), b"linked");
    hard_link(&root.join("hl"), &outside.join("target"));
    set_dacl(&root.join("hl"), NOBODY);
    let held = root.hold();
    let q = quarantine(&held);

    for (name, number) in [("m", "1"), ("d", "2"), ("hl", "4")] {
        let before = stored(&root.join(name));
        let object = nt::open_child(&held, &units(name), ACT, SHARE_ALL, 0).expect("open");
        let id = nt::info(&object).expect("info").id;
        nt::move_into(&object, &q, &units(number)).expect("move");
        drop(object);
        assert!(!exists(&root.join(name)), "{name} left its directory");
        let moved = root.join("q").join(number);
        assert_eq!(stored(&moved), before, "{name}'s stored descriptor");
        let listing = nt::list(&q).expect("list");
        assert_eq!(listed(&listing, &units(number)).id, id, "{name}'s ID");
    }
    let file_moved = root.join("q").join("1");
    assert_ne!(attributes(&file_moved) & FILE_ATTRIBUTE_READONLY, 0);
    assert!(
        exists(&outside.join("target")),
        "the link's other name stays"
    );
    reset_tree(&root.join("q"));
    assert_eq!(
        std::fs::read(root.join("q").join("4")).expect("contents"),
        b"linked"
    );
    assert_eq!(std::fs::read(&file_moved).expect("contents"), b"abc");
    assert_eq!(
        std::fs::read(root.join("q").join("2").join("inner")).expect("contents"),
        b"inner"
    );

    // Never over a name.
    file(&root.join("q"), &units("3"), b"there");
    file(root.path(), &units("m2"), b"m2");
    let object = nt::open_child(&held, &units("m2"), ACT, SHARE_ALL, 0).expect("open");
    assert_eq!(
        nt::move_into(&object, &q, &units("3")),
        Err(Failure::Status(STATUS_OBJECT_NAME_COLLISION))
    );
    drop(object);
    assert_eq!(std::fs::read(root.join("m2")).expect("m2"), b"m2");
    assert_eq!(
        std::fs::read(root.join("q").join("3")).expect("3"),
        b"there"
    );
}

/// prim_04: a deletion through the act's handle removes a file and a
/// junction, the junction's target untouched, and refuses a read-only file
/// and a directory that is not empty.
#[test]
fn install_ac_017_prim_04_dispose() {
    let root = Root::new();
    file(root.path(), &units("f"), b"f");
    set_dacl(&root.join("f"), NOBODY);
    file(root.path(), &units("r"), b"r");
    read_only(&root.join("r"));
    set_dacl(&root.join("r"), NOBODY);
    let d = directory(root.path(), "d");
    file(&d, &units("inner"), b"inner");
    let target = directory(root.path(), "t");
    file(&target, &units("canary"), b"canary");
    fixtures::junction(&root.join("j"), &target);
    let held = root.hold();

    for name in ["f", "j"] {
        let object = nt::open_child(&held, &units(name), ACT, SHARE_ALL, 0).expect("open");
        nt::dispose(&object).expect("dispose");
        drop(object);
        assert!(!exists(&root.join(name)), "{name} is gone");
    }
    assert_eq!(
        std::fs::read(target.join("canary")).expect("canary"),
        b"canary"
    );
    let object = nt::open_child(&held, &units("r"), ACT, SHARE_ALL, 0).expect("open");
    assert_eq!(
        nt::dispose(&object),
        Err(Failure::Status(STATUS_CANNOT_DELETE))
    );
    drop(object);
    assert!(exists(&root.join("r")));
    let object = nt::open_child(&held, &units("d"), ACT, SHARE_ALL, 0).expect("open");
    assert_eq!(
        nt::dispose(&object),
        Err(Failure::Status(STATUS_DIRECTORY_NOT_EMPTY))
    );
    drop(object);
    assert!(exists(&d.join("inner")));
}

/// A directory with a file hard-linked from outside it and an unlistable
/// child directory, both of whose DACLs inherit.
fn closable(root: &Root, name: &str, outside: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let dir = directory(root.path(), name);
    file(&dir, &units("h"), b"h");
    let link = outside.join(format!("{name}-h"));
    hard_link(&link, &dir.join("h"));
    let child = directory(&dir, "u");
    set_dacl(
        &child,
        &format!("D:(D;;0x1;;;{me})(A;OICI;FA;;;{me})", me = me()),
    );
    let canary = dir.join("h");
    (dir, canary, child)
}

/// prim_05: a close written through the directory's handle writes that
/// directory alone: a hard-linked canary and an unlistable child keep their
/// stored DACLs byte for byte; the same close by name, through
/// `SetNamedSecurityInfoW`, rewrites the canary's.
#[test]
fn install_ac_017_prim_05_close_by_handle() {
    let root = Root::new();
    let outside = directory(root.path(), "o");
    let (dir, canary, child) = closable(&root, "c", &outside);
    let (twin, twin_canary, _) = closable(&root, "c2", &outside);
    let closed = format!("D:P(A;OICI;FA;;;{})", me());
    let held = root.hold();

    let canary_before = stored(&canary);
    let child_before = stored(&child);
    let object = nt::open_child(
        &held,
        &units("c"),
        READ_CONTROL | WRITE_DAC | FILE_READ_ATTRIBUTES,
        SHARE_ALL,
        FILE_DIRECTORY_FILE,
    )
    .expect("open the directory");
    nt::set_security(&object, &closed).expect("close");
    drop(object);
    assert!(protected(&stored(&dir)), "the directory is closed");
    assert_eq!(
        dacl_text(&stored(&dir)),
        expected_dacl(&closed),
        "with the DACL written"
    );
    assert_eq!(stored(&canary), canary_before, "the hard-linked canary");
    assert_eq!(stored(&child), child_before, "the unlistable child");

    // The contrast: by name.
    let twin_before = stored(&twin_canary);
    let descriptor = descriptor(&closed);
    let mut present = 0;
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let mut defaulted = 0;
    let wide = verbatim(&twin, &[]);
    // SAFETY: a live descriptor and a NUL-terminated name; the descriptor is
    // freed after.
    unsafe {
        GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted);
        let error = SetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            dacl,
            std::ptr::null(),
        );
        LocalFree(descriptor.cast());
        assert_eq!(error, 0, "SetNamedSecurityInfoW");
    }
    assert_ne!(
        stored(&twin_canary),
        twin_before,
        "SetNamedSecurityInfoW rewrites a child's DACL"
    );
}

/// prim_06: names another account could choose are opened, checked and
/// moved exactly, and a decoy that differs from each is untouched.
#[test]
fn install_ac_017_prim_06_raw_names() {
    let root = Root::new();
    let r = directory(root.path(), "r");
    file(&r, &units("key.dat"), b"decoy");
    let mut lone = units("key");
    lone.push(0xD800);
    lone.extend(units(".dat"));
    let names: Vec<Vec<u16>> = vec![
        units("key.dat."),
        units("key.dat "),
        units("key'.dat"),
        units("k&y.dat"),
        units("%key%.dat"),
        units("\u{200F}key.dat"),
        units("key\u{AD}.dat"),
        units("x'+(iex(1))+'"),
        lone,
    ];
    for (index, name) in names.iter().enumerate() {
        file(&r, name, index.to_string().as_bytes());
    }
    let held = root.hold();
    let r_held = nt::hold(&held, &units("r")).expect("hold");
    let q = quarantine(&held);
    let listing = nt::list(&r_held).expect("list");
    let decoy = listed(&listing, &units("key.dat")).id;
    for (index, name) in names.iter().enumerate() {
        let entry = listed(&listing, name);
        let object = nt::open_child(&r_held, &entry.name, ACT, SHARE_ALL, 0).expect("open");
        assert_eq!(nt::info(&object).expect("info").id, entry.id);
        nt::move_into(&object, &q, &units(&(index + 1).to_string())).expect("move");
    }
    let left = nt::list(&r_held).expect("list again");
    assert_eq!(left.len(), 1, "only the decoy is left");
    assert_eq!(left[0].name, units("key.dat"));
    assert_eq!(left[0].id, decoy);
    assert_eq!(std::fs::read(r.join("key.dat")).expect("decoy"), b"decoy");
    for index in 0..names.len() {
        let moved = root.join("q").join((index + 1).to_string());
        assert_eq!(
            std::fs::read(moved).expect("moved"),
            index.to_string().as_bytes()
        );
    }
}

/// prim_07: a file's names, each with its parent's file ID, are read through
/// a handle holding `FILE_READ_ATTRIBUTES` alone, a name in a directory the
/// caller cannot list included.
#[test]
fn install_ac_017_prim_07_names_through_read_attributes() {
    let root = Root::new();
    let a = directory(root.path(), "a");
    let b = directory(root.path(), "b");
    file(&a, &units("n"), b"n");
    hard_link(&b.join("n2"), &a.join("n"));
    let held = root.hold();
    let a_id = nt::info(&nt::hold(&held, &units("a")).expect("a"))
        .expect("info")
        .id;
    let b_id = nt::info(&nt::hold(&held, &units("b")).expect("b"))
        .expect("info")
        .id;
    let a_held = nt::hold(&held, &units("a")).expect("a");
    let expected: BTreeSet<(u64, Vec<u16>)> = [(a_id, units("n")), (b_id, units("n2"))].into();

    let object =
        nt::open_child(&a_held, &units("n"), FILE_READ_ATTRIBUTES, SHARE_ALL, 0).expect("open");
    let read = |object: &Object| -> BTreeSet<(u64, Vec<u16>)> {
        nt::names(object)
            .expect("names")
            .into_iter()
            .map(|Link { parent, name }| (parent, name))
            .collect()
    };
    assert_eq!(read(&object), expected);
    // `b` now lets its owner change its DACL and nothing else.
    set_dacl(&b, NOBODY);
    assert_eq!(read(&object), expected, "through a directory not listable");
}

/// prim_08: a held directory refuses another open for deletion, and so a
/// rename; and it is not held while another handle may delete it.
#[test]
fn install_ac_017_prim_08_hold() {
    let root = Root::new();
    directory(root.path(), "h");
    directory(root.path(), "h2");
    let held = root.hold();
    let h = nt::hold(&held, &units("h")).expect("hold");
    assert_eq!(
        win32_open(&root.join("h"), DELETE, SHARE_ALL),
        Err(ERROR_SHARING_VIOLATION)
    );
    assert!(std::fs::rename(root.join("h"), root.join("h-renamed")).is_err());
    drop(h);
    std::fs::rename(root.join("h"), root.join("h-renamed")).expect("renamed once released");

    let wide = verbatim(&root.join("h2"), &[]);
    // SAFETY: a NUL-terminated name; the handle is closed below.
    let other = unsafe {
        CreateFileW(
            wide.as_ptr(),
            DELETE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(other, INVALID_HANDLE_VALUE);
    let refused = nt::hold(&held, &units("h2")).err();
    // SAFETY: the handle opened above.
    unsafe { CloseHandle(other) };
    assert_eq!(refused, Some(Failure::Status(STATUS_SHARING_VIOLATION)));
    assert!(refused.is_some_and(Failure::in_use));
}

/// A directory created by the primitive has its security from its creation;
/// a hold refuses what is not a plain directory; the root's volume reads.
#[test]
fn install_ac_017_prim_create_hold_volume() {
    let root = Root::new();
    file(root.path(), &units("f"), b"f");
    let held = root.hold();
    let sddl = format!("D:P(A;OICI;FA;;;{})", me());
    drop(quarantine(&held));
    let created = stored(&root.join("q"));
    assert!(protected(&created));
    assert_eq!(
        dacl_text(&created),
        expected_dacl(&sddl),
        "the DACL given at its creation"
    );
    assert_eq!(
        nt::create_directory(&held, &units("q"), &sddl).err(),
        Some(Failure::Status(STATUS_OBJECT_NAME_COLLISION)),
        "a name that exists is not opened"
    );
    assert!(nt::hold(&held, &units("f")).is_err(), "a file is not held");
    let (serial, file_system) = nt::volume(&held).expect("volume");
    assert_ne!(serial, 0);
    assert_eq!(file_system, "NTFS");
}

/// An open takes one name and the kind alone: a separator, a stream, `.`,
/// `..`, an empty name, an open by file ID, backup intent or deletion on
/// close are refused before anything is opened; a listing is refused on a
/// handle opened without `SYNCHRONIZE`.
#[test]
fn install_ac_017_prim_refusals() {
    let root = Root::new();
    let d = directory(root.path(), "d");
    file(&d, &units("inner"), b"inner");
    let held = root.hold();
    for name in [r"d\inner", "d/inner", "d:stream", ".", "..", ""] {
        assert_eq!(
            nt::open_child(&held, &units(name), ACT, SHARE_ALL, 0).err(),
            Some(Failure::Status(STATUS_OBJECT_NAME_INVALID)),
            "{name:?}"
        );
    }
    let mut nul = units("d");
    nul.push(0);
    assert_eq!(
        nt::open_child(&held, &nul, ACT, SHARE_ALL, 0).err(),
        Some(Failure::Status(STATUS_OBJECT_NAME_INVALID))
    );
    // FILE_OPEN_BY_FILE_ID, FILE_OPEN_FOR_BACKUP_INTENT, FILE_DELETE_ON_CLOSE.
    for option in [0x2000, 0x4000, 0x1000] {
        assert_eq!(
            nt::open_child(&held, &units("d"), ACT, SHARE_ALL, option).err(),
            Some(Failure::Status(STATUS_INVALID_PARAMETER)),
            "option {option:#x}"
        );
    }
    let asynchronous = nt::open_child(
        &held,
        &units("d"),
        FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES,
        SHARE_ALL,
        FILE_DIRECTORY_FILE,
    )
    .expect("open without SYNCHRONIZE");
    assert_eq!(
        nt::list(&asynchronous).err(),
        Some(Failure::Status(STATUS_INVALID_PARAMETER))
    );
    let q = quarantine(&held);
    let object = nt::open_child(&held, &units("d"), ACT, SHARE_ALL, 0).expect("open");
    assert_eq!(
        nt::move_into(&object, &q, &units(r"x\y")),
        Err(Failure::Status(STATUS_OBJECT_NAME_INVALID))
    );
    assert!(exists(&d.join("inner")));
}

/// NT names of the paths the verb opens by path: a drive's, in the global
/// namespace; no share, device or relative path.
#[test]
fn install_ac_017_prim_nt_name() {
    let cases = [
        (r"C:\ProgramData", r"\GLOBAL??\C:\ProgramData"),
        (r"\\?\C:\ProgramData", r"\GLOBAL??\C:\ProgramData"),
    ];
    for (path, expected) in cases {
        assert_eq!(
            nt::nt_name(Path::new(path)).map(|name| OsString::from_wide(&name)),
            Some(OsString::from(expected)),
            "{path}"
        );
    }
    for path in [
        r"\\server\share\x",
        r"\\?\UNC\server\share\x",
        r"\\.\PhysicalDrive0",
        "PhysicalDrive0",
        "C:ProgramData",
        r"\ProgramData",
    ] {
        assert_eq!(nt::nt_name(Path::new(path)), None, "{path}");
    }
}

/// The owner and the DACL written through the object's handle: `set_owner`
/// writes the owner alone, `set_security` the protected DACL and the owner
/// its text names, and refuses a text with no DACL, which would admit
/// everyone.
#[test]
fn install_ac_017_prim_owner_and_security() {
    let root = Root::new();
    file(root.path(), &units("s"), b"s");
    let held = root.hold();
    let object = nt::open_child(
        &held,
        &units("s"),
        READ_CONTROL | WRITE_DAC | WRITE_OWNER | FILE_READ_ATTRIBUTES,
        SHARE_ALL,
        0,
    )
    .expect("open for its security");
    let before = stored(&root.join("s"));
    nt::set_owner(&object, &me()).expect("set the owner");
    let after = stored(&root.join("s"));
    assert_eq!(owner_of(&after), fixtures::user(), "the owner written");
    assert_eq!(dacl_text(&after), dacl_text(&before), "the DACL kept");

    let closed = format!("D:P(A;;FA;;;{})", me());
    nt::set_security(&object, &format!("O:{}{closed}", me())).expect("set the security");
    let written = stored(&root.join("s"));
    assert_eq!(owner_of(&written), fixtures::user());
    assert_eq!(dacl_text(&written), expected_dacl(&closed));
    for refused in [
        format!("O:{}", me()),
        String::new(),
        "D:NO_ACCESS_CONTROL".to_string(),
        "D:PNO_ACCESS_CONTROL".to_string(),
        format!("D:(A;;FA;;;{})", me()),
    ] {
        assert_eq!(
            nt::set_security(&object, &refused),
            Err(Failure::Win32(1336)),
            "{refused:?}"
        );
    }
    assert_eq!(stored(&root.join("s")), written, "nothing written");
}

/// The deletion is POSIX-style: the name goes when the act's handle closes,
/// though another handle stays open; and deleting another name of a file
/// leaves the name kept with its descriptor and contents.
#[test]
fn install_ac_017_prim_dispose_names() {
    let root = Root::new();
    file(root.path(), &units("p"), b"p");
    let outside = directory(root.path(), "o");
    file(&outside, &units("kept"), b"kept");
    hard_link(&root.join("other"), &outside.join("kept"));
    let held = root.hold();

    let other_handle = std::fs::File::open(root.join("p")).expect("another handle");
    let object = nt::open_child(&held, &units("p"), ACT, SHARE_ALL, 0).expect("open");
    nt::dispose(&object).expect("dispose");
    drop(object);
    assert!(
        !exists(&root.join("p")),
        "the name is gone with another handle open"
    );
    drop(other_handle);

    let kept_before = stored(&outside.join("kept"));
    let object = nt::open_child(&held, &units("other"), ACT, SHARE_ALL, 0).expect("open");
    assert_eq!(nt::info(&object).expect("info").links, 2);
    nt::dispose(&object).expect("dispose the other name");
    drop(object);
    assert!(!exists(&root.join("other")));
    assert_eq!(stored(&outside.join("kept")), kept_before);
    assert_eq!(std::fs::read(outside.join("kept")).expect("kept"), b"kept");
    let o = nt::hold(&held, &units("o")).expect("hold");
    let kept = nt::open_child(&o, &units("kept"), FILE_READ_ATTRIBUTES, SHARE_ALL, 0)
        .expect("open the name kept");
    assert_eq!(nt::info(&kept).expect("info").links, 1);
}

/// A directory beneath which a file is open is not moved.
#[test]
fn install_ac_017_prim_open_beneath() {
    let root = Root::new();
    let k = directory(root.path(), "k");
    file(&k, &units("open"), b"open");
    let held = root.hold();
    let q = quarantine(&held);
    let beneath = std::fs::File::open(k.join("open")).expect("a file open beneath");
    let object = nt::open_child(&held, &units("k"), ACT, SHARE_ALL, FILE_DIRECTORY_FILE)
        .expect("open the directory");
    assert!(nt::move_into(&object, &q, &units("1")).is_err());
    drop(beneath);
    assert!(exists(&k.join("open")), "the directory stays");
}

/// An open that another process's batch opportunistic lock would make wait
/// is refused at once, as in use.
#[test]
fn install_ac_017_prim_oplock() {
    let root = Root::new();
    file(root.path(), &units("o"), b"o");
    let wide = verbatim(&root.join("o"), &[]);
    // SAFETY: a NUL-terminated name; the handle is closed below.
    let locked = unsafe {
        CreateFileW(
            wide.as_ptr(),
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_OVERLAPPED,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(locked, INVALID_HANDLE_VALUE);
    // The request stays pending while the oplock is held: its OVERLAPPED
    // must outlive the handle, which completes it when closed.
    let overlapped: &'static mut OVERLAPPED = Box::leak(Box::new(
        // SAFETY: an all-zero OVERLAPPED is a valid value.
        unsafe { std::mem::zeroed() },
    ));
    let mut returned = 0u32;
    // SAFETY: an overlapped handle and an OVERLAPPED that lives forever.
    let ok = unsafe {
        DeviceIoControl(
            locked,
            FSCTL_REQUEST_BATCH_OPLOCK,
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
            0,
            &mut returned,
            overlapped,
        )
    };
    // SAFETY: reads the thread's last error.
    let error = unsafe { GetLastError() };
    assert!(
        ok == 0 && error == ERROR_IO_PENDING,
        "the batch oplock is granted (error {error})"
    );
    let held = root.hold();
    let started = std::time::Instant::now();
    let refused = nt::open_child(&held, &units("o"), ACT, SHARE_ALL, 0).err();
    let elapsed = started.elapsed();
    // SAFETY: the handle opened above.
    unsafe { CloseHandle(locked) };
    assert_eq!(refused, Some(Failure::Oplocked));
    assert!(Failure::Oplocked.in_use() && Failure::Unfinished.in_use());
    assert!(Failure::Status(nt::STATUS_DELETE_PENDING).in_use());
    assert!(!Failure::Internal.in_use());
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "refused at once, not after the oplock's break ({elapsed:?})"
    );
}

/// The verb's source (SPEC-024 Amendment 2026-11-19, points 2 and 3): no
/// call that adjusts the token, writes security by name or acts on a path; no
/// backup intent; every open without following a reparse point; security
/// written only through an object's handle, in two functions.
#[test]
fn install_ac_017_static_01() {
    let sources = [
        ("custody/mod.rs", include_str!("../src/custody/mod.rs")),
        ("custody/nt.rs", include_str!("../src/custody/nt.rs")),
    ];
    let forbidden = [
        "AdjustTokenPrivileges",
        "SetNamedSecurityInfo",
        "SetSecurityInfo",
        "SetFileSecurity",
        "MoveFileEx",
        "MoveFileW",
        // The calls; FILE_DISPOSITION_INFORMATION has a field of that name.
        "DeleteFileW",
        "DeleteFile(",
        "RemoveDirectory",
        "remove_dir_all",
        "remove_file",
        "std::fs::rename",
        "FILE_OPEN_FOR_BACKUP_INTENT",
        "FILE_FLAG_BACKUP_SEMANTICS",
        "CreateFileW",
        "ImpersonateLoggedOnUser",
        "SetThreadToken",
        "RtlAdjustPrivilege",
        "NtAdjustPrivilegesToken",
        "NtSetSecurityObject",
        "NtOpenFile",
        "OpenFileById",
        "ReOpenFile",
        "FILE_OPEN_BY_FILE_ID",
        "FILE_DELETE_ON_CLOSE",
    ];
    let mut found = Vec::new();
    let mut opens = 0;
    let mut writes = 0;
    for (file, source) in sources {
        for (index, line) in source.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for call in forbidden {
                if line.contains(call) {
                    found.push(format!("{file}:{} {call}", index + 1));
                }
            }
            opens += line.matches("NtCreateFile(").count();
            writes += line.matches("SetKernelObjectSecurity(").count();
        }
    }
    assert!(found.is_empty(), "{found:#?}");
    let nt = include_str!("../src/custody/nt.rs");
    assert_eq!(opens, 1, "one call opens objects");
    assert!(
        nt.lines().any(|line| {
            line.trim()
            == "options | FILE_OPEN_REPARSE_POINT | FILE_COMPLETE_IF_OPLOCKED | synchronous_option,"
        }),
        "that call never follows a reparse point"
    );
    assert_eq!(writes, 2, "set_security and set_owner");
    let set_security = nt
        .split("pub fn set_security")
        .nth(1)
        .expect("set_security");
    let set_owner = nt.split("pub fn set_owner").nth(1).expect("set_owner");
    assert!(set_security.contains("SetKernelObjectSecurity("));
    assert!(set_owner.contains("SetKernelObjectSecurity("));
}

/// A directory is created only with a protected DACL that is present and not
/// NULL, which takes nothing from its parent.
#[test]
fn install_ac_017_prim_create_refusals() {
    let root = Root::new();
    let held = root.hold();
    for sddl in [
        format!("D:(A;OICI;FA;;;{})", me()),
        "D:NO_ACCESS_CONTROL".to_string(),
        "D:PNO_ACCESS_CONTROL".to_string(),
        format!("O:{}", me()),
    ] {
        assert_eq!(
            nt::create_directory(&held, &units("q"), &sddl).err(),
            Some(Failure::Win32(1336)),
            "{sddl}"
        );
        assert!(!exists(&root.join("q")), "{sddl}: nothing created");
    }
}

/// A listing that takes several of the call's buffers, and a file whose
/// names take more than the first buffer, are read whole.
#[test]
fn install_ac_017_prim_large_listing_and_names() {
    let root = Root::new();
    let many = directory(root.path(), "many");
    let long = "n".repeat(120);
    let count = 700;
    for index in 0..count {
        file(&many, &units(&format!("{long}{index:04}")), b"");
    }
    let held = root.hold();
    let listing = nt::list(&nt::hold(&held, &units("many")).expect("hold")).expect("list");
    assert_eq!(listing.len(), count);
    let unique: BTreeSet<Vec<u16>> = listing.into_iter().map(|entry| entry.name).collect();
    assert_eq!(unique.len(), count);

    let linked = directory(root.path(), "linked");
    file(&linked, &units("first"), b"x");
    let links = 40;
    for index in 0..links {
        hard_link(
            &linked.join(format!("{long}{index:02}")),
            &linked.join("first"),
        );
    }
    let l = nt::hold(&held, &units("linked")).expect("hold");
    let object =
        nt::open_child(&l, &units("first"), FILE_READ_ATTRIBUTES, SHARE_ALL, 0).expect("open");
    let names = nt::names(&object).expect("names");
    assert_eq!(names.len(), links + 1);
    let l_id = nt::info(&l).expect("info").id;
    assert!(names.iter().all(|link| link.parent == l_id));
    let mut expected: BTreeSet<Vec<u16>> = (0..links)
        .map(|index| units(&format!("{long}{index:02}")))
        .collect();
    expected.insert(units("first"));
    let read: BTreeSet<Vec<u16>> = names.into_iter().map(|link| link.name).collect();
    assert_eq!(read, expected, "every name, whole");
}
