//! SPEC-024 install_ac_007, and the service half of install_ac_009: the
//! agent as a real Windows service, started by the service control manager
//! (SCM) and run as LocalSystem (SPEC-024 §Operational §3 and its second
//! Amendment 2026-11-01, items 1 to 4).
//!
//! These tests create, start, stop and delete Windows services and run the
//! agent as SYSTEM, so they build only with the test-only `scm-tests`
//! feature and run only in rust-ci's Windows job, on a GitHub-hosted runner
//! that is discarded after the job (SPEC-024 Amendment 2026-11-01, point 4).
//! Each one fails without acting outside such a runner or when a service
//! named `CyberGuardAgent` exists, creates its own uniquely named service,
//! touches no service it did not create, and deletes its service on every
//! exit path (`TestService`'s `Drop`). They run one at a time
//! (`--test-threads=1`): they share the host's instance lock, the agent's
//! fixed ETW session name and the fixed installed tree.
//!
//! The service runs only at the fixed paths under `%ProgramData%` and checks
//! their custody (SPEC-024 Amendment 2026-11-04, items 1 to 3): each test
//! plants that tree with its production security (`FixedDataRoot`, in
//! `scm`), which on this runner it may (Amendment 2026-11-01, point 4).
//!
//! The SCM sends no `SHUTDOWN` without an OS shutdown, which this runner
//! does not give (Amendment 2026-11-01, point 4): these tests send `STOP`,
//! and assert that `RUNNING` accepts `SHUTDOWN`; no SCM is mocked.
#![cfg(windows)]

mod common;
mod scm;

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cg_agent::etw::{events_lost, SESSION_NAME};
use cg_agent::instance_lock::{self, LockOutcome, STDERR_ANOTHER_INSTANCE};
use scm::*;
use windows_sys::Win32::System::Services::{
    SERVICE_ACCEPT_SHUTDOWN, SERVICE_ACCEPT_STOP, SERVICE_RUNNING, SERVICE_STOPPED,
};

// ---------------------------------------------------------------------------
// The tests.
// ---------------------------------------------------------------------------

/// install_ac_007: the service reports `RUNNING` accepting `STOP` and
/// `SHUTDOWN` alone; `STOP` during normal delivery is prompt, stops the
/// capture before one final `going_offline` POST, reports `STOPPED` with
/// code 0 and leaves no agent ETW session; a second start succeeds. Entries
/// 1000 (with the version) and 1001 are written.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_007_stop_during_delivery_is_clean_and_permits_restart() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    // When each final going_offline request arrives, whether the agent's
    // ETW session still runs: the capture stops before the final send.
    let at_final_send = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = Arc::clone(&at_final_send);
    let responder: common::Responder = Arc::new(move |envelope| {
        if common::envelope_status(envelope) == "going_offline" {
            seen.lock().unwrap().push(events_lost(SESSION_NAME));
        }
        None
    });
    let heartbeat = common::TlsMockServer::start_with_responder(&pki, responder).await;
    let root = FixedDataRoot::plant();
    let config = write_config(
        &root,
        &enroll.base_url,
        &heartbeat.base_url,
        &pki.trust_anchor_pem,
        &Knobs::default(),
    );
    provision_identity(&root, &pki);
    let baseline = application_baseline();
    let service = TestService::create(&service_args(&config));

    for round in 1..=2 {
        let online_before = online_count(&heartbeat);
        service
            .start()
            .unwrap_or_else(|code| panic!("start {round}: Win32 error {code}"));
        let running = service.wait_for(SERVICE_RUNNING, Duration::from_secs(60));
        assert_pending_progress(&running, "START_PENDING");
        assert_eq!(
            running.status.dwControlsAccepted,
            SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN,
            "RUNNING accepts STOP and SHUTDOWN and no other control"
        );
        assert!(
            common::wait_until(Duration::from_secs(60), || online_count(&heartbeat)
                > online_before)
            .await,
            "start {round}: the service delivers heartbeats"
        );
        assert!(
            events_lost(SESSION_NAME).is_ok(),
            "start {round}: the agent's ETW session runs while the service does"
        );

        let stop_sent = Instant::now();
        service.control_stop().expect("STOP");
        let stopped = service.wait_for(SERVICE_STOPPED, STOP_DEADLINE + Duration::from_secs(15));
        let took = stop_sent.elapsed();
        assert!(
            took < Duration::from_secs(20),
            "start {round}: STOP during normal delivery is prompt, took {took:?}"
        );
        assert_pending_progress(&stopped, "STOP_PENDING");
        assert_eq!(
            (
                stopped.status.dwWin32ExitCode,
                stopped.status.dwServiceSpecificExitCode
            ),
            (0, 0),
            "start {round}: a clean stop reports STOPPED with code 0"
        );
        let received = heartbeat.received();
        assert_eq!(
            common::envelope_status(received.last().expect("a final POST")),
            "going_offline",
            "start {round}: the last POST is the final going_offline"
        );
        assert_eq!(
            events_lost(SESSION_NAME),
            Err(4201),
            "start {round}: a normal stop leaves no agent ETW session"
        );
        assert_eq!(
            at_final_send.lock().unwrap().last().copied(),
            Some(Err(4201)),
            "start {round}: the session was stopped when the final request arrived"
        );
    }
    assert_eq!(
        enroll.enroll_received_count(),
        0,
        "the persisted identity is loaded, never re-enrolled"
    );

    let entries = wait_entries(baseline, &[1000, 1001]);
    let running = entry(&entries, 1000);
    assert_eq!(running.level, LEVEL_INFORMATION, "1000 is information");
    assert!(
        running.data.contains(cg_agent::AGENT_VERSION),
        "1000 carries the version {}: {running:?}",
        cg_agent::AGENT_VERSION
    );
    assert_eq!(entry(&entries, 1001).level, LEVEL_INFORMATION, "1001");
    assert_eq!(
        entries.iter().filter(|e| e.event_id == 1001).count(),
        2,
        "one 1001 per clean stop: {entries:#?}"
    );
    assert!(
        !entries
            .iter()
            .any(|e| e.event_id == 1002 || e.event_id == 1003),
        "no failure entry: {entries:#?}"
    );
}

