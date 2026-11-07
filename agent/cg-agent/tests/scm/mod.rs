//! The shared harness of `cg-agent`'s service and SYSTEM tests (SPEC-024
//! Amendment 2026-11-01, point 4): a uniquely named service this test owns,
//! the Application-log entries of source `CyberGuardAgent` (the second
//! Amendment 2026-11-01, item 4), and the fixed installed tree under
//! `%ProgramData%` (§Data contracts §2 and the Amendment 2026-11-04,
//! items 1 to 4) planted with its production security.
//!
//! Included by each SCM test target with `mod scm;`, after `mod common;`;
//! those targets are `#![cfg(windows)]` and build only with the test-only
//! `scm-tests` feature. Everything here acts only on a GitHub-hosted runner
//! that is discarded after the job: `TestService` and `TestUser` refuse to
//! act anywhere else, and neither do `FixedDataRoot`, which moves the
//! host's `<ProgramData>\CyberGuard` aside, and `ConsoleAgent`, which runs
//! a real console agent elevated at those paths. On that runner a test may
//! use the fixed paths (point 4).
//!
//! The tests do not read the agent's own custody code: they spell the fixed
//! paths and the SDDLs themselves, from the SPEC, so a probe pull request
//! can build them against the tree without the implementation.
#![allow(dead_code)]

use std::io::{Read as _, Write as _};
use std::os::windows::io::{FromRawHandle, RawHandle};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use windows_sys::core::PCWSTR;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND,
    ERROR_INVALID_OWNER, ERROR_IO_PENDING, ERROR_NOT_ALL_ASSIGNED, ERROR_NO_MORE_FILES,
    ERROR_OPLOCK_NOT_GRANTED, ERROR_PATH_NOT_FOUND, ERROR_SERVICE_DOES_NOT_EXIST,
    ERROR_SERVICE_SPECIFIC_ERROR, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE, LUID,
    UNICODE_STRING, WAIT_OBJECT_0,
};
use windows_sys::Win32::NetworkManagement::NetManagement::{
    NERR_Success, NetUserAdd, NetUserDel, UF_DONT_EXPIRE_PASSWD, UF_SCRIPT, USER_INFO_1,
    USER_PRIV_USER,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertSidToStringSidW,
    ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SetSecurityInfo,
    SDDL_REVISION_1, SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, CreateRestrictedToken, CreateWellKnownSid, GetSecurityDescriptorOwner,
    GetTokenInformation, ImpersonateLoggedOnUser, IsWellKnownSid, LogonUserW,
    LookupPrivilegeValueW, RevertToSelf, SetKernelObjectSecurity, TokenGroups, TokenPrivileges,
    TokenUser, WinBuiltinAdministratorsSid, DACL_SECURITY_INFORMATION, DISABLE_MAX_PRIVILEGE,
    LOGON32_LOGON_INTERACTIVE, LOGON32_LOGON_NETWORK, LOGON32_PROVIDER_DEFAULT,
    LUID_AND_ATTRIBUTES, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
    SECURITY_ATTRIBUTES, SECURITY_MAX_SID_SIZE, SE_BACKUP_NAME, SE_PRIVILEGE_ENABLED,
    SE_RESTORE_NAME, SE_TAKE_OWNERSHIP_NAME, SID_AND_ATTRIBUTES, TOKEN_ADJUST_PRIVILEGES,
    TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE, TOKEN_GROUPS, TOKEN_IMPERSONATE,
    TOKEN_INFORMATION_CLASS, TOKEN_PRIVILEGES, TOKEN_QUERY, TOKEN_USER, WELL_KNOWN_SID_TYPE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateDirectoryW, CreateFileW, FileFullDirectoryInfo, FileFullDirectoryRestartInfo,
    GetFileInformationByHandleEx, CREATE_NEW, DELETE, FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY,
    FILE_ATTRIBUTE_NORMAL, FILE_DELETE_CHILD, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_DELETE_ON_CLOSE, FILE_FLAG_OPEN_REPARSE_POINT, FILE_FLAG_OVERLAPPED,
    FILE_FULL_DIR_INFO, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, READ_CONTROL, SYNCHRONIZE, WRITE_DAC,
    WRITE_OWNER,
};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::EventLog::{
    EvtClose, EvtNext, EvtQuery, EvtQueryChannelPath, EvtQueryReverseDirection, EvtRender,
    EvtRenderEventXml,
};
use windows_sys::Win32::System::Services::{
    CloseServiceHandle, ControlService, CreateServiceW, DeleteService, OpenSCManagerW,
    OpenServiceW, QueryServiceStatusEx, StartServiceW, SC_HANDLE, SC_MANAGER_CONNECT,
    SC_MANAGER_CREATE_SERVICE, SC_STATUS_PROCESS_INFO, SERVICE_ALL_ACCESS, SERVICE_CONTROL_STOP,
    SERVICE_DEMAND_START, SERVICE_ERROR_NORMAL, SERVICE_QUERY_STATUS, SERVICE_START_PENDING,
    SERVICE_STATUS, SERVICE_STATUS_PROCESS, SERVICE_STOPPED, SERVICE_STOP_PENDING,
    SERVICE_WIN32_OWN_PROCESS,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentProcess, OpenProcess, OpenProcessToken, TerminateProcess,
    WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
};
use windows_sys::Win32::UI::Shell::{FOLDERID_ProgramData, SHGetKnownFolderPath};

use crate::common;

/// A group of a token that is for deny only (winnt.h; its binding lives in
/// a windows-sys feature these tests do not enable).
const SE_GROUP_USE_FOR_DENY_ONLY: u32 = 0x10;

/// What an oplock request and a create relative to a handle need, whose
/// bindings also live in windows-sys features these tests do not enable.
#[repr(C)]
struct Overlapped {
    internal: usize,
    internal_high: usize,
    offset: u32,
    offset_high: u32,
    event: HANDLE,
}

#[repr(C)]
struct ObjectAttributes {
    length: u32,
    root_directory: HANDLE,
    object_name: *const UNICODE_STRING,
    attributes: u32,
    security_descriptor: *const std::ffi::c_void,
    security_quality_of_service: *const std::ffi::c_void,
}

#[repr(C)]
struct IoStatusBlock {
    status: usize,
    information: usize,
}

#[link(name = "kernel32")]
extern "system" {
    fn DeviceIoControl(
        device: HANDLE,
        control_code: u32,
        input: *const std::ffi::c_void,
        input_size: u32,
        output: *mut std::ffi::c_void,
        output_size: u32,
        returned: *mut u32,
        overlapped: *mut Overlapped,
    ) -> i32;
}

#[link(name = "ntdll")]
extern "system" {
    fn NtCreateFile(
        file: *mut HANDLE,
        access: u32,
        attributes: *const ObjectAttributes,
        status: *mut IoStatusBlock,
        allocation_size: *const i64,
        file_attributes: u32,
        share: u32,
        disposition: u32,
        options: u32,
        extended_attributes: *const std::ffi::c_void,
        extended_attributes_size: u32,
    ) -> i32;
}

/// `CTL_CODE(FILE_DEVICE_FILE_SYSTEM, 2, METHOD_BUFFERED, FILE_ANY_ACCESS)`.
const FSCTL_REQUEST_BATCH_OPLOCK: u32 = 0x0009_0008;
const OBJ_CASE_INSENSITIVE: u32 = 0x40;
const FILE_CREATE: u32 = 2;
const FILE_SYNCHRONOUS_IO_NONALERT: u32 = 0x20;
const FILE_NON_DIRECTORY_FILE: u32 = 0x40;
const FILE_DELETE_ON_CLOSE: u32 = 0x1000;

/// The `NTSTATUS` of a create the object's security refuses.
pub const STATUS_ACCESS_DENIED: i32 = 0xC000_0022_u32 as i32;

/// The product's service name (SPEC-024 §Data contracts §3); these tests
/// never create, open for change, stop or delete it.
pub const PRODUCT_SERVICE: &str = "CyberGuardAgent";

/// The service stop deadline (SPEC-024 §Operational §3).
pub const STOP_DEADLINE: Duration = Duration::from_secs(60);

pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn wide_path(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

pub fn last_error() -> u32 {
    // SAFETY: reads the calling thread's last-error value.
    unsafe { GetLastError() }
}

/// A NUL-terminated wide string Windows allocated, as a `String`.
///
/// # Safety
/// `text` points to a NUL-terminated UTF-16 string.
unsafe fn from_wide_nul(text: *const u16) -> String {
    let len = (0..).take_while(|&i| *text.add(i) != 0).count();
    String::from_utf16_lossy(std::slice::from_raw_parts(text, len))
}

// ---------------------------------------------------------------------------
// The SCM side: a uniquely named service this test owns.
// ---------------------------------------------------------------------------

/// An SCM handle, closed on drop.
pub struct Scm(SC_HANDLE);

impl Scm {
    pub fn open() -> Self {
        // SAFETY: null machine and database name the local active database.
        let handle = unsafe {
            OpenSCManagerW(
                std::ptr::null(),
                std::ptr::null(),
                SC_MANAGER_CONNECT | SC_MANAGER_CREATE_SERVICE,
            )
        };
        assert!(
            !handle.is_null(),
            "OpenSCManagerW: Win32 error {}",
            last_error()
        );
        Scm(handle)
    }
}

impl Drop for Scm {
    fn drop(&mut self) {
        // SAFETY: a handle OpenSCManagerW returned, closed once.
        unsafe { CloseServiceHandle(self.0) };
    }
}

/// Fail without acting unless this is a GitHub-hosted runner and no
/// service named `CyberGuardAgent` exists (SPEC-024 Amendment 2026-11-01,
/// point 4).
pub fn refuse_unless_disposable_runner(scm: &Scm) {
    let actions = std::env::var("GITHUB_ACTIONS").ok();
    let environment = std::env::var("RUNNER_ENVIRONMENT").ok();
    assert!(
        actions.as_deref() == Some("true") && environment.as_deref() == Some("github-hosted"),
        "refusing to create a Windows service or account outside a GitHub-hosted runner \
         (GITHUB_ACTIONS={actions:?}, RUNNER_ENVIRONMENT={environment:?})"
    );
    let name = wide(PRODUCT_SERVICE);
    // SAFETY: the name lives for the call; the handle, if any, is closed.
    let handle = unsafe { OpenServiceW(scm.0, name.as_ptr(), SERVICE_QUERY_STATUS) };
    if !handle.is_null() {
        // SAFETY: a handle OpenServiceW returned.
        unsafe { CloseServiceHandle(handle) };
        panic!("refusing to act: a service named {PRODUCT_SERVICE} exists on this host");
    }
    let error = last_error();
    assert_eq!(
        error, ERROR_SERVICE_DOES_NOT_EXIST,
        "cannot establish that {PRODUCT_SERVICE} is absent: Win32 error {error}"
    );
}

/// What polling a service saw on its way to a state.
pub struct Observed {
    pub status: SERVICE_STATUS_PROCESS,
    /// The longest time a pending state kept the same checkpoint.
    pub longest_checkpoint_gap: Duration,
    /// The largest wait hint a pending state reported, in milliseconds.
    pub largest_wait_hint: u32,
    /// How many times a pending state's checkpoint was seen to advance.
    pub checkpoint_advances: u32,
}

/// A service this test created, LocalSystem and own-process, running the
/// agent binary with `args`. `Drop` stops it if it runs (killing its
/// process only if it does not stop) and deletes it.
pub struct TestService {
    /// Closed after the service handle (fields drop after `drop`).
    _scm: Scm,
    handle: SC_HANDLE,
    pub name: String,
}

impl TestService {
    pub fn create(args: &str) -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let scm = Scm::open();
        refuse_unless_disposable_runner(&scm);
        let name = format!(
            "CgAgentTest-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        );
        let command = format!("\"{}\" {args}", env!("CARGO_BIN_EXE_cg-agent"));
        let (wname, wdisplay, wcommand) = (wide(&name), wide(&name), wide(&command));
        // SAFETY: every string lives for the call; a null account is
        // LocalSystem, a null password is none.
        let handle = unsafe {
            CreateServiceW(
                scm.0,
                wname.as_ptr(),
                wdisplay.as_ptr(),
                SERVICE_ALL_ACCESS,
                SERVICE_WIN32_OWN_PROCESS,
                SERVICE_DEMAND_START,
                SERVICE_ERROR_NORMAL,
                wcommand.as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        assert!(
            !handle.is_null(),
            "CreateServiceW {name}: Win32 error {}",
            last_error()
        );
        eprintln!("created service {name}: {command}");
        TestService {
            _scm: scm,
            handle,
            name,
        }
    }

    pub fn start(&self) -> Result<(), u32> {
        // SAFETY: no arguments.
        if unsafe { StartServiceW(self.handle, 0, std::ptr::null()) } == 0 {
            return Err(last_error());
        }
        Ok(())
    }

    /// Start the service with SCM start arguments, which it reads after its
    /// name (SPEC-024 Amendment 2026-11-05, item 1).
    pub fn start_with(&self, arguments: &[&str]) -> Result<(), u32> {
        if arguments.is_empty() {
            return self.start();
        }
        let wide_arguments: Vec<Vec<u16>> = arguments.iter().map(|text| wide(text)).collect();
        let vector: Vec<*const u16> = wide_arguments.iter().map(|text| text.as_ptr()).collect();
        // SAFETY: `vector` holds `arguments.len()` NUL-terminated strings,
        // which `wide_arguments` keeps alive for the call.
        if unsafe { StartServiceW(self.handle, vector.len() as u32, vector.as_ptr()) } == 0 {
            return Err(last_error());
        }
        Ok(())
    }

    pub fn status(&self) -> SERVICE_STATUS_PROCESS {
        self.query_status().unwrap_or_else(|error| {
            panic!("QueryServiceStatusEx {}: Win32 error {error}", self.name)
        })
    }

    /// The service's status, or the Win32 error of the query: `Drop` must
    /// not panic, since a panic while unwinding aborts the process and skips
    /// every other guard's cleanup.
    fn query_status(&self) -> Result<SERVICE_STATUS_PROCESS, u32> {
        // SAFETY: an all-zero SERVICE_STATUS_PROCESS is a valid value.
        let mut status: SERVICE_STATUS_PROCESS = unsafe { std::mem::zeroed() };
        let mut needed = 0u32;
        // SAFETY: the buffer is one SERVICE_STATUS_PROCESS.
        let ok = unsafe {
            QueryServiceStatusEx(
                self.handle,
                SC_STATUS_PROCESS_INFO,
                (&mut status as *mut SERVICE_STATUS_PROCESS).cast(),
                std::mem::size_of::<SERVICE_STATUS_PROCESS>() as u32,
                &mut needed,
            )
        };
        if ok == 0 {
            return Err(last_error());
        }
        Ok(status)
    }

    pub fn control_stop(&self) -> Result<(), u32> {
        // SAFETY: an all-zero SERVICE_STATUS is a valid out-buffer.
        let mut status: SERVICE_STATUS = unsafe { std::mem::zeroed() };
        // SAFETY: the out-buffer is one SERVICE_STATUS.
        if unsafe { ControlService(self.handle, SERVICE_CONTROL_STOP, &mut status) } == 0 {
            return Err(last_error());
        }
        Ok(())
    }

    /// Poll until the service reports `state`, failing after `timeout`.
    pub fn wait_for(&self, state: u32, timeout: Duration) -> Observed {
        let started = Instant::now();
        let mut last_seen: Option<(u32, u32)> = None;
        let mut last_change = started;
        let mut longest_checkpoint_gap = Duration::ZERO;
        let mut largest_wait_hint = 0u32;
        let mut checkpoint_advances = 0u32;
        loop {
            let status = self.status();
            let now = Instant::now();
            let seen = (status.dwCurrentState, status.dwCheckPoint);
            if last_seen != Some(seen) {
                if let Some((previous_state, previous_checkpoint)) = last_seen {
                    if previous_state == status.dwCurrentState
                        && is_pending(previous_state)
                        && status.dwCheckPoint > previous_checkpoint
                    {
                        checkpoint_advances += 1;
                    }
                    if previous_state == status.dwCurrentState || is_pending(previous_state) {
                        longest_checkpoint_gap =
                            longest_checkpoint_gap.max(now.duration_since(last_change));
                    }
                }
                last_seen = Some(seen);
                last_change = now;
            }
            if is_pending(status.dwCurrentState) {
                largest_wait_hint = largest_wait_hint.max(status.dwWaitHint);
            }
            if status.dwCurrentState == state {
                return Observed {
                    status,
                    longest_checkpoint_gap,
                    largest_wait_hint,
                    checkpoint_advances,
                };
            }
            assert!(
                now.duration_since(started) < timeout,
                "{} did not reach state {state} within {timeout:?}: state {}, exit {} / {}",
                self.name,
                status.dwCurrentState,
                status.dwWin32ExitCode,
                status.dwServiceSpecificExitCode
            );
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    /// Start the service and wait for it to stop on its own, as a start
    /// that fails before `RUNNING` does. A start the SCM refuses because
    /// the service already stopped is that outcome too.
    pub fn start_expecting_stop(&self, timeout: Duration) -> SERVICE_STATUS_PROCESS {
        self.start_and_wait_for_stop(timeout).status
    }

    /// As `start_expecting_stop`, with what polling saw on the way.
    pub fn start_and_wait_for_stop(&self, timeout: Duration) -> Observed {
        self.start_with_and_wait_for_stop(&[], timeout)
    }

    /// As `start_and_wait_for_stop`, with SCM start arguments.
    pub fn start_with_and_wait_for_stop(&self, arguments: &[&str], timeout: Duration) -> Observed {
        if let Err(code) = self.start_with(arguments) {
            eprintln!("{}: StartServiceW returned Win32 error {code}", self.name);
        }
        self.wait_for(SERVICE_STOPPED, timeout)
    }
}

impl Drop for TestService {
    fn drop(&mut self) {
        // No assertion here: see `query_status`. A failed query counts as
        // not stopped, so the stop and the kill are still attempted.
        let stopped = |status: &Result<SERVICE_STATUS_PROCESS, u32>| {
            status
                .as_ref()
                .is_ok_and(|status| status.dwCurrentState == SERVICE_STOPPED)
        };
        if !stopped(&self.query_status()) {
            let _ = self.control_stop();
            let deadline = Instant::now() + Duration::from_secs(90);
            while !stopped(&self.query_status()) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(250));
            }
            let status = self.query_status();
            if let Err(error) = &status {
                eprintln!("QueryServiceStatusEx {}: Win32 error {error}", self.name);
            }
            let running = status
                .ok()
                .filter(|s| s.dwCurrentState != SERVICE_STOPPED && s.dwProcessId != 0);
            if let Some(status) = running {
                // The process is this test's own service's; it did not stop.
                // SAFETY: a process handle opened for termination, closed once.
                unsafe {
                    let process = OpenProcess(PROCESS_TERMINATE, 0, status.dwProcessId);
                    if !process.is_null() {
                        TerminateProcess(process, 1);
                        CloseHandle(process);
                    }
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        }
        // SAFETY: the handle this test created; deleted and closed once.
        unsafe {
            if DeleteService(self.handle) == 0 {
                eprintln!(
                    "DeleteService {}: Win32 error {}",
                    self.name,
                    GetLastError()
                );
            }
            CloseServiceHandle(self.handle);
        }
    }
}

pub fn is_pending(state: u32) -> bool {
    state == SERVICE_START_PENDING || state == SERVICE_STOP_PENDING
}

/// SPEC-024 second Amendment 2026-11-01, item 3: in both pending states the
/// checkpoint advances at least every 10 seconds, with a wait hint of at
/// most 30 seconds.
pub fn assert_pending_progress(observed: &Observed, what: &str) {
    assert!(
        observed.longest_checkpoint_gap <= Duration::from_millis(10_500),
        "{what}: a pending state kept its checkpoint for {:?} (at most 10 s)",
        observed.longest_checkpoint_gap
    );
    assert!(
        observed.largest_wait_hint <= 30_000,
        "{what}: a pending state reported a wait hint of {} ms (at most 30 s)",
        observed.largest_wait_hint
    );
}

/// `STOPPED` after a failure: `ERROR_SERVICE_SPECIFIC_ERROR` and the
/// cause's code (item 3).
pub fn assert_stopped_with(status: &SERVICE_STATUS_PROCESS, code: u32) {
    assert_eq!(
        (status.dwWin32ExitCode, status.dwServiceSpecificExitCode),
        (ERROR_SERVICE_SPECIFIC_ERROR, code),
        "STOPPED carries ERROR_SERVICE_SPECIFIC_ERROR and the cause's code {code}"
    );
}

// ---------------------------------------------------------------------------
// The Application log: the entries of source CyberGuardAgent.
// ---------------------------------------------------------------------------

/// One Application-log entry, from its rendered XML.
#[derive(Debug)]
pub struct Entry {
    pub record_id: u64,
    pub event_id: u32,
    pub level: u32,
    /// The entry's strings, joined by newlines (XML-escaped as rendered).
    pub data: String,
}

/// The newest records of the Application log matching `xpath`, newest
/// first, at most `max`.
pub fn application_entries(xpath: &str, max: usize) -> Vec<Entry> {
    let (channel, query) = (wide("Application"), wide(xpath));
    // SAFETY: the strings live for the call.
    let results = unsafe {
        EvtQuery(
            0,
            channel.as_ptr(),
            query.as_ptr(),
            EvtQueryChannelPath | EvtQueryReverseDirection,
        )
    };
    assert_ne!(
        results,
        0,
        "query the Application log: Win32 error {}",
        last_error()
    );
    let mut entries = Vec::new();
    while entries.len() < max {
        let mut handles = [0isize; 16];
        let mut returned = 0u32;
        // SAFETY: room for 16 handles.
        if unsafe { EvtNext(results, 16, handles.as_mut_ptr(), 0, 0, &mut returned) } == 0 {
            break;
        }
        for &handle in &handles[..returned as usize] {
            if entries.len() < max {
                entries.push(render_entry(handle));
            }
            // SAFETY: a handle EvtNext returned, closed once.
            unsafe { EvtClose(handle) };
        }
        if returned == 0 {
            break;
        }
    }
    // SAFETY: the query handle, closed once.
    unsafe { EvtClose(results) };
    entries
}

fn render_entry(handle: isize) -> Entry {
    let mut buffer = vec![0u16; 8192];
    let (mut used, mut count) = (0u32, 0u32);
    // SAFETY: the buffer's size in bytes is passed; on
    // ERROR_INSUFFICIENT_BUFFER `used` holds the size needed.
    let mut ok = unsafe {
        EvtRender(
            0,
            handle,
            EvtRenderEventXml,
            (buffer.len() * 2) as u32,
            buffer.as_mut_ptr().cast(),
            &mut used,
            &mut count,
        )
    };
    if ok == 0 {
        buffer = vec![0u16; (used as usize).div_ceil(2) + 1];
        // SAFETY: as above, with the size Windows asked for.
        ok = unsafe {
            EvtRender(
                0,
                handle,
                EvtRenderEventXml,
                (buffer.len() * 2) as u32,
                buffer.as_mut_ptr().cast(),
                &mut used,
                &mut count,
            )
        };
    }
    assert_ne!(ok, 0, "render an event: Win32 error {}", last_error());
    let xml = String::from_utf16_lossy(&buffer[..(used as usize / 2).saturating_sub(1)]);
    Entry {
        record_id: element_number(&xml, "EventRecordID"),
        event_id: element_number(&xml, "EventID") as u32,
        level: element_number(&xml, "Level") as u32,
        data: elements(&xml, "Data").join("\n"),
    }
}

/// The text of every `<tag ...>text</tag>` element of `xml`.
fn elements(xml: &str, tag: &str) -> Vec<String> {
    let (open, close) = (format!("<{tag}"), format!("</{tag}>"));
    let mut found = Vec::new();
    let mut rest = xml;
    while let Some(at) = rest.find(&open) {
        let after = &rest[at + open.len()..];
        // The tag's own name, not a longer one (`<Data` vs `<DataItem`).
        if !after.starts_with('>') && !after.starts_with(' ') {
            rest = after;
            continue;
        }
        let Some(gt) = after.find('>') else { break };
        if after[..gt].ends_with('/') {
            rest = &after[gt + 1..];
            continue;
        }
        let body = &after[gt + 1..];
        let Some(end) = body.find(&close) else { break };
        found.push(body[..end].to_string());
        rest = &body[end + close.len()..];
    }
    found
}

fn element_number(xml: &str, tag: &str) -> u64 {
    elements(xml, tag)
        .first()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or_else(|| panic!("<{tag}> in {xml}"))
}

/// The newest record id of the Application log, before a test acts.
pub fn application_baseline() -> u64 {
    application_entries("*", 1)
        .first()
        .map_or(0, |entry| entry.record_id)
}

/// The entries of source `CyberGuardAgent` written after `baseline`,
/// oldest first.
pub fn agent_entries_after(baseline: u64) -> Vec<Entry> {
    let xpath =
        format!("*[System[Provider[@Name='{PRODUCT_SERVICE}'] and EventRecordID > {baseline}]]");
    let mut entries = application_entries(&xpath, 200);
    entries.reverse();
    entries
}

/// The entries after `baseline` once every id of `expected` is among them,
/// waiting up to 10 s for the event log to commit them.
pub fn wait_entries(baseline: u64, expected: &[u32]) -> Vec<Entry> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let entries = agent_entries_after(baseline);
        let complete = expected
            .iter()
            .all(|id| entries.iter().any(|entry| entry.event_id == *id));
        if complete || Instant::now() >= deadline {
            return entries;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Item 3: a start that fails before `RUNNING` writes no entry 1000.
pub fn assert_never_ran(entries: &[Entry]) {
    assert!(
        !entries.iter().any(|entry| entry.event_id == 1000),
        "the service never reported RUNNING: {entries:#?}"
    );
}

pub fn entry(entries: &[Entry], event_id: u32) -> &Entry {
    entries
        .iter()
        .find(|entry| entry.event_id == event_id)
        .unwrap_or_else(|| panic!("an entry {event_id} of source {PRODUCT_SERVICE}: {entries:#?}"))
}

/// Levels of the entries (SPEC-024 second Amendment, item 4).
pub const LEVEL_ERROR: u32 = 2;
pub const LEVEL_INFORMATION: u32 = 4;

/// Assert entry 1002 is an error and carries each of `fragments` (the
/// cause's line, SPEC-024 second Amendment 2026-11-01, item 4). Fragments
/// avoid characters the rendered XML escapes (`'`, `&`, `<`, `>`).
pub fn assert_failure_entry(entries: &[Entry], fragments: &[&str]) {
    let failed = entry(entries, 1002);
    assert_eq!(failed.level, LEVEL_ERROR, "1002 is an error: {failed:?}");
    for fragment in fragments {
        assert!(
            failed.data.contains(fragment),
            "1002 carries {fragment:?} of the cause's line: {failed:?}"
        );
    }
}

/// A start refused before `RUNNING` (the second Amendment 2026-11-01, item
/// 3): `STOPPED` with `code`, entry 1002 with `fragments`, no entry 1000.
pub fn refused_before_running(service: &TestService, code: u32, fragments: &[&str]) -> Vec<Entry> {
    let baseline = application_baseline();
    let stopped = service.start_expecting_stop(Duration::from_secs(60));
    assert_stopped_with(&stopped, code);
    let entries = wait_entries(baseline, &[1002]);
    assert_never_ran(&entries);
    assert_failure_entry(&entries, fragments);
    entries
}

/// As `refused_before_running`, for a start with SCM start arguments, which
/// may run a migration during `START_PENDING` (SPEC-024 Amendment
/// 2026-11-05, item 1): the checkpoint keeps advancing until the start
/// stops.
pub fn refused_with_arguments(
    service: &TestService,
    arguments: &[&str],
    code: u32,
    fragments: &[&str],
) -> Vec<Entry> {
    let baseline = application_baseline();
    let stopped = service.start_with_and_wait_for_stop(arguments, Duration::from_secs(120));
    assert_pending_progress(&stopped, "START_PENDING");
    assert_stopped_with(&stopped.status, code);
    let entries = wait_entries(baseline, &[1002]);
    assert_never_ran(&entries);
    assert_failure_entry(&entries, fragments);
    entries
}

/// A start that reports `RUNNING` and then fails, at the identity step
/// (SPEC-024 Amendment 2026-11-04, item 3, and item 5 for code 3):
/// `START_PENDING` progressed, entry 1000 precedes entry 1002, which
/// carries `fragments`, and `STOPPED` carries `code`. `RUNNING` is proved
/// by entry 1000, not by polling, which a failure milliseconds later can
/// outrun.
pub fn failed_after_running(service: &TestService, code: u32, fragments: &[&str]) -> Vec<Entry> {
    let baseline = application_baseline();
    let stopped = service.start_and_wait_for_stop(Duration::from_secs(90));
    assert_pending_progress(&stopped, "START_PENDING");
    assert_stopped_with(&stopped.status, code);
    let entries = wait_entries(baseline, &[1000, 1002]);
    let running = entry(&entries, 1000);
    let failed = entry(&entries, 1002);
    assert!(
        running.record_id < failed.record_id,
        "the service reported RUNNING before it failed: {entries:#?}"
    );
    assert!(
        !entries.iter().any(|e| e.event_id == 1001),
        "a failure is never a clean stop: {entries:#?}"
    );
    assert_failure_entry(&entries, fragments);
    entries
}

// ---------------------------------------------------------------------------
// Privileges, security descriptors and privileged file access.
// ---------------------------------------------------------------------------

/// The LUID of the privilege `name`.
fn privilege_value(name: PCWSTR) -> LUID {
    let mut luid = LUID {
        LowPart: 0,
        HighPart: 0,
    };
    // SAFETY: a constant privilege name and an out-value.
    let found = unsafe { LookupPrivilegeValueW(std::ptr::null(), name, &mut luid) };
    assert_ne!(
        found,
        0,
        "LookupPrivilegeValueW: Win32 error {}",
        last_error()
    );
    luid
}

/// Enable `SeBackupPrivilege` and `SeRestorePrivilege` in this process's
/// token; the runner's built-in Administrator holds both, disabled. With
/// them, and `FILE_FLAG_BACKUP_SEMANTICS`, this process reads, creates and
/// deletes inside the SYSTEM-only identity directory and makes SYSTEM the
/// owner of what it creates, as the installed service's own writes would
/// leave it. Idempotent.
pub fn enable_backup_and_restore() {
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: the current-process pseudo-handle; `token` is closed below.
    let opened = unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        )
    };
    assert_ne!(opened, 0, "OpenProcessToken: Win32 error {}", last_error());
    for name in [SE_BACKUP_NAME, SE_RESTORE_NAME] {
        let privileges = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: privilege_value(name),
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        // SAFETY: one privilege; no previous state is asked for.
        let ok = unsafe {
            AdjustTokenPrivileges(
                token,
                0,
                &privileges,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        let error = last_error();
        assert!(
            ok != 0 && error != ERROR_NOT_ALL_ASSIGNED,
            "enable {}: Win32 error {error} (the runner's Administrator holds it)",
            // SAFETY: a constant NUL-terminated name.
            unsafe { from_wide_nul(name) }
        );
    }
    // SAFETY: opened above, closed once.
    unsafe { CloseHandle(token) };
}

/// A handle closed on drop.
pub struct Handle(pub HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: a handle CreateFileW or a token call returned, closed
            // once.
            unsafe { CloseHandle(self.0) };
        }
    }
}

impl Handle {
    fn into_file(self) -> std::fs::File {
        let raw = self.0;
        std::mem::forget(self);
        // SAFETY: a file handle this value owned, handed to the File.
        unsafe { std::fs::File::from_raw_handle(raw as RawHandle) }
    }
}

/// A security descriptor built from SDDL, freed on drop.
struct Descriptor(PSECURITY_DESCRIPTOR);

impl Descriptor {
    fn from_sddl(sddl: &str) -> Self {
        let text = wide(sddl);
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: the string lives for the call; freed on drop.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(ok, 0, "SDDL {sddl}: Win32 error {}", last_error());
        Descriptor(descriptor)
    }

    fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0,
            bInheritHandle: 0,
        }
    }

    fn owner(&self) -> PSID {
        let mut owner: PSID = std::ptr::null_mut();
        let mut defaulted = 0;
        // SAFETY: a valid descriptor and two out-values; the SID points
        // into the descriptor.
        let ok = unsafe { GetSecurityDescriptorOwner(self.0, &mut owner, &mut defaulted) };
        assert!(ok != 0 && !owner.is_null(), "the descriptor names an owner");
        owner
    }
}

impl Drop for Descriptor {
    fn drop(&mut self) {
        // SAFETY: a LocalAlloc'd descriptor, freed once.
        unsafe { LocalFree(self.0.cast()) };
    }
}

/// `sddl` without its `O:` owner, for a creation Windows refuses an owner
/// for (`ERROR_INVALID_OWNER`); the owner is then set through the handle.
fn without_owner(sddl: &str) -> String {
    let Some(rest) = sddl.strip_prefix("O:") else {
        return sddl.to_string();
    };
    let end = ["G:", "D:", "S:"]
        .iter()
        .filter_map(|tag| rest.find(tag))
        .min()
        .unwrap_or(rest.len());
    rest[end..].to_string()
}

/// Make `owner` the owner of the object behind `handle` (opened with
/// `WRITE_OWNER`); with `SeRestorePrivilege` any SID may be set. The DACL,
/// and whether it is protected, are left as they are.
fn set_owner(handle: &Handle, owner: PSID, path: &Path) {
    // SAFETY: an open handle and a SID that outlives the call.
    let error = unsafe {
        SetSecurityInfo(
            handle.0,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            owner,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    assert_eq!(error, 0, "set the owner of {}", path.display());
}

/// Make `sid`, as SDDL spells it (an alias or `S-1-...`), the owner of
/// `path`, which exists, with `SeRestorePrivilege`; the DACL stays.
pub fn set_owner_of(path: &Path, sid: &str) {
    enable_backup_and_restore();
    let descriptor = Descriptor::from_sddl(&format!("O:{sid}"));
    let handle = open_existing(path, WRITE_OWNER, 0)
        .unwrap_or_else(|e| panic!("open {} to set its owner: Win32 error {e}", path.display()));
    set_owner(&handle, descriptor.owner(), path);
}

/// Give `path`, which exists, the owner and DACL of `sddl`, with
/// `SeRestorePrivilege`, through `SetKernelObjectSecurity`, whose change
/// applies to that object alone: `SetSecurityInfo`'s of a DACL propagates to
/// what a directory holds.
pub fn set_security(path: &Path, sddl: &str) {
    enable_backup_and_restore();
    let descriptor = Descriptor::from_sddl(sddl);
    let handle = open_existing(path, WRITE_OWNER | WRITE_DAC, 0).unwrap_or_else(|e| {
        panic!(
            "open {} to set its security: Win32 error {e}",
            path.display()
        )
    });
    // SAFETY: an open handle with both rights and a valid descriptor.
    let ok = unsafe {
        SetKernelObjectSecurity(
            handle.0,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            descriptor.0,
        )
    };
    assert_ne!(
        ok,
        0,
        "set the security of {} to {sddl}: Win32 error {}",
        path.display(),
        last_error()
    );
}

/// The well-known SID `kind`, in its binary form.
fn well_known_sid(kind: WELL_KNOWN_SID_TYPE) -> Vec<u8> {
    let mut sid = vec![0u8; SECURITY_MAX_SID_SIZE as usize];
    let mut size = SECURITY_MAX_SID_SIZE;
    // SAFETY: a buffer of SECURITY_MAX_SID_SIZE bytes.
    let ok = unsafe {
        CreateWellKnownSid(
            kind,
            std::ptr::null_mut(),
            sid.as_mut_ptr().cast(),
            &mut size,
        )
    };
    assert_ne!(ok, 0, "CreateWellKnownSid: Win32 error {}", last_error());
    sid.truncate(size as usize);
    sid
}

/// Open `path` with `access`, without following a reparse point and with
/// backup semantics (so a directory opens too, and this process's backup
/// and restore privileges apply when enabled).
fn open_existing(path: &Path, access: u32, extra_flags: u32) -> Result<Handle, u32> {
    let name = wide_path(path);
    // SAFETY: the name lives for the call; the handle is closed on drop.
    let raw = unsafe {
        CreateFileW(
            name.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT | extra_flags,
            std::ptr::null_mut(),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        return Err(last_error());
    }
    Ok(Handle(raw))
}

fn is_absent(error: u32) -> bool {
    error == ERROR_FILE_NOT_FOUND || error == ERROR_PATH_NOT_FOUND
}

/// Create the directory `path` with `sddl` as its security.
pub fn create_directory_with(path: &Path, sddl: &str) {
    enable_backup_and_restore();
    let descriptor = Descriptor::from_sddl(sddl);
    let attributes = descriptor.attributes();
    let name = wide_path(path);
    // SAFETY: the name and attributes live for the call.
    if unsafe { CreateDirectoryW(name.as_ptr(), &attributes) } != 0 {
        return;
    }
    let error = last_error();
    assert_eq!(
        error,
        ERROR_INVALID_OWNER,
        "create {} with {sddl}: Win32 error {error}",
        path.display()
    );
    // Windows refused the owner at creation: create it with the same DACL
    // and set the owner through a handle, with SeRestorePrivilege.
    let ownerless = Descriptor::from_sddl(&without_owner(sddl));
    let attributes = ownerless.attributes();
    // SAFETY: as above.
    let created = unsafe { CreateDirectoryW(name.as_ptr(), &attributes) };
    assert_ne!(
        created,
        0,
        "create {}: Win32 error {}",
        path.display(),
        last_error()
    );
    let handle = open_existing(path, WRITE_OWNER, 0)
        .unwrap_or_else(|e| panic!("open {} to set its owner: Win32 error {e}", path.display()));
    set_owner(&handle, descriptor.owner(), path);
}

/// Create the file `path`, which must not exist, with `sddl` as its
/// security (a DACL given at creation comes out protected), and write
/// `bytes` to it.
pub fn create_file_with(path: &Path, sddl: &str, bytes: &[u8]) {
    enable_backup_and_restore();
    let descriptor = Descriptor::from_sddl(sddl);
    let create = |descriptor: &Descriptor, access: u32| {
        let attributes = descriptor.attributes();
        let name = wide_path(path);
        // SAFETY: the name and attributes live for the call; the handle is
        // owned by the returned value.
        let raw = unsafe {
            CreateFileW(
                name.as_ptr(),
                access,
                0,
                &attributes,
                CREATE_NEW,
                FILE_ATTRIBUTE_NORMAL | FILE_FLAG_BACKUP_SEMANTICS,
                std::ptr::null_mut(),
            )
        };
        if raw == INVALID_HANDLE_VALUE {
            Err(last_error())
        } else {
            Ok(Handle(raw))
        }
    };
    let handle = match create(&descriptor, GENERIC_WRITE) {
        Ok(handle) => handle,
        Err(ERROR_INVALID_OWNER) => {
            // As for a directory: the owner is set before any byte is
            // written.
            let ownerless = Descriptor::from_sddl(&without_owner(sddl));
            let handle = create(&ownerless, GENERIC_WRITE | WRITE_OWNER).unwrap_or_else(|e| {
                panic!("create {}: Win32 error {e}", path.display());
            });
            set_owner(&handle, descriptor.owner(), path);
            handle
        }
        Err(e) => panic!("create {} with {sddl}: Win32 error {e}", path.display()),
    };
    let mut file = handle.into_file();
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}

/// Create the file `path`, which must not exist, with no security
/// descriptor: its DACL is only what its directory's inheritable entries
/// give it, not protected. Then make `owner` its owner, leaving the DACL as
/// it is.
pub fn create_file_inheriting(path: &Path, owner: WELL_KNOWN_SID_TYPE, bytes: &[u8]) {
    enable_backup_and_restore();
    let name = wide_path(path);
    // SAFETY: the name lives for the call; the handle is owned below.
    let raw = unsafe {
        CreateFileW(
            name.as_ptr(),
            GENERIC_WRITE | WRITE_OWNER,
            0,
            std::ptr::null(),
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    assert!(
        raw != INVALID_HANDLE_VALUE,
        "create {}: Win32 error {}",
        path.display(),
        last_error()
    );
    let handle = Handle(raw);
    let mut sid = well_known_sid(owner);
    set_owner(&handle, sid.as_mut_ptr().cast(), path);
    let mut file = handle.into_file();
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}

/// The bytes of `path`, read with backup semantics (a SYSTEM-only file).
pub fn privileged_read(path: &Path) -> Vec<u8> {
    try_privileged_read(path)
        .unwrap_or_else(|e| panic!("open {} to read it: Win32 error {e}", path.display()))
}

/// As `privileged_read`, for a file a running service may be replacing: the
/// error is the Win32 error of the open.
pub fn try_privileged_read(path: &Path) -> Result<Vec<u8>, u32> {
    enable_backup_and_restore();
    let handle = open_existing(path, GENERIC_READ, 0)?;
    let mut bytes = Vec::new();
    handle
        .into_file()
        .read_to_end(&mut bytes)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    Ok(bytes)
}

/// A handle of this process on the file `path`, opened for reading and
/// sharing every access, held until it is dropped: the open child a
/// directory cannot be renamed with. It takes no write or delete access, so
/// it does not make an open that shares read access alone fail.
pub fn hold_for_reading(path: &Path) -> Handle {
    enable_backup_and_restore();
    open_existing(path, GENERIC_READ, 0)
        .unwrap_or_else(|e| panic!("hold {} open: Win32 error {e}", path.display()))
}

/// A batch opportunistic lock on the file `path`, held until it is dropped.
/// The break Windows sends its holder is never acknowledged, so what an open
/// of the file makes wait on another process waits until the value is
/// dropped: an open of this process's own too, so a test reads nothing of the
/// file while it holds one.
pub struct Oplock {
    file: Handle,
    event: Handle,
    /// Written by the kernel while the request is pending: it outlives it.
    _overlapped: Box<Overlapped>,
}

impl Oplock {
    /// Another handle on the file rules an oplock out, so the request is
    /// repeated for a moment: a handle of a console agent just ended, or of a
    /// scanner, is closed soon.
    pub fn take(path: &Path) -> Self {
        enable_backup_and_restore();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            match Self::request(path) {
                Ok(oplock) => return oplock,
                Err(ERROR_OPLOCK_NOT_GRANTED) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(250));
                }
                Err(e) => panic!("an oplock on {}: Win32 error {e}", path.display()),
            }
        }
    }

    fn request(path: &Path) -> Result<Self, u32> {
        let file = open_existing(path, GENERIC_READ, FILE_FLAG_OVERLAPPED)?;
        // SAFETY: a manual-reset event, not signalled; closed on drop.
        let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        assert!(
            !event.is_null(),
            "CreateEventW: Win32 error {}",
            last_error()
        );
        let event = Handle(event);
        let mut overlapped = Box::new(Overlapped {
            internal: 0,
            internal_high: 0,
            offset: 0,
            offset_high: 0,
            event: event.0,
        });
        // SAFETY: an open overlapped handle and an event that outlive the
        // request, and a box that stays where it is while it is pending.
        let ok = unsafe {
            DeviceIoControl(
                file.0,
                FSCTL_REQUEST_BATCH_OPLOCK,
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut *overlapped,
            )
        };
        let error = last_error();
        if ok != 0 || error != ERROR_IO_PENDING {
            return Err(error);
        }
        Ok(Oplock {
            file,
            event,
            _overlapped: overlapped,
        })
    }

    /// Windows sent the break: an open waits on this holder.
    pub fn broken(&self) -> bool {
        // SAFETY: an event this value owns, not waited for.
        unsafe { WaitForSingleObject(self.event.0, 0) == WAIT_OBJECT_0 }
    }
}

impl Drop for Oplock {
    fn drop(&mut self) {
        // SAFETY: closing the file completes the pending request, which sets
        // the event; the box and the event are freed only after that.
        unsafe {
            CloseHandle(self.file.0);
            self.file.0 = std::ptr::null_mut();
            WaitForSingleObject(self.event.0, 5000);
        }
    }
}

/// Whether anything is at `path`, seen with backup semantics.
pub fn privileged_exists(path: &Path) -> bool {
    enable_backup_and_restore();
    match open_existing(path, FILE_READ_ATTRIBUTES, 0) {
        Ok(_) => true,
        Err(e) if is_absent(e) => false,
        Err(e) => panic!("inspect {}: Win32 error {e}", path.display()),
    }
}

/// Delete the file at `path`, if any, with restore semantics.
pub fn privileged_delete(path: &Path) {
    enable_backup_and_restore();
    match open_existing(path, DELETE, FILE_FLAG_DELETE_ON_CLOSE) {
        Ok(handle) => drop(handle),
        Err(e) if is_absent(e) => {}
        Err(e) => panic!("delete {}: Win32 error {e}", path.display()),
    }
}

/// The names in the directory `path`, listed with backup semantics;
/// `None` when it does not exist.
pub fn privileged_listing(path: &Path) -> Option<Vec<String>> {
    enable_backup_and_restore();
    let handle = match open_existing(path, FILE_LIST_DIRECTORY, 0) {
        Ok(handle) => handle,
        Err(e) if is_absent(e) => return None,
        Err(e) => panic!("open {} to list it: Win32 error {e}", path.display()),
    };
    // FILE_FULL_DIR_INFO is 8-byte aligned.
    let mut buffer = vec![0u64; 8192];
    let mut names = Vec::new();
    let mut class = FileFullDirectoryRestartInfo;
    loop {
        // SAFETY: the buffer's size in bytes is passed.
        let ok = unsafe {
            GetFileInformationByHandleEx(
                handle.0,
                class,
                buffer.as_mut_ptr().cast(),
                (buffer.len() * 8) as u32,
            )
        };
        if ok == 0 {
            let error = last_error();
            assert_eq!(
                error,
                ERROR_NO_MORE_FILES,
                "list {}: Win32 error {error}",
                path.display()
            );
            break;
        }
        class = FileFullDirectoryInfo;
        let base = buffer.as_ptr().cast::<u8>();
        let mut offset = 0usize;
        loop {
            // SAFETY: each entry lies within the buffer the call filled, at
            // the offsets it chained.
            let (next, name) = unsafe {
                let info = base.add(offset).cast::<FILE_FULL_DIR_INFO>();
                let len = (*info).FileNameLength as usize / 2;
                let text = std::ptr::addr_of!((*info).FileName).cast::<u16>();
                (
                    (*info).NextEntryOffset as usize,
                    String::from_utf16_lossy(std::slice::from_raw_parts(text, len)),
                )
            };
            if name != "." && name != ".." {
                names.push(name);
            }
            if next == 0 {
                break;
            }
            offset += next;
        }
    }
    names.sort();
    Some(names)
}

/// The owner and DACL of `path`, as SDDL, read through a backup-semantics
/// `READ_CONTROL` handle (the identity files are SYSTEM's alone).
pub fn owner_and_dacl(path: &Path) -> String {
    enable_backup_and_restore();
    let handle = open_existing(path, READ_CONTROL, 0)
        .unwrap_or_else(|e| panic!("open {} for its security: Win32 error {e}", path.display()));
    let info = OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: an open handle with READ_CONTROL; the descriptor is freed
    // below.
    let error = unsafe {
        GetSecurityInfo(
            handle.0,
            SE_FILE_OBJECT,
            info,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    assert_eq!(error, 0, "read the security of {}", path.display());
    let descriptor = Descriptor(descriptor);
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: the descriptor above; the string is freed below.
    let ok = unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor.0,
            SDDL_REVISION_1,
            info,
            &mut text,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(ok, 0, "to SDDL");
    // SAFETY: a NUL-terminated string the call allocated, freed once.
    unsafe {
        let sddl = from_wide_nul(text);
        LocalFree(text.cast());
        sddl
    }
}

/// One ACE of an SDDL DACL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ace {
    /// `A` (allow), `D` (deny), ...
    pub kind: String,
    /// Two-letter flags: `OI`, `CI`, `IO`, `ID`, ...
    pub flags: Vec<String>,
    pub rights: String,
    pub sid: String,
}

impl Ace {
    /// Full access, as SDDL writes it (`FA`, or its mask).
    pub fn is_full_access(&self) -> bool {
        self.rights.eq_ignore_ascii_case("FA") || self.rights.eq_ignore_ascii_case("0x1f01ff")
    }

    pub fn has_flag(&self, flag: &str) -> bool {
        self.flags.iter().any(|f| f == flag)
    }
}

/// An owner and DACL parsed from SDDL (`O:<sid>[G:<sid>]D:<flags>(ace)...`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Security {
    pub owner: String,
    /// The DACL's flags: `P` (protected), `AI`, `AR`.
    pub dacl_flags: String,
    pub aces: Vec<Ace>,
}

impl Security {
    pub fn protected(&self) -> bool {
        // `P` is the only flag with that letter (the others are AI, AR).
        self.dacl_flags.contains('P')
    }
}

pub fn parse_sddl(sddl: &str) -> Security {
    let dacl_at = sddl
        .find("D:")
        .unwrap_or_else(|| panic!("a DACL in {sddl}"));
    let head = &sddl[..dacl_at];
    let owner = head
        .strip_prefix("O:")
        .map(|rest| rest.split("G:").next().unwrap_or(rest).to_string())
        .unwrap_or_default();
    let dacl = &sddl[dacl_at + 2..];
    let flags_end = dacl.find('(').unwrap_or(dacl.len());
    let aces = dacl[flags_end..]
        .split(')')
        .filter_map(|chunk| chunk.strip_prefix('('))
        .map(|body| {
            let fields: Vec<&str> = body.split(';').collect();
            assert!(
                fields.len() >= 6,
                "an ACE of six fields: ({body}) in {sddl}"
            );
            let flags = fields[1]
                .as_bytes()
                .chunks(2)
                .map(|pair| String::from_utf8_lossy(pair).to_string())
                .collect();
            Ace {
                kind: fields[0].to_string(),
                flags,
                rights: fields[2].to_string(),
                sid: fields[5].to_string(),
            }
        })
        .collect();
    Security {
        owner,
        dacl_flags: dacl[..flags_end].to_string(),
        aces,
    }
}

// ---------------------------------------------------------------------------
// The fixed installed tree (SPEC-024 §Data contracts §2; Amendment
// 2026-11-04, item 1), planted with its production security.
// ---------------------------------------------------------------------------

/// `<ProgramData>\CyberGuard` and its data root, as the service's file log
/// creates them: SYSTEM and Administrators only.
pub const PRIVATE_DIRECTORY_SDDL: &str = "O:SYD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)";
/// `agent.toml` and `server-ca.pem` (§Data contracts §2: SYSTEM owner,
/// protected DACL, SYSTEM and Administrators only).
pub const CONFIG_SDDL: &str = "O:SYD:P(A;;FA;;;SY)(A;;FA;;;BA)";
/// `identity\` (Amendment 2026-11-04, item 4): owner SYSTEM, a protected
/// DACL whose only entry, inherited by its files, is SYSTEM's.
pub const IDENTITY_DIRECTORY_SDDL: &str = "O:SYD:P(A;OICI;FA;;;SY)";
/// Each identity file: owner SYSTEM, SYSTEM alone.
pub const IDENTITY_FILE_SDDL: &str = "O:SYD:P(A;;FA;;;SY)";

/// `%ProgramData%`, from the OS (`SHGetKnownFolderPath`), as the service
/// resolves it (Amendment 2026-11-04, item 1).
pub fn program_data() -> PathBuf {
    let mut raw: *mut u16 = std::ptr::null_mut();
    // SAFETY: a known-folder id and an out-pointer the call fills with a
    // CoTaskMemAlloc'd string, freed below.
    let hr =
        unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramData, 0, std::ptr::null_mut(), &mut raw) };
    assert!(
        hr >= 0 && !raw.is_null(),
        "resolve %ProgramData%: HRESULT {hr:#010x}"
    );
    // SAFETY: a NUL-terminated wide string from the call, freed once.
    let path = unsafe {
        use std::os::windows::ffi::OsStringExt;
        let len = (0..).take_while(|&i| *raw.add(i) != 0).count();
        let path = std::ffi::OsString::from_wide(std::slice::from_raw_parts(raw, len));
        CoTaskMemFree(raw.cast());
        path
    };
    PathBuf::from(path)
}

/// The fixed paths, spelled with backslashes as `<ProgramData>\CyberGuard\
/// agent\...` (Amendment 2026-11-04, items 1 and 2).
#[derive(Debug, Clone)]
pub struct FixedPaths {
    pub program_data: PathBuf,
    /// `<ProgramData>\CyberGuard`.
    pub product_root: PathBuf,
    /// `<ProgramData>\CyberGuard\agent`.
    pub data_root: PathBuf,
    pub config: PathBuf,
    pub trust_anchor: PathBuf,
    pub identity_dir: PathBuf,
    pub cert: PathBuf,
    pub key: PathBuf,
    pub identity: PathBuf,
    /// FR-014's temporary file in installed mode (item 6).
    pub hygiene_temporary: PathBuf,
    /// `<ProgramData>\CyberGuard\agent.migrating`, the migration's staging
    /// directory (Amendment 2026-11-05).
    pub staging: PathBuf,
    /// `<ProgramData>\CyberGuard\agent.legacy`, the console installation's
    /// directory after the commit.
    pub legacy: PathBuf,
    /// `migration.json` in the data root, after the commit.
    pub record: PathBuf,
}

impl FixedPaths {
    pub fn resolve() -> Self {
        let program_data = program_data();
        let product_root = program_data.join("CyberGuard");
        let data_root = product_root.join("agent");
        let identity_dir = data_root.join("identity");
        FixedPaths {
            config: data_root.join("agent.toml"),
            trust_anchor: data_root.join("server-ca.pem"),
            hygiene_temporary: data_root.join("agent.toml.hygiene-tmp"),
            staging: product_root.join("agent.migrating"),
            legacy: product_root.join("agent.legacy"),
            record: data_root.join("migration.json"),
            cert: identity_dir.join("cert.pem"),
            key: identity_dir.join("key.dat"),
            identity: identity_dir.join("identity.json"),
            program_data,
            product_root,
            data_root,
            identity_dir,
        }
    }
}

/// The path as the configuration spells it.
pub fn text(path: &Path) -> String {
    path.display().to_string()
}

/// The fixed tree `<ProgramData>\CyberGuard` this test plants. Any existing
/// `CyberGuard` is moved aside first and restored on every exit path; on
/// drop the planted tree is taken back (`takeown`, `icacls /reset`, both
/// from System32) and removed. Both tools recurse from `CyberGuard`, so they
/// reach `agent.migrating` and `agent.legacy` beside the data root as they
/// reach `identity\` inside it: `/D Y` has `takeown` take a directory this
/// account cannot list, as `agent.legacy` is, and once `icacls` has reset a
/// directory this account may delete what it holds, whatever the entries'
/// own DACLs. On this disposable runner a test may use the fixed paths
/// (SPEC-024 Amendment 2026-11-01, point 4).
pub struct FixedDataRoot {
    pub paths: FixedPaths,
    moved: Option<PathBuf>,
}

impl FixedDataRoot {
    /// `CyberGuard` and its data root with their production security.
    pub fn plant() -> Self {
        Self::plant_with(true)
    }

    /// `CyberGuard` and its data root inheriting `%ProgramData%`'s
    /// permissions, as directories an ordinary user created would.
    pub fn plant_inheriting() -> Self {
        Self::plant_with(false)
    }

    fn plant_with(protected: bool) -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        refuse_unless_disposable_runner(&Scm::open());
        enable_backup_and_restore();
        let paths = FixedPaths::resolve();
        let moved = std::fs::symlink_metadata(&paths.product_root)
            .is_ok()
            .then(|| {
                let aside = paths.program_data.join(format!(
                    "CyberGuard.scm-test-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::SeqCst)
                ));
                std::fs::rename(&paths.product_root, &aside)
                    .expect("move the existing CyberGuard aside");
                eprintln!(
                    "moved {} aside to {}",
                    paths.product_root.display(),
                    aside.display()
                );
                aside
            });
        // The guard exists before anything is planted: a failure below
        // still cleans up and restores.
        let root = FixedDataRoot { paths, moved };
        if protected {
            create_directory_with(&root.paths.product_root, PRIVATE_DIRECTORY_SDDL);
            create_directory_with(&root.paths.data_root, PRIVATE_DIRECTORY_SDDL);
        } else {
            std::fs::create_dir(&root.paths.product_root).expect("plant CyberGuard");
            std::fs::create_dir(&root.paths.data_root).expect("plant CyberGuard\\agent");
        }
        root
    }

    /// Replace whatever is at `path` with a new file of `bytes` and `sddl`.
    pub fn write_file(&self, path: &Path, sddl: &str, bytes: &[u8]) {
        privileged_delete(path);
        create_file_with(path, sddl, bytes);
    }

    /// Write `agent.toml` at its fixed path with its production security.
    pub fn write_config(&self, file: &ConfigFile) -> PathBuf {
        self.write_file(&self.paths.config, CONFIG_SDDL, file.toml().as_bytes());
        self.paths.config.clone()
    }

    /// Write `server-ca.pem` at its fixed path with its production
    /// security.
    pub fn write_trust_anchor(&self, pem: &str) {
        self.write_file(&self.paths.trust_anchor, CONFIG_SDDL, pem.as_bytes());
    }

    /// Plant `identity\` with `directory_sddl` and each of `files`' three
    /// files that is `Some`, with its SDDL.
    pub fn plant_identity(&self, directory_sddl: &str, files: &IdentityFiles, sddls: [&str; 3]) {
        create_directory_with(&self.paths.identity_dir, directory_sddl);
        for ((path, bytes), sddl) in [
            (&self.paths.cert, &files.cert),
            (&self.paths.key, &files.key),
            (&self.paths.identity, &files.json),
        ]
        .into_iter()
        .zip(sddls)
        {
            if let Some(bytes) = bytes {
                create_file_with(path, sddl, bytes);
            }
        }
    }
}

/// Run a System32 tool, logging any failure.
fn system_tool(tool: &str, args: &[&str]) {
    let system32 =
        PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()))
            .join("System32");
    match std::process::Command::new(system32.join(tool))
        .args(args)
        .output()
    {
        Ok(output) if output.status.success() => {}
        Ok(output) => eprintln!(
            "{tool} {args:?}: {}: {} {}",
            output.status,
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        ),
        Err(e) => eprintln!("{tool} {args:?}: {e}"),
    }
}

