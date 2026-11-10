//! What the setup's tests share: a bootstrap file's text, a trust anchor, and
//! on Windows a temporary host with the current user where the service's
//! custody names SYSTEM, as cg-agent's tests have it.
#![allow(dead_code)]

use std::collections::BTreeSet;

/// The token every bootstrap here carries: no line may hold it.
pub const TOKEN: &str = "s60-setup-enrollment-token-QUJDREVGR0g";
pub const ENROLL_URL: &str = "https://enroll.example.test:8444";
pub const HEARTBEAT_URL: &str = "https://ingest.example.test:8443";
pub const HOSTNAME: &str = "setup-test-host";

/// A certificate and its fingerprint.
pub struct Certificate {
    pub pem: String,
    pub der: Vec<u8>,
    pub fingerprint: String,
}

/// A new self-signed certificate.
pub fn certificate(name: &str) -> Certificate {
    let certified = rcgen::generate_simple_self_signed(vec![name.to_string()]).expect("rcgen");
    let der = certified.cert.der().to_vec();
    Certificate {
        pem: certified.cert.pem(),
        fingerprint: cg_agent::service::anchor::fingerprint(&der),
        der,
    }
}

/// A TOML basic string.
pub fn quoted(value: &str) -> String {
    format!("{value:?}")
}

/// A bootstrap file's text: its five keys, the anchor's source at `anchor`.
pub fn bootstrap_text(anchor: &str, fingerprints: &[&str]) -> String {
    let list: Vec<String> = fingerprints.iter().map(|f| quoted(f)).collect();
    format!(
        "[server]\nurl = {}\nheartbeat_url = {}\ntrust_anchor_path = {}\n\n\
         [enrollment]\ntoken = {}\n\n[setup]\ntrust_anchor_sha256 = [{}]\n",
        quoted(ENROLL_URL),
        quoted(HEARTBEAT_URL),
        quoted(anchor),
        quoted(TOKEN),
        list.join(", ")
    )
}

pub fn set(fingerprints: &[&str]) -> BTreeSet<String> {
    fingerprints.iter().map(|f| f.to_string()).collect()
}

/// No line carries the token or a value of the file beyond what is named.
pub fn assert_clean(line: &str) {
    assert!(!line.contains(TOKEN), "a line holds the token: {line}");
    assert!(
        !line.contains("QUJDREVGR0g"),
        "a line holds part of the token: {line}"
    );
}

#[cfg(windows)]
#[allow(unused_imports)]
pub use windows::*;

#[cfg(windows)]
mod windows {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::sync::OnceLock;
    use std::time::Duration;

