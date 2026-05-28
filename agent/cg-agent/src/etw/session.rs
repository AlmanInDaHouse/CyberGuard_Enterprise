//! Windows-only ETW Kernel-Process session.
//!
//! Opens the Microsoft-Windows-Kernel-Process provider via ferrisetw,
//! dispatches Launch + Terminate events to the dispatch callback, which
//! parses the EventRecord, constructs a CapturedEvent, populates the
//! CreatedTimeCache (Launch) or leaves it (Terminate), and enqueues to
//! the EventRing.
//!
//! Per ADR-0009 §Decision part 1: the dispatch path is constrained to
//! parse-and-enqueue; no I/O, no synchronization beyond the ring's
//! Mutex + AtomicU64 + the cache's Mutex<HashMap>. The ring drain
//! (POST loop) happens on a separate tokio task; β3 spawns this from
//! `run_test_mode`.
//!
//! Architecture decision (Option A per the β3 brief): the UserTrace
//! handle is moved into a spawned std::thread that runs `trace.start()`
//! (blocking, returns when the session is stopped). EtwSession itself
//! stores only the Arc<EventRing> + Arc<CreatedTimeCache>; trace's
//! lifetime is implicit in the thread's lifetime. On process exit the
//! thread is leaked along with the trace; for integration-test usage
//! (AC-004 cache-hit + AC-007 run a single agent per test then exit
//! the process) this is acceptable. Phase 4+ refactor candidate when
//! explicit stop() semantics are needed (hot-reload, graceful restart).
//!
//! Privilege-check semantics: `open()` currently always returns Ok on
//! Windows. The real privilege check happens inside `trace.start()` on
//! the spawned thread, but its error is not surfaced (the thread
//! silently exits, the drain task in lib.rs's run_test_mode polls an
//! empty ring and POSTs empty envelopes). AC-002 exercises the
//! Err(OpenError) path via synthetic injection in
//! `handle_etw_open_result`, not via this open(). Phase 4+ could add
//! a sync Win32 privilege probe before the spawn.

use ferrisetw::parser::Parser;
use ferrisetw::provider::Provider;
use ferrisetw::schema_locator::SchemaLocator;
use ferrisetw::trace::{TraceTrait, UserTrace};
use ferrisetw::EventRecord;
use std::sync::Arc;
use uuid::Uuid;

use super::cache::CreatedTimeCache;
use super::ring::EventRing;
use super::types::{ActivityId, CapturedEvent, OpenError};

const KERNEL_PROCESS_GUID: &str = "22fb2cd6-0e7b-422b-a0c7-2fad1fd0e716";
const SESSION_NAME: &str = "CGAgent-KernelProcess";

/// `WINEVENT_KEYWORD_PROCESS` — Microsoft-Windows-Kernel-Process keyword for
/// ProcessStart (event_id 1) + ProcessStop (event_id 2). Per provider
/// manifest (`wevtutil get-publisher Microsoft-Windows-Kernel-Process`).
///
/// Passed as `MatchAnyKeyword` to `EnableTraceEx2` via ferrisetw's
/// `Provider::by_guid().any()`. Explicit subscription avoids the ambiguous
/// `MatchAnyKeyword=0` semantics which do not reliably deliver
/// ProcessStart/ProcessStop events across Windows builds.
const WINEVENT_KEYWORD_PROCESS: u64 = 0x10;

/// ETW Kernel-Process capture session.
///
/// Owns the dispatch-side handles on the shared EventRing and
/// CreatedTimeCache. The UserTrace itself lives in a separately-spawned
/// std::thread (see module-level doc-comment on architecture).
pub struct EtwSession {
    pub ring: Arc<EventRing>,
    pub cache: Arc<CreatedTimeCache>,
}

impl EtwSession {
    /// Open the Kernel-Process ETW session.
    ///
    /// Spawns a background thread that runs `UserTrace::start()`
    /// (blocking ETW event-pump loop). The dispatch callback closes
    /// over Arc clones of the ring + cache; the returned EtwSession
    /// holds equivalent Arc clones so the caller (drain task) can
    /// snapshot the ring and consult/purge the cache.
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

