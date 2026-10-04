//! Kernel device path → Win32 path translation (SPEC-005 §Operational §3,
//! SPEC-017 §Operational §5).
//!
//! ETW's ProcessStart (Launch) reports a process image as a kernel device
//! path (`\Device\HarddiskVolume3\Windows\System32\cmd.exe`); ProcessStop
//! (Terminate) carries only the base name (`cmd.exe`), which no rule
//! below matches and which is returned as is. The agent builds a
//! device-prefix → drive-letter map once at startup with
//! `QueryDosDeviceW` (Windows only) and applies it when an event is
//! rendered, never in the dispatch callback:
//!
//! - UNC first: `\Device\Mup\server\share\x` and `\??\UNC\server\share\x`
//!   become `\\server\share\x`.
//! - Otherwise the longest device prefix that ends at a path separator
//!   is replaced by its drive (`C:`); `\Device\HarddiskVolume1` does not
//!   match `\Device\HarddiskVolume10\…`.
//! - Anything else (junctions, mount points, volumes mounted after
//!   startup) is returned verbatim, recognisable by its `\Device\` prefix.
//!
//! Prefixes match ASCII case-insensitively (NT object names are not
//! case-sensitive); the rest of the path is never altered.

use std::collections::HashSet;

/// The UNC prefixes, rewritten to `\\` (SPEC-005 §Operational §3).
const UNC_PREFIXES: [&str; 2] = ["\\Device\\Mup\\", "\\??\\UNC\\"];

/// Device-prefix → drive map, longest prefix first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DevicePathMap {
    entries: Vec<(String, String)>,
}

impl DevicePathMap {
    /// A map with no drive prefixes (only the UNC rule applies).
    pub fn empty() -> Self {
        Self::default()
    }

    /// Build a map from `(device prefix, drive)` pairs, e.g.
    /// `("\Device\HarddiskVolume3", "C:")`. A trailing separator on a
    /// prefix is ignored.
    pub fn from_pairs<I, D, L>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (D, L)>,
        D: Into<String>,
        L: Into<String>,
    {
        let mut entries: Vec<(String, String)> = pairs
            .into_iter()
            .map(|(device, drive)| {
                let device: String = device.into();
                (device.trim_end_matches('\\').to_string(), drive.into())
            })
            .filter(|(device, _)| !device.is_empty())
            .collect();
        // Longest prefix first; stable, so the first pair wins a tie.
        entries.sort_by_key(|(device, _)| std::cmp::Reverse(device.len()));
        Self { entries }
    }

    /// The `(device prefix, drive)` pairs, longest prefix first.
    pub fn entries(&self) -> &[(String, String)] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Translate one ETW image path to its Win32 form, or return it
    /// verbatim when no rule applies.
    pub fn translate(&self, path: &str) -> String {
        for unc in UNC_PREFIXES {
            if let Some(rest) = strip_prefix_ignore_ascii_case(path, unc) {
                return format!("\\\\{rest}");
            }
        }
        for (device, drive) in &self.entries {
            if let Some(rest) = strip_prefix_ignore_ascii_case(path, device) {
                if rest.is_empty() || rest.starts_with('\\') {
                    return format!("{drive}{rest}");
                }
            }
        }
        path.to_string()
    }

    /// Query every drive letter's device with `QueryDosDeviceW` (built
    /// once at agent startup; volumes mounted later are not seen,
    /// SPEC-005 NFR-005-007). Only `\Device\…` targets are kept.
    #[cfg(windows)]
    pub fn from_system() -> Self {
        use windows_sys::Win32::Storage::FileSystem::QueryDosDeviceW;

        let mut pairs = Vec::new();
        for letter in b'A'..=b'Z' {
            let drive = format!("{}:", letter as char);
            let name: Vec<u16> = drive.encode_utf16().chain(std::iter::once(0)).collect();
            let mut buffer = vec![0u16; 1024];
            // SAFETY: `name` is NUL-terminated; `buffer` is writable for
            // `buffer.len()` u16s, the size passed as ucchMax.
            let written =
                unsafe { QueryDosDeviceW(name.as_ptr(), buffer.as_mut_ptr(), buffer.len() as u32) };
            if written == 0 {
                continue; // no mapping for this letter
            }
            // The result is a list of NUL-terminated strings; the first is
            // the current mapping.
            let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
            let target = String::from_utf16_lossy(&buffer[..end]);
            if target.starts_with("\\Device\\") {
                pairs.push((target, drive));
            }
        }
        Self::from_pairs(pairs)
    }
}

/// `path` without `prefix`, matching the prefix ASCII case-insensitively.
fn strip_prefix_ignore_ascii_case<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    let head = path.get(..prefix.len())?;
    if head.eq_ignore_ascii_case(prefix) {
        Some(&path[prefix.len()..])
    } else {
        None
    }
}

/// Logs, once per distinct device prefix, a path the map left verbatim
/// (SPEC-005 §Failure modes: `debug`, throttled per prefix).
#[derive(Debug, Default)]
pub struct UnresolvedPathLog {
    seen: HashSet<String>,
}

impl UnresolvedPathLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Note one rendered path: `original` is the ETW value, `rendered`
    /// what the map returned.
    pub fn note(&mut self, original: &str, rendered: &str) {
        if original != rendered || !original.starts_with("\\Device\\") {
            return;
        }
        // `\Device\<name>` — the first two components.
        let prefix: String = original
            .splitn(4, '\\')
            .take(3)
            .collect::<Vec<_>>()
            .join("\\");
        if self.seen.insert(prefix.clone()) {
            tracing::debug!(
                device_prefix = %prefix,
                "image path left in device form (no drive mapping)"
            );
        }
    }
}
