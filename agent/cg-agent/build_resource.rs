//! The version resource's encoding, shared by the build scripts of
//! `cg-agent.exe` (`build.rs`) and of `cg-setup.exe`
//! (`../cg-setup/build.rs`), which include this file as a module: the
//! version from the build-only input of `src/version.rs` (SPEC-024 Amendment
//! 2026-11-01, point 2), and a compiled resource file with one VERSIONINFO
//! and, for the agent alone, one message table (the Amendment 2026-11-09,
//! item 4; the Amendment 2026-11-10, item 9).
//!
//! Its numeric file and product versions are `X.Y.Z.0`; its `FileVersion`
//! and `ProductVersion` strings are the input's `X.Y.Z`, or `<Cargo package
//! version>-dev` without it, whose numbers are Cargo's. A malformed input
//! yields no resource, and `version.rs` refuses it with its own message. A
//! well-formed one with a component above 65535, which a PE version field
//! cannot hold, fails the build. Its file flags are the pre-release flag in
//! a development build, the `-dev` one, and none in a release build
//! (SPEC-024 Amendment 2026-11-14, point 8, #140). No crate, no external
//! tool.

use std::env;

/// The resource's language and code page: U.S. English, UTF-16.
const LANGUAGE: u16 = 0x0409;
const CODE_PAGE: u16 = 1200;
/// `RT_VERSION`.
const RT_VERSION: u16 = 16;
/// `RT_MESSAGETABLE`.
const RT_MESSAGETABLE: u16 = 11;
/// `VS_FF_PRERELEASE`, in the file flags of a development build.
const VS_FF_PRERELEASE: u32 = 0x0000_0002;

/// The build's version: its text, its numbers, and whether it is a
/// development build, the `<Cargo package version>-dev` one, without the
/// input.
pub struct Version {
    pub text: String,
    pub numbers: [u16; 3],
    pub development: bool,
}

/// What a build script writes: whether to write anything, and the version.
/// `None` off a Windows MSVC target, and for a malformed input. Each call
/// tells cargo to rerun the script when the input is set, changed or removed.
pub fn version() -> Option<Version> {
    println!("cargo:rerun-if-env-changed=CG_RELEASE_VERSION");
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let abi = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if os != "windows" || abi != "msvc" {
        return None;
    }

    let cargo_version = env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION");
    let (text, components, development) = match env::var("CG_RELEASE_VERSION") {
        Ok(release) => match release_components(&release) {
            Some(components) => (release, components, false),
            // version.rs refuses it, with its own message.
            None => return None,
        },
        Err(_) => {
            let core = cargo_version.split(['-', '+']).next().unwrap_or_default();
            let components = release_components(core)
                .unwrap_or_else(|| panic!("Cargo's version {cargo_version} is not X.Y.Z"));
            (format!("{cargo_version}-dev"), components, true)
        }
    };
    let numbers = components.map(|c| {
        c.parse::<u16>().unwrap_or_else(|_| {
            panic!("version component {c} is above 65535, which a PE version field cannot hold")
        })
    });
    Some(Version {
        text,
        numbers,
        development,
    })
}

/// The three components of `X.Y.Z`, each `0` or ASCII digits without a
/// leading zero, as `version.rs` admits them; `None` for any other text.
fn release_components(s: &str) -> Option<[String; 3]> {
    let mut parts = s.split('.');
    let components = [parts.next()?, parts.next()?, parts.next()?];
    if parts.next().is_some() {
        return None;
    }
    components
        .iter()
        .all(|c| {
            !c.is_empty()
                && c.bytes().all(|b| b.is_ascii_digit())
                && (*c == "0" || !c.starts_with('0'))
        })
        .then(|| components.map(str::to_string))
}

/// The executable's own strings in its VERSIONINFO.
pub struct Names {
    pub file_description: &'static str,
    pub original_filename: &'static str,
}

/// A `.res` file: the empty resource every one begins with, then the
/// VERSIONINFO and, with `messages`, a message table whose entries
/// `messages.0` to `messages.1` are each their one insertion string.
pub fn resource_file(version: &Version, names: &Names, messages: Option<(u32, u32)>) -> Vec<u8> {
    let mut out = Vec::new();
    resource_header(&mut out, 0, 0, 0, 0, 0);
    let data = version_info(version, names);
    resource_header(&mut out, data.len() as u32, RT_VERSION, 1, 0x0030, LANGUAGE);
    out.extend_from_slice(&data);
    pad4(&mut out);
    if let Some((first, last)) = messages {
        let table = message_table(first, last);
        resource_header(
            &mut out,
            table.len() as u32,
            RT_MESSAGETABLE,
            1,
            0x0030,
            LANGUAGE,
        );
        out.extend_from_slice(&table);
        pad4(&mut out);
    }
    out
}

