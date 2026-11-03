//! The per-host instance lock (SPEC-024 second Amendment 2026-11-01 §1;
//! ADR-0008 Amendment 2026-11-02; ADR-0010 Amendment 2026-11-02).
//!
//! One CyberGuard capture agent may run per host. The lock is a named
//! mutex inside a private namespace whose boundary descriptor carries the
//! built-in Administrators SID: Windows requires every SID of a boundary
//! descriptor to be present *and enabled* in the caller's token to enter
//! the namespace, so only an elevated administrator's token or SYSTEM can
//! create or open the object, and an ordinary user can neither take nor
//! squat it. As defence in depth the namespace object's own DACL
//! (`D:P(A;;GA;;;BA)(A;;GA;;;SY)`) admits only Administrators and SYSTEM.
//! The mutex's *existence* — one open handle held for the process
//! lifetime — is the lock; when the holder dies the kernel drops its last
//! handle and the lock is free again, so a crash leaves no stale lock.
//!
//! It is taken on the secure path before the identity step, so before the
//! ETW session start reclaims the agent's fixed session (ADR-0008
//! §Decision part 2). Only a start that holds it reclaims; a start that
//! does not never calls the reclaim (ADR-0008 Amendment 2026-11-02).
//!
//! The agent reads its own process token, read-only, to tell three kinds
//! of token apart (ADR-0010 §Context and its Amendment 2026-11-02):
//!   - an elevated administrator's or SYSTEM's token (`BUILTIN\Administrators`
//!     present and enabled): it creates the lock;
//!   - a token that controls trace sessions outside that model — a member
//!     of Performance Log Users, LocalService or NetworkService, or a
//!     holder of `SeSystemProfilePrivilege` — which cannot create the lock
//!     but could stop a live instance's session: the start is refused
//!     ([`LockOutcome::ControlsEtwOutsideModel`], code 1) before the
//!     identity step;
//!   - any other token (an ordinary user, or an administrator's unelevated
//!     one): it goes on without the lock ([`LockOutcome::Unprivileged`]),
//!     to an ETW start that does not reclaim and whose refusal because the
//!     session name exists is a privilege failure (code 9).

/// The stderr/Event-log line when another live instance holds the lock
/// (SPEC-024 §1, exit code 10).
pub const STDERR_ANOTHER_INSTANCE: &str =
    "cg-agent: another CyberGuard agent instance is running on this host";

/// The stderr/Event-log line for a token that controls trace sessions but
/// cannot create the lock (SPEC-024 §1, ADR-0010 Amendment 2026-11-02
/// item 1, exit code 1).
pub const STDERR_CONTROLS_ETW: &str =
    "cg-agent: instance lock failed: this token controls ETW sessions but is not an elevated administrator's or SYSTEM's";

/// The line for any other failure to take the lock (SPEC-024 §1, exit
/// code 1): `cg-agent: instance lock failed: <step>: Win32 error <code>`.
pub fn stderr_lock_failed(step: &str, code: u32) -> String {
    format!("cg-agent: instance lock failed: {step}: Win32 error {code}")
}

/// What acquiring the per-host instance lock produced. Only `Acquired`
/// and `Unprivileged` let the agent go on; `Held`, `ControlsEtwOutsideModel`
/// and `Failed` end the start with the exit code of [`LockOutcome::exit_code`].
#[derive(Debug)]
pub enum LockOutcome {
    /// The token can create the lock and no live instance held it: the
    /// agent holds it now. Keep the [`InstanceLock`] for the process
    /// lifetime; its drop frees the lock.
    Acquired(InstanceLock),
    /// Another live elevated/SYSTEM process holds the lock. Exit code 10.
    Held,
    /// The token controls trace sessions but cannot create the lock
    /// (ADR-0010 Amendment 2026-11-02 item 1). Exit code 1.
    ControlsEtwOutsideModel,
    /// An ordinary or unelevated token that cannot control trace sessions:
    /// it takes no lock and the start goes on without it. Not an error.
    Unprivileged,
    /// Any other failure to take the lock; the agent cannot establish that
    /// no live instance holds it. Exit code 1.
    Failed { step: &'static str, code: u32 },
}

impl LockOutcome {
    /// The process exit code for an outcome that ends the start, or `None`
    /// for `Acquired`/`Unprivileged`, which go on.
    pub fn exit_code(&self) -> Option<u8> {
        match self {
            LockOutcome::Acquired(_) | LockOutcome::Unprivileged => None,
            LockOutcome::Held => Some(10),
            LockOutcome::ControlsEtwOutsideModel | LockOutcome::Failed { .. } => Some(1),
        }
    }

