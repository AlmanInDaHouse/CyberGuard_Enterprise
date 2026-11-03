//! SPEC-024 install_ac_007, and the service half of install_ac_009: the
//! agent as a real Windows service, started by the service control manager
//! (SCM) and run as LocalSystem (SPEC-024 §Operational §3 and its second
//! Amendment 2026-11-01, items 1 to 4).
//!
//! These tests create, start, stop and delete Windows services and run the
//! agent as SYSTEM, so they build only with the test-only `scm-tests`
//! feature and run only in rust-ci's Windows job, on a GitHub-hosted runner
//! that is discarded after the job (SPEC-024 Amendment 2026-11-01, point 4).
//! Each one fails without acting outside such a runner or when a service
//! named `CyberGuardAgent` exists, creates its own uniquely named service,
//! touches no service it did not create, and deletes its service on every
//! exit path (`TestService`'s `Drop`). They run one at a time
//! (`--test-threads=1`): they share the host's instance lock and the
//! agent's fixed ETW session name.
//!
//! The SCM sends no `SHUTDOWN` without an OS shutdown, which this runner
//! does not give (Amendment 2026-11-01, point 4): these tests send `STOP`,
//! and assert that `RUNNING` accepts `SHUTDOWN`; no SCM is mocked.
#![cfg(windows)]

mod common;

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cg_agent::etw::{events_lost, SESSION_NAME};
use cg_agent::instance_lock::{self, LockOutcome, STDERR_ANOTHER_INSTANCE};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_SERVICE_DOES_NOT_EXIST, ERROR_SERVICE_SPECIFIC_ERROR,
};
use windows_sys::Win32::System::EventLog::{
    EvtClose, EvtNext, EvtQuery, EvtQueryChannelPath, EvtQueryReverseDirection, EvtRender,
    EvtRenderEventXml,
};
use windows_sys::Win32::System::Services::{
    CloseServiceHandle, ControlService, CreateServiceW, DeleteService, OpenSCManagerW,
    OpenServiceW, QueryServiceStatusEx, StartServiceW, SC_HANDLE, SC_MANAGER_CONNECT,
    SC_MANAGER_CREATE_SERVICE, SC_STATUS_PROCESS_INFO, SERVICE_ACCEPT_SHUTDOWN,
    SERVICE_ACCEPT_STOP, SERVICE_ALL_ACCESS, SERVICE_CONTROL_STOP, SERVICE_DEMAND_START,
    SERVICE_ERROR_NORMAL, SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_START_PENDING,
    SERVICE_STATUS, SERVICE_STATUS_PROCESS, SERVICE_STOPPED, SERVICE_STOP_PENDING,
    SERVICE_WIN32_OWN_PROCESS,
};
use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

/// The product's service name (SPEC-024 §Data contracts §3); these tests
/// never create, open for change, stop or delete it.
const PRODUCT_SERVICE: &str = "CyberGuardAgent";

/// The service stop deadline (SPEC-024 §Operational §3).
const STOP_DEADLINE: Duration = Duration::from_secs(60);

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn last_error() -> u32 {
    // SAFETY: reads the calling thread's last-error value.
    unsafe { GetLastError() }
}

// ---------------------------------------------------------------------------
// The SCM side: a uniquely named service this test owns.
// ---------------------------------------------------------------------------

/// An SCM handle, closed on drop.
struct Scm(SC_HANDLE);

impl Scm {
    fn open() -> Self {
        // SAFETY: null machine and database name the local active database.
        let handle = unsafe {
            OpenSCManagerW(
                std::ptr::null(),
                std::ptr::null(),
                SC_MANAGER_CONNECT | SC_MANAGER_CREATE_SERVICE,
            )
        };
        assert!(
            !handle.is_null(),
            "OpenSCManagerW: Win32 error {}",
            last_error()
        );
        Scm(handle)
    }
}

impl Drop for Scm {
    fn drop(&mut self) {
        // SAFETY: a handle OpenSCManagerW returned, closed once.
        unsafe { CloseServiceHandle(self.0) };
    }
}

