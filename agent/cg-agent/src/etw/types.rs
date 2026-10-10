//! In-memory captured-event shapes + activity discriminants + raw ETW
//! open-error variants.
//!
//! `CapturedEvent` is the post-capture pre-emission representation of a
//! Kernel-Process Launch or Terminate event; `NetworkEvent` is that of a
//! Kernel-Network TCP connection opened (SPEC-019). Both are produced by
//! the dispatch logic (`dispatch.rs`, fed by session.rs on Windows),
//! traverse the bounded ring buffer (ring.rs) as a `RingEvent`, and are
//! rendered to CGES JSON once, when their batch is formed (cges/emit.rs).
//!
//! The structs are `Clone` because the ring buffer's snapshot_events
//! accessor (test API) needs to return owned copies.

use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

/// Discriminator between Launch (activity_id=1) and Terminate
/// (activity_id=2) Kernel-Process events. The integer values match the
/// CGES wire format per ADR-0011 §3 and are exposed via `as u64` cast
/// where serialization needs an integer. Deserialize via `TryFrom<u64>`
/// validates unknown discriminants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "u64", try_from = "u64")]
pub enum ActivityId {
    Launch = 1,
    Terminate = 2,
}

impl From<ActivityId> for u64 {
    fn from(activity_id: ActivityId) -> u64 {
        activity_id as u64
    }
}

impl TryFrom<u64> for ActivityId {
    type Error = String;
    fn try_from(value: u64) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(ActivityId::Launch),
            2 => Ok(ActivityId::Terminate),
            n => Err(format!("invalid activity_id: {n}")),
        }
    }
}

/// Post-capture in-memory event representation.
///
/// Fields:
/// - `pid`: PID assigned by Windows at process creation.
/// - `activity_id`: Launch or Terminate.
/// - `image_file_name`: ETW `ImageName` as captured — on Launch the
///   NT-style device path (`\Device\HarddiskVolumeN\...`), translated to
///   Win32 form when the event is rendered (`paths.rs`, SPEC-017
///   §Operational §5); on Terminate only the image's base name, which
///   has nothing to translate and is emitted as is.
/// - `parent_pid`: kernel `ParentProcessID`; emitted with `name` absent
///   when the parent is unresolvable per ADR-0011 §5 + AC-007.
/// - `command_line`: ETW `CommandLine` field (empty for Terminate).
/// - `subject_user_sid`: SID string form (e.g., `S-1-5-18`).
/// - `etw_timestamp_nanos`: the event's ETW timestamp
///   (FILETIME-converted to UTC nanoseconds at capture per
///   SPEC-005 §Operational §1).
/// - `created_time_nanos`: the process creation time, resolved at
///   dispatch: the event's own timestamp for Launch; for Terminate, the
///   cached Launch timestamp, or `None` on a cache miss (SPEC-005
///   §Operational §2, SPEC-017 §Operational §6).
/// - `exit_status`: ETW `ExitStatus` for Terminate events; `None` for
///   Launch (serde-skipped at emit time per AC-005).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedEvent {
    pub pid: u32,
    /// Agent-generated UUIDv7, unique per captured event, generated at
    /// capture (ADR-0009 §1). Retries reuse the same event_id for dedup
    /// at the ClickHouse ReplacingMergeTree merge stage. Per SPEC-005
    /// §AC AC-001 the event_id is part of the persisted row and
    /// round-trips agent → envelope → ingest → cges_events column.
    pub event_id: String,
    pub activity_id: ActivityId,
    pub image_file_name: String,
    pub parent_pid: u32,
    pub command_line: String,
    pub subject_user_sid: String,
    pub etw_timestamp_nanos: u64,
    pub created_time_nanos: Option<u64>,
    pub exit_status: Option<i32>,
}

/// The host's point of view on a TCP connection (ADR-0018 §4): it opened
/// it (`outbound`) or accepted it (`inbound`). Serialized as the CGES
/// `connection_info.direction` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Outbound,
    Inbound,
}

