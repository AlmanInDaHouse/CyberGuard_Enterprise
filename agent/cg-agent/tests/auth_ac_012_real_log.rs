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
//! - reads the last 200 records 4624 of the log with its own one-off query
//!   through the agent's rendering, and decodes them: none whose target
//!   SPEC-020 §Operational §2 excludes — judged here from the SPEC's list,
//!   not the agent's predicate — gives an event, and at least one such
//!   record is among them.
//!
//! It prints only redacted data (§Operational §11): SIDs as prefix and
//! RID, codes, whether a source was present and of which kind, and each
//! event's delay from its own timestamp to the ring — never a name.
//! Real Security log, elevated: run with
//! `cargo test -p cg-agent -- --ignored --test-threads=1 --show-output`.

#[cfg(windows)]
#[ignore = "real Security log, elevated gate: cargo test -p cg-agent -- --ignored --test-threads=1"]
#[test]
fn auth_ac_012_logons_on_the_real_log() {
    use cg_agent::etw::EventRing;
    use cg_agent::logon::{
        decode_logon, LogonDecode, LogonEvent, LogonStatus, LogonSubscription, WITHHELD,
    };
    use std::net::IpAddr;
    use std::sync::Arc;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    /// The codes of §Operational §3, from the SPEC.
    const SPEC_ACCOUNT_EXISTS: [u32; 9] = [
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

    /// §Operational §2's exclusion, written here from the SPEC.
    fn spec_excludes(sid: &str, name: &str) -> bool {
        ["S-1-5-18", "S-1-5-19", "S-1-5-20"].contains(&sid)
            || [
                "S-1-5-80-",
                "S-1-5-82-",
                "S-1-5-83-",
                "S-1-5-84-",
                "S-1-5-90-",
                "S-1-5-96-",
            ]
            .iter()
            .any(|p| sid.starts_with(p))
            || name.ends_with('$')
    }

    /// A SID as the gate may print it: its authority and first
    /// sub-authority, then its RID; a short well-known SID as it is.
    fn redact(sid: &str) -> String {
        let parts: Vec<&str> = sid.split('-').collect();
        if parts.len() <= 5 {
            sid.to_string()
        } else {
            format!(
                "{}-{}-{}-{}-...-{}",
                parts[0],
                parts[1],
                parts[2],
                parts[3],
                parts[parts.len() - 1]
            )
        }
    }

    fn source_kind(event: &LogonEvent) -> &'static str {
        match event.src.as_ref().map(|s| s.ip) {
            None => "none",
            Some(ip) if ip.is_loopback() => "loopback",
            Some(IpAddr::V4(_)) => "ipv4",
            Some(IpAddr::V6(_)) => "ipv6",
        }
    }

    let now_nanos = || {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    };

    let ring = Arc::new(EventRing::new(65536));
    let mut subscription = LogonSubscription::open(Arc::clone(&ring))
        .expect("auth_ac_012: open the Security log subscription (elevated?)");
    std::thread::sleep(Duration::from_millis(500));

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

    // The successful network logon to the loopback.
    windows::remove_loopback_connections();
    let accepted = windows::net(&["use", r"\\127.0.0.1\IPC$"]);
    windows::remove_loopback_connections();
    assert!(
        accepted,
        "net use \\\\127.0.0.1\\IPC$ must succeed (is the Server service running?)"
    );
    let current_sid = windows::current_user_sid();

    let withheld = |e: &LogonEvent| {
        e.status == LogonStatus::Failure
            && e.user_name == WITHHELD
            && e.user_domain == WITHHELD
            && e.status_detail == Some(0xC000_0064)
    };
    let kept = |e: &LogonEvent| {
        let code = match e.status_detail {
            Some(code) if code != 0 => code,
            _ => e.status_code.unwrap_or(0),
        };
        e.status == LogonStatus::Failure
            && e.user_name != WITHHELD
            && SPEC_ACCOUNT_EXISTS.contains(&code)
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
    let tests: [&dyn Fn(&LogonEvent) -> bool; 3] = [&withheld, &kept, &success];

    // Each found event's delay: from its own timestamp to when it was seen.
    let mut found: [Option<(LogonEvent, Duration)>; 3] = [None, None, None];
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut seen: Vec<LogonEvent> = Vec::new();
    while Instant::now() < deadline && found.iter().any(Option::is_none) {
        seen = ring
            .snapshot_events()
            .iter()
            .filter_map(|e| e.as_logon().cloned())
            .collect();
        let at = now_nanos();
        for (slot, test) in found.iter_mut().zip(tests) {
            if slot.is_none() {
                if let Some(e) = seen.iter().find(|e| test(e)) {
                    let delay = Duration::from_nanos(at.saturating_sub(e.timestamp_nanos));
                    *slot = Some((e.clone(), delay));
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    subscription.stop();

    // The one-off query of the log's own recent records.
    let recent = windows::recent_4624(200);
    let excluded: Vec<_> = recent
        .iter()
        .filter(|r| {
            spec_excludes(
                r.target_user_sid.as_deref().unwrap_or(""),
                r.target_user_name.as_deref().unwrap_or(""),
            )
        })
        .collect();
    let leaked = excluded
        .iter()
        .filter(|r| matches!(decode_logon(r), LogonDecode::Report(_)))
        .count();

    // Redacted report (ADR-0019 §10; the session handoff records it).
    for (label, slot) in ["withheld", "kept", "success"].iter().zip(&found) {
        match slot {
            Some((e, delay)) => println!(
                "auth_ac_012 {label} uid={} status={:#x} sub_status={:#x} name_withheld={} type={} package={} source={} elevated={:?} delay={delay:?}",
                redact(&e.user_uid),
                e.status_code.unwrap_or(0),
                e.status_detail.unwrap_or(0),
                e.user_name == WITHHELD,
                e.logon_type_id,
                e.auth_protocol,
                source_kind(e),
                e.elevated_token,
            ),
            None => println!("auth_ac_012 {label} not found"),
        }
    }
    println!(
        "auth_ac_012 ring_logons={} recent_4624={} excluded_targets={} excluded_reported={leaked}",
        seen.len(),
        recent.len(),
        excluded.len()
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
    if excluded.is_empty() {
        failures.push("the last 200 records 4624 hold no excluded target: the check is vacuous");
    }
    if leaked != 0 {
        failures.push("a record of an excluded target decoded to an event");
    }
    assert!(failures.is_empty(), "auth_ac_012 failed: {failures:?}");
}

#[cfg(windows)]
mod windows {
    use cg_agent::logon::{RawLogonRecord, RenderContext};
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, LocalFree, HANDLE};
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSidToSidW,
    };
    use windows_sys::Win32::Security::{
        LogonUserW, LookupAccountNameW, LookupAccountSidW, LOGON32_LOGON_NETWORK,
        LOGON32_PROVIDER_DEFAULT, SID_NAME_USE,
    };
    use windows_sys::Win32::System::EventLog::{
        EvtClose, EvtNext, EvtQuery, EvtQueryChannelPath, EvtQueryReverseDirection,
    };

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Run net.exe silently; whether it succeeded. Its output, which may
    /// name the user, is discarded.
    pub fn net(args: &[&str]) -> bool {
        std::process::Command::new("net")
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Remove every connection to `\\127.0.0.1`, as `net use` lists them.
    pub fn remove_loopback_connections() {
        let Ok(out) = std::process::Command::new("net").arg("use").output() else {
            return;
        };
        let listing = String::from_utf8_lossy(&out.stdout);
        for token in listing.split_whitespace() {
            if token.to_ascii_lowercase().starts_with(r"\\127.0.0.1\") {
                net(&["use", token, "/delete", "/y"]);
            }
        }
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
        let wide_sid = wide(&format!("{machine_sid}-500"));
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

    /// The last `max` records 4624 of the Security log, newest first,
    /// rendered through the agent's values context. Test-owned: the agent
    /// itself reads no past record (ADR-0019 §8).
    pub fn recent_4624(max: usize) -> Vec<RawLogonRecord> {
        let context = RenderContext::new().expect("the values context");
        let (channel, query) = (wide("Security"), wide("*[System[(EventID=4624)]]"));
        // SAFETY: the strings live for the call.
        let results = unsafe {
            EvtQuery(
                0,
                channel.as_ptr(),
                query.as_ptr(),
                EvtQueryChannelPath | EvtQueryReverseDirection,
            )
        };
        assert_ne!(results, 0, "query the Security log");
        let mut records = Vec::new();
        while records.len() < max {
            let mut handles = [0isize; 32];
            let mut returned = 0u32;
            // SAFETY: room for 32 handles.
            let ok = unsafe { EvtNext(results, 32, handles.as_mut_ptr(), 0, 0, &mut returned) };
            if ok == 0 {
                break;
            }
            for &handle in &handles[..returned as usize] {
                if records.len() < max {
                    if let Ok(raw) = context.render(handle) {
                        records.push(raw);
                    }
                }
                // SAFETY: a handle EvtNext returned, closed once.
                unsafe { EvtClose(handle) };
            }
        }
        // SAFETY: the query handle.
        unsafe { EvtClose(results) };
        records
    }
}
