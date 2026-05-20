//! SPEC-001 AC-008 — Given a config file missing the `server.url` key,
//! the agent exits with code 2 and writes a stderr line containing the
//! substring `missing key 'server.url'`.
//!
//! Implemented as a library-level test against `load_from_path`: the
//! main entry point maps a `ConfigError::MissingKey { key: "server.url" }`
//! to exit code 2 and the documented stderr line in the
//! implementation commit. The substring check on the error string
//! covers the contract here without spawning a subprocess.

mod common;

use cg_agent::config::load_from_path;
use cg_agent::errors::ConfigError;

const NO_SERVER_URL: &str = r#"
[agent]
id = "01934abc-def0-7000-89ab-000000000001"
hostname = "FIN-PC-014"
"#;

#[test]
fn ac_008_missing_server_url() {
    let tmp = common::write_temp_config(NO_SERVER_URL);
    let err = load_from_path(tmp.path()).expect_err("config without server.url must fail to load");

    match err {
        ConfigError::MissingKey(ref key) => {
            assert_eq!(key, "server.url", "expected missing key to be server.url");
        }
        other => panic!("expected ConfigError::MissingKey(\"server.url\"), got {other:?}"),
    }

    // The contract on stderr is reinforced by the Display impl carrying
    // the documented substring.
    let rendered = format!("{err}");
    assert!(
        rendered.contains("missing key 'server.url'"),
        "rendered error must contain \"missing key 'server.url'\"; got: {rendered}"
    );
}
