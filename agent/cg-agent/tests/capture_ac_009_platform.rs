//! SPEC-017 capture_ac_009 — platform.
//!
//! On a non-Windows build the secure path has no capture backend: it
//! sends heartbeats only and logs the notice once, at `info`. (The
//! `mtls_ac_001`–`009` suites run the secure path with `Capture::Off`.)
//! This binary holds one test: it installs a global log subscriber.

#![cfg(not(windows))]

mod common;

use cg_agent::Capture;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing_subscriber::fmt::MakeWriter;

const AGENT_ID: &str = "01934abc-def0-7000-89ab-0000000000aa";
const NOTICE: &str = "process capture is not available on this platform";

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
async fn capture_ac_009_no_backend_sends_heartbeats_and_logs_once() {
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
    let mock = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let agent = common::start_secure_agent(&pki, &mock.base_url, 1, Capture::Platform);

    assert!(
        common::wait_until(Duration::from_secs(5), || mock.received_count() >= 2).await,
        "heartbeats must flow without a capture backend"
    );
    agent.stop().await.expect("clean stop");

    assert!(mock
        .received()
        .iter()
        .all(|e| common::envelope_events(e).is_empty()));
    let log_text = String::from_utf8_lossy(&logs.lock().unwrap()).to_string();
    let notices: Vec<&str> = log_text.lines().filter(|l| l.contains(NOTICE)).collect();
    assert_eq!(notices.len(), 1, "the notice is logged once: {notices:?}");
    assert!(notices[0].contains("\"level\":\"INFO\""));
}
