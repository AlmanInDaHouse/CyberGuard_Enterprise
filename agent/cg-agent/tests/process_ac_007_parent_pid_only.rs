//! SPEC-005 AC-007 — `parent_process` pid-only under PPID race.
//!
//! Asserts the agent-side permissive normative per ADR-0011 §5: when
//! the kernel `ParentProcessID` is unresolvable at child Launch capture
//! time (the parent has terminated, the PID has been reused, or the
//! parent is otherwise unintrospectable), the agent emits
//! `parent_process` with `pid` populated and `name` absent entirely
//! (not null, not sentinel, no discriminator field). The persisted
//! shape: `parent_process.pid IS NOT NULL AND parent_process.name IS NULL`.
//!
//! Timing-sensitive: the PPID race window is sub-100 ms typically.
//! Retry budget per NFR-005-005: 3 attempts × 500 ms backoff. If the
//! race does not produce the unresolvable-parent condition within 3
//! attempts, the test fails with a diagnostic identifying the retry
//! count.
//!
//! Reuses common::MockServer + common::TestAgentHandle + common::start_test_agent.

mod common;

use cg_agent::etw::EtwSession;
use serde_json::Value;
use std::process::Command;
use std::time::Duration;

const MAX_ATTEMPTS: u8 = 3;
const ATTEMPT_BACKOFF: Duration = Duration::from_millis(500);

#[cfg_attr(not(target_os = "windows"), ignore)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ac_007_parent_pid_only_when_parent_dead_at_child_launch() {
    let mock = common::MockServer::start().await;
    let agent = common::start_test_agent(&mock.base_url, common::TEST_AGENT_ID).await;

    let mut produced_unresolved = false;
    let mut last_attempt: u8 = 0;

    for attempt in 1..=MAX_ATTEMPTS {
        last_attempt = attempt;

        // Spawn the parent probe; capture its PID; let it exit cleanly.
        let parent = Command::new("cmd.exe")
            .args(["/c", "exit 0"])
            .spawn()
            .expect("AC-007: parent probe spawn must succeed");
        let parent_pid = parent.id();
        let _ = parent.wait_with_output();

        // Immediately spawn the child probe. The kernel may reuse the
        // parent's PID; even if not, the parent is dead when ETW captures
        // the child's Launch event — producing the unresolved-parent path.
        let child = Command::new("cmd.exe")
            .args(["/c", "ping -n 2 127.0.0.1 >NUL & exit 0"])
            .spawn()
            .expect("AC-007: child probe spawn must succeed");
        let child_pid = child.id();
        let _ = child.wait_with_output();

        // Wait for both events to flow through the agent.
        tokio::time::sleep(Duration::from_secs(4)).await;

        let envelopes = mock.received();
        let child_event = envelopes.iter().find_map(|env| {
            env.pointer("/events")
                .and_then(Value::as_array)
                .and_then(|events| {
                    events.iter().find(|e| {
                        e.pointer("/process/pid").and_then(Value::as_u64) == Some(child_pid as u64)
                            && e.pointer("/activity_id").and_then(Value::as_u64) == Some(1)
                    })
                })
        });

        if let Some(child_event) = child_event {
            let parent_process = child_event
                .pointer("/process/parent_process")
                .expect("AC-007: child event MUST have process.parent_process object");
            let parent_pid_field = parent_process
                .pointer("/pid")
                .and_then(Value::as_u64)
                .expect("AC-007: parent_process.pid MUST always be emitted (ETW always carries ParentProcessID)");
            let parent_name_field = parent_process.pointer("/name");

            if parent_pid_field == parent_pid as u64 && parent_name_field.is_none() {
                produced_unresolved = true;
                assert!(
                    !parent_process
                        .as_object()
                        .expect("parent_process MUST be a JSON object")
                        .contains_key("name_resolved"),
                    "AC-007: parent_process MUST NOT carry a name_resolved discriminator field per ADR-0011 §5"
                );
                break;
            }
        }

        if attempt < MAX_ATTEMPTS {
            tokio::time::sleep(ATTEMPT_BACKOFF).await;
        }
    }

    assert!(
        produced_unresolved,
        "AC-007: PPID race did not produce the unresolvable-parent condition within {MAX_ATTEMPTS} attempts; last_attempt={last_attempt}. \
         This may indicate runner-side timing assumptions are broken (parent terminates after child Launch capture) or that the agent is \
         eagerly resolving parent_process.name from a different source (process snapshot, parent-process-id cache, etc.) violating the §5 \
         permissive emission contract."
    );

    agent.shutdown().await;
}

// Placeholder to ensure EtwSession is exercised at compile time.
#[allow(dead_code)]
fn _ensure_etw_session_import() -> Option<EtwSession> {
    None
}