/// install_ac_007: `RUNNING` precedes enrollment, and `STOP` while
/// enrollment waits between its retries ends the service promptly as a
/// clean stop, with no heartbeat.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_007_stop_during_an_enrollment_retry_wait_is_prompt() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    enroll.set_enroll_default(503);
    let heartbeat = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let root = FixedDataRoot::plant();
    let knobs = Knobs {
        enrollment_max_retries: 5,
        enrollment_backoff_initial_ms: 120_000,
        ..Knobs::default()
    };
    let config = write_config(
        &root,
        &enroll.base_url,
        &heartbeat.base_url,
        &pki.trust_anchor_pem,
        &knobs,
    );
    let baseline = application_baseline();
    let service = TestService::create(&service_args(&config));
    service.start().expect("start");
    service.wait_for(SERVICE_RUNNING, Duration::from_secs(60));
    assert!(
        common::wait_until(Duration::from_secs(60), || enroll.enroll_received_count()
            >= 1)
        .await,
        "the service enrolls after RUNNING"
    );
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        enroll.enroll_received_count(),
        1,
        "the service waits out its 120 s backoff"
    );

    let stop_sent = Instant::now();
    service.control_stop().expect("STOP");
    let stopped = service.wait_for(SERVICE_STOPPED, Duration::from_secs(30));
    let took = stop_sent.elapsed();
    assert!(
        took < Duration::from_secs(10),
        "STOP during a retry wait is prompt, took {took:?}"
    );
    assert_eq!(
        (
            stopped.status.dwWin32ExitCode,
            stopped.status.dwServiceSpecificExitCode
        ),
        (0, 0),
        "a stop during enrollment is a clean stop"
    );
    assert_eq!(
        heartbeat.received_count(),
        0,
        "no heartbeat without identity"
    );
    let entries = wait_entries(baseline, &[1001]);
    entry(&entries, 1001);
}

