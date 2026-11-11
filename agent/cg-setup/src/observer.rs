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
//! The recorded active file is read from its recorded size, wherever rotation
//! has moved it, then each log file whose ID was not present, oldest first
//! ([`log_names`]), each up to its last line end; what each file was read up
//! to is kept by its ID between polls.
//!
//! Every name is opened without following a reparse point and sharing read,
//! write and delete, so that no read keeps the service's rotation from
//! renaming or deleting a file; a name that is not a regular file with one
//! link is not a log file and is not read. The log directory itself is opened
//! the same way, checked to be a directory and no reparse point, and held,
//! sharing no deletion, while a poll reads under it, so that it cannot be
//! replaced meanwhile.
//!
//! A poll lists the log files twice and goes on only when both listings
//! agree, so that what it reads in their order is the order of one moment:
//! rotation only renames, creates and deletes, and a listing taken while it
//! renames differs from the next. It opens each file again to read it and
//! stops at one that has moved since, leaving it and the files after it to
//! the next poll, so that no line is read before one written earlier.

use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use cg_agent::custody::{self, FileId, Handle, Kind};
use cg_agent::service::rotation::{archive_name, ACTIVE, ARCHIVES, MAX_RECORD_BYTES, ROTATING};
use serde::Deserialize;
use windows_sys::Win32::Foundation::{ERROR_SERVICE_DOES_NOT_EXIST, GENERIC_READ};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE,
};
use windows_sys::Win32::System::Services::{
    SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_STOPPED,
};

use crate::codes::{Refusal, NOT_OBSERVABLE, OWN_STATE, PENDING, SERVICE_STOPPED as STOPPED};
use crate::scm::Manager;

/// How long the observer reads, from the start request.
pub const OBSERVE_WITHIN: Duration = Duration::from_secs(60);
/// The agent's identity line: its `fields.message`.
pub const IDENTITY_LINE: &str = "agent identity ready";
/// The agent's heartbeat line: its `fields.message`.
pub const HEARTBEAT_LINE: &str = "signed heartbeat sent";

/// How often the observer reads the state and the log.
const POLL: Duration = Duration::from_millis(500);
/// How long each open of [`observe`]'s polls may wait on another process.
pub const OPEN_BOUND: Duration = Duration::from_secs(2);
/// How many listings a poll takes before it gives up on two that agree.
const LISTINGS: usize = 8;
/// How much of a file one read takes.
const CHUNK: usize = 64 * 1024;

const SHARE_ALL: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;
/// The log directory is held sharing no deletion, so that no one can rename
/// or delete it while a poll reads under it. Windows records a handle's
/// sharing only when it holds a data access, so the hold lists the
/// directory.
const HOLD_SHARE: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE;
const HOLD_ACCESS: u32 = FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES;

/// The log files' names, oldest first: the archives from the highest number,
/// then the file being rotated, then the active file.
pub fn log_names() -> Vec<String> {
    let mut names: Vec<String> = (1..=ARCHIVES).rev().map(archive_name).collect();
    names.push(ROTATING.to_string());
    names.push(ACTIVE.to_string());
    names
}

/// A log file as one listing found it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    name: String,
    id: FileId,
    size: u64,
}

/// The log directory `dir`, opened without following it and held: `None`
/// when it does not exist; refused when it is not a directory, a reparse
/// point included.
fn hold_directory(dir: &Path, wait: Duration) -> Result<Option<Handle>, String> {
    let Some(handle) = custody::open_existing(dir, HOLD_ACCESS, HOLD_SHARE, wait)? else {
        return Ok(None);
    };
    custody::check_shape(&handle, dir, Kind::Directory)?;
    Ok(Some(handle))
}

/// Whether what a handle reports is a log file: a regular file, no reparse
/// point, with one link.
fn is_log_file(info: &BY_HANDLE_FILE_INFORMATION) -> bool {
    info.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY) == 0
        && info.nNumberOfLinks == 1
}

