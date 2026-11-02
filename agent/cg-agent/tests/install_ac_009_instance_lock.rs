//! SPEC-024 install_ac_009 — the per-host instance lock.
//!
//! A concurrent console/service instance fails before it can reclaim the
//! live instance's ETW session; an orphan after a crash is recoverable
//! without affecting another application's sessions (SPEC-024 §Operational
//! §3 and its second Amendment 2026-11-01 §1; ADR-0008 and ADR-0010
//! Amendments 2026-11-02).
//!
//! Two layers:
//!   - Pure classification (always run): without the lock an
//!     `ERROR_ALREADY_EXISTS` start failure is a privilege failure (code 9),
//!     the stop after a failed start narrows to privilege failures, and the
//!     `LockOutcome`s carry their exit codes and lines.
//!   - Real Win32 (`#[ignore]`, the elevated gate and rust-ci's Windows
//!     job, `--test-threads=1`, so they do not race the other tests that
//!     open the agent's fixed session name): a leftover is not reclaimed by
//!     an unlocked start and survives it; a held lock refuses a second
//!     instance with code 10 before it enrolls; an orphan is reclaimed
//!     while a session of another name runs on; and a token that controls
//!     trace sessions outside ADR-0010's model is detected.

mod common;

use cg_agent::etw::{OpenError, StartMode};
use cg_agent::instance_lock::{LockOutcome, STDERR_ANOTHER_INSTANCE, STDERR_CONTROLS_ETW};

/// The exact SPEC-024 §1 lines, held here so a drift in the module fails.
const ANOTHER_INSTANCE: &str =
    "cg-agent: another CyberGuard agent instance is running on this host";
const CONTROLS_ETW: &str = "cg-agent: instance lock failed: this token controls ETW sessions but is not an elevated administrator's or SYSTEM's";

#[test]
fn spec_lines_match() {
    assert_eq!(STDERR_ANOTHER_INSTANCE, ANOTHER_INSTANCE);
    assert_eq!(STDERR_CONTROLS_ETW, CONTROLS_ETW);
}

#[test]
fn unlocked_already_exists_is_a_privilege_failure() {
    // ERROR_ALREADY_EXISTS (183): the name is another start's session,
    // which an unlocked start may not reclaim → privilege, exit code 9.
    let raw = OpenError::from_win32(183);
    assert!(matches!(raw, OpenError::Failed { code: 183, .. }));
    let classified = raw.with_lock_mode(StartMode::Unlocked);
    assert_eq!(classified, OpenError::PrivilegeNotHeld);
    assert!(classified.is_privilege(), "code 9");
}

#[test]
fn locked_already_exists_keeps_code_1() {
    // A start that holds the lock is the one that reclaims; a refusal
    // because the name exists keeps code 1.
    let raw = OpenError::from_win32(183);
    let classified = raw.clone().with_lock_mode(StartMode::Locked);
    assert_eq!(classified, raw);
    assert!(!classified.is_privilege());
}

#[test]
fn with_lock_mode_leaves_other_failures_untouched() {
    for code in [5u32, 1314, 31, 6] {
        let raw = OpenError::from_win32(code);
        assert_eq!(
            raw.clone().with_lock_mode(StartMode::Unlocked),
            raw,
            "{code}"
        );
        assert_eq!(raw.clone().with_lock_mode(StartMode::Locked), raw, "{code}");
    }
}

#[test]
fn stop_after_failed_start_narrows_without_the_lock() {
    // With the lock, stop after any failed start (the reclaiming start owns
    // the name).
    for code in [5u32, 1314, 183, 31] {
        assert!(
            OpenError::from_win32(code).must_stop_after_failed_start(StartMode::Locked),
            "{code} locked"
        );
    }
    // Without the lock, only a privilege failure; never ERROR_ALREADY_EXISTS
    // (183), whose session is another start's, nor a generic failure.
    assert!(OpenError::from_win32(5).must_stop_after_failed_start(StartMode::Unlocked));
    assert!(OpenError::from_win32(1314).must_stop_after_failed_start(StartMode::Unlocked));
    assert!(!OpenError::from_win32(183).must_stop_after_failed_start(StartMode::Unlocked));
    assert!(!OpenError::from_win32(31).must_stop_after_failed_start(StartMode::Unlocked));
}