/// Fail without acting unless this is a GitHub-hosted runner and no
/// service named `CyberGuardAgent` exists (SPEC-024 Amendment 2026-11-01,
/// point 4).
fn refuse_unless_disposable_runner(scm: &Scm) {
    let actions = std::env::var("GITHUB_ACTIONS").ok();
    let environment = std::env::var("RUNNER_ENVIRONMENT").ok();
    assert!(
        actions.as_deref() == Some("true") && environment.as_deref() == Some("github-hosted"),
        "refusing to create a Windows service outside a GitHub-hosted runner \
         (GITHUB_ACTIONS={actions:?}, RUNNER_ENVIRONMENT={environment:?})"
    );
    let name = wide(PRODUCT_SERVICE);
    // SAFETY: the name lives for the call; the handle, if any, is closed.
    let handle = unsafe { OpenServiceW(scm.0, name.as_ptr(), SERVICE_QUERY_STATUS) };
    if !handle.is_null() {
        // SAFETY: a handle OpenServiceW returned.
        unsafe { CloseServiceHandle(handle) };
        panic!("refusing to act: a service named {PRODUCT_SERVICE} exists on this host");
    }
    let error = last_error();
    assert_eq!(
        error, ERROR_SERVICE_DOES_NOT_EXIST,
        "cannot establish that {PRODUCT_SERVICE} is absent: Win32 error {error}"
    );
}

/// What polling a service saw on its way to a state.
struct Observed {
    status: SERVICE_STATUS_PROCESS,
    /// The longest time a pending state kept the same checkpoint.
    longest_checkpoint_gap: Duration,
    /// The largest wait hint a pending state reported, in milliseconds.
    largest_wait_hint: u32,
}

/// A service this test created, LocalSystem and own-process, running the
/// agent binary with `args`. `Drop` stops it if it runs (killing its
/// process only if it does not stop) and deletes it.
struct TestService {
    /// Closed after the service handle (fields drop after `drop`).
    _scm: Scm,
    handle: SC_HANDLE,
    name: String,
}

