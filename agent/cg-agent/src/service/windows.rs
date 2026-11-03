//! The SCM side of the service: the dispatcher, the status the service
//! reports, its control handler, the Application log and the run itself
//! (SPEC-024 §Operational §3 and its second Amendment 2026-11-01).
//!
//! The control handler only records the stop and reports `STOP_PENDING`;
//! it never blocks on HTTP or capture teardown. A ticker thread advances
//! the checkpoint of a pending state every 5 seconds with a 15-second wait
//! hint (item 3: at least every 10 seconds, at most 30). `STOPPED` is
//! reported last, after the Application-log entry: the process can end
//! as soon as it is.

use std::future::Future;
use std::path::PathBuf;
use std::pin::pin;
use std::process::ExitCode;
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::Duration;

use tokio::sync::watch;
use windows_sys::core::PWSTR;
use windows_sys::Win32::Foundation::{
    GetLastError, ERROR_CALL_NOT_IMPLEMENTED, ERROR_FAILED_SERVICE_CONTROLLER_CONNECT,
    ERROR_SERVICE_SPECIFIC_ERROR, HANDLE, NO_ERROR,
};
use windows_sys::Win32::System::EventLog::{
    DeregisterEventSource, RegisterEventSourceW, ReportEventW, EVENTLOG_ERROR_TYPE,
    EVENTLOG_INFORMATION_TYPE,
};
use windows_sys::Win32::System::Services::{
    RegisterServiceCtrlHandlerExW, SetServiceStatus, StartServiceCtrlDispatcherW,
    SERVICE_ACCEPT_SHUTDOWN, SERVICE_ACCEPT_STOP, SERVICE_CONTROL_INTERROGATE,
    SERVICE_CONTROL_SHUTDOWN, SERVICE_CONTROL_STOP, SERVICE_RUNNING, SERVICE_START_PENDING,
    SERVICE_STATUS, SERVICE_STATUS_HANDLE, SERVICE_STOPPED, SERVICE_STOP_PENDING,
    SERVICE_TABLE_ENTRYW, SERVICE_WIN32_OWN_PROCESS,
};

use super::{
    config_error_line, ensure_identity_until_stopped, log_file, running_entry, stderr_service_log,
    Entry, ServiceExit, ENROLLMENT_ROUND_WAIT, EVENT_SOURCE, EXIT_NOT_STARTED_BY_SCM,
    EXIT_SERVICE_LOG, STDERR_NOT_STARTED_BY_SCM, STDERR_RELATIVE_CONFIG, STOP_DEADLINE,
};
use crate::instance_lock::{self, LockOutcome};
use crate::{Capture, StopOutcome};

/// How often a pending state's checkpoint advances.
const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(5);
/// The wait hint of a pending state, in milliseconds.
const WAIT_HINT_MS: u32 = 15_000;
/// Past the stop deadline, how long the run may still take before the
/// service gives up on it and reports the deadline expired.
const BACKSTOP_GRACE: Duration = Duration::from_secs(5);

/// The `--config` path, for the service's main function.
static CONFIG: OnceLock<PathBuf> = OnceLock::new();
/// The status and the stop signal the control handler reaches.
static SHARED: OnceLock<Shared> = OnceLock::new();

struct Shared {
    status: Status,
    stop: watch::Sender<bool>,
}

