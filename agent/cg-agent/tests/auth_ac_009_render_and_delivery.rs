//! SPEC-020 auth_ac_009 — render and delivery (§Operational §5).
//!
//! A ring holding process, network and logon events interleaved is
//! delivered in that order against the TLS mock, each element in its
//! class's shape; after a transient failure the resent elements are
//! identical. Synthetic events; no Windows.

mod common;
mod logon_records;

use cg_agent::etw::{Direction, EventRing, NetworkEvent};
use cg_agent::Capture;
use logon_records::{failure, reported, success};
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn auth_ac_009_three_classes_in_order_and_identical_on_retry() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    // The first POST that carries events is answered 503.
    let failures = Arc::new(AtomicUsize::new(0));
    let failures_in_responder = Arc::clone(&failures);
    let responder: common::Responder = Arc::new(move |envelope: &Value| {
        if common::envelope_events(envelope).is_empty() {
            return None;
        }
        (failures_in_responder.fetch_add(1, Ordering::SeqCst) < 1).then_some(503)
    });
    let mock = common::TlsMockServer::start_with_responder(&pki, responder).await;

    let ring = Arc::new(EventRing::new(65536));
    let logon_success = reported(&success());
    let logon_failure = reported(&failure());
    ring.enqueue_or_drop(common::synthetic_launch(1));
    ring.enqueue_or_drop(logon_success.clone());
    ring.enqueue_or_drop(NetworkEvent {
        event_id: uuid::Uuid::now_v7().to_string(),
        pid: 1,
        direction: Direction::Outbound,
        src: "127.0.0.1:50000".parse().unwrap(),
        dst: "127.0.0.1:445".parse().unwrap(),
        etw_timestamp_nanos: 1_791_590_400_000_000_000,
        created_time_nanos: None,
    });
    ring.enqueue_or_drop(logon_failure.clone());
    let agent =
        common::start_secure_agent(&pki, &mock.base_url, 30, Capture::Ring(Arc::clone(&ring)));

    let delivered = common::wait_until(Duration::from_secs(10), || {
        mock.attempts()
            .iter()
            .any(|a| a.status == 200 && !common::envelope_events(&a.envelope).is_empty())
    })
    .await;
    agent.stop().await.expect("clean stop");
    assert!(delivered, "the batch must be delivered after the 503");

    let with_events: Vec<common::MockAttempt> = mock
        .attempts()
        .into_iter()
        .filter(|a| !common::envelope_events(&a.envelope).is_empty())
        .collect();
    assert_eq!(
        with_events.iter().map(|a| a.status).collect::<Vec<_>>()[..2],
        [503, 200]
    );
    assert_eq!(
        common::envelope_events(&with_events[0].envelope),
        common::envelope_events(&with_events[1].envelope),
        "a retry carries the same elements"
    );

    let elements = common::envelope_events(&with_events[1].envelope);
    let classes: Vec<u64> = elements
        .iter()
        .map(|e| e["class_uid"].as_u64().unwrap())
        .collect();
    assert_eq!(classes, vec![1007, 3002, 4001, 3002]);

    let success_element = &elements[1];
    assert_eq!(success_element["event_id"], logon_success.event_id.as_str());
    assert_eq!(success_element["category_uid"], 3);
    assert_eq!(success_element["status_id"], 1);
    assert_eq!(
        success_element["user"]["uid"],
        "S-1-5-21-1111-2222-3333-1001"
    );
    assert_eq!(success_element["cg_elevated_token"], false);
    assert!(success_element.get("status_code").is_none());

    let failure_element = &elements[3];
    assert_eq!(failure_element["event_id"], logon_failure.event_id.as_str());
    assert_eq!(failure_element["status_id"], 2);
    assert_eq!(failure_element["user"]["name"], "<withheld>");
    assert_eq!(failure_element["status_detail"], "0xc0000064");
    assert!(failure_element.get("cg_elevated_token").is_none());
}