impl TestService {
    fn create(args: &str) -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let scm = Scm::open();
        refuse_unless_disposable_runner(&scm);
        let name = format!(
            "CgAgentTest-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        );
        let command = format!("\"{}\" {args}", env!("CARGO_BIN_EXE_cg-agent"));
        let (wname, wdisplay, wcommand) = (wide(&name), wide(&name), wide(&command));
        // SAFETY: every string lives for the call; a null account is
        // LocalSystem, a null password is none.
        let handle = unsafe {
            CreateServiceW(
                scm.0,
                wname.as_ptr(),
                wdisplay.as_ptr(),
                SERVICE_ALL_ACCESS,
                SERVICE_WIN32_OWN_PROCESS,
                SERVICE_DEMAND_START,
                SERVICE_ERROR_NORMAL,
                wcommand.as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        assert!(
            !handle.is_null(),
            "CreateServiceW {name}: Win32 error {}",
            last_error()
        );
        eprintln!("created service {name}: {command}");
        TestService {
            _scm: scm,
            handle,
            name,
        }
    }

    fn start(&self) -> Result<(), u32> {
        // SAFETY: no arguments.
        if unsafe { StartServiceW(self.handle, 0, std::ptr::null()) } == 0 {
            return Err(last_error());
        }
        Ok(())
    }

    fn status(&self) -> SERVICE_STATUS_PROCESS {
        // SAFETY: an all-zero SERVICE_STATUS_PROCESS is a valid value.
        let mut status: SERVICE_STATUS_PROCESS = unsafe { std::mem::zeroed() };
        let mut needed = 0u32;
        // SAFETY: the buffer is one SERVICE_STATUS_PROCESS.
        let ok = unsafe {
            QueryServiceStatusEx(
                self.handle,
                SC_STATUS_PROCESS_INFO,
                (&mut status as *mut SERVICE_STATUS_PROCESS).cast(),
                std::mem::size_of::<SERVICE_STATUS_PROCESS>() as u32,
                &mut needed,
            )
        };
        assert_ne!(
            ok,
            0,
            "QueryServiceStatusEx {}: Win32 error {}",
            self.name,
            last_error()
        );
        status
    }

    fn control_stop(&self) -> Result<(), u32> {
        // SAFETY: an all-zero SERVICE_STATUS is a valid out-buffer.
        let mut status: SERVICE_STATUS = unsafe { std::mem::zeroed() };
        // SAFETY: the out-buffer is one SERVICE_STATUS.
        if unsafe { ControlService(self.handle, SERVICE_CONTROL_STOP, &mut status) } == 0 {
            return Err(last_error());
        }
        Ok(())
    }

    /// Poll until the service reports `state`, failing after `timeout`.
    fn wait_for(&self, state: u32, timeout: Duration) -> Observed {
        let started = Instant::now();
        let mut last_seen: Option<(u32, u32)> = None;
        let mut last_change = started;
        let mut longest_checkpoint_gap = Duration::ZERO;
        let mut largest_wait_hint = 0u32;
        loop {
            let status = self.status();
            let now = Instant::now();
            let seen = (status.dwCurrentState, status.dwCheckPoint);
            if last_seen != Some(seen) {
                if let Some((previous_state, _)) = last_seen {
                    if previous_state == status.dwCurrentState || is_pending(previous_state) {
                        longest_checkpoint_gap =
                            longest_checkpoint_gap.max(now.duration_since(last_change));
                    }
                }
                last_seen = Some(seen);
                last_change = now;
            }
            if is_pending(status.dwCurrentState) {
                largest_wait_hint = largest_wait_hint.max(status.dwWaitHint);
            }
            if status.dwCurrentState == state {
                return Observed {
                    status,
                    longest_checkpoint_gap,
                    largest_wait_hint,
                };
            }
            assert!(
                now.duration_since(started) < timeout,
                "{} did not reach state {state} within {timeout:?}: state {}, exit {} / {}",
                self.name,
                status.dwCurrentState,
                status.dwWin32ExitCode,
                status.dwServiceSpecificExitCode
            );
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    /// Start the service and wait for it to stop on its own, as a start
    /// that fails before `RUNNING` does. A start the SCM refuses because
    /// the service already stopped is that outcome too.
    fn start_expecting_stop(&self, timeout: Duration) -> SERVICE_STATUS_PROCESS {
        if let Err(code) = self.start() {
            eprintln!("{}: StartServiceW returned Win32 error {code}", self.name);
        }
        self.wait_for(SERVICE_STOPPED, timeout).status
    }
}

impl Drop for TestService {
    fn drop(&mut self) {
        let status = self.status();
        if status.dwCurrentState != SERVICE_STOPPED {
            let _ = self.control_stop();
            let deadline = Instant::now() + Duration::from_secs(90);
            while self.status().dwCurrentState != SERVICE_STOPPED && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(250));
            }
            let status = self.status();
            if status.dwCurrentState != SERVICE_STOPPED && status.dwProcessId != 0 {
                // The process is this test's own service's; it did not stop.
                // SAFETY: a process handle opened for termination, closed once.
                unsafe {
                    let process = OpenProcess(PROCESS_TERMINATE, 0, status.dwProcessId);
                    if !process.is_null() {
                        TerminateProcess(process, 1);
                        CloseHandle(process);
                    }
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        }
        // SAFETY: the handle this test created; deleted and closed once.
        unsafe {
            if DeleteService(self.handle) == 0 {
                eprintln!(
                    "DeleteService {}: Win32 error {}",
                    self.name,
                    GetLastError()
                );
            }
            CloseServiceHandle(self.handle);
        }
    }
}

fn is_pending(state: u32) -> bool {
    state == SERVICE_START_PENDING || state == SERVICE_STOP_PENDING
}

/// SPEC-024 second Amendment 2026-11-01, item 3: in both pending states the
/// checkpoint advances at least every 10 seconds, with a wait hint of at
/// most 30 seconds.
fn assert_pending_progress(observed: &Observed, what: &str) {
    assert!(
        observed.longest_checkpoint_gap <= Duration::from_millis(10_500),
        "{what}: a pending state kept its checkpoint for {:?} (at most 10 s)",
        observed.longest_checkpoint_gap
    );
    assert!(
        observed.largest_wait_hint <= 30_000,
        "{what}: a pending state reported a wait hint of {} ms (at most 30 s)",
        observed.largest_wait_hint
    );
}

/// `STOPPED` after a failure: `ERROR_SERVICE_SPECIFIC_ERROR` and the
/// cause's code (item 3).
fn assert_stopped_with(status: &SERVICE_STATUS_PROCESS, code: u32) {
    assert_eq!(
        (status.dwWin32ExitCode, status.dwServiceSpecificExitCode),
        (ERROR_SERVICE_SPECIFIC_ERROR, code),
        "STOPPED carries ERROR_SERVICE_SPECIFIC_ERROR and the cause's code {code}"
    );
}

// ---------------------------------------------------------------------------
// The Application log: the entries of source CyberGuardAgent.
// ---------------------------------------------------------------------------

/// One Application-log entry, from its rendered XML.
#[derive(Debug)]
struct Entry {
    record_id: u64,
    event_id: u32,
    level: u32,
    /// The entry's strings, joined by newlines (XML-escaped as rendered).
    data: String,
}

/// The newest records of the Application log matching `xpath`, newest
/// first, at most `max`.
fn application_entries(xpath: &str, max: usize) -> Vec<Entry> {
    let (channel, query) = (wide("Application"), wide(xpath));
    // SAFETY: the strings live for the call.
    let results = unsafe {
        EvtQuery(
            0,
            channel.as_ptr(),
            query.as_ptr(),
            EvtQueryChannelPath | EvtQueryReverseDirection,
        )
    };
    assert_ne!(
        results,
        0,
        "query the Application log: Win32 error {}",
        last_error()
    );
    let mut entries = Vec::new();
    while entries.len() < max {
        let mut handles = [0isize; 16];
        let mut returned = 0u32;
        // SAFETY: room for 16 handles.
        if unsafe { EvtNext(results, 16, handles.as_mut_ptr(), 0, 0, &mut returned) } == 0 {
            break;
        }
        for &handle in &handles[..returned as usize] {
            if entries.len() < max {
                entries.push(render_entry(handle));
            }
            // SAFETY: a handle EvtNext returned, closed once.
            unsafe { EvtClose(handle) };
        }
        if returned == 0 {
            break;
        }
    }
    // SAFETY: the query handle, closed once.
    unsafe { EvtClose(results) };
    entries
}

fn render_entry(handle: isize) -> Entry {
    let mut buffer = vec![0u16; 8192];
    let (mut used, mut count) = (0u32, 0u32);
    // SAFETY: the buffer's size in bytes is passed; on
    // ERROR_INSUFFICIENT_BUFFER `used` holds the size needed.
    let mut ok = unsafe {
        EvtRender(
            0,
            handle,
            EvtRenderEventXml,
            (buffer.len() * 2) as u32,
            buffer.as_mut_ptr().cast(),
            &mut used,
            &mut count,
        )
    };
    if ok == 0 {
        buffer = vec![0u16; (used as usize).div_ceil(2) + 1];
        // SAFETY: as above, with the size Windows asked for.
        ok = unsafe {
            EvtRender(
                0,
                handle,
                EvtRenderEventXml,
                (buffer.len() * 2) as u32,
                buffer.as_mut_ptr().cast(),
                &mut used,
                &mut count,
            )
        };
    }
    assert_ne!(ok, 0, "render an event: Win32 error {}", last_error());
    let xml = String::from_utf16_lossy(&buffer[..(used as usize / 2).saturating_sub(1)]);
    Entry {
        record_id: element_number(&xml, "EventRecordID"),
        event_id: element_number(&xml, "EventID") as u32,
        level: element_number(&xml, "Level") as u32,
        data: elements(&xml, "Data").join("\n"),
    }
}

/// The text of every `<tag ...>text</tag>` element of `xml`.
fn elements(xml: &str, tag: &str) -> Vec<String> {
    let (open, close) = (format!("<{tag}"), format!("</{tag}>"));
    let mut found = Vec::new();
    let mut rest = xml;
    while let Some(at) = rest.find(&open) {
        let after = &rest[at + open.len()..];
        // The tag's own name, not a longer one (`<Data` vs `<DataItem`).
        if !after.starts_with('>') && !after.starts_with(' ') {
            rest = after;
            continue;
        }
        let Some(gt) = after.find('>') else { break };
        if after[..gt].ends_with('/') {
            rest = &after[gt + 1..];
            continue;
        }
        let body = &after[gt + 1..];
        let Some(end) = body.find(&close) else { break };
        found.push(body[..end].to_string());
        rest = &body[end + close.len()..];
    }
    found
}

fn element_number(xml: &str, tag: &str) -> u64 {
    elements(xml, tag)
        .first()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or_else(|| panic!("<{tag}> in {xml}"))
}

/// The newest record id of the Application log, before a test acts.
fn application_baseline() -> u64 {
    application_entries("*", 1)
        .first()
        .map_or(0, |entry| entry.record_id)
}

/// The entries of source `CyberGuardAgent` written after `baseline`,
/// oldest first.
fn agent_entries_after(baseline: u64) -> Vec<Entry> {
    let xpath =
        format!("*[System[Provider[@Name='{PRODUCT_SERVICE}'] and EventRecordID > {baseline}]]");
    let mut entries = application_entries(&xpath, 200);
    entries.reverse();
    entries
}

fn entry(entries: &[Entry], event_id: u32) -> &Entry {
    entries
        .iter()
        .find(|entry| entry.event_id == event_id)
        .unwrap_or_else(|| panic!("an entry {event_id} of source {PRODUCT_SERVICE}: {entries:#?}"))
}

/// Levels of the entries (SPEC-024 second Amendment, item 4).
const LEVEL_ERROR: u32 = 2;
const LEVEL_INFORMATION: u32 = 4;

// ---------------------------------------------------------------------------
// The agent's configuration and identity.
// ---------------------------------------------------------------------------

/// The knobs a test turns in the agent's `agent.toml`.
struct Knobs {
    request_timeout_seconds: u64,
    enrollment_timeout_seconds: u64,
    enrollment_max_retries: u32,
    enrollment_backoff_initial_ms: u64,
}

impl Default for Knobs {
    fn default() -> Self {
        Knobs {
            request_timeout_seconds: 5,
            enrollment_timeout_seconds: 30,
            enrollment_max_retries: 3,
            enrollment_backoff_initial_ms: 10,
        }
    }
}

/// Write a secure-path `agent.toml`, with its trust anchor and identity
/// paths, into `dir`, and return its absolute path.
fn write_config(
    dir: &Path,
    enroll_url: &str,
    heartbeat_url: &str,
    anchor_pem: &str,
    knobs: &Knobs,
) -> PathBuf {
    let trust_anchor = dir.join("server-ca.pem");
    std::fs::File::create(&trust_anchor)
        .and_then(|mut f| f.write_all(anchor_pem.as_bytes()))
        .expect("write trust anchor");
    let config_path = dir.join("agent.toml");
    let toml = format!(
        "[server]\n\
         url = \"{enroll_url}\"\n\
         heartbeat_url = \"{heartbeat_url}\"\n\
         trust_anchor_path = '{anchor}'\n\
         \n\
         [agent]\n\
         id = \"01934abc-def0-7000-89ab-000000000007\"\n\
         hostname = \"TEST-PC\"\n\
         \n\
         [heartbeat]\n\
         interval_seconds = 1\n\
         request_timeout_seconds = {request_timeout}\n\
         max_retries = 3\n\
         backoff_initial_ms = 50\n\
         \n\
         [enrollment]\n\
         token = \"install-ac-007\"\n\
         cert_path = '{cert}'\n\
         key_path = '{key}'\n\
         identity_path = '{identity}'\n\
         timeout_seconds = {enroll_timeout}\n\
         max_retries = {enroll_retries}\n\
         backoff_initial_ms = {enroll_backoff}\n",
        anchor = trust_anchor.display(),
        cert = dir.join("cert.pem").display(),
        key = dir.join("key.dat").display(),
        identity = dir.join("identity.json").display(),
        request_timeout = knobs.request_timeout_seconds,
        enroll_timeout = knobs.enrollment_timeout_seconds,
        enroll_retries = knobs.enrollment_max_retries,
        enroll_backoff = knobs.enrollment_backoff_initial_ms,
    );
    std::fs::write(&config_path, toml).expect("write agent.toml");
    config_path
}

/// Persist the test PKI's identity where `config_path` points, so the
/// service loads it instead of enrolling and the heartbeat mock verifies
/// its signatures under the PKI's key. Machine-scope DPAPI: SYSTEM unseals
/// what this process sealed.
fn provision_identity(config_path: &Path, pki: &common::TlsTestPki) {
    let config = cg_agent::config::load_from_path(config_path).expect("load agent.toml");
    let enrolled = cg_agent::enrollment::EnrolledIdentity {
        agent_id: pki.agent_id.clone(),
        client_certificate_pem: pki.client_cert_pem.clone(),
        issued_at: "2026-11-03T00:00:00Z".to_string(),
        expires_at: "2027-11-03T00:00:00Z".to_string(),
        secret_seed: zeroize::Zeroizing::new(pki.agent_seed),
        public_key: pki.agent_pubkey,
    };
    cg_agent::identity::persist_identity(&config, &enrolled).expect("persist the identity");
}

/// The service's arguments: `--service` and the quoted absolute config.
fn service_args(config_path: &Path) -> String {
    format!("--service --config \"{}\"", config_path.display())
}

fn online_count(mock: &common::TlsMockServer) -> usize {
    mock.received()
        .iter()
        .filter(|envelope| common::envelope_status(envelope) == "online")
        .count()
}

/// A TCP listener that accepts connections and never answers them: an
/// enrollment request stays in flight.
struct SilentListener {
    url: String,
    accepted: Arc<AtomicUsize>,
}

impl SilentListener {
    fn start() -> Self {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let port = listener.local_addr().expect("local_addr").port();
        let accepted = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&accepted);
        std::thread::spawn(move || {
            let mut held = Vec::new();
            for stream in listener.incoming().flatten() {
                counter.fetch_add(1, Ordering::SeqCst);
                held.push(stream);
            }
        });
        SilentListener {
            url: format!("https://127.0.0.1:{port}"),
            accepted,
        }
    }
}

// ---------------------------------------------------------------------------
// The tests.
// ---------------------------------------------------------------------------

/// install_ac_007: the service reports `RUNNING` accepting `STOP` and
/// `SHUTDOWN` alone; `STOP` during normal delivery is prompt, stops the
/// capture before one final `going_offline` POST, reports `STOPPED` with
/// code 0 and leaves no agent ETW session; a second start succeeds. Entries
/// 1000 (with the version) and 1001 are written.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_007_stop_during_delivery_is_clean_and_permits_restart() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    let heartbeat = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = write_config(
        dir.path(),
        &enroll.base_url,
        &heartbeat.base_url,
        &pki.trust_anchor_pem,
        &Knobs::default(),
    );
    provision_identity(&config, &pki);
    let baseline = application_baseline();
    let service = TestService::create(&service_args(&config));

    for round in 1..=2 {
        let online_before = online_count(&heartbeat);
        service
            .start()
            .unwrap_or_else(|code| panic!("start {round}: Win32 error {code}"));
        let running = service.wait_for(SERVICE_RUNNING, Duration::from_secs(60));
        assert_pending_progress(&running, "START_PENDING");
        assert_eq!(
            running.status.dwControlsAccepted,
            SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN,
            "RUNNING accepts STOP and SHUTDOWN and no other control"
        );
        assert!(
            common::wait_until(Duration::from_secs(60), || online_count(&heartbeat)
                > online_before)
            .await,
            "start {round}: the service delivers heartbeats"
        );
        assert!(
            events_lost(SESSION_NAME).is_ok(),
            "start {round}: the agent's ETW session runs while the service does"
        );

        let stop_sent = Instant::now();
        service.control_stop().expect("STOP");
        let stopped = service.wait_for(SERVICE_STOPPED, STOP_DEADLINE + Duration::from_secs(15));
        let took = stop_sent.elapsed();
        assert!(
            took < Duration::from_secs(20),
            "start {round}: STOP during normal delivery is prompt, took {took:?}"
        );
        assert_pending_progress(&stopped, "STOP_PENDING");
        assert_eq!(
            (
                stopped.status.dwWin32ExitCode,
                stopped.status.dwServiceSpecificExitCode
            ),
            (0, 0),
            "start {round}: a clean stop reports STOPPED with code 0"
        );
        let received = heartbeat.received();
        assert_eq!(
            common::envelope_status(received.last().expect("a final POST")),
            "going_offline",
            "start {round}: the last POST is the final going_offline"
        );
        assert_eq!(
            events_lost(SESSION_NAME),
            Err(4201),
            "start {round}: a normal stop leaves no agent ETW session"
        );
    }
    assert_eq!(
        enroll.enroll_received_count(),
        0,
        "the persisted identity is loaded, never re-enrolled"
    );

    let entries = agent_entries_after(baseline);
    let running = entry(&entries, 1000);
    assert_eq!(running.level, LEVEL_INFORMATION, "1000 is information");
    assert!(
        running.data.contains(cg_agent::AGENT_VERSION),
        "1000 carries the version {}: {running:?}",
        cg_agent::AGENT_VERSION
    );
    assert_eq!(entry(&entries, 1001).level, LEVEL_INFORMATION, "1001");
    assert_eq!(
        entries.iter().filter(|e| e.event_id == 1001).count(),
        2,
        "one 1001 per clean stop: {entries:#?}"
    );
    assert!(
        !entries
            .iter()
            .any(|e| e.event_id == 1002 || e.event_id == 1003),
        "no failure entry: {entries:#?}"
    );
}

/// install_ac_007: `RUNNING` precedes enrollment, and `STOP` while
/// enrollment waits between its retries ends the service promptly as a
/// clean stop, with no heartbeat.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_007_stop_during_an_enrollment_retry_wait_is_prompt() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    enroll.set_enroll_default(503);
    let heartbeat = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let knobs = Knobs {
        enrollment_max_retries: 5,
        enrollment_backoff_initial_ms: 120_000,
        ..Knobs::default()
    };
    let config = write_config(
        dir.path(),
        &enroll.base_url,
        &heartbeat.base_url,
        &pki.trust_anchor_pem,
        &knobs,
    );
    let baseline = application_baseline();
    let service = TestService::create(&service_args(&config));
    service.start().expect("start");
    service.wait_for(SERVICE_RUNNING, Duration::from_secs(60));
    assert!(
        common::wait_until(Duration::from_secs(60), || enroll.enroll_received_count()
            >= 1)
        .await,
        "the service enrolls after RUNNING"
    );
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        enroll.enroll_received_count(),
        1,
        "the service waits out its 120 s backoff"
    );

    let stop_sent = Instant::now();
    service.control_stop().expect("STOP");
    let stopped = service.wait_for(SERVICE_STOPPED, Duration::from_secs(30));
    let took = stop_sent.elapsed();
    assert!(
        took < Duration::from_secs(10),
        "STOP during a retry wait is prompt, took {took:?}"
    );
    assert_eq!(
        (
            stopped.status.dwWin32ExitCode,
            stopped.status.dwServiceSpecificExitCode
        ),
        (0, 0),
        "a stop during enrollment is a clean stop"
    );
    assert_eq!(
        heartbeat.received_count(),
        0,
        "no heartbeat without identity"
    );
    let entries = agent_entries_after(baseline);
    entry(&entries, 1001);
}