/// install_ac_007 and SPEC-002 Amendment 2026-10-27: an exhausted
/// enrollment round does not end the service (the console's code 4) nor
/// loop tightly — the next round waits at least 60 s — and `STOP` during
/// that wait is prompt.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_007_stop_between_enrollment_rounds_is_prompt() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    enroll.set_enroll_default(503);
    let heartbeat = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let root = FixedDataRoot::plant();
    let knobs = Knobs {
        enrollment_max_retries: 1,
        ..Knobs::default()
    };
    let config = write_config(
        &root,
        &enroll.base_url,
        &heartbeat.base_url,
        &pki.trust_anchor_pem,
        &knobs,
    );
    let service = TestService::create(&service_args(&config));
    service.start().expect("start");
    service.wait_for(SERVICE_RUNNING, Duration::from_secs(60));
    assert!(
        common::wait_until(Duration::from_secs(60), || enroll.enroll_received_count()
            >= 1)
        .await,
        "the first enrollment round"
    );
    tokio::time::sleep(Duration::from_secs(15)).await;
    assert_eq!(
        service.status().dwCurrentState,
        SERVICE_RUNNING,
        "an exhausted round does not end the service"
    );
    assert_eq!(
        enroll.enroll_received_count(),
        1,
        "no second round within 15 s of the first"
    );

    let stop_sent = Instant::now();
    service.control_stop().expect("STOP");
    let stopped = service.wait_for(SERVICE_STOPPED, Duration::from_secs(30));
    let took = stop_sent.elapsed();
    assert!(
        took < Duration::from_secs(10),
        "STOP between enrollment rounds is prompt, took {took:?}"
    );
    assert_eq!(
        (
            stopped.status.dwWin32ExitCode,
            stopped.status.dwServiceSpecificExitCode
        ),
        (0, 0),
        "a clean stop"
    );
}

/// install_ac_007: `STOP` while an enrollment request is in flight (the
/// server never answers) ends the service promptly as a clean stop.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_007_stop_with_an_enrollment_request_in_flight_is_prompt() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let silent = SilentListener::start();
    let heartbeat = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let root = FixedDataRoot::plant();
    let knobs = Knobs {
        enrollment_timeout_seconds: 600,
        enrollment_max_retries: 1,
        ..Knobs::default()
    };
    let config = write_config(
        &root,
        &silent.url,
        &heartbeat.base_url,
        &pki.trust_anchor_pem,
        &knobs,
    );
    let service = TestService::create(&service_args(&config));
    service.start().expect("start");
    service.wait_for(SERVICE_RUNNING, Duration::from_secs(60));
    assert!(
        common::wait_until(Duration::from_secs(60), || silent
            .accepted
            .load(Ordering::SeqCst)
            >= 1)
        .await,
        "the enrollment request is in flight"
    );
    tokio::time::sleep(Duration::from_secs(2)).await;

    let stop_sent = Instant::now();
    service.control_stop().expect("STOP");
    let stopped = service.wait_for(SERVICE_STOPPED, Duration::from_secs(30));
    let took = stop_sent.elapsed();
    assert!(
        took < Duration::from_secs(10),
        "STOP with an enrollment request in flight is prompt, took {took:?}"
    );
    assert_eq!(
        (
            stopped.status.dwWin32ExitCode,
            stopped.status.dwServiceSpecificExitCode
        ),
        (0, 0),
        "a clean stop"
    );
}

/// install_ac_007: a final request that never completes, with a request
/// timeout longer than the stop's budget, is bounded by the 60-second
/// deadline: the capture is already stopped, `STOP_PENDING` keeps its
/// checkpoint moving, and `STOPPED` reports code 12 with entry 1003
/// counting the events not delivered — never a clean stop.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_007_a_stalled_final_request_expires_the_deadline_with_code_12() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    let responder: common::Responder = Arc::new(|envelope| {
        (common::envelope_status(envelope) == "going_offline").then_some(common::STALL)
    });
    let heartbeat = common::TlsMockServer::start_with_responder(&pki, responder).await;
    let root = FixedDataRoot::plant();
    let knobs = Knobs {
        request_timeout_seconds: 300,
        ..Knobs::default()
    };
    let config = write_config(
        &root,
        &enroll.base_url,
        &heartbeat.base_url,
        &pki.trust_anchor_pem,
        &knobs,
    );
    provision_identity(&root, &pki);
    let baseline = application_baseline();
    let service = TestService::create(&service_args(&config));
    service.start().expect("start");
    service.wait_for(SERVICE_RUNNING, Duration::from_secs(60));
    assert!(
        common::wait_until(Duration::from_secs(60), || online_count(&heartbeat) >= 1).await,
        "the service delivers heartbeats"
    );

    let stop_sent = Instant::now();
    service.control_stop().expect("STOP");
    let stopped = service.wait_for(SERVICE_STOPPED, STOP_DEADLINE + Duration::from_secs(30));
    let took = stop_sent.elapsed();
    assert!(
        took >= Duration::from_secs(55) && took <= STOP_DEADLINE + Duration::from_secs(15),
        "the stop ends at its 60 s deadline, took {took:?}"
    );
    assert_pending_progress(&stopped, "STOP_PENDING");
    assert!(
        stopped.checkpoint_advances >= 5,
        "a 60 s STOP_PENDING advanced its checkpoint {} times",
        stopped.checkpoint_advances
    );
    assert_stopped_with(&stopped.status, 12);
    assert!(
        heartbeat
            .attempts()
            .iter()
            .any(|a| a.status == common::STALL
                && common::envelope_status(&a.envelope) == "going_offline"),
        "the final going_offline request was sent and stalled"
    );
    assert_eq!(
        events_lost(SESSION_NAME),
        Err(4201),
        "the capture stopped before the final request"
    );
    let entries = wait_entries(baseline, &[1003]);
    let expired = entry(&entries, 1003);
    assert_eq!(expired.level, LEVEL_ERROR, "1003 is an error");
    assert!(
        expired.data.contains(cg_agent::PENDING_FINAL_REQUEST),
        "1003 names the work still pending: {expired:?}"
    );
    let count = expired
        .data
        .split("not delivered: ")
        .nth(1)
        .map(|rest| {
            rest.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
        })
        .unwrap_or_default();
    assert!(
        !count.is_empty(),
        "1003 counts the events not delivered: {expired:?}"
    );
    assert!(
        !entries.iter().any(|e| e.event_id == 1001),
        "an expired deadline is never a clean stop: {entries:#?}"
    );
}

