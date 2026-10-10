//! SPEC-020 auth_ac_006 — the accounts reported (§Operational §2).
//!
//! Through the decoding: a 4624 whose target is each system SID or SID
//! prefix, or a name ending in `$`, gives no event; a 4624 of a local
//! account, of an Entra ID account and of ANONYMOUS LOGON gives one; a 4625
//! gives one whatever its target, SYSTEM included; a record without
//! `TargetUserSid` is unusable.

mod logon_records;

use cg_agent::etw::EventRing;
use cg_agent::logon::{decode_logon, dispatch_logon_record, LogonCounters, LogonDecode};
use logon_records::{failure, success};

fn with_target(sid: &str, name: &str) -> cg_agent::logon::RawLogonRecord {
    let mut raw = success();
    raw.target_user_sid = Some(sid.to_string());
    raw.target_user_name = Some(name.to_string());
    raw
}

#[test]
fn auth_ac_006_system_and_virtual_accounts_are_not_reported() {
    for sid in [
        "S-1-5-18",
        "S-1-5-19",
        "S-1-5-20",
        "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464",
        "S-1-5-82-3006700770-424185619-1745488364-794895919-4004696415",
        "S-1-5-83-1-1111-2222-3333-4444",
        "S-1-5-84-0-0-0-0-0",
        "S-1-5-90-0-3",
        "S-1-5-96-0-3",
    ] {
        assert_eq!(
            decode_logon(&with_target(sid, "someone")),
            LogonDecode::Excluded,
            "{sid} must not be reported"
        );
    }
}

#[test]
fn auth_ac_006_a_computer_account_is_not_reported() {
    assert_eq!(
        decode_logon(&with_target("S-1-5-21-1111-2222-3333-1104", "WS-0042$")),
        LogonDecode::Excluded
    );
}

#[test]
fn auth_ac_006_people_and_anonymous_logon_are_reported() {
    for sid in [
        "S-1-5-21-1111-2222-3333-1001",
        "S-1-12-1-1111-2222-3333-4444",
        "S-1-5-7",
    ] {
        assert!(
            matches!(
                decode_logon(&with_target(sid, "someone")),
                LogonDecode::Report(_)
            ),
            "{sid} must be reported"
        );
    }
}

#[test]
fn auth_ac_006_every_failure_is_reported() {
    for sid in ["S-1-5-18", "S-1-5-90-0-1", "S-1-0-0"] {
        let mut raw = failure();
        raw.target_user_sid = Some(sid.to_string());
        raw.target_user_name = Some("SYSTEM$".to_string());
        assert!(
            matches!(decode_logon(&raw), LogonDecode::Report(_)),
            "a 4625 of {sid} must be reported"
        );
    }
}

#[test]
fn auth_ac_006_unusable_records_are_counted_excluded_ones_are_not() {
    let ring = EventRing::new(16);
    let counters = LogonCounters::new();

    let mut no_sid = success();
    no_sid.target_user_sid = None;
    let mut empty_sid = success();
    empty_sid.target_user_sid = Some(String::new());
    let mut other_id = success();
    other_id.event_id = 4634;
    for raw in [&no_sid, &empty_sid, &other_id] {
        dispatch_logon_record(raw, &ring, &counters);
    }
    dispatch_logon_record(&with_target("S-1-5-18", "SYSTEM"), &ring, &counters);

    assert!(ring.is_empty());
    assert_eq!(counters.unusable(), 3);
    assert_eq!(counters.first_unusable().unwrap().event_id, Some(4624));

    dispatch_logon_record(&success(), &ring, &counters);
    assert_eq!(ring.len(), 1);
    assert_eq!(counters.unusable(), 3);
}
