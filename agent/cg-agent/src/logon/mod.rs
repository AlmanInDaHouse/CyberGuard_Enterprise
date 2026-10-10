//! Logon telemetry (SPEC-020, ADR-0019): Security-log events 4624 and 4625
//! become CGES Authentication (3002) events.
//!
//! This module holds the platform-independent part: the values rendered
//! from one record ([`RawLogonRecord`]), the decoding that turns them into
//! a [`LogonEvent`] or drops them (SPEC-020 §Operational §2–§4), the
//! dispatch into the shared ring, and the counting of records the agent
//! cannot use (§Operational §1). The harness drives it on every platform.
//! On Windows, `subscription` produces the records.
//!
//! Nothing here logs a user, SID, domain, workstation or address
//! (§Operational §11): only event ids, Win32 codes and counts.

use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use uuid::Uuid;

use crate::etw::{filetime_to_unix_nanos, EventRing};

#[cfg(windows)]
mod subscription;
#[cfg(windows)]
pub use subscription::{query_recent, LogonSubscription};

/// An account successfully logged on.
pub const EVENT_LOGON_SUCCESS: u16 = 4624;
/// An account failed to log on.
pub const EVENT_LOGON_FAILURE: u16 = 4625;

/// What a withheld name or domain becomes (SPEC-020 §Operational §3). No
/// Windows account name can contain `<` or `>`.
pub const WITHHELD: &str = "<withheld>";

/// What the agent writes for a name, domain or package the event does not
/// have (SPEC-020 §Data contracts).
pub const NO_VALUE: &str = "-";

/// The failure codes that name an existing account (SPEC-020 §Operational
/// §3, ADR-0019 §5): wrong password, locked out, disabled, outside logon
/// hours, workstation not allowed, account expired, password expired,
/// password must change, logon type not granted.
pub const ACCOUNT_EXISTS_CODES: [u32; 9] = [
    0xC000_006A,
    0xC000_0234,
    0xC000_0072,
    0xC000_006F,
    0xC000_0070,
    0xC000_0193,
    0xC000_0071,
    0xC000_0224,
    0xC000_015B,
];

/// The values rendered from one Security-log record, by name (SPEC-020
/// §Operational §1). `None` is a value the record does not have.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawLogonRecord {
    pub event_id: u16,
    pub version: u8,
    /// `TimeCreated/@SystemTime` as a FILETIME (100 ns since 1601).
    pub filetime_100ns: i64,
    pub target_user_sid: Option<String>,
    pub target_user_name: Option<String>,
    pub target_domain_name: Option<String>,
    pub logon_type: Option<u32>,
    pub auth_package: Option<String>,
    pub workstation: Option<String>,
    pub ip_address: Option<String>,
    pub elevated_token: Option<String>,
    pub status: Option<u32>,
    pub sub_status: Option<u32>,
}

/// Whether the logon succeeded (4624) or failed (4625).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogonStatus {
    Success,
    Failure,
}

/// Where a logon came from: its address, and the workstation name when the
/// event gives one (ADR-0019 §4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogonSource {
    pub ip: IpAddr,
    pub hostname: Option<String>,
}

/// A logon as the agent reports it (SPEC-020 §Data contracts), decoded at
/// dispatch and rendered when its batch is formed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogonEvent {
    /// UUIDv7 generated when the record is decoded (ADR-0009 §1).
    pub event_id: String,
    pub status: LogonStatus,
    pub user_uid: String,
    /// The event's name, `-`, or [`WITHHELD`].
    pub user_name: String,
    /// The event's domain, `-`, or [`WITHHELD`].
    pub user_domain: String,
    pub logon_type_id: u8,
    pub auth_protocol: String,
    pub auth_protocol_id: u8,
    pub src: Option<LogonSource>,
    /// `Status`, failures only (0 when the event has none).
    pub status_code: Option<u32>,
    /// `SubStatus`, failures only (0 when the event has none).
    pub status_detail: Option<u32>,
    /// `ElevatedToken`, successes only, when the event carries it.
    pub elevated_token: Option<bool>,
    /// `TimeCreated` in Unix nanoseconds.
    pub timestamp_nanos: u64,
}

/// What the decoding makes of a record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogonDecode {
    /// An event to enqueue.
    Report(LogonEvent),
    /// A 4624 of an account that is not a person's (§Operational §2): not
    /// an event, not counted.
    Excluded,
    /// No `TargetUserSid`, or an event id other than 4624 and 4625:
    /// dropped and counted (§Operational §1).
    Unusable,
    /// A `TimeCreated` before 1970: logged at `error` and dropped.
    BeforeEpoch,
}