/// Item 2: in service mode a `--config` that is not absolute is a
/// configuration error, code 2, before any file is read.
#[test]
fn install_ac_007_a_relative_config_in_service_mode_is_code_2() {
    let baseline = application_baseline();
    let service = TestService::create("--service --config agent.toml");
    let stopped = service.start_expecting_stop(Duration::from_secs(60));
    assert_stopped_with(&stopped, 2);
    let entries = wait_entries(baseline, &[1002]);
    assert_never_ran(&entries);
    let failed = entry(&entries, 1002);
    assert_eq!(failed.level, LEVEL_ERROR, "1002 is an error");
    assert!(
        failed
            .data
            .contains("--config must be an absolute path in service mode"),
        "1002 carries the cause's line: {failed:?}"
    );
}

/// Items 3 and 4: an invalid configuration stops the service with code 2;
/// entry 1002 names the file and the error's line, and quotes no line of
/// the file (a configuration line can carry a token).
#[test]
fn install_ac_007_an_invalid_config_names_the_file_and_quotes_no_line() {
    const SECRET: &str = "s54-token-value-never-logged";
    // At the fixed path with its production security, so the parse, not
    // the path or the custody check, is what fails (SPEC-024 Amendment
    // 2026-11-04, items 2 and 3).
    let root = FixedDataRoot::plant();
    let config_path = root.paths.config.clone();
    root.write_file(
        &config_path,
        CONFIG_SDDL,
        format!(
            "[server]\n\
             url = \"https://127.0.0.1:9\"\n\
             [enrollment]\n\
             token = \"{SECRET}\n"
        )
        .as_bytes(),
    );
    let baseline = application_baseline();
    let service = TestService::create(&service_args(&config_path));
    let stopped = service.start_expecting_stop(Duration::from_secs(60));
    assert_stopped_with(&stopped, 2);
    let entries = wait_entries(baseline, &[1002]);
    assert_never_ran(&entries);
    let failed = entry(&entries, 1002);
    assert!(
        failed.data.contains(&config_path.display().to_string()),
        "1002 names the file: {failed:?}"
    );
    assert!(
        failed.data.contains("line 4"),
        "1002 gives the parser's line: {failed:?}"
    );
    assert!(
        !entries.iter().any(|e| e.data.contains(SECRET)),
        "no entry quotes a line of the configuration: {entries:#?}"
    );
}

