//! SPEC-020 auth_ac_007 — the name of a failed logon (§Operational §3).
//!
//! Each code that names an existing account, as `SubStatus` in upper or
//! lower case, keeps the name and the domain; a code given as `Status`
//! with `SubStatus` zero or absent is tested the same way; `0xC0000064`
//! and a code outside the list give `<withheld>` for both, also when the
//! event wrote `-`, and the rendered element holds neither the submitted
//! name nor its length; the codes are written in lowercase hexadecimal.

mod logon_records;

use cg_agent::cges::render_authentication;
use cg_agent::logon::{RawLogonRecord, ACCOUNT_EXISTS_CODES, WITHHELD};
use logon_records::{failure, reported};
use std::collections::BTreeSet;

const SUBMITTED: &str = "Pa55w0rd-typed-as-a-name!";

fn failed(status: Option<u32>, sub_status: Option<u32>) -> RawLogonRecord {
    RawLogonRecord {
        target_user_name: Some(SUBMITTED.to_string()),
        status,
        sub_status,
        ..failure()
    }
}

#[test]
fn auth_ac_007_codes_that_name_an_account_keep_the_name() {
    for code in ACCOUNT_EXISTS_CODES {
        let event = reported(&failed(Some(0xC000_006D), Some(code)));
        assert_eq!(event.user_name, SUBMITTED, "{code:#x} keeps the name");
        assert_eq!(event.user_domain, "WS-0042", "{code:#x} keeps the domain");
    }
}

#[test]
fn auth_ac_007_the_comparison_ignores_the_case_of_the_hex_text() {
    // The codes are numbers once rendered; upper and lower case are the
    // same number, and the wire writes lowercase.
    let upper = u32::from_str_radix("C000006A", 16).unwrap();
    let lower = u32::from_str_radix("c000006a", 16).unwrap();
    for code in [upper, lower] {
        let event = reported(&failed(Some(0xC000_006D), Some(code)));
        assert_eq!(event.user_name, SUBMITTED);
    }
}

#[test]
fn auth_ac_007_status_is_tested_when_substatus_is_zero_or_absent() {
    for sub_status in [Some(0), None] {
        let kept = reported(&failed(Some(0xC000_0234), sub_status));
        assert_eq!(kept.user_name, SUBMITTED);
        let withheld = reported(&failed(Some(0xC000_0064), sub_status));
        assert_eq!(withheld.user_name, WITHHELD);
    }
}

#[test]
fn auth_ac_007_no_such_user_and_unknown_codes_withhold_name_and_domain() {
    for sub_status in [0xC000_0064, 0xC000_0133, 0x1234_5678] {
        let event = reported(&failed(Some(0xC000_006D), Some(sub_status)));
        assert_eq!(event.user_name, WITHHELD);
        assert_eq!(event.user_domain, WITHHELD);

        let element = serde_json::to_value(render_authentication(&event)).unwrap();
        assert!(
            !element.to_string().contains(SUBMITTED),
            "the name must not travel"
        );
        // Nor its length: the element has exactly the members of a failure,
        // and the user exactly its three.
        let keys = |v: &serde_json::Value| -> BTreeSet<String> {
            v.as_object().unwrap().keys().cloned().collect()
        };
        assert_eq!(
            keys(&element),
            [
                "event_id",
                "class_uid",
                "category_uid",
                "activity_id",
                "time",
                "status_id",
                "user",
                "logon_type_id",
                "auth_protocol",
                "auth_protocol_id",
                "src_endpoint",
                "status_code",
                "status_detail",
            ]
            .into_iter()
            .map(String::from)
            .collect()
        );
        assert_eq!(
            keys(&element["user"]),
            ["uid", "name", "domain"]
                .into_iter()
                .map(String::from)
                .collect()
        );
    }
}

#[test]
fn auth_ac_007_a_dash_is_withheld_too() {
    let mut raw = failed(Some(0xC000_006D), Some(0xC000_0064));
    raw.target_user_name = Some("-".to_string());
    raw.target_domain_name = None;
    let event = reported(&raw);
    assert_eq!(event.user_name, WITHHELD);
    assert_eq!(event.user_domain, WITHHELD);
}

#[test]
fn auth_ac_007_the_codes_are_lowercase_hexadecimal() {
    let event = reported(&failed(Some(0xC000_006D), Some(0xC000_0064)));
    let element = serde_json::to_value(render_authentication(&event)).unwrap();
    assert_eq!(element["status_code"], "0xc000006d");
    assert_eq!(element["status_detail"], "0xc0000064");

    let zero = reported(&failed(Some(0xC000_015B), None));
    let element = serde_json::to_value(render_authentication(&zero)).unwrap();
    assert_eq!(element["status_detail"], "0x0");
}
