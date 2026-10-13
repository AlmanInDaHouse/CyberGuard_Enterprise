//! ADR-0008 §Decision part 2 — the startup reclaim.
//!
//! At open the agent stops a live ETW session of its name, left by a
//! prior crash or force-kill, so that its own start does not fail with
//! `ERROR_ALREADY_EXISTS`. No other test asserts it: the elevated
//! marquees exercised it only because each one's forced end left its
//! session for the next agent to reclaim, and since debt #63's fix the
//! harness stops that session itself.
//!
//! A session of the agent's name with no provider, which `logman` starts
//! with or without elevation, stands in for the leftover. Unelevated, the
//! reclaim stops it and the start that follows fails for privilege, not
//! because the name is taken; elevated, the session opens. Either way no
//! session of the name is left. It runs from a host where no session of
//! the agent's name is running: unelevated, one an elevated agent left
//! cannot be stopped (debt #63).

#![cfg(windows)]

use cg_agent::etw::{events_lost, EtwSession, SESSION_NAME};
use std::process::Command;

#[test]
fn open_reclaims_a_leftover_session_of_the_agents_name() {
    let started = Command::new("logman")
        .args(["start", SESSION_NAME, "-ets", "-rt"])
        .status()
        .expect("run logman");
    assert!(
        started.success(),
        "logman start {SESSION_NAME} -ets -rt: {started:?}"
    );
    assert!(
        events_lost(SESSION_NAME).is_ok(),
        "the leftover session must be running before the open"
    );

    match EtwSession::open(16) {
        Ok(mut session) => session.stop(),
        Err(e) => assert!(
            e.is_privilege(),
            "after the reclaim the start must open (elevated) or fail for privilege \
             (unelevated), got {e:?}"
        ),
    }
    assert_eq!(
        events_lost(SESSION_NAME),
        Err(4201),
        "no session of the agent's name is left"
    );
}