/// install_ac_009, the service half (item 1): with another live process
/// holding the instance lock, the service stops before enrolling with code
/// 10 and entry 1002 carrying the cause's line.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_009_a_held_lock_stops_the_service_with_code_10() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    let heartbeat = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let root = FixedDataRoot::plant();
    let config = write_config(
        &root,
        &enroll.base_url,
        &heartbeat.base_url,
        &pki.trust_anchor_pem,
        &Knobs::default(),
    );
    // The live instance this lock stands for has the agent's own fixed-name
    // session: a refused instance must neither reclaim it nor stop another.
    const OTHER: &str = "CGAgent-UnrelatedSession-install-ac-007";
    let logman = |args: &[&str]| {
        std::process::Command::new("logman")
            .args(args)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    for name in [OTHER, SESSION_NAME] {
        let _ = logman(&["stop", name, "-ets"]);
        assert!(logman(&["start", name, "-ets", "-rt"]), "start {name}");
    }
    let baseline = application_baseline();
    let service = TestService::create(&service_args(&config));
    let held = match instance_lock::acquire() {
        LockOutcome::Acquired(lock) => lock,
        other => panic!("the runner's Administrator takes the lock: {other:?}"),
    };
    let stopped = service.start_expecting_stop(Duration::from_secs(60));
    drop(held);
    let survived: Vec<bool> = [OTHER, SESSION_NAME]
        .iter()
        .map(|name| logman(&["query", name, "-ets"]))
        .collect();
    for name in [OTHER, SESSION_NAME] {
        let _ = logman(&["stop", name, "-ets"]);
    }
    assert_eq!(
        survived,
        vec![true, true],
        "a refused instance stops no ETW session, the agent's own name included"
    );
    assert_stopped_with(&stopped, 10);
    assert_eq!(
        enroll.enroll_received_count(),
        0,
        "it stops before enrolling"
    );
    let entries = wait_entries(baseline, &[1002]);
    assert_never_ran(&entries);
    let failed = entry(&entries, 1002);
    assert!(
        failed.data.contains(STDERR_ANOTHER_INSTANCE),
        "1002 carries the cause's line: {failed:?}"
    );
}
/// SPEC-024 §Data contracts §2 and §Operational §3: the installed service
/// runs the secure path alone; a configuration without `[enrollment]` (the
/// plain HTTP development path), at the fixed path with its production
/// security, stops it with code 2 before `RUNNING` (Amendment 2026-11-04,
/// item 2).
#[test]
fn install_ac_007_the_service_refuses_the_plain_path_with_code_2() {
    let root = FixedDataRoot::plant();
    let config_path = root.paths.config.clone();
    root.write_file(
        &config_path,
        CONFIG_SDDL,
        b"[server]\n\
          url = \"http://127.0.0.1:9\"\n\
          \n\
          [agent]\n\
          id = \"01934abc-def0-7000-89ab-000000000007\"\n\
          hostname = \"TEST-PC\"\n",
    );
    let baseline = application_baseline();
    let service = TestService::create(&service_args(&config_path));
    let stopped = service.start_expecting_stop(Duration::from_secs(60));
    assert_stopped_with(&stopped, 2);
    let entries = wait_entries(baseline, &[1002]);
    assert_never_ran(&entries);
    let failed = entry(&entries, 1002);
    assert!(
        failed.data.contains("requires [enrollment]"),
        "1002 carries the cause's line: {failed:?}"
    );
}

/// Items 3 and 4, §Operational §1 and the Amendment 2026-11-04, item 3: a
/// data directory that another user could write in is refused before the
/// service reads or writes into it. `<ProgramData>\CyberGuard` and its data
/// root inherit `%ProgramData%`'s permissions, as directories an ordinary
/// user created would. The `agent.toml` inside would itself be refused
/// (code 2: it allows BUILTIN\Users to read it), so a code 13 shows the
/// directories are checked before the configuration is read: `STOPPED`
/// carries code 13, entry 1002 the file log's line naming a directory, not
/// `agent.toml`, and `RUNNING` is never reported.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_007_an_untrusted_data_directory_stops_the_service_with_code_13() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    let heartbeat = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let root = FixedDataRoot::plant_inheriting();
    root.write_trust_anchor(&pki.trust_anchor_pem);
    let config = root.paths.config.clone();
    root.write_file(
        &config,
        "O:SYD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FR;;;BU)",
        ConfigFile::fixed(&root.paths, &enroll.base_url, &heartbeat.base_url)
            .toml()
            .as_bytes(),
    );
    assert!(
        parse_sddl(&owner_and_dacl(&config))
            .aces
            .iter()
            .any(|ace| ace.kind == "A" && ace.sid == "BU"),
        "the fixture's agent.toml allows BUILTIN\\Users"
    );
    let baseline = application_baseline();
    let service = TestService::create(&service_args(&config));
    let stopped = service.start_expecting_stop(Duration::from_secs(60));
    assert_stopped_with(&stopped, 13);
    assert_eq!(
        enroll.enroll_received_count(),
        0,
        "it stops before enrolling"
    );
    let entries = wait_entries(baseline, &[1002]);
    assert_never_ran(&entries);
    let failed = entry(&entries, 1002);
    assert!(
        failed.data.contains("the service log cannot be written")
            && failed.data.contains(&text(&root.paths.product_root)),
        "1002 carries the cause's line, naming a data directory: {failed:?}"
    );
    assert!(
        !failed.data.contains("agent.toml"),
        "the directories are refused before agent.toml is checked: {failed:?}"
    );
}

