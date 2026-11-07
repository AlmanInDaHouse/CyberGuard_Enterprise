//! Fixtures for the legacy migration's unelevated tests (SPEC-024 Amendment
//! 2026-11-05, install_ac_013): a console layout as the console runbook
//! leaves it under a temporary `%ProgramData%`, the custody that names the
//! current user where the service's names SYSTEM, a `Watch` that injects
//! failures and crashes and records the points it saw, and helpers that
//! snapshot a tree's names, bytes, owner and DACL.
//!
//! The current user stands in SYSTEM's place, so real SYSTEM, which the
//! console's `persist_identity` allows on each identity file, is "the one
//! other SID", the enrolling account. An unelevated process cannot make
//! SYSTEM an owner, so the custody's SDDLs carry no owner part: what the
//! migration writes keeps the owner this process gives it.
//!
//! Each test crate declares `mod common; mod custody_fixtures;` beside this
//! module.
#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, Once, OnceLock};
use std::time::Duration;

use cg_agent::config::AgentConfig;
use cg_agent::custody::{self, Kind, Policy, Principal};
use cg_agent::enrollment::EnrolledIdentity;
use cg_agent::identity::installed::{self, InstalledCustody};
use cg_agent::identity::Identity;
use cg_agent::service::migration::{
    copies, prepare, Choice, Copies, Environment, Injected, MigrationCustody, Point, Prepared,
    Record, Request, Stop, Watch,
};
use cg_agent::service::InstalledPaths;
use chrono::{DateTime, Utc};
use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::CertificateDer;
use sha2::{Digest, Sha256};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_IO_PENDING, GENERIC_READ, HANDLE,
    INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW,
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{
    GetFileSecurityW, SetFileSecurityW, DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OVERLAPPED, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Ioctl::{
    FSCTL_REQUEST_BATCH_OPLOCK, FSCTL_REQUEST_OPLOCK, OPLOCK_LEVEL_CACHE_HANDLE,
    OPLOCK_LEVEL_CACHE_READ, OPLOCK_LEVEL_CACHE_WRITE, REQUEST_OPLOCK_CURRENT_VERSION,
    REQUEST_OPLOCK_INPUT_BUFFER, REQUEST_OPLOCK_INPUT_FLAG_REQUEST, REQUEST_OPLOCK_OUTPUT_BUFFER,
};
use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows_sys::Win32::System::IO::{DeviceIoControl, OVERLAPPED};

use crate::common;
use crate::custody_fixtures::{
    default_owner_sid, normalized, sid_bytes, user, user_only, user_sid,
};

/// The files of a console layout (item 2.2).
pub const FILES: [&str; 5] = [
    "agent.toml",
    "server-ca.pem",
    "cert.pem",
    "key.dat",
    "identity.json",
];
/// The identity's files, directly in the console's data root.
pub const IDENTITY_FILES: [&str; 3] = ["cert.pem", "key.dat", "identity.json"];
/// FR-014's leftover temporary (item 2.2).
pub const HYGIENE: &str = "agent.toml.hygiene-tmp";
/// The entries of a complete staging directory.
pub const STAGED: [&str; 4] = ["agent.toml", "identity", "migration.json", "server-ca.pem"];
/// A token a console's `agent.toml` can still hold when its hygiene failed.
pub const TOKEN: &str = "s57-legacy-enrollment-token-0123456789";
/// The legacy `agent.hostname`, which the generated configuration replaces.
pub const LEGACY_HOSTNAME: &str = "legacy-console-host";
/// The legacy `agent.id`, a placeholder the server-issued `agent_id` differs
/// from (§Data contracts §2): item 2.5 keeps this one in `agent.toml`.
pub const LEGACY_AGENT_ID: &str = "01934abc-def0-7000-89ab-000000000001";
pub const ENROLL_URL: &str = "https://enroll.example.test:8444";
pub const HEARTBEAT_URL: &str = "https://ingest.example.test:8443";
/// The enrolling account of the fixture: real SYSTEM, the one other SID.
pub const ENROLLING_SID: &str = "S-1-5-18";
/// `Environment.wait`, in place of the 30 seconds.
pub const WAIT: Duration = Duration::from_secs(1);

/// The prefixes of `Stop::Refused`'s lines.
pub const ARGUMENTS_LINE: &str = "cg-agent: invalid migration arguments: ";
pub const IDENTITY_LINE: &str = "cg-agent: identity error: ";
pub const TLS_LINE: &str = "cg-agent: tls: client configuration failed: migration: ";
pub const LOG_LINE: &str = "cg-agent: the service log cannot be written: ";
/// What each refusal before the commit says (the paragraph after item 2.6).
pub const IN_PLACE: &str = "the console installation is in place";
/// The names of `<P>` and the data root, which item 2.1 closes.
pub const DIRECTORIES: [&str; 2] = ["CyberGuard", "agent"];
/// What items 2.1 and 2.4 have restricted once item 2.4 is over.
pub const RESTRICTED: [&str; 7] = [
    "CyberGuard",
    "agent",
    "agent.toml",
    "server-ca.pem",
    "cert.pem",
    "key.dat",
    "identity.json",
];

/// A test that this machine cannot build skips here and fails in CI, where
/// a green run must mean it ran (GitHub Actions sets `CI`).
pub fn in_ci() -> bool {
    std::env::var_os("CI").is_some()
}

/// The current user's SID, read once.
pub fn sid() -> &'static str {
    static SID: OnceLock<String> = OnceLock::new();
    SID.get_or_init(user_sid)
}

