//! SPEC-020 auth_ac_008 — the mapping (§Operational §4).
//!
//! The `logon_type_id` and `auth_protocol_id` tables; `src_endpoint`
//! absent for `IpAddress` `-`, present for IPv4, IPv6, a zoned link-local
//! address without its zone and an IPv4-mapped address written as IPv4;
//! `hostname` absent for `-` and empty; `cg_elevated_token` from `%%1842`
//! and `%%1843`, absent for another value and on a failure; `user.domain`
//! and `auth_protocol` `-` when the event has no value; a UUIDv7
//! `event_id`; the converted `time`; a record before 1970 dropped.

mod logon_records;

use cg_agent::cges::render_authentication;
use cg_agent::etw::filetime_to_unix_nanos;
use cg_agent::logon::{auth_protocol_id, decode_logon, logon_type_id, LogonDecode, RawLogonRecord};
use logon_records::{failure, reported, success, FILETIME_BASE};
use serde_json::{json, Value};

fn element(raw: &RawLogonRecord) -> Value {
    serde_json::to_value(render_authentication(&reported(raw))).unwrap()
}

#[test]
fn auth_ac_008_logon_type_table() {
    assert_eq!(logon_type_id(Some(0)), 1);
    for t in [2u32, 3, 4, 5, 7, 8, 9, 10, 11, 12, 13] {
        assert_eq!(u32::from(logon_type_id(Some(t))), t);
    }
    for t in [Some(1), Some(6), Some(14), Some(4_000_000_000), None] {
        assert_eq!(logon_type_id(t), 99, "{t:?}");
    }
}

#[test]
fn auth_ac_008_auth_protocol_table() {
    assert_eq!(auth_protocol_id("NTLM"), 1);
    assert_eq!(auth_protocol_id("ntlm"), 1);
    assert_eq!(auth_protocol_id("Kerberos"), 2);
    assert_eq!(auth_protocol_id("kerberos"), 2);
    assert_eq!(auth_protocol_id("-"), 0);
    assert_eq!(auth_protocol_id("Negotiate"), 99);
    assert_eq!(auth_protocol_id("CloudAP"), 99);
}

#[test]
fn auth_ac_008_the_source() {
    let with_ip = |ip: &str, ws: Option<&str>| RawLogonRecord {
        ip_address: Some(ip.to_string()),
        workstation: ws.map(str::to_string),
        ..success()
    };
    assert!(element(&with_ip("-", Some("WS-0099")))
        .get("src_endpoint")
        .is_none());
    assert!(element(&RawLogonRecord {
        ip_address: None,
        ..success()
    })
    .get("src_endpoint")
    .is_none());
    assert_eq!(
        element(&with_ip("192.0.2.10", Some("WS-0099")))["src_endpoint"],
        json!({"ip": "192.0.2.10", "hostname": "WS-0099"})
    );
    assert_eq!(
        element(&with_ip("2001:db8::7", Some("-")))["src_endpoint"],
        json!({"ip": "2001:db8::7"})
    );
    assert_eq!(
        element(&with_ip("fe80::1%12", Some("")))["src_endpoint"],
        json!({"ip": "fe80::1"})
    );
    assert_eq!(
        element(&with_ip("::ffff:192.0.2.10", None))["src_endpoint"],
        json!({"ip": "192.0.2.10"})
    );
}

#[test]
fn auth_ac_008_the_elevated_token() {
    let with_token = |token: Option<&str>| RawLogonRecord {
        elevated_token: token.map(str::to_string),
        ..success()
    };
    assert_eq!(
        element(&with_token(Some("%%1842")))["cg_elevated_token"],
        true
    );
    assert_eq!(
        element(&with_token(Some("%%1843")))["cg_elevated_token"],
        false
    );
    for token in [Some("%%9999"), Some("Yes"), None] {
        assert!(element(&with_token(token))
            .get("cg_elevated_token")
            .is_none());
    }
    let mut failed = failure();
    failed.elevated_token = Some("%%1842".to_string());
    assert!(element(&failed).get("cg_elevated_token").is_none());
}

#[test]
fn auth_ac_008_a_missing_domain_or_package_is_a_dash() {
    let raw = RawLogonRecord {
        target_domain_name: None,
        auth_package: Some(String::new()),
        ..success()
    };
    let e = element(&raw);
    assert_eq!(e["user"]["domain"], "-");
    assert_eq!(e["auth_protocol"], "-");
    assert_eq!(e["auth_protocol_id"], 0);
}

#[test]
fn auth_ac_008_the_success_element() {
    let event = reported(&success());
    let id = uuid::Uuid::parse_str(&event.event_id).expect("event_id is a UUID");
    assert_eq!(id.get_version_num(), 7);
    let nanos = filetime_to_unix_nanos(FILETIME_BASE).unwrap();
    assert_eq!(
        serde_json::to_value(render_authentication(&event)).unwrap(),
        json!({
            "event_id": event.event_id,
            "class_uid": 3002,
            "category_uid": 3,
            "activity_id": 1,
            "time": nanos.to_string(),
            "status_id": 1,
            "user": {"uid": "S-1-5-21-1111-2222-3333-1001", "name": "logon-test-user", "domain": "WS-0042"},
            "logon_type_id": 3,
            "auth_protocol": "NTLM",
            "auth_protocol_id": 1,
            "src_endpoint": {"ip": "192.0.2.10", "hostname": "WS-0099"},
            "cg_elevated_token": false,
        })
    );
}

#[test]
fn auth_ac_008_a_record_before_1970_is_dropped() {
    let raw = RawLogonRecord {
        filetime_100ns: 0,
        ..success()
    };
    assert_eq!(decode_logon(&raw), LogonDecode::BeforeEpoch);
}
