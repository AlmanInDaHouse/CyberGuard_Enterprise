//! The setup's client of the service control manager (SPEC-024 Amendment
//! 2026-11-10, items 3, 6 and 8): the product service's conflict check and
//! whether it runs, its stop, its failure actions and its start.
//!
//! Each call takes the service's name: the machine's is `PRODUCT_SERVICE`,
//! and the tests name services of their own. Each one opens the service with
//! the access it needs alone, so that a right the service's security denies
//! fails that call and no other.

// The probe: the client's bodies are stubs, so its helpers are unused.
#![allow(unused)]

use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    GetLastError, ERROR_INSUFFICIENT_BUFFER, ERROR_INVALID_SERVICE_CONTROL,
    ERROR_SERVICE_CANNOT_ACCEPT_CTRL, ERROR_SERVICE_DOES_NOT_EXIST, ERROR_SERVICE_NOT_ACTIVE,
    ERROR_SERVICE_REQUEST_TIMEOUT,
};
use windows_sys::Win32::System::Services::{
    ChangeServiceConfig2W, CloseServiceHandle, ControlService, OpenSCManagerW, OpenServiceW,
    QueryServiceConfigW, QueryServiceStatusEx, StartServiceW, QUERY_SERVICE_CONFIGW, SC_ACTION,
    SC_ACTION_RESTART, SC_HANDLE, SC_MANAGER_CONNECT, SC_STATUS_PROCESS_INFO,
    SERVICE_CHANGE_CONFIG, SERVICE_CONFIG_FAILURE_ACTIONS, SERVICE_CONFIG_FAILURE_ACTIONS_FLAG,
    SERVICE_CONTROL_STOP, SERVICE_FAILURE_ACTIONSW, SERVICE_FAILURE_ACTIONS_FLAG,
    SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS, SERVICE_START, SERVICE_STATUS,
    SERVICE_STATUS_PROCESS, SERVICE_STOP, SERVICE_STOPPED, SERVICE_STOP_PENDING,
};

use crate::codes::{Refusal, CONFLICTING_SERVICE, NOT_STOPPED, OWN_STATE, START_REFUSED};
use crate::service::{
    not_the_products, Configuration, FAILURE_RESET_SECONDS, RESTART_DELAYS_MS, STOP_WITHIN,
};

/// How often the stop polls the service's state.
const POLL: Duration = Duration::from_millis(250);

fn last_error() -> u32 {
    // SAFETY: reads the calling thread's last-error value.
    unsafe { GetLastError() }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A NUL-terminated wide string the SCM returned, or an empty one for null.
///
/// # Safety
/// `text` is null or points to a NUL-terminated UTF-16 string.
unsafe fn from_wide(text: *const u16) -> String {
    if text.is_null() {
        return String::new();
    }
    let len = (0..).take_while(|&i| *text.add(i) != 0).count();
    String::from_utf16_lossy(std::slice::from_raw_parts(text, len))
}

/// A handle to the service control manager, closed on drop.
pub struct Manager(SC_HANDLE);

impl Manager {
    /// The local SCM, for connecting alone. One that cannot be opened is 40
    /// (item 3).
    pub fn open() -> Result<Self, Refusal> {
        // SAFETY: null machine and database name the local active database.
        let handle =
            unsafe { OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT) };
        if handle.is_null() {
            return Err(Refusal::new(
                OWN_STATE,
                format!(
                    "the service control manager cannot be opened: Win32 error {}",
                    last_error()
                ),
            ));
        }
        Ok(Manager(handle))
    }

    /// The service named `name`, opened for `access`: `Ok(None)` when no
    /// such service exists, the Win32 error otherwise.
    fn service(&self, name: &str, access: u32) -> Result<Option<Service>, u32> {
        let wname = wide(name);
        // SAFETY: the name lives for the call.
        let handle = unsafe { OpenServiceW(self.0, wname.as_ptr(), access) };
        if handle.is_null() {
            return match last_error() {
                ERROR_SERVICE_DOES_NOT_EXIST => Ok(None),
                error => Err(error),
            };
        }
        Ok(Some(Service(handle)))
    }
}

impl Drop for Manager {
    fn drop(&mut self) {
        // SAFETY: a handle OpenSCManagerW returned, closed once.
        unsafe { CloseServiceHandle(self.0) };
    }
}

/// A handle to a service, closed on drop.
struct Service(SC_HANDLE);

impl Drop for Service {
    fn drop(&mut self) {
        // SAFETY: a handle OpenServiceW returned, closed once.
        unsafe { CloseServiceHandle(self.0) };
    }
}

