//! Side-channel ETW session helpers via raw `ControlTraceW` (windows-sys).
//!
//! ferrisetw 1.2 exposes no native stats-query or zombie-reclaim API
//! (registry inspection confirms `query.rs` only handles
//! TRACE_PROFILE_INTERVAL sampling, not session-lost-events; and
//! `stop_if_exist` exists on master but not in the 1.2.0 release).
//! Two helpers live here:
//!
//! - `events_lost`: `EVENT_TRACE_CONTROL_QUERY` → `EventsLost` field
//!   (ADR-0008 §Decision part 2).
//! - `reclaim_zombie`: `EVENT_TRACE_CONTROL_STOP` by session name,
//!   treating "no such session" as success (Phase 0 spike pattern).

#[cfg(target_os = "windows")]
mod windows_impl {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Diagnostics::Etw::{
        ControlTraceW, CONTROLTRACE_HANDLE, EVENT_TRACE_CONTROL_QUERY, EVENT_TRACE_CONTROL_STOP,
        EVENT_TRACE_PROPERTIES, WNODE_HEADER,
    };

    const EVENT_TRACE_PROPERTIES_SIZE: usize = std::mem::size_of::<EVENT_TRACE_PROPERTIES>();
    const MAX_SESSION_NAME_WCHARS: usize = 1024;
    const MAX_LOG_FILE_WCHARS: usize = 1024;
    const BUFFER_SIZE: usize =
        EVENT_TRACE_PROPERTIES_SIZE + (MAX_SESSION_NAME_WCHARS * 2) + (MAX_LOG_FILE_WCHARS * 2);

    /// Query the kernel's lost-events counter for the named ETW session.
    ///
    /// Returns `Ok(events_lost)` on success (the kernel counter is
    /// monotonically non-decreasing within a session lifetime per
    /// ADR-0008 §Empirical justification). Returns `Err(win32_status)`
    /// on Win32 error (typically `ERROR_WMI_INSTANCE_NOT_FOUND` if the
    /// session is not running).
    pub fn events_lost(session_name: &str) -> Result<u32, u32> {
        let mut session_name_wide: Vec<u16> = OsStr::new(session_name).encode_wide().collect();
        session_name_wide.push(0);

        let mut buffer = vec![0u8; BUFFER_SIZE];
        let properties_ptr = buffer.as_mut_ptr() as *mut EVENT_TRACE_PROPERTIES;

        // SAFETY: buffer is BUFFER_SIZE bytes (asserted at type level via
        // the vec construction) and properties_ptr points to its start.
        // EVENT_TRACE_PROPERTIES is repr(C); zero-init is valid (POD).
        unsafe {
            let properties = &mut *properties_ptr;
            properties.Wnode = WNODE_HEADER {
                BufferSize: BUFFER_SIZE as u32,
                ..std::mem::zeroed()
            };
            properties.LoggerNameOffset = EVENT_TRACE_PROPERTIES_SIZE as u32;
            properties.LogFileNameOffset =
                (EVENT_TRACE_PROPERTIES_SIZE + MAX_SESSION_NAME_WCHARS * 2) as u32;
        }

        // SAFETY: ControlTraceW with session_name_wide non-null + handle=0
        // is the documented "by name" form per Microsoft Learn.
        // windows-sys 0.59 wraps the trace handle in a newtype.
        // Value=0 plus a non-null session name is the documented
        // "by-name query" form per Microsoft Learn.
        let status = unsafe {
            ControlTraceW(
                CONTROLTRACE_HANDLE { Value: 0 },
                session_name_wide.as_ptr(),
                properties_ptr,
                EVENT_TRACE_CONTROL_QUERY,
            )
        };

        if status != 0 {
            return Err(status);
        }

        // SAFETY: status==0 means ControlTraceW succeeded and populated
        // EVENT_TRACE_PROPERTIES via the buffer pointer; EventsLost is
        // now valid.
        let events_lost = unsafe { (*properties_ptr).EventsLost };
        Ok(events_lost)
    }

    /// Stop a pre-existing ETW session by name. Returns `Ok(true)` if a
    /// session was stopped, `Ok(false)` if no session existed (clean state),
    /// `Err(win32_status)` on unexpected failure.
    ///
    /// Called before `start_and_process` to reclaim zombie sessions left
    /// by a prior crash or force-kill (ferrisetw 1.2 lacks `stop_if_exist`;
    /// same side-channel as `events_lost` per the Phase 0 spike pattern).
    pub fn reclaim_zombie(session_name: &str) -> Result<bool, u32> {
        let mut session_name_wide: Vec<u16> = OsStr::new(session_name).encode_wide().collect();
        session_name_wide.push(0);

        let mut buffer = vec![0u8; BUFFER_SIZE];
        let properties_ptr = buffer.as_mut_ptr() as *mut EVENT_TRACE_PROPERTIES;

        unsafe {
            let properties = &mut *properties_ptr;
            properties.Wnode = WNODE_HEADER {
                BufferSize: BUFFER_SIZE as u32,
                ..std::mem::zeroed()
            };
            properties.LoggerNameOffset = EVENT_TRACE_PROPERTIES_SIZE as u32;
            properties.LogFileNameOffset =
                (EVENT_TRACE_PROPERTIES_SIZE + MAX_SESSION_NAME_WCHARS * 2) as u32;
        }

        let status = unsafe {
            ControlTraceW(
                CONTROLTRACE_HANDLE { Value: 0 },
                session_name_wide.as_ptr(),
                properties_ptr,
                EVENT_TRACE_CONTROL_STOP,
            )
        };

        match status {
            0 => Ok(true),     // session existed and was stopped
            4201 => Ok(false), // ERROR_WMI_INSTANCE_NOT_FOUND — clean state
            other => Err(other),
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod stub_impl {
    /// Non-Windows stub. Always returns Err(0); the AC-009 test is
    /// cfg-gated to Windows so this branch is never invoked from a test.
    pub fn events_lost(_session_name: &str) -> Result<u32, u32> {
        Err(0)
    }

    /// Non-Windows stub. Always returns Ok(false) (no zombie to reclaim).
    pub fn reclaim_zombie(_session_name: &str) -> Result<bool, u32> {
        Ok(false)
    }
}

#[cfg(not(target_os = "windows"))]
pub use stub_impl::events_lost;
#[cfg(not(target_os = "windows"))]
pub use stub_impl::reclaim_zombie;
#[cfg(target_os = "windows")]
pub use windows_impl::events_lost;
#[cfg(target_os = "windows")]
pub use windows_impl::reclaim_zombie;
