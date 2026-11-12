//! The real `cg-setup` against the host (SPEC-024 Amendment 2026-11-10,
//! *Tests*, under the Amendment 2026-11-09's *Tests*): `preflight` and then
//! `reverse` on a fresh hosted runner, with a bootstrap file and anchor this
//! test creates and a token from which every privilege but
//! `SeChangeNotifyPrivilege` is deleted. Preflight stages the anchor and the
//! configuration in `<P>` and the data root with item 3's security and writes
//! `setup-attempt.json`; the reversal removes all of it. That the staging
//! needs no privilege is the token's: it holds none to enable.
//!
//! The test writes under the machine's `%ProgramData%`, so it builds only
//! with the test-only `host-tests` feature and runs only in `package-ci`, on
//! a GitHub-hosted runner discarded after the job (CLAUDE.md, *Local
//! pre-commit gate*, *Windows package*). It fails without acting outside such
//! a runner, when a service named `CyberGuardAgent` exists, or when
//! `%ProgramData%\CyberGuard` or `%ProgramFiles%\CyberGuard` exists; `Host`
//! removes what the run left on every exit path.
//!
//! The paths, the security and the record's keys are spelled from the SPEC,
//! not from `cg-setup`'s code: the security as the Amendment 2026-11-09,
//! item 3, states it, owner Administrators and a protected DACL for SYSTEM
//! and Administrators alone, not the access masks or inheritance flags the
//! setup chooses.
#![cfg(windows)]

mod fixtures;

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::io::AsRawHandle as _;
use std::path::{Path, PathBuf};

use cg_setup::scm::{self, Manager, Product, Route};
use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, LUID};

use fixtures::{bootstrap_text, certificate, TOKEN};

const EXE: &str = env!("CARGO_BIN_EXE_cg-setup");

/// The product service (SPEC-024 Amendment 2026-11-10).
const PRODUCT_SERVICE: &str = "CyberGuardAgent";

/// The privilege the token keeps.
const CHANGE_NOTIFY: &str = "SeChangeNotifyPrivilege";

/// The fixed paths under the machine's `%ProgramData%` (§Data contracts §2).
struct Paths {
    product_root: PathBuf,
    data_root: PathBuf,
    staged_config: PathBuf,
    staged_anchor: PathBuf,
    record: PathBuf,
}

impl Paths {
    fn machine() -> Self {
        let program_data = cg_agent::custody::program_data().expect("%ProgramData%");
        let product_root = program_data.join("CyberGuard");
        let data_root = product_root.join("agent");
        Paths {
            staged_config: data_root.join("agent.toml.setup"),
            staged_anchor: data_root.join("server-ca.pem.setup"),
            record: data_root.join("setup-attempt.json"),
            product_root,
            data_root,
        }
    }

    fn all(&self) -> [&Path; 5] {
        [
            &self.product_root,
            &self.data_root,
            &self.staged_config,
            &self.staged_anchor,
            &self.record,
        ]
    }
}

fn exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

/// Fail without acting unless this is a GitHub-hosted runner with no
/// product service and no product directory.
fn refuse_unless_disposable_runner(paths: &Paths) {
    let actions = std::env::var("GITHUB_ACTIONS").ok();
    let environment = std::env::var("RUNNER_ENVIRONMENT").ok();
    assert!(
        actions.as_deref() == Some("true") && environment.as_deref() == Some("github-hosted"),
        "refusing to run cg-setup against the host outside a GitHub-hosted runner \
         (GITHUB_ACTIONS={actions:?}, RUNNER_ENVIRONMENT={environment:?})"
    );
    let manager = Manager::open().expect("the SCM, for connecting");
    let found = scm::check_product_service(
        &manager,
        PRODUCT_SERVICE,
        Path::new(r"C:\absent.exe"),
        Path::new(r"C:\absent.toml"),
        Route::Other,
    );
    assert_eq!(
        found,
        Ok(Product::Absent),
        "refusing to act: {PRODUCT_SERVICE} is not established absent"
    );
    let program_files = cg_agent::custody::program_files().expect("%ProgramFiles%");
    for directory in [&paths.product_root, &program_files.join("CyberGuard")] {
        assert!(
            !exists(directory),
            "refusing to act: {} exists on this host",
            directory.display()
        );
    }
}

/// What the run leaves under `%ProgramData%`, removed on drop, guard first.
struct Host {
    paths: Paths,
}

impl Host {
    fn take() -> Self {
        let paths = Paths::machine();
        refuse_unless_disposable_runner(&paths);
        Host { paths }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        if exists(&self.paths.product_root) {
            if let Err(error) = std::fs::remove_dir_all(&self.paths.product_root) {
                eprintln!("remove {}: {error}", self.paths.product_root.display());
            }
        }
    }
}

