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
    /// Enrollment block (SPEC-002 §Configuration). Absent for SPEC-001
    /// closed-test configs (`None`); present once the agent is wired for
    /// X.509-bound identity. Optional so SPEC-001 configs still parse.
    #[serde(default)]
    pub enrollment: Option<EnrollmentConfig>,
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

/// Enrollment configuration (SPEC-002 §Configuration). The `[enrollment]`
/// table in `agent.toml`. Paths are required and explicit — the installer
/// template points them at the `%ProgramData%\CyberGuard\agent\` defaults
/// documented in SPEC-002 §Default paths; tests point them at a temp dir.
#[derive(Debug, Deserialize, Clone)]
pub struct EnrollmentConfig {
    /// Single-use, server-issued enrollment token. Required on first run
    /// only (FR-002); `None` once identity is persisted, and dropped from
    /// `agent.toml` by post-enrollment hygiene (FR-014).
    #[serde(default)]
    pub token: Option<String>,
    /// Where the issued X.509 client certificate (PEM) is persisted.
    pub cert_path: String,
    /// Where the DPAPI-sealed Ed25519 private key (`key.dat`) is persisted.
    pub key_path: String,
    /// Where the `identity.json` cross-check file is persisted.
    pub identity_path: String,
    #[serde(default = "default_enrollment_timeout_seconds")]
    pub timeout_seconds: u64,
    #[serde(default = "default_enrollment_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_enrollment_backoff_initial_ms")]
    pub backoff_initial_ms: u64,
    #[serde(default = "default_enrollment_backoff_factor")]
    pub backoff_factor: f64,
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
fn default_enrollment_timeout_seconds() -> u64 {
    30
}
fn default_enrollment_max_retries() -> u32 {
    3
}
fn default_enrollment_backoff_initial_ms() -> u64 {
    1000
}
fn default_enrollment_backoff_factor() -> f64 {
    2.0
}

/// Load and validate the agent configuration from a TOML file at `path`.
///
/// Returns `ConfigError` on missing or unreadable file
/// (`NotFound` / `Io`), malformed TOML (`Parse`), missing required
/// keys (`MissingKey` — see SPEC-001 §Validation), or field values
/// that fail validation (`Invalid`).
pub fn load_from_path(path: &Path) -> Result<AgentConfig, ConfigError> {
    if !path.exists() {
        return Err(ConfigError::NotFound(path.display().to_string()));
    }
    let content = std::fs::read_to_string(path).map_err(|e| ConfigError::Io(e.to_string()))?;

    // Parse the TOML to a Value first so we can produce field-level
    // error messages that match SPEC-001 §Validation exactly.
    let raw: toml::Value =
        toml::from_str(&content).map_err(|e| ConfigError::Parse(e.to_string()))?;

    // Required: server.url. Treat a missing [server] table and a
    // missing url field within an existing [server] table as the same
    // condition — the user-facing contract is `server.url`.
    let server_url = raw
        .get("server")
        .and_then(|s| s.get("url"))
        .ok_or_else(|| ConfigError::MissingKey("server.url".to_string()))?
        .as_str()
        .ok_or_else(|| ConfigError::Invalid("server.url must be a string".to_string()))?;
    if !(server_url.starts_with("http://") || server_url.starts_with("https://")) {
        return Err(ConfigError::Invalid(
            "server.url not a valid URL".to_string(),
        ));
    }

    // Required: agent.id, agent.hostname. Same collapsing rule.
    let agent_id = raw
        .get("agent")
        .and_then(|a| a.get("id"))
        .ok_or_else(|| ConfigError::MissingKey("agent.id".to_string()))?
        .as_str()
        .ok_or_else(|| ConfigError::Invalid("agent.id must be a string".to_string()))?;
    if !is_uuidv7(agent_id) {
        return Err(ConfigError::Invalid("agent.id not a UUIDv7".to_string()));
    }
    let agent_hostname = raw
        .get("agent")
        .and_then(|a| a.get("hostname"))
        .ok_or_else(|| ConfigError::MissingKey("agent.hostname".to_string()))?
        .as_str()
        .ok_or_else(|| ConfigError::Invalid("agent.hostname must be a string".to_string()))?;
    if agent_hostname.is_empty() {
        return Err(ConfigError::Invalid(
            "agent.hostname must be non-empty".to_string(),
        ));
    }

    // Deserialize the whole document now that the required keys have
    // been validated. serde fills in defaults for the heartbeat / log
    // sections via the `#[serde(default)]` attribute.
    let config: AgentConfig =
        toml::from_str(&content).map_err(|e| ConfigError::Parse(e.to_string()))?;

    // Per SPEC-001 §Validation: numeric heartbeat fields must be > 0
    // and backoff_factor must be >= 1.0.
    if config.heartbeat.interval_seconds == 0
        || config.heartbeat.request_timeout_seconds == 0
        || config.heartbeat.backoff_initial_ms == 0
        || config.heartbeat.backoff_max_ms == 0
    {
        return Err(ConfigError::Invalid(
            "heartbeat numeric fields must be > 0".to_string(),
        ));
    }
    if config.heartbeat.backoff_factor < 1.0 {
        return Err(ConfigError::Invalid(
            "heartbeat.backoff_factor must be >= 1.0".to_string(),
        ));
    }

    Ok(config)
}

/// True if `s` matches the textual UUIDv7 pattern (lowercase hex, version
/// nibble `7`, RFC 4122 variant). Reused by SPEC-002 enrollment to
/// validate the server-assigned `agent_id` (FR-006).
pub(crate) fn is_uuidv7(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 5 {
        return false;
    }
    let expected_lengths = [8usize, 4, 4, 4, 12];
    for (p, expected) in parts.iter().zip(expected_lengths.iter()) {
        if p.len() != *expected {
            return false;
        }
        if !p
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return false;
        }
    }
    if !parts[2].starts_with('7') {
        return false;
    }
    let variant = parts[3].chars().next().expect("len already verified");
    matches!(variant, '8' | '9' | 'a' | 'b')
}