    /// The single stderr/Event-log line for an outcome that ends the
    /// start, or `None` for `Acquired`/`Unprivileged`.
    pub fn stderr_line(&self) -> Option<String> {
        match self {
            LockOutcome::Acquired(_) | LockOutcome::Unprivileged => None,
            LockOutcome::Held => Some(STDERR_ANOTHER_INSTANCE.to_string()),
            LockOutcome::ControlsEtwOutsideModel => Some(STDERR_CONTROLS_ETW.to_string()),
            LockOutcome::Failed { step, code } => Some(stderr_lock_failed(step, *code)),
        }
    }
}

/// How the process token relates to the instance lock, read from the
/// token alone (ADR-0010 Amendment 2026-11-02).
#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenClass {
    /// `BUILTIN\Administrators` present and enabled: an elevated
    /// administrator's token or SYSTEM. It creates the lock.
    CanCreate,
    /// Controls trace sessions outside ADR-0010's model (Performance Log
    /// Users, LocalService, NetworkService, or `SeSystemProfilePrivilege`),
    /// without being able to create the lock.
    ControlsEtw,
    /// Neither: an ordinary user or an administrator's unelevated token.
    Ordinary,
}

/// The held instance lock: the private namespace and the mutex whose open
/// handle is the lock. Dropping it frees the lock (closes the mutex) and
/// the namespace. One per process.
pub struct InstanceLock {
    #[cfg(windows)]
    mutex: windows_sys::Win32::Foundation::HANDLE,
    #[cfg(windows)]
    namespace: windows_sys::Win32::Foundation::HANDLE,
}

// SAFETY: the lock owns two OS handles it never hands out and uses only in
// `Drop`; nothing reads or mutates them concurrently. The agent holds the
// guard in `main`'s future, which tokio may poll on any worker thread.
unsafe impl Send for InstanceLock {}

impl std::fmt::Debug for InstanceLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The raw handles are not meaningful to print.
        f.write_str("InstanceLock")
    }
}

#[cfg(windows)]
impl Drop for InstanceLock {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::ClosePrivateNamespace;
        // SAFETY: both handles were produced by the acquire path below and
        // are closed exactly once here. Closing the last mutex handle frees
        // the lock; the namespace is closed without the destroy flag.
        unsafe {
            if !self.mutex.is_null() {
                CloseHandle(self.mutex);
            }
            if !self.namespace.is_null() {
                ClosePrivateNamespace(self.namespace, 0);
            }
        }
    }
}

/// Acquire the per-host instance lock. Windows only; elsewhere there is no
/// capture, so there is no lock and the start goes on
/// ([`LockOutcome::Unprivileged`]).
#[cfg(not(windows))]
pub fn acquire() -> LockOutcome {
    LockOutcome::Unprivileged
}

#[cfg(windows)]
pub use imp::{acquire, classify_token};

#[cfg(windows)]
mod imp {
    use super::{InstanceLock, LockOutcome, TokenClass};
    use windows_sys::core::{w, PCWSTR};
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, LocalFree, ERROR_ALREADY_EXISTS, HANDLE, LUID,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{
        CreateWellKnownSid, EqualSid, GetTokenInformation, LookupPrivilegeValueW, TokenGroups,
        TokenPrivileges, WinBuiltinAdministratorsSid, WinBuiltinPerfLoggingUsersSid,
        WinLocalServiceSid, WinNetworkServiceSid, SECURITY_ATTRIBUTES, SECURITY_MAX_SID_SIZE,
        SE_SYSTEM_PROFILE_NAME, TOKEN_GROUPS, TOKEN_PRIVILEGES, TOKEN_QUERY, WELL_KNOWN_SID_TYPE,
    };
    use windows_sys::Win32::System::Threading::{
        AddSIDToBoundaryDescriptor, CreateBoundaryDescriptorW, CreateMutexW,
        CreatePrivateNamespaceW, GetCurrentProcess, OpenPrivateNamespaceW, OpenProcessToken,
    };