/// Post-capture in-memory representation of a TCP connection opened,
/// reported by Kernel-Network (SPEC-019, ADR-0018 §2–§5).
///
/// Fields:
/// - `event_id`: agent-generated UUIDv7, generated at capture (ADR-0009 §1).
/// - `pid`: the provider's `PID`, the local process the connection
///   belongs to (ADR-0018 §5).
/// - `direction`: outbound (the host connected) or inbound (it accepted).
/// - `src` / `dst`: the initiator's and the acceptor's address and port
///   (ADR-0018 §4); an IPv4-mapped IPv6 address is already its IPv4.
/// - `etw_timestamp_nanos`: the event's ETW timestamp in UTC nanoseconds
///   (SPEC-005 §Operational §1).
/// - `created_time_nanos`: the creation time the agent held for `pid` at
///   dispatch, from which `actor.process.uid` is built; `None` when it
///   held none, and the event then carries no uid (SPEC-019 §Operational
///   §3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkEvent {
    pub event_id: String,
    pub pid: u32,
    pub direction: Direction,
    pub src: SocketAddr,
    pub dst: SocketAddr,
    pub etw_timestamp_nanos: u64,
    pub created_time_nanos: Option<u64>,
}

/// One event in the ring: a process, network or logon event (SPEC-019
/// §Operational §5, SPEC-020 §Operational §5). Events of the three classes
/// share one ring and leave it in the order they entered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RingEvent {
    Process(CapturedEvent),
    Network(NetworkEvent),
    Logon(crate::logon::LogonEvent),
}

impl RingEvent {
    /// The process event, if this is one.
    pub fn as_process(&self) -> Option<&CapturedEvent> {
        match self {
            RingEvent::Process(event) => Some(event),
            _ => None,
        }
    }

    /// The process event, if this is one.
    pub fn into_process(self) -> Option<CapturedEvent> {
        match self {
            RingEvent::Process(event) => Some(event),
            _ => None,
        }
    }

    /// The network event, if this is one.
    pub fn as_network(&self) -> Option<&NetworkEvent> {
        match self {
            RingEvent::Network(event) => Some(event),
            _ => None,
        }
    }

    /// The logon event, if this is one.
    pub fn as_logon(&self) -> Option<&crate::logon::LogonEvent> {
        match self {
            RingEvent::Logon(event) => Some(event),
            _ => None,
        }
    }
}

impl From<CapturedEvent> for RingEvent {
    fn from(event: CapturedEvent) -> Self {
        RingEvent::Process(event)
    }
}

impl From<NetworkEvent> for RingEvent {
    fn from(event: NetworkEvent) -> Self {
        RingEvent::Network(event)
    }
}

impl From<crate::logon::LogonEvent> for RingEvent {
    fn from(event: crate::logon::LogonEvent) -> Self {
        RingEvent::Logon(event)
    }
}

/// Why an ETW session did not start (SPEC-017 §Operational §1, ADR-0010
/// §Decision part 1).
///
/// `EtwSession::open` returns one of these only after the session start
/// has actually run and failed. The agent-level error domain wraps it
/// via `EtwError` in `errors.rs`; `handle_etw_open_result` in
/// `startup.rs` maps it to a startup action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    /// Win32 `ERROR_PRIVILEGE_NOT_HELD` (1314). The process is not
    /// running with the SeSystemProfilePrivilege or equivalent required
    /// to open the Kernel-Process provider.
    PrivilegeNotHeld,
    /// Win32 `ERROR_ACCESS_DENIED` (5). What an unelevated process gets
    /// from `StartTraceW`.
    AccessDenied,
    /// Any other start failure: the Win32 code and its system message.
    Failed { code: u32, message: String },
    /// This build has no capture backend (non-Windows platforms).
    Unsupported,
}

impl OpenError {
    /// Classify the Win32 error code a session start failed with.
    pub fn from_win32(code: u32) -> Self {
        match code {
            5 => OpenError::AccessDenied,
            1314 => OpenError::PrivilegeNotHeld,
            other => OpenError::Failed {
                code: other,
                message: win32_message(other),
            },
        }
    }

    /// True for the two privilege failures (exit code 9).
    pub fn is_privilege(&self) -> bool {
        matches!(self, OpenError::PrivilegeNotHeld | OpenError::AccessDenied)
    }
}

/// The Win32 error code inside a raw OS error value. ferrisetw reports
/// `StartTraceW` / `EnableTraceEx2` failures as `HRESULT_FROM_WIN32`
/// values (`0x8007xxxx`) carried in an `io::Error`; a plain Win32 code
/// passes through unchanged.
pub fn win32_from_os_error(raw: i32) -> u32 {
    let value = raw as u32;
    if value & 0xFFFF_0000 == 0x8007_0000 {
        value & 0xFFFF
    } else {
        value
    }
}

/// The system message for a Win32 code, without the `(os error N)`
/// suffix the standard library appends.
pub(crate) fn win32_message(code: u32) -> String {
    let text = std::io::Error::from_raw_os_error(code as i32).to_string();
    let suffix = format!(" (os error {code})");
    text.strip_suffix(&suffix).unwrap_or(&text).to_string()
}
