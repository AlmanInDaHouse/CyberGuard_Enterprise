//! SPEC-001 AC-001 — Given a valid `agent.toml`, the agent starts,
//! validates config, and reaches the Heartbeating state without error.

mod common;

use cg_agent::config::load_from_path;

const VALID_CONFIG: &str = r#"
[server]
url = "http://localhost:8080"

[agent]
id = "01934abc-def0-7000-89ab-000000000001"
hostname = "FIN-PC-014"
"#;

#[test]
fn ac_001_reads_valid_config() {
    let tmp = common::write_temp_config(VALID_CONFIG);
    let cfg = load_from_path(tmp.path()).expect("valid config must load");
    assert_eq!(cfg.server.url, "http://localhost:8080");
    assert_eq!(cfg.agent.hostname, "FIN-PC-014");
    assert_eq!(cfg.heartbeat.interval_seconds, 30, "default interval");
    assert_eq!(cfg.log.level, "info", "default log level");
}