/// Hand the process to the SCM's dispatcher; without a start by the SCM,
/// exit 11 (item 2).
pub(super) fn run(config_path: PathBuf) -> ExitCode {
    let _ = CONFIG.set(config_path);
    // An own-process service's name in the table is ignored but may not be
    // null; an empty string is allowed.
    let mut name = [0u16; 1];
    let table = [
        SERVICE_TABLE_ENTRYW {
            lpServiceName: name.as_mut_ptr(),
            lpServiceProc: Some(service_main),
        },
        SERVICE_TABLE_ENTRYW {
            lpServiceName: std::ptr::null_mut(),
            lpServiceProc: None,
        },
    ];
    // SAFETY: a null-terminated table that outlives the call, which
    // returns when the service has stopped.
    if unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) } == 0 {
        // SAFETY: reads the calling thread's last-error value.
        let error = unsafe { GetLastError() };
        if error == ERROR_FAILED_SERVICE_CONTROLLER_CONNECT {
            eprintln!("{STDERR_NOT_STARTED_BY_SCM}");
            return ExitCode::from(EXIT_NOT_STARTED_BY_SCM);
        }
        eprintln!("cg-agent: the service dispatcher failed: Win32 error {error}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

/// The service's main function, on a thread of the dispatcher's.
unsafe extern "system" fn service_main(argc: u32, argv: *mut PWSTR) {
    // argv[0] is the service's name, which an own-process service may
    // register its handler under.
    let name: Vec<u16> = if argc > 0 && !argv.is_null() && !(*argv).is_null() {
        let first = *argv;
        let len = (0..).take_while(|&i| *first.add(i) != 0).count();
        std::slice::from_raw_parts(first, len)
            .iter()
            .copied()
            .chain(std::iter::once(0))
            .collect()
    } else {
        vec![0]
    };
    let (stop_tx, stop_rx) = watch::channel(false);
    let handle =
        RegisterServiceCtrlHandlerExW(name.as_ptr(), Some(control_handler), std::ptr::null());
    if handle.is_null() {
        // Without a status handle nothing can be reported; the SCM sees
        // the service end with its process.
        return;
    }
    let shared = SHARED.get_or_init(|| Shared {
        status: Status::new(handle),
        stop: stop_tx,
    });
    shared.status.start_pending();
    let ticker = std::thread::Builder::new()
        .name("cg-service-status".to_string())
        .spawn(|| SHARED.get().expect("set").status.tick_while_pending());

    let events = EventLog::open();
    let config_path = CONFIG.get().cloned().unwrap_or_default();
    let exit = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => {
            let exit = runtime.block_on(serve(config_path, stop_rx, &shared.status, &events));
            // Blocking teardown work cut off by the deadline is not waited for.
            runtime.shutdown_background();
            exit
        }
        Err(e) => ServiceExit::Failed {
            code: 1,
            line: format!("cg-agent: the service runtime failed: {e}"),
        },
    };
    let entry = exit.entry();
    if entry.error {
        tracing::error!(exit_code = exit.specific_code(), "{}", entry.text);
    } else {
        tracing::info!("{}", entry.text);
    }
    events.write(&entry);
    drop(events);
    shared.status.stopped(exit.specific_code());
    if let Ok(ticker) = ticker {
        let _ = ticker.join();
    }
}

/// STOP and SHUTDOWN record the stop and report `STOP_PENDING`; the run
/// sees it through the watch channel. Nothing here blocks.
unsafe extern "system" fn control_handler(
    control: u32,
    _event_type: u32,
    _event_data: *mut core::ffi::c_void,
    _context: *mut core::ffi::c_void,
) -> u32 {
    match control {
        SERVICE_CONTROL_STOP | SERVICE_CONTROL_SHUTDOWN => {
            if let Some(shared) = SHARED.get() {
                shared.status.stop_pending();
                shared.stop.send_replace(true);
            }
            NO_ERROR
        }
        SERVICE_CONTROL_INTERROGATE => NO_ERROR,
        _ => ERROR_CALL_NOT_IMPLEMENTED,
    }
}

/// The service's run: the console's secure runtime under the states of
/// item 3.
async fn serve(
    config_path: PathBuf,
    stop: watch::Receiver<bool>,
    status: &Status,
    events: &EventLog,
) -> ServiceExit {
    if !config_path.is_absolute() {
        return ServiceExit::Failed {
            code: 2,
            line: STDERR_RELATIVE_CONFIG.to_string(),
        };
    }
    let mut config = match crate::config::load_from_path(&config_path) {
        Ok(config) => config,
        Err(e) => {
            return ServiceExit::Failed {
                code: 2,
                line: config_error_line(&config_path, &e),
            }
        }
    };

    // Item 1: the lock before the file log, so before RUNNING; only the
    // secure path, which captures, takes it.
    let secure = config.server.trust_anchor_path.is_some();
    let mut _instance_lock = None;
    let mut holds_lock = false;
    if secure {
        match instance_lock::acquire() {
            LockOutcome::Acquired(lock) => {
                _instance_lock = Some(lock);
                holds_lock = true;
            }
            LockOutcome::Unprivileged => {}
            outcome => {
                return ServiceExit::Failed {
                    code: outcome.exit_code().unwrap_or(1),
                    line: outcome.stderr_line().unwrap_or_default(),
                }
            }
        }
    }

    let file = match log_file::open() {
        Ok(file) => file,
        Err(reason) => {
            return ServiceExit::Failed {
                code: EXIT_SERVICE_LOG,
                line: stderr_service_log(&reason),
            }
        }
    };
    if let Err(e) = crate::init_logger_with_writer(&config.log.level, file) {
        return ServiceExit::Failed {
            code: EXIT_SERVICE_LOG,
            line: stderr_service_log(&e.to_string()),
        };
    }
    crate::log_agent_starting();
    tracing::info!(component = "service", "running as the Windows service");

    status.running();
    events.write(&running_entry());

    if !secure {
        // The plain development path, which captures nothing.
        let run = crate::run(config, stopped(stop.clone()));
        return match within_deadline(run, stop).await {
            Some(Ok(())) => ServiceExit::Clean { undelivered: 0 },
            Some(Err(e)) => failed(1, format!("cg-agent: {e}")),
            None => ServiceExit::DeadlineExpired {
                pending: "the agent's stop",
                undelivered: 0,
            },
        };
    }

    let identity = {
        let stop_signal = pin!(stopped(stop.clone()));
        match ensure_identity_until_stopped(
            &config,
            &config_path,
            ENROLLMENT_ROUND_WAIT,
            stop_signal,
        )
        .await
        {
            None => return ServiceExit::Clean { undelivered: 0 },
            Some(Err(e)) => return failed(e.exit_code(), e.stderr_line()),
            Some(Ok(identity)) => identity,
        }
    };
    config.agent.id = identity.agent_id.clone();
    let run = crate::run_secure_within(
        config,
        identity,
        Capture::Platform { holds_lock },
        stopped(stop.clone()),
        Some(STOP_DEADLINE),
    );
    match within_deadline(run, stop).await {
        Some(Ok(StopOutcome::Completed { undelivered })) => ServiceExit::Clean { undelivered },
        Some(Ok(StopOutcome::DeadlineExpired {
            pending,
            undelivered,
        })) => ServiceExit::DeadlineExpired {
            pending,
            undelivered,
        },
        Some(Err(e)) => failed(e.exit_code(), e.stderr_line()),
        None => ServiceExit::DeadlineExpired {
            pending: "the agent's stop",
            undelivered: 0,
        },
    }
}

fn failed(code: u8, line: String) -> ServiceExit {
    ServiceExit::Failed { code, line }
}

/// Resolves once STOP or SHUTDOWN has been received.
async fn stopped(mut stop: watch::Receiver<bool>) {
    let _ = stop.wait_for(|stopping| *stopping).await;
}

/// Run `run` to its end, unless it outlives the stop deadline (plus a
/// grace) once a stop is received: then `None`.
async fn within_deadline<F: Future>(run: F, stop: watch::Receiver<bool>) -> Option<F::Output> {
    let watchdog = async move {
        stopped(stop).await;
        tokio::time::sleep(STOP_DEADLINE + BACKSTOP_GRACE).await;
    };
    tokio::select! {
        output = run => Some(output),
        _ = watchdog => None,
    }
}

/// The status the service reports to the SCM.
struct Status {
    handle: StatusHandle,
    current: Mutex<SERVICE_STATUS>,
    changed: Condvar,
}

/// The SCM's status handle, which any thread of the service may use.
struct StatusHandle(SERVICE_STATUS_HANDLE);

// SAFETY: a service status handle is usable from any thread of the process.
unsafe impl Send for StatusHandle {}
// SAFETY: as above; SetServiceStatus calls are serialized by `current`.
unsafe impl Sync for StatusHandle {}

impl Status {
    fn new(handle: SERVICE_STATUS_HANDLE) -> Self {
        Status {
            handle: StatusHandle(handle),
            current: Mutex::new(SERVICE_STATUS {
                dwServiceType: SERVICE_WIN32_OWN_PROCESS,
                dwCurrentState: SERVICE_START_PENDING,
                dwControlsAccepted: 0,
                dwWin32ExitCode: 0,
                dwServiceSpecificExitCode: 0,
                dwCheckPoint: 0,
                dwWaitHint: 0,
            }),
            changed: Condvar::new(),
        }
    }

    fn report(&self, status: &SERVICE_STATUS) {
        // SAFETY: the handle RegisterServiceCtrlHandlerExW returned.
        unsafe { SetServiceStatus(self.handle.0, status) };
    }

    fn update(&self, change: impl FnOnce(&mut SERVICE_STATUS) -> bool) {
        let mut current = self.current.lock().unwrap_or_else(|e| e.into_inner());
        if change(&mut current) {
            self.report(&current);
            self.changed.notify_all();
        }
    }

    fn start_pending(&self) {
        self.update(|s| {
            s.dwCurrentState = SERVICE_START_PENDING;
            s.dwControlsAccepted = 0;
            s.dwCheckPoint = 1;
            s.dwWaitHint = WAIT_HINT_MS;
            true
        });
    }

    /// `RUNNING`, accepting STOP and SHUTDOWN alone; not after a stop.
    fn running(&self) {
        self.update(|s| {
            if s.dwCurrentState != SERVICE_START_PENDING {
                return false;
            }
            s.dwCurrentState = SERVICE_RUNNING;
            s.dwControlsAccepted = SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN;
            s.dwCheckPoint = 0;
            s.dwWaitHint = 0;
            true
        });
    }

    fn stop_pending(&self) {
        self.update(|s| {
            if s.dwCurrentState == SERVICE_STOP_PENDING || s.dwCurrentState == SERVICE_STOPPED {
                return false;
            }
            s.dwCurrentState = SERVICE_STOP_PENDING;
            s.dwControlsAccepted = 0;
            s.dwCheckPoint = 1;
            s.dwWaitHint = WAIT_HINT_MS;
            true
        });
    }

    /// `STOPPED`: code 0 after a clean stop, else
    /// `ERROR_SERVICE_SPECIFIC_ERROR` with the cause's code.
    fn stopped(&self, specific: Option<u8>) {
        self.update(|s| {
            s.dwCurrentState = SERVICE_STOPPED;
            s.dwControlsAccepted = 0;
            s.dwWin32ExitCode = specific.map_or(0, |_| ERROR_SERVICE_SPECIFIC_ERROR);
            s.dwServiceSpecificExitCode = specific.map_or(0, u32::from);
            s.dwCheckPoint = 0;
            s.dwWaitHint = 0;
            true
        });
    }

    /// Advance a pending state's checkpoint every `CHECKPOINT_INTERVAL`
    /// until the service has stopped.
    fn tick_while_pending(&self) {
        let mut current = self.current.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if current.dwCurrentState == SERVICE_STOPPED {
                return;
            }
            let (guard, timeout) = self
                .changed
                .wait_timeout(current, CHECKPOINT_INTERVAL)
                .unwrap_or_else(|e| e.into_inner());
            current = guard;
            let pending = current.dwCurrentState == SERVICE_START_PENDING
                || current.dwCurrentState == SERVICE_STOP_PENDING;
            if timeout.timed_out() && pending {
                current.dwCheckPoint += 1;
                self.report(&current);
            }
        }
    }
}

/// The Application log under the source `CyberGuardAgent` (item 4). If
/// the source cannot be opened the entries are skipped: the SCM status
/// still carries the outcome.
struct EventLog(HANDLE);

impl EventLog {
    fn open() -> Self {
        let source: Vec<u16> = EVENT_SOURCE
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: the local computer and a NUL-terminated source name.
        EventLog(unsafe { RegisterEventSourceW(std::ptr::null(), source.as_ptr()) })
    }

    fn write(&self, entry: &Entry) {
        if self.0.is_null() {
            return;
        }
        let text: Vec<u16> = entry
            .text
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let strings = [text.as_ptr()];
        let kind = if entry.error {
            EVENTLOG_ERROR_TYPE
        } else {
            EVENTLOG_INFORMATION_TYPE
        };
        // SAFETY: one NUL-terminated string that outlives the call; no SID,
        // no binary data.
        unsafe {
            ReportEventW(
                self.0,
                kind,
                0,
                entry.id,
                std::ptr::null_mut(),
                1,
                0,
                strings.as_ptr(),
                std::ptr::null(),
            )
        };
    }
}

impl Drop for EventLog {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the handle RegisterEventSourceW returned, closed once.
            unsafe { DeregisterEventSource(self.0) };
        }
    }
}
