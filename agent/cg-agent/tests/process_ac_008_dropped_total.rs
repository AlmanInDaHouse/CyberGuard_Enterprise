//! SPEC-005 AC-008 — `events_dropped_total` visibility under ring overflow.
//!
//! The agent's in-memory ring buffer is bounded in events per
//! ADR-0009 §Decision part 3, with the production size pinned to
//! 65536 by NFR-005-002. For testing the overflow path
//! deterministically, the ring exposes a test-only constructor
//! `EventRing::new_for_test(n)` that bypasses the production size.
//! On overflow, the ring drops oldest events in FIFO order and
//! increments `events_dropped_total` monotonically.
//!
//! Three assertions per SPEC-005 §AC AC-008:
//!   (i)   `events_dropped_total` reports exactly `K` after injecting
//!         `N + K` events into a ring sized `N`;
//!   (ii)  the events retained in the ring are the most recent `N`
//!         (FIFO drop of the oldest `K`);
//!   (iii) the counter is monotonic — repeating the overflow cycle
//!         increments the counter, never resets within an agent lifetime.
//!
//! Pure unit test. No testcontainers, no ETW, no probe process, no async
//! runtime. The synthetic injection at the ring enqueue boundary
//! exercises the defensive contract under sustained backpressure, not
//! a routine operating condition.

use cg_agent::etw::{ActivityId, CapturedEvent, EventRing};

const RING_SIZE: usize = 8;
const FIRST_BATCH_OVERFLOW: usize = 3;
const SECOND_BATCH_OVERFLOW: usize = 5;

fn synthetic_event(seq: u32) -> CapturedEvent {
    CapturedEvent {
        pid: 10_000 + seq,
        activity_id: ActivityId::Launch,
        image_file_name: format!("\\Device\\HarddiskVolume2\\probe_{seq}.exe"),
        parent_pid: 4,
        command_line: format!("probe_{seq}.exe"),
        subject_user_sid: String::from("S-1-5-18"),
        etw_timestamp_nanos: 1_716_123_612_901_000_000 + (seq as u64) * 1_000_000,
        exit_status: None,
    }
}

#[test]
fn ac_008_overflow_increments_dropped_total_by_exactly_k() {
    let ring = EventRing::new_for_test(RING_SIZE);
    assert_eq!(
        ring.events_dropped_total(),
        0,
        "AC-008: dropped counter MUST initialise to 0"
    );

    // Inject N + K events into a ring sized N.
    let total_to_inject = RING_SIZE + FIRST_BATCH_OVERFLOW;
    for seq in 0..total_to_inject as u32 {
        ring.enqueue_or_drop(synthetic_event(seq));
    }

    assert_eq!(
        ring.events_dropped_total(),
        FIRST_BATCH_OVERFLOW as u64,
        "AC-008: events_dropped_total MUST report exactly K={FIRST_BATCH_OVERFLOW} after N+K injections into ring sized N={RING_SIZE}"
    );
}

#[test]
fn ac_008_overflow_drops_oldest_in_fifo_order() {
    let ring = EventRing::new_for_test(RING_SIZE);

    // Inject N + K events.
    let total_to_inject = RING_SIZE + FIRST_BATCH_OVERFLOW;
    for seq in 0..total_to_inject as u32 {
        ring.enqueue_or_drop(synthetic_event(seq));
    }

    let retained = ring.snapshot_events();
    assert_eq!(
        retained.len(),
        RING_SIZE,
        "AC-008: ring MUST retain exactly N={RING_SIZE} events after overflow"
    );

    // The oldest K events (seq 0..K) should have been dropped; the
    // retained events should be seq K..(N+K).
    let retained_pids: Vec<u32> = retained.iter().map(|e| e.pid).collect();
    let expected_pids: Vec<u32> = (FIRST_BATCH_OVERFLOW as u32..total_to_inject as u32)
        .map(|seq| 10_000 + seq)
        .collect();
    assert_eq!(
        retained_pids, expected_pids,
        "AC-008: retained events MUST be the most recent N (FIFO drop of the oldest K)"
    );
}

#[test]
fn ac_008_dropped_counter_is_monotonic_across_overflow_cycles() {
    let ring = EventRing::new_for_test(RING_SIZE);

    // First overflow cycle: inject N + K1 events.
    let first_total = RING_SIZE + FIRST_BATCH_OVERFLOW;
    for seq in 0..first_total as u32 {
        ring.enqueue_or_drop(synthetic_event(seq));
    }
    let dropped_after_first = ring.events_dropped_total();
    assert_eq!(dropped_after_first, FIRST_BATCH_OVERFLOW as u64);

    // Second overflow cycle: inject additional N + K2 events on top of
    // the already-full ring. Every new injection beyond what the ring
    // can re-absorb increments the counter further. With the ring
    // already full (N events retained), all N + K2 new injections
    // displace existing events — so the counter increments by N + K2.
    let second_total = RING_SIZE + SECOND_BATCH_OVERFLOW;
    for seq in first_total as u32..(first_total + second_total) as u32 {
        ring.enqueue_or_drop(synthetic_event(seq));
    }
    let dropped_after_second = ring.events_dropped_total();

    let expected_total_dropped =
        FIRST_BATCH_OVERFLOW as u64 + RING_SIZE as u64 + SECOND_BATCH_OVERFLOW as u64;
    assert_eq!(
        dropped_after_second, expected_total_dropped,
        "AC-008: dropped counter MUST be monotonically increasing across overflow cycles \
         (after first cycle: {FIRST_BATCH_OVERFLOW}; after second cycle with ring already full: \
         {FIRST_BATCH_OVERFLOW} + {RING_SIZE} displaced + {SECOND_BATCH_OVERFLOW} = {expected_total_dropped})"
    );

    assert!(
        dropped_after_second > dropped_after_first,
        "AC-008: dropped counter MUST be strictly increasing across overflow cycles, never resetting"
    );
}
