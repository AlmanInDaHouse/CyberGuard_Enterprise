//! SPEC-019 net_ac_008 — a real connection.
//!
//! With the agent on its normal path against the TLS mock (the agent runs
//! inside this test process, as in process_ac_004), a curl.exe probe
//! connects over IPv4 loopback and over IPv6 loopback to listeners the
//! test holds. For each connection the agent delivers one outbound 4001
//! element whose `actor.process.pid` is the probe's, whose `dst_endpoint`
//! is the listener's address and port, whose `src_endpoint` is the peer
//! the listener saw, and whose `actor.process.uid` equals the
//! `process.uid` of the probe's Launch element. No 4001 element carries
//! this test process's PID: it hosts the agent, connects to the mock and
//! accepts the probe's connections, and the agent excludes its own PID.
//!
//! The probe also attempts a connection to a loopback port with no
//! listener; the test prints `net_ac_008 refused_attempt_reported=<bool>`
//! and does not assert it (ADR-0018 §10). On failure it prints the network
//! elements it received and the endpoints it expected
//! (`net_probe::report`). Real ETW, elevated: run with
//! `cargo test -p cg-agent -- --ignored --test-threads=1 --show-output`.

#[cfg(windows)]
mod common;
#[cfg(windows)]
mod net_probe;

#[cfg(windows)]
#[ignore = "real ETW, elevated gate: cargo test -p cg-agent -- --ignored --test-threads=1"]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net_ac_008_a_real_connection_is_reported_with_its_probe() {
    use net_probe::{closed_port, curl, report, Listener, Seen};
    use serde_json::Value;
    use std::time::Duration;

    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let mock = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let agent = common::start_secure_agent(&pki, &mock.base_url, 1, cg_agent::Capture::Platform);
    // The first heartbeat goes out once the ETW session is open.
    assert!(
        common::wait_until(Duration::from_secs(10), || mock.received_count() >= 1).await,
        "net_ac_008: the agent must open its ETW session and heartbeat (elevated?)"
    );

    let v4 = Listener::start("127.0.0.1:0", 64);
    let v6 = Listener::start("[::1]:0", 64);
    let refused_port = closed_port();
    let p4 = curl(&format!("http://127.0.0.1:{}/", v4.addr.port()));
    let p6 = curl(&format!("http://[::1]:{}/", v6.addr.port()));
    let refused = curl(&format!("http://127.0.0.1:{refused_port}/"));
    let peer4 = v4.peer(Duration::from_secs(10));
    let peer6 = v6.peer(Duration::from_secs(10));

    let elements = |mock: &common::TlsMockServer| -> Vec<Value> {
        mock.received()
            .iter()
            .flat_map(common::envelope_events)
            .collect()
    };
    let network = |elements: &[Value]| -> Vec<Seen> {
        elements
            .iter()
            .filter(|e| e["class_uid"].as_u64() == Some(4001))
            .filter_map(Seen::from_element)
            .collect()
    };
    let outbound_of = |seen: &[Seen], pid: u32| {
        seen.iter()
            .any(|s| s.pid == pid && s.direction == "outbound")
    };
    common::wait_until(Duration::from_secs(25), || {
        let seen = network(&elements(&mock));
        outbound_of(&seen, p4.pid) && outbound_of(&seen, p6.pid)
    })
    .await;
    // One more batch interval, so the refused attempt's element, if any, is in.
    tokio::time::sleep(Duration::from_secs(6)).await;
    agent.stop().await.expect("net_ac_008: clean stop");

    let received = elements(&mock);
    let seen = network(&received);
    let test_pid = std::process::id();

    let refused_reported = seen
        .iter()
        .any(|s| s.pid == refused.pid && s.dst.port() == refused_port);
    println!("net_ac_008 refused_attempt_reported={refused_reported}");

    let launch_uid = |pid: u32| -> Option<String> {
        received
            .iter()
            .filter(|e| e["class_uid"].as_u64() == Some(1007))
            .filter(|e| e["activity_id"].as_u64() == Some(1))
            .find(|e| e.pointer("/process/pid").and_then(Value::as_u64) == Some(u64::from(pid)))
            .and_then(|e| e.pointer("/process/uid").and_then(Value::as_str))
            .map(String::from)
    };

    let mut failures = Vec::new();
    let mut expected = Vec::new();
    for (label, probe, listener, peer) in [("IPv4", p4, &v4, peer4), ("IPv6", p6, &v6, peer6)] {
        let Some(peer) = peer else {
            failures.push(format!(
                "{label}: the listener saw no connection (curl pid={} exit={:?})",
                probe.pid, probe.exit
            ));
            continue;
        };
        expected.push(format!(
            "{label}: one outbound element pid={} src={peer} dst={} uid=<the probe's Launch uid> \
             (curl exit={:?})",
            probe.pid, listener.addr, probe.exit
        ));
        let matching: Vec<&Seen> = seen
            .iter()
            .filter(|s| {
                s.pid == probe.pid
                    && s.direction == "outbound"
                    && s.src == peer
                    && s.dst == listener.addr
            })
            .collect();
        if matching.len() != 1 {
            failures.push(format!(
                "{label}: {} matching outbound elements, expected 1",
                matching.len()
            ));
            continue;
        }
        let launch = launch_uid(probe.pid);
        if launch.is_none() {
            failures.push(format!("{label}: no Launch element for the probe"));
        } else if matching[0].uid != launch {
            failures.push(format!(
                "{label}: actor.process.uid {:?} != the Launch's process.uid {launch:?}",
                matching[0].uid
            ));
        }
    }
    expected.push(format!("no element with this test's pid={test_pid}"));
    if seen.iter().any(|s| s.pid == test_pid) {
        failures.push(format!("an element carries this test's pid={test_pid}"));
    }

    if !failures.is_empty() {
        let pids = [p4.pid, p6.pid, refused.pid, test_pid];
        let ports = [v4.addr.port(), v6.addr.port(), refused_port];
        panic!(
            "net_ac_008 failed:\n  {}\n{}",
            failures.join("\n  "),
            report(&seen, &pids, &ports, &expected)
        );
    }
}