#[test]
fn lock_outcomes_carry_their_exit_codes_and_lines() {
    assert_eq!(LockOutcome::Held.exit_code(), Some(10));
    assert_eq!(
        LockOutcome::Held.stderr_line().as_deref(),
        Some(ANOTHER_INSTANCE)
    );
    assert_eq!(LockOutcome::ControlsEtwOutsideModel.exit_code(), Some(1));
    assert_eq!(
        LockOutcome::ControlsEtwOutsideModel
            .stderr_line()
            .as_deref(),
        Some(CONTROLS_ETW)
    );
    let failed = LockOutcome::Failed {
        step: "create mutex",
        code: 5,
    };
    assert_eq!(failed.exit_code(), Some(1));
    assert_eq!(
        failed.stderr_line().as_deref(),
        Some("cg-agent: instance lock failed: create mutex: Win32 error 5")
    );
    // The two outcomes that go on carry neither.
    assert_eq!(LockOutcome::Unprivileged.exit_code(), None);
    assert_eq!(LockOutcome::Unprivileged.stderr_line(), None);
}

// ---------------------------------------------------------------------------
// Real Win32. `#[ignore]`: the elevated gate and rust-ci's Windows job run
// these with `--test-threads=1`, so they do not race the other tests that
// open the agent's fixed ETW session name.
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod real {
    use super::*;
    use cg_agent::etw::{EtwSession, SESSION_NAME};
    use cg_agent::instance_lock::{self, classify_token, TokenClass};
    use std::process::Command;

    fn logman(args: &[&str]) -> bool {
        Command::new("logman")
            .args(args)
            .status()
            .expect("run logman")
            .success()
    }

    fn session_running(name: &str) -> bool {
        logman(&["query", name, "-ets"])
    }

    /// Whether this process's token can create the lock (an elevated
    /// administrator or SYSTEM). Used to skip the elevated-only cases.
    fn can_create_lock() -> bool {
        use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
        use windows_sys::Win32::Security::TOKEN_QUERY;
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
        let mut token: HANDLE = std::ptr::null_mut();
        // SAFETY: the pseudo-handle needs no close; `token` is closed below.
        let ok = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) };
        assert_ne!(ok, 0, "OpenProcessToken");
        let class = classify_token(token);
        // SAFETY: opened just above.
        unsafe { CloseHandle(token) };
        class == TokenClass::CanCreate
    }

    /// `GetTokenInformation(TokenGroups)` into a pointer-aligned buffer.
    fn token_groups(token: windows_sys::Win32::Foundation::HANDLE) -> Vec<u64> {
        use windows_sys::Win32::Security::{GetTokenInformation, TokenGroups};
        let mut needed = 0u32;
        // SAFETY: a size query (null buffer) on a valid token.
        unsafe { GetTokenInformation(token, TokenGroups, std::ptr::null_mut(), 0, &mut needed) };
        let mut buf = vec![0u64; (needed as usize).div_ceil(8).max(1)];
        let mut written = 0u32;
        // SAFETY: buf holds at least `needed` bytes; the length matches.
        let ok = unsafe {
            GetTokenInformation(
                token,
                TokenGroups,
                buf.as_mut_ptr().cast(),
                (buf.len() * 8) as u32,
                &mut written,
            )
        };
        assert_ne!(ok, 0, "GetTokenInformation(TokenGroups)");
        buf
    }

    /// `SID_AND_ATTRIBUTES` (to disable) for each group of `groups_buf`
    /// whose SID is one of the ETW-relevant well-known SIDs `classify_token`
    /// checks — Administrators, Performance Log Users, LocalService,
    /// NetworkService. The returned SIDs point into `groups_buf`, which must
    /// outlive them. Only groups actually present are returned, so
    /// `CreateRestrictedToken` never sees an absent SID.
    fn etw_relevant_sids_to_disable(
        groups_buf: &[u64],
    ) -> Vec<windows_sys::Win32::Security::SID_AND_ATTRIBUTES> {
        use windows_sys::Win32::Security::{
            CreateWellKnownSid, EqualSid, WinBuiltinAdministratorsSid,
            WinBuiltinPerfLoggingUsersSid, WinLocalServiceSid, WinNetworkServiceSid,
            SECURITY_MAX_SID_SIZE, SID_AND_ATTRIBUTES, TOKEN_GROUPS, WELL_KNOWN_SID_TYPE,
        };
        let well_known = |kind: WELL_KNOWN_SID_TYPE| -> [u8; SECURITY_MAX_SID_SIZE as usize] {
            let mut sid = [0u8; SECURITY_MAX_SID_SIZE as usize];
            let mut len = SECURITY_MAX_SID_SIZE;
            // SAFETY: the buffer is SECURITY_MAX_SID_SIZE bytes, the size passed.
            assert_ne!(
                unsafe {
                    CreateWellKnownSid(
                        kind,
                        std::ptr::null_mut(),
                        sid.as_mut_ptr().cast(),
                        &mut len,
                    )
                },
                0,
                "CreateWellKnownSid"
            );
            sid
        };
        let mut targets = [
            well_known(WinBuiltinAdministratorsSid),
            well_known(WinBuiltinPerfLoggingUsersSid),
            well_known(WinLocalServiceSid),
            well_known(WinNetworkServiceSid),
        ];
        let mut out = Vec::new();
        // SAFETY: `groups_buf` holds a TOKEN_GROUPS; each `Sid` points inside
        // it, and the array has `GroupCount` entries.
        unsafe {
            let groups = groups_buf.as_ptr() as *const TOKEN_GROUPS;
            let count = (*groups).GroupCount as usize;
            let first = std::ptr::addr_of!((*groups).Groups).cast::<SID_AND_ATTRIBUTES>();
            for i in 0..count {
                let entry = &*first.add(i);
                let matches = targets
                    .iter_mut()
                    .any(|t| EqualSid(entry.Sid, t.as_mut_ptr().cast()) != 0);
                if matches {
                    out.push(SID_AND_ATTRIBUTES {
                        Sid: entry.Sid,
                        Attributes: 0,
                    });
                }
            }
        }
        out
    }

    /// An ordinary token derived from `source`: every ETW-relevant group
    /// made deny-only and `SeSystemProfilePrivilege` deleted (not merely
    /// disabled — `classify_token` treats a present privilege as held, since
    /// a holder can re-enable it, so `DISABLE_MAX_PRIVILEGE` would still
    /// classify as ControlsEtw). The caller closes the returned handle.
    fn restricted_ordinary_token(
        source: windows_sys::Win32::Foundation::HANDLE,
    ) -> windows_sys::Win32::Foundation::HANDLE {
        use windows_sys::Win32::Foundation::{GetLastError, LUID};
        use windows_sys::Win32::Security::{
            CreateRestrictedToken, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES,
            SE_SYSTEM_PROFILE_NAME,
        };
        let groups_buf = token_groups(source);
        let mut disable = etw_relevant_sids_to_disable(&groups_buf);
        let mut luid = LUID {
            LowPart: 0,
            HighPart: 0,
        };
        // SAFETY: a local privilege-name lookup into `luid`.
        assert_ne!(
            unsafe { LookupPrivilegeValueW(std::ptr::null(), SE_SYSTEM_PROFILE_NAME, &mut luid) },
            0,
            "LookupPrivilegeValueW(SeSystemProfilePrivilege)"
        );
        let mut delete = [LUID_AND_ATTRIBUTES {
            Luid: luid,
            Attributes: 0,
        }];
        let mut out: windows_sys::Win32::Foundation::HANDLE = std::ptr::null_mut();
        // SAFETY: `source` is duplicatable; `disable` points into `groups_buf`
        // (alive here) and `delete` is a local array; `out` receives the new
        // token the caller closes.
        let ok = unsafe {
            CreateRestrictedToken(
                source,
                0,
                disable.len() as u32,
                disable.as_mut_ptr(),
                delete.len() as u32,
                delete.as_mut_ptr(),
                0,
                std::ptr::null(),
                &mut out,
            )
        };
        assert_ne!(ok, 0, "CreateRestrictedToken (ordinary): {}", unsafe {
            GetLastError()
        });
        out
    }

    /// On a host whose token controls ETW (this dev host, unelevated),
    /// `restricted_ordinary_token` yields an Ordinary token — the synthesis
    /// the elevated `classify_token_*` test relies on, checkable without
    /// elevation here. Skips where the token cannot control ETW (an elevated
    /// admin is CanCreate; an already-ordinary token has nothing to strip).
    #[cfg(windows)]
    #[test]
    fn restricted_ordinary_token_is_ordinary() {
        use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
        use windows_sys::Win32::Security::{TOKEN_DUPLICATE, TOKEN_QUERY};
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

        if !common::token_controls_etw() {
            eprintln!("skipped: this token does not control ETW (nothing to strip)");
            return;
        }
        let mut token: HANDLE = std::ptr::null_mut();
        // SAFETY: the pseudo-handle needs no close; `token` closed below.
        assert_ne!(
            unsafe {
                OpenProcessToken(
                    GetCurrentProcess(),
                    TOKEN_QUERY | TOKEN_DUPLICATE,
                    &mut token,
                )
            },
            0,
            "OpenProcessToken"
        );
        let ordinary = restricted_ordinary_token(token);
        let class = classify_token(ordinary);
        // SAFETY: both handles were opened/created above.
        unsafe {
            CloseHandle(ordinary);
            CloseHandle(token);
        }
        assert_eq!(class, TokenClass::Ordinary);
    }

    /// Write a secure-path `agent.toml` (enrollment + trust anchor) into
    /// `dir` and return its path. The agent reaches the instance lock only
    /// on the secure path.
    fn write_secure_config(
        dir: &std::path::Path,
        enroll_url: &str,
        heartbeat_url: &str,
        anchor_pem: &str,
    ) -> std::path::PathBuf {
        use std::io::Write as _;
        let trust_anchor = dir.join("server-ca.pem");
        std::fs::File::create(&trust_anchor)
            .and_then(|mut f| f.write_all(anchor_pem.as_bytes()))
            .expect("write trust anchor");
        let config_path = dir.join("agent.toml");
        let toml = format!(
            "[server]\n\
             url = \"{enroll_url}\"\n\
             heartbeat_url = \"{heartbeat_url}\"\n\
             trust_anchor_path = '{anchor}'\n\
             \n\
             [agent]\n\
             id = \"01934abc-def0-7000-89ab-000000000009\"\n\
             hostname = \"TEST-PC\"\n\
             \n\
             [enrollment]\n\
             token = \"install-ac-009\"\n\
             cert_path = '{cert}'\n\
             key_path = '{key}'\n\
             identity_path = '{identity}'\n",
            anchor = trust_anchor.display(),
            cert = dir.join("cert.pem").display(),
            key = dir.join("key.dat").display(),
            identity = dir.join("identity.json").display(),
        );
        std::fs::write(&config_path, toml).expect("write agent.toml");
        config_path
    }

    /// Launch the real agent binary on `config_path` and return its output
    /// (it must exit on its own within 60 s).
    async fn run_binary(config_path: &std::path::Path) -> std::process::Output {
        tokio::time::timeout(
            std::time::Duration::from_secs(60),
            tokio::process::Command::new(env!("CARGO_BIN_EXE_cg-agent"))
                .arg("--config")
                .arg(config_path)
                .output(),
        )
        .await
        .expect("the agent must exit on its own")
        .expect("launch the agent binary")
    }

    /// Without the lock, a start does not reclaim a leftover session of the
    /// agent's name: it fails for privilege (ERROR_ALREADY_EXISTS → code 9)
    /// and the leftover survives. Elevation-independent — an unlocked start
    /// never reclaims (SPEC-024 §1).
    #[ignore = "real ETW: cargo test -p cg-agent -- --ignored --test-threads=1"]
    #[test]
    fn unlocked_start_does_not_reclaim_a_leftover_and_fails_privilege() {
        let _ = logman(&["stop", SESSION_NAME, "-ets"]);
        assert!(
            logman(&["start", SESSION_NAME, "-ets", "-rt"]),
            "start the leftover session"
        );
        assert!(
            session_running(SESSION_NAME),
            "the leftover must run before the open"
        );

        let result = EtwSession::open(16, StartMode::Unlocked);
        let survived = session_running(SESSION_NAME);
        let _ = logman(&["stop", SESSION_NAME, "-ets"]);

        match result {
            Ok(mut session) => {
                session.stop();
                panic!("an unlocked start must not open over an existing session of the name");
            }
            Err(e) => {
                assert!(
                    e.is_privilege(),
                    "an unlocked ERROR_ALREADY_EXISTS is a privilege failure, got {e:?}"
                );
                assert!(
                    survived,
                    "the unlocked start must leave the leftover running"
                );
            }
        }
    }

    /// A start that holds the lock reclaims an orphan of the agent's name
    /// while a session of another name runs on untouched (SPEC-024 §1,
    /// ADR-0008 Amendment 2026-11-02). Elevated only.
    #[ignore = "real ETW, elevated: cargo test -p cg-agent -- --ignored --test-threads=1"]
    #[test]
    fn locked_start_reclaims_an_orphan_and_spares_another_name() {
        if !can_create_lock() {
            eprintln!("skipped: not elevated (cannot open the agent's session)");
            return;
        }
        const OTHER: &str = "CGAgent-UnrelatedSession-install-ac-009";
        let _ = logman(&["stop", SESSION_NAME, "-ets"]);
        let _ = logman(&["stop", OTHER, "-ets"]);
        assert!(logman(&["start", SESSION_NAME, "-ets", "-rt"]), "orphan");
        assert!(logman(&["start", OTHER, "-ets", "-rt"]), "another name");

        let opened = EtwSession::open(16, StartMode::Locked);
        let other_survived = session_running(OTHER);
        let _ = logman(&["stop", OTHER, "-ets"]);
        let mut session = match opened {
            Ok(session) => session,
            Err(e) => {
                let _ = logman(&["stop", SESSION_NAME, "-ets"]);
                panic!("a locked start must reclaim the orphan and open, got {e:?}");
            }
        };
        session.stop();
        let agents_left = session_running(SESSION_NAME);
        let _ = logman(&["stop", SESSION_NAME, "-ets"]);

        assert!(
            other_survived,
            "the reclaim must not touch a session of another name"
        );
        assert!(
            !agents_left,
            "stop must leave no session of the agent's name"
        );
    }

    /// Two elevated instances: the first holds the lock; a second exits
    /// with code 10 and the SPEC-024 §1 line, before it enrolls, and
    /// without stopping an unrelated ETW session (SPEC-024 §1 "without
    /// stopping any ETW session"). The first is stood in for by holding the
    /// lock in this process; the second is the real binary. Elevated only.
    #[ignore = "real lock, elevated: cargo test -p cg-agent -- --ignored --test-threads=1"]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_held_lock_refuses_a_second_instance_with_code_10_before_enrolling() {
        if !can_create_lock() {
            eprintln!("skipped: not elevated (cannot create the lock)");
            return;
        }
        // An unrelated ETW session that the refused second instance must not
        // stop (it exits before touching any session).
        const OTHER: &str = "CGAgent-UnrelatedSession-install-ac-009";
        let _ = logman(&["stop", OTHER, "-ets"]);
        assert!(logman(&["start", OTHER, "-ets", "-rt"]), "another session");

        // The first instance: hold the lock for the whole test.
        let held = match instance_lock::acquire() {
            LockOutcome::Acquired(lock) => lock,
            other => {
                let _ = logman(&["stop", OTHER, "-ets"]);
                panic!("the first instance must acquire the lock, got {other:?}");
            }
        };

        // The second instance is the real binary, configured to enroll
        // against a TLS mock. It must exit 10 before any enrollment.
        let pki = common::generate_test_pki(common::TEST_AGENT_ID);
        let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
        let heartbeat = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;

        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = write_secure_config(
            dir.path(),
            &enroll.base_url,
            &heartbeat.base_url,
            &pki.trust_anchor_pem,
        );
        let output = run_binary(&config_path).await;

        drop(held);
        let other_survived = session_running(OTHER);
        let _ = logman(&["stop", OTHER, "-ets"]);

        assert_eq!(output.status.code(), Some(10), "{output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(ANOTHER_INSTANCE), "stderr: {stderr}");
        assert_eq!(
            enroll.enroll_received_count(),
            0,
            "the refused second instance must not enroll"
        );
        assert!(
            heartbeat.attempts().is_empty(),
            "the refused second instance must not heartbeat"
        );
        assert!(
            other_survived,
            "the refused second instance must stop no ETW session"
        );
    }

    /// A token that controls trace sessions outside ADR-0010's model is
    /// refused by the real binary with code 1 and the SPEC-024 §1 line,
    /// before it enrolls (ADR-0010 Amendment 2026-11-02 item 1). Runs only
    /// when this process's token controls ETW — the launched binary inherits
    /// it — so it exercises the end-to-end refusal where the environment
    /// allows it, and skips otherwise. Not `#[ignore]`d: the refused binary
    /// never takes the lock, so it races nothing.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_token_that_controls_etw_refuses_the_binary_with_code_1() {
        if !common::token_controls_etw() {
            eprintln!("skipped: this token does not control ETW outside the model");
            return;
        }
        let pki = common::generate_test_pki(common::TEST_AGENT_ID);
        let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
        let heartbeat = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;

        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = write_secure_config(
            dir.path(),
            &enroll.base_url,
            &heartbeat.base_url,
            &pki.trust_anchor_pem,
        );
        let output = run_binary(&config_path).await;

        assert_eq!(output.status.code(), Some(1), "{output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(CONTROLS_ETW), "stderr: {stderr}");
        assert_eq!(
            enroll.enroll_received_count(),
            0,
            "the refused binary must not enroll"
        );
        assert!(
            heartbeat.attempts().is_empty(),
            "the refused binary must not heartbeat"
        );
    }

    /// The token distinction (ADR-0010 Amendment 2026-11-02): an elevated
    /// administrator's token creates the lock; a token with
    /// `BUILTIN\Administrators` made deny-only but `SeSystemProfilePrivilege`
    /// kept controls trace sessions outside the model; one with neither is
    /// ordinary. Built from this process's token, elevated only.
    #[ignore = "needs an elevated token to restrict: cargo test -p cg-agent -- --ignored --test-threads=1"]
    #[test]
    fn classify_token_tells_the_three_token_kinds_apart() {
        use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
        use windows_sys::Win32::Security::{
            CreateRestrictedToken, CreateWellKnownSid, WinBuiltinAdministratorsSid,
            SECURITY_MAX_SID_SIZE, SID_AND_ATTRIBUTES, TOKEN_DUPLICATE, TOKEN_QUERY,
        };
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

        if !can_create_lock() {
            eprintln!("skipped: not an elevated administrator");
            return;
        }

        // This process's elevated token creates the lock.
        let mut token: HANDLE = std::ptr::null_mut();
        // SAFETY: the pseudo-handle needs no close; `token` closed below.
        assert_ne!(
            unsafe {
                OpenProcessToken(
                    GetCurrentProcess(),
                    TOKEN_QUERY | TOKEN_DUPLICATE,
                    &mut token,
                )
            },
            0,
            "OpenProcessToken"
        );
        assert_eq!(classify_token(token), TokenClass::CanCreate);

        // The Administrators SID to disable.
        let mut admins = [0u8; SECURITY_MAX_SID_SIZE as usize];
        let mut len = SECURITY_MAX_SID_SIZE;
        // SAFETY: the buffer is SECURITY_MAX_SID_SIZE bytes, the size passed.
        assert_ne!(
            unsafe {
                CreateWellKnownSid(
                    WinBuiltinAdministratorsSid,
                    std::ptr::null_mut(),
                    admins.as_mut_ptr().cast(),
                    &mut len,
                )
            },
            0,
            "CreateWellKnownSid"
        );
        let mut disable = [SID_AND_ATTRIBUTES {
            Sid: admins.as_mut_ptr().cast::<core::ffi::c_void>(),
            Attributes: 0,
        }];

        // Administrators deny-only, privileges kept → controls ETW
        // (SeSystemProfilePrivilege, which an elevated token holds).
        let mut controls: HANDLE = std::ptr::null_mut();
        // SAFETY: `token` is a duplicatable token; `disable` outlives the
        // call; `controls` receives a new token closed below.
        assert_ne!(
            unsafe {
                CreateRestrictedToken(
                    token,
                    0,
                    disable.len() as u32,
                    disable.as_mut_ptr(),
                    0,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    &mut controls,
                )
            },
            0,
            "CreateRestrictedToken (controls ETW)"
        );
        assert_eq!(
            classify_token(controls),
            TokenClass::ControlsEtw,
            "Administrators deny-only + SeSystemProfilePrivilege kept"
        );

        // Every ETW-relevant group made deny-only and SeSystemProfilePrivilege
        // deleted → ordinary. DISABLE_MAX_PRIVILEGE only *disables* the
        // privilege, leaving it present and so still a holder (ControlsEtw);
        // and this host's account is also in Performance Log Users, so
        // disabling Administrators alone is not enough. The helper strips both
        // signals (verified locally by `restricted_ordinary_token_is_ordinary`).
        let ordinary = restricted_ordinary_token(token);
        assert_eq!(
            classify_token(ordinary),
            TokenClass::Ordinary,
            "no ETW-relevant group enabled and no SeSystemProfilePrivilege"
        );

        // SAFETY: the three tokens were opened/created above.
        unsafe {
            CloseHandle(controls);
            CloseHandle(ordinary);
            CloseHandle(token);
        }
    }
}
