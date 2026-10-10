//! Synthetic Security-log records for the SPEC-020 logon tests, as the
//! subscription renders them (`RawLogonRecord`). No user data of a real
//! host: every name and SID here is made up.

#![allow(dead_code)]

use cg_agent::logon::{decode_logon, LogonDecode, LogonEvent, RawLogonRecord};

/// 2026-10-10T00:00:00Z as FILETIME (100 ns ticks since 1601).
pub const FILETIME_BASE: i64 = 116_444_736_000_000_000 + 17_915_904_000_000_000;

/// A 4624 of a local account, network logon over NTLM from 192.0.2.10.
pub fn success() -> RawLogonRecord {
    RawLogonRecord {
        event_id: 4624,
        version: 3,
        filetime_100ns: FILETIME_BASE,
        target_user_sid: Some("S-1-5-21-1111-2222-3333-1001".to_string()),
        target_user_name: Some("logon-test-user".to_string()),
        target_domain_name: Some("WS-0042".to_string()),
        logon_type: Some(3),
        auth_package: Some("NTLM".to_string()),
        workstation: Some("WS-0099".to_string()),
        ip_address: Some("192.0.2.10".to_string()),
        elevated_token: Some("%%1843".to_string()),
        status: None,
        sub_status: None,
    }
}

/// A 4625 for a name that does not exist (`0xC000006D` / `0xC0000064`).
pub fn failure() -> RawLogonRecord {
    RawLogonRecord {
        event_id: 4625,
        version: 0,
        target_user_sid: Some("S-1-0-0".to_string()),
        target_user_name: Some("typed-in-name".to_string()),
        target_domain_name: Some("WS-0042".to_string()),
        elevated_token: None,
        status: Some(0xC000_006D),
        sub_status: Some(0xC000_0064),
        ..success()
    }
}

/// The event a record decodes to; panics when it is not reported.
pub fn reported(raw: &RawLogonRecord) -> LogonEvent {
    match decode_logon(raw) {
        LogonDecode::Report(event) => event,
        other => panic!("expected an event, got {other:?}"),
    }
}
