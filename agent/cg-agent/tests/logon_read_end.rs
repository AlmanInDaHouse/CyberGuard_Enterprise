//! SPEC-020 §Operational §1 — the end of a read of the subscription.
//!
//! Regression test for the defect the elevated gate found on 2026-10-11:
//! the logon thread took `ERROR_INVALID_OPERATION` (4317) for a failed
//! read and reopened the subscription every few seconds, so no logon was
//! delivered. On a pulled subscription `EvtNext` returns
//! `ERROR_NO_MORE_ITEMS` (259) when the records are exhausted, and 4317
//! when it is called again with no new record since: both mean none is
//! left. Any other code is a failed read. The loop that reads is
//! exercised on the real Security log by auth_ac_012 and auth_ac_013.

use cg_agent::logon::read_is_exhausted;

#[test]
fn no_more_items_and_invalid_operation_mean_none_is_left() {
    assert!(read_is_exhausted(259), "ERROR_NO_MORE_ITEMS");
    assert!(read_is_exhausted(4317), "ERROR_INVALID_OPERATION");
}

#[test]
fn other_codes_are_failed_reads() {
    // ERROR_ACCESS_DENIED, ERROR_INVALID_HANDLE, RPC_S_SERVER_UNAVAILABLE,
    // ERROR_EVT_QUERY_RESULT_STALE, and no code at all.
    for code in [5u32, 6, 1722, 15011, 0] {
        assert!(!read_is_exhausted(code), "{code}");
    }
}
