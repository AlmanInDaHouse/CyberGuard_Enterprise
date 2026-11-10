//! Preflight's first checks (SPEC-024 Amendment 2026-11-09, item 3): the
//! platform, Windows 10.0 build 17763 or later on an x86-64 machine (§Scope
//! and the Amendment 2026-11-10, item 1), and the elevation.

use crate::codes::{Refusal, ELEVATION, PLATFORM};

/// A Windows version, compared as major, minor, then build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct WindowsVersion {
    pub major: u32,
    pub minor: u32,
    pub build: u32,
}

impl std::fmt::Display for WindowsVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{} build {}", self.major, self.minor, self.build)
    }
}

/// The oldest Windows the agent installs on: Windows 10 1809 and Windows
/// Server 2019 (the Amendment 2026-11-10, item 1).
pub const FLOOR: WindowsVersion = WindowsVersion {
    major: 10,
    minor: 0,
    build: 17763,
};

/// Code 20 for a Windows below `FLOOR`; the line names the version read,
/// which a compatibility mode applied to the setup lowers.
pub fn check_floor(version: WindowsVersion) -> Result<(), Refusal> {
    if version < FLOOR {
        return Err(Refusal::new(
            PLATFORM,
            format!("{UNSUPPORTED}, Windows {FLOOR} or later; this Windows reports {version}"),
        ));
    }
    Ok(())
}

/// The running Windows' version, from `RtlGetVersion`, which no
/// compatibility manifest changes.
#[cfg(windows)]
pub fn windows_version() -> WindowsVersion {
    use windows_sys::Wdk::System::SystemServices::RtlGetVersion;
    use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;

    // SAFETY: OSVERSIONINFOW is plain data; all zeros is a valid value.
    let mut info: OSVERSIONINFOW = unsafe { std::mem::zeroed() };
    info.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOW>() as u32;
    // SAFETY: a structure whose size field is set, as the call requires; it
    // always succeeds in user mode.
    unsafe { RtlGetVersion(&mut info) };
    WindowsVersion {
        major: info.dwMajorVersion,
        minor: info.dwMinorVersion,
        build: info.dwBuildNumber,
    }
}

const UNSUPPORTED: &str = "this platform is not supported: the agent installs on Windows on an \
                           x86-64 machine";

/// Code 20 unless this is Windows at or above `FLOOR` on an x86-64 machine,
/// natively: an x86-64 process emulated on another architecture is refused
/// too. The version is checked first, and `IsWow64Process2`, which early
/// Windows 10 releases lack, is resolved after it, so that such a Windows
/// reaches the check.
pub fn check_platform() -> Result<(), Refusal> {
    #[cfg(windows)]
    {
        use windows_sys::core::{s, w};
        use windows_sys::Win32::Foundation::{BOOL, HANDLE};
        use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
        use windows_sys::Win32::System::SystemInformation::{
            IMAGE_FILE_MACHINE, IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_UNKNOWN,
        };
        use windows_sys::Win32::System::Threading::GetCurrentProcess;

        check_floor(windows_version())?;

        type IsWow64Process2 = unsafe extern "system" fn(
            HANDLE,
            *mut IMAGE_FILE_MACHINE,
            *mut IMAGE_FILE_MACHINE,
        ) -> BOOL;
        // SAFETY: kernel32 is loaded in every process; a NUL-terminated name.
        let module = unsafe { GetModuleHandleW(w!("kernel32.dll")) };
        // SAFETY: a module handle, possibly null, and a NUL-terminated name.
        let address = unsafe { GetProcAddress(module, s!("IsWow64Process2")) };
        let Some(address) = address else {
            return Err(Refusal::new(
                PLATFORM,
                format!("{UNSUPPORTED}; this Windows does not export IsWow64Process2"),
            ));
        };
        // SAFETY: kernel32's IsWow64Process2 has this signature.
        let is_wow64_process2: IsWow64Process2 = unsafe { std::mem::transmute(address) };

        let mut process: IMAGE_FILE_MACHINE = 0;
        let mut native: IMAGE_FILE_MACHINE = 0;
        // SAFETY: the current process's pseudo-handle and two out-values.
        if unsafe { is_wow64_process2(GetCurrentProcess(), &mut process, &mut native) } == 0 {
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