impl Drop for FixedDataRoot {
    fn drop(&mut self) {
        let root = self.paths.product_root.clone();
        if std::fs::symlink_metadata(&root).is_ok() {
            let target = text(&root);
            system_tool("takeown.exe", &["/F", &target, "/R", "/A", "/D", "Y"]);
            system_tool("icacls.exe", &[&target, "/reset", "/T", "/C", "/Q"]);
            if let Err(e) = std::fs::remove_dir_all(&root) {
                eprintln!("remove {}: {e}", root.display());
            }
        }
        if let Some(aside) = &self.moved {
            if std::fs::symlink_metadata(&root).is_ok() {
                eprintln!(
                    "cannot restore {}: the planted tree is still there",
                    aside.display()
                );
            } else if let Err(e) = std::fs::rename(aside, &root) {
                eprintln!("restore {}: {e}", root.display());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The agent's configuration and identity.
// ---------------------------------------------------------------------------

/// The token the tests' configurations carry.
pub const TOKEN: &str = "install-ac-007";

/// The knobs a test turns in the agent's `agent.toml`.
#[derive(Debug, Clone, Copy)]
pub struct Knobs {
    pub request_timeout_seconds: u64,
    pub enrollment_timeout_seconds: u64,
    pub enrollment_max_retries: u32,
    pub enrollment_backoff_initial_ms: u64,
}

impl Default for Knobs {
    fn default() -> Self {
        Knobs {
            request_timeout_seconds: 5,
            enrollment_timeout_seconds: 30,
            enrollment_max_retries: 3,
            enrollment_backoff_initial_ms: 10,
        }
    }
}

/// A secure-path `agent.toml`; `fixed` gives the fixed paths, which a test
/// may then change.
#[derive(Debug, Clone)]
pub struct ConfigFile {
    pub enroll_url: String,
    pub heartbeat_url: Option<String>,
    pub token: Option<String>,
    pub trust_anchor_path: String,
    pub cert_path: String,
    pub key_path: String,
    pub identity_path: String,
    pub knobs: Knobs,
}

impl ConfigFile {
    pub fn fixed(paths: &FixedPaths, enroll_url: &str, heartbeat_url: &str) -> Self {
        ConfigFile {
            enroll_url: enroll_url.to_string(),
            heartbeat_url: Some(heartbeat_url.to_string()),
            token: Some(TOKEN.to_string()),
            trust_anchor_path: text(&paths.trust_anchor),
            cert_path: text(&paths.cert),
            key_path: text(&paths.key),
            identity_path: text(&paths.identity),
            knobs: Knobs::default(),
        }
    }

    /// The configuration the console runbook writes
    /// (`docs/operations/install.md`): the trust anchor and the identity
    /// files directly in the data root, with forward slashes.
    pub fn console(paths: &FixedPaths, enroll_url: &str, heartbeat_url: &str) -> Self {
        let in_data_root = |name: &str| text(&paths.data_root.join(name)).replace('\\', "/");
        ConfigFile {
            trust_anchor_path: in_data_root("server-ca.pem"),
            cert_path: in_data_root("cert.pem"),
            key_path: in_data_root("key.dat"),
            identity_path: in_data_root("identity.json"),
            ..Self::fixed(paths, enroll_url, heartbeat_url)
        }
    }

    /// The TOML; paths go in literal strings, so backslashes stay as they
    /// are.
    pub fn toml(&self) -> String {
        let heartbeat = self
            .heartbeat_url
            .as_ref()
            .map(|url| format!("heartbeat_url = \"{url}\"\n"))
            .unwrap_or_default();
        let token = self
            .token
            .as_ref()
            .map(|token| format!("token = \"{token}\"\n"))
            .unwrap_or_default();
        format!(
            "[server]\n\
             url = \"{enroll_url}\"\n\
             {heartbeat}\
             trust_anchor_path = '{anchor}'\n\
             \n\
             [agent]\n\
             id = \"01934abc-def0-7000-89ab-000000000007\"\n\
             hostname = \"TEST-PC\"\n\
             \n\
             [heartbeat]\n\
             interval_seconds = 1\n\
             request_timeout_seconds = {request_timeout}\n\
             max_retries = 3\n\
             backoff_initial_ms = 50\n\
             \n\
             [enrollment]\n\
             {token}\
             cert_path = '{cert}'\n\
             key_path = '{key}'\n\
             identity_path = '{identity}'\n\
             timeout_seconds = {enroll_timeout}\n\
             max_retries = {enroll_retries}\n\
             backoff_initial_ms = {enroll_backoff}\n",
            enroll_url = self.enroll_url,
            anchor = self.trust_anchor_path,
            cert = self.cert_path,
            key = self.key_path,
            identity = self.identity_path,
            request_timeout = self.knobs.request_timeout_seconds,
            enroll_timeout = self.knobs.enrollment_timeout_seconds,
            enroll_retries = self.knobs.enrollment_max_retries,
            enroll_backoff = self.knobs.enrollment_backoff_initial_ms,
        )
    }
}

/// Write the trust anchor and a secure-path `agent.toml` at their fixed
/// paths, with their production security, and return the config's path.
pub fn write_config(
    root: &FixedDataRoot,
    enroll_url: &str,
    heartbeat_url: &str,
    anchor_pem: &str,
    knobs: &Knobs,
) -> PathBuf {
    root.write_trust_anchor(anchor_pem);
    let mut file = ConfigFile::fixed(&root.paths, enroll_url, heartbeat_url);
    file.knobs = *knobs;
    root.write_config(&file)
}

/// A PEM section TLS refuses (SPEC-024 Amendment 2026-11-07): its body is
/// not base64. Appended to a good certificate, it leaves the first block as
/// it was.
pub const MALFORMED_PEM_SECTION: &str =
    "-----BEGIN CERTIFICATE-----\n!!!!\n-----END CERTIFICATE-----\n";

/// The three identity files' bytes; `None` leaves a file out.
#[derive(Debug, Clone)]
pub struct IdentityFiles {
    pub cert: Option<Vec<u8>>,
    /// `key.dat`: the seed sealed with machine-scope DPAPI, which SYSTEM
    /// unseals.
    pub key: Option<Vec<u8>>,
    /// `identity.json` (SPEC-002 §Data contracts).
    pub json: Option<Vec<u8>>,
}

impl IdentityFiles {
    /// A set as enrollment would persist it: `cert_pem`, `seed` sealed, and
    /// `identity.json` with `agent_id` and `pubkey`'s fingerprint.
    pub fn new(cert_pem: &str, seed: &[u8; 32], pubkey: &[u8; 32], agent_id: &str) -> Self {
        let key = cg_agent::secure_storage::default_store()
            .seal(seed)
            .unwrap_or_else(|e| panic!("seal the key with machine-scope DPAPI: {e:?}"));
        let persisted = cg_agent::identity::PersistedIdentity {
            agent_id: agent_id.to_string(),
            agent_pubkey_fingerprint: cg_agent::crypto::pubkey_fingerprint(pubkey),
            issued_at: "2026-11-03T00:00:00Z".to_string(),
            expires_at: "2027-11-03T00:00:00Z".to_string(),
        };
        let json = serde_json::to_string_pretty(&persisted).expect("identity.json");
        IdentityFiles {
            cert: Some(cert_pem.as_bytes().to_vec()),
            key: Some(key),
            json: Some(json.into_bytes()),
        }
    }

    /// The test PKI's identity.
    pub fn of(pki: &common::TlsTestPki) -> Self {
        Self::new(
            &pki.client_cert_pem,
            &pki.agent_seed,
            &pki.agent_pubkey,
            &pki.agent_id,
        )
    }
}

/// Persist the test PKI's identity at the fixed paths with the security the
/// service gives its own (Amendment 2026-11-04, item 4), so the service
/// loads it instead of enrolling and the heartbeat mock verifies its
/// signatures under the PKI's key.
pub fn provision_identity(root: &FixedDataRoot, pki: &common::TlsTestPki) {
    root.plant_identity(
        IDENTITY_DIRECTORY_SDDL,
        &IdentityFiles::of(pki),
        [IDENTITY_FILE_SDDL; 3],
    );
}

/// The service's arguments: `--service` and the quoted absolute config.
pub fn service_args(config_path: &Path) -> String {
    format!("--service --config \"{}\"", config_path.display())
}

pub fn online_count(mock: &common::TlsMockServer) -> usize {
    mock.received()
        .iter()
        .filter(|envelope| common::envelope_status(envelope) == "online")
        .count()
}

/// A TCP listener that accepts connections and never answers them: an
/// enrollment request stays in flight.
pub struct SilentListener {
    pub url: String,
    pub accepted: Arc<AtomicUsize>,
}

impl SilentListener {
    pub fn start() -> Self {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let port = listener.local_addr().expect("local_addr").port();
        let accepted = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&accepted);
        std::thread::spawn(move || {
            let mut held = Vec::new();
            for stream in listener.incoming().flatten() {
                counter.fetch_add(1, Ordering::SeqCst);
                held.push(stream);
            }
        });
        SilentListener {
            url: format!("https://127.0.0.1:{port}"),
            accepted,
        }
    }
}

/// A free local port, for a mock started later on it.
pub fn reserve_port() -> u16 {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    listener.local_addr().expect("local_addr").port()
}

// ---------------------------------------------------------------------------
// The console agent (SPEC-024 Amendment 2026-11-05): the manual installation
// the migration moves a host from.
// ---------------------------------------------------------------------------

/// A real console agent, `cg-agent --config <config>`, elevated as this test
/// is on the runner (its built-in Administrator). Its standard output and
/// error go to a file, which nothing has to drain and `output` reads. `Drop`
/// ends it, which frees the instance lock with the process, and stops the
/// ETW session that leaves: a console agent stops only on Ctrl+C, which a
/// process that shares no console with it cannot send.
pub struct ConsoleAgent {
    child: std::process::Child,
    output: tempfile::NamedTempFile,
}

impl ConsoleAgent {
    pub fn start(config: &Path) -> Self {
        refuse_unless_disposable_runner(&Scm::open());
        let output = tempfile::NamedTempFile::new().expect("the console agent's output file");
        // Duplicates of one handle share its position, so the two streams
        // append instead of overwriting each other.
        let stream = || {
            output
                .as_file()
                .try_clone()
                .map(std::process::Stdio::from)
                .expect("duplicate the console agent's output file")
        };
        let child = std::process::Command::new(env!("CARGO_BIN_EXE_cg-agent"))
            .arg("--config")
            .arg(config)
            .stdin(std::process::Stdio::null())
            .stdout(stream())
            .stderr(stream())
            .spawn()
            .expect("launch the console agent");
        ConsoleAgent { child, output }
    }

    /// How it ended, once it has.
    pub fn exited(&mut self) -> Option<std::process::ExitStatus> {
        self.child.try_wait().expect("query the console agent")
    }

    /// What it has written so far.
    pub fn output(&self) -> String {
        String::from_utf8_lossy(&std::fs::read(self.output.path()).unwrap_or_default()).to_string()
    }
}

impl Drop for ConsoleAgent {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        system_tool("logman.exe", &["stop", cg_agent::etw::SESSION_NAME, "-ets"]);
    }
}

/// The SID of the account this test runs as, as text: the account a console
/// agent it launches runs as, which that agent's identity files allow besides
/// SYSTEM.
pub fn current_user_sid() -> String {
    let output = std::process::Command::new("whoami")
        .args(["/user", "/fo", "csv", "/nh"])
        .output()
        .expect("run whoami");
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .trim()
        .rsplit(',')
        .next()
        .map(|field| field.trim_matches('"').to_string())
        .filter(|sid| sid.starts_with("S-1-"))
        .unwrap_or_else(|| panic!("no SID in whoami's output: {stdout}"))
}

// ---------------------------------------------------------------------------
// Effective access (install_ac_004): a standard user created for the test
// and a filtered administrator's token.
// ---------------------------------------------------------------------------

/// A token handle, closed on drop.
pub struct Token(HANDLE);

impl Drop for Token {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: a token handle this value owns, closed once.
            unsafe { CloseHandle(self.0) };
        }
    }
}

