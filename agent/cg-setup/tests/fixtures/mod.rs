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

    /// A test that this machine cannot assert skips that part and asserts it
    /// in CI, where a green run must mean it ran (GitHub Actions sets `CI`).
    pub fn in_ci() -> bool {
        std::env::var_os("CI").is_some()
    }

    /// An impersonation token, closed on drop.
    pub struct Token(pub HANDLE);

    impl Drop for Token {
        fn drop(&mut self) {
            // SAFETY: a token handle this value owns.
            unsafe { CloseHandle(self.0) };
        }
    }

    fn process_token() -> Token {
        use windows_sys::Win32::Security::TOKEN_DUPLICATE;
        let mut token: HANDLE = std::ptr::null_mut();
        // SAFETY: the current process's pseudo-handle and an out-value.
        assert_ne!(
            unsafe {
                OpenProcessToken(
                    GetCurrentProcess(),
                    TOKEN_QUERY | TOKEN_DUPLICATE,
                    &mut token,
                )
            },
            0,
            "OpenProcessToken"
        );
        Token(token)
    }

    fn impersonation(token: &Token) -> Token {
        use windows_sys::Win32::Security::{DuplicateToken, SecurityIdentification};
        let mut duplicate: HANDLE = std::ptr::null_mut();
        // SAFETY: a token opened for duplication and an out-value.
        assert_ne!(
            unsafe { DuplicateToken(token.0, SecurityIdentification, &mut duplicate) },
            0,
            "DuplicateToken"
        );
        Token(duplicate)
    }

    /// This process's token, as an impersonation token.
    pub fn own_token() -> Token {
        impersonation(&process_token())
    }

    /// This process's token with Administrators for deny only, as an
    /// administrator's filtered token holds it, as an impersonation token.
    pub fn filtered_token() -> Token {
        use windows_sys::Win32::Security::{
            CreateRestrictedToken, CreateWellKnownSid, WinBuiltinAdministratorsSid,
            SECURITY_MAX_SID_SIZE, SID_AND_ATTRIBUTES,
        };
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
        let process = process_token();
        let mut restricted: HANDLE = std::ptr::null_mut();
        // SAFETY: a token opened for duplication, one SID to disable, and an
        // out-value.
        assert_ne!(
            unsafe {
                CreateRestrictedToken(
                    process.0,
                    0,
                    1,
                    &disable,
                    0,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    &mut restricted,
                )
            },
            0,
            "CreateRestrictedToken"
        );
        impersonation(&Token(restricted))
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
                hostname: Some(super::HOSTNAME.to_string()),
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

    /// A copy of this test executable at `path`, with a version resource of
    /// file version `numbers` and `ProductVersion` `product_version`, as the
    /// agent's build writes one: an installed binary for the verbs' version
    /// checks. The copy runs as this executable does.
    pub fn versioned_copy(path: &Path, numbers: [u16; 4], product_version: &str) {
        use windows_sys::Win32::System::LibraryLoader::{
            BeginUpdateResourceW, EndUpdateResourceW, UpdateResourceW,
        };
        std::fs::copy(std::env::current_exe().expect("this executable"), path)
            .expect("copy this executable");
        let resource = version_resource(numbers, product_version);
        let wpath: Vec<u16> = path
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // SAFETY: the path lives for the call; the update handle is ended
        // below, and the resource lives until then.
        unsafe {
            let update = BeginUpdateResourceW(wpath.as_ptr(), 0);
            assert!(!update.is_null(), "BeginUpdateResourceW");
            let ok = UpdateResourceW(
                update,
                // RT_VERSION, MAKEINTRESOURCE(16), name 1, U.S. English.
                std::ptr::without_provenance(16),
                std::ptr::without_provenance(1),
                0x0409,
                resource.as_ptr().cast(),
                resource.len() as u32,
            );
            assert!(ok != 0, "UpdateResourceW");
            assert!(EndUpdateResourceW(update, 0) != 0, "EndUpdateResourceW");
        }
    }

    fn version_resource(numbers: [u16; 4], product_version: &str) -> Vec<u8> {
        fn pad4(out: &mut Vec<u8>) {
            while !out.len().is_multiple_of(4) {
                out.push(0);
            }
        }
        fn block(key: &str, value: Option<&[u8]>, text: bool, children: &[Vec<u8>]) -> Vec<u8> {
            let mut out = vec![0u8; 6];
            for unit in key.encode_utf16().chain(Some(0)) {
                out.extend_from_slice(&unit.to_le_bytes());
            }
            pad4(&mut out);
            let length = value.map_or(0, |bytes| {
                out.extend_from_slice(bytes);
                if text {
                    bytes.len() / 2
                } else {
                    bytes.len()
                }
            });
            for child in children {
                pad4(&mut out);
                out.extend_from_slice(child);
            }
            let total = out.len() as u16;
            out[0..2].copy_from_slice(&total.to_le_bytes());
            out[2..4].copy_from_slice(&(length as u16).to_le_bytes());
            out[4..6].copy_from_slice(&u16::from(text || value.is_none()).to_le_bytes());
            out
        }
        let utf16 = |text: &str| -> Vec<u8> {
            text.encode_utf16()
                .chain(Some(0))
                .flat_map(u16::to_le_bytes)
                .collect()
        };
        let [a, b, c, d] = numbers.map(u32::from);
        let (ms, ls) = ((a << 16) | b, (c << 16) | d);
        let fixed: Vec<u8> = [
            0xFEEF_04BD,
            0x0001_0000,
            ms,
            ls,
            ms,
            ls,
            0x0000_003F,
            0,
            0x0004_0004,
            1,
            0,
            0,
            0,
        ]
        .iter()
        .flat_map(|v: &u32| v.to_le_bytes())
        .collect();
        let product = utf16(product_version);
        let strings = [block("ProductVersion", Some(&product), true, &[])];
        let table = block("040904B0", None, true, &strings);
        let string_file_info = block("StringFileInfo", None, true, &[table]);
        let translation = [0x09u8, 0x04, 0xB0, 0x04];
        let var = block("Translation", Some(&translation), false, &[]);
        let var_file_info = block("VarFileInfo", None, true, &[var]);
        block(
            "VS_VERSION_INFO",
            Some(&fixed),
            false,
            &[string_file_info, var_file_info],
        )
    }

    /// The `DependentLoadFlags` of the PE32+ image at `path`, the flags the
    /// loader applies to the DLLs it loads for the image's imports, from the
    /// load configuration directory; `None` without that directory or with
    /// one too short to hold the field.
    pub fn dependent_load_flags(path: &Path) -> Option<u16> {
        let bytes = std::fs::read(path).expect("read the executable");
        let u16_at = |at: usize| u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap());
        let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        let pe = u32_at(0x3C);
        assert_eq!(&bytes[pe..pe + 4], b"PE\0\0", "the PE signature");
        let count = usize::from(u16_at(pe + 6));
        let optional = pe + 24;
        assert_eq!(u16_at(optional), 0x20B, "a PE32+ image");
        let sections = optional + usize::from(u16_at(pe + 20));
        // IMAGE_DIRECTORY_ENTRY_LOAD_CONFIG, the eleventh data directory.
        let rva = u32_at(optional + 112 + 10 * 8);
        if rva == 0 {
            return None;
        }
        let at = (0..count)
            .map(|i| sections + i * 40)
            .find_map(|section| {
                let start = u32_at(section + 12);
                let size = u32_at(section + 8).max(u32_at(section + 16));
                (start..start + size)
                    .contains(&rva)
                    .then(|| rva - start + u32_at(section + 20))
            })
            .unwrap_or_else(|| panic!("RVA {rva:#x} is in no section"));
        // IMAGE_LOAD_CONFIG_DIRECTORY64: its own size first, then
        // DependentLoadFlags at offset 78.
        (u32_at(at) >= 80).then(|| u16_at(at + 78))
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    /// The version resource of `path`, as GetFileVersionInfoExW reads it, if
    /// it has one.
    fn version_block(path: &Path) -> Option<Vec<u8>> {
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileVersionInfoExW, GetFileVersionInfoSizeExW, FILE_VER_GET_NEUTRAL,
        };
        let wpath = wide(&path.to_string_lossy());
        let mut ignored = 0u32;
        // SAFETY: a NUL-terminated path.
        let size = unsafe {
            GetFileVersionInfoSizeExW(FILE_VER_GET_NEUTRAL, wpath.as_ptr(), &mut ignored)
        };
        if size == 0 {
            return None;
        }
        let mut block = vec![0u8; size as usize];
        // SAFETY: the buffer holds `size` bytes.
        if unsafe {
            GetFileVersionInfoExW(
                FILE_VER_GET_NEUTRAL,
                wpath.as_ptr(),
                0,
                size,
                block.as_mut_ptr().cast(),
            )
        } == 0
        {
            return None;
        }
        Some(block)
    }

    /// The file flags' mask and the file flags of the fixed file info of the
    /// version resource of `path`, if it has one.
    pub fn file_flags(path: &Path) -> Option<(u32, u32)> {
        use windows_sys::Win32::Storage::FileSystem::{VerQueryValueW, VS_FIXEDFILEINFO};
        let block = version_block(path)?;
        let root = wide("\\");
        let mut pointer: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut length = 0u32;
        // SAFETY: the block GetFileVersionInfoExW wrote; the fixed file info
        // lies in it, `length` bytes long, read without assuming its
        // alignment.
        unsafe {
            if VerQueryValueW(
                block.as_ptr().cast(),
                root.as_ptr(),
                &mut pointer,
                &mut length,
            ) == 0
                || pointer.is_null()
                || (length as usize) < std::mem::size_of::<VS_FIXEDFILEINFO>()
            {
                return None;
            }
            let fixed = std::ptr::read_unaligned(pointer as *const VS_FIXEDFILEINFO);
            Some((fixed.dwFileFlagsMask, fixed.dwFileFlags))
        }
    }

    /// The string `key` of the U.S. English, UTF-16 table of the version
    /// resource of `path`, if it has one.
    pub fn version_string(path: &Path, key: &str) -> Option<String> {
        use windows_sys::Win32::Storage::FileSystem::VerQueryValueW;
        let block = version_block(path)?;
        let sub_block = wide(&format!("\\StringFileInfo\\040904B0\\{key}"));
        let mut pointer: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut length = 0u32;
        // SAFETY: the block GetFileVersionInfoExW wrote; the value lies in it,
        // `length` characters long, its terminator included.
        unsafe {
            if VerQueryValueW(
                block.as_ptr().cast(),
                sub_block.as_ptr(),
                &mut pointer,
                &mut length,
            ) == 0
                || pointer.is_null()
            {
                return None;
            }
            let units = std::slice::from_raw_parts(pointer as *const u16, length as usize);
            let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
            Some(String::from_utf16_lossy(&units[..end]))
        }
    }
}
