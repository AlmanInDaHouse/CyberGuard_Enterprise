//! SPEC-024 install_ac_007, the parts that need no service control manager
//! (§Test scenarios' "injected shutdown deadlines"): the stop's budget over
//! the capture teardown and the final request, the enrollment rounds of
//! service mode and their responsiveness to a stop (SPEC-002 Amendment
//! 2026-10-27), and the service's outcomes and Application-log entries
//! (the second Amendment 2026-11-01, items 3 and 4). The real service, as
//! SYSTEM under the SCM, is `install_ac_007_service.rs`.

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use cg_agent::errors::{ConfigError, EnrollmentError};
use cg_agent::etw::EventRing;
use cg_agent::service::{
    self, ServiceExit, ENROLLMENT_ROUND_WAIT, EVENT_DEADLINE, EVENT_FAILED, EVENT_RUNNING,
    EVENT_STOPPED, STOP_DEADLINE,
};
use cg_agent::{Capture, StopOutcome, PENDING_FINAL_REQUEST};

const AGENT_ID: &str = "01934abc-def0-7000-89ab-0000000000b7";

/// A secure run over a ring the test fills, against `mock`; the returned
/// sender stops it.
fn start_run(
    fixture: common::SecureFixture,
    ring: Arc<EventRing>,
    budget: Option<Duration>,
) -> (
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<Result<StopOutcome, cg_agent::errors::AgentError>>,
    tempfile::NamedTempFile,
) {
    let common::SecureFixture {
        config,
        identity,
        trust_anchor,
    } = fixture;
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let run = tokio::spawn(cg_agent::run_secure_within(
        config,
        identity,
        Capture::Ring(ring),
        async move {
            let _ = stopped.await;
        },
        budget,
    ));
    (stop, run, trust_anchor)
}

fn stalling_final_request() -> common::Responder {
    Arc::new(|envelope| {
        (common::envelope_status(envelope) == "going_offline").then_some(common::STALL)
    })
}

/// The service's budget bounds a final request whose own timeout is
/// longer: the stop ends at the budget, reported as expired with the
/// request pending and its events not delivered — never as completed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn install_ac_007_the_budget_bounds_a_stalled_final_request() {
    let pki = common::generate_test_pki(AGENT_ID);
    let mock = common::TlsMockServer::start_with_responder(&pki, stalling_final_request()).await;
    let mut fixture = common::secure_fixture(&mock.base_url, &pki);
    fixture.config.heartbeat.request_timeout_seconds = 30;
    let ring = Arc::new(EventRing::new(65536));
    let (stop, run, _anchor) = start_run(fixture, Arc::clone(&ring), Some(Duration::from_secs(2)));
    assert!(
        common::wait_until(Duration::from_secs(10), || mock.received_count() >= 1).await,
        "a first heartbeat"
    );
    common::enqueue_launches(&ring, 1..=5);

    let stop_sent = Instant::now();
    stop.send(()).expect("stop");
    let outcome = tokio::time::timeout(Duration::from_secs(20), run)
        .await
        .expect("the stop ends")
        .expect("join")
        .expect("a stop outcome");
    let took = stop_sent.elapsed();
    assert!(
        took >= Duration::from_millis(1900) && took < Duration::from_secs(6),
        "the stop ends at its 2 s budget, not the 30 s request timeout: {took:?}"
    );
    assert_eq!(
        outcome,
        StopOutcome::DeadlineExpired {
            pending: PENDING_FINAL_REQUEST,
            undelivered: 5,
        }
    );
}

/// Without a budget (the console), the stop waits out the final request's
/// own timeout and completes, counting the events lost: Ctrl+C keeps its
/// semantics (SPEC-017 Amendment 2026-10-27).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn install_ac_007_without_a_budget_the_stop_waits_the_request_timeout() {
    let pki = common::generate_test_pki(AGENT_ID);
    let mock = common::TlsMockServer::start_with_responder(&pki, stalling_final_request()).await;
    let mut fixture = common::secure_fixture(&mock.base_url, &pki);
    fixture.config.heartbeat.request_timeout_seconds = 2;
    let ring = Arc::new(EventRing::new(65536));
    let (stop, run, _anchor) = start_run(fixture, Arc::clone(&ring), None);
    assert!(
        common::wait_until(Duration::from_secs(10), || mock.received_count() >= 1).await,
        "a first heartbeat"
    );
    common::enqueue_launches(&ring, 1..=5);

    let stop_sent = Instant::now();
    stop.send(()).expect("stop");
    let outcome = tokio::time::timeout(Duration::from_secs(20), run)
        .await
        .expect("the stop ends")
        .expect("join")
        .expect("a stop outcome");
    assert!(
        stop_sent.elapsed() >= Duration::from_millis(1900),
        "the request timeout is waited out"
    );
    assert_eq!(outcome, StopOutcome::Completed { undelivered: 5 });
}

