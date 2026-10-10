//! SPEC-019 net_ac_009 — an accepted connection, and the filter.
//!
//! The test opens the capture session itself, with no PID excluded, and
//! holds two listeners, one on IPv4 loopback and one on IPv6 loopback; a
//! curl.exe probe connects to each and the listener sends a 1 MiB body.
//! The ring then holds, for each connection, one inbound event with this
//! test process's PID, the listener's address and port as destination and
//! the peer the listener saw as source; and one outbound event with the
//! probe's PID and the same endpoints (events 15 and 12 over IPv4, 31 and
//! 28 over IPv6; SPEC-019 Amendment 2026-10-10). The session's count of
//! discarded network records stays at 0: the filter by event id is
//! honoured, although the exchange makes the provider write data events.
//! On failure the test prints the network events in the ring and the
//! endpoints it expected. Real ETW, elevated: run with
//! `cargo test -p cg-agent -- --ignored --test-threads=1`.

#[cfg(windows)]
mod net_probe;

#[cfg(windows)]
#[ignore = "real ETW, elevated gate: cargo test -p cg-agent -- --ignored --test-threads=1"]
#[test]
fn net_ac_009_an_accepted_connection_and_the_filter() {
    use cg_agent::etw::EtwSession;
    use net_probe::{curl, report, Listener, Seen};
    use std::net::SocketAddr;
    use std::time::{Duration, Instant};

    let mut session =
        EtwSession::open(65536).expect("net_ac_009: open the capture session (elevated?)");
    let test_pid = std::process::id();

    let v4 = Listener::start("127.0.0.1:0", 1 << 20);
    let v6 = Listener::start("[::1]:0", 1 << 20);
    let p4 = curl(&format!("http://127.0.0.1:{}/", v4.addr.port()));
    let p6 = curl(&format!("http://[::1]:{}/", v6.addr.port()));
    let peer4 = v4.peer(Duration::from_secs(10));
    let peer6 = v6.peer(Duration::from_secs(10));
    let connections = [("IPv4", p4, v4.addr, peer4), ("IPv6", p6, v6.addr, peer6)];

    let snapshot = |session: &EtwSession| -> Vec<Seen> {
        session
            .ring
            .snapshot_events()
            .iter()
            .filter_map(|e| e.as_network().map(Seen::from_event))
            .collect()
    };
    let inbound = |seen: &[Seen], peer: SocketAddr, listener: SocketAddr| -> usize {
        seen.iter()
            .filter(|s| {
                s.pid == test_pid && s.direction == "inbound" && s.src == peer && s.dst == listener
            })
            .count()
    };
    let outbound = |seen: &[Seen], pid: u32, peer: SocketAddr, listener: SocketAddr| -> usize {
        seen.iter()
            .filter(|s| {
                s.pid == pid && s.direction == "outbound" && s.src == peer && s.dst == listener
            })
            .count()
    };
    let complete = |seen: &[Seen]| {
        connections
            .iter()
            .all(|(_, probe, listener, peer)| match peer {
                Some(peer) => {
                    inbound(seen, *peer, *listener) > 0
                        && outbound(seen, probe.pid, *peer, *listener) > 0
                }
                None => true,
            })
    };

    let deadline = Instant::now() + Duration::from_secs(15);
    let mut seen = snapshot(&session);
    while Instant::now() < deadline && !complete(&seen) {
        std::thread::sleep(Duration::from_millis(250));
        seen = snapshot(&session);
    }
    // A moment more, so a duplicate would show too.
    std::thread::sleep(Duration::from_secs(2));
    seen = snapshot(&session);
    let discarded = session.network_records_discarded();
    let first_discard = session.first_network_discard();
    session.stop();

    let mut failures = Vec::new();
    let mut expected = Vec::new();
    for (label, probe, listener, peer) in connections {
        let Some(peer) = peer else {
            failures.push(format!(
                "{label}: the listener saw no connection (curl pid={} exit={:?})",
                probe.pid, probe.exit
            ));
            continue;
        };
        expected.push(format!(
            "{label}: one inbound event pid={test_pid} src={peer} dst={listener}"
        ));
        expected.push(format!(
            "{label}: one outbound event pid={} src={peer} dst={listener} (curl exit={:?})",
            probe.pid, probe.exit
        ));
        let (i, o) = (
            inbound(&seen, peer, listener),
            outbound(&seen, probe.pid, peer, listener),
        );
        if i != 1 {
            failures.push(format!("{label}: {i} matching inbound events, expected 1"));
        }
        if o != 1 {
            failures.push(format!("{label}: {o} matching outbound events, expected 1"));
        }
        // The probe makes one connection: every event of its PID is that one.
        let of_probe = seen.iter().filter(|s| s.pid == probe.pid).count();
        if of_probe != 1 {
            failures.push(format!(
                "{label}: {of_probe} events carry the probe's pid, expected 1"
            ));
        }
    }
    expected.push("0 discarded network records".to_string());
    if discarded != 0 {
        failures.push(format!(
            "{discarded} network records discarded; the first: {first_discard:?} \
             (event id, byte lengths of PID, saddr, daddr, sport, dport)"
        ));
    }

    if !failures.is_empty() {
        let pids = [p4.pid, p6.pid, test_pid];
        let ports = [
            v4.addr.port(),
            v6.addr.port(),
            peer4.map_or(0, |p| p.port()),
            peer6.map_or(0, |p| p.port()),
        ];
        panic!(
            "net_ac_009 failed:\n  {}\n{}",
            failures.join("\n  "),
            report(&seen, &pids, &ports, &expected)
        );
    }
}
