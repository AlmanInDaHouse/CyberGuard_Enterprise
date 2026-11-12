//! The Windows version resource of `cg-setup.exe` (SPEC-024 Amendment
//! 2026-11-10, item 9): the agent's (the Amendment 2026-11-01, point 2),
//! through the agent's own encoding, with its own file name and no message
//! table. Its version is the agent's build-time input, so that `cg-setup
//! --version` and the resource agree with `cg_agent::AGENT_VERSION`; without
//! the input, `<Cargo package version>-dev` is cg-setup's own, which a test
//! holds equal to the agent's. Its link also sets the image's
//! `DependentLoadFlags` to the system directory alone (the Amendment
//! 2026-11-12 (second), point 10).

use std::env;
use std::path::PathBuf;

#[path = "../cg-agent/build_resource.rs"]
mod build_resource;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../cg-agent/build_resource.rs");
    // The Amendment 2026-11-12 (second), point 10: Burn runs cg-setup elevated
    // from its package cache, with the system's temporary folder as the
    // working directory, so the loader takes the system libraries it imports
    // from the system directory alone (LOAD_LIBRARY_SEARCH_SYSTEM32), with or
    // without a version.
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let abi = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if os == "windows" && abi == "msvc" {
        println!("cargo:rustc-link-arg-bin=cg-setup=/DEPENDENTLOADFLAG:0x800");
    }
    let Some((text, numbers)) = build_resource::version() else {
        return;
    };
    let names = build_resource::Names {
        file_description: "CyberGuard agent setup",
        original_filename: "cg-setup.exe",
    };
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("version.res");
    std::fs::write(
        &out,
        build_resource::resource_file(numbers, &text, &names, None),
    )
    .expect("write version.res");
    println!("cargo:rustc-link-arg-bin=cg-setup={}", out.display());
}
