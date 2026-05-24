//! `process.created_time` cache for Terminate event retention.
//!
//! Per SPEC-005 §Operational §2 + ADR-0011 §4 amendment (a):
//! - Populated at Launch dispatch (PID → etw_timestamp_nanos).
//! - Consulted-and-purged at Terminate dispatch (returns Option<u64>;
//!   removes entry on hit; None on miss).
//! - The miss path is the AC-004 cache-miss defensive contract — the
//!   Terminate event is emitted with `process.created_time = null`
//!   rather than a fabricated value.
//!
//! Storage: `Mutex<HashMap<u32, u64>>`. Blocking Mutex per the
//! established convention (tests/common/mod.rs + EventRing in β1).
//! Periodic sweep of stale entries (per §Operational §2's bounded-
//! memory contract) is β3 territory — requires a background tokio
//! task; this primitive provides only new() + consult_and_purge.

use std::collections::HashMap;
use std::sync::Mutex;

/// PID-keyed cache of `process.created_time` for Terminate retention.
pub struct CreatedTimeCache {
    entries: Mutex<HashMap<u32, u64>>,
}

impl CreatedTimeCache {
    /// Construct an empty cache.
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Insert a (pid, created_time_nanos) pair. Called at Launch
    /// dispatch by the ETW callback (β3 forthcoming).
    pub fn insert(&self, pid: u32, created_time_nanos: u64) {
        let mut entries = self.entries.lock().expect("cache mutex poisoned");
        entries.insert(pid, created_time_nanos);
    }

    /// Consult-and-purge for a Terminate event. Returns the cached
    /// `created_time_nanos` and removes the entry on hit; returns None
    /// on miss.
    pub fn consult_and_purge(&self, pid: u32) -> Option<u64> {
        let mut entries = self.entries.lock().expect("cache mutex poisoned");
        entries.remove(&pid)
    }

    /// Current number of entries retained. Used by future sweep logic
    /// (β3) and test diagnostics.
    pub fn len(&self) -> usize {
        let entries = self.entries.lock().expect("cache mutex poisoned");
        entries.len()
    }

    /// Returns true when the cache has zero entries retained.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for CreatedTimeCache {
    fn default() -> Self {
        Self::new()
    }
}
