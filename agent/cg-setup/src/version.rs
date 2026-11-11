//! The installed binary's version (SPEC-024 Amendment 2026-11-10, items 4
//! and 8): preflight's downgrade (29), and configure's check that the
//! installed binary is this package's, 45 when Windows holds a rename onto it
//! for the next restart and 40 otherwise.
//!
//! `cg-setup` is built with the agent's version input (§Data contracts §1),
//! so its own version is `cg_agent::AGENT_VERSION`: `X.Y.Z` in a release,
//! `X.Y.Z-dev` without the input, whose numeric version is `X.Y.Z`.

use std::path::Path;

use crate::codes::{Refusal, DOWNGRADE, OWN_STATE, RESTART_PENDING};

/// The version `cg-setup` was built with.
pub const SETUP_VERSION: &str = cg_agent::AGENT_VERSION;

/// The registry key and values where Windows holds the renames it performs
/// at the next restart.
pub const SESSION_MANAGER: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager";
pub const PENDING_RENAMES: [&str; 2] = [
    "PendingFileRenameOperations",
    "PendingFileRenameOperations2",
];

/// The numeric `X.Y.Z` of a version `X.Y.Z` or `X.Y.Z-dev`, each component
/// at most 65535, as a version resource holds it.
pub fn numeric(version: &str) -> Option<[u16; 3]> {
    let core = version.strip_suffix("-dev").unwrap_or(version);
    let mut parts = core.split('.');
    let mut numbers = [0u16; 3];
    for number in &mut numbers {
        let part = parts.next()?;
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        *number = part.parse().ok()?;
    }
    parts.next().is_none().then_some(numbers)
}

/// What the installed binary's version resource holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledVersion {
    /// The numeric file version, `dwFileVersionMS` and `dwFileVersionLS`.
    pub file_version: [u16; 4],
    /// The `ProductVersion` string of its first translation, if any.
    pub product_version: Option<String>,
}

/// Item 4: a numeric file version above `X.Y.Z.0`, `own` being `X.Y.Z`, is
/// 29. No installed binary is none.
pub fn check_downgrade(installed: Option<&InstalledVersion>, own: [u16; 3]) -> Result<(), Refusal> {
    let Some(installed) = installed else {
        return Ok(());
    };
    let [x, y, z] = own;
    if installed.file_version > [x, y, z, 0] {
        let [a, b, c, d] = installed.file_version;
        return Err(Refusal::new(
            DOWNGRADE,
            format!(
                "a downgrade: the installed agent is version {a}.{b}.{c}.{d}, above this \
                 package's {x}.{y}.{z}.0"
            ),
        ));
    }
    Ok(())
}

/// Item 8: the installed binary is this package's when its numeric file
/// version is `X.Y.Z.0` and its `ProductVersion` string is `own_text`.
pub fn is_this_packages(installed: &InstalledVersion, own: [u16; 3], own_text: &str) -> bool {
    let [x, y, z] = own;
    installed.file_version == [x, y, z, 0] && installed.product_version.as_deref() == Some(own_text)
}

/// The destinations of the renames a `REG_MULTI_SZ` value of
/// `PendingFileRenameOperations` holds: its strings in pairs, a source and a
/// destination, an empty destination being a deletion; an empty source ends
/// the list.
pub fn rename_destinations(value: &[u16]) -> Vec<String> {
    let strings: Vec<String> = value
        .split(|&unit| unit == 0)
        .map(String::from_utf16_lossy)
        .collect();
    let mut destinations = Vec::new();
    for pair in strings.chunks(2) {
        let [source, destination] = pair else {
            break;
        };
        if source.is_empty() {
            break;
        }
        if !destination.is_empty() {
            destinations.push(destination.clone());
        }
    }
    destinations
}