/// The custody of the task: the current user in SYSTEM's place.
pub fn test_custody() -> MigrationCustody {
    static CUSTODY: OnceLock<MigrationCustody> = OnceLock::new();
    CUSTODY
        .get_or_init(|| {
            let owner = Principal::Sid(sid_bytes(&default_owner_sid()));
            let sid = sid();
            MigrationCustody {
                service: user(),
                owners: vec![user(), owner.clone()],
                trusted: vec![
                    user(),
                    owner.clone(),
                    Principal::System,
                    Principal::Administrators,
                ],
                configuration: Policy {
                    owners: vec![user(), owner],
                    allowed: vec![user(), Principal::Administrators],
                    protected: true,
                },
                identity: user_only(),
                closed_directory_sddl: format!("D:P(A;OICI;FA;;;{sid})(A;OICI;FA;;;BA)"),
                shared_file_sddl: format!("D:P(A;;FA;;;{sid})(A;;FA;;;BA)"),
                identity_directory_sddl: format!("D:P(A;OICI;FA;;;{sid})"),
                identity_file_sddl: format!("D:P(A;;FA;;;{sid})"),
            }
        })
        .clone()
}

/// A directory like `%ProgramData%`'s children: ordinary users may read,
/// and create entries, but neither change the DACL nor take ownership.
pub fn programdata_child_sddl() -> String {
    format!(
        "D:(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;{})(A;OICI;0x1200a9;;;BU)(A;CI;0x116;;;BU)",
        sid()
    )
}

/// An owner and DACL as a sorted, comparable text: a DACL's order and its
/// auto-inherited flag are not what a test asserts.
pub fn canonical(sddl: &str) -> String {
    let (head, aces) = sddl.split_once('(').unwrap_or((sddl, ""));
    let head = head.replace("D:PAI", "D:P").replace("D:AI", "D:");
    let mut entries: Vec<String> = format!("({aces}")
        .split_inclusive(')')
        .map(str::to_string)
        .filter(|entry| entry != "(")
        .collect();
    entries.sort();
    format!("{head}{}", entries.concat())
}

/// The object's owner and DACL as the file system stores them, comparable
/// with `with_owner`. `custody_fixtures::owner_and_dacl` will not do here:
/// `GetNamedSecurityInfoW` marks as inherited what matches the parent's DACL
/// at the time of the call, so what it reports of a child changes when the
/// parent's DACL does, and "propagates to no child" could not be told.
pub fn security(path: &Path) -> String {
    let name = wide(path);
    let information = OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
    let mut needed = 0u32;
    // SAFETY: a size query on a NUL-terminated name.
    unsafe {
        GetFileSecurityW(
            name.as_ptr(),
            information,
            std::ptr::null_mut(),
            0,
            &mut needed,
        )
    };
    let mut buffer = vec![0u64; (needed as usize).div_ceil(8).max(1)];
    // SAFETY: `buffer` holds at least `needed` bytes, u64-aligned.
    let ok = unsafe {
        GetFileSecurityW(
            name.as_ptr(),
            information,
            buffer.as_mut_ptr().cast(),
            (buffer.len() * 8) as u32,
            &mut needed,
        )
    };
    assert_ne!(ok, 0, "GetFileSecurityW {}", path.display());
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: a valid descriptor; `text` is LocalAlloc'd, freed below.
    let ok = unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            buffer.as_mut_ptr().cast(),
            SDDL_REVISION_1,
            information,
            &mut text,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(ok, 0, "render the security of {}", path.display());
    // SAFETY: a NUL-terminated string from the call, read then freed.
    let sddl = unsafe {
        let len = (0..).take_while(|&i| *text.add(i) != 0).count();
        let sddl = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
        LocalFree(text.cast());
        sddl
    };
    canonical(&sddl)
}

/// `dacl` under this process's default owner, the owner the unelevated
/// custody leaves on what it writes.
pub fn with_owner(dacl: &str) -> String {
    canonical(&normalized(&format!("O:{}{dacl}", default_owner_sid())))
}

/// `sddl` becomes the object's DACL, in place: its file ID stays.
pub fn set_dacl(path: &Path, sddl: &str) {
    set_security(path, sddl, DACL_SECURITY_INFORMATION);
}

/// `sid` becomes the object's owner, in place. A token can give an object to
/// its own user, or to a group it may own with, and to no one else.
pub fn set_owner(path: &Path, sid: &str) {
    set_security(path, &format!("O:{sid}"), OWNER_SECURITY_INFORMATION);
}

fn set_security(path: &Path, sddl: &str, information: u32) {
    let text: Vec<u16> = sddl.encode_utf16().chain(std::iter::once(0)).collect();
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: a NUL-terminated string; the descriptor is freed below.
    assert_ne!(
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        },
        0,
        "parse {sddl}"
    );
    let name = wide(path);
    // SAFETY: the name and the descriptor live for the call.
    let ok = unsafe { SetFileSecurityW(name.as_ptr(), information, descriptor) };
    // SAFETY: read before the descriptor is freed; allocated by the call above.
    let error = unsafe { GetLastError() };
    unsafe { LocalFree(descriptor.cast()) };
    assert_ne!(ok, 0, "SetFileSecurityW {}: {error}", path.display());
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// A junction at `link` to the directory `target`.
pub fn junction(link: &Path, target: &Path) {
    let status = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .stdout(std::process::Stdio::null())
        .status()
        .expect("run mklink");
    assert!(status.success(), "mklink /J {}", link.display());
}

