//! Windows-only ETW Kernel-Process session.
//!
//! Opens the Microsoft-Windows-Kernel-Process provider via ferrisetw and
//! dispatches Launch + Terminate events to the dispatch callback, which
//! parses the EventRecord and hands the fields to `dispatch_record`
//! (timestamp, UUIDv7 event_id, cache insert or consult-and-purge,
//! enqueue to the EventRing).
//!
//! Per ADR-0009 §Decision part 1: the dispatch path is constrained to
//! parse-and-enqueue; no I/O, no synchronization beyond the ring's
//! Mutex + AtomicU64 + the cache's Mutex<HashMap>. The ring drain
//! (POST loop) runs on the agent's async task.
//!
//! Threading (SPEC-017 §Operational §1 and §4): `open` spawns one
//! dedicated pump thread that runs `trace.start()` and then
//! `process_from_handle()` (the pattern ferrisetw documents as its most
//! powerful option). `open` blocks until that thread reports the start
//! result, so a failed start (missing privilege, or any other Win32
//! error) reaches the caller with its code. The `UserTrace` is not
//! `Send`, so it stays on the pump thread; `stop` (or `Drop`) ends the
//! session by name through the side-channel helper, which makes
//! `process_from_handle` return, and then waits for the pump thread. A
//! second thread runs the hygiene work every 60 s (`hygiene.rs`).

use ferrisetw::native::EvntraceNativeError;
use ferrisetw::parser::Parser;
use ferrisetw::provider::Provider;
use ferrisetw::schema_locator::SchemaLocator;
use ferrisetw::trace::{TraceError, TraceTrait, UserTrace};
use ferrisetw::EventRecord;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::cache::CreatedTimeCache;
use super::dispatch::{dispatch_record, RawProcessRecord};
use super::hygiene::HygieneThread;
use super::ring::EventRing;
use super::types::{win32_from_os_error, ActivityId, OpenError};
use super::SESSION_NAME;

const KERNEL_PROCESS_GUID: &str = "22fb2cd6-0e7b-422b-a0c7-2fad1fd0e716";

/// `WINEVENT_KEYWORD_PROCESS` — Microsoft-Windows-Kernel-Process keyword for
/// ProcessStart (event_id 1) + ProcessStop (event_id 2). Per provider
/// manifest (`wevtutil get-publisher Microsoft-Windows-Kernel-Process`).
///
/// Passed as `MatchAnyKeyword` to `EnableTraceEx2` via ferrisetw's
/// `Provider::by_guid().any()`. Explicit subscription avoids the ambiguous
/// `MatchAnyKeyword=0` semantics which do not reliably deliver
/// ProcessStart/ProcessStop events across Windows builds.
const WINEVENT_KEYWORD_PROCESS: u64 = 0x10;

/// How long `stop` waits for the pump thread after stopping the session.
const PUMP_JOIN_TIMEOUT: Duration = Duration::from_secs(5);

/// ETW Kernel-Process capture session.
///
/// Owns the dispatch-side handles on the shared EventRing and
/// CreatedTimeCache, the pump thread that holds the `UserTrace`, and the
/// hygiene thread (cache sweep + `events_lost` poll every 60 s).
pub struct EtwSession {
    pub ring: Arc<EventRing>,
    pub cache: Arc<CreatedTimeCache>,
    pump: Option<JoinHandle<()>>,
    hygiene: Option<HygieneThread>,
}