/// Item 8: whether one of these destinations is `binary`, compared without
/// its `!` and `\??\` prefixes and equal but for ASCII case.
pub fn renames_onto(destinations: &[String], binary: &Path) -> bool {
    let Some(binary) = binary.to_str() else {
        return false;
    };
    destinations.iter().any(|destination| {
        let destination = destination.strip_prefix('!').unwrap_or(destination);
        let destination = destination.strip_prefix(r"\??\").unwrap_or(destination);
        destination.eq_ignore_ascii_case(binary)
    })
}

/// A string of the installed binary in a refusal's one line: its control
/// characters escaped, at most 64 characters of it.
fn quoted(text: &str) -> String {
    let mut quoted: String = text.chars().take(64).flat_map(char::escape_debug).collect();
    if text.chars().count() > 64 {
        quoted.push_str("...");
    }
    format!("\"{quoted}\"")
}

/// Item 8: configure's check of the installed binary. When it is not this
/// package's (or is absent), configure starts nothing and leaves the record:
/// 45 when `restart_pending`, a rename onto it held for the next restart, and
/// 40 otherwise.
pub fn check_installed(
    installed: Option<&InstalledVersion>,
    own: [u16; 3],
    own_text: &str,
    restart_pending: bool,
) -> Result<(), Refusal> {
    if installed.is_some_and(|installed| is_this_packages(installed, own, own_text)) {
        return Ok(());
    }
    let found = match installed {
        None => "no installed agent".to_string(),
        Some(InstalledVersion {
            file_version: [a, b, c, d],
            product_version,
        }) => format!(
            "the installed agent is file version {a}.{b}.{c}.{d}, product version {}",
            product_version
                .as_deref()
                .map_or("none".to_string(), quoted)
        ),
    };
    if restart_pending {
        Err(Refusal::new(
            RESTART_PENDING,
            format!(
                "a restart is pending: {found}, and Windows holds a rename onto it for the \
                 next restart; restart the computer and run the setup again"
            ),
        ))
    } else {
        Err(Refusal::new(
            OWN_STATE,
            format!("the installed agent is not this package's {own_text}: {found}"),
        ))
    }
}

#[cfg(windows)]
pub use windows::{configure_version, downgrade, pending_renames, read_installed};

#[cfg(windows)]
mod windows {
    use std::os::windows::ffi::OsStrExt as _;
    use std::path::Path;

    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_SUCCESS};
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileVersionInfoExW, GetFileVersionInfoSizeExW, VerQueryValueW, FILE_VER_GET_NEUTRAL,
        VS_FIXEDFILEINFO,
    };
    use windows_sys::Win32::System::Registry::{
        RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_MULTI_SZ,
    };

    use super::{rename_destinations, InstalledVersion, PENDING_RENAMES, SESSION_MANAGER};
    use crate::codes::{Refusal, OWN_STATE, RESTART_PENDING};

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn last_error() -> u32 {
        // SAFETY: reads the calling thread's last-error value.
        unsafe { windows_sys::Win32::Foundation::GetLastError() }
    }

    /// A value of the version block at `sub_block`, as bytes. `VerQueryValueW`
    /// gives a string value's length in characters and any other's in bytes:
    /// `unit` is the size of one, 2 or 1. A value that does not lie within
    /// the block is none.
    fn query<'a>(block: &'a [u8], sub_block: &str, unit: usize) -> Option<&'a [u8]> {
        let sub_block = wide(sub_block);
        let mut pointer: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut length = 0u32;
        // SAFETY: the block is what GetFileVersionInfoW wrote; the pointer
        // returned points into it, `length` units long.
        let ok = unsafe {
            VerQueryValueW(
                block.as_ptr().cast(),
                sub_block.as_ptr(),
                &mut pointer,
                &mut length,
            )
        };
        if ok == 0 || pointer.is_null() {
            return None;
        }
        let start = (pointer as usize).checked_sub(block.as_ptr() as usize)?;
        let end = start.checked_add((length as usize).checked_mul(unit)?)?;
        block.get(start..end)
    }

    /// Item 4: the installed binary's version resource. `Ok(None)` when it
    /// does not exist; 40 when it exists and its version cannot be read.
    pub fn read_installed(binary: &Path) -> Result<Option<InstalledVersion>, Refusal> {
        let refused = |reason: String| {
            Refusal::new(
                OWN_STATE,
                format!(
                    "the version of the installed agent {} cannot be read: {reason}",
                    binary.display()
                ),
            )
        };
        match std::fs::symlink_metadata(binary) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(refused(e.to_string())),
            Ok(_) => {}
        }
        let path: Vec<u16> = binary
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // The binary's own resource: no language-specific satellite file.
        let mut ignored = 0u32;
        // SAFETY: the path lives for the call.
        let size =
            unsafe { GetFileVersionInfoSizeExW(FILE_VER_GET_NEUTRAL, path.as_ptr(), &mut ignored) };
        if size == 0 {
            return Err(refused(format!("Win32 error {}", last_error())));
        }
        let mut block = vec![0u8; size as usize];
        // SAFETY: the buffer holds `size` bytes.
        if unsafe {
            GetFileVersionInfoExW(
                FILE_VER_GET_NEUTRAL,
                path.as_ptr(),
                0,
                size,
                block.as_mut_ptr().cast(),
            )
        } == 0
        {
            return Err(refused(format!("Win32 error {}", last_error())));
        }
        let fixed = query(&block, "\\", 1)
            .filter(|bytes| bytes.len() >= std::mem::size_of::<VS_FIXEDFILEINFO>())
            .ok_or_else(|| refused("no fixed file information".to_string()))?;
        // SAFETY: at least one VS_FIXEDFILEINFO, read unaligned.
        let fixed: VS_FIXEDFILEINFO = unsafe { std::ptr::read_unaligned(fixed.as_ptr().cast()) };
        if fixed.dwSignature != 0xFEEF_04BD {
            return Err(refused("no fixed file information".to_string()));
        }
        let file_version = [
            (fixed.dwFileVersionMS >> 16) as u16,
            fixed.dwFileVersionMS as u16,
            (fixed.dwFileVersionLS >> 16) as u16,
            fixed.dwFileVersionLS as u16,
        ];
        let product_version = query(&block, "\\VarFileInfo\\Translation", 1)
            .filter(|bytes| bytes.len() >= 4)
            .and_then(|translation| {
                let language = u16::from_le_bytes([translation[0], translation[1]]);
                let code_page = u16::from_le_bytes([translation[2], translation[3]]);
                query(
                    &block,
                    &format!("\\StringFileInfo\\{language:04x}{code_page:04x}\\ProductVersion"),
                    2,
                )
            })
            .map(|bytes| {
                let units: Vec<u16> = bytes
                    .chunks_exact(2)
                    .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                    .take_while(|&unit| unit != 0)
                    .collect();
                String::from_utf16_lossy(&units)
            });
        Ok(Some(InstalledVersion {
            file_version,
            product_version,
        }))
    }

    /// One `REG_MULTI_SZ` value of the Session Manager's key: `Ok(None)`
    /// when it is absent. A value that grows between the size and the read
    /// is read again, a few times.
    fn read_multi_sz(key: &[u16], name: &str) -> Result<Option<Vec<u16>>, String> {
        let value = wide(name);
        let mut status = ERROR_SUCCESS;
        for _ in 0..4 {
            let mut size = 0u32;
            // SAFETY: a null buffer asks for the size alone.
            status = unsafe {
                RegGetValueW(
                    HKEY_LOCAL_MACHINE,
                    key.as_ptr(),
                    value.as_ptr(),
                    RRF_RT_REG_MULTI_SZ,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &mut size,
                )
            };
            if status == ERROR_FILE_NOT_FOUND {
                return Ok(None);
            }
            if status != ERROR_SUCCESS {
                break;
            }
            let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
            let mut size = (buffer.len() * 2) as u32;
            // SAFETY: the buffer holds `size` bytes.
            status = unsafe {
                RegGetValueW(
                    HKEY_LOCAL_MACHINE,
                    key.as_ptr(),
                    value.as_ptr(),
                    RRF_RT_REG_MULTI_SZ,
                    std::ptr::null_mut(),
                    buffer.as_mut_ptr().cast(),
                    &mut size,
                )
            };
            match status {
                ERROR_SUCCESS => {
                    buffer.truncate(size as usize / 2);
                    return Ok(Some(buffer));
                }
                ERROR_FILE_NOT_FOUND => return Ok(None),
                ERROR_MORE_DATA => continue,
                _ => break,
            }
        }
        Err(format!("read {name}: Win32 error {status}"))
    }

    /// Item 8: the destinations of the renames Windows holds for the next
    /// restart, from both values, and why a value could not be read, if one
    /// could not: a rename one value holds counts though the other is
    /// unreadable. A value that is absent holds none.
    pub fn pending_renames() -> (Vec<String>, Vec<String>) {
        let key = wide(SESSION_MANAGER);
        let mut destinations = Vec::new();
        let mut unreadable = Vec::new();
        for name in PENDING_RENAMES {
            match read_multi_sz(&key, name) {
                Ok(Some(value)) => destinations.extend(rename_destinations(&value)),
                Ok(None) => {}
                Err(reason) => unreadable.push(reason),
            }
        }
        (destinations, unreadable)
    }

    /// Item 4: preflight's downgrade check of the installed binary, 29 or 40.
    pub fn downgrade(binary: &Path, own: [u16; 3]) -> Result<(), Refusal> {
        super::check_downgrade(read_installed(binary)?.as_ref(), own)
    }

    /// Item 8: configure's check of the installed binary. One whose version
    /// cannot be read is not this package's; 45 when Windows holds a rename
    /// onto it for the next restart, and 40 otherwise, renames that cannot be
    /// read included.
    pub fn configure_version(binary: &Path, own: [u16; 3], own_text: &str) -> Result<(), Refusal> {
        let installed = read_installed(binary);
        if let Ok(Some(found)) = &installed {
            if super::is_this_packages(found, own, own_text) {
                return Ok(());
            }
        }
        // A rename that cannot be read is no rename held: 40.
        let restart_pending = super::renames_onto(&pending_renames().0, binary);
        match installed {
            Ok(found) => super::check_installed(found.as_ref(), own, own_text, restart_pending),
            Err(unreadable) if !restart_pending => Err(unreadable),
            Err(_) => Err(Refusal::new(
                RESTART_PENDING,
                "a restart is pending: the version of the installed agent cannot be read, and \
                 Windows holds a rename onto it for the next restart; restart the computer and \
                 run the setup again",
            )),
        }
    }
}