/// Decode one record (SPEC-020 §Operational §2–§4). A function of the
/// rendered values alone, apart from the `event_id` it generates.
pub fn decode_logon(raw: &RawLogonRecord) -> LogonDecode {
    let status = match raw.event_id {
        EVENT_LOGON_SUCCESS => LogonStatus::Success,
        EVENT_LOGON_FAILURE => LogonStatus::Failure,
        _ => return LogonDecode::Unusable,
    };
    let Some(sid) = raw.target_user_sid.as_deref().filter(|s| !s.is_empty()) else {
        return LogonDecode::Unusable;
    };
    let name = value_or_dash(raw.target_user_name.as_deref());
    if status == LogonStatus::Success && is_excluded_target(sid, &name) {
        return LogonDecode::Excluded;
    }
    let Some(timestamp_nanos) = filetime_to_unix_nanos(raw.filetime_100ns) else {
        return LogonDecode::BeforeEpoch;
    };
    let mut domain = value_or_dash(raw.target_domain_name.as_deref());
    let mut user_name = name;
    let (status_code, status_detail, elevated_token) = match status {
        LogonStatus::Success => (None, None, elevated(raw.elevated_token.as_deref())),
        LogonStatus::Failure => {
            if !name_is_kept(failure_code(raw.status, raw.sub_status)) {
                user_name = WITHHELD.to_string();
                domain = WITHHELD.to_string();
            }
            (
                Some(raw.status.unwrap_or(0)),
                Some(raw.sub_status.unwrap_or(0)),
                None,
            )
        }
    };
    let auth_protocol = value_or_dash(raw.auth_package.as_deref());
    LogonDecode::Report(LogonEvent {
        event_id: Uuid::now_v7().to_string(),
        status,
        user_uid: sid.to_string(),
        user_name,
        user_domain: domain,
        logon_type_id: logon_type_id(raw.logon_type),
        auth_protocol_id: auth_protocol_id(&auth_protocol),
        auth_protocol,
        src: source(raw.ip_address.as_deref(), raw.workstation.as_deref()),
        status_code,
        status_detail,
        elevated_token,
        timestamp_nanos,
    })
}

/// The value as written, or `-` when the record has none.
fn value_or_dash(value: Option<&str>) -> String {
    match value {
        Some(v) if !v.is_empty() => v.to_string(),
        _ => NO_VALUE.to_string(),
    }
}

/// Whether a 4624's target is not a person's account (SPEC-020
/// §Operational §2): a system or virtual account by SID, or a computer
/// account by its trailing `$`. ANONYMOUS LOGON (`S-1-5-7`) is reported.
pub fn is_excluded_target(sid: &str, name: &str) -> bool {
    const EXACT: [&str; 3] = ["S-1-5-18", "S-1-5-19", "S-1-5-20"];
    const PREFIXES: [&str; 6] = [
        "S-1-5-80-",
        "S-1-5-82-",
        "S-1-5-83-",
        "S-1-5-84-",
        "S-1-5-90-",
        "S-1-5-96-",
    ];
    EXACT.contains(&sid) || PREFIXES.iter().any(|p| sid.starts_with(p)) || name.ends_with('$')
}

/// The code a failed logon's name rule tests: `SubStatus`, or `Status`
/// when `SubStatus` is absent or zero (SPEC-020 §Operational §3).
pub fn failure_code(status: Option<u32>, sub_status: Option<u32>) -> u32 {
    match sub_status {
        Some(code) if code != 0 => code,
        _ => status.unwrap_or(0),
    }
}

/// Whether a failed logon keeps its name and domain: only when the code
/// names an existing account (ADR-0019 §5).
pub fn name_is_kept(code: u32) -> bool {
    ACCOUNT_EXISTS_CODES.contains(&code)
}

/// OCSF `logon_type_id` from Windows `LogonType` (SPEC-020 §Operational
/// §4): 0 is System (1); 2–5 and 7–13 are the same numbers; anything
/// else, or none, is Other (99).
pub fn logon_type_id(logon_type: Option<u32>) -> u8 {
    match logon_type {
        Some(0) => 1,
        Some(t @ (2..=5 | 7..=13)) => t as u8,
        _ => 99,
    }
}

/// OCSF `auth_protocol_id` from the authentication package (SPEC-020
/// §Operational §4): NTLM 1, Kerberos 2, `-` 0 (Unknown), anything else —
/// `Negotiate` among them — 99 (Other).
pub fn auth_protocol_id(package: &str) -> u8 {
    if package == NO_VALUE {
        0
    } else if package.eq_ignore_ascii_case("NTLM") {
        1
    } else if package.eq_ignore_ascii_case("Kerberos") {
        2
    } else {
        99
    }
}

