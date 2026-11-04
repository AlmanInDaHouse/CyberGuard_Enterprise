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
//! it (SPEC-024 §Operational §1 and §2). The file rotates within the
//! bounds of `rotation`; each new active file is opened the same way.

use std::fs::File;
use std::io::Write;
use std::path::Path;

use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_APPEND_DATA, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
    FILE_SHARE_READ, OPEN_ALWAYS, READ_CONTROL,
};

use crate::custody::{self, Descriptor, Handle, Policy};

/// `CyberGuard` and `CyberGuard\agent`: SYSTEM and Administrators only.
pub(crate) const PRIVATE_DIRECTORY: &str = "O:SYD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)";
/// `logs`: SYSTEM writes, Administrators read.
const LOG_DIRECTORY: &str = "O:SYD:P(A;OICI;FA;;;SY)(A;OICI;FR;;;BA)";
/// `agent.jsonl`: as its directory.
const LOG_FILE: &str = "O:SYD:P(A;;FA;;;SY)(A;;FR;;;BA)";

/// Open (creating as needed) the service's rotating log.
pub(super) fn open() -> Result<super::rotation::RotatingLog, String> {
    let agent = custody::program_data()?.join("CyberGuard").join("agent");
    let shared = Policy::shared();
    custody::ensure_directory(
        agent.parent().expect("CyberGuard"),
        PRIVATE_DIRECTORY,
        &shared,
    )?;
    custody::ensure_directory(&agent, PRIVATE_DIRECTORY, &shared)?;
    let logs = agent.join("logs");
    custody::ensure_directory(&logs, LOG_DIRECTORY, &shared)?;
    super::rotation::RotatingLog::open(
        &logs,
        Box::new(|path| {
            let (file, size) = open_log_file(path).map_err(std::io::Error::other)?;
            Ok((Box::new(file) as Box<dyn Write + Send>, size))
        }),
    )
    .map_err(|e| e.to_string())
}

/// Open a log file for appending, created with its final security; return
/// it with its size.
fn open_log_file(path: &Path) -> Result<(File, u64), String> {
    let descriptor = Descriptor::from_sddl(LOG_FILE)?;
    let attributes = descriptor.attributes();
    let name = custody::wide(path);
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
            custody::last_error()
        ));
    }
    let existed = custody::last_error() == ERROR_ALREADY_EXISTS;
    let handle = Handle(raw);
    let info = custody::file_information(&handle, path)?;
    if info.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY) != 0 {
        return Err(format!("{} is not a regular file", path.display()));
    }
    if info.nNumberOfLinks != 1 {
        return Err(format!("{} is hard-linked", path.display()));
    }
    if existed {
        custody::check_security(&handle, path, &Policy::shared())?;
    }
    let size = (u64::from(info.nFileSizeHigh) << 32) | u64::from(info.nFileSizeLow);
    Ok((handle.into_file(), size))
}
