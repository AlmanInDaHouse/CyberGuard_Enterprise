//! `cg-setup.exe` itself (SPEC-024 Amendment 2026-11-10, items 1 and 9):
//! `--version`; a command line Burn did not form, 40 after the floor; each
//! verb's first checks, the platform and then the elevation; and its version
//! resource, the agent's with its own file name and no message table; and
//! the system directory as the only place its imports load from (the
//! Amendment 2026-11-12 (second), point 10).
//!
//! No test here lets a verb act on this host: a verb runs only under a
//! token whose Administrators group is for deny only, a filtered token
//! however this test runs, elevated in rust-ci's Windows job included, and
//! stops at the elevation's 21.
#![cfg(windows)]

mod fixtures;

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::io::AsRawHandle as _;
use std::path::Path;

use cg_setup::version::{numeric, read_installed, InstalledVersion, SETUP_VERSION};

const EXE: &str = env!("CARGO_BIN_EXE_cg-setup");

/// What a run of the setup gave.
#[derive(Debug)]
struct Run {
    code: u32,
    stdout: String,
    stderr: String,
}

/// The setup run directly, for a command line that runs no verb.
fn run(arguments: &[&str]) -> Run {
    let output = std::process::Command::new(EXE)
        .args(arguments)
        .output()
        .expect("run cg-setup");
    Run {
        code: output.status.code().expect("an exit code") as u32,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(Some(0)).collect()
}

/// This process's primary token with Administrators for deny only.
fn filtered_primary_token() -> windows_sys::Win32::Foundation::HANDLE {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::Security::{
        CreateRestrictedToken, CreateWellKnownSid, WinBuiltinAdministratorsSid,
        SECURITY_MAX_SID_SIZE, SID_AND_ATTRIBUTES, TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE,
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
    let mut sid = [0u8; SECURITY_MAX_SID_SIZE as usize];
    let mut size = SECURITY_MAX_SID_SIZE;
    // SAFETY: a buffer of `size` bytes for the SID, no domain.
    assert_ne!(
        unsafe {
            CreateWellKnownSid(
                WinBuiltinAdministratorsSid,
                std::ptr::null_mut(),
                sid.as_mut_ptr().cast(),
                &mut size,
            )
        },
        0,
        "CreateWellKnownSid"
    );
    let disable = SID_AND_ATTRIBUTES {
        Sid: sid.as_mut_ptr().cast(),
        Attributes: 0,
    };
    let mut restricted = std::ptr::null_mut();
    // SAFETY: a primary token opened for duplication, one SID to disable,
    // and an out-value.
    let ok = unsafe {
        CreateRestrictedToken(
            process,
            0,
            1,
            &disable,
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
            &mut restricted,
        )
    };
    // SAFETY: the token opened above.
    unsafe { CloseHandle(process) };
    assert_ne!(ok, 0, "CreateRestrictedToken");
    restricted
}

/// The setup run under a filtered token, with these environment variables
/// added, its output written to files of this test's.
fn run_filtered(arguments: &[&str], environment: &[(&str, &str)]) -> Run {
    use windows_sys::Win32::Foundation::{
        CloseHandle, SetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT, WAIT_OBJECT_0,
    };
    use windows_sys::Win32::System::Threading::{
        CreateProcessAsUserW, GetExitCodeProcess, WaitForSingleObject, CREATE_UNICODE_ENVIRONMENT,
        PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOW,
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
    let mut block: Vec<u16> = Vec::new();
    for (name, value) in std::env::vars_os()
        .map(|(n, v)| {
            (
                n.to_string_lossy().into_owned(),
                v.to_string_lossy().into_owned(),
            )
        })
        .chain(
            environment
                .iter()
                .map(|(n, v)| (n.to_string(), v.to_string())),
        )
    {
        block.extend(format!("{name}={value}").encode_utf16());
        block.push(0);
    }
    block.push(0);

    // SAFETY: all-zero STARTUPINFOW and PROCESS_INFORMATION are valid.
    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    startup.dwFlags = STARTF_USESTDHANDLES;
    startup.hStdOutput = files[0].as_raw_handle() as HANDLE;
    startup.hStdError = files[1].as_raw_handle() as HANDLE;
    let mut process: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let token = filtered_primary_token();
    let application = wide(OsStr::new(EXE));
    // SAFETY: the token is a restricted version of this process's primary
    // token; every string and the environment block live for the call; the
    // handles it returns are closed below.
    let ok = unsafe {
        CreateProcessAsUserW(
            token,
            application.as_ptr(),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
            CREATE_UNICODE_ENVIRONMENT,
            block.as_ptr().cast(),
            std::ptr::null(),
            &startup,
            &mut process,
        )
    };
    let error = std::io::Error::last_os_error();
    // SAFETY: the token created above.
    unsafe { CloseHandle(token) };
    assert_ne!(ok, 0, "CreateProcessAsUserW: {error}");
    let mut code = 0u32;
    // SAFETY: the handles the call returned, each closed once.
    unsafe {
        assert_eq!(
            WaitForSingleObject(process.hProcess, 60_000),
            WAIT_OBJECT_0,
            "cg-setup ends within a minute"
        );
        assert_ne!(GetExitCodeProcess(process.hProcess, &mut code), 0);
        CloseHandle(process.hThread);
        CloseHandle(process.hProcess);
    }
    drop(files);
    let read = |name: &str| std::fs::read_to_string(directory.path().join(name)).unwrap();
    Run {
        code,
        stdout: read("stdout"),
        stderr: read("stderr"),
    }
}

#[test]
fn package_setup_executable_answers_its_version() {
    let run = run(&["--version"]);
    assert_eq!(run.code, 0, "{run:?}");
    assert_eq!(run.stdout, format!("cg-setup {SETUP_VERSION}\n"));
    assert_eq!(run.stderr, "");
}

/// Any command line Burn does not form is 40, one line on standard error,
/// after the platform's floor: these run no verb, elevated or not.
#[test]
fn package_setup_executable_a_command_line_burn_did_not_form_is_40() {
    for arguments in [
        &[][..],
        &["install"],
        &["--version", "preflight"],
        &["preflight", "--bootstrap"],
        &["preflight", "--token=SECRET-VALUE"],
        &["reverse", "now"],
    ] {
        let run = run(arguments);
        assert_eq!(run.code, 40, "{arguments:?}: {run:?}");
        assert_eq!(run.stdout, "", "{arguments:?}");
        assert!(
            run.stderr
                .starts_with("cg-setup: the command line is not one the setup's bundle forms: "),
            "{arguments:?}: {run:?}"
        );
        assert_eq!(run.stderr.lines().count(), 1, "{arguments:?}: {run:?}");
        assert!(!run.stderr.contains("SECRET"), "{run:?}");
    }
}

/// Each verb checks the platform, then the elevation, before anything else:
/// under a filtered token each is 21, its one line the elevation's.
#[test]
fn package_setup_executable_each_verb_checks_the_elevation_first() {
    for arguments in [
        &["preflight"][..],
        &[
            "preflight",
            r"--bootstrap=C:\no such directory\bootstrap.toml",
        ],
        &["configure"],
        &[
            "configure",
            "--migrate-legacy=keep",
            "--trust-anchors=sha256:00",
        ],
        &["reverse"],
    ] {
        let run = run_filtered(arguments, &[]);
        assert_eq!(run.code, 21, "{arguments:?}: {run:?}");
        assert_eq!(
            run.stderr, "cg-setup: the setup must run elevated, as an administrator\n",
            "{arguments:?}"
        );
        assert_eq!(run.stdout, "", "{arguments:?}");
    }
}

/// The platform's floor is read before any other check (item 1): a
/// compatibility mode applied to the setup lowers the version
/// `RtlGetVersion` reports, which is 20 for each verb even under a filtered
/// token, and for a command line Burn did not form, the line naming the
/// version read; `--version` still answers.
#[test]
fn package_setup_executable_the_floor_comes_before_any_other_check() {
    let compatibility = [("__COMPAT_LAYER", "WIN7RTM")];
    for arguments in [
        &["preflight"][..],
        &["configure"],
        &["reverse"],
        &["preflight", "--x=1"],
        &["install"],
    ] {
        let run = run_filtered(arguments, &compatibility);
        assert_eq!(run.code, 20, "{arguments:?}: {run:?}");
        assert!(
            run.stderr.contains("Windows 10.0 build 17763 or later"),
            "{arguments:?}: {run:?}"
        );
        assert!(run.stderr.contains("this Windows reports 6.1"), "{run:?}");
        assert_eq!(run.stderr.lines().count(), 1, "{run:?}");
    }
    let run = run_filtered(&["--version"], &compatibility);
    assert_eq!(run.code, 0, "{run:?}");
    assert_eq!(
        run.stdout,
        format!(
            "cg-setup {SETUP_VERSION}
"
        )
    );
}

/// The version resource: `X.Y.Z.0` and the setup's own version string, the
/// agent's (`--version` agrees), its own `OriginalFilename`, and no message
/// table. Its file flags are the agent's too (the Amendment 2026-11-14, point
/// 8, #140): the pre-release flag in a development build, the `<Cargo package
/// version>-dev` one, and none in a release build; the version matrix runs
/// this in both.
#[test]
fn package_setup_executable_carries_the_agents_version_resource() {
    use windows_sys::Win32::Storage::FileSystem::VS_FF_PRERELEASE;
    use windows_sys::Win32::System::LibraryLoader::{
        FindResourceW, LoadLibraryExW, LOAD_LIBRARY_AS_DATAFILE,
    };
    let found = read_installed(Path::new(EXE))
        .expect("readable")
        .expect("present");
    let [x, y, z] = numeric(SETUP_VERSION).expect("X.Y.Z");
    assert_eq!(
        found,
        InstalledVersion {
            file_version: [x, y, z, 0],
            product_version: Some(SETUP_VERSION.to_string()),
        }
    );
    assert_eq!(
        fixtures::version_string(Path::new(EXE), "OriginalFilename").as_deref(),
        Some("cg-setup.exe")
    );
    assert_eq!(
        fixtures::version_string(Path::new(EXE), "FileVersion").as_deref(),
        Some(SETUP_VERSION)
    );
    let (build, expected) = if SETUP_VERSION.ends_with("-dev") {
        ("development", VS_FF_PRERELEASE)
    } else {
        ("release", 0)
    };
    let (mask, flags) = fixtures::file_flags(Path::new(EXE)).expect("the fixed file info");
    assert_eq!(
        mask & VS_FF_PRERELEASE,
        VS_FF_PRERELEASE,
        "the file flags' mask admits the pre-release flag"
    );
    assert_eq!(
        flags, expected,
        "file flags of a {build} build ({SETUP_VERSION})"
    );

    let path = wide(OsStr::new(EXE));
    // SAFETY: a NUL-terminated path; a data file runs none of its code.
    let module = unsafe {
        LoadLibraryExW(
            path.as_ptr(),
            std::ptr::null_mut(),
            LOAD_LIBRARY_AS_DATAFILE,
        )
    };
    assert!(!module.is_null(), "{}", std::io::Error::last_os_error());
    // RT_VERSION is found, RT_MESSAGETABLE is not.
    let find = |kind: usize| {
        // SAFETY: a loaded module and integer resource identifiers.
        !unsafe {
            FindResourceW(
                module,
                std::ptr::without_provenance(1),
                std::ptr::without_provenance(kind),
            )
        }
        .is_null()
    };
    let (version, messages) = (find(16), find(11));
    // SAFETY: the module loaded above.
    unsafe { windows_sys::Win32::Foundation::FreeLibrary(module) };
    assert!(version, "the version resource is found this way");
    assert!(!messages, "cg-setup.exe carries no message table");
}

/// SPEC-024 Amendment 2026-11-12 (second), point 10: Burn runs cg-setup
/// elevated from its package cache with the system's temporary folder as the
/// working directory, so the loader takes the system libraries it imports
/// from the system directory alone (`LOAD_LIBRARY_SEARCH_SYSTEM32`), which
/// its build sets in the image's load configuration.
#[test]
fn package_setup_executable_loads_its_imports_from_the_system_directory() {
    const LOAD_LIBRARY_SEARCH_SYSTEM32: u16 = 0x0800;
    assert_eq!(
        fixtures::dependent_load_flags(Path::new(EXE)),
        Some(LOAD_LIBRARY_SEARCH_SYSTEM32),
        "cg-setup.exe's DependentLoadFlags"
    );
}
