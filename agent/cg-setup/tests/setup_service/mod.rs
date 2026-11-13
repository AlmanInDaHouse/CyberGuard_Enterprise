//! The setup's client of the service control manager against real services
//! (SPEC-024 Amendment 2026-11-10, items 3, 6 and 8): the conflict check
//! (26) and `service_running`, the stop within 75 seconds (31) and what its
//! first reading found (the Amendment 2026-11-11, point 1), the failure
//! actions with their flag set to false, the start with its arguments (41
//! when refused), the reversal's start (40 when refused, point 2), and
//! configure's observer (42 to 44, item 8's outcome).
//!
//! These tests create, start, stop and delete Windows services, so they build
//! only with the test-only `scm-tests` feature and run only in rust-ci's
//! Windows job, on a GitHub-hosted runner that is discarded after the job
//! (the Amendment 2026-11-01, point 4, which the Amendment 2026-11-10's
//! *Tests* apply to `cg-setup`). Each one fails without acting outside such a
//! runner or when a service named `CyberGuardAgent` exists, creates its own
//! uniquely named services (`CgSetupTest-<pid>-<n>`), touches no service it
//! did not create, and deletes each one on every exit path (`TestService`'s
//! `Drop`). The client takes the service's name, so the product service is
//! never named.
//!
//! The services run this test executable (`service`), whose behaviour file
//! says when it reports each state and whether it accepts a stop; its command
//! line has the product's form. The tests read the SCM with their own
//! bindings, not the client's.

#[path = "../fixtures/mod.rs"]
mod fixtures;
mod service;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use cg_agent::service::migration::{Choice, Request as MigrationRequest};
use cg_setup::codes;
use cg_setup::observer::{self, LogPosition, HEARTBEAT_LINE, IDENTITY_LINE, OBSERVE_WITHIN};
use cg_setup::preflight::{Request, Staged};
use cg_setup::scm::{self, Manager, Product, Route, Stop};
use cg_setup::service::{migration_arguments, product_command_line, start_arguments};
use cg_setup::staging::{read_record, Name, Setup, SetupCustody, Step, Watch};
use cg_setup::verbs::{self, Machine};
use cg_setup::version::{numeric, SETUP_VERSION};
use cg_setup::wizard::{Answer, Fields, Offer, Starter, Wizard};
use cg_setup::Refusal;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_SERVICE_DOES_NOT_EXIST,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR};
use windows_sys::Win32::System::Services::{
    ChangeServiceConfig2W, ChangeServiceConfigW, CloseServiceHandle, ControlService,
    CreateServiceW, DeleteService, OpenSCManagerW, OpenServiceW, QueryServiceConfig2W,
    QueryServiceStatusEx, SetServiceObjectSecurity, StartServiceW, SC_ACTION, SC_ACTION_NONE,
    SC_ACTION_RESTART, SC_ACTION_RUN_COMMAND, SC_HANDLE, SC_MANAGER_CONNECT,
    SC_MANAGER_CREATE_SERVICE, SC_STATUS_PROCESS_INFO, SERVICE_ALL_ACCESS,
    SERVICE_CONFIG_FAILURE_ACTIONS, SERVICE_CONFIG_FAILURE_ACTIONS_FLAG, SERVICE_CONTROL_STOP,
    SERVICE_DEMAND_START, SERVICE_DISABLED, SERVICE_ERROR_NORMAL, SERVICE_FAILURE_ACTIONSW,
    SERVICE_FAILURE_ACTIONS_FLAG, SERVICE_NO_CHANGE, SERVICE_QUERY_STATUS, SERVICE_RUNNING,
    SERVICE_START_PENDING, SERVICE_STATUS, SERVICE_STATUS_PROCESS, SERVICE_STOPPED,
    SERVICE_STOP_PENDING, SERVICE_WIN32_OWN_PROCESS, SERVICE_WIN32_SHARE_PROCESS,
};
use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

/// The product's service name (SPEC-024 §Data contracts §3); these tests
/// never create, open for change, stop or delete it.
const PRODUCT_SERVICE: &str = "CyberGuardAgent";
/// `SERVICE_INTERACTIVE_PROCESS` (winsvc.h), in a windows-sys feature these
/// tests do not enable.
const INTERACTIVE_PROCESS: u32 = 0x100;
/// What `preset_failure_actions` sets beside its actions.
const PRESET_COMMAND: &str = r"C:\Windows\System32\cmd.exe /c exit 0";
const PRESET_REBOOT_MESSAGE: &str = "preset by the test";

// --- The SCM side: services this test owns ------------------------------------------

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn last_error() -> u32 {
    // SAFETY: reads the calling thread's last-error value.
    unsafe { GetLastError() }
}

/// Fail without acting unless this is a GitHub-hosted runner (the Amendment
/// 2026-11-01, point 4). Each test calls it before anything else it does.
fn refuse_outside_a_disposable_runner() {
    let actions = std::env::var("GITHUB_ACTIONS").ok();
    let environment = std::env::var("RUNNER_ENVIRONMENT").ok();
    assert!(
        actions.as_deref() == Some("true") && environment.as_deref() == Some("github-hosted"),
        "refusing to create a Windows service outside a GitHub-hosted runner \
         (GITHUB_ACTIONS={actions:?}, RUNNER_ENVIRONMENT={environment:?})"
    );
}

