//! SPEC-024 second Amendment 2026-11-01, item 2: `--service` runs only
//! when the service control manager started the process. Launched from a
//! console (or on a platform without an SCM) the agent exits with code 11
//! and its line, without running the console path and before it reads its
//! configuration.

use std::process::Command;

#[test]
fn install_ac_007_service_without_the_scm_exits_11_before_the_config() {
    let missing = std::env::temp_dir().join("cg-agent-install-ac-007-no-such-config.toml");
    let output = Command::new(env!("CARGO_BIN_EXE_cg-agent"))
        .arg("--service")
        .arg("--config")
        .arg(&missing)
        .output()
        .expect("launch the agent binary");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(11),
        "--service without the SCM is code 11; stderr: {stderr}"
    );
    assert!(
        stderr.contains(
            "cg-agent: --service requires a start by the Windows service control manager"
        ),
        "the line of item 2: {stderr}"
    );
    assert!(
        !stderr.contains("invalid config"),
        "checked before the configuration: {stderr}"
    );
    assert!(
        output.stdout.is_empty(),
        "the console path does not run: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}