/// install_ac_007 and SPEC-002 Amendment 2026-10-27: an exhausted
/// enrollment round does not end the service (the console's code 4) nor
/// loop tightly — the next round waits at least 60 s — and `STOP` during
/// that wait is prompt.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_007_stop_between_enrollment_rounds_is_prompt() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    enroll.set_enroll_default(503);
    let heartbeat = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let knobs = Knobs {
        enrollment_max_retries: 1,
        ..Knobs::default()
    };
    let config = write_config(
        dir.path(),
        &enroll.base_url,
        &heartbeat.base_url,
        &pki.trust_anchor_pem,
        &knobs,
    );
    let service = TestService::create(&service_args(&config));
    service.start().expect("start");
    service.wait_for(SERVICE_RUNNING, Duration::from_secs(60));
    assert!(
        common::wait_until(Duration::from_secs(60), || enroll.enroll_received_count()
            >= 1)
        .await,
        "the first enrollment round"
    );
    tokio::time::sleep(Duration::from_secs(15)).await;
    assert_eq!(
        service.status().dwCurrentState,
        SERVICE_RUNNING,
        "an exhausted round does not end the service"
    );
    assert_eq!(
        enroll.enroll_received_count(),
        1,
        "no second round within 15 s of the first"
    );

    let stop_sent = Instant::now();
    service.control_stop().expect("STOP");
    let stopped = service.wait_for(SERVICE_STOPPED, Duration::from_secs(30));
    let took = stop_sent.elapsed();
    assert!(
        took < Duration::from_secs(10),
        "STOP between enrollment rounds is prompt, took {took:?}"
    );
    assert_eq!(
        (
            stopped.status.dwWin32ExitCode,
            stopped.status.dwServiceSpecificExitCode
        ),
        (0, 0),
        "a clean stop"
    );
}

