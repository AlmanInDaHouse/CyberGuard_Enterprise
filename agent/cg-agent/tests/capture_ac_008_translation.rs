//! SPEC-017 capture_ac_008 — device path → Win32 translation.
//!
//! The pure translation maps a drive prefix, prefers the longest prefix,
//! respects the separator boundary, applies the UNC rule, and leaves an
//! unresolved path verbatim (SPEC-005 §Operational §3, SPEC-017
//! §Operational §5). The rendered event carries the translated
//! `image_file_name` and its basename as `process.name`. On Windows the
//! map built with `QueryDosDeviceW` resolves the system drive.

use cg_agent::cges::render_process_activity;
use cg_agent::etw::{ActivityId, CapturedEvent};
use cg_agent::paths::DevicePathMap;

fn map() -> DevicePathMap {
    DevicePathMap::from_pairs([
        ("\\Device\\HarddiskVolume3", "C:"),
        ("\\Device\\HarddiskVolume1", "D:"),
        ("\\Device\\HarddiskVolume3\\Mounted", "M:"),
    ])
}

#[test]
fn capture_ac_008_maps_a_drive_prefix() {
    assert_eq!(
        map().translate("\\Device\\HarddiskVolume3\\Windows\\System32\\cmd.exe"),
        "C:\\Windows\\System32\\cmd.exe"
    );
}

#[test]
fn capture_ac_008_prefers_the_longest_prefix() {
    assert_eq!(
        map().translate("\\Device\\HarddiskVolume3\\Mounted\\tool.exe"),
        "M:\\tool.exe"
    );
}

#[test]
fn capture_ac_008_respects_the_separator_boundary() {
    // HarddiskVolume1 must not match HarddiskVolume10.
    assert_eq!(
        map().translate("\\Device\\HarddiskVolume10\\x.exe"),
        "\\Device\\HarddiskVolume10\\x.exe"
    );
    assert_eq!(
        map().translate("\\Device\\HarddiskVolume1\\x.exe"),
        "D:\\x.exe"
    );
}

#[test]
fn capture_ac_008_applies_the_unc_rule() {
    assert_eq!(
        map().translate("\\Device\\Mup\\fileserver\\tools\\agent.exe"),
        "\\\\fileserver\\tools\\agent.exe"
    );
    assert_eq!(
        DevicePathMap::empty().translate("\\??\\UNC\\fileserver\\tools\\agent.exe"),
        "\\\\fileserver\\tools\\agent.exe"
    );
}

#[test]
fn capture_ac_008_leaves_an_unresolved_path_verbatim() {
    for path in [
        "\\Device\\HarddiskVolume9\\Junction\\target\\file.exe",
        "\\Device\\CdRom0\\setup.exe",
        "C:\\already\\win32.exe",
    ] {
        assert_eq!(map().translate(path), path);
    }
}

#[test]
fn capture_ac_008_prefixes_match_case_insensitively() {
    assert_eq!(
        map().translate("\\device\\harddiskvolume3\\Windows\\notepad.exe"),
        "C:\\Windows\\notepad.exe"
    );
}

#[test]
fn capture_ac_008_rendered_event_carries_the_win32_form() {
    let event = CapturedEvent {
        pid: 7144,
        event_id: "0192f0e0-0000-7000-8000-000000000001".to_string(),
        activity_id: ActivityId::Launch,
        image_file_name: "\\Device\\HarddiskVolume3\\Program Files\\App\\app.exe".to_string(),
        parent_pid: 4,
        command_line: String::new(),
        subject_user_sid: String::new(),
        etw_timestamp_nanos: 1_791_072_000_000_000_000,
        created_time_nanos: Some(1_791_072_000_000_000_000),
        exit_status: None,
    };

    let rendered = render_process_activity(&event, "01934abc-def0-7000-89ab-000000000099", &map());
    assert_eq!(
        rendered.process.image_file_name,
        "C:\\Program Files\\App\\app.exe"
    );
    assert_eq!(rendered.process.name, "app.exe");
}

#[cfg(windows)]
#[test]
fn capture_ac_008_system_map_resolves_the_system_drive() {
    let system_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
    let map = DevicePathMap::from_system();
    let (device, _) = map
        .entries()
        .iter()
        .find(|(_, drive)| drive.eq_ignore_ascii_case(&system_drive))
        .expect("QueryDosDeviceW maps the system drive");
    assert!(
        device.starts_with("\\Device\\"),
        "unexpected device {device}"
    );
    assert_eq!(
        map.translate(&format!("{device}\\Windows\\System32\\cmd.exe")),
        format!("{system_drive}\\Windows\\System32\\cmd.exe")
    );
}
