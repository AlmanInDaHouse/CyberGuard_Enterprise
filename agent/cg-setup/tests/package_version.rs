//! The installed binary's version (SPEC-024 Amendment 2026-11-10, items 4
//! and 8): the downgrade (29); configure's check that the installed binary is
//! this package's, 45 when Windows holds a rename onto it for the next
//! restart and 40 otherwise; the renames read from literal
//! `PendingFileRenameOperations` values; and, on Windows, the version
//! resource read from copies of this test executable, one without a version
//! resource and ones given one.

use std::path::Path;

use cg_setup::codes;
use cg_setup::version::{
    check_downgrade, check_installed, is_this_packages, numeric, rename_destinations, renames_onto,
    InstalledVersion, SETUP_VERSION,
};

const BINARY: &str = r"C:\Program Files\CyberGuard\Agent\cg-agent.exe";

fn installed(file_version: [u16; 4], product_version: Option<&str>) -> InstalledVersion {
    InstalledVersion {
        file_version,
        product_version: product_version.map(str::to_string),
    }
}

/// A `REG_MULTI_SZ` value of these strings, each NUL-terminated, the list
/// ending with one more NUL.
fn multi_sz(strings: &[&str]) -> Vec<u16> {
    let mut value: Vec<u16> = strings
        .iter()
        .flat_map(|text| text.encode_utf16().chain(std::iter::once(0)))
        .collect();
    value.push(0);
    value
}

#[test]
fn package_version_the_setup_is_the_agents_version() {
    assert_eq!(SETUP_VERSION, cg_agent::AGENT_VERSION);
    assert!(numeric(SETUP_VERSION).is_some(), "{SETUP_VERSION}");
}

#[test]
fn package_version_the_numeric_version() {
    assert_eq!(numeric("1.2.3"), Some([1, 2, 3]));
    assert_eq!(numeric("0.1.0-dev"), Some([0, 1, 0]));
    assert_eq!(numeric("65535.0.65535"), Some([65535, 0, 65535]));
    for bad in [
        "",
        "1.2",
        "1.2.3.4",
        "1.2.x",
        "v1.2.3",
        "1.2.3-rc1",
        "65536.0.0",
        "1..3",
        " 1.2.3",
        "+1.2.3",
    ] {
        assert_eq!(numeric(bad), None, "{bad:?}");
    }
}

/// Item 4: a file version above `X.Y.Z.0` is 29; equal, below or absent is
/// none.
#[test]
fn package_version_a_downgrade_is_29() {
    let own = [1, 2, 3];
    for (file_version, refused) in [
        ([1, 2, 3, 0], false),
        ([1, 2, 2, 9], false),
        ([0, 9, 9, 9], false),
        ([1, 2, 3, 1], true),
        ([1, 2, 4, 0], true),
        ([1, 3, 0, 0], true),
        ([2, 0, 0, 0], true),
    ] {
        let result = check_downgrade(Some(&installed(file_version, Some("x"))), own);
        if refused {
            let refusal = result.expect_err(&format!("{file_version:?}"));
            assert_eq!(refusal.code, codes::DOWNGRADE, "{refusal:?}");
            assert!(refusal.line.starts_with("cg-setup: "), "{refusal:?}");
        } else {
            assert_eq!(result, Ok(()), "{file_version:?}");
        }
    }
    assert_eq!(check_downgrade(None, own), Ok(()), "no installed binary");
}

/// Item 8: this package's binary has file version `X.Y.Z.0` and
/// `ProductVersion` `cg-setup`'s own version; anything else is 45 with a
/// restart pending and 40 without.
#[test]
fn package_version_configure_checks_the_installed_binary() {
    let own = [1, 2, 3];
    assert!(is_this_packages(
        &installed([1, 2, 3, 0], Some("1.2.3")),
        own,
        "1.2.3"
    ));
    assert_eq!(
        check_installed(
            Some(&installed([1, 2, 3, 0], Some("1.2.3"))),
            own,
            "1.2.3",
            true
        ),
        Ok(()),
        "this package's binary, whatever is pending"
    );
    for found in [
        Some(installed([1, 2, 3, 1], Some("1.2.3"))),
        Some(installed([1, 2, 2, 0], Some("1.2.3"))),
        Some(installed([1, 2, 3, 0], Some("1.2.3-dev"))),
        Some(installed([1, 2, 3, 0], Some("1.2.4"))),
        Some(installed([1, 2, 3, 0], None)),
        None,
    ] {
        assert!(
            found
                .as_ref()
                .is_none_or(|found| !is_this_packages(found, own, "1.2.3")),
            "{found:?}"
        );
        for (pending, code) in [(true, codes::RESTART_PENDING), (false, codes::OWN_STATE)] {
            let refusal = check_installed(found.as_ref(), own, "1.2.3", pending)
                .expect_err(&format!("{found:?}"));
            assert_eq!(
                refusal.code, code,
                "{found:?}, pending {pending}: {refusal:?}"
            );
        }
    }
}