/// install_ac_007: `STOP` while an enrollment request is in flight (the
/// server never answers) ends the service promptly as a clean stop.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_007_stop_with_an_enrollment_request_in_flight_is_prompt() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let silent = SilentListener::start();
    let heartbeat = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let knobs = Knobs {
        enrollment_timeout_seconds: 600,
        enrollment_max_retries: 1,
        ..Knobs::default()
    };
    let config = write_config(
        dir.path(),
        &silent.url,
        &heartbeat.base_url,
        &pki.trust_anchor_pem,
        &knobs,
    );
    let service = TestService::create(&service_args(&config));
    service.start().expect("start");
    service.wait_for(SERVICE_RUNNING, Duration::from_secs(60));
    assert!(
        common::wait_until(Duration::from_secs(60), || silent
            .accepted
            .load(Ordering::SeqCst)
            >= 1)
        .await,
        "the enrollment request is in flight"
    );
    tokio::time::sleep(Duration::from_secs(2)).await;

    let stop_sent = Instant::now();
    service.control_stop().expect("STOP");
    let stopped = service.wait_for(SERVICE_STOPPED, Duration::from_secs(30));
    let took = stop_sent.elapsed();
    assert!(
        took < Duration::from_secs(10),
        "STOP with an enrollment request in flight is prompt, took {took:?}"
    );
    assert_eq!(
        (
            stopped.status.dwWin32ExitCode,
            stopped.status.dwServiceSpecificExitCode
        ),
        (0, 0),
        "a clean stop"
    );
}

