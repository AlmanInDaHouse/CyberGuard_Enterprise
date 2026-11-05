//! SPEC-024 install_ac_002, the service's half (its Amendment 2026-11-04,
//! items 1 and 2): in installed mode the configuration, the trust anchor
//! and the identity are at the fixed paths under `%ProgramData%`, and the
//! heartbeat URL is explicit. A path is its fixed path when the two
//! strings are equal but for ASCII case; any other spelling is refused, not
//! resolved. Each refusal is code 2's line, naming the file and the key.
//! The checks are pure and the service calls them before `RUNNING`; these
//! tests run on Windows, whose path syntax they exercise.
#![cfg(windows)]

use std::path::{Path, PathBuf};

use cg_agent::service::{check_config_path, check_installed_config, is_fixed_path, InstalledPaths};

fn program_data() -> PathBuf {
    PathBuf::from(r"C:\ProgramData")
}

fn paths() -> InstalledPaths {
    InstalledPaths::under(&program_data())
}

/// A secure-path configuration at the fixed paths, with or without the
/// heartbeat URL.
fn config_text(paths: &InstalledPaths, heartbeat: bool) -> String {
    let heartbeat_line = if heartbeat {
        "heartbeat_url = \"https://127.0.0.1:8443\"\n"
    } else {
        ""
    };
    format!(
        "[server]\n\
         url = \"https://127.0.0.1:8443\"\n\
         {heartbeat_line}\
         trust_anchor_path = '{anchor}'\n\
         [agent]\n\
         id = \"01934abc-def0-7000-89ab-000000000007\"\n\
         hostname = \"TEST-PC\"\n\
         [enrollment]\n\
         token = \"install-ac-002\"\n\
         cert_path = '{cert}'\n\
         key_path = '{key}'\n\
         identity_path = '{identity}'\n",
        anchor = paths.trust_anchor.display(),
        cert = paths.cert.display(),
        key = paths.key.display(),
        identity = paths.identity.display(),
    )
}

fn check(text: &str) -> Result<(), String> {
    let paths = paths();
    let config = cg_agent::config::load_from_str(text).expect("a valid configuration");
    check_installed_config(&paths, &paths.config, &config)
}

/// Item 1: the paths under `%ProgramData%`.
#[test]
fn install_ac_002_the_installed_paths_are_under_program_data() {
    let p = paths();
    let agent = program_data().join("CyberGuard").join("agent");
    assert_eq!(p.product_root, program_data().join("CyberGuard"));
    assert_eq!(p.data_root, agent);
    assert_eq!(p.config, agent.join("agent.toml"));
    assert_eq!(p.trust_anchor, agent.join("server-ca.pem"));
    assert_eq!(p.identity_dir, agent.join("identity"));
    assert_eq!(p.cert, agent.join("identity").join("cert.pem"));
    assert_eq!(p.key, agent.join("identity").join("key.dat"));
    assert_eq!(p.identity, agent.join("identity").join("identity.json"));
    assert_eq!(p.hygiene_temporary(), agent.join("agent.toml.hygiene-tmp"));
}

