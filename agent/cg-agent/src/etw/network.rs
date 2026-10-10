//! Decoding of a Kernel-Network connection record (SPEC-019 §Operational
//! §2), independent of the ETW plumbing.
//!
//! The Windows session hands over the raw bytes of the record's `PID`,
//! `saddr`, `daddr`, `sport` and `dport` properties with the event id;
//! `decode_connection` turns them into the connection's process,
//! direction and endpoints (ADR-0018 §4). It is a pure function, so the
//! harness tests it on every platform (net_ac_005).
//!
//! Three readings of the provider's payload are HYPOTHESES, not measured
//! facts: no run on real ETW preceded this code (ADR-0018 §Context 7).
//! Each lives in exactly one function below, marked HYPOTHESIS H1, H2 or
//! H3; the elevated gate (SPEC-019 net_ac_008 and net_ac_009) confirms or
//! refutes them, and correcting one is changing that function and its
//! test vectors.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use super::types::Direction;

/// TCP connection attempted, IPv4 (outbound).
pub const TCP_CONNECT_V4: u16 = 12;
/// TCP connection accepted, IPv4 (inbound).
pub const TCP_ACCEPT_V4: u16 = 15;
/// TCP connection attempted, IPv6 (outbound).
pub const TCP_CONNECT_V6: u16 = 28;
/// TCP connection accepted, IPv6 (inbound).
pub const TCP_ACCEPT_V6: u16 = 31;

/// The provider events the agent reports (ADR-0018 §2); every other id
/// is discarded and counted (SPEC-019 §Operational §1).
pub const CONNECTION_EVENT_IDS: [u16; 4] =
    [TCP_CONNECT_V4, TCP_ACCEPT_V4, TCP_CONNECT_V6, TCP_ACCEPT_V6];

/// A connection record as decoded: the local process, the host's point
/// of view, and the initiator (`src`) and acceptor (`dst`) endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodedConnection {
    pub pid: u32,
    pub direction: Direction,
    pub src: SocketAddr,
    pub dst: SocketAddr,
}

/// The direction an event id reports: 12 and 28 outbound, 15 and 31
/// inbound (SPEC-019 §Operational §2); `None` for any other id.
pub fn connection_direction(event_id: u16) -> Option<Direction> {
    match event_id {
        TCP_CONNECT_V4 | TCP_CONNECT_V6 => Some(Direction::Outbound),
        TCP_ACCEPT_V4 | TCP_ACCEPT_V6 => Some(Direction::Inbound),
        _ => None,
    }
}

/// The record's `PID`: a `win:UInt32` in the payload's native byte order
/// (little-endian on every architecture Windows runs on).
pub fn decode_pid(bytes: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

/// Decode one connection record from its event id and the raw bytes of
/// its properties. `None` when the id is not one of the four, or a field
/// has an unexpected size.
pub fn decode_connection(
    event_id: u16,
    pid: &[u8],
    saddr: &[u8],
    daddr: &[u8],
    sport: &[u8],
    dport: &[u8],
) -> Option<DecodedConnection> {
    let direction = connection_direction(event_id)?;
    let pid = decode_pid(pid)?;
    let s = SocketAddr::new(address(saddr)?, port(sport)?);
    let d = SocketAddr::new(address(daddr)?, port(dport)?);
    let (local, remote) = local_and_remote(s, d);
    // ADR-0018 §4: src is the initiator, dst the acceptor, in both
    // directions. Outbound, the host initiated; inbound, the peer did.
    let (src, dst) = match direction {
        Direction::Outbound => (local, remote),
        Direction::Inbound => (remote, local),
    };
    Some(DecodedConnection {
        pid,
        direction,
        src,
        dst,
    })
}

/// HYPOTHESIS H1 — pending the elevated gate (SPEC-019 net_ac_008 and
/// net_ac_009): `sport` and `dport` hold the port in network byte order
/// (big-endian), not in the payload's native order.
fn port(bytes: &[u8]) -> Option<u16> {
    Some(u16::from_be_bytes(bytes.try_into().ok()?))
}

/// HYPOTHESIS H2 — pending the elevated gate (SPEC-019 net_ac_008 and
/// net_ac_009): `saddr` and `daddr` hold the address's bytes in network
/// order, 4 for IPv4 (declared `win:UInt32`) and 16 for IPv6
/// (`win:Binary`). An IPv4-mapped IPv6 address becomes its IPv4 address
/// (ADR-0018 §4); that is the contract, not part of the hypothesis.
fn address(bytes: &[u8]) -> Option<IpAddr> {
    match bytes.len() {
        4 => Some(IpAddr::V4(Ipv4Addr::from(<[u8; 4]>::try_from(bytes).ok()?))),
        16 => {
            let v6 = Ipv6Addr::from(<[u8; 16]>::try_from(bytes).ok()?);
            Some(match v6.to_ipv4_mapped() {
                Some(v4) => IpAddr::V4(v4),
                None => IpAddr::V6(v6),
            })
        }
        _ => None,
    }
}

/// HYPOTHESIS H3 — pending the elevated gate (SPEC-019 net_ac_008 and
/// net_ac_009): in all four events `saddr`/`sport` is the host's local
/// endpoint and `daddr`/`dport` the remote one. Takes (saddr, sport) and
/// (daddr, dport); returns (local, remote).
fn local_and_remote(s: SocketAddr, d: SocketAddr) -> (SocketAddr, SocketAddr) {
    (s, d)
}
