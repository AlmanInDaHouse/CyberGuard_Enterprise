//! SPEC-017 capture_ac_002 — batching.
//!
//! With 1024 or more events buffered, a POST carries exactly 1024 without
//! waiting; a single event is delivered within 5000 ms plus scheduling
//! tolerance; events keep their order across POSTs. Synthetic events are
//! fed to the ring against the TLS mock; no ETW.

mod common;

use cg_agent::delivery::{MAX_BATCH_EVENTS, MAX_BATCH_LATENCY};
use cg_agent::etw::EventRing;
use cg_agent::Capture;
use std::sync::Arc;
use std::time::{Duration, Instant};

const AGENT_ID: &str = "01934abc-def0-7000-89ab-0000000000aa";

/// Heartbeat interval long enough that no tick carries the test events.
const LONG_INTERVAL_SECONDS: u64 = 30;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capture_ac_002_full_batch_without_waiting_and_order_kept() {
    let pki = common::generate_test_pki(AGENT_ID);
    let mock = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let ring = Arc::new(EventRing::new(65536));
    let agent = common::start_secure_agent(
        &pki,
        &mock.base_url,
        LONG_INTERVAL_SECONDS,
        Capture::Ring(Arc::clone(&ring)),
    );

    // The startup tick sends the first heartbeat, without events.
    assert!(common::wait_until(Duration::from_secs(5), || mock.received_count() >= 1).await);

    let enqueued_at = Instant::now();
    common::enqueue_launches(&ring, 1..=1100);

    let full = common::wait_until(Duration::from_secs(2), || {
        mock.received()
            .iter()
            .any(|e| common::envelope_events(e).len() == MAX_BATCH_EVENTS)
    })
    .await;
    let full_after = enqueued_at.elapsed();
    assert!(full, "a POST with exactly 1024 events must go out");
    assert!(
        full_after < Duration::from_secs(1),
        "the full batch must not wait for the latency trigger ({full_after:?})"
    );

    // The remaining 76 follow by the latency trigger.
    let all = common::wait_until(Duration::from_secs(8), || {
        mock.received()
            .iter()
            .map(|e| common::envelope_events(e).len())
            .sum::<usize>()
            >= 1100
    })
    .await;
    assert!(all, "all 1100 events must be delivered");
    agent.stop().await.expect("clean stop");

    let received = mock.received();
    let pids: Vec<u64> = received.iter().flat_map(common::envelope_pids).collect();
    assert_eq!(
        pids,
        (1..=1100).collect::<Vec<u64>>(),
        "order kept across POSTs"
    );
    assert!(received
        .iter()
        .all(|e| common::envelope_events(e).len() <= MAX_BATCH_EVENTS));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capture_ac_002_single_event_within_the_latency_bound() {
    let pki = common::generate_test_pki(AGENT_ID);
    let mock = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let ring = Arc::new(EventRing::new(65536));
    let agent = common::start_secure_agent(
        &pki,
        &mock.base_url,
        LONG_INTERVAL_SECONDS,
        Capture::Ring(Arc::clone(&ring)),
    );
    assert!(common::wait_until(Duration::from_secs(5), || mock.received_count() >= 1).await);

    let enqueued_at = Instant::now();
    common::enqueue_launches(&ring, [4242]);
    let delivered = common::wait_until(Duration::from_secs(8), || {
        mock.received()
            .iter()
            .any(|e| common::envelope_pids(e) == vec![4242])
    })
    .await;
    let elapsed = enqueued_at.elapsed();
    agent.stop().await.expect("clean stop");

    assert!(delivered, "the event must be delivered");
    let tolerance = Duration::from_millis(800);
    assert!(
        elapsed <= MAX_BATCH_LATENCY + tolerance,
        "delivered after {elapsed:?}, over 5000 ms + tolerance"
    );
}