/// install_ac_007: a final request that never completes, with a request
/// timeout longer than the stop's budget, is bounded by the 60-second
/// deadline: the capture is already stopped, `STOP_PENDING` keeps its
/// checkpoint moving, and `STOPPED` reports code 12 with entry 1003
/// counting the events not delivered — never a clean stop.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_007_a_stalled_final_request_expires_the_deadline_with_code_12() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    let responder: common::Responder = Arc::new(|envelope| {
        (common::envelope_status(envelope) == "going_offline").then_some(common::STALL)
    });
    let heartbeat = common::TlsMockServer::start_with_responder(&pki, responder).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let knobs = Knobs {
        request_timeout_seconds: 300,
        ..Knobs::default()
    };
    let config = write_config(
        dir.path(),
        &enroll.base_url,
        &heartbeat.base_url,
        &pki.trust_anchor_pem,
        &knobs,
    );
    provision_identity(&config, &pki);
    let baseline = application_baseline();
    let service = TestService::create(&service_args(&config));
    service.start().expect("start");
    service.wait_for(SERVICE_RUNNING, Duration::from_secs(60));
    assert!(
        common::wait_until(Duration::from_secs(60), || online_count(&heartbeat) >= 1).await,
        "the service delivers heartbeats"
    );

    let stop_sent = Instant::now();
    service.control_stop().expect("STOP");
    let stopped = service.wait_for(SERVICE_STOPPED, STOP_DEADLINE + Duration::from_secs(30));
    let took = stop_sent.elapsed();
    assert!(
        took >= Duration::from_secs(55) && took <= STOP_DEADLINE + Duration::from_secs(15),
        "the stop ends at its 60 s deadline, took {took:?}"
    );
    assert_pending_progress(&stopped, "STOP_PENDING");
    assert_stopped_with(&stopped.status, 12);
    assert!(
        heartbeat
            .attempts()
            .iter()
            .any(|a| a.status == common::STALL
                && common::envelope_status(&a.envelope) == "going_offline"),
        "the final going_offline request was sent and stalled"
    );
    assert_eq!(
        events_lost(SESSION_NAME),
        Err(4201),
        "the capture stopped before the final request"
    );
    let entries = agent_entries_after(baseline);
    let expired = entry(&entries, 1003);
    assert_eq!(expired.level, LEVEL_ERROR, "1003 is an error");
    assert!(
        expired.data.contains("not delivered"),
        "1003 counts the events not delivered: {expired:?}"
    );
    assert!(
        !entries.iter().any(|e| e.event_id == 1001),
        "an expired deadline is never a clean stop: {entries:#?}"
    );
}

