//! SPEC-024 install_ac_010, the service's file log (§Operational §3): one
//! active file plus five archives of at most 10 MiB each, valid bounded
//! JSON lines, rotation that keeps each file's protection, and a write
//! failure that persists reported once so the service stops with code 13.
//! The rotation is exercised in a temporary directory through a plain
//! opener; the service's own opener, which gives the file its SYSTEM
//! security, runs under the SCM (`install_ac_007_service.rs`).

mod common;

use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use cg_agent::service::rotation::{
    archive_name, Opener, RotatingLog, ACTIVE, ARCHIVES, MAX_FILE_BYTES, MAX_RECORD_BYTES,
};
use serde_json::Value;

fn plain_opener() -> Opener {
    Box::new(|path: &Path| {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        let size = file.metadata()?.len();
        Ok((Box::new(file) as Box<dyn Write + Send>, size))
    })
}

/// A JSON line in the shape of the agent's own, about `pad` bytes long.
fn record(n: usize, pad: usize) -> String {
    serde_json::json!({
        "timestamp": "2026-11-03T00:00:00.000000Z",
        "level": "INFO",
        "fields": { "message": "record", "n": n, "pad": "x".repeat(pad) },
        "target": "install_ac_010",
    })
    .to_string()
}

/// The JSON lines of a file, each one parsed.
fn lines(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
        .lines()
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("{}: not a JSON line ({e}): {line:.80}", path.display()))
        })
        .collect()
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// One active file and five archives, each within 10 MiB, every line valid
/// JSON, and the newest records in the active file, continuing the newest
/// archive.
#[test]
fn install_ac_010_rotation_keeps_one_active_and_five_archives_within_10_mib() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut log = RotatingLog::open(dir.path(), plain_opener()).expect("open the log");
    let total = 70 * 1024; // about 70 MiB of ~1 KiB records
    for n in 0..total {
        writeln!(log, "{}", record(n, 1000)).expect("write");
    }
    drop(log);

    let mut expected: Vec<String> = (1..=ARCHIVES).map(archive_name).collect();
    expected.push(ACTIVE.to_string());
    expected.sort();
    assert_eq!(
        names(dir.path()),
        expected,
        "one active file and five archives"
    );
    for name in &expected {
        let path = dir.path().join(name);
        let size = std::fs::metadata(&path).expect("metadata").len();
        assert!(size <= MAX_FILE_BYTES, "{name} is {size} bytes");
        assert!(!lines(&path).is_empty(), "{name} holds records");
    }
    let active = lines(&dir.path().join(ACTIVE));
    let newest_archive = lines(&dir.path().join(archive_name(1)));
    assert_eq!(
        active.last().expect("a record")["fields"]["n"],
        total - 1,
        "the newest record is in the active file"
    );
    assert_eq!(
        newest_archive.last().expect("a record")["fields"]["n"].as_u64(),
        active[0]["fields"]["n"].as_u64().map(|n| n - 1),
        "the active file continues the newest archive"
    );
}

/// A record longer than the bound is replaced by one bounded diagnostic
/// line, valid JSON, never split; the lines around it are kept.
#[test]
fn install_ac_010_an_oversized_record_is_replaced_by_a_bounded_diagnostic() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut log = RotatingLog::open(dir.path(), plain_opener()).expect("open the log");
    writeln!(log, "{}", record(1, 10)).expect("write");
    let oversized = record(2, 100 * 1024);
    // The oversized line arrives in pieces, its newline last.
    let (head, tail) = oversized.split_at(40 * 1024);
    log.write_all(head.as_bytes()).expect("write");
    log.write_all(tail.as_bytes()).expect("write");
    log.write_all(b"\n").expect("write");
    writeln!(log, "{}", record(3, 10)).expect("write");
    drop(log);

    let raw = std::fs::read_to_string(dir.path().join(ACTIVE)).expect("read");
    assert!(
        raw.lines().all(|line| line.len() < MAX_RECORD_BYTES),
        "no line exceeds the bound"
    );
    let records = lines(&dir.path().join(ACTIVE));
    assert_eq!(records.len(), 3, "three lines: {raw:.300}");
    assert_eq!(records[0]["fields"]["n"], 1);
    assert_eq!(records[1]["level"], "WARN", "the diagnostic");
    assert_eq!(
        records[1]["fields"]["record_bytes"].as_u64(),
        Some(oversized.len() as u64),
        "the diagnostic counts the record's bytes"
    );
    assert_eq!(records[2]["fields"]["n"], 3);
}