// --- The token ---------------------------------------------------------------------

fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(Some(0)).collect()
}

/// A token handle, closed on drop.
struct Token(HANDLE);

impl Drop for Token {
    fn drop(&mut self) {
        // SAFETY: a token handle this value owns.
        unsafe { CloseHandle(self.0) };
    }
}

/// The privileges `token` holds, by LUID and name.
fn privileges(token: &Token) -> Vec<(LUID, String)> {
    use windows_sys::Win32::Security::{
        GetTokenInformation, LookupPrivilegeNameW, TokenPrivileges, TOKEN_PRIVILEGES,
    };
    let mut needed = 0u32;
    // SAFETY: a size query.
    unsafe {
        GetTokenInformation(
            token.0,
            TokenPrivileges,
            std::ptr::null_mut(),
            0,
            &mut needed,
        )
    };
    let mut buffer = vec![0u64; (needed as usize).div_ceil(8).max(1)];
    // SAFETY: `buffer` holds at least `needed` bytes, u64-aligned.
    assert_ne!(
        unsafe {
            GetTokenInformation(
                token.0,
                TokenPrivileges,
                buffer.as_mut_ptr().cast(),
                (buffer.len() * 8) as u32,
                &mut needed,
            )
        },
        0,
        "GetTokenInformation TokenPrivileges"
    );
    // SAFETY: the buffer holds a TOKEN_PRIVILEGES and its array.
    let entries = unsafe {
        let list = buffer.as_ptr() as *const TOKEN_PRIVILEGES;
        std::slice::from_raw_parts((*list).Privileges.as_ptr(), (*list).PrivilegeCount as usize)
    };
    entries
        .iter()
        .map(|entry| {
            let mut name = [0u16; 128];
            let mut length = name.len() as u32;
            // SAFETY: a LUID of this token and a buffer of `length` units.
            assert_ne!(
                unsafe {
                    LookupPrivilegeNameW(
                        std::ptr::null(),
                        &entry.Luid,
                        name.as_mut_ptr(),
                        &mut length,
                    )
                },
                0,
                "LookupPrivilegeNameW"
            );
            (
                entry.Luid,
                String::from_utf16_lossy(&name[..length as usize]),
            )
        })
        .collect()
}

