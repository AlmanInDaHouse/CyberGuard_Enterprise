//! The Windows version resource of `cg-agent.exe` (SPEC-024 Amendment
//! 2026-11-01, point 2), from the build-only input of `src/version.rs`.
//!
//! On a Windows MSVC target this writes `version.res`, a compiled resource
//! file with one VERSIONINFO, into `OUT_DIR`, and gives it to the linker of
//! the `cg-agent` binary, which takes a `.res` file as an input. Its numeric
//! file and product versions are `X.Y.Z.0`; its `FileVersion` and
//! `ProductVersion` strings are the version `cg-agent --version` prints:
//! the input's `X.Y.Z`, or `<Cargo package version>-dev` without it, whose
//! numbers are Cargo's. A malformed input writes nothing here, and
//! `version.rs` refuses it with its own message. A well-formed one with a
//! component above 65535, which a PE version field cannot hold, fails the
//! build. No crate, no external tool; cargo reruns this when the input is
//! set, changed or removed.

use std::env;
use std::path::PathBuf;

/// The resource's language and code page: U.S. English, UTF-16.
const LANGUAGE: u16 = 0x0409;
const CODE_PAGE: u16 = 1200;
/// `RT_VERSION`.
const RT_VERSION: u16 = 16;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CG_RELEASE_VERSION");
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let abi = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if os != "windows" || abi != "msvc" {
        return;
    }

    let cargo_version = env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION");
    let (text, components) = match env::var("CG_RELEASE_VERSION") {
        Ok(release) => match release_components(&release) {
            Some(components) => (release, components),
            // version.rs refuses it, with its own message.
            None => return,
        },
        Err(_) => {
            let core = cargo_version.split(['-', '+']).next().unwrap_or_default();
            let components = release_components(core)
                .unwrap_or_else(|| panic!("Cargo's version {cargo_version} is not X.Y.Z"));
            (format!("{cargo_version}-dev"), components)
        }
    };
    let numbers = components.map(|c| {
        c.parse::<u16>().unwrap_or_else(|_| {
            panic!("version component {c} is above 65535, which a PE version field cannot hold")
        })
    });

    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("version.res");
    std::fs::write(&out, resource_file(numbers, &text)).expect("write version.res");
    println!("cargo:rustc-link-arg-bin=cg-agent={}", out.display());
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

/// A `.res` file: the empty resource every one begins with, then the
/// VERSIONINFO.
fn resource_file(numbers: [u16; 3], text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    resource_header(&mut out, 0, 0, 0, 0, 0);
    let data = version_info(numbers, text);
    resource_header(&mut out, data.len() as u32, RT_VERSION, 1, 0x0030, LANGUAGE);
    out.extend_from_slice(&data);
    pad4(&mut out);
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

/// The VS_VERSIONINFO tree.
fn version_info(numbers: [u16; 3], text: &str) -> Vec<u8> {
    let [x, y, z] = numbers.map(u32::from);
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
        0,           // dwFileFlags
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
        ("FileDescription", "CyberGuard endpoint agent"),
        ("FileVersion", text),
        ("OriginalFilename", "cg-agent.exe"),
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