/// install_ac_010 under the SCM: the service's file log is
/// `%ProgramData%\CyberGuard\agent\logs\agent.jsonl`, owned by SYSTEM with
/// a protected DACL that only SYSTEM writes and Administrators read, every
/// line of it valid JSON, and its "agent starting" line carries SPEC-001's
/// fields (debt #137). Its identity line and then its heartbeat line are the
/// two lines configure's observer reads (SPEC-024 Amendment 2026-11-10, item
/// 8): each a JSON line whose `fields.message` is that text.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_010_the_service_log_is_protected_json_lines() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    let heartbeat = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let root = FixedDataRoot::plant();
    let config = write_config(
        &root,
        &enroll.base_url,
        &heartbeat.base_url,
        &pki.trust_anchor_pem,
        &Knobs::default(),
    );
    provision_identity(&root, &pki);
    let service = TestService::create(&service_args(&config));
    service.start().expect("start");
    service.wait_for(SERVICE_RUNNING, Duration::from_secs(60));
    // A second heartbeat is sent after the first one's response, so the
    // first one's line is written by then.
    assert!(
        common::wait_until(Duration::from_secs(60), || online_count(&heartbeat) >= 2).await,
        "the service delivers heartbeats"
    );
    service.control_stop().expect("STOP");
    let stopped = service.wait_for(SERVICE_STOPPED, STOP_DEADLINE + Duration::from_secs(15));
    assert_eq!(stopped.status.dwWin32ExitCode, 0, "a clean stop");

    let logs: PathBuf = root.paths.data_root.join("logs");
    let active = logs.join("agent.jsonl");
    assert_eq!(
        owner_and_dacl(&active),
        "O:SYD:P(A;;FA;;;SY)(A;;FR;;;BA)",
        "SYSTEM owns the log; only SYSTEM writes it and Administrators read it"
    );
    let text = std::fs::read_to_string(&active).expect("Administrators read the log");
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|line| {
            serde_json::from_str(line).unwrap_or_else(|e| panic!("not a JSON line ({e}): {line}"))
        })
        .collect();
    let starting = lines
        .iter()
        .rev()
        .find(|v| v["fields"]["message"] == "agent starting")
        .unwrap_or_else(|| panic!("an \"agent starting\" line: {text:.2000}"));
    let fields = &starting["fields"];
    assert_eq!(fields["agent.version"], cg_agent::AGENT_VERSION);
    assert_eq!(fields["agent.id"], "01934abc-def0-7000-89ab-000000000007");
    assert_eq!(fields["agent.hostname"], "TEST-PC");
    assert_eq!(fields["agent.platform"], "windows");
    assert_eq!(fields["server.url"], enroll.base_url.as_str());
    assert_eq!(fields["heartbeat.interval_seconds"], 1);
    assert_eq!(fields["agent.id_source"], "configuration");
    let ready = lines
        .iter()
        .rev()
        .find(|v| v["fields"]["message"] == "agent identity ready")
        .unwrap_or_else(|| panic!("an \"agent identity ready\" line: {text:.2000}"));
    assert_eq!(
        ready["fields"]["agent.id"],
        common::TEST_AGENT_ID,
        "the identity's id, the server-issued one"
    );
    let identity = lines
        .iter()
        .position(|v| v["fields"]["message"] == "agent identity ready")
        .expect("the identity line");
    let sent = lines
        .iter()
        .skip(identity + 1)
        .find(|v| v["fields"]["message"] == "signed heartbeat sent")
        .unwrap_or_else(|| {
            panic!("a \"signed heartbeat sent\" line after the identity line: {text:.4000}")
        });
    assert_eq!(sent["level"], "INFO", "at the level the service logs at");
    assert!(
        sent["fields"]["response_status"]
            .as_u64()
            .is_some_and(|status| (200..300).contains(&status)),
        "a heartbeat the server accepted: {sent}"
    );
}