/// This process's primary token with every privilege but
/// `SeChangeNotifyPrivilege` deleted (`PrivilegesToDelete`), its groups
/// unchanged: an elevated administrator's token with nothing to enable.
fn token_without_privileges() -> Token {
    use windows_sys::Win32::Security::{
        CreateRestrictedToken, LUID_AND_ATTRIBUTES, TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE,
        TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut process = std::ptr::null_mut();
    // SAFETY: the current process's pseudo-handle and an out-value.
    assert_ne!(
        unsafe {
            OpenProcessToken(
                GetCurrentProcess(),
                TOKEN_QUERY | TOKEN_DUPLICATE | TOKEN_ASSIGN_PRIMARY,
                &mut process,
            )
        },
        0,
        "OpenProcessToken"
    );
    let process = Token(process);
    let held = privileges(&process);
    assert!(
        held.iter().any(|(_, name)| name == CHANGE_NOTIFY) && held.len() > 1,
        "the runner's token holds privileges to delete: {:?}",
        held.iter().map(|(_, name)| name).collect::<Vec<_>>()
    );
    let delete: Vec<LUID_AND_ATTRIBUTES> = held
        .iter()
        .filter(|(_, name)| name != CHANGE_NOTIFY)
        .map(|(luid, _)| LUID_AND_ATTRIBUTES {
            Luid: *luid,
            Attributes: 0,
        })
        .collect();
    let mut restricted = std::ptr::null_mut();
    // SAFETY: a primary token opened for duplication, the privileges to
    // delete, and an out-value.
    assert_ne!(
        unsafe {
            CreateRestrictedToken(
                process.0,
                0,
                0,
                std::ptr::null(),
                delete.len() as u32,
                delete.as_ptr(),
                0,
                std::ptr::null(),
                &mut restricted,
            )
        },
        0,
        "CreateRestrictedToken"
    );
    let token = Token(restricted);
    let kept: Vec<String> = privileges(&token).into_iter().map(|(_, n)| n).collect();
    assert_eq!(kept, [CHANGE_NOTIFY], "the token keeps one privilege");
    token
}

/// What a run of the setup gave.
#[derive(Debug)]
struct Run {
    code: u32,
    stdout: String,
    stderr: String,
}

/// The setup run under `token`, its output written to files of this test's.
fn run_as(token: &Token, arguments: &[&str]) -> Run {
    use windows_sys::Win32::Foundation::{
        SetHandleInformation, HANDLE_FLAG_INHERIT, WAIT_OBJECT_0,
    };
    use windows_sys::Win32::System::Threading::{
        CreateProcessAsUserW, GetExitCodeProcess, WaitForSingleObject, PROCESS_INFORMATION,
        STARTF_USESTDHANDLES, STARTUPINFOW,
    };

    let directory = tempfile::tempdir().expect("a temporary directory");
    let files = ["stdout", "stderr"].map(|name| {
        let file = std::fs::File::create(directory.path().join(name)).expect("an output file");
        // SAFETY: a handle this File owns, made inheritable for the child.
        assert_ne!(
            unsafe {
                SetHandleInformation(
                    file.as_raw_handle() as HANDLE,
                    HANDLE_FLAG_INHERIT,
                    HANDLE_FLAG_INHERIT,
                )
            },
            0,
            "SetHandleInformation"
        );
        file
    });
    let mut command_line = format!("\"{EXE}\"");
    for argument in arguments {
        command_line.push_str(&format!(" \"{argument}\""));
    }
    let mut command_line = wide(OsStr::new(&command_line));
    // SAFETY: all-zero STARTUPINFOW and PROCESS_INFORMATION are valid.
    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    startup.dwFlags = STARTF_USESTDHANDLES;
    startup.hStdOutput = files[0].as_raw_handle() as HANDLE;
    startup.hStdError = files[1].as_raw_handle() as HANDLE;
    let mut process: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let application = wide(OsStr::new(EXE));
    // SAFETY: a restricted version of this process's primary token; every
    // string lives for the call; the handles it returns are closed below.
    let ok = unsafe {
        CreateProcessAsUserW(
            token.0,
            application.as_ptr(),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
            0,
            std::ptr::null(),
            std::ptr::null(),
            &startup,
            &mut process,
        )
    };
    assert_ne!(
        ok,
        0,
        "CreateProcessAsUserW: {}",
        std::io::Error::last_os_error()
    );
    let mut code = 0u32;
    // SAFETY: the handles the call returned, each closed once.
    unsafe {
        assert_eq!(
            WaitForSingleObject(process.hProcess, 180_000),
            WAIT_OBJECT_0,
            "cg-setup ends within three minutes"
        );
        assert_ne!(GetExitCodeProcess(process.hProcess, &mut code), 0);
        CloseHandle(process.hThread);
        CloseHandle(process.hProcess);
    }
    drop(files);
    let read = |name: &str| std::fs::read_to_string(directory.path().join(name)).unwrap();
    let run = Run {
        code,
        stdout: read("stdout"),
        stderr: read("stderr"),
    };
    fixtures::assert_clean(&run.stdout);
    fixtures::assert_clean(&run.stderr);
    run
}

// --- Security ----------------------------------------------------------------------

/// An object's owner and DACL as SDDL.
fn owner_and_dacl(path: &Path) -> String {
    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetNamedSecurityInfoW,
        SDDL_REVISION_1, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::{
        DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
    };
    let name = wide(path.as_os_str());
    let information = OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: a NUL-terminated path; the descriptor is LocalAlloc'd, freed below.
    let error = unsafe {
        GetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            information,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    assert_eq!(error, 0, "GetNamedSecurityInfoW {}", path.display());
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: the descriptor above; `text` is LocalAlloc'd, freed below.
    let ok = unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor,
            SDDL_REVISION_1,
            information,
            &mut text,
            std::ptr::null_mut(),
        )
    };
    // SAFETY: the descriptor the first call allocated.
    unsafe { LocalFree(descriptor) };
    assert_ne!(
        ok, 0,
        "ConvertSecurityDescriptorToStringSecurityDescriptorW"
    );
    // SAFETY: a NUL-terminated string from the call, read then freed.
    unsafe {
        let length = (0..).take_while(|&i| *text.add(i) != 0).count();
        let sddl = String::from_utf16_lossy(std::slice::from_raw_parts(text, length));
        LocalFree(text.cast());
        sddl
    }
}

