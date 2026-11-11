//! Configure's observer on temporary log directories (SPEC-024 Amendment
//! 2026-11-10, item 8's outcome): the log's position recorded before the
//! start, the active file read from its recorded size wherever rotation moves
//! it, then each log file that was not present, oldest first, each up to its
//! last line end; the identity line and then the heartbeat line, each a JSON
//! line whose `fields.message` is that text; and names that are not log files
//! left unread. The logs are written as the agent's rotation writes them:
//! by hand, renaming in the order `cg_agent::service::rotation` renames, and
//! through that rotation itself. The service's state and the 42 to 44 are
//! tested against real services, in the SCM target.
#![cfg(windows)]

mod fixtures;

use std::io::Write as _;
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cg_agent::service::rotation::{
    archive_name, RotatingLog, ACTIVE, ARCHIVES, MAX_FILE_BYTES, MAX_RECORD_BYTES, ROTATING,
};
use cg_setup::codes;
use cg_setup::observer::{log_names, LogPosition, LogReader, Seen, HEARTBEAT_LINE, IDENTITY_LINE};

use fixtures::junction;

const WAIT: Duration = Duration::from_secs(5);

// --- Helpers -----------------------------------------------------------------------

/// A JSON line as the agent's log writes one, with `message` as its
/// `fields.message`, and its newline.
fn line(message: &str) -> String {
    format!(
        "{}\n",
        serde_json::json!({
            "timestamp": "2026-11-11T10:00:00.000000Z",
            "level": "INFO",
            "fields": { "message": message, "component": "test" },
            "target": "cg_agent",
        })
    )
}

fn append(dir: &Path, name: &str, text: &str) {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(name))
        .unwrap_or_else(|e| panic!("open {name}: {e}"));
    file.write_all(text.as_bytes())
        .unwrap_or_else(|e| panic!("write {name}: {e}"));
}

/// A log directory, `logs`, in a temporary directory of the test's.
struct Logs {
    _root: tempfile::TempDir,
    dir: PathBuf,
}

impl Logs {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("a temporary directory");
        let dir = root.path().join("logs");
        std::fs::create_dir(&dir).expect("create logs");
        Logs { _root: root, dir }
    }

    /// A `logs` that does not exist yet.
    fn absent() -> Self {
        let root = tempfile::tempdir().expect("a temporary directory");
        let dir = root.path().join("logs");
        Logs { _root: root, dir }
    }

    fn append(&self, name: &str, text: &str) {
        append(&self.dir, name, text);
    }

    fn reader(&self) -> LogReader {
        LogReader::new(&self.dir, self.position())
    }

    fn position(&self) -> LogPosition {
        LogPosition::record(&self.dir).unwrap_or_else(|r| panic!("record: {r:?}"))
    }

    /// Rotate as the agent's rotation does, one rename at a time, polling
    /// `reader` after each step: the active file to `ROTATING`, a new active
    /// file, the archives shifted into the first free place, `ROTATING` to
    /// archive 1.
    fn rotate_polling(&self, reader: &mut LogReader) {
        std::fs::rename(self.dir.join(ACTIVE), self.dir.join(ROTATING)).expect("to rotating");
        reader.poll(WAIT);
        self.append(ACTIVE, "");
        reader.poll(WAIT);
        let free = (1..=ARCHIVES)
            .find(|&n| !self.dir.join(archive_name(n)).exists())
            .unwrap_or_else(|| {
                std::fs::remove_file(self.dir.join(archive_name(ARCHIVES))).expect("the oldest");
                ARCHIVES
            });
        for n in (1..free).rev() {
            std::fs::rename(
                self.dir.join(archive_name(n)),
                self.dir.join(archive_name(n + 1)),
            )
            .expect("shift an archive");
            reader.poll(WAIT);
        }
        std::fs::rename(self.dir.join(ROTATING), self.dir.join(archive_name(1)))
            .expect("to archive 1");
    }
}

fn assert_seen(reader: &mut LogReader, expected: Seen, what: &str) {
    let seen = reader.poll(WAIT);
    assert_eq!(
        seen,
        expected,
        "{what} (last error: {:?})",
        reader.last_error()
    );
}

// --- The names and the position ------------------------------------------------------

