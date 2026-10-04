//! SPEC-017 capture_ac_006 — shutdown.
//!
//! The final POST has status `going_offline` and carries the remaining
//! events up to 1024: the batch in flight, or else the next batch.
//! Synthetic events against the TLS mock; no ETW. On Windows, elevated
//! (the elevated gate), the agent's real ETW session is stopped on
//! shutdown and no session of its name is left behind.

mod common;

use cg_agent::delivery::MAX_BATCH_EVENTS;
use cg_agent::etw::EventRing;
use cg_agent::Capture;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

const AGENT_ID: &str = "01934abc-def0-7000-89ab-0000000000aa";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capture_ac_006_final_post_carries_the_next_batch() {
    let pki = common::generate_test_pki(AGENT_ID);
    let mock = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let ring = Arc::new(EventRing::new(65536));
    let agent =
        common::start_secure_agent(&pki, &mock.base_url, 30, Capture::Ring(Arc::clone(&ring)));
    assert!(common::wait_until(Duration::from_secs(5), || mock.received_count() >= 1).await);

    // Buffered, not yet due (5000 ms latency, 30 s tick) when shutdown comes.
    common::enqueue_launches(&ring, 1..=10);
    tokio::time::sleep(Duration::from_millis(200)).await;
    agent.stop().await.expect("clean stop");

    let received = mock.received();
    let last = received.last().expect("a final POST");
    assert_eq!(common::envelope_status(last), "going_offline");
    assert_eq!(common::envelope_pids(last), (1..=10).collect::<Vec<u64>>());
    assert_eq!(common::envelope_sequence(last), 2);
    assert!(ring.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capture_ac_006_final_post_carries_the_batch_in_flight_up_to_1024() {
    let pki = common::generate_test_pki(AGENT_ID);
    // Event POSTs fail (503) while online; the going_offline one passes.
    let responder: common::Responder = Arc::new(|envelope: &Value| {
        (!common::envelope_events(envelope).is_empty()
            && common::envelope_status(envelope) == "online")
            .then_some(503)
    });
    let mock = common::TlsMockServer::start_with_responder(&pki, responder).await;

    let ring = Arc::new(EventRing::new(65536));
    common::enqueue_launches(&ring, 1..=2000);
    let agent =
        common::start_secure_agent(&pki, &mock.base_url, 30, Capture::Ring(Arc::clone(&ring)));
    assert!(
        common::wait_until(Duration::from_secs(5), || {
            mock.attempts().iter().filter(|a| a.status == 503).count() >= 2
        })
        .await,
        "the first batch must be in flight, retrying"
    );
    agent.stop().await.expect("clean stop");

    let attempts = mock.attempts();
    let in_flight = attempts
        .iter()
        .find(|a| a.status == 503)
        .expect("a retried POST");
    let last = attempts.last().expect("a final POST");
    assert_eq!(last.status, 200);
    assert_eq!(common::envelope_status(&last.envelope), "going_offline");
    assert_eq!(
        common::envelope_sequence(&last.envelope),
        common::envelope_sequence(&in_flight.envelope),
        "the final POST is the batch in flight"
    );
    assert_eq!(
        common::envelope_events(&last.envelope).len(),
        MAX_BATCH_EVENTS
    );
    assert_eq!(
        common::envelope_events(&last.envelope),
        common::envelope_events(&in_flight.envelope)
    );
    // The other 976 were never formed into a POST and are lost.
    assert_eq!(ring.len(), 2000 - MAX_BATCH_EVENTS);
}

#[cfg(windows)]
#[ignore = "real ETW, elevated gate: cargo test -p cg-agent -- --ignored --test-threads=1"]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capture_ac_006_elevated_shutdown_leaves_no_session() {
    use cg_agent::etw::{events_lost, SESSION_NAME};

    let pki = common::generate_test_pki(AGENT_ID);
    let mock = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let agent = common::start_secure_agent(&pki, &mock.base_url, 1, Capture::Platform);
    assert!(
        common::wait_until(Duration::from_secs(10), || mock.received_count() >= 1).await,
        "the agent must open its ETW session and heartbeat (elevated?)"
    );
    assert!(
        events_lost(SESSION_NAME).is_ok(),
        "the agent's session runs while the agent does"
    );

    agent.stop().await.expect("clean stop");

    // ERROR_WMI_INSTANCE_NOT_FOUND: no session of that name is left.
    assert_eq!(events_lost(SESSION_NAME), Err(4201));
    let received = mock.received();
    let last = received.last().expect("a final POST");
    assert_eq!(common::envelope_status(last), "going_offline");
}