    use cg_agent::custody::{self, Policy, Principal};
    use cg_agent::service::InstalledPaths;
    use cg_setup::preflight::Host;
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE};
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{
        GetLengthSid, GetTokenInformation, TokenElevation, TokenOwner, TokenUser, TOKEN_ELEVATION,
        TOKEN_OWNER, TOKEN_QUERY, TOKEN_USER,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    pub const WAIT: Duration = Duration::from_secs(1);

    fn token_information(class: i32) -> Vec<u64> {
        let mut token: HANDLE = std::ptr::null_mut();
        // SAFETY: the current process's pseudo-handle; the token is closed below.
        assert_ne!(
            unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) },
            0,
            "OpenProcessToken"
        );
        let mut needed = 0u32;
        // SAFETY: a size query.
        unsafe { GetTokenInformation(token, class, std::ptr::null_mut(), 0, &mut needed) };
        let mut buffer = vec![0u64; (needed as usize).div_ceil(8).max(1)];
        // SAFETY: `buffer` holds at least `needed` bytes, u64-aligned.
        let ok = unsafe {
            GetTokenInformation(
                token,
                class,
                buffer.as_mut_ptr().cast(),
                (buffer.len() * 8) as u32,
                &mut needed,
            )
        };
        // SAFETY: the token opened above.
        unsafe { CloseHandle(token) };
        assert_ne!(ok, 0, "GetTokenInformation {class}");
        buffer
    }

    fn sid_bytes(sid: *mut core::ffi::c_void) -> Vec<u8> {
        // SAFETY: a valid SID inside a live buffer.
        unsafe { std::slice::from_raw_parts(sid as *const u8, GetLengthSid(sid) as usize).to_vec() }
    }

    fn sid_text(sid: &[u8]) -> String {
        let mut text: *mut u16 = std::ptr::null_mut();
        // SAFETY: a whole SID; `text` is LocalAlloc'd, freed below.
        assert_ne!(
            unsafe { ConvertSidToStringSidW(sid.as_ptr() as *mut _, &mut text) },
            0,
            "ConvertSidToStringSidW"
        );
        // SAFETY: a NUL-terminated string from the call, read then freed.
        unsafe {
            let len = (0..).take_while(|&i| *text.add(i) != 0).count();
            let rendered = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
            LocalFree(text.cast());
            rendered
        }
    }

    /// The current user's SID, binary.
    pub fn user() -> &'static [u8] {
        static USER: OnceLock<Vec<u8>> = OnceLock::new();
        USER.get_or_init(|| {
            let buffer = token_information(TokenUser);
            // SAFETY: the buffer holds a TOKEN_USER.
            sid_bytes(unsafe { (*(buffer.as_ptr() as *const TOKEN_USER)).User.Sid })
        })
    }

    /// The owner this token gives what it creates, binary.
    pub fn default_owner() -> &'static [u8] {
        static OWNER: OnceLock<Vec<u8>> = OnceLock::new();
        OWNER.get_or_init(|| {
            let buffer = token_information(TokenOwner);
            // SAFETY: the buffer holds a TOKEN_OWNER.
            sid_bytes(unsafe { (*(buffer.as_ptr() as *const TOKEN_OWNER)).Owner })
        })
    }

    /// The current user's SID as text.
    pub fn sid() -> String {
        sid_text(user())
    }

    /// Whether this token is elevated, as Windows reports it.
    pub fn token_is_elevated() -> bool {
        let mut token: HANDLE = std::ptr::null_mut();
        // SAFETY: the current process's pseudo-handle; the token is closed below.
        assert_ne!(
            unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) },
            0,
            "OpenProcessToken"
        );
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut needed = 0u32;
        // SAFETY: a fixed-size class, whose buffer must be its exact size.
        let ok = unsafe {
            GetTokenInformation(
                token,
                TokenElevation,
                std::ptr::addr_of_mut!(elevation).cast(),
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut needed,
            )
        };
        // SAFETY: the token opened above.
        unsafe { CloseHandle(token) };
        assert_ne!(ok, 0, "GetTokenInformation TokenElevation");
        elevation.TokenIsElevated != 0
    }

    /// SYSTEM and Administrators, with the current user in SYSTEM's place.
    pub fn test_policy(protected: bool) -> Policy {
        Policy {
            owners: vec![
                Principal::Sid(user().to_vec()),
                Principal::Sid(default_owner().to_vec()),
            ],
            allowed: vec![Principal::Sid(user().to_vec()), Principal::Administrators],
            protected,
        }
    }

    /// The DACL item 3 gives `<P>` and the data root, with the current user
    /// in SYSTEM's place.
    pub fn closed_directory_sddl() -> String {
        format!("D:P(A;OICI;FA;;;{})(A;OICI;FA;;;BA)", sid())
    }

    /// A directory that ordinary users may create in.
    pub fn open_directory_sddl() -> String {
        format!(
            "D:P(A;OICI;FA;;;{})(A;OICI;FA;;;BA)(A;OICI;0x1200a9;;;BU)(A;CI;0x116;;;BU)",
            sid()
        )
    }

    /// A file only the current user and Administrators may reach.
    pub fn private_file_sddl() -> String {
        format!("D:P(A;;FA;;;{})(A;;FA;;;BA)", sid())
    }

    /// A temporary host: `<P>` and the data root under a temporary
    /// `%ProgramData%`, and a directory beside it for the operator's files.
    pub struct Root {
        pub dir: tempfile::TempDir,
        pub paths: InstalledPaths,
        pub host: Host,
        /// Where the operator keeps the bootstrap file and the anchor.
        pub operator: PathBuf,
    }

    impl Root {
        /// Nothing under `%ProgramData%`.
        pub fn bare() -> Self {
            let dir = tempfile::tempdir().expect("tempdir");
            let paths = InstalledPaths::under(&dir.path().join("ProgramData"));
            std::fs::create_dir(dir.path().join("ProgramData")).expect("ProgramData");
            let operator = dir.path().join("operator");
            std::fs::create_dir(&operator).expect("operator");
            let host = Host {
                paths: paths.clone(),
                directories: test_policy(false),
                bootstrap: test_policy(false),
                hostname: super::HOSTNAME.to_string(),
                wait: WAIT,
            };
            Root {
                dir,
                paths,
                host,
                operator,
            }
        }

        /// `<P>` and the data root as item 3 creates them.
        pub fn empty() -> Self {
            let root = Self::bare();
            root.directories(&closed_directory_sddl());
            root
        }

        pub fn directories(&self, sddl: &str) {
            let any = Policy {
                owners: vec![],
                allowed: vec![],
                protected: false,
            };
            custody::ensure_directory(&self.paths.product_root, sddl, &any).expect("<P>");
            custody::ensure_directory(&self.paths.data_root, sddl, &any).expect("data root");
        }

        /// A file in the data root.
        pub fn touch(&self, name: &str) {
            std::fs::write(self.paths.data_root.join(name), b"x").expect("a file");
        }

        /// The anchor's source, with these certificates.
        pub fn anchor(&self, certificates: &[&super::Certificate]) -> PathBuf {
            let path = self.operator.join("server-ca.pem");
            let pem: String = certificates.iter().map(|c| c.pem.as_str()).collect();
            std::fs::write(&path, pem).expect("the anchor");
            path
        }

        /// A bootstrap file of `text`, created with `sddl` as its security.
        pub fn bootstrap_with(&self, name: &str, sddl: &str, text: &str) -> PathBuf {
            let path = self.operator.join(name);
            custody::create_new_file(&path, sddl, text.as_bytes()).expect("the bootstrap file");
            path
        }

        /// A protected bootstrap file of `text`.
        pub fn bootstrap(&self, text: &str) -> PathBuf {
            self.bootstrap_with("bootstrap.toml", &private_file_sddl(), text)
        }

        /// Every name under the temporary root, with its size: what a
        /// read-only step must leave as it was.
        pub fn tree(&self) -> BTreeMap<String, u64> {
            let mut tree = BTreeMap::new();
            walk(self.dir.path(), self.dir.path(), &mut tree);
            tree
        }
    }

    fn walk(base: &Path, path: &Path, tree: &mut BTreeMap<String, u64>) {
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path).expect("metadata");
            let name = path.strip_prefix(base).expect("under base");
            tree.insert(name.display().to_string(), metadata.len());
            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                walk(base, &path, tree);
            }
        }
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
}
