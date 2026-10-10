//! Windows: the pulled subscription to the Security log (SPEC-020
//! §Operational §1, ADR-0019 §1).
//!
//! `LogonSubscription::open` subscribes to the `Security` channel for
//! events 4624 and 4625, written from now on, with no bookmark, and starts
//! the agent's own logon thread. The thread waits on the subscription's
//! signal and on a stop event — and wakes every second regardless, so a
//! signal raised while it was reading is not lost — then reads the
//! available records in batches, renders each through one values context
//! (by name, not as XML), and hands the values to `dispatch_logon_record`.
//! A failed read is logged at most once per 60 s, and the subscription is
//! reopened 5 s later. `stop` sets the stop event, waits for the thread,
//! then closes the subscription the thread hands back (§Operational §6).
//!
//! [`RenderContext`] renders one record already obtained; the agent reads
//! no record but those its subscription delivers (ADR-0019 §8, live
//! only).
//!
//! Nothing here logs a user, SID, domain, workstation or address
//! (§Operational §11).

use std::ffi::c_void;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS, HANDLE,
    WAIT_OBJECT_0,
};
use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows_sys::Win32::System::EventLog::{
    EvtClose, EvtCreateRenderContext, EvtNext, EvtRender, EvtRenderContextValues,
    EvtRenderEventValues, EvtSubscribe, EvtSubscribeToFutureEvents, EvtVarTypeByte,
    EvtVarTypeFileTime, EvtVarTypeHexInt32, EvtVarTypeNull, EvtVarTypeSid, EvtVarTypeString,
    EvtVarTypeUInt16, EvtVarTypeUInt32, EVT_HANDLE, EVT_VARIANT, EVT_VARIANT_TYPE_MASK,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, ResetEvent, SetEvent, WaitForMultipleObjects, WaitForSingleObject,
};

use super::{
    dispatch_logon_record, LogonCounters, RawLogonRecord, UnusableMonitor, UnusableSample,
    LOG_INTERVAL,
};
use crate::errors::LogonError;
use crate::etw::EventRing;

/// The channel and the query of SPEC-020 §Operational §1.
const CHANNEL: &str = "Security";
const QUERY: &str = "*[System[(EventID=4624 or EventID=4625)]]";

/// The values rendered from each record, by name, in this order.
const VALUE_PATHS: [&str; 13] = [
    "Event/System/EventID",
    "Event/System/Version",
    "Event/System/TimeCreated/@SystemTime",
    "Event/EventData/Data[@Name='TargetUserSid']",
    "Event/EventData/Data[@Name='TargetUserName']",
    "Event/EventData/Data[@Name='TargetDomainName']",
    "Event/EventData/Data[@Name='LogonType']",
    "Event/EventData/Data[@Name='AuthenticationPackageName']",
    "Event/EventData/Data[@Name='WorkstationName']",
    "Event/EventData/Data[@Name='IpAddress']",
    "Event/EventData/Data[@Name='ElevatedToken']",
    "Event/EventData/Data[@Name='Status']",
    "Event/EventData/Data[@Name='SubStatus']",
];

/// Records read per `EvtNext` call.
const BATCH: usize = 32;
/// The thread's wakeup when no signal comes.
const POLL: Duration = Duration::from_secs(1);
/// The wait before a failed subscription is reopened.
const REOPEN_DELAY: Duration = Duration::from_secs(5);

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn last_error() -> u32 {
    // SAFETY: reads the calling thread's last-error value.
    unsafe { GetLastError() }
}

/// An `EVT_HANDLE` closed on drop.
struct Evt(EVT_HANDLE);

impl Drop for Evt {
    fn drop(&mut self) {
        if self.0 != 0 {
            // SAFETY: the handle came from an Evt* function and is closed once.
            unsafe { EvtClose(self.0) };
        }
    }
}

/// A Win32 event object closed on drop. Kernel handles may be used from
/// any thread.
struct Event(HANDLE);

// SAFETY: an event handle is a kernel object reference, usable from any
// thread; the struct owns it and closes it once.
unsafe impl Send for Event {}
// SAFETY: SetEvent / ResetEvent / waits on one handle from several threads
// are safe by the Win32 contract.
unsafe impl Sync for Event {}