/// The Amendment 2026-11-09, item 3: owner Administrators and a protected
/// DACL whose entries all allow, SYSTEM and Administrators alone, each named.
fn assert_security(path: &Path) {
    let sddl = owner_and_dacl(path);
    let (owner, dacl) = sddl
        .strip_prefix("O:")
        .and_then(|rest| rest.split_once("D:"))
        .unwrap_or_else(|| panic!("{}: {sddl}", path.display()));
    assert_eq!(
        owner,
        "BA",
        "{}: owner Administrators: {sddl}",
        path.display()
    );
    let (flags, list) = dacl.split_at(dacl.find('(').unwrap_or(dacl.len()));
    assert!(
        flags.contains('P'),
        "{}: a protected DACL: {sddl}",
        path.display()
    );
    let mut trustees = BTreeSet::new();
    for ace in list.split_inclusive(')') {
        let fields: Vec<&str> = ace
            .trim_matches(|c| c == '(' || c == ')')
            .split(';')
            .collect();
        assert!(
            fields.len() == 6 && fields[0] == "A",
            "{}: an allow entry: {ace} in {sddl}",
            path.display()
        );
        trustees.insert(fields[5].to_string());
    }
    assert_eq!(
        trustees,
        BTreeSet::from(["BA".to_string(), "SY".to_string()]),
        "{}: SYSTEM and Administrators alone: {sddl}",
        path.display()
    );
}

/// Read a file as the runner's Administrator.
fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

// --- The test ----------------------------------------------------------------------

/// Preflight on a fresh host stages with item 3's security and the record of
/// item 5, under a token with no privilege to enable; the reversal then
/// removes the staged files, the record, the data root and `<P>`, and starts
/// nothing, the record saying the service was not running.
#[test]
fn package_setup_host_preflight_then_reverse_with_no_privilege() {
    let host = Host::take();
    let paths = &host.paths;

    // The operator's inputs: the anchor's source and a bootstrap file only
    // SYSTEM and Administrators can reach.
    let operator = tempfile::tempdir().expect("the operator's directory");
    let certificate = certificate("host.example.test");
    let anchor = operator.path().join("server-ca.pem");
    std::fs::write(&anchor, &certificate.pem).expect("the anchor's source");
    let bootstrap = operator.path().join("bootstrap.toml");
    cg_agent::custody::create_new_file(
        &bootstrap,
        "O:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)",
        bootstrap_text(
            anchor.to_str().expect("a Unicode path"),
            &[&certificate.fingerprint],
        )
        .as_bytes(),
    )
    .expect("the bootstrap file");
    let token = token_without_privileges();

    // Preflight.
    let bootstrap_argument = format!("--bootstrap={}", bootstrap.display());
    let run = run_as(&token, &["preflight", &bootstrap_argument]);
    assert_eq!(run.code, 0, "preflight: {run:?}");
    for directory in [&paths.product_root, &paths.data_root] {
        assert_security(directory);
    }
    for file in [&paths.staged_anchor, &paths.staged_config, &paths.record] {
        assert_security(file);
    }
    let staged: Vec<Vec<u8>> = pem::parse_many(read(&paths.staged_anchor))
        .expect("the staged anchor is PEM")
        .into_iter()
        .map(|block| block.into_contents())
        .collect();
    assert_eq!(
        staged,
        std::slice::from_ref(&certificate.der),
        "the anchor is staged"
    );
    let config: toml::Table = String::from_utf8(read(&paths.staged_config))
        .expect("UTF-8")
        .parse()
        .expect("the staged configuration is TOML");
    assert_eq!(
        config
            .get("enrollment")
            .and_then(|table| table.get("token"))
            .and_then(|value| value.as_str()),
        Some(TOKEN),
        "the token is staged"
    );
    assert!(
        config.get("setup").is_none(),
        "no setup table: {:?}",
        config.keys().collect::<Vec<_>>()
    );
    let record: serde_json::Value =
        serde_json::from_slice(&read(&paths.record)).expect("the record is JSON");
    let created: BTreeSet<&str> = record["created"]
        .as_array()
        .expect("created is an array")
        .iter()
        .map(|name| name.as_str().expect("a name"))
        .collect();
    assert_eq!(
        (
            record.as_object().map(|object| object.len()),
            &record["format"],
            created,
            &record["fingerprints"],
            &record["service_running"],
        ),
        (
            Some(4),
            &serde_json::json!(1),
            BTreeSet::from([
                "product_root",
                "data_root",
                "staged_anchor",
                "staged_config"
            ]),
            &serde_json::json!([certificate.fingerprint]),
            &serde_json::json!(false),
        ),
        "the record: {record}"
    );

    // The reversal.
    let run = run_as(&token, &["reverse"]);
    assert_eq!(run.code, 0, "reverse: {run:?}");
    let left: Vec<&Path> = paths
        .all()
        .into_iter()
        .filter(|path| exists(path))
        .collect();
    assert!(left.is_empty(), "the reversal removes everything: {left:?}");
    let manager = Manager::open().expect("the SCM, for connecting");
    assert_eq!(
        scm::check_product_service(
            &manager,
            PRODUCT_SERVICE,
            Path::new(r"C:\absent.exe"),
            Path::new(r"C:\absent.toml"),
            Route::Other,
        ),
        Ok(Product::Absent),
        "no service was created"
    );
}