/// Fail without acting when a service named `CyberGuardAgent` exists.
fn refuse_beside_the_product_service(scm: SC_HANDLE) {
    let name = wide(PRODUCT_SERVICE);
    // SAFETY: the name lives for the call; the handle, if any, is closed.
    let handle = unsafe { OpenServiceW(scm, name.as_ptr(), SERVICE_QUERY_STATUS) };
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

fn open_scm() -> SC_HANDLE {
    refuse_outside_a_disposable_runner();
    // SAFETY: null machine and database name the local active database.
    let scm = unsafe {
        OpenSCManagerW(
            std::ptr::null(),
            std::ptr::null(),
            SC_MANAGER_CONNECT | SC_MANAGER_CREATE_SERVICE,
        )
    };
    assert!(
        !scm.is_null(),
        "OpenSCManagerW: Win32 error {}",
        last_error()
    );
    refuse_beside_the_product_service(scm);
    scm
}

/// A name no service has, on this runner: what a test that creates no
/// service passes the client.
fn unused_name() -> String {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let scm = open_scm();
    let name = format!(
        "CgSetupTest-{}-absent-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    );
    let wname = wide(&name);
    // SAFETY: the name lives for the call; each handle is closed once.
    let error = unsafe {
        let handle = OpenServiceW(scm, wname.as_ptr(), SERVICE_QUERY_STATUS);
        let error = if handle.is_null() { last_error() } else { 0 };
        if !handle.is_null() {
            CloseServiceHandle(handle);
        }
        CloseServiceHandle(scm);
        error
    };
    assert_eq!(
        error, ERROR_SERVICE_DOES_NOT_EXIST,
        "refusing to act: {name} is not established absent"
    );
    name
}

/// What a service is created with.
struct Spec {
    service_type: u32,
    start_type: u32,
    /// `None` is LocalSystem.
    account: Option<&'static str>,
    command_line: String,
}

/// A controllable service's files: its binary, its behaviour file and the
/// record of its start arguments, in a directory of this test's.
struct Controllable {
    _directory: tempfile::TempDir,
    binary: PathBuf,
    behaviour: PathBuf,
    record: PathBuf,
}

impl Controllable {
    fn new(behaviour: serde_json::Value) -> Self {
        // Both refusals before anything is written.
        let scm = open_scm();
        // SAFETY: a handle OpenSCManagerW returned, closed once.
        unsafe { CloseServiceHandle(scm) };
        let directory = tempfile::tempdir().expect("a temporary directory");
        let record = directory.path().join("arguments.jsonl");
        let mut behaviour = behaviour;
        behaviour["record"] = serde_json::Value::String(record.display().to_string());
        let path = directory.path().join("behaviour.json");
        std::fs::write(&path, behaviour.to_string()).expect("write the behaviour file");
        Controllable {
            binary: std::env::current_exe().expect("this executable"),
            behaviour: path,
            record,
            _directory: directory,
        }
    }

    fn spec(&self) -> Spec {
        Spec {
            service_type: SERVICE_WIN32_OWN_PROCESS,
            start_type: SERVICE_DEMAND_START,
            account: None,
            command_line: product_command_line(&self.binary, &self.behaviour),
        }
    }

    /// The start arguments each start received, in order.
    fn starts(&self) -> Vec<Vec<String>> {
        match std::fs::read_to_string(&self.record) {
            Ok(text) => text
                .lines()
                .map(|line| serde_json::from_str(line).expect("a JSON array per start"))
                .collect(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => panic!("read {}: {e}", self.record.display()),
        }
    }
}

/// A service this test created. `Drop` stops it if it runs, kills its
/// process if it does not stop, and deletes it.
struct TestService {
    scm: SC_HANDLE,
    handle: SC_HANDLE,
    name: String,
}

impl TestService {
    fn create(spec: &Spec) -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let scm = open_scm();
        let name = format!(
            "CgSetupTest-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        );
        let (wname, wcommand) = (wide(&name), wide(&spec.command_line));
        let account = spec.account.map(wide);
        // SAFETY: every string lives for the call; a null account is
        // LocalSystem, a null password is none.
        let handle = unsafe {
            CreateServiceW(
                scm,
                wname.as_ptr(),
                wname.as_ptr(),
                SERVICE_ALL_ACCESS,
                spec.service_type,
                spec.start_type,
                SERVICE_ERROR_NORMAL,
                wcommand.as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                account.as_ref().map_or(std::ptr::null(), |a| a.as_ptr()),
                std::ptr::null(),
            )
        };
        assert!(
            !handle.is_null(),
            "CreateServiceW {name}: Win32 error {}",
            last_error()
        );
        eprintln!("created service {name}: {}", spec.command_line);
        TestService { scm, handle, name }
    }

    fn controllable(files: &Controllable) -> Self {
        Self::create(&files.spec())
    }

    fn query(&self) -> Result<SERVICE_STATUS_PROCESS, u32> {
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
        if ok == 0 {
            return Err(last_error());
        }
        Ok(status)
    }

    fn state(&self) -> u32 {
        self.query()
            .unwrap_or_else(|e| panic!("QueryServiceStatusEx {}: Win32 error {e}", self.name))
            .dwCurrentState
    }

    /// Start it with the test's own binding, with no argument.
    fn start(&self) {
        // SAFETY: no arguments.
        let ok = unsafe { StartServiceW(self.handle, 0, std::ptr::null()) };
        assert!(
            ok != 0,
            "StartServiceW {}: Win32 error {}",
            self.name,
            last_error()
        );
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

    /// Poll until it reports `state`, failing after `timeout`.
    fn wait_for(&self, state: u32, timeout: Duration) {
        let started = Instant::now();
        loop {
            let current = self.state();
            if current == state {
                return;
            }
            assert!(
                started.elapsed() < timeout,
                "{} did not reach state {state} within {timeout:?}: state {current}",
                self.name
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn set_dacl(&self, sddl: &str) {
        let text = wide(sddl);
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: the text lives for the call; the descriptor is freed below.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        };
        assert!(ok != 0, "convert {sddl}: Win32 error {}", last_error());
        // SAFETY: a descriptor the conversion allocated, freed once.
        let ok = unsafe {
            let ok = SetServiceObjectSecurity(self.handle, DACL_SECURITY_INFORMATION, descriptor);
            LocalFree(descriptor);
            ok
        };
        assert!(
            ok != 0,
            "SetServiceObjectSecurity {}: Win32 error {}",
            self.name,
            last_error()
        );
    }

    /// Its failure actions: the reset period, each action's type and delay,
    /// and the command and reboot message, empty when none.
    fn failure_actions(&self) -> (u32, Vec<(i32, u32)>, String, String) {
        let mut needed = 0u32;
        // SAFETY: a zero-sized query asks for the size alone.
        unsafe {
            QueryServiceConfig2W(
                self.handle,
                SERVICE_CONFIG_FAILURE_ACTIONS,
                std::ptr::null_mut(),
                0,
                &mut needed,
            )
        };
        let mut buffer = vec![0u64; (needed as usize).div_ceil(8).max(1)];
        // SAFETY: the buffer holds `buffer.len() * 8` bytes, aligned for the
        // structure at its head.
        let ok = unsafe {
            QueryServiceConfig2W(
                self.handle,
                SERVICE_CONFIG_FAILURE_ACTIONS,
                buffer.as_mut_ptr().cast(),
                (buffer.len() * 8) as u32,
                &mut needed,
            )
        };
        assert!(
            ok != 0,
            "QueryServiceConfig2W {}: Win32 error {}",
            self.name,
            last_error()
        );
        // SAFETY: the SCM wrote the structure and its actions in the buffer.
        unsafe {
            let actions = &*buffer.as_ptr().cast::<SERVICE_FAILURE_ACTIONSW>();
            let list = if actions.cActions == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(actions.lpsaActions, actions.cActions as usize)
                    .iter()
                    .map(|action| (action.Type, action.Delay))
                    .collect()
            };
            let text = |pointer: *mut u16| {
                if pointer.is_null() {
                    return String::new();
                }
                let len = (0..).take_while(|&i| *pointer.add(i) != 0).count();
                String::from_utf16_lossy(std::slice::from_raw_parts(pointer, len))
            };
            (
                actions.dwResetPeriod,
                list,
                text(actions.lpCommand),
                text(actions.lpRebootMsg),
            )
        }
    }

    fn failure_actions_flag(&self) -> bool {
        let mut flag = SERVICE_FAILURE_ACTIONS_FLAG {
            fFailureActionsOnNonCrashFailures: 7,
        };
        let mut needed = 0u32;
        // SAFETY: the buffer is one SERVICE_FAILURE_ACTIONS_FLAG.
        let ok = unsafe {
            QueryServiceConfig2W(
                self.handle,
                SERVICE_CONFIG_FAILURE_ACTIONS_FLAG,
                (&mut flag as *mut SERVICE_FAILURE_ACTIONS_FLAG).cast(),
                std::mem::size_of::<SERVICE_FAILURE_ACTIONS_FLAG>() as u32,
                &mut needed,
            )
        };
        assert!(
            ok != 0,
            "QueryServiceConfig2W {}: Win32 error {}",
            self.name,
            last_error()
        );
        flag.fFailureActionsOnNonCrashFailures != 0
    }

    /// Set failure actions and the flag with the test's own binding.
    fn preset_failure_actions(&self, actions: &[(i32, u32)], reset: u32, flag: bool) {
        let mut list: Vec<SC_ACTION> = actions
            .iter()
            .map(|&(kind, delay)| SC_ACTION {
                Type: kind,
                Delay: delay,
            })
            .collect();
        let mut command = wide(PRESET_COMMAND);
        let mut reboot_message = wide(PRESET_REBOOT_MESSAGE);
        let failure_actions = SERVICE_FAILURE_ACTIONSW {
            dwResetPeriod: reset,
            lpRebootMsg: reboot_message.as_mut_ptr(),
            lpCommand: command.as_mut_ptr(),
            cActions: list.len() as u32,
            lpsaActions: list.as_mut_ptr(),
        };
        // SAFETY: the structure, its command and its actions live for the call.
        let ok = unsafe {
            ChangeServiceConfig2W(
                self.handle,
                SERVICE_CONFIG_FAILURE_ACTIONS,
                (&failure_actions as *const SERVICE_FAILURE_ACTIONSW).cast(),
            )
        };
        assert!(
            ok != 0,
            "preset the failure actions: Win32 error {}",
            last_error()
        );
        let flag = SERVICE_FAILURE_ACTIONS_FLAG {
            fFailureActionsOnNonCrashFailures: i32::from(flag),
        };
        // SAFETY: the structure lives for the call.
        let ok = unsafe {
            ChangeServiceConfig2W(
                self.handle,
                SERVICE_CONFIG_FAILURE_ACTIONS_FLAG,
                (&flag as *const SERVICE_FAILURE_ACTIONS_FLAG).cast(),
            )
        };
        assert!(ok != 0, "preset the flag: Win32 error {}", last_error());
    }
}

impl Drop for TestService {
    fn drop(&mut self) {
        // No assertion here: a panic while unwinding aborts the process and
        // skips the other guards' cleanup.
        let stopped = |status: &Result<SERVICE_STATUS_PROCESS, u32>| {
            status
                .as_ref()
                .is_ok_and(|status| status.dwCurrentState == SERVICE_STOPPED)
        };
        if !stopped(&self.query()) {
            let deadline = Instant::now() + Duration::from_secs(20);
            while !stopped(&self.query()) && Instant::now() < deadline {
                let _ = self.control_stop();
                std::thread::sleep(Duration::from_millis(250));
            }
            let running = self
                .query()
                .ok()
                .filter(|s| s.dwCurrentState != SERVICE_STOPPED && s.dwProcessId != 0);
            if let Some(status) = running {
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
        // SAFETY: the handles this test opened; each deleted or closed once.
        unsafe {
            if DeleteService(self.handle) == 0 {
                eprintln!(
                    "DeleteService {}: Win32 error {}",
                    self.name,
                    GetLastError()
                );
            }
            CloseServiceHandle(self.handle);
            CloseServiceHandle(self.scm);
        }
    }
}

// --- Helpers -----------------------------------------------------------------------

fn manager() -> Manager {
    Manager::open().expect("the client opens the SCM")
}

fn check(service: &TestService, files: &Controllable, route: Route) -> Result<Product, Refusal> {
    scm::check_product_service(
        &manager(),
        &service.name,
        &files.binary,
        &files.behaviour,
        route,
    )
}

fn assert_refused(result: Result<impl std::fmt::Debug, Refusal>, code: u8, fragments: &[&str]) {
    let refusal = result.expect_err("a refusal");
    assert_eq!(refusal.code, code, "{refusal:?}");
    assert!(refusal.line.starts_with("cg-setup: "), "{refusal:?}");
    for fragment in fragments {
        assert!(
            refusal.line.contains(fragment),
            "{fragment:?} in {refusal:?}"
        );
    }
}

/// What `scm::stop` reports for a service proved `STOPPED`.
fn stopped(found_running: bool) -> Stop {
    Stop {
        found_running,
        result: Ok(()),
    }
}

fn fingerprint(digit: char) -> String {
    format!("sha256:{}", digit.to_string().repeat(64))
}

/// A command line of the product's form over these two paths.
fn command_line(binary: &Path, config: &Path) -> String {
    product_command_line(binary, config)
}

// --- Item 3: the conflict check and service_running ----------------------------

/// A service whose type, account and command line are the product's is no
/// conflict, the paths compared but for ASCII case and the start type not
/// compared; stopped, it is not running, and the migration route takes it.
fn package_scm_a_service_of_the_products_configuration_is_no_conflict() {
    let files = Controllable::new(serde_json::json!({}));
    let service = TestService::controllable(&files);
    for route in [Route::Other, Route::Migration] {
        assert_eq!(
            check(&service, &files, route).expect("no conflict"),
            Product::Present { running: false },
            "{route:?}"
        );
    }
    let upper = |path: &Path| PathBuf::from(path.display().to_string().to_ascii_uppercase());
    assert_eq!(
        scm::check_product_service(
            &manager(),
            &service.name,
            &upper(&files.binary),
            &upper(&files.behaviour),
            Route::Other,
        ),
        Ok(Product::Present { running: false }),
        "the paths equal but for ASCII case"
    );

    let disabled = TestService::create(&Spec {
        start_type: SERVICE_DISABLED,
        ..files.spec()
    });
    assert_eq!(
        check(&disabled, &files, Route::Other),
        Ok(Product::Present { running: false }),
        "the start type is not compared"
    );
}

/// No service of that name: no conflict, and nothing to stop.
fn package_scm_no_product_service_is_no_conflict_and_needs_no_stop() {
    let name = unused_name();
    let files = Controllable::new(serde_json::json!({}));
    for route in [Route::Other, Route::Migration] {
        let found =
            scm::check_product_service(&manager(), &name, &files.binary, &files.behaviour, route);
        assert_eq!(found, Ok(Product::Absent), "{route:?}");
        assert!(!Product::Absent.running());
    }
    assert_eq!(
        scm::stop(&manager(), &name),
        stopped(false),
        "a service that does not exist needs no stop, and was not found running"
    );
}

/// Each way a service is not the product's is 26, on every route.
fn package_scm_each_difference_from_the_products_configuration_is_26() {
    let files = Controllable::new(serde_json::json!({}));
    let product = files.spec();
    let binary = files.binary.display().to_string();
    let config = files.behaviour.display().to_string();
    let other = files.behaviour.with_extension("toml");
    let cases: Vec<(&str, Spec)> = vec![
        (
            "type",
            Spec {
                service_type: SERVICE_WIN32_SHARE_PROCESS,
                ..files.spec()
            },
        ),
        (
            "type",
            Spec {
                service_type: SERVICE_WIN32_OWN_PROCESS | INTERACTIVE_PROCESS,
                ..files.spec()
            },
        ),
        (
            "account",
            Spec {
                account: Some(r"NT AUTHORITY\LocalService"),
                ..files.spec()
            },
        ),
        (
            "command line",
            Spec {
                command_line: format!("{binary} --service --config \"{config}\""),
                ..files.spec()
            },
        ),
        (
            "command line",
            Spec {
                command_line: command_line(&files.binary, &other),
                ..files.spec()
            },
        ),
        (
            "command line",
            Spec {
                command_line: command_line(&files.behaviour, &files.behaviour),
                ..files.spec()
            },
        ),
        (
            "command line",
            Spec {
                command_line: format!("{} --adopt-staged-config", product.command_line),
                ..files.spec()
            },
        ),
        (
            "command line",
            Spec {
                command_line: format!("\"{binary}\" --config \"{config}\""),
                ..files.spec()
            },
        ),
    ];
    for (what, spec) in &cases {
        let service = TestService::create(spec);
        for route in [Route::Other, Route::Migration] {
            assert_refused(
                check(&service, &files, route),
                codes::CONFLICTING_SERVICE,
                &[&service.name, "is not the product's", what],
            );
        }
    }
}

/// A service whose configuration the setup cannot read is 26; one whose
/// configuration is the product's but whose state it cannot read is 40.
fn package_scm_an_unreadable_configuration_is_26_and_an_unreadable_state_40() {
    let files = Controllable::new(serde_json::json!({}));
    let service = TestService::controllable(&files);
    // SYSTEM alone may open it; this test's handle was opened before.
    service.set_dacl("D:P(A;;GA;;;SY)");
    for route in [Route::Other, Route::Migration] {
        assert_refused(
            check(&service, &files, route),
            codes::CONFLICTING_SERVICE,
            &[&service.name, "cannot be read", "Win32 error 5"],
        );
    }

    let service = TestService::controllable(&files);
    // Administrators may query its configuration (CC) but not its state (LC).
    service.set_dacl("D:P(A;;GA;;;SY)(A;;CC;;;BA)");
    for route in [Route::Other, Route::Migration] {
        assert_refused(
            check(&service, &files, route),
            codes::OWN_STATE,
            &[&service.name, "state", "cannot be read", "Win32 error 5"],
        );
    }
}

/// `service_running` is true in every state but `STOPPED`, `STOP_PENDING`
/// included, and the migration route refuses each of those states (26).
fn package_scm_service_running_in_every_state_but_stopped() {
    let files = Controllable::new(serde_json::json!({
        "start_pending_ms": 8000,
        "stop_ms": 8000,
    }));
    let service = TestService::controllable(&files);
    let assert_running = |state: u32| {
        let found = check(&service, &files, Route::Other);
        assert_eq!(
            found,
            Ok(Product::Present { running: true }),
            "state {state}"
        );
        assert!(found.unwrap().running(), "service_running in state {state}");
        assert_refused(
            check(&service, &files, Route::Migration),
            codes::CONFLICTING_SERVICE,
            &[&service.name, "not STOPPED"],
        );
        assert_eq!(
            service.state(),
            state,
            "the state did not move during the checks"
        );
    };
    service.start();
    service.wait_for(SERVICE_START_PENDING, Duration::from_secs(5));
    assert_running(SERVICE_START_PENDING);
    service.wait_for(SERVICE_RUNNING, Duration::from_secs(20));
    assert_running(SERVICE_RUNNING);
    service.control_stop().expect("STOP");
    service.wait_for(SERVICE_STOP_PENDING, Duration::from_secs(5));
    assert_running(SERVICE_STOP_PENDING);
    service.wait_for(SERVICE_STOPPED, Duration::from_secs(20));
    let found = check(&service, &files, Route::Other);
    assert_eq!(found, Ok(Product::Present { running: false }), "STOPPED");
    assert!(!found.unwrap().running(), "no service_running when STOPPED");
}

// --- Item 6: the stop --------------------------------------------------------------

/// The Amendment 2026-11-11, point 1: the stop's first reading. A service
/// `STOP_PENDING` is found running, as one `START_PENDING` is; one that
/// has stopped since is not.
fn package_scm_the_stop_reports_its_first_reading() {
    let files = Controllable::new(serde_json::json!({ "stop_ms": 8000 }));
    let service = TestService::controllable(&files);
    service.start();
    service.wait_for(SERVICE_RUNNING, Duration::from_secs(20));
    service.control_stop().expect("STOP");
    service.wait_for(SERVICE_STOP_PENDING, Duration::from_secs(5));
    assert_eq!(
        scm::stop(&manager(), &service.name),
        stopped(true),
        "STOP_PENDING is found running"
    );
    assert_eq!(service.state(), SERVICE_STOPPED);
    assert_eq!(
        scm::stop(&manager(), &service.name),
        stopped(false),
        "STOPPED since"
    );

    let files = Controllable::new(serde_json::json!({ "start_pending_ms": 8000 }));
    let service = TestService::controllable(&files);
    service.start();
    service.wait_for(SERVICE_START_PENDING, Duration::from_secs(5));
    assert_eq!(
        scm::stop(&manager(), &service.name),
        stopped(true),
        "START_PENDING is found running"
    );
}

/// A running service that accepts the stop is proved `STOPPED`; one already
/// `STOPPED` needs none.
fn package_scm_stop_proves_a_running_service_stopped() {
    let files = Controllable::new(serde_json::json!({ "stop_ms": 3000 }));
    let service = TestService::controllable(&files);
    assert_eq!(
        scm::stop(&manager(), &service.name),
        stopped(false),
        "already STOPPED: no stop, and not found running"
    );
    service.start();
    service.wait_for(SERVICE_RUNNING, Duration::from_secs(20));
    let started = Instant::now();
    assert_eq!(scm::stop(&manager(), &service.name), stopped(true));
    let took = started.elapsed();
    assert_eq!(
        service.state(),
        SERVICE_STOPPED,
        "proved STOPPED when it returns"
    );
    assert!(
        took >= Duration::from_millis(2500),
        "it waited through STOP_PENDING, took {took:?}"
    );
}

/// The stop is sent again while the service cannot accept it: during
/// `START_PENDING`, and while it runs accepting no control.
fn package_scm_stop_sends_the_control_again_while_the_service_cannot_accept_it() {
    for behaviour in [
        serde_json::json!({ "start_pending_ms": 10000 }),
        serde_json::json!({ "refuse_stop_ms": 10000 }),
    ] {
        let files = Controllable::new(behaviour.clone());
        let service = TestService::controllable(&files);
        service.start();
        let target = if behaviour.get("start_pending_ms").is_some() {
            SERVICE_START_PENDING
        } else {
            SERVICE_RUNNING
        };
        service.wait_for(target, Duration::from_secs(10));
        assert!(
            service.control_stop().is_err(),
            "{behaviour}: the service cannot accept the stop yet"
        );
        let started = Instant::now();
        assert_eq!(
            scm::stop(&manager(), &service.name),
            stopped(true),
            "{behaviour}"
        );
        let took = started.elapsed();
        assert_eq!(service.state(), SERVICE_STOPPED, "{behaviour}");
        assert!(
            took >= Duration::from_secs(5),
            "{behaviour}: it waited for the service to accept the stop, took {took:?}"
        );
    }
}

/// A stop longer than the service's own 60-second deadline but within 75
/// seconds is proved; a service still `STOP_PENDING` 75 seconds after the
/// first request is 31, the 75 seconds counted from that first request even
/// when the service refused it.
fn package_scm_the_stop_waits_75_seconds_and_no_more() {
    let files = Controllable::new(serde_json::json!({ "stop_ms": 65000 }));
    let service = TestService::controllable(&files);
    service.start();
    service.wait_for(SERVICE_RUNNING, Duration::from_secs(20));
    let started = Instant::now();
    assert_eq!(
        scm::stop(&manager(), &service.name),
        stopped(true),
        "a 65-second stop"
    );
    let took = started.elapsed();
    assert_eq!(service.state(), SERVICE_STOPPED);
    assert!(
        took >= Duration::from_secs(60) && took < Duration::from_secs(75),
        "a 65-second stop took {took:?}"
    );

    let files = Controllable::new(serde_json::json!({
        "refuse_stop_ms": 10000,
        "never_stop": true,
    }));
    let service = TestService::controllable(&files);
    service.start();
    service.wait_for(SERVICE_RUNNING, Duration::from_secs(20));
    let started = Instant::now();
    let stop = scm::stop(&manager(), &service.name);
    let took = started.elapsed();
    assert!(stop.found_running, "31, and found running");
    assert_refused(
        stop.result,
        codes::NOT_STOPPED,
        &[
            &service.name,
            "not proved stopped",
            "75 seconds after the first stop request",
        ],
    );
    assert!(
        took >= Duration::from_secs(75) && took < Duration::from_secs(78),
        "31 at 75 seconds from the first request, refused at first, took {took:?}"
    );
    assert_eq!(service.state(), SERVICE_STOP_PENDING);
}

/// The 75 seconds hold while a stop control blocks: the service's handler
/// takes 80 seconds to answer each stop, longer than the SCM waits for a
/// handler (30 seconds) and than the bound, so a control is outstanding at
/// the end; the service never stops. 31 comes at 75 seconds, not when that
/// control returns.
fn package_scm_the_stop_bound_holds_while_a_control_blocks() {
    let files = Controllable::new(serde_json::json!({
        "block_stop_ms": 80000,
        "never_stop": true,
    }));
    let service = TestService::controllable(&files);
    service.start();
    service.wait_for(SERVICE_RUNNING, Duration::from_secs(20));
    let started = Instant::now();
    let stop = scm::stop(&manager(), &service.name);
    let took = started.elapsed();
    assert!(stop.found_running, "31, and found running");
    assert_refused(
        stop.result,
        codes::NOT_STOPPED,
        &[&service.name, "75 seconds after the first stop request"],
    );
    assert!(
        took >= Duration::from_secs(75) && took < Duration::from_secs(78),
        "31 at 75 seconds while a control blocks, took {took:?}"
    );
}

// --- Item 8: the failure actions and the start -------------------------------------

/// The failure actions are a restart after 60, 120 and 300 seconds, the count
/// reset after a day, and the flag false: what was set before is replaced, a
/// command and reboot message deleted, and a flag set earlier cleared. A service that does not exist is 40.
fn package_scm_the_failure_actions_are_set_and_their_flag_cleared() {
    let files = Controllable::new(serde_json::json!({}));
    let service = TestService::controllable(&files);
    service.preset_failure_actions(
        &[
            (SC_ACTION_RUN_COMMAND, 1000),
            (SC_ACTION_NONE, 0),
            (SC_ACTION_RESTART, 5000),
            (SC_ACTION_RESTART, 7000),
        ],
        60,
        true,
    );
    assert!(service.failure_actions_flag(), "the preset flag is true");
    let preset = service.failure_actions();
    assert_eq!(
        (preset.2.as_str(), preset.3.as_str()),
        (PRESET_COMMAND, PRESET_REBOOT_MESSAGE),
        "the preset command and reboot message"
    );

    assert_eq!(scm::set_failure_actions(&manager(), &service.name), Ok(()));
    assert_eq!(
        service.failure_actions(),
        (
            86_400,
            vec![
                (SC_ACTION_RESTART, 60_000),
                (SC_ACTION_RESTART, 120_000),
                (SC_ACTION_RESTART, 300_000),
            ],
            String::new(),
            String::new(),
        ),
        "the actions replaced, the command and reboot message deleted"
    );
    assert!(!service.failure_actions_flag(), "the flag is false");

    assert_refused(
        scm::set_failure_actions(&manager(), &unused_name()),
        codes::OWN_STATE,
        &["failure actions"],
    );
}

/// The start passes its arguments after the service's name, exactly, or
/// none.
fn package_scm_the_start_passes_its_arguments_or_none() {
    let files = Controllable::new(serde_json::json!({}));
    let service = TestService::controllable(&files);
    let fingerprints: BTreeSet<String> = [fingerprint('a'), fingerprint('b')].into();
    let migration = MigrationRequest {
        choice: Choice::Keep,
        fingerprints: [fingerprint('c')].into(),
    };
    let starts = [
        start_arguments(&fingerprints),
        Vec::new(),
        migration_arguments(&migration),
    ];
    for arguments in &starts {
        assert_eq!(scm::start(&manager(), &service.name, arguments), Ok(()));
        service.wait_for(SERVICE_RUNNING, Duration::from_secs(20));
        service.control_stop().expect("STOP");
        service.wait_for(SERVICE_STOPPED, Duration::from_secs(20));
    }
    assert_eq!(
        files.starts(),
        starts.to_vec(),
        "each start's arguments, in order"
    );
}

/// A start the SCM refuses is 41: a disabled service, one already running,
/// and one that does not exist.
fn package_scm_a_start_the_scm_refuses_is_41() {
    let files = Controllable::new(serde_json::json!({}));
    let disabled = TestService::create(&Spec {
        start_type: SERVICE_DISABLED,
        ..files.spec()
    });
    assert_refused(
        scm::start(&manager(), &disabled.name, &[]),
        codes::START_REFUSED,
        &[&disabled.name, "Win32 error 1058"],
    );

    let running = TestService::controllable(&files);
    running.start();
    running.wait_for(SERVICE_RUNNING, Duration::from_secs(20));
    assert_refused(
        scm::start(&manager(), &running.name, &[]),
        codes::START_REFUSED,
        &[&running.name, "Win32 error 1056"],
    );

    let absent = unused_name();
    assert_refused(
        scm::start(&manager(), &absent, &[]),
        codes::START_REFUSED,
        &[&absent, "Win32 error 1060"],
    );
    assert_eq!(files.starts().len(), 1, "only the one start ran");
}

// --- Item 7: the reversal's start ---------------------------------------------------

/// The Amendment 2026-11-11, point 2: the reversal starts a `STOPPED`
/// service with no argument; a service in another state needs no start, and
/// gets none.
fn package_scm_the_reversals_start_starts_a_stopped_service_with_no_argument() {
    let files = Controllable::new(serde_json::json!({ "stop_ms": 8000 }));
    let service = TestService::controllable(&files);
    assert_eq!(scm::start_if_stopped(&manager(), &service.name), Ok(true));
    service.wait_for(SERVICE_RUNNING, Duration::from_secs(20));
    assert_eq!(
        files.starts(),
        vec![Vec::<String>::new()],
        "one start, with no argument"
    );

    assert_eq!(
        scm::start_if_stopped(&manager(), &service.name),
        Ok(false),
        "RUNNING needs no start"
    );
    service.control_stop().expect("STOP");
    service.wait_for(SERVICE_STOP_PENDING, Duration::from_secs(5));
    assert_eq!(
        scm::start_if_stopped(&manager(), &service.name),
        Ok(false),
        "STOP_PENDING needs no start"
    );
    service.wait_for(SERVICE_STOPPED, Duration::from_secs(20));
    assert_eq!(files.starts().len(), 1, "no start but the first");
}

/// The Amendment 2026-11-11, point 2: a start the SCM refuses is 40, not 41:
/// a disabled service, and one that does not exist.
fn package_scm_a_reversals_start_the_scm_refuses_is_40() {
    let files = Controllable::new(serde_json::json!({}));
    let disabled = TestService::create(&Spec {
        start_type: SERVICE_DISABLED,
        ..files.spec()
    });
    assert_refused(
        scm::start_if_stopped(&manager(), &disabled.name),
        codes::OWN_STATE,
        &[&disabled.name, "reversal", "Win32 error 1058"],
    );
    assert!(files.starts().is_empty());

    let absent = unused_name();
    assert_refused(
        scm::start_if_stopped(&manager(), &absent),
        codes::OWN_STATE,
        &[&absent, "reversal", "Win32 error 1060"],
    );
}

// --- The verbs (item 9) -------------------------------------------------------------

/// A temporary host for the verbs, with a product service of the product's
/// configuration: its command line names the host's installed binary, a copy
/// of this test executable with this package's version, and the host's
/// `agent.toml`, which holds the service's behaviour. The host is pending
/// (`agent.toml` and no identity); the service's log is the host's.
struct VerbsHost {
    // Dropped first: the service is stopped and deleted before its files.
    service: TestService,
    root: fixtures::Root,
    binary: PathBuf,
    arguments: PathBuf,
}

fn verbs_host(behaviour: serde_json::Value, start_type: u32) -> VerbsHost {
    let root = fixtures::Root::empty();
    let binary = root.operator.join("cg-agent.exe");
    let [x, y, z] = numeric(SETUP_VERSION).expect("X.Y.Z");
    fixtures::versioned_copy(&binary, [x, y, z, 0], SETUP_VERSION);
    let arguments = root.operator.join("arguments.jsonl");
    // The agent creates its log directory; the test service writes into one.
    std::fs::create_dir(root.paths.data_root.join("logs")).expect("the log directory");
    let mut behaviour = behaviour;
    behaviour["record"] = serde_json::Value::String(arguments.display().to_string());
    behaviour["log_dir"] =
        serde_json::Value::String(root.paths.data_root.join("logs").display().to_string());
    std::fs::write(&root.paths.config, behaviour.to_string()).expect("write the behaviour");
    let service = TestService::create(&Spec {
        service_type: SERVICE_WIN32_OWN_PROCESS,
        start_type,
        account: None,
        command_line: product_command_line(&binary, &root.paths.config),
    });
    VerbsHost {
        service,
        root,
        binary,
        arguments,
    }
}

impl VerbsHost {
    fn machine<'a>(&self, watch: &'a (dyn Watch + 'a)) -> Machine<'a> {
        Machine {
            host: self.root.host.clone(),
            setup: Setup {
                paths: self.root.paths.clone(),
                custody: SetupCustody {
                    directories: fixtures::test_policy(false),
                    staged: fixtures::test_policy(true),
                    directory_sddl: fixtures::closed_directory_sddl(),
                    file_sddl: fixtures::private_file_sddl(),
                },
                wait: fixtures::WAIT,
                watch,
            },
            service: self.service.name.clone(),
            binary: self.binary.clone(),
            wizard: None,
        }
    }

    /// The start arguments each start received, in order.
    fn starts(&self) -> Vec<Vec<String>> {
        match std::fs::read_to_string(&self.arguments) {
            Ok(text) => text
                .lines()
                .map(|line| serde_json::from_str(line).expect("a JSON array per start"))
                .collect(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => panic!("read {}: {e}", self.arguments.display()),
        }
    }

    fn record(&self) -> PathBuf {
        self.root.paths.data_root.join("setup-attempt.json")
    }

    /// A bootstrap for this host, and its fingerprints.
    fn bootstrap(&self) -> (Request, BTreeSet<String>) {
        let certificate = fixtures::certificate("verbs.example.test");
        let anchor = self.root.anchor(&[&certificate]);
        let file = self.root.bootstrap(&fixtures::bootstrap_text(
            anchor.to_str().expect("a Unicode path"),
            &[&certificate.fingerprint],
        ));
        (
            Request {
                bootstrap: Some(file),
                ..Request::default()
            },
            BTreeSet::from([certificate.fingerprint]),
        )
    }
}

fn unwatched(_: Step) -> Result<(), String> {
    Ok(())
}

/// The two lines, in order, after `RUNNING`.
fn operational() -> serde_json::Value {
    serde_json::json!({
        "log": [{ "line": IDENTITY_LINE }, { "line": HEARTBEAT_LINE }],
    })
}

/// #153 end to end (the Amendment 2026-11-11, point 1): the product service
/// is `STOPPED` at item 3's check and started before the record is written;
/// preflight's stop finds it running, proves it `STOPPED` and rewrites the
/// record, and the reversal starts it again with no argument.
fn package_scm_verbs_preflight_records_a_service_started_after_its_check() {
    let host = verbs_host(serde_json::json!({ "stop_ms": 2000 }), SERVICE_DEMAND_START);
    let name = host.service.name.clone();
    let start_it = move |step: Step| {
        if step == Step::Write(Name::Record) {
            scm::start(&manager(), &name, &[]).expect("a start between the check and the stop");
        }
        Ok(())
    };
    let preflight = host.machine(&start_it);
    assert_eq!(verbs::preflight(&preflight, &Request::default()), Ok(()));
    assert_eq!(
        host.service.state(),
        SERVICE_STOPPED,
        "preflight stopped it"
    );
    let record = read_record(&preflight.setup)
        .expect("a record that passes its checks")
        .expect("a record");
    assert!(
        record.service_running,
        "the stop's reading rewrote the record"
    );
    assert_eq!(host.starts(), vec![Vec::<String>::new()]);

    assert_eq!(verbs::reverse(&host.machine(&unwatched)), Ok(()));
    host.service
        .wait_for(SERVICE_RUNNING, Duration::from_secs(20));
    assert_eq!(
        host.starts(),
        vec![Vec::<String>::new(), Vec::new()],
        "the reversal's start, with no argument"
    );
    assert!(!host.record().exists(), "the reversal removed the record");
}

/// A running product service: preflight records it running and stops it; a
/// reversal starts it; one whose record found it stopped starts nothing.
fn package_scm_verbs_preflight_stops_a_running_service_and_the_reversal_starts_it() {
    let host = verbs_host(serde_json::json!({ "stop_ms": 2000 }), SERVICE_DEMAND_START);
    host.service.start();
    host.service
        .wait_for(SERVICE_RUNNING, Duration::from_secs(20));
    let (request, _) = host.bootstrap();
    assert_eq!(
        verbs::preflight(&host.machine(&unwatched), &request),
        Ok(())
    );
    assert_eq!(host.service.state(), SERVICE_STOPPED);
    assert_eq!(verbs::reverse(&host.machine(&unwatched)), Ok(()));
    host.service
        .wait_for(SERVICE_RUNNING, Duration::from_secs(20));
    assert_eq!(host.starts().len(), 2);
    assert!(!host.root.paths.staged_config().exists());

    host.service.control_stop().expect("STOP");
    host.service
        .wait_for(SERVICE_STOPPED, Duration::from_secs(20));
    // The route that starts: the record alone, which finds it stopped.
    assert_eq!(
        verbs::preflight(&host.machine(&unwatched), &Request::default()),
        Ok(())
    );
    assert_eq!(verbs::reverse(&host.machine(&unwatched)), Ok(()));
    assert_eq!(
        host.starts().len(),
        2,
        "a service found stopped stays stopped"
    );
    assert_eq!(host.service.state(), SERVICE_STOPPED);
}

/// Configure on the route that staged: the failure actions set, the start
/// with the record's fingerprints as the adoption's arguments, the record
/// deleted, and the outcome observed: operational once the two lines appear.
fn package_scm_verbs_configure_starts_with_the_records_fingerprints_and_observes() {
    let host = verbs_host(operational(), SERVICE_DEMAND_START);
    let (request, fingerprints) = host.bootstrap();
    assert_eq!(
        verbs::preflight(&host.machine(&unwatched), &request),
        Ok(())
    );
    let started = Instant::now();
    assert_eq!(
        verbs::configure(&host.machine(&unwatched), &Request::default()),
        Ok(())
    );
    assert!(
        started.elapsed() < OBSERVE_WITHIN,
        "{:?}",
        started.elapsed()
    );
    assert_eq!(host.starts(), vec![start_arguments(&fingerprints)]);
    assert!(!host.record().exists(), "configure deleted the record");
    let (reset, actions, _, _) = host.service.failure_actions();
    assert_eq!(
        (reset, actions),
        (
            86_400,
            vec![
                (SC_ACTION_RESTART, 60_000),
                (SC_ACTION_RESTART, 120_000),
                (SC_ACTION_RESTART, 300_000),
            ]
        )
    );
    assert!(!host.service.failure_actions_flag());
}

/// The Amendment 2026-11-11 (second), point 4: a deletion of the record that
/// fails after the start keeps the outcome's code, its one line naming the
/// record left, with 0 too; and a service that stops is 42 whether or not the
/// record was deleted.
fn package_scm_verbs_configure_a_record_left_keeps_the_outcomes_code() {
    let fail_deletion = |step: Step| {
        if step == Step::Remove(Name::Record) {
            Err("injected failure of the record's deletion".to_string())
        } else {
            Ok(())
        }
    };
    for (behaviour, code) in [
        (operational(), codes::OPERATIONAL),
        (
            serde_json::json!({ "stop_with": 13 }),
            codes::SERVICE_STOPPED,
        ),
    ] {
        for deletion_fails in [true, false] {
            let host = verbs_host(behaviour.clone(), SERVICE_DEMAND_START);
            let (request, _) = host.bootstrap();
            assert_eq!(
                verbs::preflight(&host.machine(&unwatched), &request),
                Ok(())
            );
            let configure = if deletion_fails {
                host.machine(&fail_deletion)
            } else {
                host.machine(&unwatched)
            };
            let outcome = verbs::configure(&configure, &Request::default());
            let case = format!("{behaviour}, the deletion fails: {deletion_fails}: {outcome:?}");
            assert_eq!(host.record().exists(), deletion_fails, "{case}");
            match outcome {
                Ok(()) => assert!(code == codes::OPERATIONAL && !deletion_fails, "{case}"),
                Err(refusal) => {
                    assert_eq!(refusal.code, code, "{case}");
                    assert_eq!(refusal.line.matches("cg-setup: ").count(), 1, "{case}");
                    assert_eq!(
                        refusal.line.contains("injected failure"),
                        deletion_fails,
                        "{case}"
                    );
                    if code == codes::SERVICE_STOPPED {
                        assert!(refusal.line.contains("stopped after its start"), "{case}");
                    }
                }
            }
        }
    }
}

/// A start the SCM refuses is 41, after the failure actions; the record
/// stays for the next preflight.
fn package_scm_verbs_configure_a_refused_start_is_41_and_leaves_the_record() {
    let host = verbs_host(operational(), SERVICE_DISABLED);
    let (request, _) = host.bootstrap();
    assert_eq!(
        verbs::preflight(&host.machine(&unwatched), &request),
        Ok(())
    );
    let refusal = verbs::configure(&host.machine(&unwatched), &Request::default()).expect_err("41");
    assert_eq!(refusal.code, codes::START_REFUSED, "{}", refusal.line);
    assert!(
        refusal.line.contains("Win32 error 1058"),
        "{}",
        refusal.line
    );
    assert!(host.record().exists(), "the record stays");
    assert!(host.starts().is_empty());
    assert!(!host.service.failure_actions_flag());
    assert_eq!(
        host.service.failure_actions().0,
        86_400,
        "the failure actions came first"
    );
}

/// A watch that records each step, and is `Sync`.
fn recording(
    steps: &std::sync::Mutex<Vec<Step>>,
) -> impl Fn(Step) -> Result<(), String> + Sync + '_ {
    move |step| {
        steps.lock().unwrap().push(step);
        Ok(())
    }
}

/// A service running at item 3's check is recorded running from the start:
/// no rewrite follows the stop.
fn package_scm_verbs_preflight_records_a_service_running_at_its_check() {
    let host = verbs_host(serde_json::json!({ "stop_ms": 1000 }), SERVICE_DEMAND_START);
    host.service.start();
    host.service
        .wait_for(SERVICE_RUNNING, Duration::from_secs(20));
    let steps = std::sync::Mutex::new(Vec::new());
    let watch = recording(&steps);
    assert_eq!(
        verbs::preflight(&host.machine(&watch), &Request::default()),
        Ok(())
    );
    drop(watch);
    let steps = steps.into_inner().unwrap();
    assert!(steps.contains(&Step::Write(Name::Record)), "{steps:?}");
    assert!(!steps.contains(&Step::RewriteRecord), "{steps:?}");
    let record = read_record(&host.machine(&unwatched).setup)
        .unwrap()
        .expect("a record");
    assert!(record.service_running);
}

/// A product service never proved `STOPPED` is preflight's 31, after which
/// the record is removed.
fn package_scm_verbs_preflight_a_service_not_proved_stopped_is_31() {
    let host = verbs_host(
        serde_json::json!({ "refuse_stop_ms": 100_000, "never_stop": true }),
        SERVICE_DEMAND_START,
    );
    host.service.start();
    host.service
        .wait_for(SERVICE_RUNNING, Duration::from_secs(20));
    let refusal = verbs::preflight(&host.machine(&unwatched), &Request::default()).expect_err("31");
    assert_eq!(refusal.code, codes::NOT_STOPPED, "{}", refusal.line);
    assert!(!host.record().exists(), "the refusal removed the record");
}

/// A product service of another configuration is 26 before the staging.
fn package_scm_verbs_preflight_a_conflicting_service_is_26_before_the_staging() {
    let host = verbs_host(serde_json::json!({}), SERVICE_DEMAND_START);
    let (request, _) = host.bootstrap();
    let mut other = host.machine(&unwatched);
    other.binary = host.root.operator.join("another").join("cg-agent.exe");
    let refusal = verbs::preflight(&other, &request).expect_err("26");
    assert_eq!(refusal.code, codes::CONFLICTING_SERVICE, "{}", refusal.line);
    assert!(!host.record().exists());
    assert!(!host.root.paths.staged_config().exists());
}

/// On the migration route configure reads no record and starts the service
/// with the migration's arguments, from its own variables.
fn package_scm_verbs_configure_on_the_migration_route_starts_with_its_arguments() {
    let host = verbs_host(operational(), SERVICE_DEMAND_START);
    let request = Request::new(
        None,
        Some("keep"),
        Some(&format!("sha256:{}", "ab".repeat(32))),
    );
    assert_eq!(
        verbs::configure(&host.machine(&unwatched), &request),
        Ok(())
    );
    let migration = cg_setup::preflight::parse_migration(
        request.migrate_legacy.as_deref(),
        request.trust_anchors.as_deref(),
    )
    .unwrap();
    assert_eq!(host.starts(), vec![migration_arguments(&migration)]);
}

/// A log position that cannot be recorded is 40: configure starts nothing
/// and leaves the record (the Amendment 2026-11-11, point 4). The service,
/// which would run if started, is still `STOPPED` seconds later, with no
/// start received.
fn package_scm_verbs_configure_an_unrecordable_log_position_is_40() {
    let host = verbs_host(serde_json::json!({}), SERVICE_DEMAND_START);
    let (request, _) = host.bootstrap();
    assert_eq!(
        verbs::preflight(&host.machine(&unwatched), &request),
        Ok(())
    );
    let logs = host.root.paths.data_root.join("logs");
    std::fs::remove_dir(&logs).expect("the log directory");
    std::fs::write(&logs, b"not a directory").expect("a file at logs");
    let refusal = verbs::configure(&host.machine(&unwatched), &Request::default()).expect_err("40");
    assert_eq!(refusal.code, codes::OWN_STATE, "{}", refusal.line);
    assert!(refusal.line.contains("log position"), "{}", refusal.line);
    std::thread::sleep(Duration::from_secs(5));
    assert_eq!(host.service.state(), SERVICE_STOPPED, "never started");
    assert!(host.starts().is_empty(), "nothing started");
    assert!(host.record().exists(), "the record stays");
}

// --- The observer (item 8's outcome) -------------------------------------------------

/// A log directory of the test's, holding an earlier run's log with both
/// lines in its active file and in an archive: lines before the position.
fn earlier_log() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let both: String = [IDENTITY_LINE, HEARTBEAT_LINE]
        .iter()
        .map(|message| {
            format!(
                "{}\n",
                serde_json::json!({ "fields": { "message": message } })
            )
        })
        .collect();
    for name in ["agent.1.jsonl", "agent.jsonl"] {
        std::fs::write(dir.path().join(name), &both).expect("an earlier run's log");
    }
    dir
}

/// Start a controllable service with `behaviour`, whose log is in `log_dir`,
/// after recording the log's position, and observe it: the outcome and how
/// long it took from the start request, which the observer is given as
/// `before` ahead of the start itself.
fn observed(
    behaviour: serde_json::Value,
    log_dir: &Path,
    before: Duration,
) -> (Result<(), Refusal>, Duration) {
    let mut behaviour = behaviour;
    behaviour["log_dir"] = serde_json::Value::String(log_dir.display().to_string());
    let files = Controllable::new(behaviour);
    let service = TestService::controllable(&files);
    let position = LogPosition::record(log_dir).expect("record the log's position");
    let started = Instant::now();
    std::thread::sleep(before);
    scm::start(&manager(), &service.name, &[]).expect("the start");
    let outcome = observer::observe(&manager(), &service.name, log_dir, position, started);
    let elapsed = started.elapsed();
    eprintln!("observed {}: {outcome:?} after {elapsed:?}", service.name);
    (outcome, elapsed)
}

fn package_scm_observe_a_service_whose_two_lines_appear_in_order_is_operational() {
    let log = earlier_log();
    let (outcome, elapsed) = observed(
        serde_json::json!({
            "start_pending_ms": 1000,
            "log": [
                { "sleep_ms": 500 },
                { "line": "agent starting" },
                { "line": IDENTITY_LINE },
                "rotate",
                { "sleep_ms": 1000 },
                { "line": HEARTBEAT_LINE },
            ],
        }),
        log.path(),
        Duration::ZERO,
    );
    outcome.expect("operational");
    assert!(
        elapsed < OBSERVE_WITHIN,
        "operational once both lines appear: {elapsed:?}"
    );
}

fn package_scm_observe_a_service_that_stops_is_42_with_its_exit_codes() {
    let log = earlier_log();
    let (outcome, elapsed) = observed(
        serde_json::json!({
            "log": [{ "line": IDENTITY_LINE }, { "sleep_ms": 1000 }],
            "stop_with": 13,
        }),
        log.path(),
        Duration::ZERO,
    );
    assert_refused(
        outcome,
        codes::SERVICE_STOPPED,
        &["Win32 exit code 1066", "service-specific exit code 13"],
    );
    assert!(
        elapsed < OBSERVE_WITHIN,
        "42 as soon as it reports STOPPED: {elapsed:?}"
    );

    // A clean stop, whatever its code, is 42 too.
    let log = tempfile::tempdir().expect("a temporary directory");
    let (outcome, elapsed) = observed(
        serde_json::json!({ "log": [{ "sleep_ms": 1000 }], "stop_with": 0 }),
        log.path(),
        Duration::ZERO,
    );
    assert_refused(
        outcome,
        codes::SERVICE_STOPPED,
        &["Win32 exit code 0", "service-specific exit code 0"],
    );
    assert!(elapsed < OBSERVE_WITHIN, "42 for a clean stop: {elapsed:?}");
}

fn package_scm_observe_a_service_still_pending_at_the_end_is_43() {
    // The 60 seconds run from the start request the caller gives: here 15
    // seconds before the start itself.
    let log = tempfile::tempdir().expect("a temporary directory");
    let (outcome, elapsed) = observed(
        serde_json::json!({ "start_pending_ms": 75_000 }),
        log.path(),
        Duration::from_secs(15),
    );
    assert_refused(outcome, codes::PENDING, &["state 2", "60 seconds"]);
    assert!(
        elapsed >= OBSERVE_WITHIN && elapsed < OBSERVE_WITHIN + Duration::from_secs(10),
        "43 at the end of the 60 seconds from the start request: {elapsed:?}"
    );
}

fn package_scm_observe_a_pending_service_whose_two_lines_appear_is_43() {
    // Both lines, in order, while it reports START_PENDING past the end: not
    // operational, since it is not RUNNING.
    let log = tempfile::tempdir().expect("a temporary directory");
    let (outcome, elapsed) = observed(
        serde_json::json!({
            "start_pending_ms": 75_000,
            "log_while_pending": true,
            "log": [{ "line": IDENTITY_LINE }, { "line": HEARTBEAT_LINE }],
        }),
        log.path(),
        Duration::ZERO,
    );
    assert_refused(outcome, codes::PENDING, &["state 2", "60 seconds"]);
    assert!(
        elapsed >= OBSERVE_WITHIN && elapsed < OBSERVE_WITHIN + Duration::from_secs(10),
        "43 at the end of the 60 seconds: {elapsed:?}"
    );
}

fn package_scm_observe_a_running_service_without_both_lines_in_order_is_44() {
    // The earlier run's lines precede the position, and this run writes the
    // two lines out of order.
    let log = earlier_log();
    let (outcome, elapsed) = observed(
        serde_json::json!({
            "log": [{ "line": HEARTBEAT_LINE }, { "line": IDENTITY_LINE }],
        }),
        log.path(),
        Duration::ZERO,
    );
    assert_refused(
        outcome,
        codes::NOT_OBSERVABLE,
        &[
            &format!("did not show \"{HEARTBEAT_LINE}\" after \"{IDENTITY_LINE}\""),
            "60 seconds",
        ],
    );
    assert!(
        elapsed >= OBSERVE_WITHIN && elapsed < OBSERVE_WITHIN + Duration::from_secs(10),
        "44 at the end of the 60 seconds: {elapsed:?}"
    );
}

fn package_scm_observe_a_running_service_with_no_line_is_44() {
    let log = earlier_log();
    let (outcome, elapsed) = observed(serde_json::json!({}), log.path(), Duration::ZERO);
    assert_refused(
        outcome,
        codes::NOT_OBSERVABLE,
        &[
            &format!("did not show \"{IDENTITY_LINE}\" and then \"{HEARTBEAT_LINE}\""),
            "60 seconds",
        ],
    );
    assert!(
        elapsed >= OBSERVE_WITHIN && elapsed < OBSERVE_WITHIN + Duration::from_secs(10),
        "44 at the end of the 60 seconds: {elapsed:?}"
    );
}

fn package_scm_observe_a_state_that_cannot_be_read_after_the_start_is_44() {
    // SPEC-024 Amendment 2026-11-11, item 4: the start accepted, then a DACL
    // that lets SYSTEM alone read the service.
    let log = tempfile::tempdir().expect("a temporary directory");
    let mut behaviour = serde_json::json!({});
    behaviour["log_dir"] = serde_json::Value::String(log.path().display().to_string());
    let files = Controllable::new(behaviour);
    let service = TestService::controllable(&files);
    let position = LogPosition::record(log.path()).expect("record the log's position");
    let started = Instant::now();
    scm::start(&manager(), &service.name, &[]).expect("the start");
    service.set_dacl("D:P(A;;GA;;;SY)");
    let outcome = observer::observe(&manager(), &service.name, log.path(), position, started);
    eprintln!("observed {}: {outcome:?}", service.name);
    assert_refused(
        outcome,
        codes::NOT_OBSERVABLE,
        &[&service.name, "cannot be read", "Win32 error 5"],
    );
}

// --- The runner ---------------------------------------------------------------------

// --- The wizard's second reading (SPEC-024 Amendment 2026-11-13, point 3) ----

/// A starter that runs.
struct Running;

impl Starter for Running {
    fn running(&self) -> bool {
        true
    }
}

/// A wizard that runs `during` while its dialog would show, then answers.
struct Wizarding<F: Fn() -> Answer>(F);

impl<F: Fn() -> Answer> Wizard for Wizarding<F> {
    fn hold_starter(&self) -> Result<Box<dyn Starter>, Refusal> {
        Ok(Box::new(Running))
    }

    fn ask(
        &self,
        _: Offer,
        _: &dyn Starter,
        _: &mut dyn FnMut(&Fields) -> Result<Staged, Refusal>,
    ) -> Result<Answer, Refusal> {
        Ok((self.0)())
    }
}

impl TestService {
    /// Change its command line.
    fn set_command_line(&self, command_line: &str) {
        let command = wide(command_line);
        // SAFETY: the handle has SERVICE_ALL_ACCESS; the string lives for
        // the call and every other value is left as it is.
        let ok = unsafe {
            ChangeServiceConfigW(
                self.handle,
                SERVICE_NO_CHANGE,
                SERVICE_NO_CHANGE,
                SERVICE_NO_CHANGE,
                command.as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        assert!(
            ok != 0,
            "ChangeServiceConfigW {}: Win32 error {}",
            self.name,
            last_error()
        );
    }
}

/// The record's reading of the service is the one after the dialog: a
/// service started while it showed is recorded running when the record is
/// written, so the stop finds what the record says and no rewrite follows.
fn package_scm_verbs_wizard_records_the_service_as_read_after_the_dialog() {
    // The behaviour file is `agent.toml`: a pending host, which the wizard
    // offers to keep.
    let host = verbs_host(serde_json::json!({ "stop_ms": 1000 }), SERVICE_DEMAND_START);
    let wizard = Wizarding(|| {
        host.service.start();
        host.service
            .wait_for(SERVICE_RUNNING, Duration::from_secs(20));
        Answer::Keep
    });
    let steps = std::sync::Mutex::new(Vec::new());
    let watch = recording(&steps);
    let machine = Machine {
        wizard: Some(&wizard),
        ..host.machine(&watch)
    };
    assert_eq!(
        verbs::preflight(&machine, &Request::default().with_ui(Some("4"))),
        Ok(())
    );
    drop(machine);
    drop(watch);
    let steps = steps.into_inner().unwrap();
    assert!(steps.contains(&Step::Write(Name::Record)), "{steps:?}");
    assert!(
        !steps.contains(&Step::RewriteRecord),
        "the record was written with the second reading: {steps:?}"
    );
    let record = read_record(&host.machine(&unwatched).setup)
        .unwrap()
        .expect("a record");
    assert!(record.service_running);
    assert_eq!(
        host.service.state(),
        SERVICE_STOPPED,
        "preflight stopped it"
    );
}

/// A product service that conflicts once the dialog has shown is 26, with
/// nothing staged and no record.
fn package_scm_verbs_wizard_a_service_conflicting_after_the_dialog_is_26() {
    let host = verbs_host(serde_json::json!({}), SERVICE_DEMAND_START);
    let other = host.root.operator.join("another").join("cg-agent.exe");
    let wizard = Wizarding(|| {
        host.service
            .set_command_line(&product_command_line(&other, &host.root.paths.config));
        Answer::Keep
    });
    let machine = Machine {
        wizard: Some(&wizard),
        ..host.machine(&unwatched)
    };
    let refusal =
        verbs::preflight(&machine, &Request::default().with_ui(Some("4"))).expect_err("26");
    assert_eq!(refusal.code, codes::CONFLICTING_SERVICE, "{}", refusal.line);
    assert!(!host.record().exists());
    assert!(!host.root.paths.staged_config().exists());
}

/// A product service that conflicts before the dialog is 26, and no dialog
/// shows (point 3: the wizard shows after 26 and 29).
fn package_scm_verbs_wizard_a_conflicting_service_is_26_before_the_dialog() {
    let host = verbs_host(serde_json::json!({}), SERVICE_DEMAND_START);
    let wizard = Wizarding(|| panic!("no dialog shows"));
    let machine = Machine {
        wizard: Some(&wizard),
        binary: host.root.operator.join("another").join("cg-agent.exe"),
        ..host.machine(&unwatched)
    };
    let refusal =
        verbs::preflight(&machine, &Request::default().with_ui(Some("4"))).expect_err("26");
    assert_eq!(refusal.code, codes::CONFLICTING_SERVICE, "{}", refusal.line);
    assert!(!host.record().exists());
}

/// The tests, in the order they run.
const TESTS: &[(&str, fn())] = &[
    (
        "package_scm_verbs_wizard_a_conflicting_service_is_26_before_the_dialog",
        package_scm_verbs_wizard_a_conflicting_service_is_26_before_the_dialog,
    ),
    (
        "package_scm_verbs_wizard_records_the_service_as_read_after_the_dialog",
        package_scm_verbs_wizard_records_the_service_as_read_after_the_dialog,
    ),
    (
        "package_scm_verbs_wizard_a_service_conflicting_after_the_dialog_is_26",
        package_scm_verbs_wizard_a_service_conflicting_after_the_dialog_is_26,
    ),
    (
        "package_scm_a_service_of_the_products_configuration_is_no_conflict",
        package_scm_a_service_of_the_products_configuration_is_no_conflict,
    ),
    (
        "package_scm_no_product_service_is_no_conflict_and_needs_no_stop",
        package_scm_no_product_service_is_no_conflict_and_needs_no_stop,
    ),
    (
        "package_scm_each_difference_from_the_products_configuration_is_26",
        package_scm_each_difference_from_the_products_configuration_is_26,
    ),
    (
        "package_scm_an_unreadable_configuration_is_26_and_an_unreadable_state_40",
        package_scm_an_unreadable_configuration_is_26_and_an_unreadable_state_40,
    ),
    (
        "package_scm_service_running_in_every_state_but_stopped",
        package_scm_service_running_in_every_state_but_stopped,
    ),
    (
        "package_scm_stop_proves_a_running_service_stopped",
        package_scm_stop_proves_a_running_service_stopped,
    ),
    (
        "package_scm_the_stop_reports_its_first_reading",
        package_scm_the_stop_reports_its_first_reading,
    ),
    (
        "package_scm_stop_sends_the_control_again_while_the_service_cannot_accept_it",
        package_scm_stop_sends_the_control_again_while_the_service_cannot_accept_it,
    ),
    (
        "package_scm_the_stop_waits_75_seconds_and_no_more",
        package_scm_the_stop_waits_75_seconds_and_no_more,
    ),
    (
        "package_scm_the_stop_bound_holds_while_a_control_blocks",
        package_scm_the_stop_bound_holds_while_a_control_blocks,
    ),
    (
        "package_scm_the_failure_actions_are_set_and_their_flag_cleared",
        package_scm_the_failure_actions_are_set_and_their_flag_cleared,
    ),
    (
        "package_scm_the_reversals_start_starts_a_stopped_service_with_no_argument",
        package_scm_the_reversals_start_starts_a_stopped_service_with_no_argument,
    ),
    (
        "package_scm_a_reversals_start_the_scm_refuses_is_40",
        package_scm_a_reversals_start_the_scm_refuses_is_40,
    ),
    (
        "package_scm_verbs_preflight_records_a_service_started_after_its_check",
        package_scm_verbs_preflight_records_a_service_started_after_its_check,
    ),
    (
        "package_scm_verbs_preflight_stops_a_running_service_and_the_reversal_starts_it",
        package_scm_verbs_preflight_stops_a_running_service_and_the_reversal_starts_it,
    ),
    (
        "package_scm_verbs_configure_starts_with_the_records_fingerprints_and_observes",
        package_scm_verbs_configure_starts_with_the_records_fingerprints_and_observes,
    ),
    (
        "package_scm_verbs_configure_a_record_left_keeps_the_outcomes_code",
        package_scm_verbs_configure_a_record_left_keeps_the_outcomes_code,
    ),
    (
        "package_scm_verbs_configure_a_refused_start_is_41_and_leaves_the_record",
        package_scm_verbs_configure_a_refused_start_is_41_and_leaves_the_record,
    ),
    (
        "package_scm_verbs_preflight_records_a_service_running_at_its_check",
        package_scm_verbs_preflight_records_a_service_running_at_its_check,
    ),
    (
        "package_scm_verbs_preflight_a_service_not_proved_stopped_is_31",
        package_scm_verbs_preflight_a_service_not_proved_stopped_is_31,
    ),
    (
        "package_scm_verbs_preflight_a_conflicting_service_is_26_before_the_staging",
        package_scm_verbs_preflight_a_conflicting_service_is_26_before_the_staging,
    ),
    (
        "package_scm_verbs_configure_on_the_migration_route_starts_with_its_arguments",
        package_scm_verbs_configure_on_the_migration_route_starts_with_its_arguments,
    ),
    (
        "package_scm_verbs_configure_an_unrecordable_log_position_is_40",
        package_scm_verbs_configure_an_unrecordable_log_position_is_40,
    ),
    (
        "package_scm_the_start_passes_its_arguments_or_none",
        package_scm_the_start_passes_its_arguments_or_none,
    ),
    (
        "package_scm_a_start_the_scm_refuses_is_41",
        package_scm_a_start_the_scm_refuses_is_41,
    ),
    (
        "package_scm_observe_a_service_whose_two_lines_appear_in_order_is_operational",
        package_scm_observe_a_service_whose_two_lines_appear_in_order_is_operational,
    ),
    (
        "package_scm_observe_a_service_that_stops_is_42_with_its_exit_codes",
        package_scm_observe_a_service_that_stops_is_42_with_its_exit_codes,
    ),
    (
        "package_scm_observe_a_service_still_pending_at_the_end_is_43",
        package_scm_observe_a_service_still_pending_at_the_end_is_43,
    ),
    (
        "package_scm_observe_a_pending_service_whose_two_lines_appear_is_43",
        package_scm_observe_a_pending_service_whose_two_lines_appear_is_43,
    ),
    (
        "package_scm_observe_a_running_service_without_both_lines_in_order_is_44",
        package_scm_observe_a_running_service_without_both_lines_in_order_is_44,
    ),
    (
        "package_scm_observe_a_running_service_with_no_line_is_44",
        package_scm_observe_a_running_service_with_no_line_is_44,
    ),
    (
        "package_scm_observe_a_state_that_cannot_be_read_after_the_start_is_44",
        package_scm_observe_a_state_that_cannot_be_read_after_the_start_is_44,
    ),
];

/// libtest's options that take a value as the next argument.
const VALUE_OPTIONS: &[&str] = &[
    "--test-threads",
    "--skip",
    "--logfile",
    "--format",
    "--color",
    "--shuffle-seed",
    "-Z",
];

/// The tests a libtest command line selects: its filters, `--skip`,
/// `--exact`, and `--ignored` (none of these tests is ignored); `--list`
/// lists them instead of running them.
struct Selection {
    filters: Vec<String>,
    skips: Vec<String>,
    exact: bool,
    ignored_only: bool,
    list: bool,
}

impl Selection {
    fn parse(arguments: &[String]) -> Self {
        let mut selection = Selection {
            filters: Vec::new(),
            skips: Vec::new(),
            exact: false,
            ignored_only: false,
            list: false,
        };
        let mut arguments = arguments.iter();
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--list" => selection.list = true,
                "--exact" => selection.exact = true,
                "--ignored" => selection.ignored_only = true,
                "--skip" => selection.skips.extend(arguments.next().cloned()),
                option if VALUE_OPTIONS.contains(&option) => {
                    arguments.next();
                }
                option if option.starts_with("--skip=") => {
                    selection.skips.push(option["--skip=".len()..].to_string());
                }
                option if option.starts_with('-') => {}
                filter => selection.filters.push(filter.to_string()),
            }
        }
        selection
    }

    fn matches(&self, name: &str, pattern: &str) -> bool {
        if self.exact {
            name == pattern
        } else {
            name.contains(pattern)
        }
    }

    fn selects(&self, name: &str) -> bool {
        !self.ignored_only
            && (self.filters.is_empty() || self.filters.iter().any(|f| self.matches(name, f)))
            && !self.skips.iter().any(|s| self.matches(name, s))
    }
}

/// As the SCM's service, `--service --config <file>`; otherwise the tests,
/// one at a time, in libtest's report form, selected as libtest selects them
/// (filters, `--skip`, `--exact`, `--ignored`, `--list`); its other options
/// (`--test-threads=1`, `--nocapture`, ...) change nothing.
pub fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.first().map(String::as_str) == Some("--service") {
        service::main(&arguments);
        return;
    }
    let selection = Selection::parse(&arguments);
    let selected: Vec<&(&str, fn())> = TESTS
        .iter()
        .filter(|(name, _)| selection.selects(name))
        .collect();
    if selection.list {
        for (name, _) in &selected {
            println!("{name}: test");
        }
        println!("\n{} tests, 0 benchmarks", selected.len());
        return;
    }
    println!("\nrunning {} tests", selected.len());
    let started = Instant::now();
    let mut failed = Vec::new();
    for (name, test) in &selected {
        let outcome = std::panic::catch_unwind(*test);
        println!(
            "test {name} ... {}",
            if outcome.is_ok() { "ok" } else { "FAILED" }
        );
        if outcome.is_err() {
            failed.push(*name);
        }
    }
    if !failed.is_empty() {
        println!("\nfailures:");
        for name in &failed {
            println!("    {name}");
        }
    }
    println!(
        "\ntest result: {}. {} passed; {} failed; 0 ignored; 0 measured; {} filtered out; \
         finished in {:.2}s\n",
        if failed.is_empty() { "ok" } else { "FAILED" },
        selected.len() - failed.len(),
        failed.len(),
        TESTS.len() - selected.len(),
        started.elapsed().as_secs_f64()
    );
    if !failed.is_empty() {
        std::process::exit(101);
    }
}
