//! The SCM side of the service: the dispatcher, the status the service
//! reports, its control handler, the Application log and the run itself
//! (SPEC-024 §Operational §3 and its second Amendment 2026-11-01).
//!
//! The control handler only records the stop: it sets a flag, wakes the
//! status thread and signals the run, and takes no lock, so it never
//! blocks. The status thread reports `STOP_PENDING` and advances the
//! checkpoint of a pending state every 5 seconds with a 15-second wait hint
//! (item 3: at least every 10 seconds, at most 30), up to a cap past which
//! a pending state that hangs shows as hung. The run is a task of its own;
//! a watchdog beside it ends the service 60 seconds after the stop
//! whatever the run is doing. `STOPPED` is reported last, after the
//! Application-log entry: the process can end as soon as it is.

use std::path::PathBuf;
use std::pin::pin;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

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
    check_service_config, config_error_line, ensure_identity_until_stopped, log_file,
    running_entry, stderr_service_log, Entry, ServiceExit, ENROLLMENT_ROUND_WAIT, EVENT_SOURCE,
    EXIT_NOT_STARTED_BY_SCM, EXIT_SERVICE_LOG, PENDING_UNKNOWN, STDERR_NOT_STARTED_BY_SCM,
    STDERR_RELATIVE_CONFIG, STOP_BUDGET, STOP_DEADLINE,
};
use crate::instance_lock::{self, LockOutcome};
use crate::{Capture, StopOutcome};

/// How often a pending state's checkpoint advances.
const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(5);
/// The wait hint of a pending state, in milliseconds.
const WAIT_HINT_MS: u32 = 15_000;
/// How long `START_PENDING` may advance its checkpoint: past it the SCM
/// sees no progress, and a start that hangs is not pending for ever.
const START_PENDING_CAP: Duration = Duration::from_secs(120);
/// The same for `STOP_PENDING`, past the stop deadline.
const STOP_PENDING_CAP: Duration = Duration::from_secs(75);

/// The `--config` path, for the service's main function.
static CONFIG: OnceLock<PathBuf> = OnceLock::new();
/// The status and the stop signal the control handler reaches.
static SHARED: OnceLock<Shared> = OnceLock::new();

struct Shared {
    status: Status,
    stop: watch::Sender<bool>,
}

impl Shared {
    /// Record a stop: the status thread reports `STOP_PENDING`, the run
    /// sees the watch channel. Takes no lock of the service's.
    fn request_stop(&self) {
        self.status.request_stop();
        self.stop.send_replace(true);
    }
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
    let events = Arc::new(EventLog::open());
    let (stop_tx, stop_rx) = watch::channel(false);
    let handle =
        RegisterServiceCtrlHandlerExW(name.as_ptr(), Some(control_handler), std::ptr::null());
    if handle.is_null() {
        // Without a status handle nothing can be reported to the SCM: say
        // why in the Application log, and end the process.
        let error = GetLastError();
        events.write(
            &ServiceExit::Failed {
                code: 1,
                line: format!(
                    "cg-agent: the service could not register its control handler: Win32 error {error}"
                ),
            }
            .entry(),
        );
        std::process::exit(1);
    }
    let shared = SHARED.get_or_init(|| Shared {
        status: Status::new(handle),
        stop: stop_tx,
    });
    shared.status.start_pending();
    let ticker = std::thread::Builder::new()
        .name("cg-service-status".to_string())
        .spawn(|| SHARED.get().expect("set").status.tick_while_pending());