/// A final request that fails within the budget (its own timeout is
/// shorter) is a completed stop, its events counted as lost; the budget is
/// not waited out.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn install_ac_007_a_final_request_failing_within_the_budget_completes() {
    let pki = common::generate_test_pki(AGENT_ID);
    let mock = common::TlsMockServer::start_with_responder(&pki, stalling_final_request()).await;
    let mut fixture = common::secure_fixture(&mock.base_url, &pki);
    fixture.config.heartbeat.request_timeout_seconds = 1;
    let ring = Arc::new(EventRing::new(65536));
    let (stop, run, _anchor) = start_run(fixture, Arc::clone(&ring), Some(Duration::from_secs(30)));
    assert!(
        common::wait_until(Duration::from_secs(10), || mock.received_count() >= 1).await,
        "a first heartbeat"
    );
    common::enqueue_launches(&ring, 1..=3);

    let stop_sent = Instant::now();
    stop.send(()).expect("stop");
    let outcome = tokio::time::timeout(Duration::from_secs(20), run)
        .await
        .expect("the stop ends")
        .expect("join")
        .expect("a stop outcome");
    assert!(
        stop_sent.elapsed() < Duration::from_secs(10),
        "the 30 s budget is not waited out"
    );
    assert_eq!(outcome, StopOutcome::Completed { undelivered: 3 });
}

/// A delivered final request completes the stop with nothing lost.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn install_ac_007_a_delivered_final_request_completes_with_nothing_lost() {
    let pki = common::generate_test_pki(AGENT_ID);
    let mock = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let fixture = common::secure_fixture(&mock.base_url, &pki);
    let ring = Arc::new(EventRing::new(65536));
    let (stop, run, _anchor) = start_run(fixture, Arc::clone(&ring), Some(STOP_DEADLINE));
    assert!(
        common::wait_until(Duration::from_secs(10), || mock.received_count() >= 1).await,
        "a first heartbeat"
    );
    common::enqueue_launches(&ring, 1..=4);
    stop.send(()).expect("stop");
    let outcome = tokio::time::timeout(Duration::from_secs(20), run)
        .await
        .expect("the stop ends")
        .expect("join")
        .expect("a stop outcome");
    assert_eq!(outcome, StopOutcome::Completed { undelivered: 0 });
    let last = mock.received();
    let last = last.last().expect("a final POST");
    assert_eq!(common::envelope_status(last), "going_offline");
    assert_eq!(common::envelope_pids(last), vec![1, 2, 3, 4]);
}

/// SPEC-002 Amendment 2026-10-27: in service mode an exhausted enrollment
/// round is followed by another after the round wait, not by an exit; a
/// stop during the wait ends the rounds at once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn install_ac_007_enrollment_rounds_wait_and_end_on_a_stop() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let mock = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    mock.set_enroll_default(503);
    let mut fixture = common::enrollment_fixture(&mock.base_url, "tok-install-ac-007", &pki);
    fixture
        .config
        .enrollment
        .as_mut()
        .expect("[enrollment]")
        .max_retries = 1;

    let started = Instant::now();
    let stop = std::pin::pin!(tokio::time::sleep(Duration::from_millis(3500)));
    let outcome = service::ensure_identity_until_stopped(
        &fixture.config,
        &fixture.config_path,
        Duration::from_secs(1),
        stop,
    )
    .await;
    let took = started.elapsed();
    assert!(
        outcome.is_none(),
        "a stop ends the rounds without an outcome"
    );
    assert!(
        took >= Duration::from_millis(3400) && took < Duration::from_secs(5),
        "the rounds run until the stop: {took:?}"
    );
    let rounds = mock.enroll_received_count();
    assert!(
        (3..=4).contains(&rounds),
        "one request per round, a round every 1 s: {rounds}"
    );
    assert!(
        fixture.no_artifacts_exist(),
        "no identity without enrollment"
    );
}

/// A round that succeeds after an exhausted one returns the identity.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn install_ac_007_a_later_round_enrolls() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let mock = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    mock.set_enroll_plan(vec![503]);
    let mut fixture = common::enrollment_fixture(&mock.base_url, "tok-install-ac-007", &pki);
    fixture
        .config
        .enrollment
        .as_mut()
        .expect("[enrollment]")
        .max_retries = 1;

    let stop = std::pin::pin!(tokio::time::sleep(Duration::from_secs(30)));
    let outcome = service::ensure_identity_until_stopped(
        &fixture.config,
        &fixture.config_path,
        Duration::from_millis(500),
        stop,
    )
    .await;
    let identity = outcome
        .expect("not stopped")
        .expect("the second round enrolls");
    assert_eq!(identity.agent_id, common::TEST_AGENT_ID);
    assert_eq!(mock.enroll_received_count(), 2, "one request per round");
}

