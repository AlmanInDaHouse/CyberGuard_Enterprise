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
    /// TLS block (SPEC-003 §Configuration). Optional; documents the
    /// (constant) minimum TLS version. Absent ⇒ defaults apply.
    #[serde(default)]
    pub tls: Option<TlsConfig>,
    /// Envelope block (SPEC-003 §Configuration). Optional; documents the
    /// (constant) canonical form for signing. Absent ⇒ defaults apply.
    #[serde(default)]
    pub envelope: Option<EnvelopeConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub url: String,
    /// PEM file of trust-anchor root cert(s) for SERVER identity
    /// (SPEC-003 §Configuration). Its presence activates the secure
    /// heartbeat path (TLS 1.3 mTLS + signed envelope); absent ⇒
    /// SPEC-001 plain-HTTP legacy path.
    #[serde(default)]
    pub trust_anchor_path: Option<String>,
    /// Optional separate URL for the secure heartbeat path (SPEC-003
    /// Amendment 2026-05-22). When enroll and heartbeat live on different
    /// ports/schemes (SPEC-004's two-port topology), this is the mTLS
    /// heartbeat target; `url` is always used for enrollment. If set it
    /// must be `https://`; if absent the secure path falls back to `url`.
    #[serde(default)]
    pub heartbeat_url: Option<String>,
}

impl ServerConfig {
    /// The URL the secure heartbeat path connects to: `heartbeat_url` if
    /// set, otherwise `url` (SPEC-003 Amendment 2026-05-22). Enrollment
    /// always uses `url`, never this.
    pub fn heartbeat_target(&self) -> &str {
        self.heartbeat_url.as_deref().unwrap_or(&self.url)
    }
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
/// TLS configuration (SPEC-003 §Configuration). `minimum_version` is a
/// documented constant (`"1.3"`); the agent enforces TLS 1.3 in code
/// regardless and rejects a configured value below 1.3.
#[derive(Debug, Deserialize, Clone)]
pub struct TlsConfig {
    #[serde(default = "default_tls_minimum_version")]
    pub minimum_version: String,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            minimum_version: default_tls_minimum_version(),
        }
    }
}

/// Envelope configuration (SPEC-003 §Configuration). `canonical_form` is
/// `"JCS"` (RFC 8785); it is the only supported value in SPEC-003.
#[derive(Debug, Deserialize, Clone)]
pub struct EnvelopeConfig {
    #[serde(default = "default_canonical_form")]
    pub canonical_form: String,
}

impl Default for EnvelopeConfig {
    fn default() -> Self {
        Self {
            canonical_form: default_canonical_form(),
        }
    }
}

fn default_tls_minimum_version() -> String {
    "1.3".to_string()
}
fn default_canonical_form() -> String {
    "JCS".to_string()
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

    // SPEC-003 §Configuration: when the secure path is configured, the
    // server URL must be https, the canonical form must be JCS, and the
    // minimum TLS version must be 1.3.
    if config.server.trust_anchor_path.is_some() {
        // SPEC-003 Amendment 2026-05-22 (b): the https requirement applies to
        // the secure heartbeat *target*. When `heartbeat_url` is set it carries
        // the mTLS scheme (validated below) and `server.url` is the plain-HTTP
        // enroll endpoint (SPEC-004 FR-002 / SPEC-002 §Scope); the constraint on
        // `server.url` therefore applies only when `heartbeat_url` is absent (the
        // secure path then falls back to `server.url`).
        if config.server.heartbeat_url.is_none() && !config.server.url.starts_with("https://") {
            return Err(ConfigError::Invalid(
                "server.url must use https when server.trust_anchor_path is set and server.heartbeat_url is absent"
                    .to_string(),
            ));
        }
        if let Some(tls) = &config.tls {
            if tls.minimum_version != "1.3" {
                return Err(ConfigError::Invalid(
                    "tls.minimum_version must be \"1.3\"".to_string(),
                ));
            }
        }
        if let Some(envelope_cfg) = &config.envelope {
            if envelope_cfg.canonical_form != "JCS" {
                return Err(ConfigError::Invalid(
                    "envelope.canonical_form must be \"JCS\"".to_string(),
                ));
            }
        }
    }

    // SPEC-003 Amendment 2026-05-22: if heartbeat_url is set it must be
    // https (the secure path requires TLS). Validated whenever present.
    if let Some(hb) = &config.server.heartbeat_url {
        if !hb.starts_with("https://") {
            return Err(ConfigError::Invalid(
                "server.heartbeat_url must use https".to_string(),
            ));
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn server(url: &str, heartbeat_url: Option<&str>) -> ServerConfig {
        ServerConfig {
            url: url.to_string(),
            trust_anchor_path: None,
            heartbeat_url: heartbeat_url.map(str::to_string),
        }
    }

    #[test]
    fn heartbeat_target_uses_heartbeat_url_when_set() {
        let s = server("http://localhost:8080", Some("https://localhost:8443"));
        assert_eq!(s.heartbeat_target(), "https://localhost:8443");
    }

    #[test]
    fn heartbeat_target_falls_back_to_url_when_absent() {
        let s = server("https://localhost:8443", None);
        assert_eq!(s.heartbeat_target(), "https://localhost:8443");
    }

    /// Write `toml` to a temp file and run the full `load_from_path`
    /// validation against it.
    fn load_toml(toml: &str) -> Result<AgentConfig, ConfigError> {
        use std::io::Write as _;
        let mut f = tempfile::NamedTempFile::new().expect("tempfile");
        f.write_all(toml.as_bytes()).expect("write toml");
        f.flush().expect("flush");
        load_from_path(f.path())
    }

    const SECURE_BASE: &str = "\
[agent]
id = \"01934abc-def0-7000-89ab-0000000000bb\"
hostname = \"TEST-PC\"
";

    /// SPEC-003 Amendment 2026-05-22 (b): with `heartbeat_url` (https) set,
    /// `server.url` may be plain http — the SPEC-004 two-port topology.
    #[test]
    fn secure_path_allows_http_url_when_heartbeat_url_is_https() {
        let toml = format!(
            "[server]\n\
             url = \"http://127.0.0.1:8080\"\n\
             trust_anchor_path = \"/tmp/ca.pem\"\n\
             heartbeat_url = \"https://127.0.0.1:8443\"\n\
             {SECURE_BASE}"
        );
        let cfg = load_toml(&toml).expect("config with http url + https heartbeat_url must load");
        assert_eq!(cfg.server.url, "http://127.0.0.1:8080");
        assert_eq!(cfg.server.heartbeat_target(), "https://127.0.0.1:8443");
    }

    /// Backward compat: without `heartbeat_url`, an http `server.url` with a
    /// trust anchor set is still a config error (the secure path falls back to
    /// `server.url`, which must be https).
    #[test]
    fn secure_path_rejects_http_url_when_heartbeat_url_absent() {
        let toml = format!(
            "[server]\n\
             url = \"http://127.0.0.1:8443\"\n\
             trust_anchor_path = \"/tmp/ca.pem\"\n\
             {SECURE_BASE}"
        );
        let err =
            load_toml(&toml).expect_err("http url + trust anchor + no heartbeat_url must fail");
        assert!(matches!(err, ConfigError::Invalid(_)));
    }
}
