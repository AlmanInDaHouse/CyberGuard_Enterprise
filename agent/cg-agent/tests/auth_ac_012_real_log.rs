//! SPEC-020 auth_ac_012 — logons on the real Security log (elevated gate).
//!
//! The test opens the logon subscription itself, then:
//! - refuses two network logons with `LogonUserW`, each with a random
//!   password: one for a random name no account has, one for the built-in
//!   Administrator (the local account whose SID is the machine's SID with
//!   RID 500, its name looked up). The ring then holds a failure whose
//!   name and domain are `<withheld>` and whose `SubStatus` is
//!   `0xc0000064`, and a failure whose name is kept and whose code is in
//!   the list of §Operational §3;
//! - removes any connection to `\\127.0.0.1`, runs `net use
//!   \\127.0.0.1\IPC$` with the current credentials and removes it. The
//!   ring then holds a success of the current user's SID, with a name and
//!   a domain that are not withheld, logon type 3 and the elevated token;
//! - reads the last 200 records 4624 of the log through the same
//!   rendering: none whose target §Operational §2 excludes decodes to an
//!   event, and at least one such record is among them.
//!
//! It prints only redacted data (§Operational §11): SIDs as prefix and
//! RID, codes, timings and whether a source was present — never a name.
//! Real Security log, elevated: run with
//! `cargo test -p cg-agent -- --ignored --test-threads=1 --show-output`.