/// A RESOURCEHEADER with a numeric type and name: 32 bytes.
fn resource_header(
    out: &mut Vec<u8>,
    data_size: u32,
    type_id: u16,
    name_id: u16,
    memory_flags: u16,
    language: u16,
) {
    out.extend_from_slice(&data_size.to_le_bytes());
    out.extend_from_slice(&32u32.to_le_bytes());
    out.extend_from_slice(&0xFFFFu16.to_le_bytes());
    out.extend_from_slice(&type_id.to_le_bytes());
    out.extend_from_slice(&0xFFFFu16.to_le_bytes());
    out.extend_from_slice(&name_id.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // DataVersion
    out.extend_from_slice(&memory_flags.to_le_bytes());
    out.extend_from_slice(&language.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // Version
    out.extend_from_slice(&0u32.to_le_bytes()); // Characteristics
}

/// The MESSAGE_RESOURCE_DATA: one block of the entries `first` to `last`,
/// each of which is its one insertion string, in UTF-16.
fn message_table(first: u32, last: u32) -> Vec<u8> {
    let text: Vec<u8> = "%1\0".encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut entry = vec![0u8; 4];
    entry.extend_from_slice(&text);
    pad4(&mut entry);
    let length = entry.len() as u16;
    entry[0..2].copy_from_slice(&length.to_le_bytes());
    entry[2..4].copy_from_slice(&1u16.to_le_bytes()); // Flags: Unicode

    let mut out = Vec::new();
    out.extend_from_slice(&1u32.to_le_bytes()); // NumberOfBlocks
    out.extend_from_slice(&first.to_le_bytes());
    out.extend_from_slice(&last.to_le_bytes());
    out.extend_from_slice(&16u32.to_le_bytes()); // OffsetToEntries
    for _ in first..=last {
        out.extend_from_slice(&entry);
    }
    out
}

/// The VS_VERSIONINFO tree.
fn version_info(version: &Version, names: &Names) -> Vec<u8> {
    let text = version.text.as_str();
    let [x, y, z] = version.numbers.map(u32::from);
    // The Amendment 2026-11-14, point 8: a development build reads as a
    // pre-release; the mask already admits the flag.
    let flags = if version.development {
        VS_FF_PRERELEASE
    } else {
        0
    };
    let ms = (x << 16) | y;
    let ls = z << 16;
    let fixed: Vec<u8> = [
        0xFEEF_04BD, // dwSignature
        0x0001_0000, // dwStrucVersion
        ms,          // dwFileVersionMS
        ls,          // dwFileVersionLS
        ms,          // dwProductVersionMS
        ls,          // dwProductVersionLS
        0x0000_003F, // dwFileFlagsMask
        flags,       // dwFileFlags
        0x0004_0004, // dwFileOS: VOS_NT_WINDOWS32
        1,           // dwFileType: VFT_APP
        0,           // dwFileSubtype
        0,           // dwFileDateMS
        0,           // dwFileDateLS
    ]
    .iter()
    .flat_map(|v: &u32| v.to_le_bytes())
    .collect();

    let strings: Vec<Vec<u8>> = [
        ("FileDescription", names.file_description),
        ("FileVersion", text),
        ("OriginalFilename", names.original_filename),
        ("ProductName", "CyberGuard Agent"),
        ("ProductVersion", text),
    ]
    .iter()
    .map(|(key, value)| block(key, Value::Text(value), &[]))
    .collect();
    let table_key = format!("{LANGUAGE:04X}{CODE_PAGE:04X}");
    let table = block(&table_key, Value::None, &strings);
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

enum Value<'a> {
    None,
    Binary(&'a [u8]),
    Text(&'a str),
}

/// One block of the tree: `wLength`, `wValueLength`, `wType`, the key, its
/// padding, the value, then each child at a 32-bit boundary. `wLength`
/// leaves out the padding after the last child, which the parent adds.
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

fn pad4(out: &mut Vec<u8>) {
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}