/// The log file `name` in `dir`, opened for `access` without following it and
/// sharing read, write and delete: `None` when nothing is there or it is not
/// a log file.
fn open_log_file(
    dir: &Path,
    name: &str,
    access: u32,
    wait: Duration,
) -> Result<Option<(Handle, BY_HANDLE_FILE_INFORMATION)>, String> {
    let path = dir.join(name);
    let Some(handle) = custody::open_existing(&path, access, SHARE_ALL, wait)? else {
        return Ok(None);
    };
    let info = custody::file_information(&handle, &path)?;
    if !is_log_file(&info) {
        return Ok(None);
    }
    Ok(Some((handle, info)))
}

/// The log files in `dir`, oldest first, as two listings in a row agree on
/// them: names and IDs.
fn listing(dir: &Path, wait: Duration) -> Result<Vec<Entry>, String> {
    let list = || -> Result<Vec<Entry>, String> {
        let mut entries = Vec::new();
        for name in log_names() {
            if let Some((_, info)) = open_log_file(dir, &name, FILE_READ_ATTRIBUTES, wait)? {
                entries.push(Entry {
                    name,
                    id: custody::file_id(&info),
                    size: (u64::from(info.nFileSizeHigh) << 32) | u64::from(info.nFileSizeLow),
                });
            }
        }
        Ok(entries)
    };
    let same = |a: &[Entry], b: &[Entry]| {
        a.len() == b.len()
            && a.iter()
                .zip(b)
                .all(|(a, b)| a.name == b.name && a.id == b.id)
    };
    let mut previous = list()?;
    for _ in 1..LISTINGS {
        let next = list()?;
        if same(&previous, &next) {
            return Ok(next);
        }
        previous = next;
    }
    Err(format!(
        "the log files in {} kept changing while they were listed",
        dir.display()
    ))
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
        let refuse = |reason: String| {
            Refusal::new(
                OWN_STATE,
                format!("the agent's log position cannot be recorded: {reason}"),
            )
        };
        let wait = custody::OPEN_WAIT;
        let Some(_held) = hold_directory(log_dir, wait).map_err(refuse)? else {
            return Ok(LogPosition {
                active: None,
                present: Vec::new(),
            });
        };
        let entries = listing(log_dir, wait).map_err(refuse)?;
        Ok(LogPosition {
            active: entries
                .iter()
                .find(|entry| entry.name == ACTIVE)
                .map(|entry| (entry.id, entry.size)),
            present: entries.iter().map(|entry| entry.id).collect(),
        })
    }

    /// The size recorded of the active file, if there was one.
    pub fn active_size(&self) -> Option<u64> {
        self.active.map(|(_, size)| size)
    }

    /// How many log files were present.
    pub fn present(&self) -> usize {
        self.present.len()
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

/// A log line: only `fields.message` is read, and a line that does not have
/// this form is no line of the two.
#[derive(Deserialize)]
struct Line {
    fields: Option<Fields>,
}

#[derive(Deserialize)]
struct Fields {
    message: Option<String>,
}

/// The log written after a position: each poll reads what is new.
pub struct LogReader {
    dir: PathBuf,
    position: LogPosition,
    /// Each file read so far, by its ID, and the offset after its last line
    /// end read.
    read_to: Vec<(FileId, u64)>,
    seen: Seen,
    error: Option<String>,
}

impl LogReader {
    /// A reader of the log in `log_dir` after `position`.
    pub fn new(log_dir: &Path, position: LogPosition) -> Self {
        LogReader {
            dir: log_dir.to_path_buf(),
            position,
            read_to: Vec::new(),
            seen: Seen::Neither,
            error: None,
        }
    }

    /// Read what was written since the last poll, waiting on another process
    /// no longer than `wait` for each open; return what has appeared.
    pub fn poll(&mut self, wait: Duration) -> Seen {
        if self.seen != Seen::Both {
            self.error = self.read(wait).err();
        }
        self.seen
    }

    /// Why the last poll could not read the log, if it could not.
    pub fn last_error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Where the file `id` is read from: after what was read of it, the
    /// recorded active file from its recorded size, a new file from its
    /// start; `None` for a file present before the start, which is not read.
    fn start_of(&self, id: FileId) -> Option<u64> {
        if let Some(&(_, at)) = self.read_to.iter().find(|(read, _)| *read == id) {
            return Some(at);
        }
        match self.position.active {
            Some((active, size)) if active == id => Some(size),
            _ if self.position.present.contains(&id) => None,
            _ => Some(0),
        }
    }

    fn read(&mut self, wait: Duration) -> Result<(), String> {
        let Some(_held) = hold_directory(&self.dir, wait)? else {
            return Ok(());
        };
        // The recorded file first, from its recorded size, then the files
        // that were not present, oldest first.
        let entries = listing(&self.dir, wait)?;
        let recorded = self.position.active.map(|(id, _)| id);
        let (first, rest): (Vec<Entry>, Vec<Entry>) = entries
            .into_iter()
            .partition(|entry| Some(entry.id) == recorded);
        for entry in first.into_iter().chain(rest) {
            let Some(start) = self.start_of(entry.id) else {
                continue;
            };
            let opened = open_log_file(
                &self.dir,
                &entry.name,
                GENERIC_READ | FILE_READ_ATTRIBUTES,
                wait,
            )?;
            // Moved, or no longer a log file, since the listing: it and the
            // files after it are the next poll's.
            let Some((handle, _)) = opened.filter(|(_, info)| custody::file_id(info) == entry.id)
            else {
                return Ok(());
            };
            let path = self.dir.join(&entry.name);
            let end = self.scan(handle, start, &path)?;
            match self.read_to.iter_mut().find(|(read, _)| *read == entry.id) {
                Some((_, at)) => *at = end,
                None => self.read_to.push((entry.id, end)),
            }
            if self.seen == Seen::Both {
                return Ok(());
            }
        }
        Ok(())
    }

    /// Read the file behind `handle` from `start` up to its last line end,
    /// taking each line; return the offset after that line end. A line of
    /// [`MAX_RECORD_BYTES`] or more is no record of the agent's log and is
    /// passed over.
    fn scan(&mut self, handle: Handle, start: u64, path: &Path) -> Result<u64, String> {
        let failed = |e: std::io::Error| format!("read {}: {e}", path.display());
        let mut file = handle.into_file();
        file.seek(SeekFrom::Start(start)).map_err(failed)?;
        let mut chunk = vec![0u8; CHUNK];
        let mut line: Vec<u8> = Vec::new();
        let mut overlong = false;
        let mut offset = start;
        let mut end = start;
        loop {
            let read = file.read(&mut chunk).map_err(failed)?;
            if read == 0 {
                return Ok(end);
            }
            let mut rest = &chunk[..read];
            let mut taken = 0usize;
            while let Some(newline) = rest.iter().position(|&b| b == b'\n') {
                extend(&mut line, &mut overlong, &rest[..newline]);
                if !overlong {
                    self.take(&line);
                }
                line.clear();
                overlong = false;
                taken += newline + 1;
                rest = &rest[newline + 1..];
                end = offset + taken as u64;
                if self.seen == Seen::Both {
                    return Ok(end);
                }
            }
            extend(&mut line, &mut overlong, rest);
            offset += read as u64;
        }
    }

    /// One whole line, without its newline.
    fn take(&mut self, line: &[u8]) {
        let Ok(Line {
            fields: Some(Fields {
                message: Some(message),
            }),
        }) = serde_json::from_slice::<Line>(line)
        else {
            return;
        };
        self.seen = match (self.seen, message.as_str()) {
            (Seen::Neither, IDENTITY_LINE) => Seen::Identity,
            (Seen::Identity, HEARTBEAT_LINE) => Seen::Both,
            (seen, _) => seen,
        };
    }
}

/// Add `bytes` to the line being read, unless it outgrows a record.
fn extend(line: &mut Vec<u8>, overlong: &mut bool, bytes: &[u8]) {
    if *overlong {
        return;
    }
    if line.len() + bytes.len() >= MAX_RECORD_BYTES {
        *overlong = true;
        line.clear();
    } else {
        line.extend_from_slice(bytes);
    }
}

/// Observe the service `name`, started at `started`, and the log in
/// `log_dir` after `position`, for [`OBSERVE_WITHIN`] from `started`: `Ok`
/// once it is `RUNNING` and both lines have appeared in order; 42 when it
/// reports `STOPPED`, 43 when it is still pending at the end, 44 when it is
/// `RUNNING` without both lines then, or when its state cannot be read.
///
/// The state is read every [`POLL`], and the log after each reading of
/// `RUNNING`: operational is `RUNNING`, then the two lines, and the agent
/// writes both after it reports `RUNNING`. Success is such a poll that
/// found both lines. Once the end has passed, one more reading of the state
/// decides: `STOPPED` is 42, any state but `RUNNING` 43; `RUNNING` is
/// read once more for the lines written up to the end, and is 44 without
/// both. Each open waits on another process no longer than [`OPEN_BOUND`],
/// so that the last poll ends soon after the end. A log that cannot be read
/// is no line: the reason goes in 44's line.
pub fn observe(
    scm: &Manager,
    name: &str,
    log_dir: &Path,
    position: LogPosition,
    started: Instant,
) -> Result<(), Refusal> {
    let end = started + OBSERVE_WITHIN;
    let within = OBSERVE_WITHIN.as_secs();
    // After a start the SCM accepted, a state that cannot be read is no
    // observation (SPEC-024 Amendment 2026-11-11, item 4).
    let unreadable = |error: u32| {
        Refusal::new(
            NOT_OBSERVABLE,
            format!(
                "the state of the service {name} cannot be read after its start: Win32 error {error}"
            ),
        )
    };
    let service = match scm.service(name, SERVICE_QUERY_STATUS) {
        Ok(Some(service)) => service,
        Ok(None) => return Err(unreadable(ERROR_SERVICE_DOES_NOT_EXIST)),
        Err(error) => return Err(unreadable(error)),
    };
    let mut reader = LogReader::new(log_dir, position);
    loop {
        let at_end = Instant::now() >= end;
        let status = service.status().map_err(unreadable)?;
        let state = status.dwCurrentState;
        if state == SERVICE_STOPPED {
            return Err(Refusal::new(
                STOPPED,
                format!(
                    "the service {name} stopped after its start: Win32 exit code {}, \
                     service-specific exit code {}",
                    status.dwWin32ExitCode, status.dwServiceSpecificExitCode
                ),
            ));
        }
        if at_end {
            if state != SERVICE_RUNNING {
                return Err(Refusal::new(
                    PENDING,
                    format!(
                        "the service {name} is in state {state}, not RUNNING, {within} seconds \
                         after its start request"
                    ),
                ));
            }
            // The lines written up to the end.
            let seen = reader.poll(OPEN_BOUND);
            if seen == Seen::Both {
                return Ok(());
            }
            let missing = match seen {
                Seen::Identity => format!("\"{HEARTBEAT_LINE}\" after \"{IDENTITY_LINE}\""),
                _ => format!("\"{IDENTITY_LINE}\" and then \"{HEARTBEAT_LINE}\""),
            };
            let reason = reader
                .last_error()
                .map(|error| format!("; its log could not be read: {error}"))
                .unwrap_or_default();
            return Err(Refusal::new(
                NOT_OBSERVABLE,
                format!(
                    "the service {name} is RUNNING, but its log in {} did not show {missing} \
                     within {within} seconds of its start request{reason}",
                    log_dir.display()
                ),
            ));
        }
        if state == SERVICE_RUNNING && reader.poll(OPEN_BOUND) == Seen::Both {
            return Ok(());
        }
        std::thread::sleep(POLL.min(end.saturating_duration_since(Instant::now())));
    }
}