/// Item 2: in service mode a `--config` that is not absolute is a
/// configuration error, code 2, before any file is read.
#[test]
fn install_ac_007_a_relative_config_in_service_mode_is_code_2() {
    let baseline = application_baseline();
    let service = TestService::create("--service --config agent.toml");
    let stopped = service.start_expecting_stop(Duration::from_secs(60));
    assert_stopped_with(&stopped, 2);
    let entries = agent_entries_after(baseline);
    let failed = entry(&entries, 1002);
    assert_eq!(failed.level, LEVEL_ERROR, "1002 is an error");
    assert!(
        failed
            .data
            .contains("--config must be an absolute path in service mode"),
        "1002 carries the cause's line: {failed:?}"
    );
}

/// Items 3 and 4: an invalid configuration stops the service with code 2;
/// entry 1002 names the file and the error's line, and quotes no line of
/// the file (a configuration line can carry a token).
#[test]
fn install_ac_007_an_invalid_config_names_the_file_and_quotes_no_line() {
    const SECRET: &str = "s54-token-value-never-logged";
    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join("agent.toml");
    std::fs::write(
        &config_path,
        format!(
            "[server]\n\
             url = \"https://127.0.0.1:9\"\n\
             [enrollment]\n\
             token = \"{SECRET}\n"
        ),
    )
    .expect("write agent.toml");
    let baseline = application_baseline();
    let service = TestService::create(&service_args(&config_path));
    let stopped = service.start_expecting_stop(Duration::from_secs(60));
    assert_stopped_with(&stopped, 2);
    let entries = agent_entries_after(baseline);
    let failed = entry(&entries, 1002);
    assert!(
        failed.data.contains(&config_path.display().to_string()),
        "1002 names the file: {failed:?}"
    );
    assert!(
        failed.data.contains("line 4"),
        "1002 gives the parser's line: {failed:?}"
    );
    assert!(
        !entries.iter().any(|e| e.data.contains(SECRET)),
        "no entry quotes a line of the configuration: {entries:#?}"
    );
}

