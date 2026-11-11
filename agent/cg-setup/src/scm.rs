//! The setup's client of the service control manager (SPEC-024 Amendment
//! 2026-11-10, items 3, 6 and 8): the product service's conflict check and
//! whether it runs, its stop, its failure actions and its start.
//!
//! Each call takes the service's name: the machine's is `PRODUCT_SERVICE`,
//! and the tests name services of their own. Each one opens the service with
//! the access it needs alone, so that a right the service's security denies
//! fails that call and no other.

use std::sync::mpsc;
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
        Self::connect().map_err(|error| {
            Refusal::new(
                OWN_STATE,
                format!("the service control manager cannot be opened: Win32 error {error}"),
            )
        })
    }

    fn connect() -> Result<Self, u32> {
        // SAFETY: null machine and database name the local active database.
        let handle =
            unsafe { OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT) };
        if handle.is_null() {
            return Err(last_error());
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
    let service = match scm.service(name, SERVICE_QUERY_CONFIG) {
        Ok(None) => return Ok(Product::Absent),
        Ok(Some(service)) => service,
        Err(error) => {
            return Err(conflicting(
                name,
                format!("cannot be read: Win32 error {error}"),
            ))
        }
    };
    let found = service.configuration().map_err(|error| {
        conflicting(
            name,
            format!("has a configuration that cannot be read: Win32 error {error}"),
        )
    })?;
    if let Some(reason) = not_the_products(&found, binary, config) {
        return Err(conflicting(name, format!("is not the product's: {reason}")));
    }
    // The state through a handle of its own, so that a denied status is
    // 40 and not a configuration that cannot be read.
    let unreadable = |error: u32| {
        Refusal::new(
            OWN_STATE,
            format!("the state of the service {name} cannot be read: Win32 error {error}"),
        )
    };
    // A service deleted since its configuration was read is no service.
    let state = match scm.service(name, SERVICE_QUERY_STATUS) {
        Ok(None) => return Ok(Product::Absent),
        Ok(Some(service)) => service.status().map_err(unreadable)?.dwCurrentState,
        Err(error) => return Err(unreadable(error)),
    };
    let running = state != SERVICE_STOPPED;
    if route == Route::Migration && running {
        return Err(conflicting(
            name,
            format!("is in state {state}, not STOPPED, and the migration route needs it stopped"),
        ));
    }
    Ok(Product::Present { running })
}

fn not_stopped(name: &str, reason: impl std::fmt::Display) -> Refusal {
    Refusal::new(
        NOT_STOPPED,
        format!("the service {name} was not proved stopped: {reason}"),
    )
}

/// One stop control sent to the service `name` on a thread of its own,
/// through a handle opened for `SERVICE_STOP` alone: `ControlService` can
/// block for up to 30 seconds while a service handles a control, and the
/// stop's bound must not wait on it.
fn send_stop(name: &str) -> mpsc::Receiver<Result<(), u32>> {
    let (sender, receiver) = mpsc::channel();
    let name = name.to_string();
    std::thread::spawn(move || {
        let result = Manager::connect().and_then(|scm| {
            scm.service(&name, SERVICE_STOP)?
                .ok_or(ERROR_SERVICE_DOES_NOT_EXIST)?
                .control_stop()
        });
        // The receiver is gone when the stop has already ended.
        let _ = sender.send(result);
    });
    receiver
}

/// Item 6: send the service `name`, if it is not `STOPPED`, the stop control,
/// and prove it `STOPPED` within `STOP_WITHIN` of the first request, sending
/// the control again while the service cannot accept it. A service still in
/// another state at the end is 31; a service that does not exist needs no
/// stop.
///
/// The state is read every `POLL` through a handle for the status alone, and
/// only a reading taken before the end proves `STOPPED`. Each control is sent
/// by `send_stop`, one at a time; a control still outstanding at the end is
/// left to finish on its own.
pub fn stop(scm: &Manager, name: &str) -> Result<(), Refusal> {
    let service = match scm.service(name, SERVICE_QUERY_STATUS) {
        Ok(None) => return Ok(()),
        Ok(Some(service)) => service,
        Err(error) => {
            return Err(not_stopped(
                name,
                format!("it cannot be opened to read its state: Win32 error {error}"),
            ))
        }
    };
    let state = |service: &Service| {
        service
            .status()
            .map(|status| status.dwCurrentState)
            .map_err(|error| {
                not_stopped(
                    name,
                    format!("its state cannot be read: Win32 error {error}"),
                )
            })
    };
    if state(&service)? == SERVICE_STOPPED {
        return Ok(());
    }
    let end = Instant::now() + STOP_WITHIN;
    let mut outstanding = Some(send_stop(name));
    let mut accepted = false;
    let mut last_refusal = None;
    loop {
        if let Some(receiver) = &outstanding {
            match receiver.try_recv() {
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => outstanding = None,
                Ok(result) => {
                    outstanding = None;
                    match result {
                        Ok(()) => accepted = true,
                        // It cannot accept the control in its state, did not
                        // answer the control in time, or has just stopped.
                        Err(
                            error @ (ERROR_SERVICE_CANNOT_ACCEPT_CTRL
                            | ERROR_INVALID_SERVICE_CONTROL
                            | ERROR_SERVICE_REQUEST_TIMEOUT
                            | ERROR_SERVICE_NOT_ACTIVE),
                        ) => {
                            accepted = false;
                            last_refusal = Some(error);
                        }
                        Err(error) => {
                            return Err(not_stopped(
                                name,
                                format!("the stop request failed: Win32 error {error}"),
                            ))
                        }
                    }
                }
            }
        }
        let read_at = Instant::now();
        let current = state(&service)?;
        if current == SERVICE_STOPPED && read_at < end {
            return Ok(());
        }
        if Instant::now() >= end {
            let refusal = last_refusal
                .map(|error| format!(", its last stop request refused with Win32 error {error}"))
                .unwrap_or_default();
            return Err(not_stopped(
                name,
                format!(
                    "it is in state {current} {} seconds after the first stop request{refusal}",
                    STOP_WITHIN.as_secs()
                ),
            ));
        }
        // A service that accepted the stop is stopping; one that has not
        // yet accepted it, or is back in a state other than STOP_PENDING,
        // is sent the control again.
        if outstanding.is_none() && (!accepted || current != SERVICE_STOP_PENDING) {
            outstanding = Some(send_stop(name));
        }
        std::thread::sleep(POLL.min(end.saturating_duration_since(Instant::now())));
    }
}