/// The renames of a `PendingFileRenameOperations` value: pairs of a source
/// and a destination, an empty destination a deletion, an empty source the
/// end of the list.
#[test]
fn package_version_the_pending_renames_are_read_in_pairs() {
    let value = multi_sz(&[
        r"\??\C:\Windows\Temp\a.tmp",
        "",
        r"\??\C:\Program Files\CyberGuard\Agent\cg-agent.exe.new",
        r"!\??\C:\Program Files\CyberGuard\Agent\cg-agent.exe",
        r"\??\C:\x",
        r"\??\C:\y",
    ]);
    assert_eq!(
        rename_destinations(&value),
        vec![
            r"!\??\C:\Program Files\CyberGuard\Agent\cg-agent.exe".to_string(),
            r"\??\C:\y".to_string(),
        ]
    );
    assert_eq!(rename_destinations(&[]), Vec::<String>::new());
    assert_eq!(rename_destinations(&[0, 0]), Vec::<String>::new());
    // What follows an empty source is not read.
    let mut value = multi_sz(&[r"\??\C:\a", r"\??\C:\b"]);
    value.extend(multi_sz(&["", r"\??\C:\c", r"\??\C:\d"]));
    assert_eq!(rename_destinations(&value), vec![r"\??\C:\b".to_string()]);
}

/// A destination is the installed binary without its `!` and `\??\`
/// prefixes, equal but for ASCII case.
#[test]
fn package_version_a_rename_onto_the_binary() {
    let binary = Path::new(BINARY);
    for destination in [
        BINARY.to_string(),
        format!(r"\??\{BINARY}"),
        format!(r"!\??\{BINARY}"),
        format!("!{BINARY}"),
        format!(r"\??\{}", BINARY.to_ascii_uppercase()),
    ] {
        assert!(
            renames_onto(std::slice::from_ref(&destination), binary),
            "{destination}"
        );
    }
    for destination in [
        format!("{BINARY}.new"),
        format!(r"\\?\{BINARY}"),
        format!(r"\??\!{BINARY}"),
        format!(r" \??\{BINARY}"),
        r"\??\C:\Program Files\CyberGuard\Agent\cg-setup.exe".to_string(),
    ] {
        assert!(
            !renames_onto(std::slice::from_ref(&destination), binary),
            "{destination}"
        );
    }
    assert!(!renames_onto(&[], binary));
    // A non-ASCII letter's case is not folded.
    let binary = Path::new(r"C:\Agént\cg-agent.exe");
    assert!(!renames_onto(
        &[r"\??\C:\AGÉNT\CG-AGENT.EXE".to_string()],
        binary
    ));
    assert!(renames_onto(
        &[r"\??\C:\AGéNT\CG-AGENT.EXE".to_string()],
        binary
    ));
}

#[cfg(windows)]
mod windows {
    use std::path::{Path, PathBuf};

    use cg_setup::codes;
    use cg_setup::version::{
        configure_version, downgrade, pending_renames, read_installed, InstalledVersion,
    };
    use windows_sys::Win32::System::LibraryLoader::{
        BeginUpdateResourceW, EndUpdateResourceW, UpdateResourceW,
    };

    const LANGUAGE: u16 = 0x0409;
    const CODE_PAGE: u16 = 1200;
    /// `RT_VERSION`, `MAKEINTRESOURCE(16)`.
    const RT_VERSION: usize = 16;

