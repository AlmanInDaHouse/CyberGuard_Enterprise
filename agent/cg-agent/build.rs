//! The Windows resources of `cg-agent.exe`: the version (SPEC-024 Amendment
//! 2026-11-01, point 2), from the build-only input of `src/version.rs`, and
//! the message table of the Application log's entries (SPEC-024 Amendment
//! 2026-11-09, item 4).
//!
//! On a Windows MSVC target this writes `version.res`, a compiled resource
//! file with one VERSIONINFO and one message table, into `OUT_DIR`, and gives
//! it to the linker of the `cg-agent` binary, which takes a `.res` file as an
//! input. The encoding is `build_resource.rs`, which `cg-setup`'s build
//! shares. Its numeric
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

#[path = "build_resource.rs"]
mod build_resource;

/// The Application-log entries, `EVENT_RUNNING` to `EVENT_DEADLINE` of
/// `src/service/mod.rs`; `tests/package_executable.rs` holds them equal.
const FIRST_ENTRY: u32 = 1000;
const LAST_ENTRY: u32 = 1003;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build_resource.rs");
    let Some((text, numbers)) = build_resource::version() else {
        return;
    };
    let names = build_resource::Names {
        file_description: "CyberGuard endpoint agent",
        original_filename: "cg-agent.exe",
    };
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("version.res");
    std::fs::write(
        &out,
        build_resource::resource_file(numbers, &text, &names, Some((FIRST_ENTRY, LAST_ENTRY))),
    )
    .expect("write version.res");
    println!("cargo:rustc-link-arg-bin=cg-agent={}", out.display());
}
