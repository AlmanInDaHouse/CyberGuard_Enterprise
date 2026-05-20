//! Domain error types for `cg-agent`. See SPEC-001 §Failure modes.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config file not found: {0}")]
    NotFound(String),

    #[error("failed to read config file: {0}")]
    Io(String),

    #[error("failed to parse config TOML: {0}")]
    Parse(String),

    #[error("invalid config: missing key '{0}'")]
    MissingKey(String),

    #[error("invalid config: {0}")]
    Invalid(String),
}

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("transport error: {0}")]
    Network(String),

    #[error("server returned non-2xx: {status}")]
    BadStatus { status: u16 },

    #[error("retry budget exhausted after {attempts} attempts: {last_error}")]
    RetryExhausted { attempts: u32, last_error: String },

    #[error("request timed out after {timeout_ms} ms")]
    Timeout { timeout_ms: u64 },
}

#[derive(Debug, Error)]
pub enum AgentError {
    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error(transparent)]
    Transport(#[from] TransportError),
}