    enum Value<'a> {
        None,
        Binary(&'a [u8]),
        Text(&'a str),
    }

    fn pad4(out: &mut Vec<u8>) {
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
    }

    /// One block of a VS_VERSIONINFO tree, as the agent's build writes it.
    fn block(key: &str, value: Value, children: &[Vec<u8>]) -> Vec<u8> {
        let mut out = vec![0u8; 6];
        for unit in key.encode_utf16().chain(std::iter::once(0)) {
            out.extend_from_slice(&unit.to_le_bytes());
        }
        pad4(&mut out);
        let (value_length, kind): (u16, u16) = match value {
            Value::None => (0, 1),
            Value::Binary(bytes) => {
                out.extend_from_slice(bytes);
                (bytes.len() as u16, 0)
            }
            Value::Text(text) => {
                let units: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
                for unit in &units {
                    out.extend_from_slice(&unit.to_le_bytes());
                }
                (units.len() as u16, 1)
            }
        };
        for child in children {
            pad4(&mut out);
            out.extend_from_slice(child);
        }
        let length = out.len() as u16;
        out[0..2].copy_from_slice(&length.to_le_bytes());
        out[2..4].copy_from_slice(&value_length.to_le_bytes());
        out[4..6].copy_from_slice(&kind.to_le_bytes());
        out
    }

    /// A version resource with this file version and, if given, this
    /// `ProductVersion` string. Its numeric product version is another one,
    /// 9.9.9.9, so that a reader of the wrong field is seen.
    fn version_info(numbers: [u16; 4], product_version: Option<&str>) -> Vec<u8> {
        version_info_signed(0xFEEF_04BD, numbers, product_version)
    }

    fn version_info_signed(
        signature: u32,
        numbers: [u16; 4],
        product_version: Option<&str>,
    ) -> Vec<u8> {
        let [a, b, c, d] = numbers.map(u32::from);
        let (ms, ls) = ((a << 16) | b, (c << 16) | d);
        let fixed: Vec<u8> = [
            signature,
            0x0001_0000,
            ms,
            ls,
            0x0009_0009,
            0x0009_0009,
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
        let mut strings = vec![block("FileDescription", Value::Text("a test"), &[])];
        if let Some(text) = product_version {
            strings.push(block("ProductVersion", Value::Text(text), &[]));
        }
        let table = block(
            &format!("{LANGUAGE:04X}{CODE_PAGE:04X}"),
            Value::None,
            &strings,
        );
        let string_file_info = block("StringFileInfo", Value::None, &[table]);
        let mut translation = LANGUAGE.to_le_bytes().to_vec();
        translation.extend_from_slice(&CODE_PAGE.to_le_bytes());
        let var = block("Translation", Value::Binary(&translation), &[]);
        let var_file_info = block("VarFileInfo", Value::None, &[var]);
        block(
            "VS_VERSION_INFO",
            Value::Binary(&fixed),
            &[string_file_info, var_file_info],
        )
    }

    fn wide(path: &Path) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt as _;
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    /// A copy of this test executable, which carries no version resource.
    fn copy_of_this_executable(directory: &Path, name: &str) -> PathBuf {
        let path = directory.join(name);
        std::fs::copy(std::env::current_exe().expect("this executable"), &path)
            .expect("copy this executable");
        path
    }

    fn set_version(path: &Path, resource: &[u8]) {
        let wpath = wide(path);
        // SAFETY: the path lives for the call; the update handle is ended
        // below, and the resource lives until then.
        unsafe {
            let update = BeginUpdateResourceW(wpath.as_ptr(), 0);
            assert!(!update.is_null(), "BeginUpdateResourceW");
            let ok = UpdateResourceW(
                update,
                std::ptr::without_provenance(RT_VERSION),
                std::ptr::without_provenance(1),
                LANGUAGE,
                resource.as_ptr().cast(),
                resource.len() as u32,
            );
            assert!(ok != 0, "UpdateResourceW");
            assert!(EndUpdateResourceW(update, 0) != 0, "EndUpdateResourceW");
        }
    }

    /// Item 4: no installed binary is none; one whose version cannot be read
    /// is 40; one with a version resource gives its file version and its
    /// `ProductVersion`.
    #[test]
    fn package_version_the_installed_binarys_version_resource() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        assert_eq!(
            read_installed(&directory.path().join("cg-agent.exe")),
            Ok(None),
            "no installed binary"
        );
        assert_eq!(
            read_installed(&directory.path().join("absent").join("cg-agent.exe")),
            Ok(None),
            "no directory either"
        );

        let text = directory.path().join("text.exe");
        std::fs::write(&text, "not a portable executable").expect("write");
        let unversioned = copy_of_this_executable(directory.path(), "unversioned.exe");
        for path in [&text, &unversioned] {
            let refusal = read_installed(path).expect_err(&path.display().to_string());
            assert_eq!(refusal.code, codes::OWN_STATE, "{refusal:?}");
            assert!(refusal.line.contains("cannot be read"), "{refusal:?}");
        }

        let versioned = copy_of_this_executable(directory.path(), "versioned.exe");
        set_version(&versioned, &version_info([1, 2, 3, 4], Some("1.2.3")));
        assert_eq!(
            read_installed(&versioned),
            Ok(Some(InstalledVersion {
                file_version: [1, 2, 3, 4],
                product_version: Some("1.2.3".to_string()),
            }))
        );

        let no_product = copy_of_this_executable(directory.path(), "no-product.exe");
        set_version(&no_product, &version_info([65535, 0, 7, 0], None));
        assert_eq!(
            read_installed(&no_product),
            Ok(Some(InstalledVersion {
                file_version: [65535, 0, 7, 0],
                product_version: None,
            }))
        );

        let unsigned = copy_of_this_executable(directory.path(), "unsigned.exe");
        set_version(
            &unsigned,
            &version_info_signed(0x1234_5678, [1, 2, 3, 0], Some("1.2.3")),
        );
        let refusal = read_installed(&unsigned).expect_err("a fixed part without its signature");
        assert_eq!(refusal.code, codes::OWN_STATE, "{refusal:?}");
    }

    /// Item 4 on a binary: none, a downgrade (29), and a version that cannot
    /// be read (40).
    #[test]
    fn package_version_preflights_downgrade_check() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let own = [1, 2, 3];
        assert_eq!(
            downgrade(&directory.path().join("cg-agent.exe"), own),
            Ok(())
        );
        let same = copy_of_this_executable(directory.path(), "same.exe");
        set_version(&same, &version_info([1, 2, 3, 0], Some("1.2.3")));
        assert_eq!(downgrade(&same, own), Ok(()));
        let newer = copy_of_this_executable(directory.path(), "newer.exe");
        set_version(&newer, &version_info([1, 2, 3, 1], Some("1.2.3")));
        assert_eq!(
            downgrade(&newer, own).map_err(|r| r.code),
            Err(codes::DOWNGRADE)
        );
        let unversioned = copy_of_this_executable(directory.path(), "unversioned.exe");
        assert_eq!(
            downgrade(&unversioned, own).map_err(|r| r.code),
            Err(codes::OWN_STATE)
        );
    }

