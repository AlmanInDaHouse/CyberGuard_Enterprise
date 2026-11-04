//! Fixtures for the unelevated custody tests: the current user's SID, a
//! policy that names it where the service's names SYSTEM, and the owner
//! and DACL of an object as SDDL. An unelevated process cannot create an
//! object owned by SYSTEM, so these tests check the same code against the
//! current user; the service and SYSTEM tests check SYSTEM's policy.
//! What this process creates is owned by its token's default owner: the
//! user unelevated, Administrators for an elevated administrator (the CI
//! runner's built-in Administrator).
#![allow(dead_code)]

use std::path::Path;

use cg_agent::custody::{Policy, Principal};
use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE};
use windows_sys::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertSidToStringSidW,
    ConvertStringSecurityDescriptorToSecurityDescriptorW, ConvertStringSidToSidW,
    GetNamedSecurityInfoW, SDDL_REVISION_1, SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    GetLengthSid, GetTokenInformation, TokenOwner, DACL_SECURITY_INFORMATION,
    OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, TOKEN_OWNER, TOKEN_QUERY,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// The current user's SID as text, from `whoami /user /fo csv /nh`.
pub fn user_sid() -> String {
    let output = std::process::Command::new("whoami")
        .args(["/user", "/fo", "csv", "/nh"])
        .output()
        .expect("run whoami");
    let text = String::from_utf8_lossy(&output.stdout);
    text.trim()
        .rsplit(',')
        .next()
        .map(|field| field.trim_matches('"').to_string())
        .filter(|sid| sid.starts_with("S-1-"))
        .unwrap_or_else(|| panic!("no SID in whoami output: {text}"))
}

/// A SID's binary form.
pub fn sid_bytes(sid: &str) -> Vec<u8> {
    let wide: Vec<u16> = sid.encode_utf16().chain(std::iter::once(0)).collect();
    let mut binary: PSID = std::ptr::null_mut();
    // SAFETY: a NUL-terminated string; `binary` is LocalAlloc'd, freed below.
    assert_ne!(
        unsafe { ConvertStringSidToSidW(wide.as_ptr(), &mut binary) },
        0,
        "ConvertStringSidToSidW({sid})"
    );
    // SAFETY: a valid SID of GetLengthSid bytes.
    let bytes = unsafe {
        std::slice::from_raw_parts(binary as *const u8, GetLengthSid(binary) as usize).to_vec()
    };
    // SAFETY: allocated by ConvertStringSidToSidW.
    unsafe { LocalFree(binary.cast()) };
    bytes
}

/// The SID of this process token's default owner, as text.
pub fn default_owner_sid() -> String {
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: the current-process pseudo-handle; `token` is closed below.
    assert_ne!(
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) },
        0,
        "OpenProcessToken"
    );
    let mut needed = 0u32;
    // SAFETY: a size query on a valid token.
    unsafe { GetTokenInformation(token, TokenOwner, std::ptr::null_mut(), 0, &mut needed) };
    let mut buffer = vec![0u64; (needed as usize).div_ceil(8).max(1)];
    // SAFETY: `buffer` holds at least `needed` bytes, u64-aligned.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenOwner,
            buffer.as_mut_ptr().cast(),
            (buffer.len() * 8) as u32,
            &mut needed,
        )
    };
    // SAFETY: opened above.
    unsafe { CloseHandle(token) };
    assert_ne!(ok, 0, "GetTokenInformation(TokenOwner)");
    // SAFETY: a TOKEN_OWNER whose Owner points into `buffer`.
    let owner: PSID = unsafe { (*(buffer.as_ptr() as *const TOKEN_OWNER)).Owner };
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: a valid SID; `text` is LocalAlloc'd, freed below.
    assert_ne!(
        unsafe { ConvertSidToStringSidW(owner, &mut text) },
        0,
        "ConvertSidToStringSidW"
    );
    // SAFETY: a NUL-terminated string from the call, read then freed.
    unsafe {
        let len = (0..).take_while(|&i| *text.add(i) != 0).count();
        let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
        LocalFree(text.cast());
        sid
    }
}

/// `sddl`'s owner and DACL as Windows renders them (aliases such as `BA`
/// and `LA` in place of SIDs), for comparison with `owner_and_dacl`.
pub fn normalized(sddl: &str) -> String {
    let wide: Vec<u16> = sddl.encode_utf16().chain(std::iter::once(0)).collect();
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: a NUL-terminated string; the descriptor is freed below.
    assert_ne!(
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        },
        0,
        "parse {sddl}"
    );
    let rendered = render(descriptor);
    // SAFETY: allocated by the call above.
    unsafe { LocalFree(descriptor.cast()) };
    rendered
}

fn render(descriptor: PSECURITY_DESCRIPTOR) -> String {
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: a valid descriptor; `text` is LocalAlloc'd, freed below.
    let ok = unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor,
            SDDL_REVISION_1,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut text,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(
        ok, 0,
        "ConvertSecurityDescriptorToStringSecurityDescriptorW"
    );
    // SAFETY: a NUL-terminated string from the call, read then freed.
    unsafe {
        let len = (0..).take_while(|&i| *text.add(i) != 0).count();
        let sddl = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
        LocalFree(text.cast());
        sddl
    }
}

/// The current user as a principal.
pub fn user() -> Principal {
    Principal::Sid(sid_bytes(&user_sid()))
}

/// The current user where SYSTEM's policy names SYSTEM alone: the only
/// allowed principal, a protected DACL; the owner is what this process
/// creates objects with.
pub fn user_only() -> Policy {
    Policy {
        owners: vec![user(), Principal::Sid(sid_bytes(&default_owner_sid()))],
        allowed: vec![user()],
        protected: true,
    }
}

/// A protected DACL with the current user's full control and `extra`
/// entries (SDDL ACE strings), inherited by children when `container`.
pub fn sddl(container: bool, extra: &str) -> String {
    let inherit = if container { "OICI" } else { "" };
    format!("D:P(A;{inherit};FA;;;{}){extra}", user_sid())
}

/// The owner and DACL of `path`, as SDDL.
pub fn owner_and_dacl(path: &Path) -> String {
    let wide: Vec<u16> = path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: a NUL-terminated name; the descriptor is freed below.
    let error = unsafe {
        GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    assert_eq!(error, 0, "GetNamedSecurityInfoW {}", path.display());
    let sddl = render(descriptor);
    // SAFETY: allocated by GetNamedSecurityInfoW.
    unsafe { LocalFree(descriptor.cast()) };
    sddl
}