#[cfg(windows)]
#[ignore = "real Security log, elevated gate: cargo test -p cg-agent -- --ignored --test-threads=1"]
#[test]
fn auth_ac_012_logons_on_the_real_log() {
    use cg_agent::etw::EventRing;
    use cg_agent::logon::{
        decode_logon, is_excluded_target, name_is_kept, query_recent, LogonDecode, LogonEvent,
        LogonStatus, LogonSubscription, WITHHELD,
    };
    use std::process::Command;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    /// A SID as the gate may print it: prefix and RID.
    fn redact(sid: &str) -> String {
        match sid.strip_prefix("S-1-5-21-") {
            Some(rest) => format!("S-1-5-21-...-{}", rest.rsplit('-').next().unwrap_or("?")),
            None => sid.to_string(),
        }
    }

    let ring = Arc::new(EventRing::new(65536));
    let mut subscription = LogonSubscription::open(Arc::clone(&ring))
        .expect("auth_ac_012: open the Security log subscription (elevated?)");
    std::thread::sleep(Duration::from_millis(500));
    let start = Instant::now();

    // The two refused logons.
    let random = uuid::Uuid::now_v7().simple().to_string();
    let missing_name = format!("cg-nouser-{}", &random[20..]);
    let password = format!("Cg-{random}!");
    assert!(
        windows::logon_network(&missing_name, &password).is_err(),
        "a logon for a name no account has must fail"
    );
    let admin = windows::builtin_administrator().expect("look up the RID-500 account");
    assert!(
        windows::logon_network(&admin, &password).is_err(),
        "a logon with a random password must fail"
    );
    let failures_sent = start.elapsed();

    // The successful network logon to the loopback.
    let _ = Command::new("net")
        .args(["use", r"\\127.0.0.1\IPC$", "/delete", "/y"])
        .output();
    let net = Command::new("net")
        .args(["use", r"\\127.0.0.1\IPC$"])
        .output()
        .expect("run net.exe");
    let _ = Command::new("net")
        .args(["use", r"\\127.0.0.1\IPC$", "/delete", "/y"])
        .output();
    assert!(
        net.status.success(),
        "net use \\\\127.0.0.1\\IPC$ must succeed (is the Server service running?)"
    );
    let current_sid = windows::current_user_sid();

    let logons = |ring: &EventRing| -> Vec<LogonEvent> {
        ring.snapshot_events()
            .iter()
            .filter_map(|e| e.as_logon().cloned())
            .collect()
    };
    let withheld = |e: &LogonEvent| {
        e.status == LogonStatus::Failure
            && e.user_name == WITHHELD
            && e.user_domain == WITHHELD
            && e.status_detail == Some(0xC000_0064)
    };
    let kept = |e: &LogonEvent| {
        e.status == LogonStatus::Failure
            && e.user_name != WITHHELD
            && name_is_kept(cg_agent::logon::failure_code(
                e.status_code,
                e.status_detail,
            ))
    };
    let success = |e: &LogonEvent| {
        e.status == LogonStatus::Success
            && e.user_uid == current_sid
            && e.logon_type_id == 3
            && !e.user_name.is_empty()
            && e.user_name != WITHHELD
            && !e.user_domain.is_empty()
            && e.user_domain != WITHHELD
            && e.elevated_token.is_some()
    };
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut seen = logons(&ring);
    let mut found = [None::<Duration>; 3];
    while Instant::now() < deadline && found.iter().any(Option::is_none) {
        seen = logons(&ring);
        for (slot, test) in
            found
                .iter_mut()
                .zip([&withheld as &dyn Fn(&LogonEvent) -> bool, &kept, &success])
        {
            if slot.is_none() && seen.iter().any(test) {
                *slot = Some(start.elapsed());
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    subscription.stop();

    // The one-off query of the log's own recent records.
    let recent = query_recent(200).expect("query the last 4624 records");
    let excluded_targets = recent
        .iter()
        .filter(|r| {
            is_excluded_target(
                r.target_user_sid.as_deref().unwrap_or(""),
                r.target_user_name.as_deref().unwrap_or(""),
            )
        })
        .count();
    let leaked = recent
        .iter()
        .filter(|r| {
            is_excluded_target(
                r.target_user_sid.as_deref().unwrap_or(""),
                r.target_user_name.as_deref().unwrap_or(""),
            ) && matches!(decode_logon(r), LogonDecode::Report(_))
        })
        .count();

    // Redacted report (ADR-0019 §10; the handoff records it).
    for e in seen.iter().filter(|e| e.status == LogonStatus::Failure) {
        println!(
            "auth_ac_012 failure uid={} status={:#x} sub_status={:#x} name_withheld={} type={}",
            redact(&e.user_uid),
            e.status_code.unwrap_or(0),
            e.status_detail.unwrap_or(0),
            e.user_name == WITHHELD,
            e.logon_type_id
        );
    }
    if let Some(e) = seen.iter().find(|e| success(e)) {
        println!(
            "auth_ac_012 success uid={} type={} package={} source_present={} elevated={:?}",
            redact(&e.user_uid),
            e.logon_type_id,
            e.auth_protocol,
            e.src.is_some(),
            e.elevated_token
        );
    }
    println!(
        "auth_ac_012 delays failures_sent_at={failures_sent:?} withheld={:?} kept={:?} success={:?}",
        found[0], found[1], found[2]
    );
    println!(
        "auth_ac_012 recent_4624={} excluded_targets={excluded_targets} excluded_reported={leaked}",
        recent.len()
    );

    let mut failures = Vec::new();
    if found[0].is_none() {
        failures.push("no failure with <withheld> name and domain and 0xc0000064");
    }
    if found[1].is_none() {
        failures.push("no failure with a kept name and an account-exists code");
    }
    if found[2].is_none() {
        failures.push("no success of the current user, type 3, with the elevated token");
    }
    if excluded_targets == 0 {
        failures.push("the last 200 records 4624 hold no excluded target: the check is vacuous");
    }
    if leaked != 0 {
        failures.push("a record of an excluded target decoded to an event");
    }
    assert!(
        failures.is_empty(),
        "auth_ac_012 failed: {failures:?}; {} logon events in the ring",
        seen.len()
    );
}

#[cfg(windows)]
mod windows {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, LocalFree, HANDLE};
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSidToSidW,
    };
    use windows_sys::Win32::Security::{
        LogonUserW, LookupAccountNameW, LookupAccountSidW, LOGON32_LOGON_NETWORK,
        LOGON32_PROVIDER_DEFAULT, SID_NAME_USE,
    };

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// A network logon of `user` on this machine; `Err` with the Win32
    /// code when it is refused.
    pub fn logon_network(user: &str, password: &str) -> Result<(), u32> {
        let (user, domain, password) = (wide(user), wide("."), wide(password));
        let mut token: HANDLE = std::ptr::null_mut();
        // SAFETY: the strings live for the call; `token` receives a handle.
        let ok = unsafe {
            LogonUserW(
                user.as_ptr(),
                domain.as_ptr(),
                password.as_ptr(),
                LOGON32_LOGON_NETWORK,
                LOGON32_PROVIDER_DEFAULT,
                &mut token,
            )
        };
        if ok == 0 {
            // SAFETY: reads this thread's last error.
            return Err(unsafe { GetLastError() });
        }
        // SAFETY: the token came from LogonUserW.
        unsafe { CloseHandle(token) };
        Ok(())
    }

    /// The name of the local account whose SID is the machine's SID with
    /// RID 500.
    pub fn builtin_administrator() -> Option<String> {
        let machine = std::env::var("COMPUTERNAME").ok()?;
        let machine_sid = account_sid(&machine)?;
        let admin_sid = format!("{machine_sid}-500");
        let wide_sid = wide(&admin_sid);
        let mut sid = std::ptr::null_mut();
        // SAFETY: converts a string SID; freed below.
        if unsafe { ConvertStringSidToSidW(wide_sid.as_ptr(), &mut sid) } == 0 {
            return None;
        }
        let mut name = [0u16; 256];
        let mut name_len = name.len() as u32;
        let mut domain = [0u16; 256];
        let mut domain_len = domain.len() as u32;
        let mut kind: SID_NAME_USE = 0;
        // SAFETY: buffers sized as declared.
        let ok = unsafe {
            LookupAccountSidW(
                std::ptr::null(),
                sid,
                name.as_mut_ptr(),
                &mut name_len,
                domain.as_mut_ptr(),
                &mut domain_len,
                &mut kind,
            )
        };
        // SAFETY: `sid` came from ConvertStringSidToSidW.
        unsafe { LocalFree(sid) };
        (ok != 0).then(|| String::from_utf16_lossy(&name[..name_len as usize]))
    }

    /// The SID of an account name, as a string.
    fn account_sid(account: &str) -> Option<String> {
        let account = wide(account);
        let mut sid = vec![0u8; 256];
        let mut sid_len = sid.len() as u32;
        let mut domain = [0u16; 256];
        let mut domain_len = domain.len() as u32;
        let mut kind: SID_NAME_USE = 0;
        // SAFETY: buffers sized as declared.
        let ok = unsafe {
            LookupAccountNameW(
                std::ptr::null(),
                account.as_ptr(),
                sid.as_mut_ptr() as *mut _,
                &mut sid_len,
                domain.as_mut_ptr(),
                &mut domain_len,
                &mut kind,
            )
        };
        if ok == 0 {
            return None;
        }
        let mut text: *mut u16 = std::ptr::null_mut();
        // SAFETY: `sid` holds a valid SID; `text` is freed below.
        if unsafe { ConvertSidToStringSidW(sid.as_mut_ptr() as *mut _, &mut text) } == 0 {
            return None;
        }
        // SAFETY: a NUL-terminated string from ConvertSidToStringSidW.
        let out = unsafe {
            let mut len = 0usize;
            while *text.add(len) != 0 {
                len += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(text, len))
        };
        // SAFETY: allocated by ConvertSidToStringSidW.
        unsafe { LocalFree(text as *mut _) };
        Some(out)
    }

    /// The current user's SID, from `whoami /user`.
    pub fn current_user_sid() -> String {
        let out = std::process::Command::new("whoami")
            .args(["/user", "/fo", "csv", "/nh"])
            .output()
            .expect("run whoami");
        let line = String::from_utf8_lossy(&out.stdout);
        line.trim()
            .rsplit(',')
            .next()
            .unwrap_or("")
            .trim_matches('"')
            .to_string()
    }
}
