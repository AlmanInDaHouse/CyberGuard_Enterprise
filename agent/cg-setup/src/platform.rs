//! Preflight's first checks (SPEC-024 Amendment 2026-11-09, item 3): the
//! platform, Windows on an x86-64 machine (§Scope), and the elevation.

use crate::codes::{Refusal, ELEVATION, PLATFORM};

const UNSUPPORTED: &str = "this platform is not supported: the agent installs on Windows on an \
                           x86-64 machine";

/// Code 20 unless this is Windows on an x86-64 machine, natively: an x86-64
/// process emulated on another architecture is refused too.
pub fn check_platform() -> Result<(), Refusal> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::SystemInformation::{
            IMAGE_FILE_MACHINE, IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_UNKNOWN,
        };
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, IsWow64Process2};

        let mut process: IMAGE_FILE_MACHINE = 0;
        let mut native: IMAGE_FILE_MACHINE = 0;
        // SAFETY: the current process's pseudo-handle and two out-values.
        if unsafe { IsWow64Process2(GetCurrentProcess(), &mut process, &mut native) } == 0 {
            return Err(Refusal::new(
                PLATFORM,
                format!(
                    "{UNSUPPORTED}; the machine's architecture cannot be read: Win32 error {}",
                    // SAFETY: reads the calling thread's last-error value.
                    unsafe { windows_sys::Win32::Foundation::GetLastError() }
                ),
            ));
        }
        if native != IMAGE_FILE_MACHINE_AMD64 || process != IMAGE_FILE_MACHINE_UNKNOWN {
            return Err(Refusal::new(
                PLATFORM,
                format!(
                    "{UNSUPPORTED}; this machine is {native:#06x}, this process {process:#06x}"
                ),
            ));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Err(Refusal::new(PLATFORM, UNSUPPORTED))
    }
}

fn not_elevated() -> Refusal {
    Refusal::new(
        ELEVATION,
        "the setup must run elevated, as an administrator",
    )
}

/// Code 21 unless the process's token has Administrators enabled: a filtered
/// administrator's token holds it for deny only.
pub fn check_elevation() -> Result<(), Refusal> {
    #[cfg(windows)]
    {
        // SAFETY: null is the calling thread's effective token.
        unsafe { check_elevation_of(std::ptr::null_mut()) }
    }
    #[cfg(not(windows))]
    {
        Err(not_elevated())
    }
}

/// `check_elevation` for an impersonation token, or for the calling thread's
/// effective token when `token` is null.
///
/// # Safety
///
/// `token` is null or an open impersonation token with `TOKEN_QUERY`.
#[cfg(windows)]
pub unsafe fn check_elevation_of(
    token: windows_sys::Win32::Foundation::HANDLE,
) -> Result<(), Refusal> {
    use windows_sys::Win32::Security::{
        CheckTokenMembership, CreateWellKnownSid, WinBuiltinAdministratorsSid,
        SECURITY_MAX_SID_SIZE,
    };

    let mut sid = [0u8; SECURITY_MAX_SID_SIZE as usize];
    let mut size = SECURITY_MAX_SID_SIZE;
    // SAFETY: a buffer of `size` bytes for the SID, no domain.
    if unsafe {
        CreateWellKnownSid(
            WinBuiltinAdministratorsSid,
            std::ptr::null_mut(),
            sid.as_mut_ptr().cast(),
            &mut size,
        )
    } == 0
    {
        return Err(not_elevated());
    }
    let mut member = 0;
    // SAFETY: the caller's impersonation token or null, the SID built above,
    // and an out-value.
    if unsafe { CheckTokenMembership(token, sid.as_mut_ptr().cast(), &mut member) } == 0
        || member == 0
    {
        return Err(not_elevated());
    }
    Ok(())
}
