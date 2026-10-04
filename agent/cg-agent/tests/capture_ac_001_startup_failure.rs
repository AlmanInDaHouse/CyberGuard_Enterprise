//! SPEC-017 capture_ac_001 — startup failure.
//!
//! A privilege failure of the ETW session start maps to exit code 9 with
//! the SPEC-005 AC-002 line; any other start failure maps to exit code 1
//! with `cg-agent: ETW session open failed: <code> <message>`; a build
//! without a capture backend continues startup. `EtwSession::open`
//! returns the start result itself: on unelevated Windows it is a
//! privilege failure; elevated, the session opens and `stop` leaves no
//! session of the agent's name behind.
//!
//! On unelevated Windows the real binary enrolls, then exits with code 9
//! and the AC-002 line on stderr (and in its log) without a single
//! heartbeat POST; the in-process secure path returns the same failure.
//! Both are skipped when elevated (the session would open).

mod common;

use cg_agent::errors::{AgentError, EtwError, STDERR_INSUFFICIENT_PRIVILEGE};
use cg_agent::etw::{win32_from_os_error, EtwSession, OpenError};
use cg_agent::startup::handle_etw_open_result;

const AC_002_LINE: &str = "cg-agent: insufficient privilege to open \
                           Microsoft-Windows-Kernel-Process ETW session; \
                           run as elevated user or LocalSystem";

#[test]
fn capture_ac_001_privilege_failure_exits_9_with_the_ac_002_line() {
    assert_eq!(STDERR_INSUFFICIENT_PRIVILEGE, AC_002_LINE);
    for open_error in [OpenError::AccessDenied, OpenError::PrivilegeNotHeld] {
        let etw = EtwError::from(open_error.clone());
        assert_eq!(etw.exit_code(), 9, "{open_error:?} must exit 9");
        assert_eq!(etw.stderr_line(), AC_002_LINE);
        assert_eq!(AgentError::Etw(etw).exit_code(), 9);

        let abort = handle_etw_open_result(Err(open_error)).expect_err("privilege must abort");
        assert_eq!(abort.exit_code, 9);
        assert_eq!(abort.stderr_message, AC_002_LINE);
    }
}

#[test]
fn capture_ac_001_other_failure_exits_1_with_its_code() {
    // ERROR_GEN_FAILURE (31): any start failure that is not a privilege one.
    let open_error = OpenError::from_win32(31);
    assert!(matches!(open_error, OpenError::Failed { code: 31, .. }));
    assert!(!open_error.is_privilege());

    let etw = EtwError::from(open_error.clone());
    assert_eq!(etw.exit_code(), 1);
    let line = etw.stderr_line();
    assert!(
        line.starts_with("cg-agent: ETW session open failed: 31 "),
        "unexpected line: {line}"
    );
    assert!(
        !line.contains("os error"),
        "no duplicated code suffix: {line}"
    );
    assert_eq!(AgentError::Etw(etw).exit_code(), 1);

    let abort = handle_etw_open_result(Err(open_error)).expect_err("a start failure must abort");
    assert_eq!(abort.exit_code, 1);
    assert_eq!(abort.stderr_message, line);
}

#[test]
fn capture_ac_001_privilege_codes_are_classified() {
    assert_eq!(OpenError::from_win32(5), OpenError::AccessDenied);
    assert_eq!(OpenError::from_win32(1314), OpenError::PrivilegeNotHeld);
}

#[test]
fn capture_ac_001_hresult_carries_the_win32_code() {
    // ferrisetw reports StartTraceW failures as HRESULT_FROM_WIN32 values.
    assert_eq!(win32_from_os_error(0x8007_0005_u32 as i32), 5);
    assert_eq!(win32_from_os_error(0x8007_0522_u32 as i32), 1314);
    assert_eq!(win32_from_os_error(5), 5);
}

#[test]
fn capture_ac_001_no_backend_continues_startup() {
    assert_eq!(handle_etw_open_result(Err(OpenError::Unsupported)), Ok(()));
    assert_eq!(handle_etw_open_result(Ok(())), Ok(()));
}