/// A standard local user this test created (`USER_PRIV_USER`, a member of
/// Users only), with a token from `LogonUserW`. `Drop` closes the token and
/// deletes the account on every exit path (SPEC-024 Amendment 2026-11-01,
/// point 4). Created only on a GitHub-hosted runner.
pub struct TestUser {
    pub name: String,
    token: Token,
}

impl TestUser {
    pub fn create() -> Self {
        refuse_unless_disposable_runner(&Scm::open());
        let mut random = [0u8; 20];
        getrandom::getrandom(&mut random).expect("random bytes");
        let hex: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let name = format!("cgt{}", &hex[..8]);
        // Upper and lower case, digits and symbols, none of the name.
        let password = format!("Kp-{}-xQ7!", &hex[8..]);
        let (mut wname, mut wpassword) = (wide(&name), wide(&password));
        let info = USER_INFO_1 {
            usri1_name: wname.as_mut_ptr(),
            usri1_password: wpassword.as_mut_ptr(),
            usri1_password_age: 0,
            usri1_priv: USER_PRIV_USER,
            usri1_home_dir: std::ptr::null_mut(),
            usri1_comment: std::ptr::null_mut(),
            usri1_flags: UF_SCRIPT | UF_DONT_EXPIRE_PASSWD,
            usri1_script_path: std::ptr::null_mut(),
        };
        let mut parameter = 0u32;
        // SAFETY: level 1 with one USER_INFO_1 whose strings live for the
        // call.
        let status = unsafe {
            NetUserAdd(
                std::ptr::null(),
                1,
                (&info as *const USER_INFO_1).cast(),
                &mut parameter,
            )
        };
        assert_eq!(
            status, NERR_Success,
            "NetUserAdd {name}: status {status}, parameter {parameter}"
        );
        eprintln!("created the standard local user {name}");
        // The guard exists before the logon: a failure below deletes the
        // account.
        let mut user = TestUser {
            name,
            token: Token(std::ptr::null_mut()),
        };
        let domain = wide(".");
        // An interactive logon, the right a standard user has on a
        // workstation; a network logon when this host's policy withholds it.
        for logon in [LOGON32_LOGON_INTERACTIVE, LOGON32_LOGON_NETWORK] {
            let mut token: HANDLE = std::ptr::null_mut();
            // SAFETY: the strings live for the call; `token` is owned below.
            let ok = unsafe {
                LogonUserW(
                    wname.as_ptr(),
                    domain.as_ptr(),
                    wpassword.as_ptr(),
                    logon,
                    LOGON32_PROVIDER_DEFAULT,
                    &mut token,
                )
            };
            if ok != 0 {
                eprintln!("logged on {} (logon type {logon})", user.name);
                user.token = Token(token);
                break;
            }
            eprintln!(
                "LogonUserW {} (logon type {logon}): Win32 error {}",
                user.name,
                last_error()
            );
        }
        assert!(
            !user.token.0.is_null(),
            "a token for the standard user {}",
            user.name
        );
        user
    }