/// Item 8: the failure actions of the service `name`: a restart after 60, 120
/// and 300 seconds, the count reset after 86400 seconds, and the
/// failure-actions flag set to false, so that a flag set earlier is cleared;
/// a reboot message or command set earlier is deleted. A failure is 40.
pub fn set_failure_actions(scm: &Manager, name: &str) -> Result<(), Refusal> {
    let failed = |what: &str, error: u32| {
        Refusal::new(
            OWN_STATE,
            format!("the failure actions of the service {name} cannot be set: {what}: Win32 error {error}"),
        )
    };
    // A restart action needs the start right on the handle that sets it.
    let service = match scm.service(name, SERVICE_CHANGE_CONFIG | SERVICE_START) {
        Ok(Some(service)) => service,
        Ok(None) => return Err(failed("open", ERROR_SERVICE_DOES_NOT_EXIST)),
        Err(error) => return Err(failed("open", error)),
    };
    let mut actions = RESTART_DELAYS_MS.map(|delay| SC_ACTION {
        Type: SC_ACTION_RESTART,
        Delay: delay,
    });
    // An empty string deletes a reboot message or command set earlier; a
    // null pointer would leave it.
    let (mut reboot_message, mut command) = ([0u16], [0u16]);
    let failure_actions = SERVICE_FAILURE_ACTIONSW {
        dwResetPeriod: FAILURE_RESET_SECONDS,
        lpRebootMsg: reboot_message.as_mut_ptr(),
        lpCommand: command.as_mut_ptr(),
        cActions: actions.len() as u32,
        lpsaActions: actions.as_mut_ptr(),
    };
    // SAFETY: the structure, its strings and its actions live for the call.
    let ok = unsafe {
        ChangeServiceConfig2W(
            service.0,
            SERVICE_CONFIG_FAILURE_ACTIONS,
            (&failure_actions as *const SERVICE_FAILURE_ACTIONSW).cast(),
        )
    };
    if ok == 0 {
        return Err(failed("the actions", last_error()));
    }
    let flag = SERVICE_FAILURE_ACTIONS_FLAG {
        fFailureActionsOnNonCrashFailures: 0,
    };
    // SAFETY: the structure lives for the call.
    let ok = unsafe {
        ChangeServiceConfig2W(
            service.0,
            SERVICE_CONFIG_FAILURE_ACTIONS_FLAG,
            (&flag as *const SERVICE_FAILURE_ACTIONS_FLAG).cast(),
        )
    };
    if ok == 0 {
        return Err(failed("the flag", last_error()));
    }
    Ok(())
}

/// Item 8: start the service `name` with these start arguments, which it
/// reads after its name, or with none. A start the SCM refuses is 41.
pub fn start(scm: &Manager, name: &str, arguments: &[String]) -> Result<(), Refusal> {
    let refused = |error: u32| {
        Refusal::new(
            START_REFUSED,
            format!("the service control manager refused to start the service {name}: Win32 error {error}"),
        )
    };
    let service = match scm.service(name, SERVICE_START) {
        Ok(Some(service)) => service,
        Ok(None) => return Err(refused(ERROR_SERVICE_DOES_NOT_EXIST)),
        Err(error) => return Err(refused(error)),
    };
    let wide_arguments: Vec<Vec<u16>> = arguments.iter().map(|text| wide(text)).collect();
    let vector: Vec<*const u16> = wide_arguments.iter().map(|text| text.as_ptr()).collect();
    let pointer = if vector.is_empty() {
        std::ptr::null()
    } else {
        vector.as_ptr()
    };
    // SAFETY: `vector` holds `vector.len()` NUL-terminated strings, which
    // `wide_arguments` keeps alive for the call.
    if unsafe { StartServiceW(service.0, vector.len() as u32, pointer) } == 0 {
        return Err(refused(last_error()));
    }
    Ok(())
}
