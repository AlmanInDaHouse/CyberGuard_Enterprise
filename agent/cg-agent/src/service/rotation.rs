//! The service's size-bounded JSON-lines log (SPEC-024 §Operational §3,
//! install_ac_010): one active file and five archives, each at most
//! 10 MiB. Before a record would take the active file past its bound, the
//! archives shift (`agent.4.jsonl` → `agent.5.jsonl`, …, the oldest
//! deleted), the active file becomes `agent.1.jsonl`, and a new one is
//! opened. Renames keep each file's own security, so rotation broadens no
//! access; the opener gives a new active file its final security at
//! creation.
//!
//! A record is one line. A record longer than [`MAX_RECORD_BYTES`] is not
//! split: a bounded diagnostic line, valid JSON, takes its place. A failed
//! write is retried once on a reopened file; if that fails too the log
//! stops writing and reports the failure once, through `on_failure`, so the
//! service can stop with code 13.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// The bound of the active file and of each archive.
pub const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
/// The number of archives kept.
pub const ARCHIVES: usize = 5;
/// The longest record written as it is, its newline included.
pub const MAX_RECORD_BYTES: usize = 64 * 1024;

/// The active file's name.
pub const ACTIVE: &str = "agent.jsonl";

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
        };
        log.reopen()?;
        if log.size >= MAX_FILE_BYTES {
            log.rotate()?;
        }
        Ok(log)
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

    /// Shift the archives and open a new active file.
    fn rotate(&mut self) -> io::Result<()> {
        if let Some(mut active) = self.active.take() {
            let _ = active.flush();
        }
        let oldest = self.dir.join(archive_name(ARCHIVES));
        match std::fs::remove_file(&oldest) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        for n in (1..ARCHIVES).rev() {
            let from = self.dir.join(archive_name(n));
            if from.exists() {
                std::fs::rename(&from, self.dir.join(archive_name(n + 1)))?;
            }
        }
        let active = self.dir.join(ACTIVE);
        if active.exists() {
            std::fs::rename(&active, self.dir.join(archive_name(1)))?;
        }
        self.reopen()
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
        if self.size > 0 && self.size + len > MAX_FILE_BYTES {
            if let Err(e) = self.rotate() {
                self.fail(format!("rotate: {e}"));
                return;
            }
        }
        if let Err(first) = self.append(record) {
            // One retry on a reopened file; a failure that persists stops
            // the log.
            let retried = self.reopen().and_then(|()| self.append(record));
            if let Err(second) = retried {
                self.fail(format!("write: {first}; after reopening: {second}"));
                return;
            }
        }
        self.size += len;
    }

    fn append(&mut self, record: &[u8]) -> io::Result<()> {
        let active = self
            .active
            .as_mut()
            .ok_or_else(|| io::Error::other("no active log file"))?;
        let mut line = Vec::with_capacity(record.len() + 1);
        line.extend_from_slice(record);
        line.push(b'\n');
        active.write_all(&line)?;
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