    pub fn token(&self) -> &Token {
        &self.token
    }

    /// The account's SID, as SDDL spells it.
    pub fn sid(&self) -> String {
        token_user_sid(&self.token)
    }
}

impl Drop for TestUser {
    fn drop(&mut self) {
        drop(std::mem::replace(
            &mut self.token,
            Token(std::ptr::null_mut()),
        ));
        let name = wide(&self.name);
        // SAFETY: the name lives for the call.
        let status = unsafe { NetUserDel(std::ptr::null(), name.as_ptr()) };
        if status != NERR_Success {
            eprintln!("NetUserDel {}: status {status}", self.name);
        }
    }
}

/// This process's token as UAC's filtered administrator token would be:
/// `CreateRestrictedToken` with `DISABLE_MAX_PRIVILEGE` (no privilege but
/// SeChangeNotifyPrivilege) and Administrators (S-1-5-32-544) for deny
/// only. The runner runs as the built-in Administrator, which has no linked
/// filtered token (UAC's admin approval mode does not apply to it), so the
/// test builds one (SPEC-024 Amendment 2026-11-04, Effect).
pub fn filtered_administrator_token() -> Token {
    let mut process: HANDLE = std::ptr::null_mut();
    // SAFETY: the current-process pseudo-handle; `process` is owned below.
    let opened = unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_DUPLICATE | TOKEN_QUERY | TOKEN_ASSIGN_PRIMARY | TOKEN_IMPERSONATE,
            &mut process,
        )
    };
    assert_ne!(opened, 0, "OpenProcessToken: Win32 error {}", last_error());
    let process = Token(process);
    let mut administrators = well_known_sid(WinBuiltinAdministratorsSid);
    let deny_only = SID_AND_ATTRIBUTES {
        Sid: administrators.as_mut_ptr().cast(),
        Attributes: 0,
    };
    let mut restricted: HANDLE = std::ptr::null_mut();
    // SAFETY: one SID to disable, living for the call; `restricted` is
    // owned below.
    let ok = unsafe {
        CreateRestrictedToken(
            process.0,
            DISABLE_MAX_PRIVILEGE,
            1,
            &deny_only,
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
            &mut restricted,
        )
    };
    // The new token's handle has the access the process token was opened
    // with: enough to query it and to impersonate it.
    assert_ne!(ok, 0, "CreateRestrictedToken: Win32 error {}", last_error());
    let restricted = Token(restricted);
    let attributes = group_attributes(&restricted, WinBuiltinAdministratorsSid)
        .expect("the filtered token lists Administrators");
    assert_ne!(
        attributes & SE_GROUP_USE_FOR_DENY_ONLY,
        0,
        "Administrators is for deny only in the filtered token"
    );
    restricted
}