        // Phase 4 fix: call start() then process_from_handle() on the
        // SAME dedicated thread. ferrisetw's documented "most powerful
        // option" (trace.rs §TraceBuilder::start docstring): start()
        // registers the session (StartTraceW + EnableTraceEx2 +
        // OpenTraceW), process_from_handle() blocks on Win32 ProcessTrace
        // which delivers events to the callback on this thread.
        //
        // Prior pattern (start_and_process + park) separated the session
        // owner thread from the ProcessTrace pump thread — ferrisetw's
        // start_and_process() spawns an internal fire-and-forget thread
        // for ProcessTrace. That topology prevented event delivery: the
        // callback never fired despite the session being open. Collapsing
        // start + pump onto one dedicated thread (the spike's working
        // pattern) resolves the dispatch gap.
        //
        // The OUTER thread::spawn isolates the non-Send UserTrace from
        // the tokio async runtime in run_test_mode. UserTrace lives as a
        // local on this thread's stack; its Drop impl calls
        // ControlTrace(STOP) when process_from_handle returns.
        std::thread::spawn(move || {
            tracing::info!(target: "cg_agent::etw", "trace.start invoked on dedicated ETW thread");
            match trace.start() {
                Ok((trace_session, handle)) => {
                    tracing::info!(
                        target: "cg_agent::etw",
                        "trace.start completed Ok; entering process_from_handle pump on this thread (blocking until session stops)",
                    );
                    match UserTrace::process_from_handle(handle) {
                        Ok(()) => {
                            tracing::info!(
                                target: "cg_agent::etw",
                                "process_from_handle returned Ok (session stopped normally)",
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
                    // trace_session (UserTrace) drops here — its Drop impl
                    // calls ControlTrace(STOP), releasing the OS session.
                    drop(trace_session);
                }
                Err(e) => {
                    tracing::error!(target: "cg_agent::etw", error = ?e, "trace.start failed")
                }
            }
        });

        tracing::debug!(target: "cg_agent::etw", "EtwSession::open returning Ok");
        Ok(Self { ring, cache })
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

    let pid: u32 = parser.try_parse("ProcessID").unwrap_or(0);
    let parent_pid: u32 = parser.try_parse("ParentProcessID").unwrap_or(0);
    // Property name is "ImageName" per the Kernel-Process provider manifest (v0–v4).
    let image_file_name: String = parser.try_parse("ImageName").unwrap_or_default();
    let command_line: String = parser.try_parse("CommandLine").unwrap_or_default();
    let subject_user_sid: String = parser.try_parse("UserSID").unwrap_or_default();
    let exit_status: Option<i32> = match activity_id {
        // Property name is "ExitCode" per the Kernel-Process provider manifest (v0–v2).
        ActivityId::Terminate => parser.try_parse("ExitCode").ok(),
        ActivityId::Launch => None,
    };

    let etw_timestamp_nanos = etw_filetime_to_unix_nanos(record.raw_timestamp());
    let event_id = Uuid::new_v4().to_string();

    let event = CapturedEvent {
        pid,
        event_id,
        activity_id,
        image_file_name,
        parent_pid,
        command_line,
        subject_user_sid,
        etw_timestamp_nanos,
        exit_status,
    };

    if matches!(activity_id, ActivityId::Launch) {
        cache.insert(pid, etw_timestamp_nanos);
    }

    ring.enqueue_or_drop(event);
}

/// Convert ETW FILETIME (100-nanosecond intervals since 1601-01-01 UTC)
/// to Unix nanoseconds (since 1970-01-01 UTC).
///
/// FILETIME-to-Unix-nanos: 11_644_473_600 seconds between 1601 and 1970
/// epochs × 10_000_000 (100-ns intervals per second) = 116444736000000000.
/// Per SPEC-005 §Operational §1.
fn etw_filetime_to_unix_nanos(filetime_100ns: i64) -> u64 {
    const FILETIME_TO_UNIX_100NS: i64 = 116_444_736_000_000_000;
    let unix_100ns = filetime_100ns.saturating_sub(FILETIME_TO_UNIX_100NS);
    (unix_100ns.max(0) as u64).saturating_mul(100)
}
