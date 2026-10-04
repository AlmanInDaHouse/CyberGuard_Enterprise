//! SPEC-017 capture_ac_003 — at-least-once.
//!
//! After a 5xx or a transient connection failure the same POST is
//! retried: same `sequence_number`, events byte-identical, a fresh
//! `nonce`; no later event is sent before it; it is delivered once the
//! server recovers. Synthetic events against the TLS mock; no ETW.

mod common;

use cg_agent::etw::EventRing;
use cg_agent::Capture;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

const AGENT_ID: &str = "01934abc-def0-7000-89ab-0000000000aa";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capture_ac_003_5xx_retries_the_same_post_in_order() {
    let pki = common::generate_test_pki(AGENT_ID);
    // The first two POSTs that carry events are answered 503.
    let failures = Arc::new(AtomicUsize::new(0));
    let failures_in_responder = Arc::clone(&failures);
    let responder: common::Responder = Arc::new(move |envelope: &Value| {
        if common::envelope_events(envelope).is_empty() {
            return None;
        }
        (failures_in_responder.fetch_add(1, Ordering::SeqCst) < 2).then_some(503)
    });
    let mock = common::TlsMockServer::start_with_responder(&pki, responder).await;

    // Batch A is buffered before the agent starts: the startup POST carries it.
    let ring = Arc::new(EventRing::new(65536));
    common::enqueue_launches(&ring, 1..=3);
    let agent =
        common::start_secure_agent(&pki, &mock.base_url, 30, Capture::Ring(Arc::clone(&ring)));
    // Batch B arrives while A is being retried.
    assert!(
        common::wait_until(Duration::from_secs(5), || !mock.attempts().is_empty()).await,
        "the first attempt must reach the mock"
    );
    common::enqueue_launches(&ring, 10..=11);

    let delivered = common::wait_until(Duration::from_secs(10), || {
        mock.received()
            .iter()
            .any(|e| common::envelope_pids(e) == vec![10, 11])
    })
    .await;
    agent.stop().await.expect("clean stop");
    assert!(delivered, "batch B must be delivered after A");

    let with_events: Vec<common::MockAttempt> = mock
        .attempts()
        .into_iter()
        .filter(|a| !common::envelope_events(&a.envelope).is_empty())
        .collect();
    let statuses: Vec<u16> = with_events.iter().map(|a| a.status).collect();
    assert_eq!(&statuses[..4], &[503, 503, 200, 200], "{statuses:?}");

    let a_attempts = &with_events[..3];
    let sequence = common::envelope_sequence(&a_attempts[0].envelope);
    let events = serde_json::to_string(&common::envelope_events(&a_attempts[0].envelope)).unwrap();
    let mut nonces = HashSet::new();
    for attempt in a_attempts {
        assert_eq!(common::envelope_sequence(&attempt.envelope), sequence);
        assert_eq!(
            serde_json::to_string(&common::envelope_events(&attempt.envelope)).unwrap(),
            events,
            "a retry carries byte-identical events"
        );
        assert_eq!(common::envelope_pids(&attempt.envelope), vec![1, 2, 3]);
        nonces.insert(attempt.envelope["nonce"].as_str().unwrap().to_string());
    }
    assert_eq!(nonces.len(), 3, "each attempt has a fresh nonce");

    let b = &with_events[3];
    assert_eq!(common::envelope_pids(&b.envelope), vec![10, 11]);
    assert_eq!(
        common::envelope_sequence(&b.envelope),
        sequence + 1,
        "B is the next POST, sent only after A was delivered"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capture_ac_003_connection_failure_retries_until_the_server_is_up() {
    let pki = common::generate_test_pki(AGENT_ID);
    // A free local port with nothing listening yet.
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        listener.local_addr().expect("addr").port()
    };
    let url = format!("https://127.0.0.1:{port}");

    let ring = Arc::new(EventRing::new(65536));
    common::enqueue_launches(&ring, 1..=2);
    let agent = common::start_secure_agent(&pki, &url, 30, Capture::Ring(Arc::clone(&ring)));

    // Connection refused for a while, then the server comes up.
    tokio::time::sleep(Duration::from_millis(700)).await;
    let mock = common::TlsMockServer::start_on_port(&pki, port).await;

    let delivered = common::wait_until(Duration::from_secs(8), || {
        mock.received()
            .iter()
            .any(|e| common::envelope_pids(e) == vec![1, 2])
    })
    .await;
    agent.stop().await.expect("clean stop");
    assert!(
        delivered,
        "the batch must be delivered once the server is up"
    );

    let received = mock.received();
    let first = &received[0];
    assert_eq!(common::envelope_pids(first), vec![1, 2]);
    assert_eq!(
        common::envelope_sequence(first),
        1,
        "the retried POST keeps its sequence_number"
    );
}