#[test]
fn package_observer_the_two_lines_are_the_agents() {
    // The texts cg-agent's install_ac_010 pins in the service's own log.
    assert_eq!(IDENTITY_LINE, "agent identity ready");
    assert_eq!(HEARTBEAT_LINE, "signed heartbeat sent");
}

#[test]
fn package_observer_the_log_names_are_oldest_first() {
    let mut expected: Vec<String> = (1..=ARCHIVES).rev().map(archive_name).collect();
    expected.push(ROTATING.to_string());
    expected.push(ACTIVE.to_string());
    assert_eq!(log_names(), expected);
    assert_eq!(
        log_names()[..3],
        ["agent.5.jsonl", "agent.4.jsonl", "agent.3.jsonl"]
    );
    assert_eq!(
        log_names()[5..],
        ["agent.rotating.jsonl", "agent.jsonl"],
        "the rotating file, then the active one"
    );
}

#[test]
fn package_observer_records_the_active_files_size_and_the_files_present() {
    let logs = Logs::new();
    logs.append(&archive_name(2), &line("old"));
    logs.append(&archive_name(1), &line("old"));
    let active = format!("{}{}", line(IDENTITY_LINE), line(HEARTBEAT_LINE));
    logs.append(ACTIVE, &active);
    let position = logs.position();
    assert_eq!(position.active_size(), Some(active.len() as u64));
    assert_eq!(position.present(), 3, "two archives and the active file");

    let none = Logs::new();
    logs.append(ROTATING, &line("old"));
    let position = none.position();
    assert_eq!(position.active_size(), None, "no active file");
    assert_eq!(position.present(), 0);
    assert_eq!(logs.position().present(), 4, "the rotating file counts");
}

#[test]
fn package_observer_no_log_directory_records_no_file_and_reads_a_new_log() {
    let logs = Logs::absent();
    let position = logs.position();
    assert_eq!(position.active_size(), None);
    assert_eq!(position.present(), 0);
    let mut reader = LogReader::new(&logs.dir, position);
    assert_seen(&mut reader, Seen::Neither, "no directory yet");
    assert_eq!(reader.last_error(), None, "an absent directory is no error");
    std::fs::create_dir(&logs.dir).expect("create logs");
    logs.append(ACTIVE, &line(IDENTITY_LINE));
    assert_seen(&mut reader, Seen::Identity, "the identity line");
    logs.append(ACTIVE, &line(HEARTBEAT_LINE));
    assert_seen(&mut reader, Seen::Both, "then the heartbeat line");
}

// --- What is read ------------------------------------------------------------------

#[test]
fn package_observer_reads_the_active_file_from_its_recorded_size() {
    let logs = Logs::new();
    let both = format!("{}{}", line(IDENTITY_LINE), line(HEARTBEAT_LINE));
    logs.append(&archive_name(1), &both);
    logs.append(ACTIVE, &both);
    let mut reader = logs.reader();
    assert_seen(
        &mut reader,
        Seen::Neither,
        "the lines before the position are an earlier run's",
    );
    logs.append(ACTIVE, &line("agent starting"));
    logs.append(ACTIVE, &line(IDENTITY_LINE));
    assert_seen(&mut reader, Seen::Identity, "the identity line after it");
    logs.append(ACTIVE, &line(HEARTBEAT_LINE));
    assert_seen(&mut reader, Seen::Both, "then the heartbeat line");
}

#[test]
fn package_observer_the_two_lines_count_in_order() {
    let logs = Logs::new();
    let mut reader = logs.reader();
    logs.append(ACTIVE, &line(HEARTBEAT_LINE));
    assert_seen(&mut reader, Seen::Neither, "a heartbeat line alone");
    logs.append(ACTIVE, &line(IDENTITY_LINE));
    assert_seen(
        &mut reader,
        Seen::Identity,
        "the identity line after the heartbeat line is the identity alone",
    );
    logs.append(ACTIVE, &line(HEARTBEAT_LINE));
    assert_seen(&mut reader, Seen::Both, "a heartbeat line after it");

    // Both in one poll, out of order, then in order.
    let logs = Logs::new();
    let mut reader = logs.reader();
    logs.append(
        ACTIVE,
        &format!("{}{}", line(HEARTBEAT_LINE), line(IDENTITY_LINE)),
    );
    assert_seen(&mut reader, Seen::Identity, "out of order in one read");
    let logs = Logs::new();
    let mut reader = logs.reader();
    logs.append(
        ACTIVE,
        &format!("{}{}", line(IDENTITY_LINE), line(HEARTBEAT_LINE)),
    );
    assert_seen(&mut reader, Seen::Both, "in order in one read");
}

