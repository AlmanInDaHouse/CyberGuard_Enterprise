//! The agent's product version (SPEC-024 §Data contracts §1 and its
//! Amendment 2026-10-31, debt #131).
//!
//! A release build is given the exact `X.Y.Z` of its `vX.Y.Z` tag through
//! the build-only input `CG_RELEASE_VERSION`; a build without it reports
//! `<Cargo package version>-dev`. The input is read when the crate is
//! compiled (`option_env!`), so cargo rebuilds it when the input is set,
//! changed or removed, and the running agent reads no environment variable
//! for its version. A malformed or empty input fails the build. On a
//! Windows MSVC target `build.rs` writes the same version into the executable's version resource,
//! and refuses a component above 65535, which a PE version field cannot hold
//! (SPEC-024 Amendment 2026-11-01, point 2). MSI's tighter limits are checked
//! by the release's version check (`.github/scripts/release-version.mjs`)
//! and, with the package, by packaging (install_ac_015).
//! The envelope and CGES schema versions are independent of this one.

/// The build input, as the build was given it.
const RELEASE_INPUT: Option<&str> = option_env!("CG_RELEASE_VERSION");

/// The version the CLI, enrollment, both heartbeat paths and the lifecycle
/// log report.
pub const AGENT_VERSION: &str = match RELEASE_INPUT {
    Some(v) => v,
    None => concat!(env!("CARGO_PKG_VERSION"), "-dev"),
};

const _: () = assert!(
    match RELEASE_INPUT {
        Some(v) => is_release_version(v),
        None => true,
    },
    "CG_RELEASE_VERSION must be X.Y.Z: three decimal numbers without leading zeros, \
     with no prefix, suffix or whitespace"
);

/// True if `s` is `X.Y.Z`: three SemVer numeric identifiers (ASCII digits,
/// `0` or no leading zero) joined by dots, nothing before or after.
pub const fn is_release_version(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    let mut dots = 0;
    let mut digits = 0;
    let mut leading_zero = false;
    while i < b.len() {
        let c = b[i];
        if c == b'.' {
            if digits == 0 || dots == 2 {
                return false;
            }
            dots += 1;
            digits = 0;
            leading_zero = false;
        } else if c.is_ascii_digit() {
            if leading_zero {
                return false;
            }
            leading_zero = digits == 0 && c == b'0';
            digits += 1;
        } else {
            return false;
        }
        i += 1;
    }
    dots == 2 && digits > 0
}

#[cfg(test)]
mod tests {
    use super::is_release_version;

    #[test]
    fn accepts_x_y_z() {
        for v in [
            "0.0.0",
            "0.1.0",
            "1.2.3",
            "10.20.30",
            "255.255.65535",
            "300.0.0",
        ] {
            assert!(is_release_version(v), "{v}");
        }
    }

    #[test]
    fn rejects_everything_else() {
        for v in [
            "",
            "1",
            "1.2",
            "1.2.3.4",
            "v1.2.3",
            "V1.2.3",
            "01.2.3",
            "1.02.3",
            "1.2.03",
            "00.1.2",
            "1.2.3-rc.1",
            "1.2.3+build",
            " 1.2.3",
            "1.2.3 ",
            "1.2.3\n",
            "1..3",
            ".1.2",
            "1.2.",
            "a.b.c",
            "1.2.x",
            "１.2.3",
        ] {
            assert!(!is_release_version(v), "{v:?}");
        }
    }
}
