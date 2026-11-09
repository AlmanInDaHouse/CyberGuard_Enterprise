//! What SPEC-024's package takes from the executable it carries. The binary
//! has no external runtime dependency beyond the OS (SPEC-001 NFR-004; SPEC-024
//! §Data contracts §1), so its import tables name only DLLs Windows ships. It
//! holds a message table for entries 1000 to 1003 (Amendment 2026-11-09,
//! item 4), for the Event Log source the MSI registers. Both read the binary
//! `cargo test` built, unelevated, and change nothing on the host; they are
//! preconditions of install_ac_001 and install_ac_010, not those criteria.

#![cfg(windows)]

use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use cg_agent::service::{EVENT_DEADLINE, EVENT_FAILED, EVENT_RUNNING, EVENT_STOPPED};
use windows_sys::Win32::Foundation::{FreeLibrary, HMODULE};
use windows_sys::Win32::System::Diagnostics::Debug::{
    FormatMessageW, FORMAT_MESSAGE_ARGUMENT_ARRAY, FORMAT_MESSAGE_FROM_HMODULE,
};
use windows_sys::Win32::System::LibraryLoader::{LoadLibraryExW, LOAD_LIBRARY_AS_DATAFILE};

/// U.S. English, the language of the executable's resources.
const LANGUAGE: u32 = 0x0409;

/// The DLLs the agent may import, each one part of every supported Windows.
/// Beyond these, an API set other than the C runtime's is the system's too.
const SYSTEM_DLLS: [&str; 13] = [
    "advapi32.dll",
    "bcrypt.dll",
    "bcryptprimitives.dll",
    "combase.dll",
    "crypt32.dll",
    "kernel32.dll",
    "ntdll.dll",
    "ole32.dll",
    "oleaut32.dll",
    "shell32.dll",
    "tdh.dll",
    "wevtapi.dll",
    "ws2_32.dll",
];

fn u16_at(bytes: &[u8], at: usize) -> usize {
    u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()) as usize
}

fn u32_at(bytes: &[u8], at: usize) -> usize {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
}

/// The file offset of a relative virtual address, through the section table.
fn offset_of(bytes: &[u8], sections: usize, count: usize, rva: usize) -> usize {
    (0..count)
        .map(|i| sections + i * 40)
        .find_map(|section| {
            let start = u32_at(bytes, section + 12);
            let size = u32_at(bytes, section + 8).max(u32_at(bytes, section + 16));
            (start..start + size)
                .contains(&rva)
                .then(|| rva - start + u32_at(bytes, section + 20))
        })
        .unwrap_or_else(|| panic!("RVA {rva:#x} is in no section"))
}

/// The names of every DLL a PE32+ image imports, delay-loaded ones included.
fn imported_dlls(path: &Path) -> Vec<String> {
    let bytes = std::fs::read(path).expect("read the executable");
    let pe = u32_at(&bytes, 0x3C);
    assert_eq!(&bytes[pe..pe + 4], b"PE\0\0", "the PE signature");
    let sections_count = u16_at(&bytes, pe + 6);
    let optional = pe + 24;
    assert_eq!(u16_at(&bytes, optional), 0x20B, "a PE32+ image");
    let sections = optional + u16_at(&bytes, pe + 20);
    let directory = |index: usize| u32_at(&bytes, optional + 112 + index * 8);
    let name_at = |rva: usize| {
        let start = offset_of(&bytes, sections, sections_count, rva);
        let len = bytes[start..].iter().position(|b| *b == 0).expect("a NUL");
        String::from_utf8_lossy(&bytes[start..start + len]).into_owned()
    };

    let mut names = Vec::new();
    // (directory index, descriptor size, offset of its Name RVA)
    for (index, size, name) in [(1, 20, 12), (13, 32, 4)] {
        let rva = directory(index);
        if rva == 0 {
            continue;
        }
        let mut descriptor = offset_of(&bytes, sections, sections_count, rva);
        while bytes[descriptor..descriptor + size].iter().any(|b| *b != 0) {
            names.push(name_at(u32_at(&bytes, descriptor + name)));
            descriptor += size;
        }
    }
    names
}

#[test]
fn package_executable_imports_only_what_windows_ships() {
    let names = imported_dlls(Path::new(env!("CARGO_BIN_EXE_cg-agent")));
    assert!(
        names.iter().any(|n| n.eq_ignore_ascii_case("kernel32.dll")),
        "the import table was read: {names:?}"
    );
    let foreign: Vec<&String> = names
        .iter()
        .filter(|name| {
            let name = name.to_ascii_lowercase();
            let api_set = name.starts_with("api-ms-win-") && !name.starts_with("api-ms-win-crt-");
            !api_set && !SYSTEM_DLLS.contains(&name.as_str())
        })
        .collect();
    assert!(
        foreign.is_empty(),
        "cg-agent.exe needs a DLL the endpoint may not have: {foreign:?}"
    );
}

/// The text the message file yields for an entry and its one insertion
/// string through `FormatMessageW`; `None` when it has no such entry.
fn message(module: HMODULE, id: u32, language: u32, insert: &str) -> Option<String> {
    let insert: Vec<u16> = insert.encode_utf16().chain(std::iter::once(0)).collect();
    let arguments = [insert.as_ptr() as usize];
    let mut text = vec![0u16; 1024];
    // SAFETY: `module` is a loaded image, `text` holds the length passed, and
    // the argument array holds one pointer to a NUL-terminated string that
    // outlives the call.
    let length = unsafe {
        FormatMessageW(
            FORMAT_MESSAGE_FROM_HMODULE | FORMAT_MESSAGE_ARGUMENT_ARRAY,
            module.cast_const(),
            id,
            language,
            text.as_mut_ptr(),
            text.len() as u32,
            arguments.as_ptr().cast(),
        )
    };
    (length != 0).then(|| String::from_utf16_lossy(&text[..length as usize]))
}

#[test]
fn package_message_table_holds_the_service_entries() {
    let path: Vec<u16> = std::ffi::OsStr::new(env!("CARGO_BIN_EXE_cg-agent"))
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: `path` is NUL-terminated; a data file runs none of its code.
    let module = unsafe {
        LoadLibraryExW(
            path.as_ptr(),
            std::ptr::null_mut(),
            LOAD_LIBRARY_AS_DATAFILE,
        )
    };
    assert!(
        !module.is_null(),
        "load the executable as a data file: {}",
        std::io::Error::last_os_error()
    );

    // An entry's text is its one string, whole. Language 0 is the search
    // through the thread's and the system's languages a host in another
    // language makes, which must still reach the table.
    let text = "cg-agent 1.2.3 stopped at 100% of %2";
    for language in [LANGUAGE, 0] {
        for id in [EVENT_RUNNING, EVENT_STOPPED, EVENT_FAILED, EVENT_DEADLINE] {
            assert_eq!(
                message(module, id, language, text).as_deref(),
                Some(text),
                "{id} in language {language:#x}"
            );
        }
        assert_eq!(message(module, EVENT_RUNNING - 1, language, text), None);
        assert_eq!(message(module, EVENT_DEADLINE + 1, language, text), None);
    }

    // SAFETY: the module LoadLibraryExW returned, freed once.
    unsafe { FreeLibrary(module) };
}