#[test]
fn package_observer_only_a_json_lines_message_counts() {
    let logs = Logs::new();
    let mut reader = logs.reader();
    let near_misses = [
        format!("{IDENTITY_LINE}\n"),
        format!("not json: {{\"fields\":{{\"message\":\"{IDENTITY_LINE}\"}}}}\n"),
        format!(
            "{}\n",
            serde_json::json!({ "fields": { "text": IDENTITY_LINE } })
        ),
        format!(
            "{}\n",
            serde_json::json!({ "message": IDENTITY_LINE, "fields": {} })
        ),
        format!(
            "{}\n",
            serde_json::json!({ "fields": { "message": format!("{IDENTITY_LINE}!") } })
        ),
        format!(
            "{}\n",
            serde_json::json!({ "fields": { "message": [IDENTITY_LINE] } })
        ),
        format!(
            "{}\n",
            serde_json::json!([{ "fields": { "message": IDENTITY_LINE } }])
        ),
    ];
    for text in &near_misses {
        logs.append(ACTIVE, text);
    }
    assert_seen(
        &mut reader,
        Seen::Neither,
        "no line whose message is the text",
    );
    logs.append(
        ACTIVE,
        &format!(
            "{}\n",
            serde_json::json!({ "fields": { "message": IDENTITY_LINE } })
        ),
    );
    assert_seen(
        &mut reader,
        Seen::Identity,
        "a JSON line whose message it is",
    );
}

#[test]
fn package_observer_a_line_counts_once_it_ends() {
    let logs = Logs::new();
    let mut reader = logs.reader();
    let identity = line(IDENTITY_LINE);
    let (head, tail) = identity.split_at(identity.len() / 2);
    logs.append(ACTIVE, head);
    assert_seen(&mut reader, Seen::Neither, "half a line");
    let without_newline = &tail[..tail.len() - 1];
    logs.append(ACTIVE, without_newline);
    assert_seen(&mut reader, Seen::Neither, "a line without its end");
    logs.append(ACTIVE, "\n");
    assert_seen(&mut reader, Seen::Identity, "the line, once it ends");
}

#[test]
fn package_observer_a_line_longer_than_a_record_does_not_stop_the_reading() {
    let logs = Logs::new();
    let mut reader = logs.reader();
    let long = format!(
        "{}\n",
        serde_json::json!({
            "fields": { "message": "padding", "padding": "x".repeat(2 * MAX_RECORD_BYTES) }
        })
    );
    logs.append(ACTIVE, &long);
    logs.append(ACTIVE, &line(IDENTITY_LINE));
    assert_seen(&mut reader, Seen::Identity, "the line after the long one");
    logs.append(ACTIVE, &long);
    logs.append(ACTIVE, &long);
    logs.append(ACTIVE, &line(HEARTBEAT_LINE));
    assert_seen(&mut reader, Seen::Both, "after two more");
}

#[test]
fn package_observer_reads_the_active_file_while_the_agent_holds_it() {
    // As the service holds its log: appending, sharing read alone.
    let logs = Logs::new();
    let mut reader = logs.reader();
    let mut held = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ)
        .open(logs.dir.join(ACTIVE))
        .expect("open as the service does");
    held.write_all(line(IDENTITY_LINE).as_bytes())
        .expect("the identity line");
    assert_seen(&mut reader, Seen::Identity, "while the writer holds it");
    held.write_all(line(HEARTBEAT_LINE).as_bytes())
        .expect("the heartbeat line");
    assert_seen(&mut reader, Seen::Both, "still held");
    assert_eq!(reader.last_error(), None);
}

