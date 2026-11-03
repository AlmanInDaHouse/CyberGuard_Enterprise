//! The service's size-bounded JSON-lines log (SPEC-024 §Operational §3,
//! install_ac_010): one active file and five archives, each at most
//! 10 MiB. Before a record would take the active file past its bound, the
//! archives shift (`agent.4.jsonl` → `agent.5.jsonl`, …, the oldest
//! deleted), the active file becomes `agent.1.jsonl`, and a new one is
//! opened. Renames keep each file's own security, so rotation broadens no
//! access; the opener gives a new active file its final security at
//! creation.
//!
//! The active file is renamed first, to `agent.rotating.jsonl`: if another
//! process holds it open (an administrator reading the log), nothing has
//! moved, the log goes on in the same file and rotation is tried again at
//! later records. The archives then shift into the first free place, so a
//! retry after a partial shift loses no archive. A rotation that keeps
//! failing for [`ROTATION_RETRY_LIMIT`] is a persistent failure.
//!
//! A record is one line. A record of [`MAX_RECORD_BYTES`] or more, its
//! newline included, is not split: a bounded diagnostic line, valid JSON,
//! takes its place. A failed write is retried once on a reopened file,
//! after a newline that ends whatever part of the record the failed write
//! left; if the retry fails too the log stops writing and reports the
//! failure once, through `on_failure`, so the service can stop with code
//! 13.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The bound of the active file and of each archive.
pub const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
/// The number of archives kept.
pub const ARCHIVES: usize = 5;
/// The longest record written as it is, its newline included.
pub const MAX_RECORD_BYTES: usize = 64 * 1024;

/// The active file's name.
pub const ACTIVE: &str = "agent.jsonl";
/// The active file's name while it rotates.
pub const ROTATING: &str = "agent.rotating.jsonl";

/// How long rotation may keep failing before the log counts it as a
/// persistent failure; meanwhile the active file grows past its bound.
pub const ROTATION_RETRY_LIMIT: Duration = Duration::from_secs(60);
/// While rotation fails, how often it is tried again.
const ROTATION_RETRY_EVERY: Duration = Duration::from_secs(1);

/// The name of archive `n` (1 is the newest).
pub fn archive_name(n: usize) -> String {
    format!("agent.{n}.jsonl")
}

/// Opens the active file at a path for appending, creating it with its
/// final security; returns the writer and the file's current size.
pub type Opener = Box<dyn Fn(&Path) -> io::Result<(Box<dyn Write + Send>, u64)> + Send>;

/// Called once when the log can no longer be written, with the reason.
pub type OnFailure = Box<dyn Fn(String) + Send>;

/// The rotating log; a `Write` whose input is whole JSON lines.
pub struct RotatingLog {
    dir: PathBuf,
    opener: Opener,
    active: Option<Box<dyn Write + Send>>,
    size: u64,
    /// The line being received, until its newline.
    pending: Vec<u8>,
    /// The line being received outgrew the bound: its bytes are counted
    /// and dropped until its newline.
    discarding: bool,
    line_len: usize,
    failed: bool,
    on_failure: Option<OnFailure>,
    /// Since when rotation has been failing, and when it was last tried.
    rotation_failing: Option<(Instant, Instant)>,
    rotation_retry_limit: Duration,
    /// `ROTATING` exists: a rotation to finish.
    unfinished: bool,
}

impl RotatingLog {
    /// Open the log in `dir`, rotating first when the active file is
    /// already at its bound.
    pub fn open(dir: &Path, opener: Opener) -> io::Result<Self> {
        let mut log = RotatingLog {
            dir: dir.to_path_buf(),
            opener,
            active: None,
            size: 0,
            pending: Vec::new(),
            discarding: false,
            line_len: 0,
            failed: false,
            on_failure: None,
            rotation_failing: None,
            rotation_retry_limit: ROTATION_RETRY_LIMIT,
            unfinished: dir.join(ROTATING).exists(),
        };
        log.reopen()?;
        if log.size >= MAX_FILE_BYTES || log.unfinished {
            log.rotate()?;
        }
        Ok(log)
    }

    /// Shorten [`ROTATION_RETRY_LIMIT`] (for tests).
    #[doc(hidden)]
    pub fn set_rotation_retry_limit(&mut self, limit: Duration) {
        self.rotation_retry_limit = limit;
    }

    /// Report a write failure that persists through `on_failure`.
    pub fn on_failure(&mut self, on_failure: OnFailure) {
        self.on_failure = Some(on_failure);
    }

    fn reopen(&mut self) -> io::Result<()> {
        self.active = None;
        let (writer, size) = (self.opener)(&self.dir.join(ACTIVE))?;
        self.active = Some(writer);
        self.size = size;
        Ok(())
    }

