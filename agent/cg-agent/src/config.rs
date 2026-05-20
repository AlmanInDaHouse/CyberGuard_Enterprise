//! Agent configuration. TOML schema per SPEC-001 §Configuration.

use crate::errors::ConfigError;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct AgentConfig {
    pub server: ServerConfig,
    pub agent: AgentIdentity,
    #[serde(default)]
    pub heartbeat: HeartbeatConfig,
    #[serde(default)]
    pub log: LogConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub url: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentIdentity {
    pub id: String,
    pub hostname: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HeartbeatConfig {
    #[serde(default = "default_interval_seconds")]
    pub interval_seconds: u64,
    #[serde(default = "default_request_timeout_seconds")]
    pub request_timeout_seconds: u64,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_backoff_initial_ms")]
    pub backoff_initial_ms: u64,
    #[serde(default = "default_backoff_factor")]
    pub backoff_factor: f64,
    #[serde(default = "default_backoff_max_ms")]
    pub backoff_max_ms: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LogConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval_seconds: default_interval_seconds(),
            request_timeout_seconds: default_request_timeout_seconds(),
            max_retries: default_max_retries(),
            backoff_initial_ms: default_backoff_initial_ms(),
            backoff_factor: default_backoff_factor(),
            backoff_max_ms: default_backoff_max_ms(),
        }
    }
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
        }
    }
}

fn default_interval_seconds() -> u64 {
    30
}
fn default_request_timeout_seconds() -> u64 {
    10
}
fn default_max_retries() -> u32 {
    3
}
fn default_backoff_initial_ms() -> u64 {
    1000
}
fn default_backoff_factor() -> f64 {
    2.0
}
fn default_backoff_max_ms() -> u64 {
    60000
}
fn default_log_level() -> String {
    "info".to_string()
}

/// Load and validate the agent configuration from a TOML file at `path`.
///
/// Returns `ConfigError` on:
///   - missing or unreadable file (`NotFound` / `Io`),
///   - malformed TOML (`Parse`),
///   - missing required keys (`MissingKey` — see SPEC-001 §Validation),
///   - field values that fail validation (`Invalid`).
pub fn load_from_path(_path: &Path) -> Result<AgentConfig, ConfigError> {
    todo!("implemented in the heartbeat commit")
}
