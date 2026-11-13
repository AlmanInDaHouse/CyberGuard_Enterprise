//! The process that started the setup (SPEC-024 Amendment 2026-11-13, point
//! 3): on the wizard's route preflight holds, by a handle it opens before the
//! dialog, the process that created it and was created before it, which
//! under Burn is Burn's elevated process. A process ID Windows has given
//! again since is refused by the creation times. The dialog closes when that
//! process exits, and a submission after its exit is 22.

use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};

use windows_sys::Wdk::System::Threading::{NtQueryInformationProcess, ProcessBasicInformation};
use windows_sys::Win32::Foundation::{FILETIME, HANDLE, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetProcessTimes, OpenProcess, WaitForSingleObject,
    PROCESS_BASIC_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
};

use crate::codes::{Refusal, OWN_STATE};
use crate::wizard::Starter;

/// A process held by a handle that can wait on it.
#[derive(Debug)]
pub struct Process {
    handle: OwnedHandle,
}

/// `handle`'s creation time, in 100-nanosecond intervals since 1601.
fn created(handle: HANDLE) -> Result<u64, String> {
    let zero = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut creation, mut exit, mut kernel, mut user) = (zero, zero, zero, zero);
    // SAFETY: `handle` is open with PROCESS_QUERY_LIMITED_INFORMATION (or is
    // the current process's pseudo-handle), and each pointer is to a FILETIME.
    let ok = unsafe { GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) };
    if ok == 0 {
        return Err(format!(
            "its creation time cannot be read: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok((u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
}

/// This process's creation time.
pub fn own_creation() -> Result<u64, String> {
    // SAFETY: the current process's pseudo-handle needs no closing.
    created(unsafe { GetCurrentProcess() })
}

/// The ID of the process that created this one.
pub fn parent_id() -> Result<u32, String> {
    // SAFETY: an all-zero PROCESS_BASIC_INFORMATION is a valid value of a
    // plain C struct.
    let mut information: PROCESS_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
    let mut length = 0u32;
    // SAFETY: the pseudo-handle of the current process, and a buffer of the
    // size the class takes.
    let status = unsafe {
        NtQueryInformationProcess(
            GetCurrentProcess(),
            ProcessBasicInformation,
            std::ptr::addr_of_mut!(information).cast(),
            std::mem::size_of::<PROCESS_BASIC_INFORMATION>() as u32,
            &mut length,
        )
    };
    if status < 0 {
        return Err(format!(
            "its process ID cannot be read: NTSTATUS {status:#010x}"
        ));
    }
    u32::try_from(information.InheritedFromUniqueProcessId)
        .map_err(|_| "its process ID is out of range".to_string())
}

impl Process {
    /// The process `id`, held, if it was created before `before` (a
    /// creation time as `own_creation` gives it): a later one has the ID of
    /// one that exited.
    pub fn hold(id: u32, before: u64) -> Result<Self, String> {
        // SAFETY: OpenProcess takes any ID; a null result is its failure.
        let raw = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                id,
            )
        };
        if raw.is_null() {
            return Err(format!(
                "process {id} cannot be opened: {}",
                std::io::Error::last_os_error()
            ));
        }
        // SAFETY: `raw` is a handle this process owns and closes once.
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        let creation = created(handle.as_raw_handle())?;
        if creation >= before {
            return Err(format!(
                "process {id} was created after this one: its ID was given again"
            ));
        }
        Ok(Process { handle })
    }

    /// The process that created this one and was created before it, held:
    /// code 40 if it cannot be.
    pub fn starter() -> Result<Self, Refusal> {
        parent_id()
            .and_then(|id| Process::hold(id, own_creation()?))
            .map_err(|reason| {
                Refusal::new(
                    OWN_STATE,
                    format!("the process that started the setup cannot be held: {reason}"),
                )
            })
    }
}

impl Starter for Process {
    fn running(&self) -> bool {
        // SAFETY: the handle is open with SYNCHRONIZE; a zero timeout only
        // polls its state.
        unsafe { WaitForSingleObject(self.handle.as_raw_handle(), 0) == WAIT_TIMEOUT }
    }
}
