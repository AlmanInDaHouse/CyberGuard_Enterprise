//! The service these tests control: this test executable itself, which the
//! SCM starts with the product's command line form, `"<executable>"
//! --service --config "<file>"`, so that the conflict check reads it as the
//! product's. It is no other target: the test-only feature is required by
//! this test target alone (SPEC-024 Amendment 2026-11-01, point 4).
//!
//! The file is its behaviour, a JSON object whose keys are all optional:
//! - `start_pending_ms`: how long it reports `START_PENDING`, accepting no
//!   control, before `RUNNING`;
//! - `refuse_stop_ms`: how long, once `RUNNING`, it accepts no control;
//! - `block_stop_ms`: how long its handler takes to answer a stop control;
//! - `stop_ms`: how long it reports `STOP_PENDING` after a stop before
//!   `STOPPED`;
//! - `never_stop`: after a stop it stays `STOP_PENDING`;
//! - `record`: a file to which each start appends its start arguments after
//!   the service's name, as one JSON array per line, before `RUNNING`.
//!
//! In each pending state its checkpoint advances every second.

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Deserialize;
use windows_sys::core::PWSTR;
use windows_sys::Win32::Foundation::{ERROR_CALL_NOT_IMPLEMENTED, NO_ERROR};
use windows_sys::Win32::System::Services::{
    RegisterServiceCtrlHandlerExW, SetServiceStatus, StartServiceCtrlDispatcherW,
    SERVICE_ACCEPT_STOP, SERVICE_CONTROL_INTERROGATE, SERVICE_CONTROL_STOP, SERVICE_RUNNING,
    SERVICE_START_PENDING, SERVICE_STATUS, SERVICE_STATUS_HANDLE, SERVICE_STOPPED,
    SERVICE_STOP_PENDING, SERVICE_TABLE_ENTRYW, SERVICE_WIN32_OWN_PROCESS,
};

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Behaviour {
    start_pending_ms: u64,
    refuse_stop_ms: u64,
    block_stop_ms: u64,
    stop_ms: u64,
    never_stop: bool,
    record: Option<PathBuf>,
}

static BEHAVIOUR: OnceLock<Behaviour> = OnceLock::new();
static STATUS_HANDLE: AtomicUsize = AtomicUsize::new(0);
static CHECKPOINT: AtomicU32 = AtomicU32::new(0);
static STOP_REQUESTED: Mutex<bool> = Mutex::new(false);
static STOP_SIGNAL: Condvar = Condvar::new();

/// Run as the service: `arguments` are the command line's after the
/// executable, `--service --config <file>`.
pub fn main(arguments: &[String]) {
    let [service, config, path] = arguments else {
        eprintln!("setup_service: usage: --service --config <behaviour file>");
        std::process::exit(2);
    };
    if service != "--service" || config != "--config" {
        eprintln!("setup_service: usage: --service --config <behaviour file>");
        std::process::exit(2);
    }
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read the behaviour file {path}: {e}"));
    let behaviour: Behaviour = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("parse the behaviour file {path}: {e}"));
    BEHAVIOUR.set(behaviour).expect("set once");
    // An own-process service's name in the table is ignored, but not null.
    let mut name = [0u16];
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
    // SAFETY: the table ends with a null entry and lives for the call, which
    // returns once the service has stopped.
    if unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) } == 0 {
        eprintln!("setup_service: not started by the service control manager");
        std::process::exit(1);
    }
}

/// # Safety
/// `text` points to a NUL-terminated UTF-16 string.
unsafe fn from_wide(text: *const u16) -> String {
    let len = (0..).take_while(|&i| *text.add(i) != 0).count();
    String::from_utf16_lossy(std::slice::from_raw_parts(text, len))
}

fn report(state: u32, accepted: u32) {
    let pending = state == SERVICE_START_PENDING || state == SERVICE_STOP_PENDING;
    let status = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: state,
        dwControlsAccepted: accepted,
        dwWin32ExitCode: NO_ERROR,
        dwServiceSpecificExitCode: 0,
        dwCheckPoint: if pending {
            CHECKPOINT.fetch_add(1, Ordering::SeqCst) + 1
        } else {
            0
        },
        dwWaitHint: if pending { 5_000 } else { 0 },
    };
    let handle = STATUS_HANDLE.load(Ordering::SeqCst) as SERVICE_STATUS_HANDLE;
    // SAFETY: the handle RegisterServiceCtrlHandlerExW returned; the status
    // lives for the call.
    unsafe { SetServiceStatus(handle, &status) };
}

/// Report `state` for `duration`, advancing its checkpoint every second.
fn pending(state: u32, duration: Duration) {
    let end = Instant::now() + duration;
    loop {
        report(state, 0);
        let now = Instant::now();
        if now >= end {
            return;
        }
        std::thread::sleep((end - now).min(Duration::from_secs(1)));
    }
}

unsafe extern "system" fn handler(
    control: u32,
    _event_type: u32,
    _event_data: *mut core::ffi::c_void,
    _context: *mut core::ffi::c_void,
) -> u32 {
    match control {
        SERVICE_CONTROL_STOP => {
            let block = BEHAVIOUR.get().map_or(0, |b| b.block_stop_ms);
            std::thread::sleep(Duration::from_millis(block));
            *STOP_REQUESTED.lock().unwrap() = true;
            STOP_SIGNAL.notify_all();
            NO_ERROR
        }
        SERVICE_CONTROL_INTERROGATE => NO_ERROR,
        _ => ERROR_CALL_NOT_IMPLEMENTED,
    }
}

unsafe extern "system" fn service_main(argc: u32, argv: *mut PWSTR) {
    let arguments: Vec<String> = (0..argc as usize)
        .map(|i| from_wide(*argv.add(i)))
        .collect();
    let handle = RegisterServiceCtrlHandlerExW(*argv, Some(handler), std::ptr::null());
    STATUS_HANDLE.store(handle as usize, Ordering::SeqCst);
    let behaviour = BEHAVIOUR.get().expect("read before the dispatcher");
    report(SERVICE_START_PENDING, 0);
    if let Some(record) = &behaviour.record {
        let line = serde_json::to_string(&arguments[1..]).expect("strings serialize");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(record)
            .expect("open the record");
        writeln!(file, "{line}").expect("write the record");
    }
    pending(
        SERVICE_START_PENDING,
        Duration::from_millis(behaviour.start_pending_ms),
    );
    if behaviour.refuse_stop_ms > 0 {
        report(SERVICE_RUNNING, 0);
        std::thread::sleep(Duration::from_millis(behaviour.refuse_stop_ms));
    }
    report(SERVICE_RUNNING, SERVICE_ACCEPT_STOP);
    let mut requested = STOP_REQUESTED.lock().unwrap();
    while !*requested {
        requested = STOP_SIGNAL.wait(requested).unwrap();
    }
    drop(requested);
    if behaviour.never_stop {
        loop {
            pending(SERVICE_STOP_PENDING, Duration::from_secs(1));
        }
    }
    pending(
        SERVICE_STOP_PENDING,
        Duration::from_millis(behaviour.stop_ms),
    );
    report(SERVICE_STOPPED, 0);
}