/// What `GetTokenInformation` answers for `class`, in an 8-byte aligned
/// buffer.
fn token_information(token: &Token, class: TOKEN_INFORMATION_CLASS) -> Vec<u64> {
    let mut needed = 0u32;
    // SAFETY: a size query.
    unsafe { GetTokenInformation(token.0, class, std::ptr::null_mut(), 0, &mut needed) };
    let mut buffer = vec![0u64; (needed as usize).div_ceil(8)];
    // SAFETY: a buffer of at least `needed` bytes, 8-byte aligned.
    let ok = unsafe {
        GetTokenInformation(
            token.0,
            class,
            buffer.as_mut_ptr().cast(),
            (buffer.len() * 8) as u32,
            &mut needed,
        )
    };
    assert_ne!(ok, 0, "GetTokenInformation: Win32 error {}", last_error());
    buffer
}

/// The SID of `token`'s user, as SDDL spells it.
pub fn token_user_sid(token: &Token) -> String {
    let buffer = token_information(token, TokenUser);
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: the call wrote a TOKEN_USER, whose SID lies in the buffer; the
    // string is freed once.
    unsafe {
        let user = buffer.as_ptr().cast::<TOKEN_USER>();
        let ok = ConvertSidToStringSidW((*user).User.Sid, &mut text);
        assert_ne!(
            ok,
            0,
            "ConvertSidToStringSidW: Win32 error {}",
            last_error()
        );
        let sid = from_wide_nul(text);
        LocalFree(text.cast());
        sid
    }
}