/// `path` opened with `access` and `share`, kept open by the value.
pub fn hold(path: &Path, access: u32, share: u32) -> std::fs::File {
    std::fs::OpenOptions::new()
        .access_mode(access)
        .share_mode(share)
        .open(path)
        .unwrap_or_else(|e| panic!("hold {} open: {e}", path.display()))
}

pub const SHARE_ALL: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;
pub const ACCESS_READ: u32 = GENERIC_READ;
pub const ACCESS_WRITE: u32 = 0x4000_0000;
pub const ACCESS_DELETE: u32 = 0x0001_0000;

/// An object of a tree. `bytes` is `None` for a file this user cannot read,
/// and `security` for a reparse point, which is not followed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object {
    pub kind: &'static str,
    pub bytes: Option<Vec<u8>>,
    pub security: Option<String>,
}

pub type Tree = BTreeMap<String, Object>;

/// Each object under `root`, `root` itself as `.`, with its bytes, owner and
/// DACL; a reparse point is recorded and not followed.
pub fn tree(root: &Path) -> Tree {
    let mut objects = Tree::new();
    if std::fs::symlink_metadata(root).is_ok() {
        walk(root, ".", &mut objects);
    }
    objects
}

fn walk(path: &Path, name: &str, objects: &mut Tree) {
    let meta = std::fs::symlink_metadata(path)
        .unwrap_or_else(|e| panic!("inspect {}: {e}", path.display()));
    if meta.file_type().is_symlink() {
        objects.insert(
            name.to_string(),
            Object {
                kind: "reparse point",
                bytes: None,
                security: None,
            },
        );
        return;
    }
    if meta.is_dir() {
        objects.insert(
            name.to_string(),
            Object {
                kind: "directory",
                bytes: None,
                security: Some(security(path)),
            },
        );
        for child in entries(path) {
            let child_name = if name == "." {
                child.clone()
            } else {
                format!("{name}/{child}")
            };
            walk(&path.join(&child), &child_name, objects);
        }
        return;
    }
    objects.insert(
        name.to_string(),
        Object {
            kind: "file",
            bytes: std::fs::read(path).ok(),
            security: Some(security(path)),
        },
    );
}