impl Event {
    fn new(manual_reset: bool, initial: bool) -> Result<Self, LogonError> {
        // SAFETY: default security, unnamed event.
        let handle = unsafe {
            CreateEventW(
                std::ptr::null(),
                i32::from(manual_reset),
                i32::from(initial),
                std::ptr::null(),
            )
        };
        if handle.is_null() {
            return Err(LogonError::from_win32(last_error()));
        }
        Ok(Self(handle))
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        // SAFETY: the handle came from CreateEventW and is closed once.
        unsafe { CloseHandle(self.0) };
    }
}

/// The values context of SPEC-020 §Operational §1: the 13 values the
/// agent renders from a record, by name.
pub struct RenderContext(Evt);

impl RenderContext {
    pub fn new() -> Result<Self, LogonError> {
        let paths: Vec<Vec<u16>> = VALUE_PATHS.iter().map(|p| wide(p)).collect();
        let ptrs: Vec<*const u16> = paths.iter().map(|p| p.as_ptr()).collect();
        // SAFETY: `ptrs` points into `paths`, alive for the call.
        let handle = unsafe {
            EvtCreateRenderContext(ptrs.len() as u32, ptrs.as_ptr(), EvtRenderContextValues)
        };
        if handle == 0 {
            return Err(LogonError::from_win32(last_error()));
        }
        Ok(Self(Evt(handle)))
    }

    /// Render the values of one record whose handle the caller holds.
    /// `Err` holds the Win32 code of a failed render, or `None` when the
    /// record lacks the values the agent needs.
    pub fn render(&self, event: EVT_HANDLE) -> Result<RawLogonRecord, Option<u32>> {
        render(self.0 .0, event)
    }
}

/// Subscribe to the channel's future events; `Err` with the Win32 code.
fn subscribe(signal: &Event) -> Result<Evt, u32> {
    let channel = wide(CHANNEL);
    let query = wide(QUERY);
    // SAFETY: the strings live for the call; no bookmark, no callback.
    let handle = unsafe {
        EvtSubscribe(
            0,
            signal.0,
            channel.as_ptr(),
            query.as_ptr(),
            0,
            std::ptr::null(),
            None,
            EvtSubscribeToFutureEvents,
        )
    };
    if handle == 0 {
        return Err(last_error());
    }
    Ok(Evt(handle))
}

/// A NUL-terminated wide string.
///
/// # Safety
/// `ptr` is null or points to a NUL-terminated UTF-16 string.
unsafe fn wide_to_string(ptr: *const u16) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let mut len = 0usize;
    while *ptr.add(len) != 0 {
        len += 1;
    }
    Some(String::from_utf16_lossy(std::slice::from_raw_parts(
        ptr, len,
    )))
}

/// One rendered value as text, number or SID string; `None` for a value
/// of another type or none.
///
/// # Safety
/// `v` is a variant `EvtRender` filled, whose pointers live as long as the
/// render buffer.
unsafe fn text_of(v: &EVT_VARIANT) -> Option<String> {
    match v.Type & EVT_VARIANT_TYPE_MASK {
        t if t == EvtVarTypeString as u32 => wide_to_string(v.Anonymous.StringVal),
        t if t == EvtVarTypeSid as u32 => {
            let mut out: *mut u16 = std::ptr::null_mut();
            if ConvertSidToStringSidW(v.Anonymous.SidVal, &mut out) == 0 {
                return None;
            }
            let text = wide_to_string(out);
            LocalFree(out as *mut c_void);
            text
        }
        _ => None,
    }
}

/// # Safety
/// As for [`text_of`].
unsafe fn u32_of(v: &EVT_VARIANT) -> Option<u32> {
    match v.Type & EVT_VARIANT_TYPE_MASK {
        t if t == EvtVarTypeUInt32 as u32 || t == EvtVarTypeHexInt32 as u32 => {
            Some(v.Anonymous.UInt32Val)
        }
        t if t == EvtVarTypeUInt16 as u32 => Some(u32::from(v.Anonymous.UInt16Val)),
        t if t == EvtVarTypeByte as u32 => Some(u32::from(v.Anonymous.ByteVal)),
        _ => None,
    }
}