/// Which of the privileges that would let a process past a file's security
/// the process `process_id` holds enabled (SPEC-024 Amendment 2026-11-05,
/// item 1: "no privilege is enabled"): a SYSTEM service's token holds the
/// three, disabled.
pub fn enabled_custody_privileges(process_id: u32) -> Vec<String> {
    // SAFETY: an out-value handle, owned below.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    assert!(
        !process.is_null(),
        "OpenProcess {process_id}: Win32 error {}",
        last_error()
    );
    let process = Handle(process);
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: the handle above; `token` is owned below.
    let opened = unsafe { OpenProcessToken(process.0, TOKEN_QUERY, &mut token) };
    assert_ne!(
        opened,
        0,
        "OpenProcessToken {process_id}: Win32 error {}",
        last_error()
    );
    let token = Token(token);
    let buffer = token_information(&token, TokenPrivileges);
    // SAFETY: the call wrote a TOKEN_PRIVILEGES and its PrivilegeCount
    // entries.
    let held: Vec<LUID_AND_ATTRIBUTES> = unsafe {
        let privileges = buffer.as_ptr().cast::<TOKEN_PRIVILEGES>();
        let first = std::ptr::addr_of!((*privileges).Privileges).cast::<LUID_AND_ATTRIBUTES>();
        (0..(*privileges).PrivilegeCount as usize)
            .map(|i| *first.add(i))
            .collect()
    };
    [SE_BACKUP_NAME, SE_RESTORE_NAME, SE_TAKE_OWNERSHIP_NAME]
        .into_iter()
        .filter(|name| {
            let luid = privilege_value(*name);
            held.iter().any(|privilege| {
                privilege.Luid.LowPart == luid.LowPart
                    && privilege.Luid.HighPart == luid.HighPart
                    && privilege.Attributes & SE_PRIVILEGE_ENABLED != 0
            })
        })
        // SAFETY: a constant NUL-terminated name.
        .map(|name| unsafe { from_wide_nul(name) })
        .collect()
}