/// The names `directory` holds, sorted; none when it is absent.
pub fn entries(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(directory)
        .map(|listing| {
            listing
                .map(|entry| {
                    entry
                        .expect("a directory entry")
                        .file_name()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// The two trees are the same, naming what differs without printing bytes.
pub fn assert_same_tree(before: &Tree, after: &Tree, what: &str) {
    let names: BTreeSet<&String> = before.keys().chain(after.keys()).collect();
    let differing: Vec<&str> = names
        .into_iter()
        .filter(|name| before.get(*name) != after.get(*name))
        .map(String::as_str)
        .collect();
    assert!(differing.is_empty(), "{what}: {differing:?} changed");
}

/// A fingerprint as the start arguments give it: over one certificate's DER.
pub fn fingerprint(der: &[u8]) -> String {
    let digest = Sha256::digest(der);
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("sha256:{hex}")
}

/// The fingerprints of a PEM document's certificates.
pub fn fingerprints_of(pem: &[u8]) -> BTreeSet<String> {
    CertificateDer::pem_slice_iter(pem)
        .map(|certificate| fingerprint(certificate.expect("a certificate").as_ref()))
        .collect()
}

/// The console's `agent.toml`, as the runbook writes it: paths in the data
/// root, with forward slashes, and both URLs. Each setting a generated
/// configuration keeps has a value of its own, none its default: `[tls]` and
/// `[envelope]` have one legal value, so they are only there.
pub fn console_toml(data_root: &Path) -> String {
    let root = forward(data_root);
    format!(
        "[server]\n\
         url = \"{ENROLL_URL}\"\n\
         heartbeat_url = \"{HEARTBEAT_URL}\"\n\
         trust_anchor_path = \"{root}/server-ca.pem\"\n\
         \n\
         [agent]\n\
         id = \"{LEGACY_AGENT_ID}\"\n\
         hostname = \"{LEGACY_HOSTNAME}\"\n\
         \n\
         [heartbeat]\n\
         interval_seconds = 17\n\
         request_timeout_seconds = 11\n\
         max_retries = 7\n\
         backoff_initial_ms = 130\n\
         backoff_factor = 1.5\n\
         backoff_max_ms = 1700\n\
         \n\
         [log]\n\
         level = \"debug\"\n\
         \n\
         [enrollment]\n\
         token = \"{TOKEN}\"\n\
         cert_path = \"{root}/cert.pem\"\n\
         key_path = \"{root}/key.dat\"\n\
         identity_path = \"{root}/identity.json\"\n\
         timeout_seconds = 23\n\
         max_retries = 9\n\
         backoff_initial_ms = 270\n\
         backoff_factor = 2.5\n\
         \n\
         [tls]\n\
         minimum_version = \"1.3\"\n\
         \n\
         [envelope]\n\
         canonical_form = \"JCS\"\n"
    )
}

/// `path` with forward slashes, as the console runbook spells it.
pub fn forward(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

/// A host under a temporary `%ProgramData%`.
pub struct Console {
    pub dir: tempfile::TempDir,
    pub paths: InstalledPaths,
    /// A test may change it: `prepare` reads it at each start.
    pub custody: MigrationCustody,
    pub now: DateTime<Utc>,
    pub pki: common::TlsTestPki,
    /// The `agent.toml` written.
    pub config_text: String,
}

impl Console {
    /// `<P>` and the data root as `New-Item` leaves them under
    /// `%ProgramData%`, and a console layout in the data root.
    pub fn new() -> Self {
        let sddl = programdata_child_sddl();
        Self::with_directories(&sddl, &sddl)
    }

    /// The same for another agent, enrolled with its own key.
    pub fn for_agent(agent_id: &str) -> Self {
        let console = Self::bare_for(agent_id);
        let sddl = programdata_child_sddl();
        console.make_directories(&sddl, &sddl);
        console.fill();
        console
    }

    /// A console layout under directories item 2.1 has closed: the data
    /// directories' check passes.
    pub fn closed() -> Self {
        let sddl = test_custody().closed_directory_sddl;
        Self::with_directories(&sddl, &sddl)
    }

    /// A console layout under directories with these DACLs.
    pub fn with_directories(product_root_sddl: &str, data_root_sddl: &str) -> Self {
        let console = Self::bare();
        console.make_directories(product_root_sddl, data_root_sddl);
        console.fill();
        console
    }

    /// A console layout in a data root whose case-sensitive lookup is on;
    /// `None` where Windows does not offer it (it can depend on an optional
    /// feature).
    pub fn case_sensitive() -> Option<Self> {
        let console = Self::bare();
        let sddl = programdata_child_sddl();
        console.make_directories(&sddl, &sddl);
        let status = std::process::Command::new("fsutil")
            .args(["file", "setCaseSensitiveInfo"])
            .arg(&console.paths.data_root)
            .arg("enable")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("run fsutil");
        if !status.success() {
            return None;
        }
        console.fill();
        Some(console)
    }

    /// No directory at all under `%ProgramData%`.
    pub fn bare() -> Self {
        Self::bare_for(common::TEST_AGENT_ID)
    }

    fn bare_for(agent_id: &str) -> Self {
        install_log_capture();
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = InstalledPaths::under(dir.path());
        let config_text = console_toml(&paths.data_root);
        Console {
            paths,
            dir,
            custody: test_custody(),
            now: Utc::now(),
            pki: common::generate_test_pki(agent_id),
            config_text,
        }
    }

    /// `<P>` and the data root with a fresh service's files: `identity\`
    /// in the data root, so no console layout.
    pub fn service_host(directories_sddl: &str) -> Self {
        let console = Self::bare();
        console.make_directories(directories_sddl, directories_sddl);
        let custody = &console.custody;
        for (path, text) in [
            (&console.paths.config, "[server]\n"),
            (&console.paths.trust_anchor, "anchor"),
        ] {
            custody::create_new_file(path, &custody.shared_file_sddl, text.as_bytes())
                .expect("a configuration file");
        }
        custody::ensure_directory(
            &console.paths.identity_dir,
            &custody.identity_directory_sddl,
            &custody.identity,
        )
        .expect("identity\\");
        for path in [
            &console.paths.cert,
            &console.paths.key,
            &console.paths.identity,
        ] {
            custody::create_new_file(path, &custody.identity_file_sddl, b"x")
                .expect("an identity file");
        }
        console
    }

    pub fn make_directories(&self, product_root_sddl: &str, data_root_sddl: &str) {
        let any = Policy {
            owners: vec![],
            allowed: vec![],
            protected: false,
        };
        custody::ensure_directory(&self.paths.product_root, product_root_sddl, &any).expect("<P>");
        custody::ensure_directory(&self.paths.data_root, data_root_sddl, &any)
            .expect("the data root");
    }

    /// `agent.toml` and `server-ca.pem` written plainly, and the identity
    /// written by the console's own code. The rest of each test rests on
    /// what this asserts.
    fn fill(&self) {
        self.write("agent.toml", self.config_text.as_bytes());
        self.write("server-ca.pem", self.pki.trust_anchor_pem.as_bytes());
        let config = cg_agent::config::load_from_str(&self.config_text).expect("the console's");
        cg_agent::identity::persist_identity(&config, &self.enrolled()).expect("persist");

        let mut expected: Vec<String> = FILES.iter().map(|name| name.to_string()).collect();
        expected.sort();
        assert_eq!(entries(&self.paths.data_root), expected, "the layout");
        assert_ne!(self.pki.agent_id, LEGACY_AGENT_ID, "two different ids");
        for name in IDENTITY_FILES {
            assert_eq!(
                security(&self.file(name)),
                with_owner(&format!("D:P(A;;FA;;;{})(A;;FA;;;SY)", sid())),
                "{name}: the console's protected DACL, this user and SYSTEM"
            );
        }
        assert!(
            !security(&self.file("agent.toml")).contains("D:P"),
            "agent.toml has no ACL of CyberGuard's"
        );
    }

    /// What enrollment returned, expiring long after the tests run.
    pub fn enrolled(&self) -> EnrolledIdentity {
        EnrolledIdentity {
            agent_id: self.pki.agent_id.clone(),
            client_certificate_pem: self.pki.client_cert_pem.clone(),
            issued_at: "2026-11-01T00:00:00Z".to_string(),
            expires_at: "2036-11-01T00:00:00Z".to_string(),
            secret_seed: zeroize::Zeroizing::new(self.pki.agent_seed),
            public_key: self.pki.agent_pubkey,
        }
    }

    /// A file of the console's data root.
    pub fn file(&self, name: &str) -> PathBuf {
        self.paths.data_root.join(name)
    }

    /// What it takes to make the staged identity this `other` console's, a
    /// set that is complete and consistent in itself, written in place.
    pub fn staged_identity_of(&self, other: &Console) -> Vec<(PathBuf, Vec<u8>)> {
        IDENTITY_FILES
            .iter()
            .map(|name| {
                let bytes = std::fs::read(other.file(name)).expect("read the other identity");
                (self.staged(&format!("identity/{name}")), bytes)
            })
            .collect()
    }

    /// A file of the staging directory.
    pub fn staged(&self, name: &str) -> PathBuf {
        self.paths.staging().join(name)
    }

    /// A file of the legacy directory.
    pub fn legacy(&self, name: &str) -> PathBuf {
        self.paths.legacy().join(name)
    }

    /// A path beside `<P>`, outside everything the migration touches.
    pub fn outside(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// Writes into the data root, keeping an existing file's security.
    pub fn write(&self, name: &str, bytes: &[u8]) {
        std::fs::write(self.file(name), bytes).expect("write a data root file");
    }

    /// The whole of the temporary `%ProgramData%`.
    pub fn whole(&self) -> Tree {
        tree(self.dir.path())
    }

    /// The data root's files, without the directory's own security.
    pub fn files(&self) -> Tree {
        let mut files = tree(&self.paths.data_root);
        files.remove(".");
        files
    }

    pub fn fingerprint(&self) -> String {
        fingerprints_of(self.pki.trust_anchor_pem.as_bytes())
            .into_iter()
            .next()
            .expect("the anchor holds a certificate")
    }

    pub fn key_fingerprint(&self) -> String {
        cg_agent::crypto::pubkey_fingerprint(&self.pki.agent_pubkey)
    }

    /// What the operator asks for: the choice and the anchor's fingerprint.
    pub fn request(&self, choice: Choice) -> Request {
        Request {
            choice,
            fingerprints: BTreeSet::from([self.fingerprint()]),
        }
    }

    pub fn environment<'a>(&self, watch: &'a dyn Watch) -> Environment<'a> {
        Environment {
            paths: self.paths.clone(),
            custody: self.custody.clone(),
            now: self.now,
            wait: WAIT,
            watch,
        }
    }

    /// One start, as the next start after any before it: `lock` takes the
    /// instance lock, at most once.
    pub fn start_with_lock(
        &self,
        request: Option<&Request>,
        script: &Script,
        lock: &mut dyn FnMut() -> Result<(), Stop>,
    ) -> Result<Prepared, Stop> {
        let mut calls = 0;
        let result = prepare(&self.environment(script), request, &mut || {
            calls += 1;
            lock()
        });
        assert!(calls <= 1, "the lock is taken at most once: {calls}");
        result
    }

    /// One start whose lock is free.
    pub fn start(&self, request: Option<&Request>, script: &Script) -> Result<Prepared, Stop> {
        self.start_with_lock(request, script, &mut || Ok(()))
    }

    /// A migrating start that commits, with nothing injected.
    pub fn migrate(&self, choice: Choice) -> Request {
        let request = self.request(choice);
        assert_eq!(
            self.start(Some(&request), &Script::new()),
            Ok(Prepared::Committed),
            "the migration commits"
        );
        request
    }

    /// A migrating start interrupted at `point`.
    pub fn crash_at(&self, point: Point, request: &Request) {
        let script = Script::new().crash(point);
        assert_eq!(
            self.start(Some(request), &script),
            Err(Stop::Crashed),
            "the start ends at {point:?}"
        );
        assert_eq!(
            script.seen().last(),
            Some(&point),
            "nothing after the crash ran"
        );
    }

    /// One pass over the old copies (item 5).
    pub fn copies(&self, script: &Script) -> Result<Copies, String> {
        copies(&self.environment(script))
    }

    /// The staging directory's `migration.json`.
    pub fn staged_record(&self) -> Record {
        read_record(&self.staged("migration.json"))
    }

    /// The data root's `migration.json`.
    pub fn record(&self) -> Record {
        read_record(&self.paths.record())
    }

    /// A record of this console's migration, written by hand.
    pub fn sample_record(&self, choice: Choice) -> Record {
        Record {
            choice,
            fingerprints: vec![self.fingerprint()],
            agent_id: self.pki.agent_id.clone(),
            key_fingerprint: self.key_fingerprint(),
            enrolling_account: Some(ENROLLING_SID.to_string()),
            files: FILES
                .iter()
                .map(|name| (name.to_string(), "0".to_string()))
                .collect(),
            reversal_pending: false,
            copies_not_reached: vec![],
        }
    }

    /// A directory that counts (item 4), the service's own.
    pub fn counting_directory(&self, path: &Path) {
        custody::ensure_directory(
            path,
            &self.custody.closed_directory_sddl,
            &self.custody.configuration,
        )
        .expect("a directory that counts");
    }

    /// A record that passes `agent.toml`'s policy, in `directory`.
    pub fn plant_record(&self, directory: &Path, choice: Choice) {
        custody::create_new_file(
            &directory.join("migration.json"),
            &self.custody.shared_file_sddl,
            &serde_json::to_vec(&self.sample_record(choice)).expect("a record"),
        )
        .expect("migration.json");
    }

    /// The layout after a commit: the service's data root beside the legacy
    /// directory, and no staging directory.
    pub fn assert_committed(&self) {
        assert_eq!(
            entries(&self.paths.data_root),
            ["agent.toml", "identity", "migration.json", "server-ca.pem"],
            "the migrated data root"
        );
        assert_eq!(
            entries(&self.paths.identity_dir),
            ["cert.pem", "identity.json", "key.dat"],
            "the migrated identity"
        );
        assert!(!self.paths.staging().exists(), "the staging directory left");
        assert!(self.paths.legacy().is_dir(), "the legacy directory");
        let custody = &self.custody;
        for path in [
            &self.paths.config,
            &self.paths.trust_anchor,
            &self.paths.record(),
        ] {
            custody::check(path, Kind::File, &custody.configuration).expect("its policy");
        }
        custody::check(&self.paths.identity_dir, Kind::Directory, &custody.identity)
            .expect("identity\\");
        for path in [&self.paths.cert, &self.paths.key, &self.paths.identity] {
            custody::check(path, Kind::File, &custody.identity).expect("an identity file");
        }
    }

    /// The console's files are in the data root, with their bytes; no legacy
    /// directory exists. `restricted` have item 2.4's security, the others
    /// the security of `before`.
    pub fn assert_console_in_place(&self, before: &Tree, restricted: &[&str]) {
        assert!(!self.paths.legacy().exists(), "no legacy directory");
        let after = self.files();
        let shared = with_owner(&self.custody.shared_file_sddl);
        for (name, object) in before {
            let now = after
                .get(name)
                .unwrap_or_else(|| panic!("{name} left the data root"));
            assert_eq!(now.bytes, object.bytes, "{name}: its bytes");
            let expected = if restricted.contains(&name.as_str()) {
                Some(&shared)
            } else {
                object.security.as_ref()
            };
            assert_eq!(now.security.as_ref(), expected, "{name}: its security");
        }
    }

    /// `<P>` and the data root as item 2.1 leaves them.
    pub fn assert_directories_closed(&self) {
        let closed = with_owner(&self.custody.closed_directory_sddl);
        assert_eq!(security(&self.paths.product_root), closed, "<P>");
        assert_eq!(security(&self.paths.data_root), closed, "the data root");
    }

    /// The configuration the migration generated.
    pub fn generated_config(&self) -> AgentConfig {
        let text = std::fs::read_to_string(&self.paths.config).expect("read agent.toml");
        cg_agent::config::load_from_str(&text).expect("the generated configuration")
    }

    /// The migrated identity, through the service's own check and load.
    pub fn load_identity(&self) -> Identity {
        let config = self.generated_config();
        let custody = InstalledCustody {
            identity_dir: self.paths.identity_dir.clone(),
            hygiene_temporary: self.paths.hygiene_temporary(),
            policy: self.custody.identity.clone(),
            config_policy: self.custody.configuration.clone(),
            directory_sddl: self.custody.identity_directory_sddl.clone(),
            file_sddl: self.custody.identity_file_sddl.clone(),
        };
        installed::check(&config, &custody).expect("the identity step's check");
        installed::load(&config).expect("the identity loads")
    }
}

/// `migration.json` at `path`.
pub fn read_record(path: &Path) -> Record {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_slice(&bytes).expect("migration.json parses")
}

type Hook = Box<dyn FnMut() + Send>;

enum Step {
    Fail,
    Crash,
    Run(Hook),
}

/// A `Watch` that records each point and injects at those it was told of.
/// A hook runs immediately before the operation of its point, as another
/// process would; a failure or a crash replaces the operation.
#[derive(Default)]
pub struct Script {
    seen: Mutex<Vec<Point>>,
    steps: Mutex<Vec<(Point, Step)>>,
}

impl Script {
    pub fn new() -> Self {
        Self::default()
    }

    fn step(self, point: Point, step: Step) -> Self {
        self.steps.lock().expect("steps").push((point, step));
        self
    }

    /// The operation of `point` fails as if Windows had refused it.
    pub fn fail(self, point: Point) -> Self {
        self.step(point, Step::Fail)
    }

    /// The start ends at `point`.
    pub fn crash(self, point: Point) -> Self {
        self.step(point, Step::Crash)
    }

    /// `hook` runs immediately before the operation of `point`.
    pub fn hook(self, point: Point, hook: impl FnMut() + Send + 'static) -> Self {
        self.step(point, Step::Run(Box::new(hook)))
    }

    /// Each point reached, in order.
    pub fn seen(&self) -> Vec<Point> {
        self.seen.lock().expect("seen").clone()
    }

    pub fn saw(&self, point: Point) -> bool {
        self.seen().contains(&point)
    }

    /// The names item 2.4 reached, in order.
    pub fn restrictions(&self) -> Vec<&'static str> {
        self.seen()
            .into_iter()
            .filter_map(|point| match point {
                Point::Restrict(name) => Some(name),
                _ => None,
            })
            .collect()
    }
}

impl Watch for Script {
    fn at(&self, point: Point) -> Result<(), Injected> {
        self.seen.lock().expect("seen").push(point);
        let mut steps = self.steps.lock().expect("steps");
        for (at, step) in steps.iter_mut() {
            if *at == point {
                match step {
                    Step::Fail => return Err(Injected::Failure("injected refusal".to_string())),
                    Step::Crash => return Err(Injected::Crash),
                    Step::Run(hook) => hook(),
                }
            }
        }
        Ok(())
    }
}

/// No line of a start carries a token, a SID or a PEM (the amendment's
/// preamble and item 7).
pub fn assert_clean(line: &str) {
    assert!(!line.contains(TOKEN), "a token: {line}");
    assert!(!line.contains("S-1-"), "a SID: {line}");
    assert!(!line.contains("BEGIN"), "a PEM: {line}");
}

/// The code and line of a refusal.
pub fn refused(result: Result<Prepared, Stop>) -> (u8, String) {
    match result {
        Err(Stop::Refused { code, line }) => {
            assert_clean(&line);
            (code, line)
        }
        other => panic!("a refusal was expected: {other:?}"),
    }
}

fn line_of(result: Result<Prepared, Stop>, code: u8, prefix: &str) -> String {
    let (actual, line) = refused(result);
    assert_eq!(actual, code, "{line}");
    assert!(line.starts_with(prefix), "{line}");
    line
}

/// Code 2 for the migration's arguments.
pub fn arguments_refusal(result: Result<Prepared, Stop>) -> String {
    line_of(result, 2, ARGUMENTS_LINE)
}

/// Code 2 for a configuration file, `path`.
pub fn config_refusal(result: Result<Prepared, Stop>, path: &Path) -> String {
    let prefix = format!("cg-agent: invalid config {}: ", path.display());
    line_of(result, 2, &prefix)
}

/// Code 5.
pub fn identity_refusal(result: Result<Prepared, Stop>) -> String {
    line_of(result, 5, IDENTITY_LINE)
}

/// Code 6.
pub fn tls_refusal(result: Result<Prepared, Stop>) -> String {
    line_of(result, 6, TLS_LINE)
}

/// Code 13.
pub fn log_refusal(result: Result<Prepared, Stop>) -> String {
    line_of(result, 13, LOG_LINE)
}

/// `word` stands in `line` as a name of its own: `agent` in `cg-agent:` or
/// `agent.toml` is not the data root, and neither `CyberGuard` nor `agent`
/// in a path that goes on past it, to `agent.toml` say, is that directory.
pub fn names(line: &str, word: &str) -> bool {
    line.match_indices(word).any(|(at, _)| {
        let before = line[..at].chars().next_back();
        let rest = &line[at + word.len()..];
        let mut after = rest.chars();
        let next = after.next();
        let left = !before.is_some_and(|c| c.is_alphanumeric() || c == '-');
        let right = match next {
            Some('.') => !after.next().is_some_and(char::is_alphanumeric),
            Some(c) => !c.is_alphanumeric() && !matches!(c, '-' | '\\' | '/'),
            None => true,
        };
        left && right
    })
}

/// The refusal says the console installation is in place and names what
/// items 2.1 and 2.4 restricted so far.
pub fn assert_in_place_line(line: &str, restricted: &[&str]) {
    assert!(line.contains(IN_PLACE), "{line}");
    for name in restricted {
        assert!(names(line, name), "{name} is not named: {line}");
    }
}

/// A reason names a path and the failed condition, never a file's content
/// (the amendment's preamble): none of `values` stands in the line.
pub fn assert_quotes_none(line: &str, values: &[&str]) {
    for value in values {
        assert!(!line.contains(value), "{value} is quoted: {line}");
    }
}

thread_local! {
    /// What `capture` is collecting on this thread.
    static CAPTURED: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
}

/// The subscriber's writer: this thread's `capture`, or nowhere.
struct ThreadLines;

impl std::io::Write for ThreadLines {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        CAPTURED.with_borrow_mut(|lines| {
            if let Some(lines) = lines {
                lines.extend_from_slice(data);
            }
        });
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// One subscriber for the test binary, in place before any start runs. A
/// subscriber per test (`tracing::subscriber::with_default`) loses lines
/// when tests run in parallel, as `cargo test --all` runs them: a thread
/// with no subscriber caches a callsite as disabled while another installs
/// its own, which shows only under that race.
fn install_log_capture() {
    static INSTALLED: Once = Once::new();
    INSTALLED.call_once(|| {
        let subscriber = tracing_subscriber::fmt()
            .with_writer(|| ThreadLines)
            .with_max_level(tracing::Level::TRACE)
            .json()
            .finish();
        tracing::subscriber::set_global_default(subscriber).expect("one subscriber per binary");
    });
}

/// `f`'s value and every line `f` logged on this thread, at every level.
pub fn capture<T>(f: impl FnOnce() -> T) -> (T, Vec<String>) {
    install_log_capture();
    CAPTURED.set(Some(Vec::new()));
    let value = f();
    let bytes = CAPTURED.take().expect("this thread's capture");
    let text = String::from_utf8_lossy(&bytes);
    (value, text.lines().map(str::to_string).collect())
}

/// A held opportunistic lock on a file or directory, released on drop. The
/// break Windows sends the holder is never acknowledged, so what it makes
/// wait waits until the value is dropped.
pub struct Oplock {
    handle: HANDLE,
    event: HANDLE,
    _overlapped: Box<OVERLAPPED>,
    _input: Box<REQUEST_OPLOCK_INPUT_BUFFER>,
    _output: Box<REQUEST_OPLOCK_OUTPUT_BUFFER>,
}

// SAFETY: the two handles and the buffers are owned by the value and used by
// one thread at a time.
unsafe impl Send for Oplock {}

impl Drop for Oplock {
    fn drop(&mut self) {
        // SAFETY: both handles are owned here. Closing the file completes
        // the pending request, which the buffers outlive.
        unsafe {
            CloseHandle(self.handle);
            WaitForSingleObject(self.event, 5000);
            CloseHandle(self.event);
        }
    }
}

/// A read-write-handle oplock: a read, security or delete open, a deletion
/// and a rename wait (checked on NTFS while the tests were written).
pub const OPLOCK_RWH: u32 =
    OPLOCK_LEVEL_CACHE_READ | OPLOCK_LEVEL_CACHE_WRITE | OPLOCK_LEVEL_CACHE_HANDLE;
/// A read-handle oplock: opens go through; a security change, a deletion
/// and a rename, of the object or of a directory above it, wait.
pub const OPLOCK_RH: u32 = OPLOCK_LEVEL_CACHE_READ | OPLOCK_LEVEL_CACHE_HANDLE;

/// Take a batch opportunistic lock on the file `path`: even an open for
/// attributes and security alone waits on it, which no other level makes
/// wait. An ordinary user can take one on a name it planted.
pub fn batch_oplock(path: &Path) -> Result<Oplock, u32> {
    request_oplock(path, None)
}

/// Take an opportunistic lock of `level` on `path`; the error is the Win32
/// code, 300 (`ERROR_OPLOCK_NOT_GRANTED`) when another handle rules it out.
pub fn oplock(path: &Path, level: u32) -> Result<Oplock, u32> {
    request_oplock(path, Some(level))
}

fn request_oplock(path: &Path, level: Option<u32>) -> Result<Oplock, u32> {
    let name = wide(path);
    // SAFETY: the name lives for the call; the handle is owned by the value
    // or closed below.
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            GENERIC_READ,
            SHARE_ALL,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_OVERLAPPED | FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        // SAFETY: reads the thread's last error.
        return Err(unsafe { GetLastError() });
    }
    let input = Box::new(REQUEST_OPLOCK_INPUT_BUFFER {
        StructureVersion: REQUEST_OPLOCK_CURRENT_VERSION as u16,
        StructureLength: std::mem::size_of::<REQUEST_OPLOCK_INPUT_BUFFER>() as u16,
        RequestedOplockLevel: level.unwrap_or(0),
        Flags: REQUEST_OPLOCK_INPUT_FLAG_REQUEST,
    });
    // SAFETY: all-zero is a valid value of these plain structures.
    let mut output: Box<REQUEST_OPLOCK_OUTPUT_BUFFER> = Box::new(unsafe { std::mem::zeroed() });
    output.StructureVersion = REQUEST_OPLOCK_CURRENT_VERSION as u16;
    output.StructureLength = std::mem::size_of::<REQUEST_OPLOCK_OUTPUT_BUFFER>() as u16;
    // SAFETY: as above.
    let mut overlapped: Box<OVERLAPPED> = Box::new(unsafe { std::mem::zeroed() });
    // SAFETY: a manual-reset event, closed on drop.
    let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
    overlapped.hEvent = event;
    // SAFETY: an open overlapped handle; the buffers are boxed so that they
    // stay where they are while the request is pending. A batch oplock
    // takes no buffers.
    let ok = unsafe {
        if level.is_some() {
            DeviceIoControl(
                handle,
                FSCTL_REQUEST_OPLOCK,
                &*input as *const REQUEST_OPLOCK_INPUT_BUFFER as *const core::ffi::c_void,
                std::mem::size_of::<REQUEST_OPLOCK_INPUT_BUFFER>() as u32,
                &mut *output as *mut REQUEST_OPLOCK_OUTPUT_BUFFER as *mut core::ffi::c_void,
                std::mem::size_of::<REQUEST_OPLOCK_OUTPUT_BUFFER>() as u32,
                std::ptr::null_mut(),
                &mut *overlapped,
            )
        } else {
            DeviceIoControl(
                handle,
                FSCTL_REQUEST_BATCH_OPLOCK,
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut *overlapped,
            )
        }
    };
    // SAFETY: reads the thread's last error.
    let error = unsafe { GetLastError() };
    if ok != 0 || error != ERROR_IO_PENDING {
        // SAFETY: the two handles opened above.
        unsafe {
            CloseHandle(handle);
            CloseHandle(event);
        }
        return Err(error);
    }
    Ok(Oplock {
        handle,
        event,
        _overlapped: overlapped,
        _input: input,
        _output: output,
    })
}