    /// `SE_GROUP_ENABLED` from `TOKEN_GROUPS` attributes (Win32
    /// `winnt.h`). Defined here to avoid pulling the whole
    /// `Win32_System_SystemServices` feature for one ABI-stable value.
    const SE_GROUP_ENABLED: u32 = 0x0000_0004;

    /// The boundary descriptor's name, the namespace alias and the mutex's
    /// name inside it. The mutex name is `<alias>\<leaf>`.
    const BOUNDARY_NAME: PCWSTR = w!("CyberGuardAgentBoundary");
    const NAMESPACE_ALIAS: PCWSTR = w!("CyberGuardAgent");
    const MUTEX_NAME: PCWSTR = w!("CyberGuardAgent\\InstanceLock");
    /// DACL: protected, full control to built-in Administrators (BA) and
    /// SYSTEM (SY), nothing else.
    const NAMESPACE_SDDL: PCWSTR = w!("D:P(A;;GA;;;BA)(A;;GA;;;SY)");

    pub fn acquire() -> LockOutcome {
        let token = match open_process_token() {
            Ok(token) => token,
            Err(code) => {
                return LockOutcome::Failed {
                    step: "open process token",
                    code,
                }
            }
        };
        let class = classify_token(token);
        // SAFETY: `token` was opened just above and is used nowhere else.
        unsafe { CloseHandle(token) };
        match class {
            TokenClass::CanCreate => create_or_detect_lock(),
            TokenClass::ControlsEtw => LockOutcome::ControlsEtwOutsideModel,
            TokenClass::Ordinary => LockOutcome::Unprivileged,
        }
    }

    /// Classify a token by its groups and privileges (read-only). Public
    /// for the tests, which synthesize tokens of each class.
    pub fn classify_token(token: HANDLE) -> TokenClass {
        if has_enabled_group(token, WinBuiltinAdministratorsSid) {
            return TokenClass::CanCreate;
        }
        if has_enabled_group(token, WinBuiltinPerfLoggingUsersSid)
            || has_enabled_group(token, WinLocalServiceSid)
            || has_enabled_group(token, WinNetworkServiceSid)
            || has_privilege(token, SE_SYSTEM_PROFILE_NAME)
        {
            return TokenClass::ControlsEtw;
        }
        TokenClass::Ordinary
    }

