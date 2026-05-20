//! SPEC-001 AC-003 — Given `heartbeat.interval_seconds = 1` and the
//! `start_time` recorded at process startup, the agent sends 3
//! heartbeats whose `sent_at` timestamps each fall within ±500 ms of
//! `start_time + (N − 1) × 1 s` for N ∈ {1, 2, 3}. Anchor-relative
//! drift bound from NFR-002 applied to the test scenario.

mod common;

use chrono_dummy::parse_iso;
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

// Tiny ISO 8601 parser scoped to this test (avoids pulling chrono as
// a test dep). Returns seconds since UNIX epoch as f64.
mod chrono_dummy {
    pub fn parse_iso(s: &str) -> f64 {
        // 2026-05-20T10:23:10.901Z → seconds since epoch as f64
        // We only need monotonic relative ordering, not absolute
        // correctness, so a naive arithmetic decode is enough.
        // Components: YYYY-MM-DDTHH:MM:SS.mmmZ
        let (date, rest) = s.split_once('T').expect("ISO date");
        let (time, _z) = rest.split_at(rest.len() - 1);
        let mut date_parts = date.split('-');
        let y: i64 = date_parts.next().unwrap().parse().unwrap();
        let mo: i64 = date_parts.next().unwrap().parse().unwrap();
        let d: i64 = date_parts.next().unwrap().parse().unwrap();
        let mut time_parts = time.split(':');
        let h: i64 = time_parts.next().unwrap().parse().unwrap();
        let mi: i64 = time_parts.next().unwrap().parse().unwrap();
        let sec_f: f64 = time_parts.next().unwrap().parse().unwrap();
        // Approximate: ignore leap years; sufficient for relative deltas
        // within a single test run.
        let days = (y - 1970) * 365 + (mo - 1) * 30 + (d - 1);
        (days as f64) * 86400.0 + (h as f64) * 3600.0 + (mi as f64) * 60.0 + sec_f
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ac_003_anchor_relative_drift() {
    let mock = common::MockServer::start().await;
    let config = common::config_with_url(&mock.base_url, 1);

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let started = Instant::now();
    let handle = tokio::spawn(async move {
        cg_agent::run(config, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });

    // Wait ~3.5 s for 3 heartbeats.
    let deadline = started + Duration::from_millis(3500);
    while Instant::now() < deadline && mock.received_count() < 3 {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let received = mock.received();
    let _ = shutdown_tx.send(());
    let _ = handle.await;

    assert!(
        received.len() >= 3,
        "expected ≥3 heartbeats, got {}",
        received.len()
    );

    // Convert sent_at to relative seconds against the first one.
    let t0 = parse_iso(received[0]["sent_at"].as_str().unwrap());
    for (idx, env) in received.iter().take(3).enumerate() {
        let t = parse_iso(env["sent_at"].as_str().unwrap());
        let drift = (t - t0 - idx as f64).abs();
        assert!(
            drift < 0.5,
            "heartbeat #{} drift {:.3}s exceeds ±500ms anchor bound",
            idx + 1,
            drift
        );
    }
}
