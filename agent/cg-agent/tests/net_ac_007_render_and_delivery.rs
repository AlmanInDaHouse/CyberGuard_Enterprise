//! SPEC-019 net_ac_007 — render and delivery.
//!
//! A ring holding process and network events interleaved is delivered in
//! that order, each element in its class's shape (§Data contracts), with
//! `actor.process.uid` omitted where it is unknown; after a transient
//! failure the resent elements are byte-identical. Synthetic events
//! against the TLS mock; no ETW.

mod common;

use cg_agent::etw::{format_process_uid, Direction, EventRing, NetworkEvent};
use cg_agent::Capture;
use serde_json::Value;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

const CREATED_NS: u64 = 1_791_590_400_000_000_000;

fn connection(pid: u32, created: Option<u64>, src: &str, dst: &str, inbound: bool) -> NetworkEvent {
    NetworkEvent {
        event_id: uuid::Uuid::now_v7().to_string(),
        pid,
        direction: if inbound {
            Direction::Inbound
        } else {
            Direction::Outbound
        },
        src: src.parse().unwrap(),
        dst: dst.parse().unwrap(),
        etw_timestamp_nanos: CREATED_NS + 5_000,
        created_time_nanos: created,
    }
}

fn keys(element: &Value) -> BTreeSet<String> {
    element.as_object().unwrap().keys().cloned().collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net_ac_007_mixed_classes_in_order_and_byte_identical_on_retry() {
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
    ring.enqueue_or_drop(common::synthetic_launch(1));
    ring.enqueue_or_drop(connection(
        4321,
        Some(CREATED_NS),
        "192.0.2.10:49213",
        "198.51.100.7:443",
        false,
    ));
    ring.enqueue_or_drop(common::synthetic_launch(2));
    ring.enqueue_or_drop(connection(
        1234,
        None,
        "[2001:db8::7]:50123",
        "[2001:db8::10]:8443",
        true,
    ));
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

    // Byte-identical across the retry.
    let first = serde_json::to_string(&common::envelope_events(&with_events[0].envelope)).unwrap();
    let second = serde_json::to_string(&common::envelope_events(&with_events[1].envelope)).unwrap();
    assert_eq!(first, second, "a retry carries byte-identical elements");

    // In the order they entered the ring, each in its class's shape.
    let elements = common::envelope_events(&with_events[1].envelope);
    let classes: Vec<u64> = elements
        .iter()
        .map(|e| e["class_uid"].as_u64().unwrap())
        .collect();
    assert_eq!(classes, vec![1007, 4001, 1007, 4001]);
    assert_eq!(elements[0].pointer("/process/pid").unwrap(), 1);
    assert_eq!(elements[2].pointer("/process/pid").unwrap(), 2);

    let network_keys: BTreeSet<String> = [
        "event_id",
        "class_uid",
        "activity_id",
        "time",
        "src_endpoint",
        "dst_endpoint",
        "connection_info",
        "actor",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    let outbound = &elements[1];
    assert_eq!(keys(outbound), network_keys);
    assert_eq!(outbound["activity_id"], 1);
    assert_eq!(outbound["time"], (CREATED_NS + 5_000).to_string());
    assert_eq!(
        outbound["src_endpoint"],
        serde_json::json!({"ip": "192.0.2.10", "port": 49213})
    );
    assert_eq!(
        outbound["dst_endpoint"],
        serde_json::json!({"ip": "198.51.100.7", "port": 443})
    );
    assert_eq!(
        outbound["connection_info"],
        serde_json::json!({"protocol_name": "tcp", "direction": "outbound"})
    );
    assert_eq!(
        outbound["actor"],
        serde_json::json!({"process": {
            "pid": 4321,
            "uid": format_process_uid(common::TEST_AGENT_ID, 4321, CREATED_NS),
        }})
    );

    let inbound = &elements[3];
    assert_eq!(keys(inbound), network_keys);
    assert_eq!(
        inbound["src_endpoint"],
        serde_json::json!({"ip": "2001:db8::7", "port": 50123})
    );
    assert_eq!(inbound["connection_info"]["direction"], "inbound");
    assert_eq!(
        inbound["actor"],
        serde_json::json!({"process": {"pid": 1234}}),
        "uid is omitted where it is unknown"
    );
}
