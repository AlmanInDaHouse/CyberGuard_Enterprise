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
//! established convention (tests/common/mod.rs + EventRing). The
//! periodic sweep of stale entries (§Operational §2's bounded-memory
//! contract, NFR-005-006) is `sweep`, driven every 60 s by the Windows
//! session's hygiene thread (`hygiene.rs`).

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
    /// dispatch (`dispatch_record`).
    pub fn insert(&self, pid: u32, created_time_nanos: u64) {
        let mut entries = self.entries.lock().expect("cache mutex poisoned");
        entries.insert(pid, created_time_nanos);
    }

    /// Consult-and-purge for a Terminate event, at dispatch. Returns the
    /// cached `created_time_nanos` and removes the entry on hit; returns
    /// None on miss.
    pub fn consult_and_purge(&self, pid: u32) -> Option<u64> {
        let mut entries = self.entries.lock().expect("cache mutex poisoned");
        entries.remove(&pid)
    }

    /// Evict the entries whose process is gone (SPEC-005 §Operational §2,
    /// NFR-005-006). `is_alive` is asked about each cached PID outside
    /// the lock, so the dispatch callback is never held up by it; an
    /// entry that a new Launch replaced meanwhile is kept. Returns the
    /// number evicted.
    pub fn sweep(&self, is_alive: impl Fn(u32) -> bool) -> usize {
        let snapshot: Vec<(u32, u64)> = {
            let entries = self.entries.lock().expect("cache mutex poisoned");
            entries.iter().map(|(&pid, &time)| (pid, time)).collect()
        };
        let gone: Vec<(u32, u64)> = snapshot
            .into_iter()
            .filter(|&(pid, _)| !is_alive(pid))
            .collect();
        let mut entries = self.entries.lock().expect("cache mutex poisoned");
        let mut evicted = 0;
        for (pid, time) in gone {
            if entries.get(&pid) == Some(&time) {
                entries.remove(&pid);
                evicted += 1;
            }
        }
        evicted
    }

    /// Current number of entries retained.
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