/// A permanent enrollment error ends the rounds at once, without waiting
/// for another round: it needs an operator (SPEC-024 §Operational §3).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn install_ac_007_a_permanent_enrollment_error_ends_the_rounds() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let mock = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    mock.set_enroll_default(401);
    let fixture = common::enrollment_fixture(&mock.base_url, "tok-install-ac-007", &pki);

    let started = Instant::now();
    let stop = std::pin::pin!(tokio::time::sleep(Duration::from_secs(120)));
    let outcome = service::ensure_identity_until_stopped(
        &fixture.config,
        &fixture.config_path,
        ENROLLMENT_ROUND_WAIT,
        stop,
    )
    .await;
    match outcome {
        Some(Err(EnrollmentError::Refused(_))) => {}
        other => panic!(
            "a refused token ends the rounds: {:?}",
            other.map(|r| r.err())
        ),
    }
    assert!(started.elapsed() < Duration::from_secs(10), "no round wait");
    assert_eq!(mock.enroll_received_count(), 1);
}

/// A stop during a retry's backoff, inside a round, ends the rounds at
/// once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn install_ac_007_a_stop_during_a_retry_backoff_is_prompt() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let mock = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    mock.set_enroll_default(503);
    let mut fixture = common::enrollment_fixture(&mock.base_url, "tok-install-ac-007", &pki);
    let enrollment = fixture.config.enrollment.as_mut().expect("[enrollment]");
    enrollment.max_retries = 3;
    enrollment.backoff_initial_ms = 60_000;

    let started = Instant::now();
    let stop = std::pin::pin!(tokio::time::sleep(Duration::from_secs(1)));
    let outcome = service::ensure_identity_until_stopped(
        &fixture.config,
        &fixture.config_path,
        ENROLLMENT_ROUND_WAIT,
        stop,
    )
    .await;
    assert!(outcome.is_none(), "stopped");
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "the 60 s backoff is not waited out: {:?}",
        started.elapsed()
    );
    assert_eq!(mock.enroll_received_count(), 1);
}

/// Item 4: a configuration error's entry names the file and the parser's
/// line and column and quotes no line of the file, which can carry a
/// token; the console's own message is unchanged.
#[test]
fn install_ac_007_a_config_error_line_quotes_no_line_of_the_file() {
    const SECRET: &str = "s54-token-value-never-logged";
    let config = common::write_temp_config(&format!(
        "[server]\n\
         url = \"https://127.0.0.1:9\"\n\
         [enrollment]\n\
         token = \"{SECRET}\n"
    ));
    let error = cg_agent::config::load_from_path(config.path()).expect_err("unparsable");
    match &error {
        ConfigError::Parse {
            position: Some((line, _)),
            ..
        } => assert_eq!(*line, 4, "the parser's line"),
        other => panic!("a parse error with a position: {other:?}"),
    }
    let line = service::config_error_line(config.path(), &error);
    assert!(
        line.contains(&config.path().display().to_string()),
        "names the file: {line}"
    );
    assert!(line.contains("line 4"), "gives the line: {line}");
    assert!(!line.contains(SECRET), "quotes no line of the file: {line}");
    assert!(
        error
            .to_string()
            .starts_with("failed to parse config TOML: "),
        "the console's message is unchanged: {error}"
    );
}

/// Items 3 and 4: what `STOPPED` carries and which entry records each end.
#[test]
fn install_ac_007_outcomes_carry_their_codes_and_entries() {
    assert_eq!(STOP_DEADLINE, Duration::from_secs(60));
    assert!(ENROLLMENT_ROUND_WAIT >= Duration::from_secs(60));

    let clean = ServiceExit::Clean { undelivered: 2 };
    assert_eq!(clean.specific_code(), None, "code 0");
    let entry = clean.entry();
    assert_eq!((entry.id, entry.error), (EVENT_STOPPED, false));
    assert!(entry.text.contains("not delivered: 2"), "{}", entry.text);

    let failed = ServiceExit::Failed {
        code: 10,
        line: cg_agent::instance_lock::STDERR_ANOTHER_INSTANCE.to_string(),
    };
    assert_eq!(failed.specific_code(), Some(10));
    let entry = failed.entry();
    assert_eq!((entry.id, entry.error), (EVENT_FAILED, true));
    assert!(entry.text.contains("code 10"), "{}", entry.text);
    assert!(
        entry
            .text
            .contains(cg_agent::instance_lock::STDERR_ANOTHER_INSTANCE),
        "{}",
        entry.text
    );

    let expired = ServiceExit::DeadlineExpired {
        pending: PENDING_FINAL_REQUEST,
        undelivered: 7,
    };
    assert_eq!(expired.specific_code(), Some(12));
    let entry = expired.entry();
    assert_eq!((entry.id, entry.error), (EVENT_DEADLINE, true));
    assert!(entry.text.contains(PENDING_FINAL_REQUEST), "{}", entry.text);
    assert!(entry.text.contains("not delivered: 7"), "{}", entry.text);

    let running = service::running_entry();
    assert_eq!((running.id, running.error), (EVENT_RUNNING, false));
    assert!(running.text.contains(cg_agent::AGENT_VERSION));
}