/// install_ac_009, the service half (item 1): with another live process
/// holding the instance lock, the service stops before enrolling with code
/// 10 and entry 1002 carrying the cause's line.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_ac_009_a_held_lock_stops_the_service_with_code_10() {
    let pki = common::generate_test_pki(common::TEST_AGENT_ID);
    let enroll = common::TlsEnrollMock::start(&pki, common::TlsMockMode::Normal).await;
    let heartbeat = common::TlsMockServer::start(&pki, common::TlsMockMode::Normal).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = write_config(
        dir.path(),
        &enroll.base_url,
        &heartbeat.base_url,
        &pki.trust_anchor_pem,
        &Knobs::default(),
    );
    let baseline = application_baseline();
    let service = TestService::create(&service_args(&config));
    let held = match instance_lock::acquire() {
        LockOutcome::Acquired(lock) => lock,
        other => panic!("the runner's Administrator takes the lock: {other:?}"),
    };
    let stopped = service.start_expecting_stop(Duration::from_secs(60));
    drop(held);
    assert_stopped_with(&stopped, 10);
    assert_eq!(
        enroll.enroll_received_count(),
        0,
        "it stops before enrolling"
    );
    let entries = agent_entries_after(baseline);
    let failed = entry(&entries, 1002);
    assert!(
        failed.data.contains(STDERR_ANOTHER_INSTANCE),
        "1002 carries the cause's line: {failed:?}"
    );
}