impl EtwSession {
    /// Open the Kernel-Process ETW session.
    ///
    /// Returns only when the session has started (`Ok`) or its start has
    /// failed (`Err`, with the Win32 code). On success the pump thread
    /// delivers events to the dispatch callback until `stop`.
    pub fn open(ring_capacity: usize) -> Result<Self, OpenError> {
        tracing::info!(target: "cg_agent::etw", "EtwSession::open invoked");

        // Reclaim any zombie ETW session with our name left by a prior
        // crash or force-kill. ferrisetw 1.2 lacks stop_if_exist; same
        // side-channel pattern as events_lost (Phase 0 spike validated).
        match super::reclaim_zombie(SESSION_NAME) {
            Ok(true) => {
                tracing::warn!(
                    target: "cg_agent::etw",
                    session_name = SESSION_NAME,
                    "reclaimed zombie ETW session (pre-existing session stopped)",
                );
            }
            Ok(false) => {
                tracing::debug!(
                    target: "cg_agent::etw",
                    session_name = SESSION_NAME,
                    "no pre-existing ETW session (clean state)",
                );
            }
            Err(rc) => {
                tracing::warn!(
                    target: "cg_agent::etw",
                    session_name = SESSION_NAME,
                    win32_status = rc,
                    "zombie reclaim ControlTraceW(STOP) failed; continuing (session open may fail with AlreadyExist)",
                );
            }
        }

        let ring = Arc::new(EventRing::new(ring_capacity));
        let cache = Arc::new(CreatedTimeCache::new());

        let ring_for_callback = Arc::clone(&ring);
        let cache_for_callback = Arc::clone(&cache);

        let provider = Provider::by_guid(KERNEL_PROCESS_GUID)
            .any(WINEVENT_KEYWORD_PROCESS)
            .add_callback(
                move |record: &EventRecord, schema_locator: &SchemaLocator| {
                    tracing::trace!(
                        target: "cg_agent::etw",
                        event_id = record.event_id(),
                        "dispatch callback fired",
                    );
                    dispatch_callback(
                        record,
                        schema_locator,
                        &ring_for_callback,
                        &cache_for_callback,
                    );
                },
            )
            .build();

        let trace = UserTrace::new()
            .named(String::from(SESSION_NAME))
            .enable(provider);

        // The pump thread reports the start result once, then blocks in
        // process_from_handle until the session is stopped.
        let (started_tx, started_rx) = mpsc::channel::<Result<(), OpenError>>();
        let pump = std::thread::Builder::new()
            .name("cg-etw-pump".to_string())
            .spawn(move || {
                let (trace_session, handle) = match trace.start() {
                    Ok(started) => started,
                    Err(e) => {
                        let _ = started_tx.send(Err(open_error(&e)));
                        return;
                    }
                };
                let _ = started_tx.send(Ok(()));
                match UserTrace::process_from_handle(handle) {
                    Ok(()) => {
                        tracing::info!(
                            target: "cg_agent::etw",
                            "process_from_handle returned Ok (session stopped)",
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            target: "cg_agent::etw",
                            error = ?e,
                            "process_from_handle returned Err",
                        );
                    }
                }
                // UserTrace's Drop stops (already stopped) and closes the
                // trace handle.
                drop(trace_session);
            })
            .map_err(|e| OpenError::Failed {
                code: 0,
                message: format!("could not spawn the ETW pump thread: {e}"),
            })?;

        match started_rx.recv() {
            Ok(Ok(())) => {
                tracing::info!(
                    target: "cg_agent::etw",
                    session_name = SESSION_NAME,
                    provider_guid = KERNEL_PROCESS_GUID,
                    "ETW session opened",
                );
                let hygiene = match HygieneThread::spawn(Arc::clone(&cache), SESSION_NAME) {
                    Ok(thread) => Some(thread),
                    Err(e) => {
                        tracing::warn!(
                            target: "cg_agent::etw",
                            error = %e,
                            "could not spawn the hygiene thread; no cache sweep or events_lost poll",
                        );
                        None
                    }
                };
                Ok(Self {
                    ring,
                    cache,
                    pump: Some(pump),
                    hygiene,
                })
            }
            Ok(Err(e)) => {
                let _ = pump.join();
                // A start that failed after StartTraceW succeeded (e.g. in
                // EnableTraceEx2) can leave the session behind; a privilege
                // failure creates none.
                if !e.is_privilege() {
                    let _ = super::stop_session(SESSION_NAME);
                }
                Err(e)
            }
            Err(_) => {
                let _ = pump.join();
                Err(OpenError::Failed {
                    code: 0,
                    message: "the ETW pump thread ended before reporting the session start"
                        .to_string(),
                })
            }
        }
    }