/// A record written in several pieces is one line.
#[test]
fn install_ac_010_a_record_in_pieces_is_one_line() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut log = RotatingLog::open(dir.path(), plain_opener()).expect("open the log");
    let line = record(7, 100);
    let (a, b) = line.split_at(50);
    log.write_all(a.as_bytes()).expect("write");
    log.write_all(b.as_bytes()).expect("write");
    log.write_all(b"\n").expect("write");
    drop(log);
    let records = lines(&dir.path().join(ACTIVE));
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["fields"]["n"], 7);
}

/// An active file already at its bound is archived when the log opens.
#[test]
fn install_ac_010_an_active_file_at_its_bound_is_archived_at_open() {
    let dir = tempfile::tempdir().expect("tempdir");
    let full = record(0, 1000) + "\n";
    let mut content = String::new();
    while (content.len() as u64) < MAX_FILE_BYTES {
        content.push_str(&full);
    }
    std::fs::write(dir.path().join(ACTIVE), &content).expect("seed");
    let mut log = RotatingLog::open(dir.path(), plain_opener()).expect("open the log");
    writeln!(log, "{}", record(1, 10)).expect("write");
    drop(log);
    assert_eq!(
        std::fs::metadata(dir.path().join(archive_name(1)))
            .expect("archived")
            .len(),
        content.len() as u64
    );
    assert_eq!(lines(&dir.path().join(ACTIVE)).len(), 1);
}

/// A writer that fails every write.
struct Failing;

impl Write for Failing {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("disk full"))
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A write failure that persists through a reopen is reported once, with
/// its reason, and the log stops writing.
#[test]
fn install_ac_010_a_persistent_write_failure_is_reported_once() {
    let dir = tempfile::tempdir().expect("tempdir");
    let opener: Opener = Box::new(|_: &Path| Ok((Box::new(Failing) as Box<dyn Write + Send>, 0)));
    let mut log = RotatingLog::open(dir.path(), opener).expect("open the log");
    let reported = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&reported);
    log.on_failure(Box::new(move |reason| sink.lock().unwrap().push(reason)));
    for n in 0..3 {
        writeln!(log, "{}", record(n, 10)).expect("the log itself never errors");
    }
    let reported = reported.lock().unwrap();
    assert_eq!(reported.len(), 1, "reported once: {reported:?}");
    assert!(reported[0].contains("disk full"), "{}", reported[0]);
}

