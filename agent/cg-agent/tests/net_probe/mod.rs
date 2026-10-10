//! Helpers for the real-ETW network tests (SPEC-019 net_ac_008 and
//! net_ac_009): a one-shot loopback listener, the curl.exe probe, and the
//! report a failure prints.
//!
//! The report lists every network event that could belong to the
//! connections the test made: by PID, or by a port in either byte order.
//! For each it prints the event as decoded and the readings that would be
//! true if a hypothesis of `etw/network.rs` were wrong: the ports in the
//! other byte order (H1), the IPv4 octets reversed (H2); a wrong H3 shows
//! as `src` and `dst` swapped against the expected endpoints.

#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use cg_agent::etw::{Direction, NetworkEvent};
use serde_json::Value;

/// A listener that accepts one connection, reads the request, answers a
/// minimal HTTP response whose body is `body_len` bytes, and closes.
pub struct Listener {
    pub addr: SocketAddr,
    peer_rx: mpsc::Receiver<SocketAddr>,
}

impl Listener {
    pub fn start(bind: &str, body_len: usize) -> Self {
        let listener = TcpListener::bind(bind).expect("bind the test listener");
        let addr = listener.local_addr().expect("listener address");
        let (peer_tx, peer_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let Ok((mut stream, peer)) = listener.accept() else {
                return;
            };
            let _ = peer_tx.send(peer);
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let mut request = Vec::new();
            let mut buf = [0u8; 4096];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                match stream.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => request.extend_from_slice(&buf[..n]),
                }
            }
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {body_len}\r\nConnection: close\r\n\r\n"
            );
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(&vec![b'x'; body_len]);
            let _ = stream.flush();
        });
        Self { addr, peer_rx }
    }

    /// The address and port the listener saw as its peer.
    pub fn peer(&self, timeout: Duration) -> Option<SocketAddr> {
        self.peer_rx.recv_timeout(timeout).ok()
    }
}

/// A loopback port with no listener: bound, noted and closed.
pub fn closed_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a port to close");
    listener.local_addr().expect("address").port()
}

/// A probe that ran: its PID and exit code.
#[derive(Debug, Clone, Copy)]
pub struct Probe {
    pub pid: u32,
    pub exit: Option<i32>,
}

/// Run `curl.exe -g -s -m 5 <url>` to completion (curl.exe ships with
/// Windows; `-g` keeps it from reading `[::1]` as a pattern).
pub fn curl(url: &str) -> Probe {
    let mut child = Command::new("curl.exe")
        .args(["-g", "-s", "-m", "5", url])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn curl.exe");
    let pid = child.id();
    let status = child.wait().expect("curl.exe exits");
    Probe {
        pid,
        exit: status.code(),
    }
}

/// A network event as the tests compare it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    pub pid: u32,
    pub direction: String,
    pub src: SocketAddr,
    pub dst: SocketAddr,
    pub uid: Option<String>,
}

impl Seen {
    /// From a 4001 element of an envelope.
    pub fn from_element(element: &Value) -> Option<Self> {
        let endpoint = |name: &str| -> Option<SocketAddr> {
            let ip: IpAddr = element
                .pointer(&format!("/{name}/ip"))?
                .as_str()?
                .parse()
                .ok()?;
            let port = element.pointer(&format!("/{name}/port"))?.as_u64()? as u16;
            Some(SocketAddr::new(ip, port))
        };
        Some(Self {
            pid: element.pointer("/actor/process/pid")?.as_u64()? as u32,
            direction: element
                .pointer("/connection_info/direction")?
                .as_str()?
                .to_string(),
            src: endpoint("src_endpoint")?,
            dst: endpoint("dst_endpoint")?,
            uid: element
                .pointer("/actor/process/uid")
                .and_then(Value::as_str)
                .map(String::from),
        })
    }

    /// From a network event in the ring.
    pub fn from_event(event: &NetworkEvent) -> Self {
        Self {
            pid: event.pid,
            direction: match event.direction {
                Direction::Outbound => "outbound".to_string(),
                Direction::Inbound => "inbound".to_string(),
            },
            src: event.src,
            dst: event.dst,
            uid: None,
        }
    }

    fn touches(&self, pids: &[u32], ports: &[u16]) -> bool {
        let port_match = |p: u16| ports.contains(&p) || ports.contains(&p.swap_bytes());
        pids.contains(&self.pid) || port_match(self.src.port()) || port_match(self.dst.port())
    }
}

fn reversed(addr: SocketAddr) -> String {
    match addr.ip() {
        IpAddr::V4(v4) => {
            let mut octets = v4.octets();
            octets.reverse();
            Ipv4Addr::from(octets).to_string()
        }
        IpAddr::V6(v6) => v6.to_string(),
    }
}

/// One line per event: as decoded, then the alternative readings.
pub fn describe(seen: &Seen) -> String {
    format!(
        "pid={} {} src={} dst={} uid={} | if ports are native-order (H1): src_port={} \
         dst_port={} | if IPv4 bytes are reversed (H2): src_ip={} dst_ip={}",
        seen.pid,
        seen.direction,
        seen.src,
        seen.dst,
        seen.uid.as_deref().unwrap_or("-"),
        seen.src.port().swap_bytes(),
        seen.dst.port().swap_bytes(),
        reversed(seen.src),
        reversed(seen.dst),
    )
}

/// The report a failing test prints: what it expected, how many network
/// events it saw, and every one that touches `pids` or `ports`.
pub fn report(all: &[Seen], pids: &[u32], ports: &[u16], expected: &[String]) -> String {
    let mut out = String::from("expected:\n");
    for line in expected {
        out.push_str(&format!("  {line}\n"));
    }
    let relevant: Vec<&Seen> = all.iter().filter(|s| s.touches(pids, ports)).collect();
    out.push_str(&format!(
        "network events seen: {} in total, {} touching pids {pids:?} or ports {ports:?}:\n",
        all.len(),
        relevant.len()
    ));
    for seen in relevant {
        out.push_str(&format!("  {}\n", describe(seen)));
    }
    out
}
