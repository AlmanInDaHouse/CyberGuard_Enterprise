//! SPEC-017 capture_ac_004 — rejection.
//!
//! A 4xx is retried up to `max_retries` attempts, then the batch is
//! dropped, counted in the dropped total and logged at `error`, and the
//! next batch is sent. Synthetic events against the TLS mock; no ETW.
//! This binary holds one test: it installs a global log subscriber.

mod common;

use cg_agent::etw::EventRing;
use cg_agent::Capture;
use serde_json::Value;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing_subscriber::fmt::MakeWriter;

const AGENT_ID: &str = "01934abc-def0-7000-89ab-0000000000aa";

#[derive(Clone)]
struct BufferWriter {
    buf: Arc<Mutex<Vec<u8>>>,
}

impl io::Write for BufferWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buf.lock().unwrap().extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for BufferWriter {
    type Writer = BufferWriter;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capture_ac_004_rejected_batch_is_dropped_counted_and_logged() {
    let logs = Arc::new(Mutex::new(Vec::<u8>::new()));
    tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .with_writer(BufferWriter {
                buf: Arc::clone(&logs),
            })
            .with_max_level(tracing::Level::INFO)
            .json()
            .finish(),
    )
    .expect("one global subscriber per test binary");

    let pki = common::generate_test_pki(AGENT_ID);
    // Batch A (pids below 1000) is always rejected 400.
    let responder: common::Responder = Arc::new(|envelope: &Value| {
        common::envelope_pids(envelope)
            .iter()
            .any(|&pid| pid < 1000)
            .then_some(400)
    });
    let mock = common::TlsMockServer::start_with_responder(&pki, responder).await;

    let ring = Arc::new(EventRing::new(65536));
    common::enqueue_launches(&ring, 1..=3);
    let agent =
        common::start_secure_agent(&pki, &mock.base_url, 1, Capture::Ring(Arc::clone(&ring)));

    let rejected =
        |mock: &common::TlsMockServer| mock.attempts().iter().filter(|a| a.status == 400).count();
    assert!(
        common::wait_until(Duration::from_secs(5), || rejected(&mock) >= 3).await,
        "batch A must be attempted max_retries = 3 times"
    );
    // Batch B, after A was given up.
    tokio::time::sleep(Duration::from_millis(300)).await;
    common::enqueue_launches(&ring, 1001..=1002);
    let delivered = common::wait_until(Duration::from_secs(8), || {
        mock.received()
            .iter()
            .any(|e| common::envelope_pids(e) == vec![1001, 1002])
    })
    .await;
    agent.stop().await.expect("clean stop");
    assert!(delivered, "the next batch must be sent and delivered");

    let attempts = mock.attempts();
    let a: Vec<&common::MockAttempt> = attempts
        .iter()
        .filter(|a| common::envelope_pids(&a.envelope) == vec![1, 2, 3])
        .collect();
    assert_eq!(
        a.len(),
        3,
        "exactly max_retries attempts for the rejected batch"
    );
    assert!(a.iter().all(|a| a.status == 400));
    let a_sequence = common::envelope_sequence(&a[0].envelope);
    assert!(a
        .iter()
        .all(|x| common::envelope_sequence(&x.envelope) == a_sequence));

    assert_eq!(
        ring.events_dropped_total(),
        3,
        "the rejected batch's events join the dropped total"
    );

    let log_text = String::from_utf8_lossy(&logs.lock().unwrap()).to_string();
    let dropped_line = log_text
        .lines()
        .find(|l| l.contains("batch dropped after repeated rejections"))
        .expect("one error line records the drop");
    assert!(
        dropped_line.contains("\"level\":\"ERROR\""),
        "{dropped_line}"
    );
    assert!(
        dropped_line.contains("\"response_status\":400"),
        "{dropped_line}"
    );
    assert!(
        dropped_line.contains("\"events_dropped\":3"),
        "{dropped_line}"
    );
}