#[test]
fn package_observer_a_line_across_a_read_chunk_counts() {
    // The reader takes 64 KiB at a time: a filler line just under a record's
    // bound puts the identity line across the first 64 KiB of the file.
    let logs = Logs::new();
    let mut reader = logs.reader();
    let filler = format!("{}\n", "f".repeat(64 * 1024 - 20 - 1));
    assert!(filler.len() < MAX_RECORD_BYTES);
    logs.append(ACTIVE, &filler);
    let identity = line(IDENTITY_LINE);
    assert!(filler.len() < 64 * 1024 && filler.len() + identity.len() > 64 * 1024);
    logs.append(ACTIVE, &identity);
    logs.append(ACTIVE, &line(HEARTBEAT_LINE));
    assert_seen(&mut reader, Seen::Both, "a line across the chunk's end");
}

#[test]
fn package_observer_a_new_file_at_an_old_files_name_is_read() {
    // What was present is told by its ID, not its name.
    let logs = Logs::new();
    logs.append(&archive_name(1), &line(IDENTITY_LINE));
    logs.append(ACTIVE, &line("old"));
    let mut reader = logs.reader();
    std::fs::remove_file(logs.dir.join(archive_name(1))).expect("remove archive 1");
    logs.append(&archive_name(1), &line(IDENTITY_LINE));
    assert_seen(&mut reader, Seen::Identity, "a new file named archive 1");
}

// --- Rotation ------------------------------------------------------------------------

#[test]
fn package_observer_follows_the_recorded_file_through_rotation() {
    let logs = Logs::new();
    let both = format!("{}{}", line(IDENTITY_LINE), line(HEARTBEAT_LINE));
    logs.append(&archive_name(1), &both);
    logs.append(ACTIVE, &both);
    let mut reader = logs.reader();
    // The identity line in the recorded file, after its size, read once it
    // has moved to the rotating file and then to an archive.
    logs.append(ACTIVE, &line(IDENTITY_LINE));
    logs.rotate_polling(&mut reader);
    assert_seen(
        &mut reader,
        Seen::Identity,
        "the recorded file, now archive 1",
    );
    logs.append(ACTIVE, &line(HEARTBEAT_LINE));
    assert_seen(&mut reader, Seen::Both, "then the new active file");
}

#[test]
fn package_observer_follows_a_file_read_under_one_name_by_its_id() {
    // Read as the active file, then rotated: the new active file, shorter
    // than what was read of the old one, is read from its start.
    let logs = Logs::new();
    let mut reader = logs.reader();
    logs.append(ACTIVE, &line(IDENTITY_LINE));
    for _ in 0..20 {
        logs.append(ACTIVE, &line("filler"));
    }
    assert_seen(&mut reader, Seen::Identity, "the active file, read");
    logs.rotate_polling(&mut reader);
    logs.append(ACTIVE, &line(HEARTBEAT_LINE));
    assert_seen(
        &mut reader,
        Seen::Both,
        "the new active file, from its start",
    );
}

#[test]
fn package_observer_reads_the_recorded_files_tail_before_the_files_after_it() {
    // Nothing is polled until two rotations are over: the recorded file is
    // archive 2, its tail holds the identity line, archive 1 the heartbeat
    // line.
    let logs = Logs::new();
    logs.append(ACTIVE, &line("before the position"));
    let position = logs.position();
    logs.append(ACTIVE, &line(IDENTITY_LINE));
    let mut ignored = LogReader::new(&logs.dir, position.clone());
    logs.rotate_polling(&mut ignored);
    logs.append(ACTIVE, &line(HEARTBEAT_LINE));
    logs.rotate_polling(&mut ignored);
    let mut reader = LogReader::new(&logs.dir, position);
    assert_seen(&mut reader, Seen::Both, "archive 2's tail, then archive 1");
}

#[test]
fn package_observer_reads_the_recorded_file_before_the_new_files() {
    // A new file at an older name than the recorded file, which rotation does
    // not make: the recorded file's tail is still read first.
    let logs = Logs::new();
    logs.append(ACTIVE, &line("before the position"));
    let mut reader = logs.reader();
    logs.append(ACTIVE, &line(IDENTITY_LINE));
    logs.append(&archive_name(1), &line(HEARTBEAT_LINE));
    assert_seen(
        &mut reader,
        Seen::Both,
        "the recorded file's identity line, then the new archive's heartbeat",
    );
}