    /// Stop the hygiene thread, then the session, and wait for the pump
    /// thread (SPEC-017 §Operational §4). Idempotent; also run by `Drop`.
    pub fn stop(&mut self) {
        if let Some(hygiene) = self.hygiene.take() {
            hygiene.stop();
        }
        let Some(pump) = self.pump.take() else {
            return;
        };
        if let Err(rc) = super::stop_session(SESSION_NAME) {
            tracing::warn!(
                target: "cg_agent::etw",
                session_name = SESSION_NAME,
                win32_status = rc,
                "ControlTraceW(STOP) of the agent's session failed",
            );
        }
        let deadline = Instant::now() + PUMP_JOIN_TIMEOUT;
        while !pump.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        if pump.is_finished() {
            let _ = pump.join();
            tracing::info!(
                target: "cg_agent::etw",
                session_name = SESSION_NAME,
                "ETW session closed",
            );
        } else {
            tracing::warn!(
                target: "cg_agent::etw",
                session_name = SESSION_NAME,
                timeout_ms = PUMP_JOIN_TIMEOUT.as_millis() as u64,
                "ETW pump thread did not finish after the session stop",
            );
        }
    }
}

impl Drop for EtwSession {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Map a ferrisetw start error to the agent's `OpenError`, recovering
/// the Win32 code (ferrisetw carries it as an `HRESULT_FROM_WIN32`).
fn open_error(err: &TraceError) -> OpenError {
    match err {
        TraceError::EtwNativeError(EvntraceNativeError::IoError(io)) => match io.raw_os_error() {
            Some(raw) => OpenError::from_win32(win32_from_os_error(raw)),
            None => OpenError::Failed {
                code: 0,
                message: io.to_string(),
            },
        },
        // ERROR_ALREADY_EXISTS (183): a session of our name survived the
        // startup reclaim.
        TraceError::EtwNativeError(EvntraceNativeError::AlreadyExist) => OpenError::from_win32(183),
        // ERROR_INVALID_HANDLE (6).
        TraceError::EtwNativeError(EvntraceNativeError::InvalidHandle) => OpenError::from_win32(6),
        TraceError::InvalidTraceName => OpenError::Failed {
            code: 0,
            message: "invalid ETW session name".to_string(),
        },
    }
}

fn dispatch_callback(
    record: &EventRecord,
    schema_locator: &SchemaLocator,
    ring: &EventRing,
    cache: &CreatedTimeCache,
) {
    let activity_id = match record.event_id() {
        1 => ActivityId::Launch,
        2 => ActivityId::Terminate,
        _ => return,
    };

    let schema = match schema_locator.event_schema(record) {
        Ok(s) => s,
        Err(_) => return,
    };

    let parser = Parser::create(record, &schema);

    let raw = RawProcessRecord {
        activity_id,
        pid: parser.try_parse("ProcessID").unwrap_or(0),
        parent_pid: parser.try_parse("ParentProcessID").unwrap_or(0),
        // Property name is "ImageName" per the Kernel-Process provider manifest (v0–v4).
        image_file_name: parser.try_parse("ImageName").unwrap_or_default(),
        command_line: parser.try_parse("CommandLine").unwrap_or_default(),
        subject_user_sid: parser.try_parse("UserSID").unwrap_or_default(),
        filetime_100ns: record.raw_timestamp(),
        exit_status: match activity_id {
            // Property name is "ExitCode" per the Kernel-Process provider manifest (v0–v2).
            ActivityId::Terminate => parser.try_parse("ExitCode").ok(),
            ActivityId::Launch => None,
        },
    };

    dispatch_record(raw, cache, ring);
}