/// Render one record's values (SPEC-020 §Operational §1). `Err` holds the
/// Win32 code of a render that failed, or `None` when the record lacks
/// the values the agent needs.
fn render(context: EVT_HANDLE, event: EVT_HANDLE) -> Result<RawLogonRecord, Option<u32>> {
    let mut used = 0u32;
    let mut count = 0u32;
    // SAFETY: a size query with no buffer.
    let ok = unsafe {
        EvtRender(
            context,
            event,
            EvtRenderEventValues,
            0,
            std::ptr::null_mut(),
            &mut used,
            &mut count,
        )
    };
    if ok == 0 {
        let code = last_error();
        if code != ERROR_INSUFFICIENT_BUFFER {
            return Err(Some(code));
        }
    }
    // u64 words keep the variants aligned.
    let mut buffer = vec![0u64; (used as usize).div_ceil(8)];
    // SAFETY: the buffer holds `used` bytes.
    let ok = unsafe {
        EvtRender(
            context,
            event,
            EvtRenderEventValues,
            (buffer.len() * 8) as u32,
            buffer.as_mut_ptr() as *mut c_void,
            &mut used,
            &mut count,
        )
    };
    if ok == 0 {
        return Err(Some(last_error()));
    }
    if (count as usize) < VALUE_PATHS.len() {
        return Err(None);
    }
    // SAFETY: EvtRender wrote `count` variants at the start of the buffer.
    let values = unsafe {
        std::slice::from_raw_parts(buffer.as_ptr() as *const EVT_VARIANT, count as usize)
    };
    // SAFETY: the variants and what they point to live in `buffer`.
    unsafe {
        let null = |v: &EVT_VARIANT| v.Type & EVT_VARIANT_TYPE_MASK == EvtVarTypeNull as u32;
        let filetime = if values[2].Type & EVT_VARIANT_TYPE_MASK == EvtVarTypeFileTime as u32 {
            values[2].Anonymous.FileTimeVal as i64
        } else {
            return Err(None);
        };
        Ok(RawLogonRecord {
            event_id: u32_of(&values[0]).unwrap_or(0) as u16,
            version: u32_of(&values[1]).unwrap_or(0) as u8,
            filetime_100ns: filetime,
            target_user_sid: if null(&values[3]) {
                None
            } else {
                text_of(&values[3])
            },
            target_user_name: text_of(&values[4]),
            target_domain_name: text_of(&values[5]),
            logon_type: u32_of(&values[6]),
            auth_package: text_of(&values[7]),
            workstation: text_of(&values[8]),
            ip_address: text_of(&values[9]),
            elevated_token: text_of(&values[10]),
            status: u32_of(&values[11]),
            sub_status: u32_of(&values[12]),
        })
    }
}

/// Read every record available on `results`, render each and pass it to
/// `each`; render failures go to `counters`. `Ok` when the results are
/// exhausted, `Err` with the Win32 code of a failed read.
fn drain(
    results: EVT_HANDLE,
    context: EVT_HANDLE,
    counters: &LogonCounters,
    mut each: impl FnMut(RawLogonRecord) -> bool,
) -> Result<(), u32> {
    loop {
        let mut handles = [0isize; BATCH];
        let mut returned = 0u32;
        // SAFETY: `handles` has room for BATCH handles.
        let ok = unsafe {
            EvtNext(
                results,
                BATCH as u32,
                handles.as_mut_ptr(),
                0,
                0,
                &mut returned,
            )
        };
        if ok == 0 {
            let code = last_error();
            return if code == ERROR_NO_MORE_ITEMS {
                Ok(())
            } else {
                Err(code)
            };
        }
        let mut more = true;
        for &handle in &handles[..returned as usize] {
            let event = Evt(handle);
            if more {
                match render(context, event.0) {
                    Ok(raw) => more = each(raw),
                    Err(code) => counters.record_unusable(UnusableSample {
                        event_id: None,
                        win32_code: code,
                    }),
                }
            }
        }
        if !more {
            return Ok(());
        }
    }
}

/// The subscription and its thread (SPEC-020 §Operational §1, §6).
pub struct LogonSubscription {
    stop: Arc<Event>,
    /// The thread hands the subscription back when it ends, for `stop` to
    /// close it.
    thread: Option<JoinHandle<Option<Evt>>>,
    counters: Arc<LogonCounters>,
}

