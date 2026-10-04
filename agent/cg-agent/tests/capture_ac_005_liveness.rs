//! SPEC-017 capture_ac_005 — liveness.
//!
//! Without events, one POST per tick on the absolute timeline
//! (`start_time + k · interval`); a POST with events since the previous
//! tick suppresses the empty one; `sequence_number` grows by one per
//! POST. Synthetic events against the TLS mock; no ETW.

mod common;

use cg_agent::delivery::MAX_BATCH_EVENTS;
use cg_agent::etw::EventRing;
use cg_agent::Capture;
use std::sync::Arc;
use std::time::{Duration, Instant};

const AGENT_ID: &str = "01934abc-def0-7000-89ab-0000000000aa";
const INTERVAL_SECONDS: u64 = 2;
const TOLERANCE: Duration = Duration::from_millis(600);

fn near(actual: Duration, expected: Duration) -> bool {
    actual + TOLERANCE >= expected && actual <= expected + TOLERANCE
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capture_ac_005_one_post_per_tick_and_event_posts_suppress_the_empty_one() {
    let pki = common::generate_test_pki(AGENT_ID);
    let mock = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let ring = Arc::new(EventRing::new(65536));
    let started = Instant::now();
    let agent = common::start_secure_agent(
        &pki,
        &mock.base_url,
        INTERVAL_SECONDS,
        Capture::Ring(Arc::clone(&ring)),
    );

    // Ticks at 0, 2 and 4 s: three POSTs without events.
    tokio::time::sleep(Duration::from_millis(4600)).await;
    // A full batch goes out at once (between the ticks at 4 and 6 s)...
    common::enqueue_launches(&ring, 1..=MAX_BATCH_EVENTS as u32);
    // ...so the tick at 6 s sends nothing, and the tick at 8 s does.
    tokio::time::sleep(Duration::from_millis(4000)).await;
    agent.stop().await.expect("clean stop");

    let posts: Vec<common::MockAttempt> = mock
        .attempts()
        .into_iter()
        .filter(|a| common::envelope_status(&a.envelope) == "online")
        .collect();
    let summary: Vec<(u64, usize, Duration)> = posts
        .iter()
        .map(|a| {
            (
                common::envelope_sequence(&a.envelope),
                common::envelope_events(&a.envelope).len(),
                a.at.duration_since(started),
            )
        })
        .collect();
    assert_eq!(posts.len(), 5, "POSTs (seq, events, at): {summary:?}");

    let sequences: Vec<u64> = summary.iter().map(|(s, _, _)| *s).collect();
    assert_eq!(
        sequences,
        vec![1, 2, 3, 4, 5],
        "one sequence_number per POST"
    );

    let interval = Duration::from_secs(INTERVAL_SECONDS);
    for (k, (_, events, at)) in summary.iter().take(3).enumerate() {
        assert_eq!(*events, 0, "tick POST {k} carries no events");
        assert!(
            near(*at, interval * k as u32),
            "tick POST {k} at {at:?}: {summary:?}"
        );
    }
    let (_, events, at) = summary[3];
    assert_eq!(events, MAX_BATCH_EVENTS, "the batch POST: {summary:?}");
    assert!(near(at, Duration::from_millis(4600)), "{summary:?}");
    let (_, events, at) = summary[4];
    assert_eq!(events, 0);
    assert!(
        near(at, interval * 4),
        "the empty POST at 6 s is suppressed; the next is at 8 s: {summary:?}"
    );
}