/// Real ETW, either way: unelevated, `open` reports the privilege failure
/// (it no longer returns `Ok` before the start runs); elevated, the
/// session opens and `stop` leaves no session of the agent's name.
#[cfg(windows)]
#[test]
fn capture_ac_001_open_returns_the_start_result() {
    let _etw = ETW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    match EtwSession::open(16) {
        Ok(mut session) => {
            session.stop();
            // ERROR_WMI_INSTANCE_NOT_FOUND: no session of that name exists.
            assert_eq!(
                cg_agent::etw::events_lost(cg_agent::etw::SESSION_NAME),
                Err(4201),
                "elevated: stop must leave no session behind"
            );
        }
        Err(e) => assert!(
            e.is_privilege(),
            "unelevated: open must report the privilege failure, got {e:?}"
        ),
    }
}

#[cfg(not(windows))]
#[test]
fn capture_ac_001_open_reports_no_backend() {
    assert!(matches!(EtwSession::open(16), Err(OpenError::Unsupported)));
}

/// Serializes the tests of this binary that may open the agent's ETW
/// session: the name is fixed, and a second open reclaims the first.
#[cfg(windows)]
static ETW_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// `true` when this process may open the agent's ETW session (elevated):
/// the session is then stopped at once.
#[cfg(windows)]
fn elevated() -> bool {
    let _etw = ETW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    match EtwSession::open(16) {
        Ok(mut session) => {
            session.stop();
            true
        }
        Err(e) if e.is_privilege() => false,
        Err(e) => panic!("unexpected ETW start failure: {e:?}"),
    }
}

/// The in-process secure path, unelevated: the capture start fails with
/// exit code 9 before any POST.
#[cfg(windows)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capture_ac_001_unelevated_secure_path_fails_before_any_post() {
    if elevated() {
        eprintln!("skipped: elevated (the ETW session opens)");
        return;
    }
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let mock = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let agent = common::start_secure_agent(&pki, &mock.base_url, 1, cg_agent::Capture::Platform);

    let outcome = agent.stop().await;
    let err = outcome.expect_err("the start failure must end run_secure");
    assert_eq!(err.exit_code(), 9);
    assert!(mock.attempts().is_empty(), "no heartbeat or event POST");
}

/// The real binary, unelevated: it enrolls, then exits 9 with the AC-002
/// line on stderr and in its log, and never POSTs a heartbeat.
#[cfg(windows)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capture_ac_001_unelevated_binary_exits_9() {
    use std::io::Write as _;

    if elevated() {
        eprintln!("skipped: elevated (the ETW session opens)");
        return;
    }
    let enroll = common::MockServer::start().await;
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let heartbeat = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;

    let dir = tempfile::tempdir().expect("tempdir");
    let trust_anchor = dir.path().join("server-ca.pem");
    std::fs::File::create(&trust_anchor)
        .and_then(|mut f| f.write_all(pki.trust_anchor_pem.as_bytes()))
        .expect("write trust anchor");
    let config_path = dir.path().join("agent.toml");
    // Paths go in TOML literal strings (single quotes): Windows backslashes.
    let toml = format!(
        "[server]\n\
         url = \"{enroll_url}\"\n\
         heartbeat_url = \"{heartbeat_url}\"\n\
         trust_anchor_path = '{anchor}'\n\
         \n\
         [agent]\n\
         id = \"01934abc-def0-7000-89ab-000000000001\"\n\
         hostname = \"TEST-PC\"\n\
         \n\
         [enrollment]\n\
         token = \"capture-ac-001\"\n\
         cert_path = '{cert}'\n\
         key_path = '{key}'\n\
         identity_path = '{identity}'\n",
        enroll_url = enroll.base_url,
        heartbeat_url = heartbeat.base_url,
        anchor = trust_anchor.display(),
        cert = dir.path().join("cert.pem").display(),
        key = dir.path().join("key.dat").display(),
        identity = dir.path().join("identity.json").display(),
    );
    std::fs::write(&config_path, toml).expect("write agent.toml");

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        tokio::process::Command::new(env!("CARGO_BIN_EXE_cg-agent"))
            .arg("--config")
            .arg(&config_path)
            .output(),
    )
    .await
    .expect("the agent must exit on its own")
    .expect("launch the agent binary");

    assert_eq!(output.status.code(), Some(9), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(AC_002_LINE), "stderr: {stderr}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout
            .lines()
            .any(|l| l.contains("\"level\":\"ERROR\"") && l.contains(AC_002_LINE)),
        "the AC-002 line is logged at error: {stdout}"
    );
    assert_eq!(
        enroll.enroll_received_count(),
        1,
        "the agent enrolled first"
    );
    assert!(
        heartbeat.attempts().is_empty(),
        "no heartbeat or event POST"
    );
}