/// The attributes of the well-known group `kind` in `token`'s groups.
fn group_attributes(token: &Token, kind: WELL_KNOWN_SID_TYPE) -> Option<u32> {
    let buffer = token_information(token, TokenGroups);
    // SAFETY: the call wrote a TOKEN_GROUPS and its GroupCount entries.
    unsafe {
        let groups = buffer.as_ptr().cast::<TOKEN_GROUPS>();
        let first = std::ptr::addr_of!((*groups).Groups).cast::<SID_AND_ATTRIBUTES>();
        (0..(*groups).GroupCount as usize)
            .map(|i| *first.add(i))
            .find(|group| IsWellKnownSid(group.Sid, kind) != 0)
            .map(|group| group.Attributes)
    }
}

/// An object whose effective access a token is probed for.
#[derive(Debug, Clone)]
pub struct ProbeTarget {
    pub path: PathBuf,
    pub directory: bool,
}

/// What a token is refused on each target: read (list), write (add a file
/// or a directory, or delete an entry of a directory), delete, change the
/// DACL, take ownership (install_ac_004).
fn attempts(directory: bool) -> &'static [(&'static str, u32)] {
    if directory {
        &[
            ("FILE_LIST_DIRECTORY", FILE_LIST_DIRECTORY),
            ("FILE_ADD_FILE", FILE_ADD_FILE),
            ("FILE_ADD_SUBDIRECTORY", FILE_ADD_SUBDIRECTORY),
            ("FILE_DELETE_CHILD", FILE_DELETE_CHILD),
            ("DELETE", DELETE),
            ("WRITE_DAC", WRITE_DAC),
            ("WRITE_OWNER", WRITE_OWNER),
        ]
    } else {
        &[
            ("GENERIC_READ", GENERIC_READ),
            ("GENERIC_WRITE", GENERIC_WRITE),
            ("DELETE", DELETE),
            ("WRITE_DAC", WRITE_DAC),
            ("WRITE_OWNER", WRITE_OWNER),
        ]
    }
}

/// `action`'s value, run on a thread of its own (never a runtime's) under
/// `token`'s impersonation.
fn as_token<T: Send + 'static>(token: &Token, action: impl FnOnce() -> T + Send + 'static) -> T {
    let raw = token.0 as usize;
    std::thread::spawn(move || {
        // SAFETY: a token handle the caller keeps open for the call.
        let impersonating = unsafe { ImpersonateLoggedOnUser(raw as HANDLE) };
        assert_ne!(
            impersonating,
            0,
            "ImpersonateLoggedOnUser: Win32 error {}",
            last_error()
        );
        let value = action();
        // SAFETY: ends this thread's impersonation.
        let reverted = unsafe { RevertToSelf() };
        assert_ne!(reverted, 0, "RevertToSelf: Win32 error {}", last_error());
        value
    })
    .join()
    .expect("the impersonating thread")
}

/// A handle on the directory `path`, opened under `token` for adding an
/// entry, which this process keeps for as long as a test needs it (SPEC-024
/// Amendment 2026-11-05, item 2.1: "through a handle opened before or after").
pub fn open_to_add_entries(token: &Token, path: &Path) -> Handle {
    let owned = path.to_path_buf();
    let raw = as_token(token, move || {
        open_existing(&owned, FILE_ADD_FILE, 0).map(|handle| {
            let raw = handle.0 as usize;
            std::mem::forget(handle);
            raw
        })
    });
    let raw =
        raw.unwrap_or_else(|e| panic!("open {} to add to it: Win32 error {e}", path.display()));
    Handle(raw as HANDLE)
}

/// What creating the file `name` through `directory`, a handle, answers
/// under `token`, as an `NTSTATUS`: Windows checks that create against the
/// directory's security as it stands, not the handle's granted access. A file
/// it creates is deleted as its handle closes, so a create leaves no entry.
pub fn create_through(token: &Token, directory: &Handle, name: &str) -> i32 {
    let root = directory.0 as usize;
    let name = name.to_string();
    as_token(token, move || {
        let text: Vec<u16> = name.encode_utf16().collect();
        let size = (text.len() * 2) as u16;
        let object_name = UNICODE_STRING {
            Length: size,
            MaximumLength: size,
            Buffer: text.as_ptr().cast_mut(),
        };
        let attributes = ObjectAttributes {
            length: std::mem::size_of::<ObjectAttributes>() as u32,
            root_directory: root as HANDLE,
            object_name: &object_name,
            attributes: OBJ_CASE_INSENSITIVE,
            security_descriptor: std::ptr::null(),
            security_quality_of_service: std::ptr::null(),
        };
        let mut status_block = IoStatusBlock {
            status: 0,
            information: 0,
        };
        let mut file: HANDLE = std::ptr::null_mut();
        // SAFETY: every structure lives for the call, and the buffer of
        // `object_name` holds `Length` bytes; `file` is closed below.
        let status = unsafe {
            NtCreateFile(
                &mut file,
                DELETE | SYNCHRONIZE,
                &attributes,
                &mut status_block,
                std::ptr::null(),
                FILE_ATTRIBUTE_NORMAL,
                0,
                FILE_CREATE,
                FILE_NON_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT | FILE_DELETE_ON_CLOSE,
                std::ptr::null(),
                0,
            )
        };
        if status >= 0 {
            // SAFETY: the file the call opened, closed once.
            unsafe { CloseHandle(file) };
        }
        status
    })
}

/// Impersonate `token` on a thread of its own (never a runtime's) and try
/// each of `attempts` on each target, opened without following a reparse
/// point and with backup semantics (which grant nothing to a token without
/// the privileges). Every attempt must fail with `ERROR_ACCESS_DENIED`; the
/// problems found are returned. Positive controls under the same
/// impersonation show the token itself works, so a denial is the objects'
/// doing: `kernel32.dll`, and each of `readable`, files the caller planted
/// in the same tree with an entry that lets this token read them, must be
/// opened for `GENERIC_READ`. A probe that denies everything (a broken
/// logon, an identification-level impersonation) cannot pass.
pub fn access_problems(
    token: &Token,
    who: &str,
    targets: &[ProbeTarget],
    readable: &[PathBuf],
) -> Vec<String> {
    let raw = token.0 as usize;
    let who = who.to_string();
    let targets = targets.to_vec();
    let readable = readable.to_vec();
    std::thread::spawn(move || {
        let token = raw as HANDLE;
        let mut problems = Vec::new();
        // SAFETY: a token handle the caller keeps open for the call.
        if unsafe { ImpersonateLoggedOnUser(token) } == 0 {
            problems.push(format!(
                "{who}: ImpersonateLoggedOnUser: Win32 error {}",
                last_error()
            ));
            return problems;
        }
        let control =
            PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()))
                .join("System32")
                .join("kernel32.dll");
        for control in std::iter::once(&control).chain(&readable) {
            if let Err(e) = open_existing(control, GENERIC_READ, 0) {
                problems.push(format!(
                    "{who}: the positive control, reading {}, failed: Win32 error {e}",
                    control.display()
                ));
            }
        }
        for target in &targets {
            for &(name, access) in attempts(target.directory) {
                match open_existing(&target.path, access, 0) {
                    Ok(_) => problems.push(format!(
                        "{who}: {name} on {} was granted",
                        target.path.display()
                    )),
                    Err(ERROR_ACCESS_DENIED) => {}
                    Err(e) => problems.push(format!(
                        "{who}: {name} on {}: Win32 error {e}, not access denied",
                        target.path.display()
                    )),
                }
            }
        }
        // SAFETY: ends this thread's impersonation.
        if unsafe { RevertToSelf() } == 0 {
            problems.push(format!("{who}: RevertToSelf: Win32 error {}", last_error()));
        }
        problems
    })
    .join()
    .expect("the access probe's thread")
}
