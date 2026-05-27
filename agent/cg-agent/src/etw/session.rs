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
use ferrisetw::trace::UserTrace;
use ferrisetw::EventRecord;
use std::sync::Arc;
use uuid::Uuid;

use super::cache::CreatedTimeCache;
use super::ring::EventRing;
use super::types::{ActivityId, CapturedEvent, OpenError};

const KERNEL_PROCESS_GUID: &str = "22fb2cd6-0e7b-422b-a0c7-2fad1fd0e716";
const SESSION_NAME: &str = "CGAgent-KernelProcess";

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

        let ring = Arc::new(EventRing::new(ring_capacity));
        let cache = Arc::new(CreatedTimeCache::new());

        let ring_for_callback = Arc::clone(&ring);
        let cache_for_callback = Arc::clone(&cache);

        let provider = Provider::by_guid(KERNEL_PROCESS_GUID)
            .add_callback(
                move |record: &EventRecord, schema_locator: &SchemaLocator| {
                    tracing::info!(
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

        // Phase 3.5.I-FIX2: ferrisetw 1.2's trace.start() returns
        // Result<(UserTrace, PROCESSTRACE_HANDLE), TraceError>. The Ok
        // variant carries the live session handles. β3's pattern bound
        // them to underscore-prefixed names; underscore prefix only
        // suppresses unused-variable warnings, it does NOT extend
        // lifetime, so both binds dropped at the end of the match arm
        // (~500µs after trace.start returned). UserTrace's Drop impl
        // closed the underlying ETW session and Kernel-Process events
        // stopped flowing — empirically confirmed at Phase 3.5.I-DIAG3
        // (3cbe845): "trace.start completed Ok" followed by 40× empty
        // ring drains with zero dispatch callback firings.
        //
        // Fix: park the spawned thread inside the Ok arm. park() blocks
        // indefinitely (until unpark, which is never called); the
        // kept_trace + kept_handle bindings stay live in the parked
        // thread's stack for the lifetime of the process. At process
        // exit the OS detaches this thread; bindings drop; UserTrace's
        // Drop impl closes the session cleanly.
        std::thread::spawn(move || {
            tracing::info!(target: "cg_agent::etw", "trace.start invoked on spawned thread");
            match trace.start() {
                Ok((kept_trace, kept_handle)) => {
                    tracing::info!(
                        target: "cg_agent::etw",
                        "trace.start completed Ok; parking thread to keep ETW session alive",
                    );
                    std::thread::park();
                    // Unreachable in current usage (no unpark call site).
                    // Reserved for a future explicit-shutdown refactor;
                    // documents the intended cleanup path. PROCESSTRACE_HANDLE
                    // is Copy so drop() would be a no-op; kept_handle simply
                    // falls out of scope. UserTrace has a non-trivial Drop
                    // impl that closes the session.
                    let _ = kept_handle;
                    drop(kept_trace);
                }
                Err(e) => {
                    tracing::error!(target: "cg_agent::etw", error = ?e, "trace.start failed")
                }
            }
        });

        tracing::info!(target: "cg_agent::etw", "EtwSession::open returning Ok");
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
    let image_file_name: String = parser.try_parse("ImageFileName").unwrap_or_default();
    let command_line: String = parser.try_parse("CommandLine").unwrap_or_default();
    let subject_user_sid: String = parser.try_parse("UserSID").unwrap_or_default();
    let exit_status: Option<i32> = match activity_id {
        ActivityId::Terminate => parser.try_parse("ExitStatus").ok(),
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