impl LogonSubscription {
    /// Subscribe and start the logon thread, which enqueues into `ring`.
    /// Returns only once the subscription is open, or with the Win32
    /// failure to open it.
    pub fn open(ring: Arc<EventRing>) -> Result<Self, LogonError> {
        let context = RenderContext::new()?;
        // Manual reset, initially signaled: the thread reads first.
        let signal = Event::new(true, true)?;
        let subscription = subscribe(&signal).map_err(LogonError::from_win32)?;
        let stop = Arc::new(Event::new(true, false)?);
        let counters = Arc::new(LogonCounters::new());

        let thread_stop = Arc::clone(&stop);
        let thread_counters = Arc::clone(&counters);
        let thread = std::thread::Builder::new()
            .name("cg-logon".to_string())
            .spawn(move || {
                run(
                    context,
                    signal,
                    subscription,
                    &thread_stop,
                    &ring,
                    &thread_counters,
                )
            })
            .map_err(|e| LogonError::Failed {
                code: 0,
                message: format!("spawn the logon thread: {e}"),
            })?;
        tracing::info!(
            target: "cg_agent::logon",
            channel = CHANNEL,
            "Security log subscription opened"
        );
        Ok(Self {
            stop,
            thread: Some(thread),
            counters,
        })
    }

    /// The count of records the agent could not use.
    pub fn counters(&self) -> &LogonCounters {
        &self.counters
    }

    /// Signal the thread to stop, wait for it, then close the subscription
    /// it hands back (SPEC-020 §Operational §6). Idempotent.
    pub fn stop(&mut self) {
        if let Some(thread) = self.thread.take() {
            // SAFETY: the event is alive while `self.stop` is.
            unsafe { SetEvent(self.stop.0) };
            let subscription = thread.join().ok().flatten();
            drop(subscription);
        }
    }
}

impl Drop for LogonSubscription {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The logon thread.
fn run(
    context: RenderContext,
    signal: Event,
    subscription: Evt,
    stop: &Event,
    ring: &EventRing,
    counters: &LogonCounters,
) -> Option<Evt> {
    let mut subscription = Some(subscription);
    let mut monitor = UnusableMonitor::new();
    let mut last_failure_log: Option<Instant> = None;
    loop {
        let handles = [stop.0, signal.0];
        // SAFETY: both handles are alive for the wait.
        let woke =
            unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, POLL.as_millis() as u32) };
        if woke == WAIT_OBJECT_0 {
            break;
        }
        let Some(current) = subscription.as_ref() else {
            subscription = reopen(&signal, stop, &mut last_failure_log);
            continue;
        };
        match drain(current.0, context.0 .0, counters, |raw| {
            dispatch_logon_record(&raw, ring, counters);
            true
        }) {
            Ok(()) => {
                // SAFETY: the event is alive; the next signal sets it again.
                unsafe { ResetEvent(signal.0) };
            }
            Err(code) => {
                log_failure(&mut last_failure_log, "read", code);
                subscription = None;
                if wait_or_stop(stop, REOPEN_DELAY) {
                    break;
                }
                subscription = reopen(&signal, stop, &mut last_failure_log);
            }
        }
        monitor.observe(counters, Instant::now());
    }
    subscription
}

/// Wait `delay`, or less when the stop event fires; true when it did.
fn wait_or_stop(stop: &Event, delay: Duration) -> bool {
    // SAFETY: the handle is alive for the wait.
    unsafe { WaitForSingleObject(stop.0, delay.as_millis() as u32) == WAIT_OBJECT_0 }
}

fn reopen(signal: &Event, stop: &Event, last_log: &mut Option<Instant>) -> Option<Evt> {
    match subscribe(signal) {
        Ok(subscription) => Some(subscription),
        Err(code) => {
            log_failure(last_log, "reopen", code);
            let _ = wait_or_stop(stop, REOPEN_DELAY);
            None
        }
    }
}

fn log_failure(last_log: &mut Option<Instant>, what: &str, code: u32) {
    let now = Instant::now();
    if last_log.is_none_or(|last| now.duration_since(last) >= LOG_INTERVAL) {
        tracing::warn!(
            target: "cg_agent::logon",
            operation = what,
            win32_code = code,
            "Security log subscription failed; reopening it from new events"
        );
        *last_log = Some(now);
    }
}