impl Service {
    fn configuration(&self) -> Result<Configuration, u32> {
        let mut needed = 0u32;
        // SAFETY: a zero-sized query asks for the size alone.
        let ok = unsafe { QueryServiceConfigW(self.0, std::ptr::null_mut(), 0, &mut needed) };
        if ok == 0 {
            let error = last_error();
            if error != ERROR_INSUFFICIENT_BUFFER {
                return Err(error);
            }
        }
        // The buffer is aligned for the structure at its head.
        let words = (needed as usize)
            .div_ceil(std::mem::size_of::<u64>())
            .max(1);
        let mut buffer = vec![0u64; words];
        let size = (words * std::mem::size_of::<u64>()) as u32;
        // SAFETY: the buffer holds `size` bytes, aligned for the structure.
        let ok = unsafe {
            QueryServiceConfigW(
                self.0,
                buffer.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>(),
                size,
                &mut needed,
            )
        };
        if ok == 0 {
            return Err(last_error());
        }
        // SAFETY: the SCM wrote the structure at the buffer's head, and its
        // strings within the buffer, which lives for these reads.
        unsafe {
            let config = &*buffer.as_ptr().cast::<QUERY_SERVICE_CONFIGW>();
            Ok(Configuration {
                service_type: config.dwServiceType,
                account: from_wide(config.lpServiceStartName),
                command_line: from_wide(config.lpBinaryPathName),
            })
        }
    }

    fn status(&self) -> Result<SERVICE_STATUS_PROCESS, u32> {
        // SAFETY: an all-zero SERVICE_STATUS_PROCESS is a valid value.
        let mut status: SERVICE_STATUS_PROCESS = unsafe { std::mem::zeroed() };
        let mut needed = 0u32;
        // SAFETY: the buffer is one SERVICE_STATUS_PROCESS.
        let ok = unsafe {
            QueryServiceStatusEx(
                self.0,
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

    fn control_stop(&self) -> Result<(), u32> {
        // SAFETY: an all-zero SERVICE_STATUS is a valid out-buffer.
        let mut status: SERVICE_STATUS = unsafe { std::mem::zeroed() };
        // SAFETY: the out-buffer is one SERVICE_STATUS.
        if unsafe { ControlService(self.0, SERVICE_CONTROL_STOP, &mut status) } == 0 {
            return Err(last_error());
        }
        Ok(())
    }
}

/// What item 3 found of the product service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Product {
    /// No product service: no conflict, and nothing to stop.
    Absent,
    /// A product service whose configuration is the product's, and whether
    /// it was in a state other than `STOPPED`, `STOP_PENDING` included
    /// (item 5's `service_running`).
    Present { running: bool },
}

impl Product {
    /// Item 5's `service_running`.
    pub fn running(self) -> bool {
        matches!(self, Product::Present { running: true })
    }
}

/// Which route item 3's check runs on: on the migration route a product
/// service in any state but `STOPPED` is a conflict too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Migration,
    Other,
}

fn conflicting(name: &str, reason: impl std::fmt::Display) -> Refusal {
    Refusal::new(
        CONFLICTING_SERVICE,
        format!("a conflicting service: the service {name} {reason}"),
    )
}

/// Item 3: the service `name` against the product's configuration, with
/// `binary` and `config` the paths of its command line. A service whose
/// configuration cannot be read, or is not the product's, is 26; on the
/// migration route one in any state but `STOPPED` is 26 too. A status that
/// cannot be read is 40.
pub fn check_product_service(
    scm: &Manager,
    name: &str,
    binary: &std::path::Path,
    config: &std::path::Path,
    route: Route,
) -> Result<Product, Refusal> {
    unimplemented!("check_product_service")
}

fn not_stopped(name: &str, reason: impl std::fmt::Display) -> Refusal {
    Refusal::new(
        NOT_STOPPED,
        format!("the service {name} was not proved stopped: {reason}"),
    )
}

/// Item 6: send the service `name`, if it is not `STOPPED`, the stop control,
/// and prove it `STOPPED` within `STOP_WITHIN` of the first request, sending
/// the control again while the service cannot accept it. A service still in
/// another state at the end is 31; a service that does not exist needs no
/// stop.
pub fn stop(scm: &Manager, name: &str) -> Result<(), Refusal> {
    unimplemented!("stop")
}

/// Item 8: the failure actions of the service `name`: a restart after 60, 120
/// and 300 seconds, the count reset after 86400 seconds, and the
/// failure-actions flag set to false, so that a flag set earlier is cleared.
/// A failure is 40.
pub fn set_failure_actions(scm: &Manager, name: &str) -> Result<(), Refusal> {
    unimplemented!("set_failure_actions")
}

/// Item 8: start the service `name` with these start arguments, which it
/// reads after its name, or with none. A start the SCM refuses is 41.
pub fn start(scm: &Manager, name: &str, arguments: &[String]) -> Result<(), Refusal> {
    unimplemented!("start")
}
