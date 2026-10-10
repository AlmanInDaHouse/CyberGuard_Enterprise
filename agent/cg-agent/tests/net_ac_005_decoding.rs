//! SPEC-019 net_ac_005 — decoding.
//!
//! The pure decoding of a Kernel-Network record gives, for events 12, 15,
//! 28 and 31, the direction and the endpoints of ADR-0018 §4 (`src` the
//! initiator, `dst` the acceptor), with ports and addresses in their text
//! form, including an IPv4-mapped IPv6 address emitted as IPv4. A record
//! of another event id, or with a field of the wrong size, gives no event.
//!
//! The records are built the way the decoding assumes the provider writes
//! them (`network.rs`, hypotheses H1–H3): the PID little-endian, the
//! ports big-endian, the addresses in network order, and saddr/sport the
//! host's local endpoint. The elevated gate (net_ac_008, net_ac_009)
//! checks those assumptions against real ETW.

use cg_agent::etw::{decode_connection, DecodedConnection, Direction};
use std::net::{IpAddr, SocketAddr};

/// The raw bytes of (pid, saddr, daddr, sport, dport).
type RawFields = (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>);

/// The raw properties of one record, as the provider is assumed to write
/// them.
fn raw(pid: u32, local: &str, remote: &str) -> RawFields {
    let local: SocketAddr = local.parse().unwrap();
    let remote: SocketAddr = remote.parse().unwrap();
    let octets = |ip: IpAddr| match ip {
        IpAddr::V4(v4) => v4.octets().to_vec(),
        IpAddr::V6(v6) => v6.octets().to_vec(),
    };
    (
        pid.to_le_bytes().to_vec(),
        octets(local.ip()),
        octets(remote.ip()),
        local.port().to_be_bytes().to_vec(),
        remote.port().to_be_bytes().to_vec(),
    )
}

fn decode(event_id: u16, pid: u32, local: &str, remote: &str) -> Option<DecodedConnection> {
    let (pid, saddr, daddr, sport, dport) = raw(pid, local, remote);
    decode_connection(event_id, &pid, &saddr, &daddr, &sport, &dport)
}

/// `ip:port` in text, the CGES form (RFC 5952 for IPv6, without brackets).
fn text(addr: SocketAddr) -> (String, u16) {
    (addr.ip().to_string(), addr.port())
}

#[test]
fn net_ac_005_outbound_ipv4_src_is_the_host() {
    let c = decode(12, 4321, "192.0.2.10:49213", "198.51.100.7:443").expect("event 12");
    assert_eq!(c.pid, 4321);
    assert_eq!(c.direction, Direction::Outbound);
    assert_eq!(text(c.src), ("192.0.2.10".to_string(), 49213));
    assert_eq!(text(c.dst), ("198.51.100.7".to_string(), 443));
}

#[test]
fn net_ac_005_inbound_ipv4_src_is_the_peer() {
    // The host accepted on 10.0.0.5:22 a connection from 203.0.113.9:50001.
    let c = decode(15, 900, "10.0.0.5:22", "203.0.113.9:50001").expect("event 15");
    assert_eq!(c.pid, 900);
    assert_eq!(c.direction, Direction::Inbound);
    assert_eq!(text(c.src), ("203.0.113.9".to_string(), 50001));
    assert_eq!(text(c.dst), ("10.0.0.5".to_string(), 22));
}

#[test]
fn net_ac_005_outbound_ipv6_in_rfc_5952_form() {
    let c = decode(
        28,
        77,
        "[2001:db8:0:0:0:0:0:1]:50123",
        "[2001:db8:0:0:0:0:0:2]:443",
    )
    .expect("event 28");
    assert_eq!(c.direction, Direction::Outbound);
    assert_eq!(text(c.src), ("2001:db8::1".to_string(), 50123));
    assert_eq!(text(c.dst), ("2001:db8::2".to_string(), 443));
}

#[test]
fn net_ac_005_inbound_ipv6_loopback() {
    let c = decode(31, 5, "[::1]:8080", "[::1]:55555").expect("event 31");
    assert_eq!(c.direction, Direction::Inbound);
    assert_eq!(text(c.src), ("::1".to_string(), 55555));
    assert_eq!(text(c.dst), ("::1".to_string(), 8080));
}

#[test]
fn net_ac_005_ipv4_mapped_ipv6_is_emitted_as_ipv4() {
    let c = decode(
        28,
        77,
        "[::ffff:192.0.2.10]:50123",
        "[::ffff:198.51.100.7]:443",
    )
    .expect("event 28");
    assert_eq!(text(c.src), ("192.0.2.10".to_string(), 50123));
    assert_eq!(text(c.dst), ("198.51.100.7".to_string(), 443));
    assert!(c.src.is_ipv4() && c.dst.is_ipv4());
}

#[test]
fn net_ac_005_another_event_id_gives_no_event() {
    // Data sent / received, disconnect, retransmit, and the UDP events.
    for id in [10, 11, 13, 14, 16, 17, 18, 26, 27, 29, 42, 43, 49, 58, 59] {
        assert_eq!(
            decode(id, 4321, "192.0.2.10:49213", "198.51.100.7:443"),
            None,
            "event {id} must give no event"
        );
    }
}

#[test]
fn net_ac_005_a_field_of_the_wrong_size_gives_no_event() {
    let (pid, saddr, daddr, sport, dport) = raw(4321, "192.0.2.10:49213", "198.51.100.7:443");
    assert!(decode_connection(12, &pid, &saddr, &daddr, &sport, &dport).is_some());
    assert_eq!(
        decode_connection(12, &pid[..2], &saddr, &daddr, &sport, &dport),
        None
    );
    assert_eq!(
        decode_connection(12, &pid, &saddr[..3], &daddr, &sport, &dport),
        None
    );
    assert_eq!(
        decode_connection(12, &pid, &saddr, &daddr, &[], &dport),
        None
    );
    assert_eq!(
        decode_connection(12, &pid, &saddr, &[0u8; 8], &sport, &dport),
        None
    );
}