#[test]
fn package_observer_reads_new_files_oldest_first() {
    // Archives written after the position, read from the highest number.
    let logs = Logs::new();
    let mut reader = logs.reader();
    logs.append(&archive_name(2), &line(IDENTITY_LINE));
    logs.append(&archive_name(1), &line(HEARTBEAT_LINE));
    assert_seen(&mut reader, Seen::Both, "archive 2, then archive 1");

    let logs = Logs::new();
    let mut reader = logs.reader();
    logs.append(&archive_name(2), &line(HEARTBEAT_LINE));
    logs.append(&archive_name(1), &line(IDENTITY_LINE));
    assert_seen(
        &mut reader,
        Seen::Identity,
        "archive 2's heartbeat line precedes archive 1's identity line",
    );

    let logs = Logs::new();
    let mut reader = logs.reader();
    logs.append(&archive_name(1), &line(IDENTITY_LINE));
    logs.append(ROTATING, &line("rotating"));
    logs.append(ACTIVE, &line(HEARTBEAT_LINE));
    assert_seen(&mut reader, Seen::Both, "archive 1, rotating, active");

    let logs = Logs::new();
    let mut reader = logs.reader();
    logs.append(ROTATING, &line(HEARTBEAT_LINE));
    logs.append(ACTIVE, &line(IDENTITY_LINE));
    assert_seen(
        &mut reader,
        Seen::Identity,
        "the rotating file precedes the active one",
    );
}

#[test]
fn package_observer_files_present_before_the_start_are_not_read() {
    let logs = Logs::new();
    for n in 1..=ARCHIVES {
        logs.append(&archive_name(n), &line(IDENTITY_LINE));
    }
    logs.append(ROTATING, &line(IDENTITY_LINE));
    let mut reader = logs.reader();
    logs.append(&archive_name(1), &line("appended to an old archive"));
    logs.append(ACTIVE, &line(HEARTBEAT_LINE));
    assert_seen(
        &mut reader,
        Seen::Neither,
        "the identity lines are in files present at the position",
    );
}

#[test]
fn package_observer_follows_the_agents_own_rotation() {
    // The agent's rotation itself, rotating three times, with a reader that
    // polls throughout: its reads never keep a rename from happening, and it
    // follows each file by its ID while the writer holds the active one as
    // the service does.
    let logs = Logs::new();
    logs.append(ACTIVE, &line("before the position"));
    let position = logs.position();
    let failed = Arc::new(AtomicBool::new(false));
    let mut log = RotatingLog::open(
        &logs.dir,
        Box::new(|path| {
            // As the service opens its log: appending, sharing read alone.
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ)
                .open(path)?;
            let size = file.metadata()?.len();
            Ok((Box::new(file) as Box<dyn std::io::Write + Send>, size))
        }),
    )
    .expect("open the rotating log");
    let flag = Arc::clone(&failed);
    log.on_failure(Box::new(move |_| flag.store(true, Ordering::SeqCst)));
    // A rotation that fails is retried for this long before it counts.
    log.set_rotation_retry_limit(Duration::ZERO);

    let done = Arc::new(AtomicBool::new(false));
    let poller = {
        let (dir, done) = (logs.dir.clone(), Arc::clone(&done));
        let position = position.clone();
        std::thread::spawn(move || {
            let mut reader = LogReader::new(&dir, position);
            let mut polls = 0u32;
            while !done.load(Ordering::SeqCst) {
                reader.poll(WAIT);
                polls += 1;
            }
            // Once more after the last line, the writer still open.
            let seen = reader.poll(WAIT);
            (polls, seen, reader.last_error().map(str::to_string))
        })
    };
    log.write_all(line(IDENTITY_LINE).as_bytes())
        .expect("the identity line");
    let filler = line(&"f".repeat(1000));
    let fill = 3 * MAX_FILE_BYTES / filler.len() as u64 + 1;
    for _ in 0..fill {
        log.write_all(filler.as_bytes()).expect("a filler line");
    }
    log.write_all(line(HEARTBEAT_LINE).as_bytes())
        .expect("the heartbeat line");
    done.store(true, Ordering::SeqCst);
    let (polls, seen, error) = poller.join().expect("the poller");
    drop(log);
    assert_eq!(
        (seen, error),
        (Seen::Both, None),
        "the reader that polled throughout, with the active file held open"
    );

    assert!(!failed.load(Ordering::SeqCst), "the log never failed");
    assert!(
        polls > 1,
        "the reader polled while the log rotated: {polls}"
    );
    for n in 1..=3 {
        let size = std::fs::metadata(logs.dir.join(archive_name(n)))
            .unwrap_or_else(|e| panic!("archive {n}: {e}"))
            .len();
        assert!(
            size <= MAX_FILE_BYTES,
            "archive {n} is within its bound: {size}"
        );
    }
    assert!(
        !logs.dir.join(archive_name(4)).exists(),
        "three rotations, no fourth"
    );
    let mut reader = LogReader::new(&logs.dir, position);
    assert_seen(
        &mut reader,
        Seen::Both,
        "the identity line in archive 3, the heartbeat in the active file",
    );
}