/// Item 2: `--config` is the fixed `agent.toml`, equal but for ASCII case;
/// any other spelling is refused with the key and the fixed path.
#[test]
fn install_ac_002_config_must_be_the_fixed_path() {
    let p = paths();
    let fixed = p.config.display().to_string();
    check_config_path(&p, &p.config).expect("the fixed path");
    check_config_path(&p, Path::new(&fixed.to_uppercase())).expect("ASCII case is ignored");

    for other in [
        fixed.replace('\\', "/"),
        format!(r"\\?\{fixed}"),
        fixed.replace(r"\agent\", r"\agent\..\agent\"),
        r"C:\Users\Public\agent.toml".to_string(),
        r"\\server\share\CyberGuard\agent\agent.toml".to_string(),
        r"C:\PROGRA~3\CyberGuard\agent\agent.toml".to_string(),
    ] {
        let line = check_config_path(&p, Path::new(&other))
            .expect_err(&format!("{other} is not the fixed path"));
        assert_eq!(
            line,
            format!("cg-agent: invalid config {other}: --config must be {fixed} in service mode"),
            "{other}"
        );
    }
}

/// Item 2: after parsing, the trust anchor and the three identity paths
/// are the fixed ones, and the heartbeat URL is explicit.
#[test]
fn install_ac_002_the_configured_paths_must_be_the_fixed_ones() {
    let p = paths();
    let valid = config_text(&p, true);
    check(&valid).expect("the fixed paths and an explicit heartbeat URL");

    for (key, fixed) in [
        ("server.trust_anchor_path", &p.trust_anchor),
        ("enrollment.cert_path", &p.cert),
        ("enrollment.key_path", &p.key),
        ("enrollment.identity_path", &p.identity),
    ] {
        let fixed_text = fixed.display().to_string();
        for other in [
            r"C:\Users\Public\elsewhere".to_string(),
            fixed_text.replace('\\', "/"),
            fixed_text.replace(r"\agent\", r"\agent\.\"),
            "relative\\file".to_string(),
        ] {
            let text = valid.replace(&format!("'{fixed_text}'"), &format!("'{other}'"));
            assert_ne!(text, valid, "the fixture replaced {key}");
            let line = check(&text).expect_err(&format!("{key} = {other}"));
            assert_eq!(
                line,
                format!(
                    "cg-agent: invalid config {}: {key} must be {fixed_text} in service mode",
                    p.config.display()
                ),
                "{key} = {other}"
            );
        }
    }

    let line = check(&config_text(&p, false)).expect_err("no heartbeat URL");
    assert_eq!(
        line,
        format!(
            "cg-agent: invalid config {}: the service requires server.heartbeat_url",
            p.config.display()
        )
    );
}

/// Item 2: the plain path is refused first, as before.
#[test]
fn install_ac_002_the_plain_path_is_refused_first() {
    let p = paths();
    let plain = "[server]\nurl = \"http://127.0.0.1:9\"\n\
                 [agent]\nid = \"01934abc-def0-7000-89ab-000000000007\"\nhostname = \"TEST-PC\"\n";
    let config = cg_agent::config::load_from_str(plain).expect("a valid plain configuration");
    let line = check_installed_config(&p, &p.config, &config).expect_err("plain");
    assert!(line.contains("requires [enrollment]"), "{line}");
}

/// The comparison itself: ASCII case alone.
#[test]
fn install_ac_002_a_fixed_path_differs_only_in_ascii_case() {
    let fixed = Path::new(r"C:\ProgramData\CyberGuard\agent\agent.toml");
    assert!(is_fixed_path(
        r"c:\programdata\cyberguard\AGENT\agent.toml",
        fixed
    ));
    assert!(!is_fixed_path(
        r"C:\ProgramData\CyberGuard\agent\agent.toml ",
        fixed
    ));
    assert!(!is_fixed_path(
        r"C:\ProgramData\CyberGuard\agent\agent.tom",
        fixed
    ));
    assert!(!is_fixed_path("", fixed));
    // Debt #146: a variant whose case folds to ASCII only outside ASCII
    // (U+0131 dotless i, U+212A Kelvin sign) is another spelling, refused.
    let key = Path::new(r"C:\ProgramData\CyberGuard\agent\identity\key.dat");
    assert!(is_fixed_path(
        r"C:\PROGRAMDATA\CYBERGUARD\AGENT\IDENTITY\KEY.DAT",
        key
    ));
    assert!(!is_fixed_path(
        "C:\\ProgramData\\CyberGuard\\agent\\\u{131}dentity\\key.dat",
        key
    ));
    assert!(!is_fixed_path(
        "C:\\ProgramData\\CyberGuard\\agent\\identity\\\u{212A}ey.dat",
        key
    ));
}