    /// Item 8 on a binary, with no rename held onto these temporary paths:
    /// this package's passes; another version, none or one that cannot be
    /// read is 40.
    #[test]
    fn package_version_configures_check_of_the_binary() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let own = [1, 2, 3];
        let this = copy_of_this_executable(directory.path(), "this.exe");
        set_version(&this, &version_info([1, 2, 3, 0], Some("1.2.3")));
        assert_eq!(configure_version(&this, own, "1.2.3"), Ok(()));
        let other = copy_of_this_executable(directory.path(), "other.exe");
        set_version(&other, &version_info([1, 2, 3, 0], Some("1.2.4")));
        let unversioned = copy_of_this_executable(directory.path(), "unversioned.exe");
        for path in [other, unversioned, directory.path().join("absent.exe")] {
            assert_eq!(
                configure_version(&path, own, "1.2.3").map_err(|r| r.code),
                Err(codes::OWN_STATE),
                "{}",
                path.display()
            );
        }
    }

    /// The agent's own version resource, as its build writes it, reads back
    /// as `X.Y.Z.0` and its version string, when the build left it beside
    /// this test (a `cargo test --all` build).
    #[test]
    fn package_version_the_agents_build_is_this_packages() {
        let agent = std::env::current_exe()
            .expect("this executable")
            .parent()
            .and_then(Path::parent)
            .expect("target/<profile>")
            .join("cg-agent.exe");
        if !agent.exists() {
            eprintln!("{} is not built; nothing to read", agent.display());
            return;
        }
        let found = read_installed(&agent).expect("readable").expect("present");
        let own = cg_setup::version::numeric(cg_setup::version::SETUP_VERSION).expect("numeric");
        assert!(
            cg_setup::version::is_this_packages(&found, own, cg_setup::version::SETUP_VERSION),
            "{found:?}"
        );
    }

    /// The renames Windows holds are readable unelevated; on a host with none
    /// the list is empty.
    #[test]
    fn package_version_the_pending_renames_are_readable() {
        let (_, unreadable) = pending_renames();
        assert!(
            unreadable.is_empty(),
            "the Session Manager's values: {unreadable:?}"
        );
    }
}

/// Item 9: a refusal is one line; the installed binary's `ProductVersion` in
/// it is escaped and bounded.
#[test]
fn package_version_a_refusal_quoting_the_binary_is_one_line() {
    let hostile = format!("1.2.3\r\n{}", "x".repeat(70_000));
    for pending in [true, false] {
        let refusal = check_installed(
            Some(&installed([1, 2, 3, 0], Some(&hostile))),
            [1, 2, 3],
            "1.2.3",
            pending,
        )
        .expect_err("not this package's");
        assert!(
            !refusal.line.contains('\n') && !refusal.line.contains('\r'),
            "{:?}",
            &refusal.line[..200]
        );
        assert!(refusal.line.len() < 600, "{} bytes", refusal.line.len());
    }
}