    let config_path = CONFIG.get().cloned().unwrap_or_default();
    let exit = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => {
            let exit = runtime.block_on(supervise(config_path, stop_rx, Arc::clone(&events)));
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

/// STOP and SHUTDOWN record the stop; the status thread reports
/// `STOP_PENDING` and the run sees the watch channel. Nothing here blocks.
unsafe extern "system" fn control_handler(
    control: u32,
    _event_type: u32,
    _event_data: *mut core::ffi::c_void,
    _context: *mut core::ffi::c_void,
) -> u32 {
    match control {
        SERVICE_CONTROL_STOP | SERVICE_CONTROL_SHUTDOWN => {
            if let Some(shared) = SHARED.get() {
                shared.request_stop();
            }
            NO_ERROR
        }
        SERVICE_CONTROL_INTERROGATE => NO_ERROR,
        _ => ERROR_CALL_NOT_IMPLEMENTED,
    }
}

/// Run the service as a task of its own, beside a watchdog that ends it 60
/// seconds after a stop, whatever the run is doing; a run that panics is a
/// failure with code 1.
async fn supervise(
    config_path: PathBuf,
    stop: watch::Receiver<bool>,
    events: Arc<EventLog>,
) -> ServiceExit {
    let mut run = tokio::spawn(serve(config_path, stop.clone(), events));
    let watchdog = async move {
        stopped(stop).await;
        tokio::time::sleep(STOP_DEADLINE).await;
    };
    tokio::select! {
        biased;
        joined = &mut run => joined.unwrap_or_else(|e| ServiceExit::Failed {
            code: 1,
            line: format!(
                "cg-agent: the service run ended abnormally ({})",
                if e.is_panic() { "panic" } else { "cancelled" }
            ),
        }),
        _ = watchdog => {
            run.abort();
            ServiceExit::DeadlineExpired {
                pending: PENDING_UNKNOWN,
                undelivered: None,
            }
        }
    }
}

/// The console's secure runtime under the states of item 3.
async fn serve(
    config_path: PathBuf,
    stop: watch::Receiver<bool>,
    events: Arc<EventLog>,
) -> ServiceExit {
    let status = &SHARED.get().expect("set before the run").status;
    if !config_path.is_absolute() {
        return failed(2, STDERR_RELATIVE_CONFIG.to_string());
    }
    let mut config = match crate::config::load_from_path(&config_path) {
        Ok(config) => config,
        Err(e) => return failed(2, config_error_line(&config_path, &e)),
    };
    // The installed service runs the secure path alone (SPEC-024 §Data
    // contracts §2): no plain HTTP.
    if let Err(line) = check_service_config(&config_path, &config) {
        return failed(2, line);
    }

    // Item 1: the lock before the file log, so before RUNNING.
    let mut _instance_lock = None;
    let mut holds_lock = false;
    match instance_lock::acquire() {
        LockOutcome::Acquired(lock) => {
            _instance_lock = Some(lock);
            holds_lock = true;
        }
        LockOutcome::Unprivileged => {}
        outcome => {
            return failed(
                outcome.exit_code().unwrap_or(1),
                outcome.stderr_line().unwrap_or_default(),
            )
        }
    }

    let file = match log_file::open() {
        Ok(file) => file,
        Err(reason) => return failed(EXIT_SERVICE_LOG, stderr_service_log(&reason)),
    };
    if let Err(e) = crate::init_logger_with_writer(&config.log.level, file) {
        return failed(EXIT_SERVICE_LOG, stderr_service_log(&e.to_string()));
    }
    crate::log_agent_starting();
    tracing::info!(component = "service", "running as the Windows service");

    status.running();
    events.write(&running_entry());

    // SPEC-002 Amendment 2026-10-27: only an absent identity set starts
    // enrollment; an incomplete one fails closed.
    if let Err(e) = crate::identity::check_identity_set(&config) {
        return failed(e.exit_code(), e.stderr_line());
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
    if *stop.borrow() {
        // The stop arrived as the identity step ended: no capture starts.
        return ServiceExit::Clean { undelivered: 0 };
    }
    config.agent.id = identity.agent_id.clone();
    let outcome = crate::run_secure_within(
        config,
        identity,
        Capture::Platform { holds_lock },
        stopped(stop),
        Some(STOP_BUDGET),
    )
    .await;
    match outcome {
        Ok(StopOutcome::Completed { undelivered }) => ServiceExit::Clean { undelivered },
        Ok(StopOutcome::DeadlineExpired {
            pending,
            undelivered,
        }) => ServiceExit::DeadlineExpired {
            pending,
            undelivered: Some(undelivered),
        },
        Err(e) => failed(e.exit_code(), e.stderr_line()),
    }
}

fn failed(code: u8, line: String) -> ServiceExit {
    ServiceExit::Failed { code, line }
}

/// Resolves once STOP or SHUTDOWN has been received.
async fn stopped(mut stop: watch::Receiver<bool>) {
    let _ = stop.wait_for(|stopping| *stopping).await;
}

/// The status the service reports to the SCM.
struct Status {
    handle: StatusHandle,
    current: Mutex<Current>,
    changed: Condvar,
    stop_requested: AtomicBool,
}

/// The reported status and when its state began.
struct Current {
    status: SERVICE_STATUS,
    since: Instant,
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
            current: Mutex::new(Current {
                status: SERVICE_STATUS {
                    dwServiceType: SERVICE_WIN32_OWN_PROCESS,
                    dwCurrentState: SERVICE_START_PENDING,
                    dwControlsAccepted: 0,
                    dwWin32ExitCode: 0,
                    dwServiceSpecificExitCode: 0,
                    dwCheckPoint: 0,
                    dwWaitHint: 0,
                },
                since: Instant::now(),
            }),
            changed: Condvar::new(),
            stop_requested: AtomicBool::new(false),
        }
    }

    fn report(&self, status: &SERVICE_STATUS) {
        // SAFETY: the handle RegisterServiceCtrlHandlerExW returned.
        unsafe { SetServiceStatus(self.handle.0, status) };
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Current> {
        self.current.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Move to `state`, accepting `accepted`, and report it.
    fn enter(&self, current: &mut Current, state: u32, accepted: u32) {
        let pending = state == SERVICE_START_PENDING || state == SERVICE_STOP_PENDING;
        current.status.dwCurrentState = state;
        current.status.dwControlsAccepted = accepted;
        current.status.dwCheckPoint = u32::from(pending);
        current.status.dwWaitHint = if pending { WAIT_HINT_MS } else { 0 };
        current.since = Instant::now();
        self.report(&current.status);
        self.changed.notify_all();
    }

    fn start_pending(&self) {
        let mut current = self.lock();
        self.enter(&mut current, SERVICE_START_PENDING, 0);
    }

    /// `RUNNING`, accepting STOP and SHUTDOWN alone; never after a stop.
    fn running(&self) {
        let mut current = self.lock();
        if current.status.dwCurrentState == SERVICE_START_PENDING
            && !self.stop_requested.load(Ordering::SeqCst)
        {
            self.enter(
                &mut current,
                SERVICE_RUNNING,
                SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN,
            );
        }
    }

    /// Called by the control handler: set the flag and wake the status
    /// thread, without taking the lock.
    fn request_stop(&self) {
        self.stop_requested.store(true, Ordering::SeqCst);
        self.changed.notify_all();
    }

    /// `STOPPED`: code 0 after a clean stop, else
    /// `ERROR_SERVICE_SPECIFIC_ERROR` with the cause's code.
    fn stopped(&self, specific: Option<u8>) {
        let mut current = self.lock();
        current.status.dwWin32ExitCode = specific.map_or(0, |_| ERROR_SERVICE_SPECIFIC_ERROR);
        current.status.dwServiceSpecificExitCode = specific.map_or(0, u32::from);
        self.enter(&mut current, SERVICE_STOPPED, 0);
    }

    /// The status thread: report `STOP_PENDING` once a stop is requested,
    /// and advance a pending state's checkpoint every `CHECKPOINT_INTERVAL`
    /// until its cap, until the service has stopped.
    fn tick_while_pending(&self) {
        let mut current = self.lock();
        loop {
            let state = current.status.dwCurrentState;
            if state == SERVICE_STOPPED {
                return;
            }
            if self.stop_requested.load(Ordering::SeqCst)
                && (state == SERVICE_START_PENDING || state == SERVICE_RUNNING)
            {
                self.enter(&mut current, SERVICE_STOP_PENDING, 0);
                continue;
            }
            let (guard, timeout) = self
                .changed
                .wait_timeout(current, CHECKPOINT_INTERVAL)
                .unwrap_or_else(|e| e.into_inner());
            current = guard;
            let cap = match current.status.dwCurrentState {
                SERVICE_START_PENDING => START_PENDING_CAP,
                SERVICE_STOP_PENDING => STOP_PENDING_CAP,
                _ => continue,
            };
            if timeout.timed_out() && current.since.elapsed() < cap {
                current.status.dwCheckPoint += 1;
                self.report(&current.status);
            }
        }
    }
}

/// The Application log under the source `CyberGuardAgent` (item 4). If
/// the source cannot be opened the entries are skipped: the SCM status
/// still carries the outcome.
struct EventLog(HANDLE);

// SAFETY: an event source handle may be used from any thread; ReportEventW
// is thread-safe.
unsafe impl Send for EventLog {}
// SAFETY: as above.
unsafe impl Sync for EventLog {}

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
