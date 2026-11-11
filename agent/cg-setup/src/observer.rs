//! Configure's observer (SPEC-024 Amendment 2026-11-10, item 8's outcome,
//! within item 7 of the Amendment 2026-11-09): whether the product service,
//! once started, is operational, read from its state and its own log.
//!
//! Before the start, configure records the log's position
//! ([`LogPosition::record`]). [`observe`] then reads, for
//! [`OBSERVE_WITHIN`] from the start request, the service's state and the
//! log lines written after that position: a service that reports `STOPPED`
//! is 42, one still pending at the end is 43, and one `RUNNING` whose
//! [`IDENTITY_LINE`] and then [`HEARTBEAT_LINE`] have not both appeared by
//! then is 44.
//!
//! The log is the service's rotating log (`cg_agent::service::rotation`).
//! Every name is opened without following a reparse point and sharing read,
//! write and delete, so that the reads never stop the service's rotation.

use std::path::Path;
use std::time::{Duration, Instant};

use cg_agent::custody::FileId;

use crate::codes::Refusal;
use crate::scm::Manager;

/// How long the observer reads, from the start request.
pub const OBSERVE_WITHIN: Duration = Duration::from_secs(60);
/// The agent's identity line: its `fields.message`.
pub const IDENTITY_LINE: &str = "agent identity ready";
/// The agent's heartbeat line: its `fields.message`.
pub const HEARTBEAT_LINE: &str = "signed heartbeat sent";

/// The log files' names, oldest first: the archives from the highest number,
/// then the file being rotated, then the active file.
pub fn log_names() -> Vec<String> {
    unimplemented!("log_names")
}

/// The log's position before the start: the file ID and size of the active
/// file, or its absence, and the IDs of the log files present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogPosition {
    active: Option<(FileId, u64)>,
    present: Vec<FileId>,
}

impl LogPosition {
    /// Record the position of the log in `log_dir`. A `log_dir` that does not
    /// exist holds no log file. One that is not a directory, a reparse point
    /// included, or a log that cannot be read, is 40.
    pub fn record(log_dir: &Path) -> Result<Self, Refusal> {
        let _ = log_dir;
        unimplemented!("LogPosition::record")
    }

    /// The size recorded of the active file, if there was one.
    pub fn active_size(&self) -> Option<u64> {
        unimplemented!("LogPosition::active_size")
    }

    /// How many log files were present.
    pub fn present(&self) -> usize {
        unimplemented!("LogPosition::present")
    }
}

/// Which of the two lines have appeared, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seen {
    /// Neither line, or the heartbeat line alone.
    Neither,
    /// The identity line.
    Identity,
    /// The identity line, then the heartbeat line.
    Both,
}

/// The log written after a position: each poll reads what is new.
pub struct LogReader {
    _private: (),
}

impl LogReader {
    /// A reader of the log in `log_dir` after `position`.
    pub fn new(log_dir: &Path, position: LogPosition) -> Self {
        let _ = (log_dir, position);
        unimplemented!("LogReader::new")
    }

    /// Read what was written since the last poll, waiting on another process
    /// no longer than `wait` for each open; return what has appeared.
    pub fn poll(&mut self, wait: Duration) -> Seen {
        let _ = wait;
        unimplemented!("LogReader::poll")
    }

    /// Why the last poll could not read the log, if it could not.
    pub fn last_error(&self) -> Option<&str> {
        unimplemented!("LogReader::last_error")
    }
}

/// Observe the service `name`, started at `started`, and the log in
/// `log_dir` after `position`, for [`OBSERVE_WITHIN`] from `started`: `Ok`
/// once it is `RUNNING` and both lines have appeared in order; 42 when it
/// reports `STOPPED`, 43 when it is still pending at the end, 44 when it is
/// `RUNNING` without both lines then, and 40 when its state cannot be read.
pub fn observe(
    scm: &Manager,
    name: &str,
    log_dir: &Path,
    position: LogPosition,
    started: Instant,
) -> Result<(), Refusal> {
    let _ = (scm, name, log_dir, position, started);
    unimplemented!("observe")
}