/// A write that fails once and succeeds on the reopened file is not a
/// failure.
#[test]
fn install_ac_010_a_failure_that_a_reopen_cures_is_not_reported() {
    let dir = tempfile::tempdir().expect("tempdir");
    let opened = Arc::new(Mutex::new(0usize));
    let count = Arc::clone(&opened);
    let opener: Opener = Box::new(move |path: &Path| {
        let mut opened = count.lock().unwrap();
        *opened += 1;
        if *opened == 1 {
            return Ok((Box::new(Failing) as Box<dyn Write + Send>, 0));
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok((Box::new(file) as Box<dyn Write + Send>, 0))
    });
    let mut log = RotatingLog::open(dir.path(), opener).expect("open the log");
    let reported = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = Arc::clone(&reported);
    log.on_failure(Box::new(move |reason| sink.lock().unwrap().push(reason)));
    writeln!(log, "{}", record(1, 10)).expect("write");
    drop(log);
    assert!(reported.lock().unwrap().is_empty());
    assert_eq!(lines(&dir.path().join(ACTIVE)).len(), 1);
}

/// Rotation keeps each file's protection: an archive carries exactly the
/// security descriptor its active file had (renames do not re-inherit).
#[cfg(windows)]
#[test]
fn install_ac_010_rotation_keeps_each_files_protection() {
    use windows_sys::Win32::Foundation::{LocalFree, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW,
        ConvertStringSecurityDescriptorToSecurityDescriptorW, GetNamedSecurityInfoW,
        SDDL_REVISION_1, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::{
        DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_APPEND_DATA, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, OPEN_ALWAYS,
    };

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// A protected DACL, not the temporary directory's inherited one: the
    /// owner keeps full access (so the log can rename it), Administrators
    /// read.
    const SDDL: &str = "D:P(A;;FA;;;OW)(A;;FR;;;BA)";

    fn dacl_of(path: &Path) -> String {
        let name = wide(&path.display().to_string());
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: out-pointers for the DACL inside `descriptor`, freed below.
        let error = unsafe {
            GetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut descriptor,
            )
        };
        assert_eq!(error, 0, "GetNamedSecurityInfoW {}", path.display());
        let mut text: *mut u16 = std::ptr::null_mut();
        // SAFETY: the descriptor above; the string is freed below.
        let ok = unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor,
                SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION,
                &mut text,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(ok, 0, "to SDDL");
        // SAFETY: a NUL-terminated string the call allocated.
        let sddl = unsafe {
            let len = (0..).take_while(|&i| *text.add(i) != 0).count();
            String::from_utf16_lossy(std::slice::from_raw_parts(text, len))
        };
        // SAFETY: both allocated by the calls above, freed once.
        unsafe {
            LocalFree(text.cast());
            LocalFree(descriptor);
        }
        sddl
    }

    let opener: Opener = Box::new(|path: &Path| {
        let sddl = wide(SDDL);
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: the string lives for the call; freed below.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(ok, 0, "build the descriptor");
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let name = wide(&path.display().to_string());
        // SAFETY: the name and attributes live for the call.
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                FILE_APPEND_DATA,
                FILE_SHARE_READ,
                &attributes,
                OPEN_ALWAYS,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };
        // SAFETY: allocated above, freed once.
        unsafe { LocalFree(descriptor) };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        use std::os::windows::io::{FromRawHandle, RawHandle};
        // SAFETY: a handle this closure owns, handed to the File.
        let file = unsafe { std::fs::File::from_raw_handle(handle as RawHandle) };
        let size = file.metadata()?.len();
        Ok((Box::new(file) as Box<dyn Write + Send>, size))
    });

    let dir = tempfile::tempdir().expect("tempdir");
    let mut log = RotatingLog::open(dir.path(), opener).expect("open the log");
    writeln!(log, "{}", record(0, 10)).expect("write");
    let before = dacl_of(&dir.path().join(ACTIVE));
    assert!(before.starts_with("D:P"), "a protected DACL: {before}");
    let records = (MAX_FILE_BYTES / 1024) as usize + 10;
    for n in 1..=records {
        writeln!(log, "{}", record(n, 1000)).expect("write");
    }
    drop(log);
    assert_eq!(
        dacl_of(&dir.path().join(archive_name(1))),
        before,
        "the archive keeps the active file's protection"
    );
    assert_eq!(
        dacl_of(&dir.path().join(ACTIVE)),
        before,
        "the new active file has its final protection at creation"
    );
}

/// Debt #137 (SPEC-001 §Observability): the "agent starting" line carries
/// the version, the agent's id, platform and hostname, the server URL and
/// the heartbeat interval.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn install_ac_010_the_starting_line_carries_spec_001_fields() {
    let mock = common::MockServer::start().await;
    let config = common::write_temp_config(&format!(
        "[server]\n\
         url = \"{url}\"\n\
         \n\
         [agent]\n\
         id = \"01934abc-def0-7000-89ab-000000000137\"\n\
         hostname = \"FIN-PC-137\"\n\
         \n\
         [heartbeat]\n\
         interval_seconds = 1\n",
        url = mock.base_url
    ));
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_cg-agent"))
        .arg("--config")
        .arg(config.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("launch the agent binary");
    assert!(
        common::wait_until(std::time::Duration::from_secs(10), || mock.received_count()
            >= 1)
        .await,
        "a heartbeat"
    );
    child.start_kill().expect("end the agent");
    let output = child.wait_with_output().await.expect("agent output");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let starting = stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| v["fields"]["message"].as_str() == Some("agent starting"))
        .unwrap_or_else(|| panic!("no \"agent starting\" line: {stdout}"));
    let fields = &starting["fields"];
    assert_eq!(fields["agent.id"], "01934abc-def0-7000-89ab-000000000137");
    assert_eq!(fields["agent.hostname"], "FIN-PC-137");
    assert_eq!(fields["agent.platform"], cg_agent::detect_platform());
    assert_eq!(fields["server.url"], mock.base_url.as_str());
    assert_eq!(fields["heartbeat.interval_seconds"], 1);
    assert_eq!(fields["agent.version"], cg_agent::AGENT_VERSION);
}
