//! SPEC-019 net_ac_009 — an accepted connection, and the filter.
//!
//! The test opens the capture session itself, with no PID excluded, and
//! holds a listener; a curl.exe probe connects to it over loopback and
//! the listener sends a 1 MiB body. The ring then holds one inbound event
//! for that connection, with this test process's PID, the listener's
//! address and port as destination and the peer the listener saw as
//! source; and one outbound event with the probe's PID and the same
//! endpoints. The session's count of discarded network records stays at
//! 0: the filter by event id is honoured, although the exchange makes the
//! provider write data events. On failure the test prints the network
//! events in the ring and the endpoints it expected. Real ETW, elevated:
//! run with `cargo test -p cg-agent -- --ignored --test-threads=1`.

#[cfg(windows)]
mod net_probe;

#[cfg(windows)]
#[ignore = "real ETW, elevated gate: cargo test -p cg-agent -- --ignored --test-threads=1"]
#[test]
fn net_ac_009_an_accepted_connection_and_the_filter() {
    use cg_agent::etw::EtwSession;
    use net_probe::{curl, report, Listener, Seen};
    use std::time::{Duration, Instant};

    let mut session =
        EtwSession::open(65536).expect("net_ac_009: open the capture session (elevated?)");
    let test_pid = std::process::id();

    let listener = Listener::start("127.0.0.1:0", 1 << 20);
    let probe = curl(&format!("http://127.0.0.1:{}/", listener.addr.port()));
    let peer = listener.peer(Duration::from_secs(10));

    let snapshot = |session: &EtwSession| -> Vec<Seen> {
        session
            .ring
            .snapshot_events()
            .iter()
            .filter_map(|e| e.as_network().map(Seen::from_event))
            .collect()
    };
    let inbound = |seen: &[Seen], peer| -> usize {
        seen.iter()
            .filter(|s| {
                s.pid == test_pid
                    && s.direction == "inbound"
                    && s.src == peer
                    && s.dst == listener.addr
            })
            .count()
    };
    let outbound = |seen: &[Seen], peer| -> usize {
        seen.iter()
            .filter(|s| {
                s.pid == probe.pid
                    && s.direction == "outbound"
                    && s.src == peer
                    && s.dst == listener.addr
            })
            .count()
    };

    let mut seen = snapshot(&session);
    if let Some(peer) = peer {
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline && (inbound(&seen, peer) == 0 || outbound(&seen, peer) == 0)
        {
            std::thread::sleep(Duration::from_millis(250));
            seen = snapshot(&session);
        }
        // A moment more, so a duplicate would show too.
        std::thread::sleep(Duration::from_secs(2));
        seen = snapshot(&session);
    }
    let discarded = session.network_records_discarded();
    let first_discard = session.first_network_discard();
    session.stop();

    let mut failures = Vec::new();
    let mut expected = Vec::new();
    match peer {
        None => failures.push(format!(
            "the listener saw no connection (curl pid={} exit={:?})",
            probe.pid, probe.exit
        )),
        Some(peer) => {
            expected.push(format!(
                "one inbound event pid={test_pid} src={peer} dst={}",
                listener.addr
            ));
            expected.push(format!(
                "one outbound event pid={} src={peer} dst={} (curl exit={:?})",
                probe.pid, listener.addr, probe.exit
            ));
            let (i, o) = (inbound(&seen, peer), outbound(&seen, peer));
            if i != 1 {
                failures.push(format!("{i} matching inbound events, expected 1"));
            }
            if o != 1 {
                failures.push(format!("{o} matching outbound events, expected 1"));
            }
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
        let pids = [probe.pid, test_pid];
        let ports = [listener.addr.port(), peer.map_or(0, |p| p.port())];
        panic!(
            "net_ac_009 failed:\n  {}\n{}",
            failures.join("\n  "),
            report(&seen, &pids, &ports, &expected)
        );
    }
}