// --- Names that are not log files ------------------------------------------------------

#[test]
fn package_observer_a_name_that_is_not_a_log_file_is_not_read() {
    let logs = Logs::new();
    let elsewhere = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(
        elsewhere.path().join(ACTIVE),
        format!("{}{}", line(IDENTITY_LINE), line(HEARTBEAT_LINE)),
    )
    .expect("write elsewhere");
    // A junction and a directory at log names, before and after the position.
    junction(&logs.dir.join(archive_name(2)), elsewhere.path());
    std::fs::create_dir(logs.dir.join(archive_name(3))).expect("a directory");
    let position = logs.position();
    assert_eq!(position.present(), 0, "neither is a log file");
    let mut reader = LogReader::new(&logs.dir, position);
    junction(&logs.dir.join(archive_name(1)), elsewhere.path());
    std::fs::create_dir(logs.dir.join(ROTATING)).expect("a directory");
    assert_seen(&mut reader, Seen::Neither, "nothing read through them");
    logs.append(ACTIVE, &line(IDENTITY_LINE));
    logs.append(ACTIVE, &line(HEARTBEAT_LINE));
    assert_seen(&mut reader, Seen::Both, "the log files are still read");
}

#[test]
fn package_observer_a_hard_linked_log_file_is_not_read() {
    let logs = Logs::new();
    let elsewhere = tempfile::tempdir().expect("a temporary directory");
    let target = elsewhere.path().join("other.jsonl");
    std::fs::write(
        &target,
        format!("{}{}", line(IDENTITY_LINE), line(HEARTBEAT_LINE)),
    )
    .expect("write elsewhere");
    let mut reader = logs.reader();
    std::fs::hard_link(&target, logs.dir.join(ACTIVE)).expect("a hard link");
    assert_seen(&mut reader, Seen::Neither, "a hard-linked name is not read");
}

#[test]
fn package_observer_a_log_directory_that_is_not_a_directory_is_40_and_not_read() {
    let elsewhere = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(
        elsewhere.path().join(ACTIVE),
        format!("{}{}", line(IDENTITY_LINE), line(HEARTBEAT_LINE)),
    )
    .expect("write elsewhere");

    let logs = Logs::absent();
    junction(&logs.dir, elsewhere.path());
    let refusal = LogPosition::record(&logs.dir).expect_err("a junction at the log directory");
    assert_eq!(refusal.code, codes::OWN_STATE, "{refusal:?}");
    assert!(refusal.line.starts_with("cg-setup: "), "{refusal:?}");
    assert!(refusal.line.contains("reparse point"), "{refusal:?}");

    let file = Logs::absent();
    std::fs::write(&file.dir, "a file").expect("a file at logs");
    let refusal = LogPosition::record(&file.dir).expect_err("a file at the log directory");
    assert_eq!(refusal.code, codes::OWN_STATE, "{refusal:?}");

    // Recorded on a directory that a junction then replaces.
    let logs = Logs::new();
    let mut reader = logs.reader();
    std::fs::remove_dir(&logs.dir).expect("remove logs");
    junction(&logs.dir, elsewhere.path());
    assert_seen(&mut reader, Seen::Neither, "nothing read through it");
    let error = reader.last_error().expect("the reason it read nothing");
    assert!(error.contains("reparse point"), "{error}");
}