    fn open_process_token() -> Result<HANDLE, u32> {
        let mut token: HANDLE = std::ptr::null_mut();
        // SAFETY: the current-process pseudo-handle needs no close; `token`
        // receives a handle the caller closes.
        let ok = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) };
        if ok == 0 {
            // SAFETY: a plain Win32 status read.
            return Err(unsafe { GetLastError() });
        }
        Ok(token)
    }

    /// `GetTokenInformation` for a variable-length class, into a
    /// pointer-aligned buffer (`Vec<u64>` as in `identity.rs`).
    fn token_information(token: HANDLE, class: i32) -> Result<Vec<u64>, u32> {
        let mut needed = 0u32;
        // SAFETY: a size query (null buffer, length 0) on a valid token.
        unsafe { GetTokenInformation(token, class, std::ptr::null_mut(), 0, &mut needed) };
        let mut buffer = vec![0u64; (needed as usize).div_ceil(8).max(1)];
        let mut written = 0u32;
        // SAFETY: `buffer` holds at least `needed` bytes; the length passed
        // matches it.
        let ok = unsafe {
            GetTokenInformation(
                token,
                class,
                buffer.as_mut_ptr().cast(),
                (buffer.len() * 8) as u32,
                &mut written,
            )
        };
        if ok == 0 {
            // SAFETY: a plain Win32 status read.
            return Err(unsafe { GetLastError() });
        }
        Ok(buffer)
    }

    /// `SECURITY_MAX_SID_SIZE` bytes, counted in `u32`s so the SID buffer
    /// is DWORD-aligned (a SID's `SubAuthority` entries are `u32`).
    const SID_U32S: usize = (SECURITY_MAX_SID_SIZE as usize).div_ceil(4);

    /// The well-known SID of `sid_type`, into a DWORD-aligned stack buffer.
    fn well_known_sid(sid_type: WELL_KNOWN_SID_TYPE) -> Option<[u32; SID_U32S]> {
        let mut sid = [0u32; SID_U32S];
        let mut len = SECURITY_MAX_SID_SIZE;
        // SAFETY: the buffer holds SECURITY_MAX_SID_SIZE bytes, the size
        // passed; `len` is in/out.
        let ok = unsafe {
            CreateWellKnownSid(
                sid_type,
                std::ptr::null_mut(),
                sid.as_mut_ptr().cast(),
                &mut len,
            )
        };
        (ok != 0).then_some(sid)
    }

    /// Whether `token` has `sid_type` as an enabled group (present and not
    /// deny-only).
    fn has_enabled_group(token: HANDLE, sid_type: WELL_KNOWN_SID_TYPE) -> bool {
        let Some(mut target) = well_known_sid(sid_type) else {
            return false;
        };
        let Ok(buffer) = token_information(token, TokenGroups) else {
            return false;
        };
        // SAFETY: TokenGroups wrote a TOKEN_GROUPS into `buffer`; the array
        // of `GroupCount` SID_AND_ATTRIBUTES follows the count. Each `Sid`
        // points inside `buffer`, which outlives this scan.
        unsafe {
            let groups = buffer.as_ptr() as *const TOKEN_GROUPS;
            let count = (*groups).GroupCount as usize;
            let first = std::ptr::addr_of!((*groups).Groups)
                .cast::<windows_sys::Win32::Security::SID_AND_ATTRIBUTES>();
            for i in 0..count {
                let entry = &*first.add(i);
                if (entry.Attributes & SE_GROUP_ENABLED) != 0
                    && EqualSid(entry.Sid, target.as_mut_ptr().cast::<core::ffi::c_void>()) != 0
                {
                    return true;
                }
            }
        }
        false
    }

    /// Whether `token` holds the privilege `name` (present in the token,
    /// enabled or not — the holder can enable it on demand).
    fn has_privilege(token: HANDLE, name: PCWSTR) -> bool {
        let mut luid = LUID {
            LowPart: 0,
            HighPart: 0,
        };
        // SAFETY: a local name lookup into `luid`.
        if unsafe { LookupPrivilegeValueW(std::ptr::null(), name, &mut luid) } == 0 {
            return false;
        }
        let Ok(buffer) = token_information(token, TokenPrivileges) else {
            return false;
        };
        // SAFETY: TokenPrivileges wrote a TOKEN_PRIVILEGES into `buffer`;
        // the array of `PrivilegeCount` LUID_AND_ATTRIBUTES follows.
        unsafe {
            let privileges = buffer.as_ptr() as *const TOKEN_PRIVILEGES;
            let count = (*privileges).PrivilegeCount as usize;
            let first = std::ptr::addr_of!((*privileges).Privileges)
                .cast::<windows_sys::Win32::Security::LUID_AND_ATTRIBUTES>();
            for i in 0..count {
                let entry = &*first.add(i);
                if entry.Luid.LowPart == luid.LowPart && entry.Luid.HighPart == luid.HighPart {
                    return true;
                }
            }
        }
        false
    }

    /// Build the private namespace and the mutex; report whether we took
    /// the lock, another live holder has it, or a step failed. The caller
    /// is already known to have a token that can create the object.
    fn create_or_detect_lock() -> LockOutcome {
        // The boundary descriptor with the Administrators SID: a token must
        // carry that SID (enabled) to enter the namespace.
        let boundary = {
            // SAFETY: a named boundary descriptor; NULL on failure.
            let mut handle = unsafe { CreateBoundaryDescriptorW(BOUNDARY_NAME, 0) };
            if handle.is_null() {
                return LockOutcome::Failed {
                    step: "create boundary descriptor",
                    code: last_error(),
                };
            }
            let Some(mut admins) = well_known_sid(WinBuiltinAdministratorsSid) else {
                // SAFETY: `handle` was created above.
                unsafe { delete_boundary(handle) };
                return LockOutcome::Failed {
                    step: "build Administrators SID",
                    code: last_error(),
                };
            };
            // SAFETY: `handle` is a valid boundary descriptor; `admins`
            // holds a valid SID that outlives the call. The call may move
            // the descriptor, so it takes `&mut handle`.
            let ok = unsafe {
                AddSIDToBoundaryDescriptor(
                    &mut handle,
                    admins.as_mut_ptr().cast::<core::ffi::c_void>(),
                )
            };
            if ok == 0 {
                let code = last_error();
                // SAFETY: `handle` was created above.
                unsafe { delete_boundary(handle) };
                return LockOutcome::Failed {
                    step: "add SID to boundary descriptor",
                    code,
                };
            }
            handle
        };

        // The namespace: create it, or open an existing one (a live sibling
        // created it). Its own DACL admits only Administrators and SYSTEM.
        let namespace = match open_or_create_namespace(boundary) {
            Ok(namespace) => namespace,
            Err(outcome) => {
                // SAFETY: `boundary` was created above and is done with.
                unsafe { delete_boundary(boundary) };
                return outcome;
            }
        };
        // SAFETY: the boundary descriptor is no longer needed once the
        // namespace is created or opened.
        unsafe { delete_boundary(boundary) };

        // The mutex inside the namespace. Its existence is the lock: if it
        // already exists, a live sibling holds a handle to it.
        // SAFETY: a named mutex, no initial ownership, the name inside our
        // namespace; NULL on failure.
        let mutex = unsafe { CreateMutexW(std::ptr::null(), 0, MUTEX_NAME) };
        if mutex.is_null() {
            let code = last_error();
            // SAFETY: `namespace` was created/opened above.
            unsafe { close_namespace(namespace) };
            return LockOutcome::Failed {
                step: "create mutex",
                code,
            };
        }
        if last_error() == ERROR_ALREADY_EXISTS {
            // Another live process holds the lock.
            // SAFETY: both handles were produced just above.
            unsafe {
                CloseHandle(mutex);
                close_namespace(namespace);
            }
            return LockOutcome::Held;
        }
        LockOutcome::Acquired(InstanceLock { mutex, namespace })
    }

    /// Create the private namespace, or open it if a live sibling already
    /// did (`ERROR_ALREADY_EXISTS`).
    fn open_or_create_namespace(boundary: HANDLE) -> Result<HANDLE, LockOutcome> {
        let security_descriptor = match parse_sddl(NAMESPACE_SDDL) {
            Ok(sd) => sd,
            Err(code) => {
                return Err(LockOutcome::Failed {
                    step: "parse namespace security descriptor",
                    code,
                })
            }
        };
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: security_descriptor,
            bInheritHandle: 0,
        };
        // SAFETY: `boundary` is a valid boundary descriptor; `attributes`
        // and its security descriptor outlive the call.
        let namespace = unsafe {
            CreatePrivateNamespaceW(
                &attributes,
                boundary.cast::<core::ffi::c_void>(),
                NAMESPACE_ALIAS,
            )
        };
        let create_error = last_error();
        // SAFETY: the security descriptor was LocalAlloc'd by the SDDL
        // parse and is not needed after the create call.
        unsafe { LocalFree(security_descriptor) };
        if !namespace.is_null() {
            return Ok(namespace);
        }
        if create_error != ERROR_ALREADY_EXISTS {
            return Err(LockOutcome::Failed {
                step: "create private namespace",
                code: create_error,
            });
        }
        // SAFETY: `boundary` is a valid boundary descriptor.
        let namespace =
            unsafe { OpenPrivateNamespaceW(boundary.cast::<core::ffi::c_void>(), NAMESPACE_ALIAS) };
        if namespace.is_null() {
            return Err(LockOutcome::Failed {
                step: "open private namespace",
                code: last_error(),
            });
        }
        Ok(namespace)
    }

    /// Parse an SDDL string to a self-relative security descriptor
    /// (`LocalAlloc`'d; the caller `LocalFree`s it).
    fn parse_sddl(sddl: PCWSTR) -> Result<*mut core::ffi::c_void, u32> {
        let mut descriptor: windows_sys::Win32::Security::PSECURITY_DESCRIPTOR =
            std::ptr::null_mut();
        // SAFETY: a local parse; `descriptor` receives a LocalAlloc'd SD.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl,
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(last_error());
        }
        Ok(descriptor)
    }

    /// SAFETY: `handle` is a boundary descriptor from
    /// `CreateBoundaryDescriptorW`.
    unsafe fn delete_boundary(handle: HANDLE) {
        windows_sys::Win32::System::Threading::DeleteBoundaryDescriptor(handle);
    }

    /// SAFETY: `handle` is a namespace handle from `CreatePrivateNamespaceW`
    /// or `OpenPrivateNamespaceW`.
    unsafe fn close_namespace(handle: HANDLE) {
        windows_sys::Win32::System::Threading::ClosePrivateNamespace(handle, 0);
    }

    fn last_error() -> u32 {
        // SAFETY: a plain Win32 status read.
        unsafe { GetLastError() }
    }
}
