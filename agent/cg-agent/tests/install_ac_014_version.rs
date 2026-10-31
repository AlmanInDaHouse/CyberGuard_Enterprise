//! SPEC-024 install_ac_014 — one product version across the agent's
//! surfaces (debt #131): `cg-agent --version`, the enrollment request's
//! `agent_version`, the plain and the secure heartbeat's
//! `agent.agent_version`, and the "agent starting" lifecycle line's
//! `agent.version` all carry the value `common::expected_agent_version()`
//! derives from the contract. The version matrix
//! (`.github/scripts/agent-version-matrix.mjs`) runs this file without the
//! build input, with two fixture versions unlike Cargo's, and without it
//! again, in one target directory, so an embedded value a rebuild left stale
//! fails here; it also checks that a malformed input fails the build.

mod common;

use serde_json::Value;
use std::process::Stdio;
use std::time::Duration;
use tokio::sync::oneshot;

/// A value in the running agent's environment under the build input's name.
/// The version is fixed when the agent is built; the agent reads no
/// environment variable for it, so this one must not show anywhere.
const DECOY: &str = "250.251.65000";

#[test]
fn install_ac_014_cli_version() {
    // No config, network, identity, service or elevation: an empty
    // directory as the working directory, where no agent.toml exists.
    let dir = tempfile::tempdir().expect("tempdir");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cg-agent"))
        .arg("--version")
        .current_dir(dir.path())
        .env("CG_RELEASE_VERSION", DECOY)
        .output()
        .expect("launch the agent binary");
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    assert_eq!(
        stdout.trim_end(),
        format!("cg-agent {}", common::expected_agent_version())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn install_ac_014_plain_heartbeat_and_lifecycle() {
    let mock = common::MockServer::start().await;
    let config = common::write_temp_config(&format!(
        "[server]\n\
         url = \"{url}\"\n\
         \n\
         [agent]\n\
         id = \"01934abc-def0-7000-89ab-000000000001\"\n\
         hostname = \"FIN-PC-014\"\n\
         \n\
         [heartbeat]\n\
         interval_seconds = 1\n",
        url = mock.base_url
    ));
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_cg-agent"))
        .arg("--config")
        .arg(config.path())
        .env("CG_RELEASE_VERSION", DECOY)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("launch the agent binary");

    for _ in 0..200 {
        if mock.received_count() >= 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    child.start_kill().expect("end the agent");
    let output = child.wait_with_output().await.expect("agent output");

    let expected = common::expected_agent_version();
    let received = mock.received();
    assert!(!received.is_empty(), "expected >= 1 plain heartbeat");
    assert_eq!(
        received[0]["agent"]["agent_version"].as_str(),
        Some(expected.as_str()),
        "plain heartbeat agent.agent_version"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let starting = stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| v["fields"]["message"].as_str() == Some("agent starting"))
        .unwrap_or_else(|| panic!("no \"agent starting\" line in stdout: {stdout}"));
    assert_eq!(
        starting["fields"]["agent.version"].as_str(),
        Some(expected.as_str()),
        "lifecycle agent.version: {starting}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn install_ac_014_enrollment_request() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let mock = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    let fixture = common::enrollment_fixture(&mock.base_url, "tok-install-ac-014", &pki);

    cg_agent::identity::ensure_identity(&fixture.config, &fixture.config_path)
        .await
        .expect("enrollment should succeed");

    let requests = mock.enroll_received();
    assert_eq!(requests.len(), 1, "exactly one enrollment request");
    assert_eq!(
        requests[0]["agent_version"].as_str(),
        Some(common::expected_agent_version().as_str()),
        "enrollment agent_version"
    );
}

/// True if this Windows process's token is elevated (`TokenElevation`). A
/// token that cannot be read counts as elevated, so the test skips rather
/// than let the agent open its ETW session.
#[cfg(windows)]
fn elevated() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: the pseudo-handle of the current process and an out pointer to
    // a local HANDLE; the token is closed below.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return true;
    }
    let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let mut len = 0u32;
    // SAFETY: a valid token handle, and a buffer of the size passed.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            (&mut elevation as *mut TOKEN_ELEVATION).cast(),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
    };
    // SAFETY: the handle OpenProcessToken returned.
    unsafe { CloseHandle(token) };
    ok == 0 || elevation.TokenIsElevated != 0
}

/// The binary on the secure path, the decoy in its environment: its
/// enrollment request and, where the secure path captures nothing, its signed
/// heartbeats carry the build's version. The in-process tests above share
/// the test's environment, which in the matrix equals the build input, so
/// they cannot tell a runtime read from the embedded value; this one can.
/// On Windows the secure path opens the ETW session after enrolling: an
/// unelevated agent then exits 9, so only its enrollment is observed, and an
/// elevated run is skipped rather than capture.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn install_ac_014_binary_secure_path() {
    #[cfg(windows)]
    if elevated() {
        eprintln!("skipped: elevated (the agent would open its ETW session)");
        return;
    }
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    // The mock verifies each envelope under the PKI's key, not the key the
    // agent generates, and refuses it; it records every attempt's envelope.
    let heartbeat = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;

    let dir = tempfile::tempdir().expect("tempdir");
    let trust_anchor = dir.path().join("server-ca.pem");
    std::fs::write(&trust_anchor, pki.trust_anchor_pem.as_bytes()).expect("write trust anchor");
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
         hostname = \"FIN-PC-014\"\n\
         \n\
         [heartbeat]\n\
         interval_seconds = 1\n\
         \n\
         [enrollment]\n\
         token = \"tok-install-ac-014-binary\"\n\
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

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_cg-agent"))
        .arg("--config")
        .arg(&config_path)
        .env("CG_RELEASE_VERSION", DECOY)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("launch the agent binary");
    let observed = common::wait_until(Duration::from_secs(20), || {
        if cfg!(windows) {
            enroll.enroll_received_count() >= 1
        } else {
            !heartbeat.attempts().is_empty()
        }
    })
    .await;
    let _ = child.start_kill();
    let _ = child.wait().await;
    assert!(observed, "the agent's requests did not arrive");

    let expected = common::expected_agent_version();
    let requests = enroll.enroll_received();
    assert_eq!(requests.len(), 1, "exactly one enrollment request");
    assert_eq!(
        requests[0]["agent_version"].as_str(),
        Some(expected.as_str()),
        "binary enrollment agent_version"
    );
    if !cfg!(windows) {
        let attempts = heartbeat.attempts();
        assert_eq!(
            attempts[0].envelope["body"]["agent"]["agent_version"].as_str(),
            Some(expected.as_str()),
            "binary secure heartbeat agent.agent_version"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn install_ac_014_secure_heartbeat() {
    const AGENT_ID: &str = "01934abc-def0-7000-89ab-0000000000ab";
    let pki = common::generate_test_pki(AGENT_ID);
    let mock = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let common::SecureFixture {
        config,
        identity,
        trust_anchor: _trust_anchor,
    } = common::secure_fixture(&mock.base_url, &pki);

    let (tx, rx) = oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        cg_agent::run_secure(config, identity, cg_agent::Capture::Off, async move {
            let _ = rx.await;
        })
        .await
    });
    for _ in 0..100 {
        if mock.received_count() >= 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let received = mock.received();
    let _ = tx.send(());
    let _ = handle.await;

    assert!(!received.is_empty(), "expected >= 1 signed heartbeat");
    assert_eq!(
        received[0]["body"]["agent"]["agent_version"].as_str(),
        Some(common::expected_agent_version().as_str()),
        "secure heartbeat agent.agent_version"
    );
}