    /// Rotate: the active file to `ROTATING` (nothing moves if that
    /// fails), a new active file, the archives shifted into the first free
    /// place, `ROTATING` to archive 1. Each step that is already done is
    /// skipped, so a retry finishes an interrupted rotation.
    fn rotate(&mut self) -> io::Result<()> {
        let rotating = self.dir.join(ROTATING);
        if !rotating.exists() {
            if let Some(mut active) = self.active.take() {
                let _ = active.flush();
            }
            let renamed = std::fs::rename(self.dir.join(ACTIVE), &rotating);
            // Whether or not it moved, write on in the active file.
            self.reopen()?;
            renamed?;
            self.unfinished = true;
        }
        // The first free archive place, making one if all five are taken.
        let free = (1..=ARCHIVES)
            .find(|&n| !self.dir.join(archive_name(n)).exists())
            .map_or_else(
                || std::fs::remove_file(self.dir.join(archive_name(ARCHIVES))).map(|()| ARCHIVES),
                Ok,
            )?;
        for n in (1..free).rev() {
            std::fs::rename(
                self.dir.join(archive_name(n)),
                self.dir.join(archive_name(n + 1)),
            )?;
        }
        std::fs::rename(&rotating, self.dir.join(archive_name(1)))?;
        self.unfinished = false;
        Ok(())
    }

    /// Rotate if due, tolerating a failure for the retry limit.
    fn rotate_if_due(&mut self, len: u64) -> Result<(), String> {
        let full = self.size > 0 && self.size + len > MAX_FILE_BYTES;
        if !self.unfinished && !full {
            return Ok(());
        }
        let now = Instant::now();
        if let Some((_, tried)) = self.rotation_failing {
            if now.duration_since(tried) < ROTATION_RETRY_EVERY {
                return Ok(());
            }
        }
        match self.rotate() {
            Ok(()) => {
                self.rotation_failing = None;
                Ok(())
            }
            Err(e) => {
                let since = self.rotation_failing.map_or(now, |(since, _)| since);
                self.rotation_failing = Some((since, now));
                if now.duration_since(since) >= self.rotation_retry_limit {
                    return Err(format!(
                        "rotate: {e} (failing for {} s)",
                        now.duration_since(since).as_secs()
                    ));
                }
                if self.active.is_none() {
                    self.reopen()
                        .map_err(|e| format!("reopen after a failed rotation: {e}"))?;
                }
                Ok(())
            }
        }
    }

    /// Write one record (without its newline), within the bounds.
    fn write_record(&mut self, record: &[u8]) {
        if self.failed {
            return;
        }
        let diagnostic;
        let record = if record.len() + 1 > MAX_RECORD_BYTES {
            diagnostic = oversized_record(record.len());
            diagnostic.as_bytes()
        } else {
            record
        };
        let len = record.len() as u64 + 1;
        if let Err(reason) = self.rotate_if_due(len) {
            self.fail(reason);
            return;
        }
        if let Err(first) = self.append(record) {
            // One retry on a reopened file, after a newline that ends any
            // part of the record the failed write left; a failure that
            // persists stops the log.
            let before = self.size;
            let retried = self.reopen().and_then(|()| {
                if self.size > before {
                    self.append_raw(b"\n")?;
                    self.size += 1;
                }
                self.append(record)
            });
            if let Err(second) = retried {
                self.fail(format!("write: {first}; after reopening: {second}"));
                return;
            }
        }
        self.size += len;
    }

    fn append(&mut self, record: &[u8]) -> io::Result<()> {
        let mut line = Vec::with_capacity(record.len() + 1);
        line.extend_from_slice(record);
        line.push(b'\n');
        self.append_raw(&line)
    }

    fn append_raw(&mut self, bytes: &[u8]) -> io::Result<()> {
        let active = self
            .active
            .as_mut()
            .ok_or_else(|| io::Error::other("no active log file"))?;
        active.write_all(bytes)?;
        active.flush()
    }

    fn fail(&mut self, reason: String) {
        self.failed = true;
        self.active = None;
        if let Some(on_failure) = self.on_failure.take() {
            on_failure(reason);
        }
    }
}

impl RotatingLog {
    fn take(&mut self, bytes: &[u8]) {
        self.line_len += bytes.len();
        if self.discarding {
            return;
        }
        if self.pending.len() + bytes.len() + 1 > MAX_RECORD_BYTES {
            self.discarding = true;
            self.pending.clear();
        } else {
            self.pending.extend_from_slice(bytes);
        }
    }

    fn end_line(&mut self) {
        if self.discarding {
            let diagnostic = oversized_record(self.line_len);
            self.write_record(diagnostic.as_bytes());
        } else {
            let line = std::mem::take(&mut self.pending);
            self.write_record(&line);
        }
        self.pending.clear();
        self.discarding = false;
        self.line_len = 0;
    }
}

impl Write for RotatingLog {
    /// `buf` holds whole lines, or part of one: each complete line is
    /// written as a record, the rest waits for its newline.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut rest = buf;
        while let Some(end) = rest.iter().position(|&b| b == b'\n') {
            self.take(&rest[..end]);
            self.end_line();
            rest = &rest[end + 1..];
        }
        self.take(rest);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The diagnostic that replaces a record too long to keep: valid JSON in
/// the shape of the other lines, and bounded.
fn oversized_record(len: usize) -> String {
    serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        "level": "WARN",
        "fields": {
            "message": "a log record was replaced: it exceeded the bound of one record",
            "record_bytes": len,
            "bound_bytes": MAX_RECORD_BYTES,
        },
        "target": "cg_agent::service::rotation",
    })
    .to_string()
}
