//! SPEC-005 AC-007 — the parent is carried by pid only.
//!
//! As realized and ratified (SPEC-017 §Data contracts, ADR-0011
//! Amendment 2026-10-04): a Launch event carries its parent as a flat
//! `process.parent_pid` — the kernel `ParentProcessID` — and never a
//! `parent_process` object or a parent name, not even when the parent has
//! already exited. `parent_pid` is `null` only when ETW reports 0.
//!
//! A parent `cmd.exe` starts a child with `start /b` and exits at once,
//! so the child's `ParentProcessID` names a process that is gone. The
//! agent runs its secure path with real ETW capture against the TLS mock
//! (Windows, elevated; in the elevated gate). Retry budget per
//! NFR-005-005: 3 attempts × 500 ms backoff.

#![cfg(windows)]

mod common;

use serde_json::Value;
use std::process::{Command, Stdio};
use std::time::Duration;

const MAX_ATTEMPTS: u8 = 3;
const ATTEMPT_BACKOFF: Duration = Duration::from_millis(500);

#[ignore = "real ETW, elevated gate: cargo test -p cg-agent -- --ignored --test-threads=1"]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ac_007_parent_pid_only_when_parent_dead_at_child_launch() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let mock = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let agent = common::start_secure_agent(&pki, &mock.base_url, 1, cg_agent::Capture::Platform);
    assert!(
        common::wait_until(Duration::from_secs(10), || mock.received_count() >= 1).await,
        "AC-007: the agent must open its ETW session and heartbeat (elevated?)"
    );

    let child_of = |mock: &common::TlsMockServer, parent_pid: u64| -> Option<Value> {
        mock.received()
            .iter()
            .flat_map(common::envelope_events)
            .find(|e| {
                e.pointer("/activity_id").and_then(Value::as_u64) == Some(1)
                    && e.pointer("/process/parent_pid").and_then(Value::as_u64) == Some(parent_pid)
            })
    };

    let mut child_event = None;
    let mut last_attempt: u8 = 0;
    for attempt in 1..=MAX_ATTEMPTS {
        last_attempt = attempt;

        // The parent starts the child and exits immediately.
        let mut parent = Command::new("cmd.exe")
            .args([
                "/c",
                "start",
                "/b",
                "cmd.exe",
                "/c",
                "ping",
                "-n",
                "2",
                "127.0.0.1",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("AC-007: parent probe spawn must succeed");
        let parent_pid = u64::from(parent.id());
        let _ = parent.wait();

        if common::wait_until(Duration::from_secs(8), || {
            child_of(&mock, parent_pid).is_some()
        })
        .await
        {
            child_event = child_of(&mock, parent_pid);
            break;
        }
        if attempt < MAX_ATTEMPTS {
            tokio::time::sleep(ATTEMPT_BACKOFF).await;
        }
    }
    agent.stop().await.expect("AC-007: clean stop");

    let child_event = child_event.unwrap_or_else(|| {
        panic!(
            "AC-007: no child Launch naming the exited parent within {MAX_ATTEMPTS} attempts; \
             last_attempt={last_attempt}"
        )
    });
    let process = child_event
        .pointer("/process")
        .and_then(Value::as_object)
        .expect("AC-007: the event has a process object");
    assert!(
        process.get("parent_pid").and_then(Value::as_u64).is_some(),
        "AC-007: process.parent_pid MUST carry the kernel ParentProcessID"
    );
    assert!(
        !process.contains_key("parent_process"),
        "AC-007: the wire carries no parent_process object (SPEC-017 §Data contracts)"
    );
    assert!(
        !process
            .keys()
            .any(|k| k.starts_with("parent_") && k != "parent_pid"),
        "AC-007: no parent name or resolution flag per ADR-0011 §5: {process:?}"
    );
}