/// The logon's source (SPEC-020 §Operational §4): present when
/// `IpAddress`, without any zone suffix, is an address; an IPv4-mapped
/// IPv6 address becomes IPv4. `hostname` when the workstation name is
/// neither `-` nor empty.
pub fn source(ip_address: Option<&str>, workstation: Option<&str>) -> Option<LogonSource> {
    let text = ip_address?;
    let text = text.split('%').next().unwrap_or(text);
    let ip: IpAddr = text.parse().ok()?;
    let ip = match ip {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => IpAddr::V6(v6),
        },
        v4 => v4,
    };
    let hostname = workstation
        .filter(|w| !w.is_empty() && *w != NO_VALUE)
        .map(str::to_string);
    Some(LogonSource { ip, hostname })
}

/// `ElevatedToken`: `%%1842` is yes, `%%1843` no; anything else, or none,
/// is unknown (SPEC-020 §Operational §4).
pub fn elevated(token: Option<&str>) -> Option<bool> {
    match token {
        Some("%%1842") => Some(true),
        Some("%%1843") => Some(false),
        _ => None,
    }
}

/// A Windows status code as the wire writes it: lowercase hexadecimal with
/// the `0x` prefix, `0x0` for zero (SPEC-020 §Data contracts).
pub fn hex_code(code: u32) -> String {
    format!("{code:#x}")
}

/// One unusable record kept for the log: its event id when it is known,
/// the Win32 code when there is one. Nothing else of the record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnusableSample {
    pub event_id: Option<u16>,
    pub win32_code: Option<u32>,
}

/// The count of records the agent could not use (SPEC-020 §Operational
/// §1), and the first of them.
#[derive(Debug, Default)]
pub struct LogonCounters {
    inner: Mutex<(u64, Option<UnusableSample>)>,
}

impl LogonCounters {
    pub fn new() -> Self {
        Self::default()
    }

    /// Count one unusable record; the first is kept.
    pub fn record_unusable(&self, sample: UnusableSample) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.0 += 1;
        inner.1.get_or_insert(sample);
    }

    /// Unusable records so far.
    pub fn unusable(&self) -> u64 {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).0
    }

    /// The first unusable record, if any.
    pub fn first_unusable(&self) -> Option<UnusableSample> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).1
    }
}

/// Logs an increase of the unusable count at `warn`, at most once per 60 s
/// (SPEC-020 §Operational §1).
#[derive(Debug, Default)]
pub struct UnusableMonitor {
    logged_total: u64,
    last_log: Option<Instant>,
}

/// The least time between two `warn` lines of one monitor.
pub const LOG_INTERVAL: Duration = Duration::from_secs(60);

impl UnusableMonitor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check `counters` at `now`; log and return the increase since the
    /// last line when there is one and the interval has passed, else 0.
    pub fn observe(&mut self, counters: &LogonCounters, now: Instant) -> u64 {
        let total = counters.unusable();
        let delta = total.saturating_sub(self.logged_total);
        let due = self
            .last_log
            .is_none_or(|last| now.duration_since(last) >= LOG_INTERVAL);
        if delta == 0 || !due {
            return 0;
        }
        let first = counters.first_unusable();
        tracing::warn!(
            target: "cg_agent::logon",
            logon_records_unusable = total,
            delta_since_last_line = delta,
            first_event_id = first.and_then(|s| s.event_id),
            first_win32_code = first.and_then(|s| s.win32_code),
            "Security-log records the agent could not use",
        );
        self.logged_total = total;
        self.last_log = Some(now);
        delta
    }
}

/// Decode one record and act on it (SPEC-020 §Operational §1–§5): enqueue
/// an event, drop an excluded logon silently, count an unusable record,
/// log a pre-1970 record at `error` by its event id alone.
pub fn dispatch_logon_record(raw: &RawLogonRecord, ring: &EventRing, counters: &LogonCounters) {
    match decode_logon(raw) {
        LogonDecode::Report(event) => ring.enqueue_or_drop(event),
        LogonDecode::Excluded => {}
        LogonDecode::Unusable => counters.record_unusable(UnusableSample {
            event_id: Some(raw.event_id),
            win32_code: None,
        }),
        LogonDecode::BeforeEpoch => tracing::error!(
            target: "cg_agent::logon",
            event_id = raw.event_id,
            reason = "timestamp_before_unix_epoch",
            "logon record with a pre-1970 timestamp dropped",
        ),
    }
}
