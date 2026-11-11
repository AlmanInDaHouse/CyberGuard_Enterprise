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
//!   the service's name, as one JSON array per line, before `RUNNING`;
//! - `log_dir` and `log`: once `RUNNING`, the steps it takes on the log in
//!   that directory, in order: `{"line": "<message>"}` appends a JSON line
//!   whose `fields.message` is the message to `agent.jsonl`, as the agent's
//!   log writes one, holding that file open as the agent does;
//!   `"rotate"` renames as the agent's rotation renames; `{"sleep_ms": <n>}`
//!   waits;
//! - `log_while_pending`: those steps are taken in `START_PENDING` instead,
//!   before its pending time;
//! - `stop_with`: after those steps it reports `STOPPED` with this
//!   service-specific exit code, unasked.
//!
//! In each pending state its checkpoint advances every second.

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use cg_agent::service::rotation::{archive_name, ACTIVE, ARCHIVES, ROTATING};
use serde::Deserialize;
use windows_sys::core::PWSTR;
use windows_sys::Win32::Foundation::{
    ERROR_CALL_NOT_IMPLEMENTED, ERROR_SERVICE_SPECIFIC_ERROR, NO_ERROR,
};
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
    log_dir: Option<PathBuf>,
    log: Vec<Step>,
    log_while_pending: bool,
    stop_with: Option<u32>,
}

/// One step on the log.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum Step {
    Line(String),
    Rotate,
    SleepMs(u64),
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
    report_with(state, accepted, 0);
}

/// `report`, with `code` as its service-specific exit code when it is not 0.
fn report_with(state: u32, accepted: u32, code: u32) {
    let pending = state == SERVICE_START_PENDING || state == SERVICE_STOP_PENDING;
    let status = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: state,
        dwControlsAccepted: accepted,
        dwWin32ExitCode: if code == 0 {
            NO_ERROR
        } else {
            ERROR_SERVICE_SPECIFIC_ERROR
        },
        dwServiceSpecificExitCode: code,
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
    let _held_while_pending = match &behaviour.log_dir {
        Some(dir) if behaviour.log_while_pending => take_steps(dir, &behaviour.log),
        _ => None,
    };
    pending(
        SERVICE_START_PENDING,
        Duration::from_millis(behaviour.start_pending_ms),
    );
    if behaviour.refuse_stop_ms > 0 {
        report(SERVICE_RUNNING, 0);
        std::thread::sleep(Duration::from_millis(behaviour.refuse_stop_ms));
    }
    report(SERVICE_RUNNING, SERVICE_ACCEPT_STOP);
    let _held = match &behaviour.log_dir {
        Some(dir) if !behaviour.log_while_pending => take_steps(dir, &behaviour.log),
        _ => None,
    };
    if let Some(code) = behaviour.stop_with {
        report_with(SERVICE_STOPPED, 0, code);
        return;
    }
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

/// The steps on the log in `dir`, in order. The active file is held open as
/// the agent holds it, appending and sharing read alone, and closed before a
/// rotation renames it; the service holds the last one open until it stops.
fn take_steps(dir: &std::path::Path, steps: &[Step]) -> Option<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt as _;
    let open = || {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ)
            .open(dir.join(ACTIVE))
            .expect("open the active file")
    };
    let mut held: Option<std::fs::File> = None;
    for step in steps {
        match step {
            Step::Line(message) => {
                let line = serde_json::json!({
                    "timestamp": "2026-11-11T10:00:00.000000Z",
                    "level": "INFO",
                    "fields": { "message": message },
                    "target": "setup_service",
                });
                let file = held.get_or_insert_with(open);
                writeln!(file, "{line}").expect("write a line");
            }
            // As the agent's rotation: the active file, closed, to the
            // rotating name, a new active file, the archives shifted into the
            // first free place, the rotating file to archive 1.
            Step::Rotate => {
                drop(held.take());
                std::fs::rename(dir.join(ACTIVE), dir.join(ROTATING)).expect("to rotating");
                held = Some(open());
                let free = (1..=ARCHIVES)
                    .find(|&n| !dir.join(archive_name(n)).exists())
                    .unwrap_or_else(|| {
                        std::fs::remove_file(dir.join(archive_name(ARCHIVES))).expect("the oldest");
                        ARCHIVES
                    });
                for n in (1..free).rev() {
                    std::fs::rename(dir.join(archive_name(n)), dir.join(archive_name(n + 1)))
                        .expect("shift an archive");
                }
                std::fs::rename(dir.join(ROTATING), dir.join(archive_name(1)))
                    .expect("to archive 1");
            }
            Step::SleepMs(ms) => std::thread::sleep(Duration::from_millis(*ms)),
        }
    }
    held
}
