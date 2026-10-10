//! SPEC-020 auth_ac_010 — the agent's own output (§Operational §11).
//!
//! With the agent's log captured at every level, logon records whose user,
//! SID, domain, workstation and address are distinctive strings go through
//! the decoding, the ring and the delivery against the TLS mock, and
//! through the counting and the `warn` line of unusable records, and a
//! pre-1970 record through its `error` line; none of those strings appears
//! in the captured log.

mod common;
mod logon_records;

use cg_agent::etw::EventRing;
use cg_agent::logon::{dispatch_logon_record, LogonCounters, RawLogonRecord, UnusableMonitor};
use cg_agent::Capture;
use logon_records::{failure, success};
use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing_subscriber::fmt::MakeWriter;

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

const USER: &str = "zz-distinctive-user";
const SID: &str = "S-1-5-21-7777-8888-9999-4242";
const DOMAIN: &str = "ZZ-DISTINCTIVE-DOMAIN";
const WORKSTATION: &str = "ZZ-DISTINCTIVE-WS";
const ADDRESS: &str = "203.0.113.77";

fn distinctive(base: RawLogonRecord) -> RawLogonRecord {
    RawLogonRecord {
        target_user_sid: Some(SID.to_string()),
        target_user_name: Some(USER.to_string()),
        target_domain_name: Some(DOMAIN.to_string()),
        workstation: Some(WORKSTATION.to_string()),
        ip_address: Some(ADDRESS.to_string()),
        ..base
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn auth_ac_010_no_logon_data_in_the_agent_log() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(BufferWriter {
            buf: Arc::clone(&buf),
        })
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("one subscriber per test binary");

    let ring = Arc::new(EventRing::new(1024));
    let counters = LogonCounters::new();
    let mut monitor = UnusableMonitor::new();

    // Reported: a success and a failure whose code keeps the name.
    let mut kept = distinctive(failure());
    kept.sub_status = Some(0xC000_006A);
    for raw in [distinctive(success()), kept] {
        dispatch_logon_record(&raw, &ring, &counters);
    }
    // Unusable (another event id), counted and logged at warn.
    let mut other = distinctive(success());
    other.event_id = 4634;
    dispatch_logon_record(&other, &ring, &counters);
    assert_eq!(monitor.observe(&counters, Instant::now()), 1);
    // Before 1970, logged at error.
    let mut early = distinctive(success());
    early.filetime_100ns = 0;
    dispatch_logon_record(&early, &ring, &counters);
    assert_eq!(ring.len(), 2);

    // Delivered through the agent's loop.
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let mock = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let agent =
        common::start_secure_agent(&pki, &mock.base_url, 30, Capture::Ring(Arc::clone(&ring)));
    let delivered = common::wait_until(Duration::from_secs(10), || {
        mock.received()
            .iter()
            .any(|e| !common::envelope_events(e).is_empty())
    })
    .await;
    agent.stop().await.expect("clean stop");
    assert!(delivered, "the logons must be delivered");
    // They did travel: the data is in the envelope, not in the log.
    let sent: String = mock
        .received()
        .iter()
        .flat_map(common::envelope_events)
        .map(|e| e.to_string())
        .collect();
    assert!(sent.contains(SID) && sent.contains(USER));

    let log = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
    assert!(
        log.contains("could not use") && log.contains("pre-1970"),
        "the warn and error lines were captured"
    );
    for value in [USER, SID, DOMAIN, WORKSTATION, ADDRESS] {
        assert!(!log.contains(value), "the agent log must not hold {value}");
    }
}
